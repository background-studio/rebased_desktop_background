package com.intellij.openapi.application;

import com.intellij.openapi.util.BuildNumber;

public abstract class ApplicationInfo {
  public static ApplicationInfo getInstance() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public abstract BuildNumber getBuild();
}
