package studio.background.rebased.bridge;

import com.intellij.openapi.Disposable;
import com.intellij.openapi.application.Application;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.editor.Editor;
import com.intellij.openapi.editor.EditorFactory;
import com.intellij.openapi.editor.colors.EditorColorsListener;
import com.intellij.openapi.editor.colors.EditorColorsScheme;
import com.intellij.openapi.editor.colors.TextAttributesKey;
import com.intellij.openapi.editor.colors.impl.DelegateColorScheme;
import com.intellij.openapi.editor.event.EditorFactoryEvent;
import com.intellij.openapi.editor.event.EditorFactoryListener;
import com.intellij.openapi.editor.ex.EditorEx;
import com.intellij.openapi.editor.markup.TextAttributes;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.Disposer;
import com.intellij.util.messages.MessageBus;
import com.intellij.util.messages.MessageBusConnection;
import com.intellij.util.messages.Topic;

import javax.swing.JComponent;
import javax.swing.JPanel;
import java.awt.Color;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Shared runtime fixtures for stub-classpath bridge unit tests.
 *
 * Protocol, editor, and window lifecycle tests use these fixtures on the stub classpath.
 * NativePainterApiTest runs separately against the real Rebased jars.
 *
 * Keep IdeGlassPane/PainterHelper fakes beside WindowIsolationTest, not in this class.
 */
public final class BridgeUnitFixtures {
  private BridgeUnitFixtures() {
  }

  public static final class FakeProject implements Project {
    private final String name;
    private boolean disposed;

    public FakeProject(String name) {
      this.name = name;
    }

    @Override
    public String getName() {
      return name;
    }

    @Override
    public boolean isDisposed() {
      return disposed;
    }

    public void disposeProject() {
      disposed = true;
    }
  }

  public static final class FakeScheme implements EditorColorsScheme {
    private final Map<TextAttributesKey, TextAttributes> attributes = new IdentityHashMap<>();
    private Color foreground = new Color(0xCCCCCC);
    private Color background = new Color(0x1E1E1E);

    public FakeScheme put(TextAttributesKey key, Color foregroundColor) {
      attributes.put(key, new TextAttributes(foregroundColor));
      return this;
    }

    public FakeScheme foreground(Color color) {
      foreground = color;
      return this;
    }

    public FakeScheme background(Color color) {
      background = color;
      return this;
    }

    @Override
    public TextAttributes getAttributes(TextAttributesKey key) {
      return attributes.get(key);
    }

    @Override
    public Color getDefaultForeground() {
      return foreground;
    }

    @Override
    public Color getDefaultBackground() {
      return background;
    }
  }

  public static final class FakeEditor implements EditorEx {
    private final Project project;
    private final JComponent component = new JPanel();
    private EditorColorsScheme scheme;
    private boolean disposed;
    private int reinitCount;

    public FakeEditor(Project project, EditorColorsScheme scheme) {
      this.project = project;
      this.scheme = scheme;
    }

    @Override
    public JComponent getContentComponent() {
      return component;
    }

    @Override
    public EditorColorsScheme getColorsScheme() {
      return scheme;
    }

    @Override
    public Project getProject() {
      return project;
    }

    @Override
    public boolean isDisposed() {
      return disposed;
    }

    @Override
    public void setColorsScheme(EditorColorsScheme next) {
      scheme = next;
    }

    @Override
    public void reinitSettings() {
      reinitCount++;
    }

    public int reinitCount() {
      return reinitCount;
    }

    public void disposeEditor() {
      disposed = true;
    }

    public boolean isLifted() {
      EditorColorsScheme current = scheme;
      if (current != null && current.getClass().getSimpleName().equals("LiftedScheme")) {
        return true;
      }
      if (current instanceof DelegateColorScheme delegate) {
        EditorColorsScheme inner = delegate.getDelegate();
        return inner != null && inner.getClass().getSimpleName().equals("LiftedScheme");
      }
      return false;
    }

    public EditorColorsScheme rawScheme() {
      return scheme;
    }
  }

  public static final class FakeEditorFactory extends EditorFactory {
    final List<Editor> editors = new CopyOnWriteArrayList<>();
    final List<EditorFactoryListener> listeners = new CopyOnWriteArrayList<>();

    @Override
    public Editor[] getAllEditors() {
      return editors.toArray(Editor[]::new);
    }

    @Override
    public void addEditorFactoryListener(EditorFactoryListener listener, Disposable parentDisposable) {
      listeners.add(listener);
      bindListenerLifetime(parentDisposable, () -> listeners.remove(listener));
    }

    public FakeEditor addEditor(Project project, EditorColorsScheme scheme) {
      FakeEditor editor = new FakeEditor(project, scheme);
      editors.add(editor);
      return editor;
    }

    public void fireCreated(Editor editor) {
      EditorFactoryEvent event = new EditorFactoryEvent() {
        @Override
        public Editor getEditor() {
          return editor;
        }
      };
      for (EditorFactoryListener listener : listeners) {
        listener.editorCreated(event);
      }
    }
  }

  public static final class Harness {
    public final FakeEditorFactory editorFactory = new FakeEditorFactory();
    public final List<Runnable> later = new ArrayList<>();
    private final List<EditorColorsListener> colorListeners = new CopyOnWriteArrayList<>();

    public Harness install() {
      ApplicationManager.setApplication(new Application() {
        @Override
        public void invokeLater(Runnable runnable, ModalityState state) {
          later.add(runnable);
        }

        @Override
        public MessageBus getMessageBus() {
          return parent -> new MessageBusConnection() {
            @Override
            public <L> void subscribe(Topic<L> topic, L handler) {
              EditorColorsListener listener = (EditorColorsListener)handler;
              colorListeners.add(listener);
              Disposer.register(parent, () -> colorListeners.remove(listener));
            }
          };
        }
      });
      EditorFactory.setInstance(editorFactory);
      return this;
    }

    public void flushLater() {
      List<Runnable> batch = new ArrayList<>(later);
      later.clear();
      batch.forEach(Runnable::run);
    }

    public void fireColorSchemeChanged() {
      for (EditorColorsListener listener : colorListeners) {
        listener.globalSchemeChange(null);
      }
      flushLater();
    }

    public void reset() {
      editorFactory.editors.clear();
      editorFactory.listeners.clear();
      later.clear();
      colorListeners.clear();
      ApplicationManager.setApplication(null);
      EditorFactory.setInstance(null);
    }
  }

  public static void assertTrue(boolean condition, String message) {
    if (!condition) {
      throw new AssertionError(message);
    }
  }

  public static void assertEquals(Object expected, Object actual, String message) {
    if (expected == null ? actual != null : !expected.equals(actual)) {
      throw new AssertionError(message + " expected=" + expected + " actual=" + actual);
    }
  }

  public static void assertFails(Runnable action, String needle) {
    try {
      action.run();
    }
    catch (RuntimeException error) {
      if (error.getMessage() != null && error.getMessage().contains(needle)) {
        return;
      }
      throw new AssertionError("expected message containing " + needle + " but was " + error, error);
    }
    throw new AssertionError("expected failure containing " + needle);
  }
}
