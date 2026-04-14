import { isWindows } from "../platform";

/** Send a command to a PTY session with split writes.
 *
 *  Splits into two writes:
 *  1. Ctrl-U + text (clears any existing input, then types the command)
 *  2. \r (Enter — sent separately)
 *
 *  The Ctrl-U prefix is required for Ink-based agents (Claude Code, Codex, etc.)
 *  which ignore Ctrl-U when bundled with text in raw mode, and is harmless for
 *  POSIX shells with readline (bash/zsh/fish) where it cancels any pending input.
 *
 *  On native Windows shells (cmd.exe, PowerShell) without a detected agent,
 *  Ctrl-U is not a line-kill control code and is echoed literally (e.g. "§cmd"
 *  or "^Ucmd"), breaking the command. We skip the prefix in that case.
 *
 *  @param writeFn - A function that writes raw data to the PTY (may include retry logic)
 *  @param text    - The command text to inject (without trailing newline)
 *  @param agentType - Agent detected in the PTY, if any (null/undefined = plain shell)
 */
export async function sendCommand(
  writeFn: (data: string) => Promise<void>,
  text: string,
  agentType?: string | null,
): Promise<void> {
  const prefix = isWindows() && !agentType ? "" : "\x15";
  await writeFn(prefix + text);
  await writeFn("\r");
}
