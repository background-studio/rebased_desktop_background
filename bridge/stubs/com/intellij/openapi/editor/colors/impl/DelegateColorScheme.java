package com.intellij.openapi.editor.colors.impl;

import com.intellij.openapi.editor.colors.EditorColorsScheme;
import com.intellij.openapi.editor.colors.TextAttributesKey;
import com.intellij.openapi.editor.markup.TextAttributes;

import java.awt.Color;

public class DelegateColorScheme implements EditorColorsScheme {
  private final EditorColorsScheme delegate;

  public DelegateColorScheme(EditorColorsScheme delegate) {
    this.delegate = delegate;
  }

  public EditorColorsScheme getDelegate() {
    return delegate;
  }

  @Override
  public TextAttributes getAttributes(TextAttributesKey key) {
    return delegate.getAttributes(key);
  }

  @Override
  public Color getDefaultForeground() {
    return delegate.getDefaultForeground();
  }

  @Override
  public Color getDefaultBackground() {
    return delegate.getDefaultBackground();
  }
}
