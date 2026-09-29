package studio.background.rebased.bridge;

import com.intellij.openapi.project.Project;
import com.intellij.openapi.project.ProjectManager;
import com.intellij.openapi.ui.AbstractPainter;
import com.intellij.openapi.ui.Painter;
import com.intellij.openapi.wm.WindowManager;
import com.intellij.openapi.wm.impl.IdeBackgroundUtil;

import javax.swing.JFrame;
import javax.swing.JPanel;
import javax.swing.SwingUtilities;
import java.awt.Component;
import java.awt.Graphics2D;
import java.awt.Image;
import java.awt.Insets;
import java.awt.image.BufferedImage;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

/** Actual WindowBackgrounds with per-root painter fixtures. Never opens a visible window. */
public final class WindowIsolationTest {
  public static void main(String[] args) throws Exception {
    SwingUtilities.invokeAndWait(WindowIsolationTest::verify);
    System.out.println("WindowIsolationTest: discovery, isolation, heartbeat, conflict, close/reopen and clear passed");
  }

  private static void verify() {
    new BridgeUnitFixtures.Harness().install();
    Project a = new BridgeUnitFixtures.FakeProject("a");
    Project b = new BridgeUnitFixtures.FakeProject("b");
    Map<Project, JFrame> frames = new IdentityHashMap<>();
    List<Project> projects = new ArrayList<>();
    ProjectManager.setInstance(new ProjectManager() {
      @Override public Project[] getOpenProjects() { return projects.toArray(Project[]::new); }
    });
    WindowManager.setInstance(new WindowManager() {
      @Override public JFrame getFrame(Project project) { return frames.get(project); }
    });
    List<JFrame> allFrames = new ArrayList<>();
    try {
      frames.put(a, frame(allFrames)); projects.add(a);
      WindowBackgrounds windows = new WindowBackgrounds();
      windows.discover();
      String first = windows.targetIds().getFirst();
      var red = spec("red", 0xffff0000);
      TextContrast contrast = new TextContrast();
      windows.apply(Map.of(first, red), contrast);
      FakeHelper firstHelper = glass(frames.get(a)).getNamedPainters(BackgroundApplier.EDITOR_PROPERTY);
      Painter firstPainter = firstHelper.painters.getFirst();
      check(color(firstPainter) == 0xffff0000, "first window should be red");
      frames.put(b, frame(allFrames)); projects.add(b);
      windows.discover();
      String second = windows.targetIds().stream().filter(id -> !id.equals(first)).findFirst().orElseThrow();
      windows.apply(Map.of(first, red), contrast);
      check(firstHelper.painters.getFirst() == firstPainter, "new window must not reset existing painter");
      check(glass(frames.get(b)).helpers.isEmpty(), "unconfigured new window must remain original");
      var blue = spec("blue", 0xff0000ff);
      windows.apply(Map.of(first, red, second, blue), contrast);
      FakeHelper secondHelper = glass(frames.get(b)).getNamedPainters(BackgroundApplier.EDITOR_PROPERTY);
      Painter secondPainter = secondHelper.painters.getFirst();
      check(color(firstPainter) != color(secondPainter), "two roots must use different images");
      windows.apply(Map.of(first, red, second, blue), contrast);
      check(firstHelper.painters.size() == 1 && secondHelper.painters.size() == 1, "heartbeat must not add duplicate painters");
      windows.apply(Map.of(first, spec("green", 0xff00ff00), second, blue), contrast);
      check(secondHelper.painters.getFirst() == secondPainter, "rotation must not change the other window");
      IdeBackgroundUtil.setBackgroundProvider((project, property) -> project == a ? "user-background" : "");
      check(!firstHelper.painters.getFirst().needsRepaint(), "user background takes priority");
      check(secondPainter.needsRepaint(), "user conflict must be scoped to affected window");
      check(!windows.apply(Map.of(first, red, second, blue), contrast).isEmpty(), "conflict must be reported");
      IdeBackgroundUtil.setBackgroundProvider((project, property) -> "");
      projects.remove(a);
      windows.discover();
      check(firstHelper.painters.isEmpty(), "closed window must release painters");
      check(windows.targetIds().equals(List.of(second)), "closed target must be removed");
      projects.add(a);
      windows.discover();
      check(!windows.targetIds().contains(first), "reopened target needs new ID");
      check(secondHelper.painters.getFirst() == secondPainter, "reopening must not touch other window");
      windows.clear();
      check(secondHelper.painters.isEmpty(), "pause/restore must remove window painters");
    }
    finally {
      IdeBackgroundUtil.setBackgroundProvider((project, property) -> "");
      allFrames.forEach(JFrame::dispose);
    }
  }

  private static JFrame frame(List<JFrame> all) {
    JFrame frame = new JFrame();
    frame.setGlassPane(new FakeGlass());
    frame.addNotify();
    all.add(frame);
    return frame;
  }
  private static FakeGlass glass(JFrame frame) { return (FakeGlass)frame.getGlassPane(); }
  private static WindowBackgrounds.Spec spec(String name, int color) {
    BufferedImage image = new BufferedImage(1, 1, BufferedImage.TYPE_INT_ARGB);
    image.setRGB(0, 0, color);
    return new WindowBackgrounds.Spec(name, image, 20, "SCALE", "CENTER", 0, true, true);
  }
  private static int color(Painter painter) {
    BufferedImage canvas = new BufferedImage(1, 1, BufferedImage.TYPE_INT_ARGB);
    Graphics2D graphics = canvas.createGraphics();
    try { painter.paint(new JPanel(), graphics); }
    finally { graphics.dispose(); }
    return canvas.getRGB(0, 0);
  }
  private static void check(boolean ok, String message) { if (!ok) { throw new AssertionError(message); } }

  public static final class FakeGlass extends JPanel {
    private static final long serialVersionUID = 1L;
    final transient Map<String, FakeHelper> helpers = new HashMap<>();
    public FakeHelper getNamedPainters(String name) { return helpers.computeIfAbsent(name, ignored -> new FakeHelper()); }
  }
  public static final class FakeHelper {
    final List<Painter> painters = new ArrayList<>();
    void addPainter(Painter painter, Component component) { painters.add(painter); }
    void removePainter(Painter painter) { painters.remove(painter); }
    static AbstractPainter newImagePainter(Image image, IdeBackgroundUtil.Fill fill, IdeBackgroundUtil.Anchor anchor, float alpha, Insets insets) {
      return new AbstractPainter() {
        @Override public boolean needsRepaint() { return true; }
        @Override public void executePaint(Component component, Graphics2D graphics) { graphics.drawImage(image, 0, 0, null); }
      };
    }
  }
}
