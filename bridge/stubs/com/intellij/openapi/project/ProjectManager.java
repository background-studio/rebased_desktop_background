package com.intellij.openapi.project;

public abstract class ProjectManager {
  public static ProjectManager getInstance() {
    throw new UnsupportedOperationException("compile-only stub");
  }

  public abstract Project[] getOpenProjects();
}
