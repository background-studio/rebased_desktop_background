package com.intellij.openapi.editor.colors;

import java.util.EventListener;

public interface EditorColorsListener extends EventListener {
  void globalSchemeChange(EditorColorsScheme scheme);
}
