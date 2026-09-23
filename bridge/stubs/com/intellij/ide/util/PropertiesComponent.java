package com.intellij.ide.util;

import com.intellij.openapi.project.Project;

public abstract class PropertiesComponent {
  public static PropertiesComponent getInstance() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public static PropertiesComponent getInstance(Project project) {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public abstract String getValue(String name);
}
