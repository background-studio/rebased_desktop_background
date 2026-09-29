package studio.background.rebased.bridge;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;

/**
 * Newline-delimited UTF-8 over a synchronous Windows pipe handle. Reads and writes must stay on
 * one thread: a pending synchronous read blocks writes on the same handle.
 */
final class LineChannel {
  static final int MAX_LINE_BYTES = 256 * 1024;

  private final RandomAccessFile pipe;
  private final byte[] buffer = new byte[8192];
  private int bufferStart;
  private int bufferEnd;

  LineChannel(RandomAccessFile pipe) {
    this.pipe = pipe;
  }

  String readLine() throws IOException {
    ByteArrayOutputStream line = new ByteArrayOutputStream();
    while (true) {
      if (bufferStart == bufferEnd) {
        int read = pipe.read(buffer);
        if (read < 0) {
          return null;
        }
        bufferStart = 0;
        bufferEnd = read;
        continue;
      }
      byte next = buffer[bufferStart++];
      if (next == '\n') {
        return decode(line.toByteArray());
      }
      if (line.size() >= MAX_LINE_BYTES) {
        throw new IOException("worker line exceeds " + MAX_LINE_BYTES + " bytes");
      }
      line.write(next);
    }
  }

  void writeLine(String json) throws IOException {
    byte[] bytes = (json + "\n").getBytes(StandardCharsets.UTF_8);
    if (bytes.length > MAX_LINE_BYTES) {
      throw new IOException("bridge line exceeds " + MAX_LINE_BYTES + " bytes");
    }
    pipe.write(bytes);
  }

  private static String decode(byte[] bytes) throws IOException {
    int length = bytes.length;
    if (length > 0 && bytes[length - 1] == '\r') {
      length--;
    }
    try {
      return StandardCharsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes, 0, length))
        .toString();
    }
    catch (CharacterCodingException e) {
      throw new IOException("worker line is not valid UTF-8", e);
    }
  }
}
