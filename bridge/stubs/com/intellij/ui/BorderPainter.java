package com.intellij.ui;

import java.awt.Graphics;
import javax.swing.JComponent;

public interface BorderPainter {
  default void paintBeforeChildren(JComponent component, Graphics graphics) { }
  void paintAfterChildren(JComponent component, Graphics graphics);
  boolean isPaintingOrigin(JComponent component);
}
