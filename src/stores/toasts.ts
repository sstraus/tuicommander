import { createStore } from "solid-js/store";

export interface Toast {
	id: number;
	title: string;
	message: string;
	level: "info" | "warn" | "error";
	createdAt: number;
	action?: { label: string; onClick: () => void };
}

let nextId = 1;

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

function createToastsStore() {
	const [state, setState] = createStore<{ toasts: Toast[] }>({ toasts: [] });
	const dismissTimers = new Map<number, ReturnType<typeof setTimeout>>();

	return {
		get toasts() {
			return state.toasts;
		},

		add(
			title: string,
			message = "",
			level: "info" | "warn" | "error" = "info",
			sound = false,
			action?: { label: string; onClick: () => void },
		) {
			const id = nextId++;
			const toast: Toast = { id, title, message, level, createdAt: Date.now(), action };
			setState("toasts", (prev) => [...prev, toast]);
			if (sound) playSound(level);
			dismissTimers.set(
				id,
				setTimeout(() => this.remove(id), 4000),
			);
			return id;
		},

		remove(id: number) {
			const timer = dismissTimers.get(id);
			if (timer !== undefined) {
				clearTimeout(timer);
				dismissTimers.delete(id);
			}
			setState("toasts", (prev) => prev.filter((t) => t.id !== id));
		},
	};
}

export const toastsStore = createToastsStore();
