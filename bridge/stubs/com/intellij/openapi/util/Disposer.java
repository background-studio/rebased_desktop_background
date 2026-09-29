package com.intellij.openapi.util;

import com.intellij.openapi.Disposable;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public final class Disposer {
  private static final Map<Disposable, List<Disposable>> CHILDREN = new ConcurrentHashMap<>();

  private Disposer() {
  }

  public static Disposable newDisposable(String debugName) {
    return new NamedDisposable(debugName);
  }

  public static void register(Disposable parent, Disposable child) {
    CHILDREN.computeIfAbsent(parent, ignored -> new ArrayList<>()).add(child);
  }

  public static void dispose(Disposable disposable) {
    List<Disposable> children = CHILDREN.remove(disposable);
    if (children != null) {
      for (int i = children.size() - 1; i >= 0; i--) {
        dispose(children.get(i));
      }
    }
    disposable.dispose();
  }

  private static final class NamedDisposable implements Disposable {
    private final String name;
    private boolean disposed;

    private NamedDisposable(String name) {
      this.name = name;
    }

    @Override
    public void dispose() {
      disposed = true;
    }

    @Override
    public String toString() {
      return "Disposable(" + name + ", disposed=" + disposed + ")";
    }
  }
}
