// PWA input sync helpers. The PWA textarea is the source of truth for user
// input; the PTY is a write-only sink that receives deltas. The ONLY case
// where we accept data back from the PTY into the textarea is a strict
// extension of what we've sent (tab completion / autocomplete) — everything
// else (prompt redraws, lagging echoes, history nav replacing typed text)
// is ignored.

/** Window after Enter during which all PTY input-line updates are ignored.
 *  Prevents a lagging echo of the just-sent command from flashing back into
 *  the (now cleared) textarea before the shell advances the prompt. */
export const POST_SEND_GUARD_MS = 500;

/** True while the post-send guard is still suppressing PTY echoes. */
export function isPostSendGuardActive(now: number, lastSendAt: number): boolean {
  return lastSendAt > 0 && now - lastSendAt < POST_SEND_GUARD_MS;
}

/** True if `echo` extends `syncedText` (tab completion, autocomplete). */
export function isSupersetEcho(echo: string, syncedText: string): boolean {
  return echo.length > syncedText.length && echo.startsWith(syncedText);
}
