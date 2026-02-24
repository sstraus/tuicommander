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
  /** Human-readable source label shown before the text (e.g. "Usage") */
  label?: string;
  /** Optional inline SVG icon */
  icon?: string;
  /** Higher = more visible. Default 0. Messages >=80 get warning styling. */
  priority: number;
  /** Time-to-live in milliseconds (0 = persistent until removed) */
  ttlMs: number;
  /** Timestamp when the message was added */
  createdAt: number;
  /** Optional click handler (e.g. open dashboard panel) */
  onClick?: () => void;
}

/** Rotation state exposed to the TickerArea component */
export interface RotationState {
  /** Currently displayed message, or null if none */
  message: TickerMessage | null;
  /** 1-indexed position in the rotation list (0 when no messages) */
  current: number;
  /** Total messages in the rotation list (0 when no messages) */
  total: number;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Rotation interval between same-priority messages */
const ROTATION_MS = 5_000;

/** Scavenge interval for expired messages */
const SCAVENGE_MS = 1_000;

/** Pause duration after manual cycle (click-to-advance) */
const MANUAL_PAUSE_MS = 10_000;

/** Priority >= this value pins the message (no rotation) */
export const URGENT_PRIORITY = 100;

/** Priority < this value appears only in the popover, not in rotation */
export const LOW_PRIORITY_THRESHOLD = 10;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

function createStatusBarTicker() {
  const [messages, setMessages] = createSignal<TickerMessage[]>([]);
  const [rotationIndex, setRotationIndex] = createSignal(0);
  let scavengeTimer: ReturnType<typeof setInterval> | null = null;
  let rotationTimer: ReturnType<typeof setInterval> | null = null;
  let pauseTimer: ReturnType<typeof setTimeout> | null = null;

  /** Filter to active (non-expired) messages */
  function activeMessages(): TickerMessage[] {
    const now = Date.now();
    return messages().filter(
      (m) => m.ttlMs === 0 || now - m.createdAt < m.ttlMs,
    );
  }

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

  /** Stop rotation timer, restart after pause duration */
  function pauseRotation(): void {
    if (rotationTimer) {
      clearInterval(rotationTimer);
      rotationTimer = null;
    }
    if (pauseTimer) clearTimeout(pauseTimer);
    pauseTimer = setTimeout(() => {
      pauseTimer = null;
      if (messages().length > 0 && !rotationTimer) {
        rotationTimer = setInterval(rotate, ROTATION_MS);
      }
    }, MANUAL_PAUSE_MS);
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
     * @deprecated Use getRotationState() for richer display info.
     */
    getCurrentMessage(): TickerMessage | null {
      const active = activeMessages();
      if (active.length === 0) return null;

      const sorted = [...active].sort((a, b) => b.priority - a.priority);
      const maxPriority = sorted[0].priority;
      const topPriority = sorted.filter((m) => m.priority === maxPriority);
      const idx = rotationIndex() % topPriority.length;
      return topPriority[idx];
    },

    /**
     * Get rotation state for the TickerArea component.
     * Respects priority tiers: urgent pins, normal rotates, low is popover-only.
     */
    getRotationState(): RotationState {
      const active = activeMessages();
      if (active.length === 0) return { message: null, current: 0, total: 0 };

      // Separate by priority tier
      const urgent = active.filter((m) => m.priority >= URGENT_PRIORITY);
      const normal = active.filter(
        (m) => m.priority >= LOW_PRIORITY_THRESHOLD && m.priority < URGENT_PRIORITY,
      );

      // Urgent messages take precedence
      const pool = urgent.length > 0 ? urgent : normal;
      if (pool.length === 0) return { message: null, current: 0, total: 0 };

      // Sort by priority descending for stable ordering
      pool.sort((a, b) => b.priority - a.priority || a.createdAt - b.createdAt);

      const idx = rotationIndex() % pool.length;
      return {
        message: pool[idx],
        current: idx + 1,
        total: pool.length,
      };
    },

    /** Get all active (non-expired) messages sorted by priority desc (for popover) */
    getActiveMessages(): TickerMessage[] {
      return [...activeMessages()].sort(
        (a, b) => b.priority - a.priority || a.createdAt - b.createdAt,
      );
    },

    /** Get all active (non-expired) messages */
    getAll(): TickerMessage[] {
      return activeMessages();
    },

    /** Advance to next message manually and pause auto-rotation for 10s */
    advanceManually(): void {
      setRotationIndex((prev) => prev + 1);
      pauseRotation();
    },

    /** Clear all messages and stop timers (for testing) */
    clear(): void {
      setMessages([]);
      setRotationIndex(0);
      stopTimers();
      if (pauseTimer) {
        clearTimeout(pauseTimer);
        pauseTimer = null;
      }
    },

    /** Manually trigger scavenge (for testing) */
    _scavenge: scavenge,

    /** Manually trigger rotation (for testing) */
    _rotate: rotate,
  };
}

export const statusBarTicker = createStatusBarTicker();
