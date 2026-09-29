package studio.background.rebased.bridge;

import java.awt.Component;
import java.awt.Insets;
import java.lang.reflect.Method;

/** Run with the real Rebased lib/*.jar, not stubs. No application boot or user settings writes. */
public final class NativePainterApiTest {
  public static void main(String[] args) throws Exception {
    ClassLoader loader = NativePainterApiTest.class.getClassLoader();
    Class<?> glass = Class.forName("com.intellij.openapi.wm.impl.IdeGlassPaneImpl", false, loader);
    Class<?> painter = Class.forName("com.intellij.openapi.ui.Painter", false, loader);
    Method named = glass.getDeclaredMethod("getNamedPainters", String.class);
    named.setAccessible(true);
    Class<?> helper = named.getReturnType();
    helper.getDeclaredMethod("addPainter", painter, Component.class).setAccessible(true);
    helper.getDeclaredMethod("removePainter", painter).setAccessible(true);
    Class<?> backgrounds = Class.forName("com.intellij.openapi.wm.impl.IdeBackgroundUtil", false, loader);
    Class<?> fill = Class.forName(backgrounds.getName() + "$Fill", false, loader);
    Class<?> anchor = Class.forName(backgrounds.getName() + "$Anchor", false, loader);
    helper.getDeclaredMethod("newImagePainter", java.awt.Image.class, fill, anchor, float.class, Insets.class).setAccessible(true);
    Class<?> project = Class.forName("com.intellij.openapi.project.Project", false, loader);
    backgrounds.getMethod("getBackgroundSpec", project, String.class);
    backgrounds.getField("NO_BACKGROUND");
    backgrounds.getField("NO_BACKGROUND_PREDICATE");
    Class<?> borders = Class.forName("com.intellij.ui.BorderPainter", false, loader);
    borders.getMethod("paintAfterChildren", javax.swing.JComponent.class, java.awt.Graphics.class);
    for (String name : new String[]{"com.intellij.toolWindow.xNext.island.XNextIslandHolder", "com.intellij.openapi.fileEditor.impl.EditorsSplitters"}) {
      Class<?> island = Class.forName(name, false, loader);
      island.getMethod("getBorderPainter$intellij_platform_ide_impl");
      island.getMethod("setBorderPainter$intellij_platform_ide_impl", borders);
    }
    for (String name : new String[]{"createToolWindowBorderPainter$1", "createEditorBorderPainter$1"}) {
      Class.forName("com.intellij.openapi.application.impl.islands.IslandsUICustomization$" + name, false, loader);
    }
    Class.forName(backgrounds.getName() + "$MyTransform", false, loader).getDeclaredConstructor().setAccessible(true);
    Class.forName("com.intellij.util.ui.JBSwingUtilities", false, loader).getMethod("addGlobalCGTransform", java.util.function.BiFunction.class);
    Class.forName("com.intellij.openapi.wm.WindowManager", false, loader).getMethod("getFrame", project);
    // Islands' renderer calls this API to strip native wallpaper graphics. Our shield must
    // retain the native delegate, including graphics created by CellRendererPane.
    var canvas = new java.awt.image.BufferedImage(20, 20, java.awt.image.BufferedImage.TYPE_INT_ARGB);
    var protectedGraphics = new WindowPainterGraphics(canvas.createGraphics());
    try {
      Method original = backgrounds.getMethod("getOriginalGraphics", java.awt.Graphics.class);
      if (original.invoke(null, protectedGraphics) != protectedGraphics) {
        throw new AssertionError("Islands can strip the window painter shield");
      }
      var child = (java.awt.Graphics2D)protectedGraphics.create();
      try {
        if (original.invoke(null, child) != child) { throw new AssertionError("renderer child lost protection"); }
        child.translate(3, 4);
        child.setColor(java.awt.Color.RED);
        child.fillRect(0, 0, 2, 2);
      } finally { child.dispose(); }
      if (canvas.getRGB(3, 4) != java.awt.Color.RED.getRGB() || canvas.getRGB(0, 0) != 0) {
        throw new AssertionError("shield must preserve native coordinates and drawing");
      }
    } finally { protectedGraphics.dispose(); }
    System.out.println("NativePainterApiTest: real Rebased API and Islands renderer graphics protection verified");
  }
}
