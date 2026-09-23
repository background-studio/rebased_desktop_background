package com.intellij.ide;

import java.util.List;

public interface AppLifecycleListener {
  default void appFrameCreated(List<String> commandLineArgs) {
  }

  default void appStarted() {
  }

  default void appWillBeClosed(boolean isRestart) {
  }
}
