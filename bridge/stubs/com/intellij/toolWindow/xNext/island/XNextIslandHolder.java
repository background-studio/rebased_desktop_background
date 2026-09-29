package com.intellij.toolWindow.xNext.island;

import javax.swing.JPanel;
import com.intellij.ui.BorderPainter;
import com.intellij.openapi.application.impl.islands.IslandsUICustomization;

@SuppressWarnings("serial")
public class XNextIslandHolder extends JPanel {
  private BorderPainter painter = new IslandsUICustomization.createToolWindowBorderPainter$1();
  public BorderPainter getBorderPainter$intellij_platform_ide_impl() { return painter; }
  public void setBorderPainter$intellij_platform_ide_impl(BorderPainter value) { painter = value; }
}
