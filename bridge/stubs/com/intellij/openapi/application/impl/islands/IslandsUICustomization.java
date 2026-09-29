package com.intellij.openapi.application.impl.islands;

import javax.swing.JComponent;
import java.util.function.Predicate;

/** Test fixture matching the real platform Islands predicate's class ownership. */
public final class IslandsUICustomization {
  public static Predicate<JComponent> backgroundGate() { return component -> true; }
  public static final class createToolWindowBorderPainter$1 implements com.intellij.ui.BorderPainter {
    @Override public boolean isPaintingOrigin(JComponent component) { return true; }
    @Override public void paintBeforeChildren(JComponent component, java.awt.Graphics graphics) {
      graphics.setColor(java.awt.Color.GREEN); graphics.fillRect(10, 10, 1, 1);
    }
    @Override public void paintAfterChildren(JComponent component, java.awt.Graphics graphics) {
      java.awt.Graphics2D g = (java.awt.Graphics2D)graphics.create();
      try {
        g.setColor(java.awt.Color.BLACK); g.fillRect(0, 0, 20, 4);
        g.fill(new java.awt.geom.Rectangle2D.Double(0, 0, 4, 20));
        var mask = new java.awt.image.BufferedImage(20, 20, 2);
        mask.setRGB(8, 8, java.awt.Color.BLACK.getRGB());
        g.drawImage(mask, 0, 0, null);
        g.setColor(java.awt.Color.WHITE); g.drawLine(2, 2, 17, 2);
      } finally { g.dispose(); }
    }
  }
}
