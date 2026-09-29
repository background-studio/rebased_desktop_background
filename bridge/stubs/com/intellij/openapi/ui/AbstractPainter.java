package com.intellij.openapi.ui;

import java.awt.Component;
import java.awt.Graphics2D;

public abstract class AbstractPainter implements Painter {
  public abstract void executePaint(Component component, Graphics2D graphics);
  public void paint(Component component, Graphics2D graphics) { executePaint(component, graphics); }
}
