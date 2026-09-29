package studio.background.rebased.bridge;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** JSON objects/arrays whose leaves are strings, integers, booleans or null. Nested depth <= 16. */
final class FlatJson {
  static final int MAX_DEPTH = 16;

  private final String text;
  private int position;

  private FlatJson(String text) {
    this.text = text;
  }

  static Map<String, Object> parse(String text) {
    FlatJson parser = new FlatJson(text);
    parser.skipWhitespace();
    Map<String, Object> result = parser.readObject(1);
    parser.skipWhitespace();
    if (parser.position != text.length()) {
      throw parser.error("trailing characters");
    }
    return result;
  }

  static String object(Object... keysAndValues) {
    if (keysAndValues.length % 2 != 0) {
      throw new IllegalArgumentException("keys and values must be paired");
    }
    Map<String, Object> map = new LinkedHashMap<>();
    for (int i = 0; i < keysAndValues.length; i += 2) {
      if (!(keysAndValues[i] instanceof String key)) {
        throw new IllegalArgumentException("JSON keys must be strings");
      }
      if (map.containsKey(key)) {
        throw new IllegalArgumentException("duplicate key " + key);
      }
      map.put(key, keysAndValues[i + 1]);
    }
    StringBuilder out = new StringBuilder();
    writeValue(out, map, 1);
    return out.toString();
  }

  static String stringify(Object value) {
    StringBuilder out = new StringBuilder();
    writeValue(out, value, 1);
    return out.toString();
  }

  private Map<String, Object> readObject(int depth) {
    ensureDepth(depth);
    expect('{');
    Map<String, Object> result = new LinkedHashMap<>();
    skipWhitespace();
    if (peek() == '}') {
      position++;
      return result;
    }
    while (true) {
      skipWhitespace();
      String key = readString();
      skipWhitespace();
      expect(':');
      skipWhitespace();
      if (result.containsKey(key)) {
        throw error("duplicate key " + key);
      }
      result.put(key, readValue(depth));
      skipWhitespace();
      char next = next();
      if (next == '}') {
        return result;
      }
      if (next != ',') {
        throw error("expected , or }");
      }
    }
  }

  private List<Object> readArray(int depth) {
    ensureDepth(depth);
    expect('[');
    List<Object> result = new ArrayList<>();
    skipWhitespace();
    if (peek() == ']') {
      position++;
      return result;
    }
    while (true) {
      skipWhitespace();
      result.add(readValue(depth));
      skipWhitespace();
      char next = next();
      if (next == ']') {
        return result;
      }
      if (next != ',') {
        throw error("expected , or ]");
      }
    }
  }

  private Object readValue(int parentDepth) {
    char c = peek();
    if (c == '{') {
      return readObject(parentDepth + 1);
    }
    if (c == '[') {
      return readArray(parentDepth + 1);
    }
    if (c == '"') {
      return readString();
    }
    if (c == '-' || (c >= '0' && c <= '9')) {
      return readInteger();
    }
    if (text.startsWith("true", position)) {
      position += 4;
      return Boolean.TRUE;
    }
    if (text.startsWith("false", position)) {
      position += 5;
      return Boolean.FALSE;
    }
    if (text.startsWith("null", position)) {
      position += 4;
      return null;
    }
    throw error("unsupported value");
  }

  private void ensureDepth(int depth) {
    if (depth > MAX_DEPTH) {
      throw error("nesting exceeds " + MAX_DEPTH);
    }
  }

  private Long readInteger() {
    int start = position;
    if (peek() == '-') {
      position++;
    }
    while (position < text.length() && Character.isDigit(text.charAt(position))) {
      position++;
    }
    if (position < text.length() && ".eE".indexOf(text.charAt(position)) >= 0) {
      throw error("only integers are supported");
    }
    try {
      return Long.parseLong(text.substring(start, position));
    }
    catch (NumberFormatException e) {
      throw error("invalid integer");
    }
  }

  private String readString() {
    expect('"');
    StringBuilder out = new StringBuilder();
    while (true) {
      char c = next();
      if (c == '"') {
        return out.toString();
      }
      if (c < 0x20) {
        throw error("control character in string");
      }
      if (c != '\\') {
        out.append(c);
        continue;
      }
      char escaped = next();
      switch (escaped) {
        case '"', '\\', '/' -> out.append(escaped);
        case 'b' -> out.append('\b');
        case 'f' -> out.append('\f');
        case 'n' -> out.append('\n');
        case 'r' -> out.append('\r');
        case 't' -> out.append('\t');
        case 'u' -> {
          if (position + 4 > text.length()) {
            throw error("truncated unicode escape");
          }
          try {
            out.append((char)Integer.parseInt(text.substring(position, position + 4), 16));
          }
          catch (NumberFormatException e) {
            throw error("invalid unicode escape");
          }
          position += 4;
        }
        default -> throw error("invalid escape");
      }
    }
  }

  private static void writeValue(StringBuilder out, Object value, int depth) {
    if (value == null) {
      out.append("null");
      return;
    }
    if (value instanceof String string) {
      writeString(out, string);
      return;
    }
    if (value instanceof Boolean || value instanceof Integer || value instanceof Long) {
      out.append(value);
      return;
    }
    if (depth > MAX_DEPTH) {
      throw new IllegalArgumentException("JSON nesting exceeds " + MAX_DEPTH);
    }
    if (value instanceof Map<?, ?> map) {
      out.append('{');
      boolean first = true;
      for (Map.Entry<?, ?> entry : map.entrySet()) {
        if (!(entry.getKey() instanceof String key)) {
          throw new IllegalArgumentException("JSON object keys must be strings");
        }
        if (!first) {
          out.append(',');
        }
        first = false;
        writeString(out, key);
        out.append(':');
        writeValue(out, entry.getValue(), depth + 1);
      }
      out.append('}');
      return;
    }
    if (value instanceof List<?> list) {
      out.append('[');
      for (int i = 0; i < list.size(); i++) {
        if (i > 0) {
          out.append(',');
        }
        writeValue(out, list.get(i), depth + 1);
      }
      out.append(']');
      return;
    }
    if (value instanceof Object[] array) {
      out.append('[');
      for (int i = 0; i < array.length; i++) {
        if (i > 0) {
          out.append(',');
        }
        writeValue(out, array[i], depth + 1);
      }
      out.append(']');
      return;
    }
    throw new IllegalArgumentException("unsupported JSON value: " + value.getClass().getName());
  }

  private static void writeString(StringBuilder out, String value) {
    out.append('"');
    for (int i = 0; i < value.length(); i++) {
      char c = value.charAt(i);
      switch (c) {
        case '"' -> out.append("\\\"");
        case '\\' -> out.append("\\\\");
        case '\n' -> out.append("\\n");
        case '\r' -> out.append("\\r");
        case '\t' -> out.append("\\t");
        default -> {
          if (c < 0x20) {
            out.append(String.format("\\u%04x", (int)c));
          }
          else {
            out.append(c);
          }
        }
      }
    }
    out.append('"');
  }

  private void skipWhitespace() {
    while (position < text.length() && " \t\r\n".indexOf(text.charAt(position)) >= 0) {
      position++;
    }
  }

  private char peek() {
    if (position >= text.length()) {
      throw error("unexpected end");
    }
    return text.charAt(position);
  }

  private char next() {
    char c = peek();
    position++;
    return c;
  }

  private void expect(char expected) {
    if (next() != expected) {
      throw error("expected " + expected);
    }
  }

  private IllegalArgumentException error(String reason) {
    return new IllegalArgumentException("invalid worker JSON at " + position + ": " + reason);
  }
}
