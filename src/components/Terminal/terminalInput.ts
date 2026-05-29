// Terminal keyboard input → escape sequence mapping.
// Pure function: KeyboardEvent → string (to send to PTY) or null (don't handle).

const ARROW_SUFFIX: Record<string, string> = {
	ArrowUp: "A",
	ArrowDown: "B",
	ArrowRight: "C",
	ArrowLeft: "D",
};

const F_KEYS: Record<string, string> = {
	F1: "\x1bOP",
	F2: "\x1bOQ",
	F3: "\x1bOR",
	F4: "\x1bOS",
	F5: "\x1b[15~",
	F6: "\x1b[17~",
	F7: "\x1b[18~",
	F8: "\x1b[19~",
	F9: "\x1b[20~",
	F10: "\x1b[21~",
	F11: "\x1b[23~",
	F12: "\x1b[24~",
};

const NAV_KEYS: Record<string, string> = {
	Home: "\x1b[H",
	End: "\x1b[F",
	Insert: "\x1b[2~",
	Delete: "\x1b[3~",
	PageUp: "\x1b[5~",
	PageDown: "\x1b[6~",
};

const IGNORED_KEYS = new Set([
	"Shift",
	"Control",
	"Alt",
	"Meta",
	"CapsLock",
	"NumLock",
	"ScrollLock",
	"Hyper",
	"Super",
	"ContextMenu",
	"OS",
]);

function modifierParam(e: KeyboardEvent): number {
	return 1 + (e.shiftKey ? 1 : 0) + (e.altKey ? 2 : 0) + (e.ctrlKey ? 4 : 0);
}

/**
 * Convert a KeyboardEvent to the terminal escape sequence string to send to the PTY.
 * Returns null if the key should not be handled (modifier-only, Meta/Cmd).
 */
export function keyToSequence(e: KeyboardEvent): string | null {
	if (e.metaKey) return null;
	if (IGNORED_KEYS.has(e.key)) return null;

	// Arrow keys
	const arrowSuffix = ARROW_SUFFIX[e.key];
	if (arrowSuffix) {
		const mod = modifierParam(e);
		return mod > 1 ? `\x1b[1;${mod}${arrowSuffix}` : `\x1b[${arrowSuffix}`;
	}

	// Function keys
	const fKey = F_KEYS[e.key];
	if (fKey) return fKey;

	// Navigation keys
	const navKey = NAV_KEYS[e.key];
	if (navKey) return navKey;

	// Simple named keys
	switch (e.key) {
		case "Enter":
			return "\r";
		case "Tab":
			return e.shiftKey ? "\x1b[Z" : "\t";
		case "Backspace":
			return "\x7f";
		case "Escape":
			return "\x1b";
	}

	// Ctrl+key → control characters
	if (e.ctrlKey && !e.altKey && e.key.length === 1) {
		const lower = e.key.toLowerCase();
		const code = lower.charCodeAt(0);
		if (code >= 0x61 && code <= 0x7a) {
			return String.fromCharCode(code - 0x60);
		}
		const ctrlPunct: Record<string, number> = {
			"@": 0x00,
			"[": 0x1b,
			"\\": 0x1c,
			"]": 0x1d,
			"^": 0x1e,
			_: 0x1f,
		};
		if (e.key in ctrlPunct) {
			return String.fromCharCode(ctrlPunct[e.key]);
		}
	}

	// Alt+letter → ESC + char (use e.code to avoid macOS dead-key characters)
	if (e.altKey && !e.ctrlKey) {
		const seq = altSequenceFromCode(e);
		if (seq) return seq;
		if (e.key.length === 1) return `\x1b${e.key}`;
	}

	// Printable single character
	if (e.key.length === 1) {
		return e.key;
	}

	return null;
}

/**
 * How long after `compositionend` a matching trailing keydown is treated as the
 * WKWebView duplicate. Kept short on purpose: the duplicate arrives within ~1ms,
 * so a small window absorbs it without swallowing a real base-letter keystroke a
 * human types later (e.g. the second `e` in "vêem"). Platform-agnostic — engines
 * that don't emit the duplicate simply never see a matching keydown in time.
 */
export const DUP_KEYDOWN_WINDOW_MS = 50;

/**
 * State machine for dead-key / IME composition through a hidden <input>.
 *
 * Canvas elements in WKWebView don't participate in macOS text input, so dead
 * keys (quotes, accents, ç, ã …) fail when listeners live on the canvas.
 * Routing input through a real <input> fixes composition.
 *
 * Two WKWebView quirks this handles:
 * 1. `compositionend` may fire with empty data → we return null (no write).
 * 2. WKWebView fires a spurious keydown for the resolution key right after
 *    compositionend (e.g. `´` + `e` → `é` but also a trailing `e` keydown).
 *    `shouldSuppressKeydown` eats that duplicate.
 *
 * The duplicate fires within a couple of ms of `compositionend` and carries the
 * composed character's *base* letter (`é` → `e`, `ç` → `c`). We therefore eat a
 * non-composing keydown only when it lands inside `DUP_KEYDOWN_WINDOW_MS` AND its
 * key matches that base letter. A timer-based one-shot was previously used but
 * raced: a `setTimeout(0)` reset could fire before the (also async) duplicate
 * keydown, letting it leak through as a doubled character ("ée", "çc"). Matching
 * on the base letter + time window removes the ordering dependency and avoids
 * eating an unrelated fast keystroke after an accent.
 *
 * `now` defaults to `performance.now` — injectable for tests.
 */
export function createCompositionState(now: () => number = () => performance.now()): {
	onCompositionEnd(data: string | null | undefined): string | null;
	shouldSuppressKeydown(isComposing: boolean, key?: string): boolean;
} {
	let suppressBase = "";
	let suppressUntil = 0;
	return {
		onCompositionEnd(data) {
			if (!data) {
				// Cancelled/empty composition: disarm any window still armed from a
				// prior composition so it can't eat a later matching keystroke.
				suppressBase = "";
				suppressUntil = 0;
				return null;
			}
			// The first NFD code point is the base letter the trailing keydown
			// reports ("e-acute" -> "e", cedilla -> "c"). Array.from is code-point
			// aware, so an astral-plane composition yields the whole character (not a
			// half-surrogate); such a base never matches a single-char key below.
			suppressBase = (Array.from(data.normalize("NFD"))[0] ?? "").toLowerCase();
			suppressUntil = now() + DUP_KEYDOWN_WINDOW_MS;
			return data;
		},
		shouldSuppressKeydown(isComposing, key) {
			if (isComposing) return true;
			if (
				suppressBase !== "" &&
				now() <= suppressUntil &&
				key !== undefined &&
				key.length === 1 &&
				key.toLowerCase() === suppressBase
			) {
				suppressBase = "";
				suppressUntil = 0;
				return true;
			}
			return false;
		},
	};
}

/**
 * macOS Alt/Option key handling via e.code.
 * On macOS, Alt+letter produces dead-key characters in e.key (e.g. π for Alt+P).
 * Terminal emulators need ESC + base letter instead.
 * Also handles Alt+punctuation for shell keybindings (Alt+., Alt+/, etc.).
 */
export function altSequenceFromCode(e: KeyboardEvent): string | null {
	const code = e.code;
	if (!code) return null;

	if (code.startsWith("Key")) {
		const ch = code.slice(3).toLowerCase();
		return "\x1b" + (e.shiftKey ? ch.toUpperCase() : ch);
	}
	if (code.startsWith("Digit")) {
		return "\x1b" + code.slice(5);
	}

	switch (code) {
		case "Backspace":
			return "\x1b\x7f"; // Alt+Backspace = backward-kill-word
		case "Space":
			return "\x1b ";
		case "Period":
			return "\x1b."; // Alt+. = insert-last-argument
		case "Comma":
			return "\x1b,";
		case "Slash":
			return "\x1b/";
		case "Minus":
			return "\x1b-";
		case "Equal":
			return "\x1b=";
		case "Semicolon":
			return "\x1b;";
		case "Quote":
			return "\x1b'";
		case "BracketLeft":
			return "\x1b[";
		case "BracketRight":
			return "\x1b]";
		case "Backslash":
			return "\x1b\\";
		case "Backquote":
			return "\x1b`";
		case "ArrowLeft":
			return "\x1b[1;3D"; // word backward
		case "ArrowRight":
			return "\x1b[1;3C"; // word forward
		case "ArrowUp":
			return "\x1b[1;3A";
		case "ArrowDown":
			return "\x1b[1;3B";
	}

	return null;
}
