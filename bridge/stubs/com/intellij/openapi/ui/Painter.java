package com.intellij.openapi.ui;

import java.awt.Component;
import java.awt.Graphics2D;

public interface Painter {
  boolean needsRepaint();
  void paint(Component component, Graphics2D graphics);
}
