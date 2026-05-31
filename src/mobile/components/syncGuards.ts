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

/** Minimal end-anchored keystroke delta to turn `oldText` into `newText`.
 *
 *  Assumes the remote cursor is at the end of the line — true while we only
 *  append/backspace and the user hasn't moved the readline cursor via arrows.
 *  Keeps the longest common prefix, backspaces only the divergent tail of
 *  `oldText` from the end, then types the new tail. Append (no backspaces) and
 *  truncate (no retype) fall out as special cases.
 *
 *  This replaces a full-line nuke (`\x7f`×oldLen + newText) that fired on any
 *  mid-line edit: that burst of keystrokes flickered readline and corrupted the
 *  line when a write dropped/reordered over a laggy mobile link. The minimal
 *  delta sends the fewest keystrokes and stays correct as long as the cursor is
 *  at end-of-line. */
export function computeInputDelta(oldText: string, newText: string): string {
	const max = Math.min(oldText.length, newText.length);
	let prefix = 0;
	while (prefix < max && oldText[prefix] === newText[prefix]) prefix++;
	return "\x7f".repeat(oldText.length - prefix) + newText.slice(prefix);
}
