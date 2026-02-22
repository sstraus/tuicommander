/**
 * Per-plugin error/info logging with a bounded ring buffer.
 *
 * Each plugin gets its own PluginLogger instance. Logs are kept in memory
 * (no disk I/O) with a configurable max capacity. Oldest entries are
 * dropped when the buffer is full.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type LogLevel = "debug" | "info" | "warn" | "error";

export interface LogEntry {
  timestamp: number;
  level: LogLevel;
  message: string;
  data?: unknown;
}

// ---------------------------------------------------------------------------
// PluginLogger
// ---------------------------------------------------------------------------

const DEFAULT_CAPACITY = 500;

export class PluginLogger {
  private buffer: LogEntry[];
  private head = 0;
  private count = 0;
  readonly capacity: number;

  constructor(capacity = DEFAULT_CAPACITY) {
    this.capacity = capacity;
    this.buffer = new Array(capacity);
  }

  /** Append a log entry. Drops the oldest entry when the buffer is full. */
  log(level: LogLevel, message: string, data?: unknown): void {
    const entry: LogEntry = { timestamp: Date.now(), level, message, data };
    const index = (this.head + this.count) % this.capacity;

    if (this.count < this.capacity) {
      this.buffer[index] = entry;
      this.count++;
    } else {
      // Overwrite oldest
      this.buffer[this.head] = entry;
      this.head = (this.head + 1) % this.capacity;
    }
  }

  /** Convenience methods */
  debug(message: string, data?: unknown): void { this.log("debug", message, data); }
  info(message: string, data?: unknown): void { this.log("info", message, data); }
  warn(message: string, data?: unknown): void { this.log("warn", message, data); }
  error(message: string, data?: unknown): void { this.log("error", message, data); }

  /** Return a snapshot of all entries in chronological order (oldest first). */
  getEntries(): readonly LogEntry[] {
    const result: LogEntry[] = new Array(this.count);
    for (let i = 0; i < this.count; i++) {
      result[i] = this.buffer[(this.head + i) % this.capacity];
    }
    return result;
  }

  /** Number of entries currently in the buffer. */
  get size(): number {
    return this.count;
  }

  /** Number of error-level entries in the buffer. */
  get errorCount(): number {
    let n = 0;
    for (let i = 0; i < this.count; i++) {
      if (this.buffer[(this.head + i) % this.capacity].level === "error") n++;
    }
    return n;
  }

  /** Clear all entries. */
  clear(): void {
    this.head = 0;
    this.count = 0;
  }
}
