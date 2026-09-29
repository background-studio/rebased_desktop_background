package studio.background.rebased.bridge;

import java.awt.*;
import java.awt.geom.AffineTransform;
import java.awt.image.*;
import java.awt.image.renderable.RenderableImage;

/** Only used while the Islands border painter runs AFTER children, never for panel content.
 * Keep the theme's original outline geometry/colour, omit its filled surround and wallpaper
 * replay. The underlying window background already has the configured image opacity.
 */
final class IslandBorderGraphics extends WindowPainterGraphics {
  IslandBorderGraphics(Graphics2D graphics) { super(graphics); }
  @Override public Graphics create() { return new IslandBorderGraphics((Graphics2D)delegate.create()); }
  @Override public void fill(Shape shape) { }
  @Override public void fillRect(int x,int y,int w,int h) { }
  @Override public void clearRect(int x,int y,int w,int h) { }
  @Override public void fillRoundRect(int x,int y,int w,int h,int a,int b) { }
  @Override public void fillOval(int x,int y,int w,int h) { }
  @Override public void fillArc(int x,int y,int w,int h,int a,int b) { }
  @Override public void fillPolygon(int[] x,int[] y,int n) { }
  @Override public void copyArea(int x,int y,int w,int h,int dx,int dy) { }
  @Override public boolean drawImage(Image i,AffineTransform t,ImageObserver o) { return true; }
  @Override public void drawImage(BufferedImage i,BufferedImageOp o,int x,int y) { }
  @Override public void drawRenderedImage(RenderedImage i,AffineTransform t) { }
  @Override public void drawRenderableImage(RenderableImage i,AffineTransform t) { }
  @Override public boolean drawImage(Image i,int x,int y,ImageObserver o) { return true; }
  @Override public boolean drawImage(Image i,int x,int y,int w,int h,ImageObserver o) { return true; }
  @Override public boolean drawImage(Image i,int x,int y,Color c,ImageObserver o) { return true; }
  @Override public boolean drawImage(Image i,int x,int y,int w,int h,Color c,ImageObserver o) { return true; }
  @Override public boolean drawImage(Image i,int x,int y,int xx,int yy,int sx,int sy,int sxx,int syy,ImageObserver o) { return true; }
  @Override public boolean drawImage(Image i,int x,int y,int xx,int yy,int sx,int sy,int sxx,int syy,Color c,ImageObserver o) { return true; }
}
