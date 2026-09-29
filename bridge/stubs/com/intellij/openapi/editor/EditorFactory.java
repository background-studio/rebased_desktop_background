package com.intellij.openapi.editor;

import com.intellij.openapi.Disposable;
import com.intellij.openapi.editor.event.EditorFactoryListener;
import com.intellij.openapi.util.Disposer;

public abstract class EditorFactory {
  private static volatile EditorFactory instance;

  public static EditorFactory getInstance() {
    EditorFactory current = instance;
    if (current == null) {
      throw new IllegalStateException("EditorFactory is not set");
    }
    return current;
  }

  /** Test/runtime hook used by bridge unit fixtures. */
  public static void setInstance(EditorFactory next) {
    instance = next;
  }

  public abstract Editor[] getAllEditors();

  public abstract void addEditorFactoryListener(EditorFactoryListener listener, Disposable parentDisposable);

  protected static void bindListenerLifetime(Disposable parentDisposable, Runnable cleanup) {
    Disposer.register(parentDisposable, cleanup::run);
  }
}
