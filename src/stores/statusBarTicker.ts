import { createSignal } from "solid-js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface TickerMessage {
  /** Unique message ID (scoped to plugin) */
  id: string;
  /** Plugin that posted the message */
  pluginId: string;
  /** Display text (short, ~40 chars max for status bar) */
  text: string;
  /** Optional inline SVG icon */
  icon?: string;
  /** Higher = more visible. Default 0. Messages >=80 get warning styling. */
  priority: number;
  /** Time-to-live in milliseconds (0 = persistent until removed) */
  ttlMs: number;
  /** Timestamp when the message was added */
  createdAt: number;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/** Rotation interval between same-priority messages */
const ROTATION_MS = 5_000;

/** Scavenge interval for expired messages */
const SCAVENGE_MS = 1_000;

function createStatusBarTicker() {
  const [messages, setMessages] = createSignal<TickerMessage[]>([]);
  const [rotationIndex, setRotationIndex] = createSignal(0);
  let scavengeTimer: ReturnType<typeof setInterval> | null = null;
  let rotationTimer: ReturnType<typeof setInterval> | null = null;

  /** Remove expired messages */
  function scavenge(): void {
    const now = Date.now();
    setMessages((prev) =>
      prev.filter((m) => m.ttlMs === 0 || now - m.createdAt < m.ttlMs),
    );
  }

  /** Advance rotation index */
  function rotate(): void {
    setRotationIndex((prev) => prev + 1);
  }

  function startTimers(): void {
    if (!scavengeTimer) {
      scavengeTimer = setInterval(scavenge, SCAVENGE_MS);
    }
    if (!rotationTimer) {
      rotationTimer = setInterval(rotate, ROTATION_MS);
    }
  }

  function stopTimers(): void {
    if (scavengeTimer) {
      clearInterval(scavengeTimer);
      scavengeTimer = null;
    }
    if (rotationTimer) {
      clearInterval(rotationTimer);
      rotationTimer = null;
    }
  }

  return {
    /**
     * Add or update a ticker message. If a message with the same id+pluginId
     * already exists, it is replaced (resets TTL).
     */
    addMessage(msg: Omit<TickerMessage, "createdAt">): void {
      const entry: TickerMessage = { ...msg, createdAt: Date.now() };
      setMessages((prev) => {
        const filtered = prev.filter(
          (m) => !(m.id === msg.id && m.pluginId === msg.pluginId),
        );
        return [...filtered, entry];
      });
      startTimers();
    },

    /** Remove a message by id and pluginId */
    removeMessage(id: string, pluginId: string): void {
      setMessages((prev) =>
        prev.filter((m) => !(m.id === id && m.pluginId === pluginId)),
      );
    },

    /** Remove all messages from a plugin */
    removeAllForPlugin(pluginId: string): void {
      setMessages((prev) => prev.filter((m) => m.pluginId !== pluginId));
    },

    /**
     * Get the current message to display. Returns the highest-priority
     * non-expired message, rotating among equal-priority messages.
     */
    getCurrentMessage(): TickerMessage | null {
      const now = Date.now();
      const active = messages().filter(
        (m) => m.ttlMs === 0 || now - m.createdAt < m.ttlMs,
      );
      if (active.length === 0) return null;

      // Sort by priority descending
      const sorted = [...active].sort((a, b) => b.priority - a.priority);
      const maxPriority = sorted[0].priority;

      // Get all messages at the highest priority level
      const topPriority = sorted.filter((m) => m.priority === maxPriority);

      // Rotate among top-priority messages
      const idx = rotationIndex() % topPriority.length;
      return topPriority[idx];
    },

    /** Get all active (non-expired) messages */
    getAll(): TickerMessage[] {
      const now = Date.now();
      return messages().filter(
        (m) => m.ttlMs === 0 || now - m.createdAt < m.ttlMs,
      );
    },

    /** Clear all messages and stop timers (for testing) */
    clear(): void {
      setMessages([]);
      setRotationIndex(0);
      stopTimers();
    },

    /** Manually trigger scavenge (for testing) */
    _scavenge: scavenge,

    /** Manually trigger rotation (for testing) */
    _rotate: rotate,
  };
}

export const statusBarTicker = createStatusBarTicker();
