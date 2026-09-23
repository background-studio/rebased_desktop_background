package com.intellij.openapi.editor;

import com.intellij.openapi.editor.colors.EditorColorsScheme;

import javax.swing.JComponent;

public interface Editor {
  JComponent getContentComponent();

  EditorColorsScheme getColorsScheme();

  boolean isDisposed();
}
