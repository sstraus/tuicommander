// Pure dedup helpers for CommandInput sync. Extracted so they can be
// unit-tested directly instead of via mock simulators (review TEST-1).

/** Window after a send() during which incoming ptyInputLine updates must be
 *  ignored — prevents the cleared prompt from xterm from overwriting
 *  syncedText or bubbling into the textarea. */
export const SEND_GUARD_MS = 1000;

/** Grace period after pendingWrites drops to 0. During this window, only
 *  superset echoes (tab completion) are accepted immediately — anything else
 *  is display-only until the window expires. Covers the gap between "RPC
 *  resolved" and "echo arrived via WebSocket". */
export const WRITE_GRACE_MS = 300;

/** True while the post-send guard is still active. */
export function isSendGuardActive(now: number, lastSendAt: number): boolean {
  return now - lastSendAt < SEND_GUARD_MS;
}

/** True if `echo` is a strict prefix of `syncedText` — a definitive stale
 *  echo that must always be rejected regardless of timing. */
export function isStalePrefix(echo: string, syncedText: string): boolean {
  return echo.length < syncedText.length && syncedText.startsWith(echo);
}

/** True if `echo` extends `syncedText` (tab completion, autocomplete). */
export function isSupersetEcho(echo: string, syncedText: string): boolean {
  return echo.length > syncedText.length && echo.startsWith(syncedText);
}

/** True while the post-write grace window is active. */
export function isWriteGraceActive(now: number, lastWriteSettledAt: number): boolean {
  return lastWriteSettledAt > 0 && now - lastWriteSettledAt < WRITE_GRACE_MS;
}

export type EchoVerdict = "accept" | "display-only" | "reject";

/** Classify an incoming ptyInputLine echo against current sync state.
 *  - "accept": update syncedText + display
 *  - "display-only": update display but keep syncedText intact
 *  - "reject": ignore entirely (stale prefix, always wrong) */
export function classifyEcho(
  echo: string,
  syncedText: string,
  now: number,
  lastWriteSettledAt: number,
): EchoVerdict {
  // Stale prefix — always reject, zero false positives
  if (isStalePrefix(echo, syncedText)) return "reject";

  // Superset (tab completion) — always accept immediately
  if (isSupersetEcho(echo, syncedText)) return "accept";

  // Within grace window after writes settled — display-only for anything
  // that isn't a superset (already handled above)
  if (isWriteGraceActive(now, lastWriteSettledAt)) return "display-only";

  // Grace expired, not a prefix, not a superset — terminal-driven change
  // (history nav, Ctrl+U, agent insert). Accept it.
  return "accept";
}
