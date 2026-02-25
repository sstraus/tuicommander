/**
 * Centralized application-wide log store.
 *
 * Captures errors, warnings, and info messages from all layers (UI, plugins,
 * git, network, etc.) into a bounded ring buffer. The ErrorLogPanel subscribes
 * to this store for display. Logs are also forwarded to the browser console.
 *
 * The local JS ring buffer serves as a fast reactive cache for the UI.
 * Every push is also fire-and-forget mirrored to the Rust backend ring buffer
 * via `push_log`, making logs durable across webview reloads.
 */

import { createSignal } from "solid-js";
import { rpc } from "../transport";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type AppLogLevel = "debug" | "info" | "warn" | "error";

export type AppLogSource =
  | "app"
  | "plugin"
  | "git"
  | "network"
  | "terminal"
  | "github"
  | "dictation"
  | "store"
  | "config";

export interface AppLogEntry {
  id: number;
  timestamp: number;
  level: AppLogLevel;
  source: AppLogSource;
  message: string;
  data?: unknown;
}

/** Shape returned by the Rust get_logs command */
interface RustLogEntry {
  id: number;
  timestamp_ms: number;
  level: string;
  source: string;
  message: string;
  data_json?: string | null;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

const MAX_ENTRIES = 1000;

let nextId = 1;

function createAppLogger() {
  const buffer: AppLogEntry[] = [];
  let head = 0;
  let count = 0;

  // Reactive signal that bumps every time a log entry is added.
  // Components subscribe to this to re-render when new entries arrive.
  const [revision, setRevision] = createSignal(0);

  // Count of errors since last time the panel was opened
  const [unseenErrorCount, setUnseenErrorCount] = createSignal(0);

  // Track whether the Rust backend is reachable.
  // Entries pushed before backend is ready are queued and drained on first success.
  let backendReady = false;
  const pendingQueue: Array<{ level: string; source: string; message: string; dataJson?: string }> = [];

  /** Fire-and-forget push to Rust backend. Queues if not ready yet. */
  function pushToRust(level: string, source: string, message: string, dataJson?: string): void {
    if (!backendReady) {
      pendingQueue.push({ level, source, message, dataJson });
      // Attempt to drain — if it succeeds, backendReady flips true
      drainQueue();
      return;
    }
    rpc("push_log", { level, source, message, dataJson: dataJson ?? null }).catch(() => {
      // Silently ignore — the local buffer already has the entry
    });
  }

  /** Try to send queued entries to Rust. On first success, mark backend as ready. */
  function drainQueue(): void {
    if (pendingQueue.length === 0) return;
    const entry = pendingQueue[0];
    rpc("push_log", {
      level: entry.level,
      source: entry.source,
      message: entry.message,
      dataJson: entry.dataJson ?? null,
    })
      .then(() => {
        backendReady = true;
        pendingQueue.shift();
        // Drain remaining entries
        for (const queued of pendingQueue.splice(0)) {
          rpc("push_log", {
            level: queued.level,
            source: queued.source,
            message: queued.message,
            dataJson: queued.dataJson ?? null,
          }).catch(() => {});
        }
      })
      .catch(() => {
        // Backend not ready yet — entries stay queued
      });
  }

  function push(level: AppLogLevel, source: AppLogSource, message: string, data?: unknown): void {
    const entry: AppLogEntry = {
      id: nextId++,
      timestamp: Date.now(),
      level,
      source,
      message,
      data,
    };

    if (count < MAX_ENTRIES) {
      buffer[count] = entry;
      count++;
    } else {
      buffer[head] = entry;
      head = (head + 1) % MAX_ENTRIES;
    }

    if (level === "error") {
      setUnseenErrorCount((c) => c + 1);
    }

    setRevision((r) => r + 1);

    // Forward to browser console
    const tag = `[${source}]`;
    switch (level) {
      case "error": console.error(tag, message, data !== undefined ? data : ""); break;
      case "warn": console.warn(tag, message, data !== undefined ? data : ""); break;
      case "info": console.info(tag, message, data !== undefined ? data : ""); break;
      case "debug": console.debug(tag, message, data !== undefined ? data : ""); break;
    }

    // Mirror to Rust backend (fire-and-forget)
    const dataJson = data !== undefined ? JSON.stringify(data) : undefined;
    pushToRust(level, source, message, dataJson);
  }

  function getEntries(): readonly AppLogEntry[] {
    // Force reactive subscription
    revision();
    const result: AppLogEntry[] = new Array(count);
    for (let i = 0; i < count; i++) {
      result[i] = buffer[(head + i) % MAX_ENTRIES];
    }
    return result;
  }

  function clear(): void {
    head = 0;
    count = 0;
    setRevision((r) => r + 1);
    setUnseenErrorCount(0);

    // Clear Rust backend too
    rpc("clear_logs").catch(() => {});
  }

  function markSeen(): void {
    setUnseenErrorCount(0);
  }

  /**
   * Hydrate the local buffer from the Rust backend.
   * Called on webview reload to recover logs from the previous session.
   * Merges Rust entries that aren't already in the local buffer.
   */
  async function hydrateFromRust(): Promise<void> {
    try {
      const rustEntries = await rpc<RustLogEntry[]>("get_logs", { limit: 0 });
      if (!rustEntries || rustEntries.length === 0) return;

      backendReady = true;

      // Build a set of existing entry IDs for dedup
      const existingIds = new Set<number>();
      for (let i = 0; i < count; i++) {
        existingIds.add(buffer[(head + i) % MAX_ENTRIES].id);
      }

      let added = 0;
      for (const re of rustEntries) {
        if (existingIds.has(re.id)) continue;

        let data: unknown;
        if (re.data_json) {
          try { data = JSON.parse(re.data_json); } catch { data = re.data_json; }
        }

        const entry: AppLogEntry = {
          id: re.id,
          timestamp: re.timestamp_ms,
          level: re.level as AppLogLevel,
          source: re.source as AppLogSource,
          message: re.message,
          data,
        };

        if (count < MAX_ENTRIES) {
          buffer[count] = entry;
          count++;
        } else {
          buffer[head] = entry;
          head = (head + 1) % MAX_ENTRIES;
        }

        if (re.level === "error") {
          setUnseenErrorCount((c) => c + 1);
        }
        added++;
      }

      if (added > 0) {
        // Advance nextId past the highest Rust ID
        const maxRustId = rustEntries.reduce((max, e) => Math.max(max, e.id), 0);
        if (maxRustId >= nextId) {
          nextId = maxRustId + 1;
        }
        setRevision((r) => r + 1);
      }
    } catch {
      // Hydration failed — not critical, local buffer still works
    }
  }

  return {
    // Convenience loggers by level
    error(source: AppLogSource, message: string, data?: unknown): void {
      push("error", source, message, data);
    },
    warn(source: AppLogSource, message: string, data?: unknown): void {
      push("warn", source, message, data);
    },
    info(source: AppLogSource, message: string, data?: unknown): void {
      push("info", source, message, data);
    },
    debug(source: AppLogSource, message: string, data?: unknown): void {
      push("debug", source, message, data);
    },

    /** Raw push for programmatic use */
    push,

    /** All entries in chronological order (oldest first). Reactive. */
    getEntries,

    /** Remove all entries */
    clear,

    /** Reset unseen error count (called when panel opens) */
    markSeen,

    /** Hydrate local buffer from Rust backend (call on webview reload) */
    hydrateFromRust,

    /** Number of error entries since last markSeen(). Reactive. */
    unseenErrorCount,

    /** Total entry count. Reactive. */
    entryCount(): number {
      revision();
      return count;
    },
  };
}

export const appLogger = createAppLogger();
