package com.intellij.openapi.project;

public abstract class ProjectManager {
  private static volatile ProjectManager instance;

  public static ProjectManager getInstance() {
    ProjectManager current = instance;
    if (current == null) {
      throw new IllegalStateException("ProjectManager is not set");
    }
    return current;
  }

  /** Test/runtime hook used by bridge unit fixtures. */
  public static void setInstance(ProjectManager next) {
    instance = next;
  }

  public abstract Project[] getOpenProjects();
}
