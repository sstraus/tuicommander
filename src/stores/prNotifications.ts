import { createStore } from "solid-js/store";

/** Notification types for PR state transitions */
export type PrNotificationType =
  | "merged"
  | "closed"
  | "blocked"
  | "ci_failed"
  | "changes_requested"
  | "ready";

export interface PrNotification {
  id: string;
  repoPath: string;
  branch: string;
  prNumber: number;
  title: string;
  type: PrNotificationType;
  createdAt: number;
  /** Cumulative milliseconds the app was focused since notification appeared */
  focusedTimeMs: number;
  dismissed: boolean;
}

const AUTO_DISMISS_MS = 5 * 60 * 1000; // 5 minutes of focused time

interface PrNotificationsState {
  notifications: PrNotification[];
}

function createPrNotificationsStore() {
  const [state, setState] = createStore<PrNotificationsState>({
    notifications: [],
  });

  let tickTimer: number | null = null;

  /** Start ticking focused time — stops automatically when no active notifications */
  function startFocusTimer(): void {
    if (tickTimer) return;
    tickTimer = window.setInterval(() => {
      const hasActive = state.notifications.some((n) => !n.dismissed);
      if (!hasActive) {
        // No active notifications — stop the timer to avoid idle CPU
        clearInterval(tickTimer!);
        tickTimer = null;
        return;
      }
      if (document.hasFocus()) {
        setState(
          "notifications",
          (n) => !n.dismissed,
          "focusedTimeMs",
          (ms) => ms + 1000,
        );
        // Auto-dismiss notifications that exceeded focus time
        setState(
          "notifications",
          (n) => !n.dismissed && n.focusedTimeMs >= AUTO_DISMISS_MS,
          "dismissed",
          true,
        );
      }
    }, 1000);
  }

  function stopFocusTimer(): void {
    if (tickTimer) {
      clearInterval(tickTimer);
      tickTimer = null;
    }
  }

  /** Add a new notification (deduplicates by pr+type) */
  function add(notification: Omit<PrNotification, "id" | "createdAt" | "focusedTimeMs" | "dismissed">): void {
    const id = `${notification.repoPath}:${notification.prNumber}:${notification.type}`;
    // Don't duplicate — if same notification already active, skip
    if (state.notifications.some((n) => n.id === id && !n.dismissed)) return;

    setState("notifications", (prev) => [
      ...prev.filter((n) => n.id !== id), // Remove old dismissed one if exists
      {
        ...notification,
        id,
        createdAt: Date.now(),
        focusedTimeMs: 0,
        dismissed: false,
      },
    ]);

    // Restart focus timer if it was stopped due to no active notifications
    if (!tickTimer) startFocusTimer();
  }

  /** Dismiss a single notification */
  function dismiss(id: string): void {
    setState("notifications", (n) => n.id === id, "dismissed", true);
  }

  /** Dismiss all active notifications */
  function dismissAll(): void {
    setState("notifications", (n) => !n.dismissed, "dismissed", true);
  }

  /** Get active (non-dismissed) notifications */
  function getActive(): PrNotification[] {
    return state.notifications.filter((n) => !n.dismissed);
  }

  /** Clear all notifications (for reset/testing) */
  function clearAll(): void {
    setState("notifications", []);
  }

  return {
    state,
    startFocusTimer,
    stopFocusTimer,
    add,
    dismiss,
    dismissAll,
    getActive,
    clearAll,
  };
}

export const prNotificationsStore = createPrNotificationsStore();
