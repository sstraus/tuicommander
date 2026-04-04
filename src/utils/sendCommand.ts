/** Send a command to a PTY session with split writes.
 *
 *  Always splits into two writes:
 *  1. Ctrl-U + text (clears any existing input, then types the command)
 *  2. \r (Enter — sent separately)
 *
 *  The split is required for Ink-based agents (Claude Code, Codex, etc.)
 *  which ignore Ctrl-U when bundled with text in raw mode. Using split
 *  writes unconditionally is safe for shell sessions too and avoids
 *  echo bugs when agent detection fails.
 *
 *  @param writeFn - A function that writes raw data to the PTY (may include retry logic)
 *  @param text    - The command text to inject (without trailing newline)
 *  @param _agentType - Unused (kept for API compatibility)
 */
export async function sendCommand(
  writeFn: (data: string) => Promise<void>,
  text: string,
  _agentType?: string | null,
): Promise<void> {
  await writeFn("\x15" + text);
  await writeFn("\r");
}
