package com.intellij.openapi.editor.ex;

import com.intellij.openapi.editor.Editor;
import com.intellij.openapi.editor.colors.EditorColorsScheme;

public interface EditorEx extends Editor {
  void setColorsScheme(EditorColorsScheme scheme);

  void reinitSettings();
}
