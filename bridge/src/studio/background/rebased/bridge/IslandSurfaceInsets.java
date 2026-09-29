package studio.background.rebased.bridge;

import com.intellij.openapi.wm.impl.IdeBackgroundUtil;
import javax.swing.JComponent;
import java.awt.*;
import java.awt.geom.Area;

/** Replace only the opaque inset ring, not children or the native thin outline. */
final class IslandSurfaceInsets {
  static void paint(JComponent component, Graphics2D raw) {
    Insets insets = component.getInsets();
    int w = component.getWidth(), h = component.getHeight();
    Color base = WindowSurfaceGraphics.baseColor();
    if (base == null || w <= 0 || h <= 0) { return; }
    Area ring = new Area(new Rectangle(0, 0, w, h));
    ring.subtract(new Area(new Rectangle(insets.left, insets.top,
      Math.max(0, w - insets.left - insets.right), Math.max(0, h - insets.top - insets.bottom))));
    Graphics2D copy = (Graphics2D)raw.create();
    try {
      copy.clip(ring);
      Graphics2D background = IdeBackgroundUtil.withEditorBackground(copy, component);
      background.setColor(base);
      background.fillRect(0, 0, w, h);
    } finally { copy.dispose(); }
  }
}
