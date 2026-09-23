package com.intellij.util.messages;

import com.intellij.openapi.Disposable;

public interface MessageBus {
  MessageBusConnection connect(Disposable parentDisposable);
}
