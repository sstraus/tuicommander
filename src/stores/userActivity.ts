import { createSignal } from "solid-js";

const ACTIVITY_THROTTLE_MS = 500;

function createUserActivityStore() {
	const [lastActivityAt, setLastActivityAt] = createSignal(0);
	let lastRecordedAt = 0;

	let clickHandler: (() => void) | null = null;
	let keydownHandler: (() => void) | null = null;

	function recordActivity(): void {
		const now = Date.now();
		if (now - lastRecordedAt < ACTIVITY_THROTTLE_MS) return;
		lastRecordedAt = now;
		setLastActivityAt(now);
	}

	function msSinceLastActivity(): number {
		const last = lastActivityAt();
		if (last === 0) return Infinity;
		return Date.now() - last;
	}

	function startListening(): void {
		stopListening();
		clickHandler = recordActivity;
		keydownHandler = recordActivity;
		window.addEventListener("click", clickHandler, { passive: true });
		window.addEventListener("keydown", keydownHandler, { passive: true });
	}

	function stopListening(): void {
		if (clickHandler) {
			window.removeEventListener("click", clickHandler);
			clickHandler = null;
		}
		if (keydownHandler) {
			window.removeEventListener("keydown", keydownHandler);
			keydownHandler = null;
		}
	}

	function reset(): void {
		lastRecordedAt = 0;
		setLastActivityAt(0);
	}

	return {
		lastActivityAt,
		recordActivity,
		msSinceLastActivity,
		startListening,
		stopListening,
		reset,
	};
}

export const userActivityStore = createUserActivityStore();
