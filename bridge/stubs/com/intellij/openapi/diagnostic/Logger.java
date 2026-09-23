package com.intellij.openapi.diagnostic;

public abstract class Logger {
  public static Logger getInstance(Class<?> type) {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public void info(String message) {
  }

  public void warn(String message) {
  }

  public abstract void warn(String message, Throwable throwable);
}
