import { createStore } from "solid-js/store";

/** User-facing PR notification types — every one MUST have a label/icon/cls in
 *  Toolbar's NOTIFICATION_LABELS. This list is the single source of truth: the
 *  union below derives from it, and `isNotificationType` gates which incoming
 *  transitions become notifications. */
export const PR_NOTIFICATION_TYPES = [
	"merged",
	"closed",
	"blocked",
	"ci_failed",
	"ci_recovered",
	"changes_requested",
	"ready",
	"review_started",
] as const;

/** Notification types for PR state transitions */
export type PrNotificationType = (typeof PR_NOTIFICATION_TYPES)[number];

/** True when a transition type is a renderable notification. The Rust poller also
 *  emits watcher-only transitions (`pushed`, `opened`) that have no label/icon and
 *  must NOT be added as notifications — doing so crashes the popover render
 *  (NOTIFICATION_LABELS[type] → undefined → undefined.cls). Allowlist, so any future
 *  watcher-only transition is ignored by default rather than crashing. */
export function isNotificationType(type: string): type is PrNotificationType {
	return (PR_NOTIFICATION_TYPES as readonly string[]).includes(type);
}

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
				setState("notifications", (n) => !n.dismissed && n.focusedTimeMs >= AUTO_DISMISS_MS, "dismissed", true);
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

		setState("notifications", (prev) => {
			const filtered = prev.filter((n) => n.id !== id);
			const next = [
				...filtered,
				{
					...notification,
					id,
					createdAt: Date.now(),
					focusedTimeMs: 0,
					dismissed: false,
				},
			];
			if (next.length > 200) {
				const dismissed = next.filter((n) => n.dismissed);
				const active = next.filter((n) => !n.dismissed);
				return [...dismissed.slice(-50), ...active];
			}
			return next;
		});

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
		_testCancelPendingTimers(): void {
			stopFocusTimer();
		},
	};
}

export const prNotificationsStore = createPrNotificationsStore();
