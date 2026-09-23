package com.intellij.openapi.editor;

import com.intellij.openapi.Disposable;
import com.intellij.openapi.editor.event.EditorFactoryListener;

public abstract class EditorFactory {
  public static EditorFactory getInstance() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public abstract Editor[] getAllEditors();

  public abstract void addEditorFactoryListener(EditorFactoryListener listener, Disposable parentDisposable);
}
