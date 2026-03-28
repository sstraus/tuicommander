/** Send a command to a PTY session with agent-aware Enter handling.
 *
 *  Ink-based agents (Claude Code, Codex, etc.) run in raw mode — they need:
 *  1. Ctrl-U + text in one write (clears any existing input first)
 *  2. \r in a separate write (Ink swallows \r when bundled with text)
 *
 *  Shell sessions (cooked mode) handle Ctrl-U + text + \r in a single write.
 *
 *  @param writeFn - A function that writes raw data to the PTY (may include retry logic)
 *  @param text    - The command text to inject (without trailing newline)
 *  @param agentType - The detected agent type, or null/undefined for shell sessions
 */
export async function sendCommand(
  writeFn: (data: string) => Promise<void>,
  text: string,
  agentType?: string | null,
): Promise<void> {
  if (agentType) {
    await writeFn("\x15" + text);
    await writeFn("\r");
  } else {
    await writeFn("\x15" + text + "\r");
  }
}
