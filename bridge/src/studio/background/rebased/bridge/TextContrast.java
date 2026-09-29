package studio.background.rebased.bridge;

import com.intellij.openapi.Disposable;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.diagnostic.Logger;
import com.intellij.openapi.editor.Editor;
import com.intellij.openapi.editor.EditorFactory;
import com.intellij.openapi.editor.colors.EditorColorsListener;
import com.intellij.openapi.editor.colors.EditorColorsManager;
import com.intellij.openapi.editor.colors.EditorColorsScheme;
import com.intellij.openapi.editor.colors.TextAttributesKey;
import com.intellij.openapi.editor.colors.impl.DelegateColorScheme;
import com.intellij.openapi.editor.event.EditorFactoryEvent;
import com.intellij.openapi.editor.event.EditorFactoryListener;
import com.intellij.openapi.editor.ex.EditorEx;
import com.intellij.openapi.editor.markup.TextAttributes;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.Disposer;

import java.awt.Color;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Pushes editor text colours away from the editor background while a Background Studio image
 * is shown, because the image lifts the background under the text. Each editor's own scheme is
 * wrapped rather than replaced, so its per-editor overrides keep working, and the global scheme
 * is never touched, so nothing is persisted. All methods except the scheme reads run on the EDT.
 */
final class TextContrast {
  private static final Logger LOG = Logger.getInstance(TextContrast.class);

  private volatile float factor;
  private volatile boolean globalMode;
  private volatile Map<Project, Float> projectFactors = Map.of();
  private Disposable listeners;

  void apply(int percent) {
    if (percent <= 0) {
      clear();
      return;
    }
    float next = percent / 100f;
    boolean changed = !globalMode || next != factor;
    globalMode = true;
    factor = next;
    projectFactors = Map.of();
    listen();
    int wrapped = 0;
    int refreshed = 0;
    for (Editor editor : EditorFactory.getInstance().getAllEditors()) {
      if (!(editor instanceof EditorEx ex) || ex.isDisposed()) {
        continue;
      }
      if (liftedScheme(ex) == null) {
        wrap(ex);
        wrapped++;
      }
      else if (changed) {
        ex.reinitSettings();
        ex.getContentComponent().repaint();
        refreshed++;
      }
    }
    if (changed || wrapped > 0) {
      LOG.info("Background Studio text contrast " + percent + "%: wrapped " + wrapped
               + " editor(s), refreshed " + refreshed);
    }
  }

  /** Per-project contrast. Projects absent from the map (or <=0) are unwrapped. */
  void apply(Map<Project, Integer> percents) {
    Map<Project, Float> next = new IdentityHashMap<>();
    if (percents != null) {
      for (Map.Entry<Project, Integer> entry : percents.entrySet()) {
        Project project = entry.getKey();
        Integer percent = entry.getValue();
        if (project == null || percent == null || percent <= 0) {
          continue;
        }
        next.put(project, percent / 100f);
      }
    }
    if (next.isEmpty()) {
      clear();
      return;
    }
    Map<Project, Float> previous = projectFactors;
    boolean wasGlobal = globalMode;
    float previousGlobal = factor;
    globalMode = false;
    factor = 0;
    projectFactors = Collections.unmodifiableMap(next);
    listen();
    int wrapped = 0;
    int refreshed = 0;
    int restored = 0;
    for (Editor editor : EditorFactory.getInstance().getAllEditors()) {
      if (!(editor instanceof EditorEx ex) || ex.isDisposed()) {
        continue;
      }
      Project project = ex.getProject();
      float wanted = factorFor(project);
      float previousWanted = wasGlobal ? previousGlobal : previousFactor(previous, project);
      if (wanted <= 0) {
        if (unwrap(ex)) {
          restored++;
        }
        continue;
      }
      if (liftedScheme(ex) == null) {
        wrap(ex);
        wrapped++;
      }
      else if (wanted != previousWanted) {
        // Only reinit editors whose project factor actually changed.
        ex.reinitSettings();
        ex.getContentComponent().repaint();
        refreshed++;
      }
    }
    if (wrapped > 0 || refreshed > 0 || restored > 0) {
      LOG.info("Background Studio per-project text contrast: wrapped " + wrapped
               + ", refreshed " + refreshed + ", cleared " + restored);
    }
  }

  private static float previousFactor(Map<Project, Float> previous, Project project) {
    if (project == null) {
      return 0f;
    }
    Float value = previous.get(project);
    return value == null ? 0f : value.floatValue();
  }

  void clear() {
    if (listeners == null && factor <= 0 && projectFactors.isEmpty()) {
      return;
    }
    factor = 0;
    globalMode = false;
    projectFactors = Map.of();
    if (listeners != null) {
      Disposer.dispose(listeners);
      listeners = null;
    }
    int restored = 0;
    for (Editor editor : EditorFactory.getInstance().getAllEditors()) {
      if (editor instanceof EditorEx ex && !ex.isDisposed() && unwrap(ex)) {
        restored++;
      }
    }
    LOG.info("Background Studio text contrast cleared: restored " + restored + " editor(s)");
  }

  private float factorFor(Project project) {
    if (globalMode) {
      return factor;
    }
    if (project == null) {
      return 0;
    }
    Float value = projectFactors.get(project);
    return value == null ? 0f : value;
  }

  private void listen() {
    if (listeners != null) {
      return;
    }
    Disposable disposable = Disposer.newDisposable("Background Studio text contrast");
    EditorFactory.getInstance().addEditorFactoryListener(new EditorFactoryListener() {
      @Override
      public void editorCreated(EditorFactoryEvent event) {
        Editor editor = event.getEditor();
        // Creators often install their own scheme right after the editor is built.
        ApplicationManager.getApplication().invokeLater(() -> {
          if (!(editor instanceof EditorEx ex) || ex.isDisposed()) {
            return;
          }
          float wanted = factorFor(ex.getProject());
          if (wanted > 0 && liftedScheme(ex) == null) {
            wrap(ex);
          }
        }, ModalityState.any());
      }
    }, disposable);
    ApplicationManager.getApplication().getMessageBus().connect(disposable).subscribe(
      EditorColorsManager.TOPIC,
      (EditorColorsListener)scheme -> ApplicationManager.getApplication().invokeLater(this::rewrapAll, ModalityState.any())
    );
    listeners = disposable;
  }

  /** The wrapped schemes keep pointing at the old global scheme after a theme switch. */
  private void rewrapAll() {
    if (globalMode) {
      if (factor <= 0) {
        return;
      }
    }
    else if (projectFactors.isEmpty()) {
      return;
    }
    int rewrapped = 0;
    int restored = 0;
    for (Editor editor : EditorFactory.getInstance().getAllEditors()) {
      if (!(editor instanceof EditorEx ex) || ex.isDisposed()) {
        continue;
      }
      float wanted = factorFor(ex.getProject());
      if (wanted <= 0) {
        if (unwrap(ex)) {
          restored++;
        }
        continue;
      }
      if (unwrap(ex) || liftedScheme(ex) == null) {
        wrap(ex);
        rewrapped++;
      }
    }
    LOG.info("Background Studio text contrast followed a colour scheme change: rewrapped "
             + rewrapped + ", cleared " + restored);
  }

  private void wrap(EditorEx editor) {
    editor.setColorsScheme(new LiftedScheme(editor.getColorsScheme(), this, editor.getProject()));
    editor.getContentComponent().repaint();
  }

  /** Handing the original delegate back makes the editor re-read the current global scheme. */
  private static boolean unwrap(EditorEx editor) {
    LiftedScheme lifted = liftedScheme(editor);
    if (lifted == null) {
      return false;
    }
    editor.setColorsScheme(lifted.getDelegate());
    editor.getContentComponent().repaint();
    return true;
  }

  /** {@code EditorImpl} always wraps a foreign scheme in its own delegate. */
  private static LiftedScheme liftedScheme(EditorEx editor) {
    EditorColorsScheme scheme = editor.getColorsScheme();
    if (scheme instanceof LiftedScheme lifted) {
      return lifted;
    }
    if (scheme instanceof DelegateColorScheme delegate && delegate.getDelegate() instanceof LiftedScheme lifted) {
      return lifted;
    }
    return null;
  }

  static Color lift(Color color, float factor, boolean darkBackground) {
    float[] hsb = Color.RGBtoHSB(color.getRed(), color.getGreen(), color.getBlue(), null);
    float brightness = darkBackground ? hsb[2] + (1f - hsb[2]) * factor : hsb[2] * (1f - factor);
    int rgb = Color.HSBtoRGB(hsb[0], hsb[1], brightness) & 0xFFFFFF;
    return new Color(rgb | (color.getAlpha() << 24), true);
  }

  static boolean isDark(Color background) {
    return background == null
           || 0.2126 * background.getRed() + 0.7152 * background.getGreen() + 0.0722 * background.getBlue() < 128;
  }

  private static final class LiftedScheme extends DelegateColorScheme {
    private record LiftedAttributes(TextAttributes source, float factor, boolean dark, TextAttributes result) {
    }

    private record LiftedColor(Color source, float factor, boolean dark, Color result) {
    }

    private final TextContrast owner;
    private final Project project;
    private final ConcurrentHashMap<TextAttributesKey, LiftedAttributes> attributes = new ConcurrentHashMap<>();
    private volatile LiftedColor foreground;

    LiftedScheme(EditorColorsScheme delegate, TextContrast owner, Project project) {
      super(delegate);
      this.owner = owner;
      this.project = project;
    }

    private float currentFactor() {
      return owner.factorFor(project);
    }

    @Override
    public TextAttributes getAttributes(TextAttributesKey key) {
      TextAttributes source = super.getAttributes(key);
      float factor = currentFactor();
      if (source == null || factor <= 0 || source.getForegroundColor() == null) {
        return source;
      }
      boolean dark = isDark(super.getDefaultBackground());
      LiftedAttributes cached = attributes.get(key);
      if (cached != null && cached.source() == source && cached.factor() == factor && cached.dark() == dark) {
        return cached.result();
      }
      TextAttributes result = source.clone();
      result.setForegroundColor(lift(source.getForegroundColor(), factor, dark));
      attributes.put(key, new LiftedAttributes(source, factor, dark, result));
      return result;
    }

    @Override
    public Color getDefaultForeground() {
      Color source = super.getDefaultForeground();
      float factor = currentFactor();
      if (source == null || factor <= 0) {
        return source;
      }
      boolean dark = isDark(super.getDefaultBackground());
      LiftedColor cached = foreground;
      if (cached != null && cached.source() == source && cached.factor() == factor && cached.dark() == dark) {
        return cached.result();
      }
      Color result = lift(source, factor, dark);
      foreground = new LiftedColor(source, factor, dark, result);
      return result;
    }
  }
}
