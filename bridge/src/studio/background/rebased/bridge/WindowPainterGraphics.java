package studio.background.rebased.bridge;

import java.awt.*;
import java.awt.font.FontRenderContext;
import java.awt.font.GlyphVector;
import java.awt.geom.AffineTransform;
import java.awt.image.*;
import java.awt.image.renderable.RenderableImage;
import java.text.AttributedCharacterIterator;
import java.util.Map;

/** Keeps Islands' preserveGraphics from stripping the window painter from table renderers. */
class WindowPainterGraphics extends Graphics2D {
  final Graphics2D delegate;
  WindowPainterGraphics(Graphics2D delegate) { this.delegate = delegate; }
  @Override public Graphics create() { return new WindowPainterGraphics((Graphics2D)delegate.create()); }
  @Override public void dispose() { delegate.dispose(); }
  @Override public void draw(Shape s) { delegate.draw(s); }
  @Override public void fill(Shape s) { delegate.fill(s); }
  @Override public boolean drawImage(Image i, AffineTransform t, ImageObserver o) { return delegate.drawImage(i,t,o); }
  @Override public void drawImage(BufferedImage i, BufferedImageOp o,int x,int y) { delegate.drawImage(i,o,x,y); }
  @Override public void drawRenderedImage(RenderedImage i,AffineTransform t) { delegate.drawRenderedImage(i,t); }
  @Override public void drawRenderableImage(RenderableImage i,AffineTransform t) { delegate.drawRenderableImage(i,t); }
  @Override public void drawString(String s,int x,int y) { delegate.drawString(s,x,y); }
  @Override public void drawString(String s,float x,float y) { delegate.drawString(s,x,y); }
  @Override public void drawString(AttributedCharacterIterator s,int x,int y) { delegate.drawString(s,x,y); }
  @Override public void drawString(AttributedCharacterIterator s,float x,float y) { delegate.drawString(s,x,y); }
  @Override public void drawGlyphVector(GlyphVector v,float x,float y) { delegate.drawGlyphVector(v,x,y); }
  @Override public boolean hit(Rectangle r,Shape s,boolean stroke) { return delegate.hit(r,s,stroke); }
  @Override public GraphicsConfiguration getDeviceConfiguration() { return delegate.getDeviceConfiguration(); }
  @Override public void setComposite(Composite c) { delegate.setComposite(c); }
  @Override public Composite getComposite() { return delegate.getComposite(); }
  @Override public void setPaint(Paint p) { delegate.setPaint(p); }
  @Override public Paint getPaint() { return delegate.getPaint(); }
  @Override public void setStroke(Stroke s) { delegate.setStroke(s); }
  @Override public Stroke getStroke() { return delegate.getStroke(); }
  @Override public void setRenderingHint(RenderingHints.Key k,Object v) { delegate.setRenderingHint(k,v); }
  @Override public Object getRenderingHint(RenderingHints.Key k) { return delegate.getRenderingHint(k); }
  @Override public void setRenderingHints(Map<?,?> m) { delegate.setRenderingHints(m); }
  @Override public void addRenderingHints(Map<?,?> m) { delegate.addRenderingHints(m); }
  @Override public RenderingHints getRenderingHints() { return delegate.getRenderingHints(); }
  @Override public void translate(int x,int y) { delegate.translate(x,y); }
  @Override public void translate(double x,double y) { delegate.translate(x,y); }
  @Override public void rotate(double a) { delegate.rotate(a); }
  @Override public void rotate(double a,double x,double y) { delegate.rotate(a,x,y); }
  @Override public void scale(double x,double y) { delegate.scale(x,y); }
  @Override public void shear(double x,double y) { delegate.shear(x,y); }
  @Override public void transform(AffineTransform t) { delegate.transform(t); }
  @Override public void setTransform(AffineTransform t) { delegate.setTransform(t); }
  @Override public AffineTransform getTransform() { return delegate.getTransform(); }
  @Override public void setBackground(Color c) { delegate.setBackground(c); }
  @Override public Color getBackground() { return delegate.getBackground(); }
  @Override public void clip(Shape s) { delegate.clip(s); }
  @Override public FontRenderContext getFontRenderContext() { return delegate.getFontRenderContext(); }
  @Override public Color getColor() { return delegate.getColor(); }
  @Override public void setColor(Color c) { delegate.setColor(c); }
  @Override public void setPaintMode() { delegate.setPaintMode(); }
  @Override public void setXORMode(Color c) { delegate.setXORMode(c); }
  @Override public Font getFont() { return delegate.getFont(); }
  @Override public void setFont(Font f) { delegate.setFont(f); }
  @Override public FontMetrics getFontMetrics(Font f) { return delegate.getFontMetrics(f); }
  @Override public Rectangle getClipBounds() { return delegate.getClipBounds(); }
  @Override public void clipRect(int x,int y,int w,int h) { delegate.clipRect(x,y,w,h); }
  @Override public void setClip(int x,int y,int w,int h) { delegate.setClip(x,y,w,h); }
  @Override public Shape getClip() { return delegate.getClip(); }
  @Override public void setClip(Shape s) { delegate.setClip(s); }
  @Override public void copyArea(int x,int y,int w,int h,int dx,int dy) { delegate.copyArea(x,y,w,h,dx,dy); }
  @Override public void drawLine(int x,int y,int xx,int yy) { delegate.drawLine(x,y,xx,yy); }
  @Override public void fillRect(int x,int y,int w,int h) { delegate.fillRect(x,y,w,h); }
  @Override public void clearRect(int x,int y,int w,int h) { delegate.clearRect(x,y,w,h); }
  @Override public void drawRoundRect(int x,int y,int w,int h,int a,int b) { delegate.drawRoundRect(x,y,w,h,a,b); }
  @Override public void fillRoundRect(int x,int y,int w,int h,int a,int b) { delegate.fillRoundRect(x,y,w,h,a,b); }
  @Override public void drawOval(int x,int y,int w,int h) { delegate.drawOval(x,y,w,h); }
  @Override public void fillOval(int x,int y,int w,int h) { delegate.fillOval(x,y,w,h); }
  @Override public void drawArc(int x,int y,int w,int h,int a,int b) { delegate.drawArc(x,y,w,h,a,b); }
  @Override public void fillArc(int x,int y,int w,int h,int a,int b) { delegate.fillArc(x,y,w,h,a,b); }
  @Override public void drawPolyline(int[] x,int[] y,int n) { delegate.drawPolyline(x,y,n); }
  @Override public void drawPolygon(int[] x,int[] y,int n) { delegate.drawPolygon(x,y,n); }
  @Override public void fillPolygon(int[] x,int[] y,int n) { delegate.fillPolygon(x,y,n); }
  @Override public boolean drawImage(Image i,int x,int y,ImageObserver o) { return delegate.drawImage(i,x,y,o); }
  @Override public boolean drawImage(Image i,int x,int y,int w,int h,ImageObserver o) { return delegate.drawImage(i,x,y,w,h,o); }
  @Override public boolean drawImage(Image i,int x,int y,Color c,ImageObserver o) { return delegate.drawImage(i,x,y,c,o); }
  @Override public boolean drawImage(Image i,int x,int y,int w,int h,Color c,ImageObserver o) { return delegate.drawImage(i,x,y,w,h,c,o); }
  @Override public boolean drawImage(Image i,int x,int y,int xx,int yy,int sx,int sy,int sxx,int syy,ImageObserver o) { return delegate.drawImage(i,x,y,xx,yy,sx,sy,sxx,syy,o); }
  @Override public boolean drawImage(Image i,int x,int y,int xx,int yy,int sx,int sy,int sxx,int syy,Color c,ImageObserver o) { return delegate.drawImage(i,x,y,xx,yy,sx,sy,sxx,syy,c,o); }
}
