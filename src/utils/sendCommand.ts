import { rpc } from "../transport";
import { isWindows } from "../platform";
import { appLogger } from "../stores/appLogger";

/** Shell family classification from the Rust PTY layer.
 *  Serialized as kebab-case to match `serde(rename_all = "kebab-case")`. */
export type ShellFamily = "posix" | "windows-native" | "unknown";

/** Per-session cache of the resolved shell family. Queried once on first
 *  use and reused afterwards — the shell doesn't change mid-session. */
const shellFamilyCache = new Map<string, ShellFamily>();

/** Fetch (and cache) the shell family for a PTY session. Returns "unknown"
 *  if the backend can't tell us — `sendCommand` then falls back to the
 *  platform heuristic. */
export async function getShellFamily(sessionId: string): Promise<ShellFamily> {
  const cached = shellFamilyCache.get(sessionId);
  if (cached) return cached;
  try {
    const family = await rpc<ShellFamily | null>("get_session_shell_family", { sessionId });
    const resolved: ShellFamily = family ?? "unknown";
    shellFamilyCache.set(sessionId, resolved);
    return resolved;
  } catch (err) {
    appLogger.warn("terminal", "Failed to query shell family; falling back to platform heuristic", err);
    return "unknown";
  }
}

/** Drop a session's cached entry. Call on session close so a reused
 *  session id (unlikely but possible) doesn't keep a stale classification. */
export function clearShellFamilyCache(sessionId: string): void {
  shellFamilyCache.delete(sessionId);
}

/** Send a command to a PTY session with split writes.
 *
 *  Splits into two writes:
 *  1. Ctrl-U + text (clears any existing input, then types the command)
 *  2. \r (Enter — sent separately)
 *
 *  The Ctrl-U prefix is required for Ink-based agents (Claude Code, Codex, etc.)
 *  which ignore Ctrl-U when bundled with text in raw mode, and is desirable for
 *  POSIX shells with readline (bash/zsh/fish) where it cancels any pending input.
 *
 *  On native Windows shells (cmd.exe, PowerShell) without a detected agent,
 *  Ctrl-U is not a line-kill control code and is echoed literally (e.g. "§cmd"
 *  or "^Ucmd"), breaking the command. We skip the prefix in that case.
 *
 *  Critical: git-bash on Windows runs bash/readline — same needs as a POSIX
 *  shell on Linux. The `shellFamily` argument resolves the ambiguity; when
 *  omitted we fall back to `isWindows()` (safe for cmd/PowerShell, wrong for
 *  git-bash — callers should provide shellFamily whenever possible).
 *
 *  @param writeFn      Function that writes raw data to the PTY.
 *  @param text         Command text to inject (without trailing newline).
 *  @param agentType    Detected agent in the PTY (null = plain shell).
 *  @param shellFamily  Classification of the session's underlying shell.
 */
export async function sendCommand(
  writeFn: (data: string) => Promise<void>,
  text: string,
  agentType?: string | null,
  shellFamily?: ShellFamily,
): Promise<void> {
  const skipPrefix = !agentType && isWindowsNative(shellFamily);
  const prefix = skipPrefix ? "" : "\x15";
  const payload = text.includes("\n")
    ? `\x1b[200~${text}\x1b[201~`
    : text;
  await writeFn(prefix + payload);
  await writeFn("\r");
}

/** Send a single raw character/escape sequence to a PTY running a TUI dialog
 *  in raw stdin mode (Claude Code edit-confirm, bash-confirm, apply-patch, ...).
 *
 *  Unlike `sendCommand`, this writes EXACTLY the bytes provided — no Ctrl-U
 *  prefix, no trailing `\r`. Adding either breaks raw-mode dialog parsers:
 *  Claude Code reads one key and interprets trailing bytes as the next prompt,
 *  Codex aborts on unexpected input. This is the intended counterpart to
 *  `sendCommand` for the ChoicePrompt / numbered-option path.
 *
 *  Intentionally a one-liner over `writeFn` so the call site is centralized
 *  (grep-able, uniform logging) and the AGENTS.md "never raw text+\r" rule
 *  still routes through a named helper even for single-key writes.
 */
export async function sendPtyKey(
  writeFn: (data: string) => Promise<void>,
  key: string,
): Promise<void> {
  await writeFn(key);
}

/** True when the session runs a native Windows shell (cmd / PowerShell).
 *  POSIX shells (incl. git-bash on Windows) return false so they still
 *  receive the Ctrl-U prefix.
 *  When shellFamily is omitted/unknown, fall back to the platform heuristic. */
function isWindowsNative(shellFamily: ShellFamily | undefined): boolean {
  if (shellFamily === "windows-native") return true;
  if (shellFamily === "posix") return false;
  return isWindows();
}
