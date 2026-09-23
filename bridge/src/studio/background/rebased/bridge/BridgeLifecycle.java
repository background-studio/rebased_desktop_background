package studio.background.rebased.bridge;

import com.intellij.ide.AppLifecycleListener;

import java.util.List;

public final class BridgeLifecycle implements AppLifecycleListener {
  @Override
  public void appFrameCreated(List<String> commandLineArgs) {
    BridgeClient.startOnce();
  }

  @Override
  public void appStarted() {
    BridgeClient.startOnce();
  }

  @Override
  public void appWillBeClosed(boolean isRestart) {
    BridgeClient.stop();
  }
}
