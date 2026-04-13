// Pure dedup helpers for CommandInput sync. Extracted so they can be
// unit-tested directly instead of via mock simulators (review TEST-1).

/** Window after a send() during which incoming ptyInputLine updates must be
 *  ignored — prevents the cleared prompt from xterm from overwriting
 *  syncedText or bubbling into the textarea. */
export const SEND_GUARD_MS = 1000;

/** Window after any write to PTY during which incoming text is treated as
 *  an echo of our own write rather than a terminal-driven update. */
export const ECHO_WINDOW_MS = 500;

/** True while the post-send guard is still active. */
export function isSendGuardActive(now: number, lastSendAt: number): boolean {
  return now - lastSendAt < SEND_GUARD_MS;
}

/** True while the echo window is still open (incoming text likely our own echo). */
export function isWithinEchoWindow(now: number, lastWriteAt: number): boolean {
  return now - lastWriteAt < ECHO_WINDOW_MS;
}
