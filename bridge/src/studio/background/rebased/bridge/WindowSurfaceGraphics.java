package studio.background.rebased.bridge;

import java.awt.*;
import java.util.Set;
import java.util.HashSet;
import javax.swing.UIManager;

/** Normalizes only ordinary opaque surface fills, BEFORE native wallpaper compositing.
 * Text, strokes, selections, translucent overlays, images and custom paints are untouched.
 * No component colours or global UI defaults are changed.
 */
final class WindowSurfaceGraphics extends WindowPainterGraphics {
  private final Color base;
  private final Set<Integer> surfaces;

  WindowSurfaceGraphics(Graphics2D graphics) { this(graphics, baseColor(), surfaceColors()); }
  private WindowSurfaceGraphics(Graphics2D graphics, Color base, Set<Integer> surfaces) {
    super(graphics); this.base = base; this.surfaces = surfaces;
  }
  static Color baseColor() {
    Color color = UIManager.getColor("MainWindow.background");
    return color != null ? color : UIManager.getColor("Panel.background");
  }
  private static Set<Integer> surfaceColors() {
    Set<Integer> colors = new HashSet<>();
    for (String key : new String[]{"Panel.background", "ToolWindow.background", "Editor.background",
        "EditorTabs.background", "Tree.background", "Table.background", "Viewport.background"}) {
      Color color = UIManager.getColor(key);
      if (color != null && color.getAlpha() == 255) { colors.add(color.getRGB()); }
    }
    return Set.copyOf(colors);
  }
  @Override public Graphics create() { return new WindowSurfaceGraphics((Graphics2D)delegate.create(), base, surfaces); }
  private void fillSurface(Runnable fill) {
    Paint previous = delegate.getPaint();
    boolean ordinary = base != null && previous instanceof Color color && surfaces.contains(color.getRGB())
      && delegate.getComposite() instanceof AlphaComposite composite
      && composite.getRule() == AlphaComposite.SRC_OVER && composite.getAlpha() == 1f;
    if (ordinary) { delegate.setColor(base); }
    try { fill.run(); } finally { if (ordinary) { delegate.setPaint(previous); } }
  }
  @Override public void fillRect(int x,int y,int w,int h) {
    if(w > 1 && h > 1) { fillSurface(() -> delegate.fillRect(x,y,w,h)); }
    else { delegate.fillRect(x,y,w,h); }
  }
  @Override public void fill(Shape shape) {
    Rectangle r = shape.getBounds();
    if(r.width > 1 && r.height > 1) { fillSurface(() -> delegate.fill(shape)); }
    else { delegate.fill(shape); }
  }
  @Override public void fillRoundRect(int x,int y,int w,int h,int a,int b) {
    fillSurface(() -> delegate.fillRoundRect(x,y,w,h,a,b));
  }
}
