/** Flow control watermarks (bytes pending in xterm.js write queue).
 *  When the write queue exceeds HIGH_WATERMARK, the PTY reader is paused
 *  to prevent unbounded memory growth. When it drains below LOW_WATERMARK,
 *  the reader resumes. */
export const HIGH_WATERMARK = 512 * 1024;  // 512KB
export const LOW_WATERMARK = 128 * 1024;   // 128KB

/** Encapsulates PTY backpressure state: tracks pending write bytes and
 *  decides when to pause/resume the PTY reader.
 *
 *  The caller (Terminal.tsx) is responsible for actually calling pty.pause()
 *  and pty.resume() — this class only manages the state machine. */
export class FlowController {
  isPaused = false;
  pendingBytes = 0;

  /** Record bytes queued for terminal.write(). */
  trackWrite(bytes: number): void {
    this.pendingBytes += bytes;
  }

  /** Record bytes drained (write callback fired). */
  trackDrain(bytes: number): void {
    this.pendingBytes -= bytes;
  }

  /** Check if the reader should be paused. Returns "pause" once when
   *  threshold is crossed, "none" otherwise (including if already paused). */
  checkPause(): "pause" | "none" {
    if (!this.isPaused && this.pendingBytes > HIGH_WATERMARK) {
      this.isPaused = true;
      return "pause";
    }
    return "none";
  }

  /** Check if the reader should be resumed. Returns "resume" once when
   *  pending drops below LOW_WATERMARK, "none" otherwise. */
  checkResume(): "resume" | "none" {
    if (this.isPaused && this.pendingBytes < LOW_WATERMARK) {
      this.isPaused = false;
      return "resume";
    }
    return "none";
  }

  /** Force resume without checking threshold (e.g. on cleanup or visibility change).
   *  Returns true if was paused (caller should call pty.resume). */
  forceResume(): boolean {
    if (this.isPaused) {
      this.isPaused = false;
      return true;
    }
    return false;
  }

  /** Reset all state (e.g. on visibility change where stale values are meaningless). */
  reset(): void {
    this.isPaused = false;
    this.pendingBytes = 0;
  }
}
