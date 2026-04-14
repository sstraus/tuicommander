// Pure dedup helpers for CommandInput sync. Extracted so they can be
// unit-tested directly instead of via mock simulators (review TEST-1).

/** Window after a send() during which incoming ptyInputLine updates must be
 *  ignored — prevents the cleared prompt from xterm from overwriting
 *  syncedText or bubbling into the textarea. */
export const SEND_GUARD_MS = 1000;

/** True while the post-send guard is still active. */
export function isSendGuardActive(now: number, lastSendAt: number): boolean {
  return now - lastSendAt < SEND_GUARD_MS;
}
