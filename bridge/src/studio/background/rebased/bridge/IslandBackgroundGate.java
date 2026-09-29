package studio.background.rebased.bridge;

import com.intellij.openapi.wm.impl.IdeBackgroundUtil;

import javax.swing.JComponent;
import java.awt.Component;
import java.awt.Container;
import java.awt.Graphics2D;
import java.lang.reflect.Method;
import java.util.function.BiFunction;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.function.Predicate;

/** Enables window-local painters inside Islands panels without a global background property. */
final class IslandBackgroundGate {
  private static final String ISLANDS = "com.intellij.openapi.application.impl.islands.IslandsUICustomization";
  private static final Object NO_BACKGROUND = key("NO_BACKGROUND");
  private static final Object NO_BACKGROUND_PREDICATE = key("NO_BACKGROUND_PREDICATE");
  private final Map<JComponent, ScopedPredicate> changed = new WeakHashMap<>();
  private final IslandBorders borders = new IslandBorders();
  private Object transformRegistration;
  private Method disposeTransform;

  // Does not capture the component or window: detached renderer/components can be collected.
  private record ScopedPredicate(Predicate<JComponent> original) implements Predicate<JComponent> {
    @Override public boolean test(JComponent component) {
      // Keep explicit opt-outs (popups, specialized editors) instead of forcing every panel.
      return Boolean.TRUE.equals(component.getClientProperty(NO_BACKGROUND));
    }
  }

  /** EDT only. Also revisited on heartbeat for newly opened tabs and theme-recreated components. */
  void refresh(JComponent root) {
    if (transformRegistration == null) { installTransform(root); }
    changed.entrySet().removeIf(entry -> {
      JComponent component = entry.getKey();
      if (component == root || javax.swing.SwingUtilities.isDescendingFrom(component, root)) { return false; }
      restore(component, entry.getValue());
      return true;
    });
    visit(root);
    borders.refresh(root);
  }

  private void visit(Component component) {
    if (component instanceof JComponent jc) {
      if (enable(jc)) { jc.repaint(); }
    }
    if (component instanceof Container container) {
      for (Component child : container.getComponents()) { visit(child); }
    }
  }

  void clear() {
    borders.clear();
    if (transformRegistration != null) {
      try { disposeTransform.invoke(transformRegistration); }
      catch (ReflectiveOperationException error) { throw new IllegalStateException("移除窗口绘制变换失败", error); }
      transformRegistration = null;
    }
    changed.forEach(IslandBackgroundGate::restore);
    changed.clear();
  }

  private boolean enable(JComponent component) {
    Object value = component.getClientProperty(NO_BACKGROUND_PREDICATE);
    // Do not replace arbitrary component/plugin predicates; only the platform's Islands gate.
    if (!(value instanceof Predicate<?>) || !value.getClass().getName().startsWith(ISLANDS + "$$Lambda")) { return false; }
    @SuppressWarnings("unchecked")
    Predicate<JComponent> original = (Predicate<JComponent>)value;
    ScopedPredicate scoped = new ScopedPredicate(original);
    changed.put(component, scoped);
    component.putClientProperty(NO_BACKGROUND_PREDICATE, scoped);
    return true;
  }

  /** Cell renderers may be attached only while painting, after a heartbeat tree scan. */
  @SuppressWarnings("unchecked")
  private void installTransform(JComponent root) {
    try {
      ClassLoader loader = IdeBackgroundUtil.class.getClassLoader();
      Class<?> transforms = Class.forName("com.intellij.util.ui.JBSwingUtilities", true, loader);
      Class<?> nativeType = Class.forName(IdeBackgroundUtil.class.getName() + "$MyTransform", true, loader);
      var constructor = nativeType.getDeclaredConstructor();
      constructor.setAccessible(true);
      BiFunction<JComponent, Graphics2D, Graphics2D> nativeTransform =
        (BiFunction<JComponent, Graphics2D, Graphics2D>)constructor.newInstance();
      BiFunction<JComponent, Graphics2D, Graphics2D> transform = (component, graphics) -> {
        if (component != root && !javax.swing.SwingUtilities.isDescendingFrom(component, root)) { return graphics; }
        enable(component);
        // Replay the platform transform after lifting the Islands-only veto. This preserves
        // selection colours, popup opt-outs, editor rules and the window's named painter routing.
        Graphics2D original = graphics;
        while (true) {
          if (original instanceof WindowPainterGraphics protectedGraphics) { original = protectedGraphics.delegate; continue; }
          Graphics2D unwrapped = IdeBackgroundUtil.getOriginalGraphics(original);
          if (unwrapped == original) { break; }
          original = unwrapped;
        }
        // Always shield this window's graphics, including ancestors with no Islands predicate.
        // Otherwise a holder can unwrap an unprotected parent and paint its 2-3px insets black.
        Graphics2D surface = component instanceof javax.swing.text.JTextComponent
          || component instanceof javax.swing.AbstractButton ? original : new WindowSurfaceGraphics(original);
        Graphics2D painted = nativeTransform.apply(component, surface);
        return painted == surface ? original : new WindowPainterGraphics(painted);
      };
      disposeTransform = Class.forName("com.intellij.openapi.Disposable", true, loader).getMethod("dispose");
      transformRegistration = transforms.getMethod("addGlobalCGTransform", BiFunction.class).invoke(null, transform);
    }
    catch (ReflectiveOperationException error) {
      throw new IllegalStateException("Rebased 窗口主体绘制接口不兼容", error);
    }
  }

  private static void restore(JComponent component, ScopedPredicate scoped) {
    // A theme/plugin may have installed a different predicate after us. Never overwrite it.
    if (component.getClientProperty(NO_BACKGROUND_PREDICATE) == scoped) {
      component.putClientProperty(NO_BACKGROUND_PREDICATE, scoped.original());
      component.repaint();
    }
  }

  private static Object key(String name) {
    try { return IdeBackgroundUtil.class.getField(name).get(null); }
    catch (ReflectiveOperationException error) { throw new IllegalStateException("Rebased 背景抑制接口不兼容：" + name, error); }
  }
}
