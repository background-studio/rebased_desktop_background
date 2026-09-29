package studio.background.rebased.bridge;

import static studio.background.rebased.bridge.BridgeUnitFixtures.assertEquals;
import static studio.background.rebased.bridge.BridgeUnitFixtures.assertTrue;

/** Unit entry: LineChannel byte limit. */
public final class LineChannelLimitTest {
  public static void main(String[] args) {
    assertEquals(256 * 1024, LineChannel.MAX_LINE_BYTES, "MAX_LINE_BYTES");
    assertTrue(LineChannel.MAX_LINE_BYTES > 64 * 1024, "limit raised beyond old 64KiB");
    System.out.println("LineChannelLimitTest: OK (" + LineChannel.MAX_LINE_BYTES + " bytes)");
  }
}
