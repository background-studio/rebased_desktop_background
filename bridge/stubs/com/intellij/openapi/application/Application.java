package com.intellij.openapi.application;

import com.intellij.util.messages.MessageBus;

public interface Application {
  void invokeLater(Runnable runnable, ModalityState state);

  MessageBus getMessageBus();
}
