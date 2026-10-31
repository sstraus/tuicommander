import { createStore } from "solid-js/store";
import { activityStore } from "./activityStore";

let shouldMirrorToBell = () => true;

/** Keep the core toast store independent of desktop notification settings. */
export function setToastBellMirrorResolver(resolver: () => boolean): void {
	shouldMirrorToBell = resolver;
}

export interface Toast {
	id: number;
	title: string;
	message: string;
	level: "info" | "warn" | "error";
	createdAt: number;
	/** Distinct messages grouped into this transient card. Bell entries stay separate. */
	count?: number;
	repoPath?: string;
	/** Backend session that raised this toast. Set for MCP `ui action=toast`,
	 *  where it is what makes the toast clickable: a repo holds many tabs, so
	 *  only the session id can name the terminal that actually spoke. */
	sessionId?: string;
	action?: { label: string; onClick: () => void };
	secondaryAction?: { label: string; onClick: () => void };
	/** Presentation group for dismissing visible and queued cards together. */
	dismissGroup?: string;
}

let nextId = 1;

const NOTICE_WINDOW_MS = 5000;

/** Auto-dismiss delay per level (ms). `info` is transient; `warn` lingers so
 *  actionable messages can be read; `error` is sticky (0 = never auto-dismiss)
 *  and stays until the user clicks it away. Callers can override per toast. */
export const DEFAULT_DURATION_MS: Record<Toast["level"], number> = {
	info: 20000,
	warn: 60000,
	error: 0,
};

/** Lazy-initialized AudioContext (created on first sound to satisfy autoplay policy) */
let audioCtx: AudioContext | null = null;

function getAudioCtx(): AudioContext {
	if (!audioCtx) audioCtx = new AudioContext();
	return audioCtx;
}

/**
 * Play a short synthesized notification sound via Web Audio API.
 *
 * - info:  single soft blip (880 Hz, 80ms)
 * - warn:  double beep (660 Hz, 80ms × 2 with 60ms gap)
 * - error: descending tone (440→220 Hz, 200ms)
 */
function playSound(level: "info" | "warn" | "error"): void {
	try {
		const ctx = getAudioCtx();
		const now = ctx.currentTime;

		if (level === "info") {
			const osc = ctx.createOscillator();
			const gain = ctx.createGain();
			osc.type = "sine";
			osc.frequency.setValueAtTime(880, now);
			gain.gain.setValueAtTime(0.15, now);
			gain.gain.exponentialRampToValueAtTime(0.001, now + 0.08);
			osc.connect(gain).connect(ctx.destination);
			osc.start(now);
			osc.stop(now + 0.08);
		} else if (level === "warn") {
			for (let i = 0; i < 2; i++) {
				const t = now + i * 0.14;
				const osc = ctx.createOscillator();
				const gain = ctx.createGain();
				osc.type = "sine";
				osc.frequency.setValueAtTime(660, t);
				gain.gain.setValueAtTime(0.15, t);
				gain.gain.exponentialRampToValueAtTime(0.001, t + 0.08);
				osc.connect(gain).connect(ctx.destination);
				osc.start(t);
				osc.stop(t + 0.08);
			}
		} else {
			// error: descending sweep
			const osc = ctx.createOscillator();
			const gain = ctx.createGain();
			osc.type = "sine";
			osc.frequency.setValueAtTime(440, now);
			osc.frequency.exponentialRampToValueAtTime(220, now + 0.2);
			gain.gain.setValueAtTime(0.18, now);
			gain.gain.exponentialRampToValueAtTime(0.001, now + 0.2);
			osc.connect(gain).connect(ctx.destination);
			osc.start(now);
			osc.stop(now + 0.2);
		}
	} catch {
		// AudioContext not available — skip silently
	}
}

/** Activity section that collects the mirrored toasts in the toolbar bell.
 *  Registered in App.tsx next to the other built-in sections. */
export const TOAST_ACTIVITY_SECTION_ID = "messages";

/** Bell icon per level. Compile-time constants — ActivityItem.icon is rendered
 *  via innerHTML and must never carry a runtime-built string. */
const LEVEL_ICONS: Record<Toast["level"], string> = {
	info: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm0 3.5a1 1 0 1 1 0 2 1 1 0 0 1 0-2zM6.75 7h1.5a.75.75 0 0 1 .75.75v3.5h.75a.75.75 0 0 1 0 1.5h-3a.75.75 0 0 1 0-1.5h.75V8.5h-.75a.75.75 0 0 1 0-1.5z"/></svg>',
	warn: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8.87 1.5a1 1 0 0 0-1.74 0L.36 13.25A1 1 0 0 0 1.23 14.75h13.54a1 1 0 0 0 .87-1.5zM8 5.25a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0V6a.75.75 0 0 1 .75-.75zm0 5.5a1 1 0 1 1 0 2 1 1 0 0 1 0-2z"/></svg>',
	error:
		'<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm3.36 10.3a.75.75 0 0 1-1.06 1.06L8 9.06l-2.3 2.3a.75.75 0 0 1-1.06-1.06L6.94 8 4.64 5.7a.75.75 0 0 1 1.06-1.06L8 6.94l2.3-2.3a.75.75 0 0 1 1.06 1.06L9.06 8l2.3 2.3z"/></svg>',
};

/** A toast auto-dismisses, often while the user is looking at another window, so
 *  the message is gone before it is read. Mirroring it into the bell keeps it
 *  readable afterwards. Opt out with the "Keep toasts in the bell" setting. */
function mirrorToBell(toast: Toast, force = false): void {
	if (!force && !shouldMirrorToBell()) return;
	activityStore.addItem({
		id: `toast-${toast.id}`,
		pluginId: "core",
		sectionId: TOAST_ACTIVITY_SECTION_ID,
		title: toast.title,
		subtitle: toast.message || undefined,
		icon: LEVEL_ICONS[toast.level],
		severity: toast.level,
		dismissible: true,
		onClick: toast.action?.onClick,
		repoPath: toast.repoPath,
	});
}

function createToastsStore() {
	const [state, setState] = createStore<{ toasts: Toast[] }>({ toasts: [] });
	const dismissTimers = new Map<number, ReturnType<typeof setTimeout>>();
	const durations = new Map<number, number>();
	const queued: Toast[] = [];
	let lastBellNotice: (Toast & { connectionId?: string }) | undefined;

	function clearDismissTimer(id: number): void {
		const timer = dismissTimers.get(id);
		if (timer !== undefined) clearTimeout(timer);
		dismissTimers.delete(id);
	}

	function armDismissTimer(id: number, remove: (id: number) => void): void {
		clearDismissTimer(id);
		const duration = durations.get(id) ?? 0;
		if (duration > 0)
			dismissTimers.set(
				id,
				setTimeout(() => remove(id), duration),
			);
	}

	return {
		get toasts() {
			return state.toasts;
		},

		/** Whether an identical toast is already visible. Duplicate backend events
		 * must not create a stack of copies, while distinct errors still stack. */
		hasVisible(title: string, message: string, level: Toast["level"], repoPath?: string): boolean {
			return state.toasts.some(
				(toast) =>
					toast.title === title && toast.message === message && toast.level === level && toast.repoPath === repoPath,
			);
		},

		/** Backend/agent events are retained without interrupting the active input. */
		addToBell(
			title: string,
			message = "",
			level: Toast["level"] = "info",
			repoPath?: string,
			action?: Toast["action"],
			sessionId?: string,
			connectionId?: string,
		) {
			if (
				lastBellNotice?.title === title &&
				lastBellNotice.message === message &&
				lastBellNotice.level === level &&
				lastBellNotice.repoPath === repoPath &&
				lastBellNotice.sessionId === sessionId &&
				lastBellNotice.connectionId === connectionId &&
				Date.now() - lastBellNotice.createdAt <= NOTICE_WINDOW_MS &&
				activityStore.getActive().some((item) => item.id === `toast-${lastBellNotice!.id}`)
			)
				return -1;
			const id = nextId++;
			lastBellNotice = { id, title, message, level, createdAt: Date.now(), repoPath, action, sessionId, connectionId };
			mirrorToBell(lastBellNotice, true);
			return id;
		},

		add(
			title: string,
			message = "",
			level: "info" | "warn" | "error" = "info",
			sound = false,
			action?: { label: string; onClick: () => void },
			durationMs?: number,
			repoPath?: string,
			sessionId?: string,
			mirrorInBell = true,
			options: Pick<Toast, "secondaryAction" | "dismissGroup"> = {},
		) {
			if (
				this.hasVisible(title, message, level, repoPath) ||
				queued.some(
					(item) =>
						item.title === title &&
						item.message === message &&
						item.level === level &&
						item.repoPath === repoPath &&
						item.sessionId === sessionId,
				)
			) {
				return -1;
			}
			const id = nextId++;
			const toast: Toast = {
				id,
				title,
				message,
				level,
				createdAt: Date.now(),
				action,
				repoPath,
				sessionId,
				...options,
			};
			const group = state.toasts.find(
				(item) =>
					item.title === title &&
					item.level === level &&
					item.repoPath === repoPath &&
					item.sessionId === sessionId &&
					item.dismissGroup === toast.dismissGroup &&
					toast.createdAt - item.createdAt <= NOTICE_WINDOW_MS,
			);
			const overflow = !group && state.toasts.length >= 2;
			if (mirrorInBell) mirrorToBell(toast, overflow);
			if (sound) playSound(level);
			durations.set(group?.id ?? id, durationMs ?? DEFAULT_DURATION_MS[level]);
			if (overflow) {
				// Errors take a slot ahead of informational cards. Domain-specific
				// cards cannot rely on the Messages bell, so retain them visibly or queued.
				const displaced = state.toasts.find((item) => item.level !== "error");
				if (displaced && (level === "error" || !mirrorInBell)) {
					clearDismissTimer(displaced.id);
					queued.push(displaced);
					setState("toasts", (items) => items.filter((item) => item.id !== displaced.id));
				} else {
					if (!mirrorInBell) queued.push(toast);
					else durations.delete(id);
					return id;
				}
			}
			if (group) {
				setState("toasts", (item) => item.id === group.id, {
					message,
					action,
					count: (group.count ?? 1) + 1,
				});
				armDismissTimer(group.id, (toastId) => this.remove(toastId));
				return group.id;
			}
			setState("toasts", (prev) => [...prev, toast]);
			armDismissTimer(id, (toastId) => this.remove(toastId));
			return id;
		},

		/** Remove queued cards first, so dismissing a visible card cannot reveal a muted one. */
		removeGroup(group: string) {
			const pending = queued.filter((toast) => toast.dismissGroup === group);
			for (const toast of pending) {
				clearDismissTimer(toast.id);
				durations.delete(toast.id);
			}
			queued.splice(0, queued.length, ...queued.filter((toast) => toast.dismissGroup !== group));
			for (const toast of state.toasts.filter((toast) => toast.dismissGroup === group)) this.remove(toast.id);
		},

		remove(id: number) {
			clearDismissTimer(id);
			durations.delete(id);
			const pendingIndex = queued.findIndex((item) => item.id === id);
			if (pendingIndex >= 0) queued.splice(pendingIndex, 1);
			setState("toasts", (prev) => prev.filter((t) => t.id !== id));
			if (state.toasts.length < 2 && queued.length > 0) {
				const next = queued.shift()!;
				setState("toasts", (prev) => [...prev, next]);
				armDismissTimer(next.id, (toastId) => this.remove(toastId));
			}
		},
	};
}

export const toastsStore = createToastsStore();
