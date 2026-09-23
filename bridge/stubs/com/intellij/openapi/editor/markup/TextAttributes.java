package com.intellij.openapi.editor.markup;

import java.awt.Color;

public class TextAttributes implements Cloneable {
  public Color getForegroundColor() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public void setForegroundColor(Color color) {
    throw new UnsupportedOperationException("compile-only stub");
  }

  @Override
  public TextAttributes clone() {
    throw new UnsupportedOperationException("compile-only stub");
  }
}
