package com.intellij.openapi.editor.colors;

import com.intellij.openapi.editor.markup.TextAttributes;

import java.awt.Color;

public interface EditorColorsScheme {
  TextAttributes getAttributes(TextAttributesKey key);

  Color getDefaultForeground();

  Color getDefaultBackground();
}
