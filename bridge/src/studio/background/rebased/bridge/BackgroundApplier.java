package studio.background.rebased.bridge;

import com.intellij.ide.util.PropertiesComponent;
import com.intellij.openapi.application.Application;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.diagnostic.Logger;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.project.ProjectManager;
import com.intellij.openapi.wm.impl.IdeBackgroundUtil;

import javax.imageio.ImageIO;
import javax.imageio.ImageReader;
import javax.imageio.stream.ImageInputStream;
import java.io.File;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.function.Supplier;
import java.util.regex.Pattern;

/**
 * Drives the platform wallpaper painter through JVM system properties. They are the lowest
 * priority source in {@code IdeBackgroundUtil.getBackgroundSpec}, so nothing is persisted and a
 * background the user set with "Set Background Image" still wins; that case is reported instead.
 */
final class BackgroundApplier {
  static final String EDITOR_PROPERTY = "idea.background.editor";
  static final String FRAME_PROPERTY = "idea.background.frame";

  private static final Logger LOG = Logger.getInstance(BackgroundApplier.class);
  private static final Set<String> FILLS = Set.of("SCALE", "PLAIN", "TILE");
  private static final Set<String> ANCHORS = Set.of(
    "TOP_LEFT", "TOP_CENTER", "TOP_RIGHT",
    "MIDDLE_LEFT", "CENTER", "MIDDLE_RIGHT",
    "BOTTOM_LEFT", "BOTTOM_CENTER", "BOTTOM_RIGHT"
  );
  private static final Pattern MEDIA_FILE_NAME = Pattern.compile("[0-9a-f]{64}\\.png");
  private static final long EDT_TIMEOUT_SECONDS = 5;

  record Result(boolean ok, String message, String conflicts) {
    static Result success() {
      return new Result(true, "", "");
    }

    static Result failure(String message) {
      return new Result(false, message, "");
    }
  }

  private final String originalEditor = System.getProperty(EDITOR_PROPERTY);
  private final String originalFrame = System.getProperty(FRAME_PROPERTY);

  Result apply(Map<String, Object> desired) {
    Object mode = desired.get("mode");
    if ("hold".equals(mode)) {
      return Result.success();
    }
    if ("clear".equals(mode)) {
      return onEdt(() -> {
        boolean changed = set(EDITOR_PROPERTY, originalEditor) | set(FRAME_PROPERTY, originalFrame);
        if (changed) {
          IdeBackgroundUtil.resetBackgroundImagePainters();
          LOG.info("Background Studio background cleared");
        }
        return Result.success();
      });
    }
    if (!"show".equals(mode)) {
      return Result.failure("桥接插件收到未知的 mode：" + mode);
    }

    String spec;
    boolean editor;
    boolean frame;
    try {
      spec = buildSpec(desired);
      editor = requireBoolean(desired, "editor");
      frame = requireBoolean(desired, "frame");
    }
    catch (IllegalArgumentException e) {
      return Result.failure(e.getMessage());
    }
    return onEdt(() -> {
      boolean changed = set(EDITOR_PROPERTY, editor ? spec : originalEditor)
                        | set(FRAME_PROPERTY, frame ? spec : originalFrame);
      if (changed) {
        IdeBackgroundUtil.resetBackgroundImagePainters();
        LOG.info("Background Studio background applied: " + spec + " editor=" + editor + " frame=" + frame);
      }
      List<String> conflicts = findUserBackgrounds(editor, frame);
      if (conflicts.isEmpty()) {
        return Result.success();
      }
      String joined = String.join("；", conflicts);
      return new Result(
        false,
        "Rebased 里已经用 Set Background Image 设置了背景（" + joined + "），它会盖住插件背景。"
        + "请在 Rebased 中按 Ctrl+Shift+A 搜索 Set Background Image，清除后再试。",
        joined
      );
    });
  }

  private static String buildSpec(Map<String, Object> desired) {
    File image = requireMediaFile(desired.get("image"));
    Object opacity = desired.get("opacity");
    if (!(opacity instanceof Long value) || value < 0 || value > 100) {
      throw new IllegalArgumentException("opacity 必须是 0-100 的整数。");
    }
    Object fill = desired.get("fill");
    if (!(fill instanceof String) || !FILLS.contains(fill)) {
      throw new IllegalArgumentException("fill 无效：" + fill);
    }
    Object anchor = desired.get("anchor");
    if (!(anchor instanceof String) || !ANCHORS.contains(anchor)) {
      throw new IllegalArgumentException("anchor 无效：" + anchor);
    }
    return image.getPath() + "," + opacity + "," + fill + "," + anchor;
  }

  private static File requireMediaFile(Object value) {
    if (!(value instanceof String path) || path.isEmpty()) {
      throw new IllegalArgumentException("缺少背景图片路径。");
    }
    if (path.indexOf(',') >= 0) {
      throw new IllegalArgumentException("背景图片路径不能包含逗号：" + path);
    }
    String localAppData = System.getenv("LOCALAPPDATA");
    if (localAppData == null || localAppData.isEmpty()) {
      throw new IllegalArgumentException("Rebased 进程里没有 LOCALAPPDATA 环境变量。");
    }
    try {
      File root = new File(localAppData, "RebasedBackgroundStudio\\media").getCanonicalFile();
      File image = new File(path).getCanonicalFile();
      if (!root.equals(image.getParentFile()) || !MEDIA_FILE_NAME.matcher(image.getName()).matches()) {
        throw new IllegalArgumentException("背景图片不在 worker 缓存目录中：" + path);
      }
      if (!image.isFile()) {
        throw new IllegalArgumentException("背景图片不存在：" + path);
      }
      requireReadablePng(image);
      return image;
    }
    catch (IOException e) {
      throw new IllegalArgumentException("无法读取背景图片：" + e.getMessage());
    }
  }

  private static void requireReadablePng(File image) throws IOException {
    try (ImageInputStream input = ImageIO.createImageInputStream(image)) {
      if (input == null) {
        throw new IOException("ImageIO 无法打开 " + image);
      }
      Iterator<ImageReader> readers = ImageIO.getImageReaders(input);
      if (!readers.hasNext()) {
        throw new IOException("ImageIO 不认识这个图片格式：" + image);
      }
      ImageReader reader = readers.next();
      try {
        reader.setInput(input);
        if (reader.getWidth(0) <= 0 || reader.getHeight(0) <= 0) {
          throw new IOException("图片尺寸无效：" + image);
        }
      }
      finally {
        reader.dispose();
      }
    }
  }

  private static boolean requireBoolean(Map<String, Object> desired, String key) {
    Object value = desired.get(key);
    if (!(value instanceof Boolean flag)) {
      throw new IllegalArgumentException(key + " 必须是布尔值。");
    }
    return flag;
  }

  private static boolean set(String property, String value) {
    String previous = System.getProperty(property);
    if (Objects.equals(previous, value)) {
      return false;
    }
    if (value == null) {
      System.clearProperty(property);
    }
    else {
      System.setProperty(property, value);
    }
    return true;
  }

  private static List<String> findUserBackgrounds(boolean editor, boolean frame) {
    List<String> conflicts = new ArrayList<>();
    PropertiesComponent application = PropertiesComponent.getInstance();
    if (editor && isSet(application.getValue(EDITOR_PROPERTY))) {
      conflicts.add("全局·编辑器");
    }
    if (frame && isSet(application.getValue(FRAME_PROPERTY))) {
      conflicts.add("全局·空编辑区");
    }
    for (Project project : ProjectManager.getInstance().getOpenProjects()) {
      PropertiesComponent properties = PropertiesComponent.getInstance(project);
      if (editor && isSet(properties.getValue(EDITOR_PROPERTY))) {
        conflicts.add("项目 " + project.getName() + "·编辑器");
      }
      if (frame && isSet(properties.getValue(FRAME_PROPERTY))) {
        conflicts.add("项目 " + project.getName() + "·空编辑区");
      }
    }
    return conflicts;
  }

  private static boolean isSet(String value) {
    return value != null && !value.isEmpty();
  }

  private static Result onEdt(Supplier<Result> task) {
    Application application = ApplicationManager.getApplication();
    if (application == null) {
      return Result.failure("Rebased 应用尚未初始化。");
    }
    CompletableFuture<Result> future = new CompletableFuture<>();
    application.invokeLater(() -> {
      try {
        future.complete(task.get());
      }
      catch (Throwable t) {
        LOG.warn("Background Studio bridge failed on EDT", t);
        future.complete(Result.failure("桥接插件应用背景失败：" + t));
      }
    }, ModalityState.any());
    try {
      return future.get(EDT_TIMEOUT_SECONDS, TimeUnit.SECONDS);
    }
    catch (TimeoutException e) {
      return Result.failure("Rebased 界面线程 " + EDT_TIMEOUT_SECONDS + " 秒内没有响应。");
    }
    catch (InterruptedException e) {
      Thread.currentThread().interrupt();
      return Result.failure("桥接插件线程被中断。");
    }
    catch (ExecutionException e) {
      return Result.failure("桥接插件应用背景失败：" + e.getCause());
    }
  }
}
