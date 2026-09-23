package com.intellij.openapi.editor.colors.impl;

import com.intellij.openapi.editor.colors.EditorColorsScheme;
import com.intellij.openapi.editor.colors.TextAttributesKey;
import com.intellij.openapi.editor.markup.TextAttributes;

import java.awt.Color;

public class DelegateColorScheme implements EditorColorsScheme {
  public DelegateColorScheme(EditorColorsScheme delegate) {
  }

  public EditorColorsScheme getDelegate() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  @Override
  public TextAttributes getAttributes(TextAttributesKey key) {
    throw new UnsupportedOperationException("compile-only stub");
  }

  @Override
  public Color getDefaultForeground() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  @Override
  public Color getDefaultBackground() {
    throw new UnsupportedOperationException("compile-only stub");
  }
}
