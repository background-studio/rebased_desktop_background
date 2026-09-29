package studio.background.rebased.bridge;

import com.intellij.openapi.application.ApplicationInfo;
import com.intellij.openapi.diagnostic.Logger;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;

final class BridgeClient implements Runnable {
  static final String PIPE_NAME = "\\\\.\\pipe\\background-studio-rebased-bridge";
  static final int BRIDGE_PROTOCOL = 2;

  private static final long RETRY_MILLIS = 3_000;
  private static final long REJECTED_RETRY_MILLIS = 30_000;
  private static final Logger LOG = Logger.getInstance(BridgeClient.class);
  private static final AtomicBoolean STARTED = new AtomicBoolean();
  private static volatile boolean stopping;

  private final BackgroundApplier applier = new BackgroundApplier();
  private long nextRetryMillis = RETRY_MILLIS;

  static void startOnce() {
    if (!STARTED.compareAndSet(false, true)) {
      return;
    }
    Thread thread = new Thread(new BridgeClient(), "Background Studio Bridge");
    thread.setDaemon(true);
    thread.start();
  }

  static void stop() {
    stopping = true;
  }

  @Override
  public void run() {
    LOG.info("Background Studio Bridge " + BridgeBuildInfo.VERSION + " started, pid " + ProcessHandle.current().pid());
    boolean waitingLogged = false;
    while (!stopping) {
      RandomAccessFile pipe;
      try {
        pipe = new RandomAccessFile(PIPE_NAME, "rw");
      }
      catch (FileNotFoundException e) {
        if (!waitingLogged) {
          LOG.info("Background Studio worker pipe is not available yet: " + e.getMessage());
          waitingLogged = true;
        }
        sleep(RETRY_MILLIS);
        continue;
      }
      waitingLogged = false;
      nextRetryMillis = RETRY_MILLIS;
      LOG.info("Connected to Background Studio worker");
      try {
        session(new LineChannel(pipe));
        LOG.info("Background Studio worker closed the session");
      }
      catch (IOException | IllegalArgumentException e) {
        LOG.info("Background Studio worker session ended: " + e.getMessage());
      }
      finally {
        try {
          pipe.close();
        }
        catch (IOException ignored) {
        }
      }
      sleep(nextRetryMillis);
    }
  }

  private void session(LineChannel channel) throws IOException {
    channel.writeLine(FlatJson.object(
      "type", "hello",
      "bridgeProtocol", BRIDGE_PROTOCOL,
      "bridgeVersion", BridgeBuildInfo.VERSION,
      "pid", ProcessHandle.current().pid(),
      "ideBuild", ideBuild()
    ));
    while (!stopping) {
      String line = channel.readLine();
      if (line == null) {
        return;
      }
      Map<String, Object> message = FlatJson.parse(line);
      Object type = message.get("type");
      if ("reject".equals(type)) {
        LOG.warn("Background Studio worker rejected this bridge: " + message.get("message"));
        nextRetryMillis = REJECTED_RETRY_MILLIS;
        return;
      }
      if (!"desired".equals(type)) {
        throw new IOException("unexpected worker message type: " + type);
      }
      Object seq = message.get("seq");
      if (!(seq instanceof Long)) {
        throw new IOException("desired.seq must be an integer");
      }
      BackgroundApplier.Result result = applier.apply(message);
      channel.writeLine(FlatJson.object(
        "type", "report",
        "seq", seq,
        "ok", result.ok(),
        "message", bounded(result.message(), 2048),
        "conflicts", bounded(result.conflicts(), 1024),
        "targetIds", applier.targetIds()
      ));
    }
  }

  private static String ideBuild() {
    try {
      return ApplicationInfo.getInstance().getBuild().asString();
    }
    catch (RuntimeException e) {
      return "unknown";
    }
  }

  private static String bounded(String text, int maximum) {
    int count = text.codePointCount(0, text.length());
    return count <= maximum ? text : text.substring(0, text.offsetByCodePoints(0, maximum - 1)) + "…";
  }

  private static void sleep(long millis) {
    try {
      Thread.sleep(millis);
    }
    catch (InterruptedException e) {
      Thread.currentThread().interrupt();
      stopping = true;
    }
  }
}
