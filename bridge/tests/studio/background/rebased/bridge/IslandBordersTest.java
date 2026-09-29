package studio.background.rebased.bridge;

import com.intellij.toolWindow.xNext.island.XNextIslandHolder;
import com.intellij.openapi.application.impl.islands.IslandsUICustomization;
import java.awt.*;
import java.awt.image.BufferedImage;
import javax.swing.*;

public final class IslandBordersTest {
  public static void main(String[] args) throws Exception {
    SwingUtilities.invokeAndWait(IslandBordersTest::verify);
    System.out.println("IslandBordersTest: no border masks, retained native lines, isolation and restoration passed");
  }
  private static void verify() {
    JPanel root = new JPanel();
    XNextIslandHolder first = new XNextIslandHolder(), other = new XNextIslandHolder();
    root.add(first);
    var original = first.getBorderPainter$intellij_platform_ide_impl();
    var otherOriginal = other.getBorderPainter$intellij_platform_ide_impl();
    IslandBorders borders = new IslandBorders(); borders.refresh(root);
    var replacement = first.getBorderPainter$intellij_platform_ide_impl();
    check(replacement != original, "must replace only native Islands border painter");
    check(other.getBorderPainter$intellij_platform_ide_impl() == otherOriginal, "other windows untouched");
    borders.refresh(root);
    check(first.getBorderPainter$intellij_platform_ide_impl() == replacement, "heartbeat must not stack wrappers");
    BufferedImage canvas = new BufferedImage(20, 20, BufferedImage.TYPE_INT_ARGB);
    Graphics2D graphics = canvas.createGraphics();
    try {
      graphics.setColor(Color.BLUE); graphics.fillRect(0, 0, 20, 20);
      replacement.paintBeforeChildren(first, graphics);
      replacement.paintAfterChildren(first, graphics);
      check(canvas.getRGB(0, 0) == Color.BLUE.getRGB(), "border surround must not darken background");
      check(canvas.getRGB(8, 8) == Color.BLUE.getRGB(), "native wallpaper replay must not overlay image again");
      check(canvas.getRGB(3, 2) == Color.WHITE.getRGB(), "original thin dividing line retained");
      check(canvas.getRGB(10, 10) == Color.GREEN.getRGB(), "before-children empty editor fill preserved");
      check(replacement.isPaintingOrigin(first), "original painting origin preserved");
    } finally { graphics.dispose(); }
    root.remove(first); borders.refresh(root);
    check(first.getBorderPainter$intellij_platform_ide_impl() == original, "detached component restored");
    root.add(first); borders.refresh(root); borders.clear();
    check(first.getBorderPainter$intellij_platform_ide_impl() == original, "pause restores original painter");
    borders.refresh(root);
    var themeReplacement = new IslandsUICustomization.createToolWindowBorderPainter$1();
    first.setBorderPainter$intellij_platform_ide_impl(themeReplacement);
    borders.refresh(root); borders.clear();
    check(first.getBorderPainter$intellij_platform_ide_impl() == themeReplacement, "theme update becomes new restore point");
    borders.refresh(root);
    first.setBorderPainter$intellij_platform_ide_impl(original); borders.clear();
    check(first.getBorderPainter$intellij_platform_ide_impl() == original, "clear must not overwrite later external change");
    // Real holders are opaque and keep 2-3px EmptyBorder insets even without the after-fill.
    first.setSize(20,20); first.setBorder(BorderFactory.createEmptyBorder(3,3,3,3));
    Object oldBase = UIManager.get("MainWindow.background");
    UIManager.put("MainWindow.background", Color.LIGHT_GRAY);
    Graphics2D ring = canvas.createGraphics();
    try {
      ring.setColor(Color.BLACK); ring.fillRect(0,0,20,20);
      ring.setColor(Color.BLUE); ring.fillRect(3,3,14,14);
      IslandSurfaceInsets.paint(first, ring);
      check(canvas.getRGB(1,10) == Color.LIGHT_GRAY.getRGB(), "opaque holder inset must not stay black");
      check(canvas.getRGB(10,10) == Color.BLUE.getRGB(), "inset repaint must not cover child content");
    } finally { ring.dispose(); UIManager.put("MainWindow.background", oldBase); }
  }
  private static void check(boolean value, String message) { if (!value) { throw new AssertionError(message); } }
}
