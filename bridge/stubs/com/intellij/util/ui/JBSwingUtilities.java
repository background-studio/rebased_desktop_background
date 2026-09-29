package com.intellij.util.ui;
import com.intellij.openapi.Disposable;
import javax.swing.JComponent;
import java.awt.Graphics2D;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BiFunction;
public final class JBSwingUtilities {
  private static final List<BiFunction<JComponent, Graphics2D, Graphics2D>> transforms = new ArrayList<>();
  public static Disposable addGlobalCGTransform(BiFunction<JComponent, Graphics2D, Graphics2D> transform) {
    transforms.add(transform);
    return () -> transforms.remove(transform);
  }
  public static Graphics2D runGlobalCGTransform(JComponent component, Graphics2D graphics) {
    for (var transform : List.copyOf(transforms)) { graphics = transform.apply(component, graphics); }
    return graphics;
  }
  public static int registrations() { return transforms.size(); }
}
