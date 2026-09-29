package com.intellij.openapi.editor;

import com.intellij.openapi.editor.colors.EditorColorsScheme;
import com.intellij.openapi.project.Project;

import javax.swing.JComponent;

public interface Editor {
  JComponent getContentComponent();

  EditorColorsScheme getColorsScheme();

  Project getProject();

  boolean isDisposed();
}
