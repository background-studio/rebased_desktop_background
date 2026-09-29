package com.intellij.openapi.application;

public abstract class ModalityState {
  private static final ModalityState ANY = new ModalityState() {};

  public static ModalityState any() {
    return ANY;
  }
}
