package com.intellij.openapi.application;

public class ApplicationManager {
  private static volatile Application application;

  public static Application getApplication() {
    Application current = application;
    if (current == null) {
      throw new IllegalStateException("Application is not set");
    }
    return current;
  }

  /** Test/runtime hook used by bridge unit fixtures. */
  public static void setApplication(Application next) {
    application = next;
  }
}
