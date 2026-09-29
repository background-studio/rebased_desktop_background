package studio.background.rebased.bridge;

import com.intellij.openapi.project.Project;
import com.intellij.openapi.project.ProjectManager;
import com.intellij.openapi.ui.AbstractPainter;
import com.intellij.openapi.ui.Painter;
import com.intellij.openapi.wm.WindowManager;
import com.intellij.openapi.wm.impl.IdeBackgroundUtil;

import javax.imageio.ImageIO;
import javax.swing.JFrame;
import javax.swing.JRootPane;
import java.awt.Component;
import java.awt.Graphics2D;
import java.awt.Insets;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/** Window-local native painters; never writes project/application PropertiesComponent. */
final class WindowBackgrounds {
  private final Map<Project, Target> targets = new IdentityHashMap<>();
  private volatile List<String> targetIds = List.of();
  // Preparation runs on the pipe thread. Only immutable prepared snapshots cross to the EDT.
  private Map<String, Object> previousInput = Map.of();
  private Map<String, Spec> previousPrepared = Map.of();

  record Spec(String imagePath, BufferedImage image, int opacity, String fill, String anchor,
              int contrast, boolean editor, boolean frame) {}

  private static final class Target {
    final String id = ProcessHandle.current().pid() + "-" + UUID.randomUUID().toString().replace("-", "");
    final JRootPane root;
    final IslandBackgroundGate islands = new IslandBackgroundGate();
    Attachment editor;
    Attachment frame;
    Spec spec;

    Target(JRootPane root) { this.root = root; }

    void clear() {
      islands.clear();
      if (editor == null && frame == null && spec == null) { return; }
      if (editor != null) { editor.remove(); editor = null; }
      if (frame != null) { frame.remove(); frame = null; }
      spec = null;
      root.repaint();
    }
  }

  /** Called on the EDT, including hold/clear, so new windows are discovered without changing them. */
  void discover() {
    Map<Project, JRootPane> open = new IdentityHashMap<>();
    for (Project project : ProjectManager.getInstance().getOpenProjects()) {
      if (project.isDisposed()) { continue; }
      JFrame frame = WindowManager.getInstance().getFrame(project);
      if (frame != null && frame.isDisplayable()) { open.put(project, frame.getRootPane()); }
    }
    targets.entrySet().removeIf(entry -> {
      if (open.get(entry.getKey()) == entry.getValue().root) { return false; }
      entry.getValue().clear();
      return true;
    });
    open.forEach((project, root) -> targets.computeIfAbsent(project, ignored -> new Target(root)));
    targetIds = targets.values().stream().map(target -> target.id).sorted().toList();
    if (targetIds.size() > 128) { throw new IllegalStateException("Rebased 窗口数超过 128 上限。"); }
  }

  List<String> targetIds() { return targetIds; }

  void forgetPrepared() {
    previousInput = Map.of();
    previousPrepared = Map.of();
  }

  Map<String, Spec> prepare(Object value) {
    if (!(value instanceof Map<?, ?> raw) || raw.size() > 128) {
      throw new IllegalArgumentException("windows 必须是最多 128 项的窗口配置对象。");
    }
    Map<String, Object> input = new LinkedHashMap<>();
    for (var entry : raw.entrySet()) {
      if (!(entry.getKey() instanceof String id) || !id.matches("[A-Za-z0-9_-]{1,64}")) {
        throw new IllegalArgumentException("窗口 ID 无效。");
      }
      // A worker can serve several IDE processes. Do not decode or retain other processes' images.
      if (id.startsWith(ProcessHandle.current().pid() + "-")) { input.put(id, entry.getValue()); }
    }
    if (input.equals(previousInput)) { return previousPrepared; }
    Map<String, Spec> prepared = new LinkedHashMap<>();
    Map<String, BufferedImage> images = new HashMap<>();
    previousPrepared.values().forEach(spec -> images.put(spec.imagePath(), spec.image()));
    for (var entry : input.entrySet()) {
      if (!(entry.getValue() instanceof Map<?, ?> rawSpec)) {
        throw new IllegalArgumentException("窗口配置必须是对象。");
      }
      Map<String, Object> fields = new LinkedHashMap<>();
      for (var field : rawSpec.entrySet()) {
        if (!(field.getKey() instanceof String key)) { throw new IllegalArgumentException("配置键无效。"); }
        fields.put(key, field.getValue());
      }
      String spec = BackgroundApplier.buildSpec(fields);
      String path = spec.substring(0, spec.indexOf(','));
      BufferedImage image = images.get(path);
      if (image == null) {
        try { image = ImageIO.read(new File(path)); }
        catch (IOException error) { throw new IllegalArgumentException("无法解码窗口背景：" + error.getMessage()); }
        if (image == null) { throw new IllegalArgumentException("窗口背景不是可解码图片。"); }
        images.put(path, image);
      }
      prepared.put(entry.getKey(), new Spec(path, image,
        BackgroundApplier.requirePercent(fields, "opacity"), (String)fields.get("fill"),
        (String)fields.get("anchor"), BackgroundApplier.requirePercent(fields, "textContrast"),
        BackgroundApplier.requireBoolean(fields, "editor"), BackgroundApplier.requireBoolean(fields, "frame")));
    }
    previousInput = input;
    previousPrepared = Map.copyOf(prepared);
    return previousPrepared;
  }

  List<String> apply(Map<String, Spec> specs, TextContrast contrast) {
    List<String> conflicts = new ArrayList<>();
    Map<Project, Integer> factors = new IdentityHashMap<>();
    targets.forEach((project, target) -> {
      Spec spec = specs.get(target.id);
      if (spec == null) { if (target.spec != null) { target.clear(); } return; }
      if (!spec.equals(target.spec)) {
        // Prepare both attachments before replacing either, so incompatible APIs fail without
        // silently falling back to process-global backgrounds or destroying the previous image.
        Attachment editor = spec.editor() ? Attachment.create(target.root, project, BackgroundApplier.EDITOR_PROPERTY, spec) : null;
        Attachment frame = spec.frame() ? Attachment.create(target.root, project, BackgroundApplier.FRAME_PROPERTY, spec) : null;
        target.clear();
        target.editor = editor;
        target.frame = frame;
        if (editor != null) { editor.install(); }
        if (frame != null) { frame.install(); }
        target.spec = spec;
        target.root.repaint();
      }
      boolean editorConflict = spec.editor() && isUserBackground(project, BackgroundApplier.EDITOR_PROPERTY);
      if (spec.editor() && !editorConflict) { target.islands.refresh(target.root); }
      else { target.islands.clear(); }
      if (editorConflict) { conflicts.add("项目 " + project.getName() + "·编辑器"); }
      if (spec.frame() && isUserBackground(project, BackgroundApplier.FRAME_PROPERTY)) {
        conflicts.add("项目 " + project.getName() + "·空编辑区");
      }
      if (spec.editor() && !editorConflict) { factors.put(project, spec.contrast()); }
    });
    contrast.apply(factors);
    return conflicts;
  }

  void clear() { targets.values().forEach(Target::clear); }

  private static boolean isUserBackground(Project project, String property) {
    return !IdeBackgroundUtil.getBackgroundSpec(project, property).isEmpty();
  }

  /** Platform helper is package-private; only add/remove our own painter, never clear its set. */
  private record Attachment(Object helper, Method addMethod, Method removeMethod, Painter painter) {
    static Attachment create(JRootPane root, Project project, String property, Spec spec) {
      try {
        Object glass = root.getGlassPane();
        Method named = glass.getClass().getDeclaredMethod("getNamedPainters", String.class);
        named.setAccessible(true);
        Object helper = named.invoke(glass, property);
        Method add = helper.getClass().getDeclaredMethod("addPainter", Painter.class, Component.class);
        Method remove = helper.getClass().getDeclaredMethod("removePainter", Painter.class);
        add.setAccessible(true);
        remove.setAccessible(true);
        Method factory = helper.getClass().getDeclaredMethod("newImagePainter", java.awt.Image.class,
          IdeBackgroundUtil.Fill.class, IdeBackgroundUtil.Anchor.class, float.class, Insets.class);
        factory.setAccessible(true);
        AbstractPainter nativePainter = (AbstractPainter)factory.invoke(null, spec.image(),
          IdeBackgroundUtil.Fill.valueOf(spec.fill()), IdeBackgroundUtil.Anchor.valueOf(spec.anchor()),
          spec.opacity() / 100f, new Insets(0, 0, 0, 0));
        Painter guarded = new AbstractPainter() {
          @Override public boolean needsRepaint() { return !project.isDisposed() && !isUserBackground(project, property); }
          @Override public void executePaint(Component component, Graphics2D graphics) { nativePainter.paint(component, graphics); }
        };
        return new Attachment(helper, add, remove, guarded);
      }
      catch (ReflectiveOperationException error) {
        throw new IllegalStateException("当前 Rebased 不支持窗口级原生背景绘制，请更新桥接插件：" + error, error);
      }
    }

    void install() { invoke(addMethod, painter, null); }
    void remove() { invoke(removeMethod, painter); }
    private void invoke(Method method, Object... args) {
      try { method.invoke(helper, args); }
      catch (ReflectiveOperationException error) { throw new IllegalStateException("更新窗口背景绘制器失败。", error); }
    }
  }
}
