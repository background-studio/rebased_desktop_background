package com.intellij.openapi.wm;

import com.intellij.openapi.project.Project;

import javax.swing.JFrame;

public abstract class WindowManager {
  private static volatile WindowManager instance;

  public static WindowManager getInstance() {
    WindowManager current = instance;
    if (current == null) {
      throw new IllegalStateException("WindowManager is not set");
    }
    return current;
  }

  /** Test/runtime hook used by bridge unit fixtures. */
  public static void setInstance(WindowManager next) {
    instance = next;
  }

  public abstract JFrame getFrame(Project project);
}
