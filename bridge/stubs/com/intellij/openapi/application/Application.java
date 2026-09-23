package com.intellij.openapi.application;

public interface Application {
  void invokeLater(Runnable runnable, ModalityState state);
}
