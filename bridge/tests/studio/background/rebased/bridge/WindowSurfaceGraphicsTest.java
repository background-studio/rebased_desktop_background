package studio.background.rebased.bridge;

import java.awt.*;
import java.awt.image.BufferedImage;
import javax.swing.UIManager;

public final class WindowSurfaceGraphicsTest {
  public static void main(String[] args) {
    Color dark = new Color(25,26,28), base = new Color(38,40,44);
    Object oldPanel = UIManager.get("Panel.background"), oldMain = UIManager.get("MainWindow.background");
    UIManager.put("Panel.background", dark); UIManager.put("MainWindow.background", base);
    try {
      BufferedImage image = new BufferedImage(30,30,BufferedImage.TYPE_INT_ARGB);
      Graphics2D raw = image.createGraphics();
      WindowSurfaceGraphics graphics = new WindowSurfaceGraphics(raw);
      try {
        graphics.setColor(dark); graphics.fillRect(0,0,10,10);
        check(image.getRGB(2,2) == base.getRGB(), "ordinary panel fill must match outside base");
        check(graphics.getColor().equals(dark), "normalization must not change subsequent text colour");
        graphics.drawLine(0,12,10,12);
        check(image.getRGB(2,12) == dark.getRGB(), "line colour must remain unchanged");
        graphics.fillRect(12,0,1,10);
        check(image.getRGB(12,2) == dark.getRGB(), "one-pixel separator must remain unchanged");
        graphics.setColor(Color.BLUE); graphics.fillRect(15,0,10,10);
        check(image.getRGB(17,2) == Color.BLUE.getRGB(), "selected row must not become neutral");
        Graphics2D child = (Graphics2D)graphics.create();
        try { child.setColor(dark); child.fillRect(0,15,10,10); } finally { child.dispose(); }
        check(image.getRGB(2,17) == base.getRGB(), "child renderer graphics retain normalization");
        graphics.setComposite(AlphaComposite.SrcOver.derive(.5f));
        graphics.setColor(dark); graphics.fillRect(15,15,10,10);
        check((image.getRGB(17,17) & 0xffffff) != (base.getRGB() & 0xffffff), "translucent overlay must not be recoloured");
      } finally { graphics.dispose(); }
    } finally { UIManager.put("Panel.background", oldPanel); UIManager.put("MainWindow.background", oldMain); }
    System.out.println("WindowSurfaceGraphicsTest: uniform base, original outlines, selections and alpha preserved");
  }
  private static void check(boolean value, String message) { if (!value) { throw new AssertionError(message); } }
}
