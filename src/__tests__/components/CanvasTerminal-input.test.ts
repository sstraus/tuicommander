import { describe, expect, it } from "vitest";
import {
	altSequenceFromCode,
	createCompositionState,
	DUP_KEYDOWN_WINDOW_MS,
	keyToSequence,
} from "../../components/Terminal/terminalInput";

describe("keyToSequence", () => {
	const evt = (key: string, opts: Partial<KeyboardEvent> = {}): KeyboardEvent =>
		({
			key,
			code: "",
			ctrlKey: false,
			altKey: false,
			shiftKey: false,
			metaKey: false,
			...opts,
		}) as unknown as KeyboardEvent;

	// --- Printable characters ---
	it("passes printable ASCII as-is", () => {
		expect(keyToSequence(evt("a"))).toBe("a");
		expect(keyToSequence(evt("Z"))).toBe("Z");
		expect(keyToSequence(evt("1"))).toBe("1");
		expect(keyToSequence(evt(" "))).toBe(" ");
	});

	// --- Enter / Tab / Backspace / Escape ---
	it("maps Enter to CR", () => {
		expect(keyToSequence(evt("Enter"))).toBe("\r");
	});

	it("maps Tab to HT", () => {
		expect(keyToSequence(evt("Tab"))).toBe("\t");
	});

	it("maps Backspace to DEL", () => {
		expect(keyToSequence(evt("Backspace"))).toBe("\x7f");
	});

	it("maps Escape to ESC", () => {
		expect(keyToSequence(evt("Escape"))).toBe("\x1b");
	});

	// --- Arrow keys ---
	it("maps arrow keys to CSI sequences", () => {
		expect(keyToSequence(evt("ArrowUp"))).toBe("\x1b[A");
		expect(keyToSequence(evt("ArrowDown"))).toBe("\x1b[B");
		expect(keyToSequence(evt("ArrowRight"))).toBe("\x1b[C");
		expect(keyToSequence(evt("ArrowLeft"))).toBe("\x1b[D");
	});

	it("maps Shift+ArrowUp to modified CSI sequence", () => {
		expect(keyToSequence(evt("ArrowUp", { shiftKey: true }))).toBe("\x1b[1;2A");
	});

	it("maps Ctrl+ArrowRight", () => {
		expect(keyToSequence(evt("ArrowRight", { ctrlKey: true }))).toBe("\x1b[1;5C");
	});

	// --- Function keys ---
	it("maps F1-F4", () => {
		expect(keyToSequence(evt("F1"))).toBe("\x1bOP");
		expect(keyToSequence(evt("F2"))).toBe("\x1bOQ");
		expect(keyToSequence(evt("F3"))).toBe("\x1bOR");
		expect(keyToSequence(evt("F4"))).toBe("\x1bOS");
	});

	it("maps F5-F12", () => {
		expect(keyToSequence(evt("F5"))).toBe("\x1b[15~");
		expect(keyToSequence(evt("F6"))).toBe("\x1b[17~");
		expect(keyToSequence(evt("F7"))).toBe("\x1b[18~");
		expect(keyToSequence(evt("F8"))).toBe("\x1b[19~");
		expect(keyToSequence(evt("F9"))).toBe("\x1b[20~");
		expect(keyToSequence(evt("F10"))).toBe("\x1b[21~");
		expect(keyToSequence(evt("F11"))).toBe("\x1b[23~");
		expect(keyToSequence(evt("F12"))).toBe("\x1b[24~");
	});

	// --- Ctrl+letter ---
	it("maps Ctrl+C to ETX (0x03)", () => {
		expect(keyToSequence(evt("c", { ctrlKey: true }))).toBe("\x03");
	});

	it("maps Ctrl+A to SOH (0x01)", () => {
		expect(keyToSequence(evt("a", { ctrlKey: true }))).toBe("\x01");
	});

	it("maps Ctrl+Z to SUB (0x1a)", () => {
		expect(keyToSequence(evt("z", { ctrlKey: true }))).toBe("\x1a");
	});

	// --- Ctrl+punctuation ---
	it("maps Ctrl+[ to ESC (0x1b)", () => {
		expect(keyToSequence(evt("[", { ctrlKey: true }))).toBe("\x1b");
	});

	it("maps Ctrl+\\ to FS (0x1c)", () => {
		expect(keyToSequence(evt("\\", { ctrlKey: true }))).toBe("\x1c");
	});

	it("maps Ctrl+] to GS (0x1d)", () => {
		expect(keyToSequence(evt("]", { ctrlKey: true }))).toBe("\x1d");
	});

	it("maps Ctrl+^ to RS (0x1e)", () => {
		expect(keyToSequence(evt("^", { ctrlKey: true }))).toBe("\x1e");
	});

	it("maps Ctrl+_ to US (0x1f)", () => {
		expect(keyToSequence(evt("_", { ctrlKey: true }))).toBe("\x1f");
	});

	it("maps Ctrl+@ to NUL (0x00)", () => {
		expect(keyToSequence(evt("@", { ctrlKey: true }))).toBe("\x00");
	});

	// --- Shift+Tab ---
	it("maps Shift+Tab to reverse tab (CSI Z)", () => {
		expect(keyToSequence(evt("Tab", { shiftKey: true }))).toBe("\x1b[Z");
	});

	// --- Alt+letter (with e.code for macOS dead-key handling) ---
	it("maps Alt+letter via e.code to ESC + base char", () => {
		expect(keyToSequence(evt("π", { altKey: true, code: "KeyP" } as Partial<KeyboardEvent>))).toBe("\x1bp");
		expect(keyToSequence(evt("∂", { altKey: true, code: "KeyD" } as Partial<KeyboardEvent>))).toBe("\x1bd");
	});

	it("maps Alt+Shift+letter via e.code to ESC + uppercase", () => {
		expect(keyToSequence(evt("∏", { altKey: true, shiftKey: true, code: "KeyP" } as Partial<KeyboardEvent>))).toBe(
			"\x1bP",
		);
	});

	it("maps Alt+digit via e.code", () => {
		expect(keyToSequence(evt("¡", { altKey: true, code: "Digit1" } as Partial<KeyboardEvent>))).toBe("\x1b1");
	});

	// --- Navigation keys ---
	it("maps Home/End/Insert/Delete/PageUp/PageDown", () => {
		expect(keyToSequence(evt("Home"))).toBe("\x1b[H");
		expect(keyToSequence(evt("End"))).toBe("\x1b[F");
		expect(keyToSequence(evt("Insert"))).toBe("\x1b[2~");
		expect(keyToSequence(evt("Delete"))).toBe("\x1b[3~");
		expect(keyToSequence(evt("PageUp"))).toBe("\x1b[5~");
		expect(keyToSequence(evt("PageDown"))).toBe("\x1b[6~");
	});

	// --- Meta (Cmd) keys return null (not intercepted) ---
	it("returns null for meta-modified keys", () => {
		expect(keyToSequence(evt("c", { metaKey: true }))).toBeNull();
		expect(keyToSequence(evt("v", { metaKey: true }))).toBeNull();
	});

	// --- Non-character keys return null ---
	it("returns null for modifier-only keys", () => {
		expect(keyToSequence(evt("Shift"))).toBeNull();
		expect(keyToSequence(evt("Control"))).toBeNull();
		expect(keyToSequence(evt("Alt"))).toBeNull();
		expect(keyToSequence(evt("Meta"))).toBeNull();
		expect(keyToSequence(evt("CapsLock"))).toBeNull();
	});
});

describe("altSequenceFromCode", () => {
	const evt = (code: string, opts: Partial<KeyboardEvent> = {}): KeyboardEvent =>
		({
			key: "",
			code,
			ctrlKey: false,
			altKey: true,
			shiftKey: false,
			metaKey: false,
			...opts,
		}) as unknown as KeyboardEvent;

	it("maps Alt+Backspace to backward-kill-word", () => {
		expect(altSequenceFromCode(evt("Backspace"))).toBe("\x1b\x7f");
	});

	it("maps Alt+. to insert-last-argument", () => {
		expect(altSequenceFromCode(evt("Period"))).toBe("\x1b.");
	});

	it("maps Alt+ArrowLeft to word backward", () => {
		expect(altSequenceFromCode(evt("ArrowLeft"))).toBe("\x1b[1;3D");
	});

	it("maps Alt+ArrowRight to word forward", () => {
		expect(altSequenceFromCode(evt("ArrowRight"))).toBe("\x1b[1;3C");
	});

	it("maps Alt+/ for history search", () => {
		expect(altSequenceFromCode(evt("Slash"))).toBe("\x1b/");
	});

	it("maps Alt+Space", () => {
		expect(altSequenceFromCode(evt("Space"))).toBe("\x1b ");
	});

	it("maps Alt+Comma", () => {
		expect(altSequenceFromCode(evt("Comma"))).toBe("\x1b,");
	});

	it("maps Alt+Minus", () => {
		expect(altSequenceFromCode(evt("Minus"))).toBe("\x1b-");
	});

	it("maps Alt+Equal", () => {
		expect(altSequenceFromCode(evt("Equal"))).toBe("\x1b=");
	});

	it("maps Alt+Semicolon", () => {
		expect(altSequenceFromCode(evt("Semicolon"))).toBe("\x1b;");
	});

	it("maps Alt+Quote", () => {
		expect(altSequenceFromCode(evt("Quote"))).toBe("\x1b'");
	});

	it("maps Alt+BracketLeft", () => {
		expect(altSequenceFromCode(evt("BracketLeft"))).toBe("\x1b[");
	});

	it("maps Alt+BracketRight", () => {
		expect(altSequenceFromCode(evt("BracketRight"))).toBe("\x1b]");
	});

	it("maps Alt+Backslash", () => {
		expect(altSequenceFromCode(evt("Backslash"))).toBe("\x1b\\");
	});

	it("maps Alt+Backquote", () => {
		expect(altSequenceFromCode(evt("Backquote"))).toBe("\x1b`");
	});

	it("maps Alt+ArrowUp", () => {
		expect(altSequenceFromCode(evt("ArrowUp"))).toBe("\x1b[1;3A");
	});

	it("maps Alt+ArrowDown", () => {
		expect(altSequenceFromCode(evt("ArrowDown"))).toBe("\x1b[1;3B");
	});

	it("returns null for unknown codes", () => {
		expect(altSequenceFromCode(evt("IntlBackslash"))).toBeNull();
	});
});

describe("createCompositionState — dead-key / IME composition", () => {
	function makeState() {
		// Controllable clock so tests can advance time deterministically.
		let nowMs = 1000;
		const advance = (ms: number) => {
			nowMs += ms;
		};
		const state = createCompositionState(() => nowMs);
		return { state, advance };
	}

	// --- compositionend ---

	it("onCompositionEnd returns composed data", () => {
		const { state } = makeState();
		expect(state.onCompositionEnd("ç")).toBe("ç");
	});

	it("onCompositionEnd returns null for empty string", () => {
		const { state } = makeState();
		expect(state.onCompositionEnd("")).toBeNull();
	});

	it("onCompositionEnd returns null for null/undefined", () => {
		const { state } = makeState();
		expect(state.onCompositionEnd(null)).toBeNull();
		expect(state.onCompositionEnd(undefined)).toBeNull();
	});

	// --- shouldSuppressKeydown during composition ---

	it("suppresses keydown when isComposing=true (dead key in progress)", () => {
		const { state } = makeState();
		expect(state.shouldSuppressKeydown(true)).toBe(true);
	});

	it("does NOT suppress normal keydown when not composing", () => {
		const { state } = makeState();
		expect(state.shouldSuppressKeydown(false, "a")).toBe(false);
	});

	// --- WKWebView duplicate keydown suppression ---

	it("suppresses the trailing duplicate matching the base letter (ç → c)", () => {
		const { state } = makeState();
		state.onCompositionEnd("ç");
		expect(state.shouldSuppressKeydown(false, "c")).toBe(true);
	});

	it("suppresses an accented-vowel duplicate by its base letter (é → e)", () => {
		const { state } = makeState();
		state.onCompositionEnd("é");
		expect(state.shouldSuppressKeydown(false, "e")).toBe(true);
	});

	it("suppresses only ONE duplicate keydown after compositionend", () => {
		const { state } = makeState();
		state.onCompositionEnd("ç");
		state.shouldSuppressKeydown(false, "c"); // consume the one suppressed duplicate
		expect(state.shouldSuppressKeydown(false, "c")).toBe(false);
	});

	it("suppresses regardless of dispatch ordering when delayed within the window", () => {
		const { state, advance } = makeState();
		state.onCompositionEnd("ç");
		advance(50); // WKWebView fires the duplicate keydown a few ms later
		expect(state.shouldSuppressKeydown(false, "c")).toBe(true);
	});

	it("does NOT suppress a non-matching key — fast real keystroke after an accent (é then p)", () => {
		const { state } = makeState();
		state.onCompositionEnd("é");
		expect(state.shouldSuppressKeydown(false, "p")).toBe(false);
	});

	it("stops suppressing once the time window elapses (no duplicate ever arrived)", () => {
		const { state, advance } = makeState();
		state.onCompositionEnd("á");
		advance(DUP_KEYDOWN_WINDOW_MS + 1);
		expect(state.shouldSuppressKeydown(false, "a")).toBe(false);
	});

	it("full sequence: compositionend → duplicate eaten → next real key passes", () => {
		const { state } = makeState();

		// ´ + e → é
		expect(state.onCompositionEnd("é")).toBe("é");

		// WKWebView duplicate keydown(key="e", isComposing=false) — must be eaten
		expect(state.shouldSuppressKeydown(false, "e")).toBe(true);

		// Next real keystroke must pass through
		expect(state.shouldSuppressKeydown(false, "e")).toBe(false);
	});

	// --- Cancelled composition must NOT suppress next keydown ---

	it("does not suppress next keydown after empty compositionend", () => {
		const { state } = makeState();
		state.onCompositionEnd("");
		expect(state.shouldSuppressKeydown(false, "a")).toBe(false);
	});

	it("does not suppress next keydown after null compositionend", () => {
		const { state } = makeState();
		state.onCompositionEnd(null);
		expect(state.shouldSuppressKeydown(false, "a")).toBe(false);
	});

	it("does not suppress next keydown after undefined compositionend", () => {
		const { state } = makeState();
		state.onCompositionEnd(undefined);
		expect(state.shouldSuppressKeydown(false, "a")).toBe(false);
	});

	it("isComposing=true keydown during composition is always blocked", () => {
		const { state } = makeState();
		// No compositionend yet — user is mid dead-key sequence
		expect(state.shouldSuppressKeydown(true)).toBe(true);
		expect(state.shouldSuppressKeydown(true)).toBe(true);
	});
});
