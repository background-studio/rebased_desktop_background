package com.intellij.openapi.editor.event;

import java.util.EventListener;

public interface EditorFactoryListener extends EventListener {
  default void editorCreated(EditorFactoryEvent event) {
  }

  default void editorReleased(EditorFactoryEvent event) {
  }
}
