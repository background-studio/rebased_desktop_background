package com.intellij.openapi.wm.impl;

public final class IdeBackgroundUtil {
  public static final Object NO_BACKGROUND = new Object();
  public static final Object NO_BACKGROUND_PREDICATE = new Object();
  public static java.awt.Graphics2D getOriginalGraphics(java.awt.Graphics graphics) { return (java.awt.Graphics2D)graphics; }
  public static java.awt.Graphics2D withEditorBackground(java.awt.Graphics graphics, javax.swing.JComponent component) { return (java.awt.Graphics2D)graphics; }
  private static final class MyTransform implements java.util.function.BiFunction<javax.swing.JComponent, java.awt.Graphics2D, java.awt.Graphics2D> {
    @Override public java.awt.Graphics2D apply(javax.swing.JComponent component, java.awt.Graphics2D graphics) {
      component.putClientProperty("test.nativeTransformReplayed", true);
      return graphics;
    }
  }
  public enum Fill { PLAIN, SCALE, TILE }
  public enum Anchor { TOP_LEFT, TOP_CENTER, TOP_RIGHT, MIDDLE_LEFT, CENTER, MIDDLE_RIGHT, BOTTOM_LEFT, BOTTOM_CENTER, BOTTOM_RIGHT }
  public static String getBackgroundSpec(com.intellij.openapi.project.Project project, String property) {
    return backgrounds.apply(project, property);
  }
  private static java.util.function.BiFunction<com.intellij.openapi.project.Project, String, String> backgrounds = (project, property) -> "";
  public static void setBackgroundProvider(java.util.function.BiFunction<com.intellij.openapi.project.Project, String, String> provider) { backgrounds = provider; }
  public static void resetBackgroundImagePainters() {
    throw new UnsupportedOperationException("compile-only stub");
  }
}
