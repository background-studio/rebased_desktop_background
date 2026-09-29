package studio.background.rebased.bridge;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static studio.background.rebased.bridge.BridgeUnitFixtures.assertEquals;
import static studio.background.rebased.bridge.BridgeUnitFixtures.assertFails;
import static studio.background.rebased.bridge.BridgeUnitFixtures.assertTrue;

/** Unit entry: nested JSON object/array parse+stringify, depth cap, duplicate keys. */
public final class FlatJsonNestedTest {
  public static void main(String[] args) {
    Map<String, Object> nested = FlatJson.parse(
      "{\"type\":\"desired\",\"mode\":\"windows\",\"windows\":{\"t1\":{\"opacity\":20,\"editor\":true},\"t2\":{\"opacity\":30}},\"targetIds\":[\"t1\",\"t2\"]}"
    );
    assertEquals("windows", nested.get("mode"), "mode");
    @SuppressWarnings("unchecked")
    Map<String, Object> windows = (Map<String, Object>)nested.get("windows");
    assertEquals(20L, ((Map<?, ?>)windows.get("t1")).get("opacity"), "t1.opacity");
    @SuppressWarnings("unchecked")
    List<Object> ids = (List<Object>)nested.get("targetIds");
    assertEquals(List.of("t1", "t2"), ids, "targetIds");

    String encoded = FlatJson.object(
      "type", "report",
      "seq", 3L,
      "ok", true,
      "message", "",
      "conflicts", "",
      "targetIds", List.of("a", "b"),
      "windows", Map.of("a", Map.of("opacity", 10, "editor", true))
    );
    Map<String, Object> roundTrip = FlatJson.parse(encoded);
    assertEquals(List.of("a", "b"), roundTrip.get("targetIds"), "roundTrip.targetIds");
    assertTrue(encoded.contains("\"windows\":{"), "serialized windows object");

    assertFails(() -> FlatJson.parse("{\"a\":1,\"a\":2}"), "duplicate key");
    assertFails(() -> FlatJson.object("a", 1, "a", 2), "duplicate key");

    String deepOk = nestObject(FlatJson.MAX_DEPTH);
    FlatJson.parse(deepOk);
    assertFails(() -> FlatJson.parse(nestObject(FlatJson.MAX_DEPTH + 1)), "nesting exceeds");

    // Leaves also consume a depth slot in writeValue, so a pure list may nest up to MAX_DEPTH.
    FlatJson.stringify(nestList(FlatJson.MAX_DEPTH));
    assertFails(() -> FlatJson.stringify(nestList(FlatJson.MAX_DEPTH + 1)), "nesting exceeds");

    System.out.println("FlatJsonNestedTest: OK");
  }

  private static String nestObject(int depth) {
    StringBuilder out = new StringBuilder();
    for (int i = 0; i < depth; i++) {
      out.append("{\"k\":");
    }
    out.append("1");
    out.append("}".repeat(depth));
    return out.toString();
  }

  private static List<Object> nestList(int depth) {
    List<Object> current = new ArrayList<>();
    current.add(1);
    for (int i = 1; i < depth; i++) {
      List<Object> wrap = new ArrayList<>();
      wrap.add(current);
      current = wrap;
    }
    return current;
  }
}
