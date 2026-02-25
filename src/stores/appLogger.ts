/**
 * Centralized application-wide log store.
 *
 * Captures errors, warnings, and info messages from all layers (UI, plugins,
 * git, network, etc.) into a bounded ring buffer. The ErrorLogPanel subscribes
 * to this store for display. Logs are also forwarded to the browser console.
 */

import { createSignal } from "solid-js";

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
  }

  function markSeen(): void {
    setUnseenErrorCount(0);
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
