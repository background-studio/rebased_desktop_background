package com.intellij.openapi.editor.colors;

public final class TextAttributesKey {
  private final String externalName;

  private TextAttributesKey(String externalName) {
    this.externalName = externalName;
  }

  public static TextAttributesKey createTextAttributesKey(String externalName) {
    return new TextAttributesKey(externalName);
  }

  public String getExternalName() {
    return externalName;
  }

  @Override
  public boolean equals(Object other) {
    return other instanceof TextAttributesKey key && externalName.equals(key.externalName);
  }

  @Override
  public int hashCode() {
    return externalName.hashCode();
  }

  @Override
  public String toString() {
    return externalName;
  }
}
