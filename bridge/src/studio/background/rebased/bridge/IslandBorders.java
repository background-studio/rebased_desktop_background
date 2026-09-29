package studio.background.rebased.bridge;

import com.intellij.openapi.wm.impl.IdeBackgroundUtil;
import com.intellij.ui.BorderPainter;
import javax.swing.*;
import java.awt.*;
import java.lang.reflect.Method;
import java.util.Map;
import java.util.WeakHashMap;

/** Window-scoped, reversible removal of Islands' extra border-area fill. EDT only. */
final class IslandBorders {
  private static final String ISLANDS = "com.intellij.openapi.application.impl.islands.IslandsUICustomization$";
  private final Map<JComponent, Attachment> changed = new WeakHashMap<>();

  void refresh(JComponent root) {
    changed.entrySet().removeIf(entry -> {
      if (entry.getKey() == root || SwingUtilities.isDescendingFrom(entry.getKey(), root)) { return false; }
      entry.getValue().restore(entry.getKey());
      return true;
    });
    visit(root);
  }

  private void visit(Component component) {
    if (component instanceof JComponent jc && isIsland(component.getClass())) { enable(jc); }
    if (component instanceof Container container) {
      for (Component child : container.getComponents()) { visit(child); }
    }
  }

  private static boolean isIsland(Class<?> type) {
    for (; type != null; type = type.getSuperclass()) {
      if (type.getName().equals("com.intellij.toolWindow.xNext.island.XNextIslandHolder")
          || type.getName().equals("com.intellij.openapi.fileEditor.impl.EditorsSplitters")) { return true; }
    }
    return false;
  }

  private void enable(JComponent component) {
    try {
      Method get = component.getClass().getMethod("getBorderPainter$intellij_platform_ide_impl");
      Method set = component.getClass().getMethod("setBorderPainter$intellij_platform_ide_impl", BorderPainter.class);
      BorderPainter original = (BorderPainter)get.invoke(component);
      String name = original.getClass().getName();
      if (!name.equals(ISLANDS + "createToolWindowBorderPainter$1")
          && !name.equals(ISLANDS + "createEditorBorderPainter$1")) { return; }
      LinesOnly replacement = new LinesOnly(original);
      set.invoke(component, replacement);
      changed.put(component, new Attachment(get, set, original, replacement));
      component.repaint();
    } catch (ReflectiveOperationException error) {
      throw new IllegalStateException("Rebased 区块边框绘制接口不兼容", error);
    }
  }

  void clear() {
    changed.forEach((component, attachment) -> attachment.restore(component));
    changed.clear();
  }

  private record LinesOnly(BorderPainter original) implements BorderPainter {
    @Override public void paintBeforeChildren(JComponent component, Graphics graphics) {
      original.paintBeforeChildren(component, graphics);
    }
    @Override public boolean isPaintingOrigin(JComponent component) { return original.isPaintingOrigin(component); }
    @Override public void paintAfterChildren(JComponent component, Graphics graphics) {
      Graphics2D raw = (Graphics2D)graphics;
      while (true) {
        if (raw instanceof WindowPainterGraphics shield) { raw = shield.delegate; continue; }
        Graphics2D unwrapped = IdeBackgroundUtil.getOriginalGraphics(raw);
        if (unwrapped == raw) { break; }
        raw = unwrapped;
      }
      Graphics2D lines = new IslandBorderGraphics((Graphics2D)raw.create());
      try {
        // Omitting the theme's after-children fill alone leaves the holder's opaque insets.
        // Repaint just that ring once using the same base + native image as the content.
        IslandSurfaceInsets.paint(component, raw);
        lines.setStroke(new BasicStroke(1f));
        original.paintAfterChildren(component, lines);
      }
      finally { lines.dispose(); }
    }
  }


  private record Attachment(Method get, Method set, BorderPainter original, BorderPainter replacement) {
    void restore(JComponent component) {
      try {
        // Do not overwrite a later theme/plugin change, including a freshly-created border.
        if (get.invoke(component) == replacement) { set.invoke(component, original); component.repaint(); }
      } catch (ReflectiveOperationException error) {
        throw new IllegalStateException("还原 Rebased 区块边框失败", error);
      }
    }
  }
}
