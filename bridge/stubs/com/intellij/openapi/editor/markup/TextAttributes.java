package com.intellij.openapi.editor.markup;

import java.awt.Color;

public class TextAttributes implements Cloneable {
  private Color foregroundColor;

  public TextAttributes() {
  }

  public TextAttributes(Color foregroundColor) {
    this.foregroundColor = foregroundColor;
  }

  public Color getForegroundColor() {
    return foregroundColor;
  }

  public void setForegroundColor(Color color) {
    foregroundColor = color;
  }

  @Override
  public TextAttributes clone() {
    return new TextAttributes(foregroundColor);
  }
}
