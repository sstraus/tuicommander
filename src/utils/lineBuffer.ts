/**
 * Reassembles streaming PTY chunks into complete lines.
 *
 * PTY data arrives in arbitrary-sized chunks that may split in the middle of
 * a line. LineBuffer accumulates input until a newline is found, then emits
 * the complete lines. The partial trailing line is retained for the next push.
 */
export class LineBuffer {
  private buffer = "";

  /**
   * Push a raw chunk of PTY output.
   * Returns all complete lines (split on \n) found in the chunk.
   * The partial trailing line is held internally until a newline arrives.
   */
  push(chunk: string): string[] {
    this.buffer += chunk;
    const parts = this.buffer.split("\n");
    // Last element is the incomplete trailing line (may be "" if chunk ended with \n)
    this.buffer = parts.pop() ?? "";
    // Strip trailing \r for Windows PTY output (\r\n line endings)
    return parts.map((line) => line.endsWith("\r") ? line.slice(0, -1) : line);
  }
}
