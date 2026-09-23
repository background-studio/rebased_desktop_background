package com.intellij.util.messages;

public interface MessageBusConnection {
  <L> void subscribe(Topic<L> topic, L handler);
}
