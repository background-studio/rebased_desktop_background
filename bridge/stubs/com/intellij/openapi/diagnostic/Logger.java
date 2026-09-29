package com.intellij.openapi.diagnostic;

public abstract class Logger {
  private static final Logger NOOP = new Logger() {
    @Override
    public void warn(String message, Throwable throwable) {
    }
  };

  public static Logger getInstance(Class<?> type) {
    return NOOP;
  }

  public void info(String message) {
  }

  public void warn(String message) {
  }

  public abstract void warn(String message, Throwable throwable);
}
