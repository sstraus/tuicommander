import { afterEach, describe, expect, it, vi } from "vitest";
import { resetPlatformCache } from "../../platform";
import {
	AGENT_ENTER_GAP_MS,
	containsShellMetacharacters,
	sendCommand,
	shouldAutoSubmitSuggestion,
	UNVERIFIED_ENTER_GAP_MS,
} from "../../utils/sendCommand";

/**
 * Fake writer that records every call in order. Returns a resolved promise
 * so sendCommand's internal awaits don't stall.
 */
function makeRecorder() {
	const calls: string[] = [];
	const writeFn = async (data: string): Promise<void> => {
		calls.push(data);
	};
	return { writeFn, calls };
}

/**
 * Replace navigator.platform for the duration of a test so isWindows()
 * returns the expected value. Restored via afterEach.
 *
 * The cache reset is not optional: `detectPlatform` answers from the UA string
 * once and remembers, because the platform cannot change while the app runs.
 * Swapping `navigator.platform` without clearing it leaves the answer from
 * whichever test ran first, which makes the outcome depend on file order.
 */
function setPlatform(value: string) {
	Object.defineProperty(navigator, "platform", {
		value,
		configurable: true,
	});
	resetPlatformCache();
}

describe("sendCommand", () => {
	const originalPlatform = navigator.platform;

	afterEach(() => {
		setPlatform(originalPlatform);
	});

	it("always sends Ctrl-U prefix when an agent is attached (ignores shellFamily)", async () => {
		setPlatform("Win32");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", "claude", "windows-native");
		expect(calls).toEqual(["\x15", "ls", "\r"]);
	});

	/**
	 * Regression: Claude Code (verified live on v2.1.280) treats a long input
	 * chunk as a paste. A Ctrl-U bundled into that chunk becomes pasted content:
	 * Claude strips it ("Removed 1 invisible character · review and press Enter
	 * to send") and refuses the following Enter, however late it arrives. A
	 * 584-char dictation stayed unsent with a 500ms Enter gap; with Ctrl-U in its
	 * own earlier read, 584 and 1500 chars both submitted.
	 */
	it("sends Ctrl-U to an agent in its own write, separated in TIME from the text", async () => {
		setPlatform("MacIntel");
		const stamps: number[] = [];
		const calls: string[] = [];
		const writeFn = async (data: string): Promise<void> => {
			calls.push(data);
			stamps.push(performance.now());
		};
		const longText = "dictated text ".repeat(50).trim();
		await sendCommand(writeFn, longText, "claude", "posix");
		expect(calls).toEqual(["\x15", longText, "\r"]);
		expect(stamps[1] - stamps[0]).toBeGreaterThanOrEqual(AGENT_ENTER_GAP_MS - 5);
	});

	it("sends Ctrl-U for POSIX shellFamily even when running on Windows (git-bash regression)", async () => {
		setPlatform("Win32");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", null, "posix");
		expect(calls).toEqual(["\x15ls", "\r"]);
	});

	it("skips Ctrl-U for windows-native shellFamily when no agent", async () => {
		setPlatform("Win32");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "dir", null, "windows-native");
		expect(calls).toEqual(["dir", "\r"]);
	});

	it("falls back to platform heuristic for unknown shellFamily on Windows (skip)", async () => {
		setPlatform("Win32");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "echo hi", null, "unknown");
		expect(calls).toEqual(["echo hi", "\r"]);
	});

	it("falls back to platform heuristic for unknown shellFamily on macOS (send Ctrl-U)", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", null, "unknown");
		expect(calls).toEqual(["\x15ls", "\r"]);
	});

	it("falls back to platform heuristic when shellFamily omitted on macOS", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls");
		expect(calls).toEqual(["\x15ls", "\r"]);
	});

	it("falls back to platform heuristic when shellFamily omitted on Windows", async () => {
		setPlatform("Win32");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "dir");
		expect(calls).toEqual(["dir", "\r"]);
	});

	it("wraps multi-line text in bracketed paste sequences", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "line1\nline2", null, "posix");
		expect(calls).toEqual(["\x15\x1b[200~line1\nline2\x1b[201~", "\r"]);
	});

	it("does not wrap single-line text in bracketed paste", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "single line", null, "posix");
		expect(calls).toEqual(["\x15single line", "\r"]);
	});

	it("sends Enter as a separate write regardless of prefix decision", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "foo", null, "posix");
		expect(calls.length).toBe(2);
		expect(calls[1]).toBe("\r");
	});

	it("withholds the trailing Enter when submit is false", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "rm -rf /", null, "posix", false);
		// Text is typed (with Ctrl-U prefix) but NOT executed — user must press Enter.
		expect(calls).toEqual(["\x15rm -rf /"]);
	});

	it("submits by default (submit omitted) — backward compatible", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", null, "posix");
		expect(calls).toEqual(["\x15ls", "\r"]);
	});

	/**
	 * Regression: two writes are not two reads. Without an elapsed-time gap the
	 * PTY coalesces payload + CR into one read() and an Ink/raw-mode agent
	 * (Codex, Claude Code) renders the CR as a newline in its composer instead
	 * of submitting — the suggestion is typed but never sent.
	 */
	it("separates the Enter from the payload in TIME when an agent is attached", async () => {
		setPlatform("MacIntel");
		const stamps: number[] = [];
		const writeFn = async (): Promise<void> => {
			stamps.push(performance.now());
		};
		await sendCommand(writeFn, "run the tests", "claude", "posix");
		expect(stamps.length).toBe(3);
		// setTimeout never fires early; allow a small scheduler tolerance.
		expect(stamps[2] - stamps[1]).toBeGreaterThanOrEqual(AGENT_ENTER_GAP_MS - 5);
	});

	/**
	 * Regression (story 1163): Codex consumes a long plain write as a paste burst
	 * and swallows an Enter that arrives while it is still ingesting — a 1000-char
	 * wake left in the composer at a 200ms gap, live on 0.159.0. A bracketed paste
	 * arrives as one event, so the same CR is an ordinary Enter.
	 */
	it("frames a long single-line Codex payload as a bracketed paste", async () => {
		setPlatform("MacIntel");
		const { writeFn, calls } = makeRecorder();
		const longLine = "BG DONE exit=100 ".repeat(100).trim();
		await sendCommand(writeFn, longLine, "codex", "posix");
		expect(calls).toEqual(["\x15", `\x1b[200~${longLine}\x1b[201~`, "\r"]);
	});

	it("keeps the existing Enter gap for Claude", async () => {
		// Assert the chosen delay, not wall-clock time: a late timer on a loaded
		// machine is not a defect, so no upper bound belongs here.
		const timeout = vi.spyOn(globalThis, "setTimeout");
		try {
			await sendCommand(async () => {}, "run tests", "claude", "posix");
			const delays = timeout.mock.calls.map(([, ms]) => ms);
			expect(delays).toContain(AGENT_ENTER_GAP_MS);
			expect(delays).not.toContain(UNVERIFIED_ENTER_GAP_MS);
		} finally {
			timeout.mockRestore();
		}
	});

	it("uses the Codex-safe Enter gap for an unrecognized agent type", async () => {
		const stamps: number[] = [];
		await sendCommand(
			async () => {
				stamps.push(performance.now());
			},
			"review this",
			"future-agent",
			"posix",
		);
		expect(stamps).toHaveLength(3);
		expect(stamps[2] - stamps[1]).toBeGreaterThanOrEqual(195);
	});

	it("does not delay the Enter on a plain shell (line-buffered, no coalescing risk)", async () => {
		setPlatform("MacIntel");
		const stamps: number[] = [];
		const writeFn = async (): Promise<void> => {
			stamps.push(performance.now());
		};
		await sendCommand(writeFn, "ls", null, "posix");
		expect(stamps[1] - stamps[0]).toBeLessThan(AGENT_ENTER_GAP_MS);
	});

	/**
	 * pi (0.83.0) accepts BOTH shapes — verified live against a real pi PTY:
	 * a single combined `text\r` write submits, and so does the split
	 * Ctrl-U + text / gap / CR sequence this function emits. Ctrl-U is consumed
	 * as a line-kill, never echoed literally. So pi needs no special-casing: it
	 * takes the same agent path as every other agent. This pins that — a future
	 * "optimization" that routes pi around the gap would be a silent regression
	 * on the agents that DO need it, for no gain on pi.
	 */
	it("routes pi through the standard agent path (Ctrl-U + gapped Enter)", async () => {
		setPlatform("MacIntel");
		const stamps: number[] = [];
		const calls: string[] = [];
		const writeFn = async (data: string): Promise<void> => {
			calls.push(data);
			stamps.push(performance.now());
		};
		await sendCommand(writeFn, "say only the word OK", "pi", "posix");
		expect(calls).toEqual(["\x15", "say only the word OK", "\r"]);
		expect(stamps[2] - stamps[1]).toBeGreaterThanOrEqual(AGENT_ENTER_GAP_MS - 5);
	});

	it("does not pay the Enter gap when the Enter is withheld", async () => {
		setPlatform("MacIntel");
		const started = performance.now();
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "run the tests", "codex", "posix", false);
		expect(calls).toEqual(["\x15", "run the tests"]);
		// Only the Ctrl-U gap elapses; a second gap would mean the Enter's was paid too.
		expect(performance.now() - started).toBeLessThan(2 * AGENT_ENTER_GAP_MS);
	});
});

describe("containsShellMetacharacters", () => {
	it("flags command chaining, substitution, and redirection", () => {
		for (const s of ["a; b", "a | b", "a && b", "$(whoami)", "`id`", "echo > f", "cat < f", "a\nb"]) {
			expect(containsShellMetacharacters(s)).toBe(true);
		}
	});

	it("does not flag plain suggestion prose", () => {
		for (const s of ["Fix the bug", "Run tests", "Deploy", "Refactor auth module"]) {
			expect(containsShellMetacharacters(s)).toBe(false);
		}
	});
});

describe("shouldAutoSubmitSuggestion", () => {
	it("always submits on an agent, even multi-line or metachar-bearing text", () => {
		for (const s of ["Fix the bug", "line one\nline two", "a; b", "$(whoami)", "echo > f"]) {
			expect(shouldAutoSubmitSuggestion("codex", s)).toBe(true);
			expect(shouldAutoSubmitSuggestion("claude", s)).toBe(true);
		}
	});

	it("withholds submit on a shell for metachar-bearing or multi-line text", () => {
		for (const agentType of [null, undefined]) {
			for (const s of ["a; b", "a | b", "$(whoami)", "echo > f", "line one\nline two"]) {
				expect(shouldAutoSubmitSuggestion(agentType, s)).toBe(false);
			}
		}
	});

	it("submits on a shell for plain single-line prose", () => {
		for (const agentType of [null, undefined]) {
			for (const s of ["Run tests", "Deploy", "Fix the bug"]) {
				expect(shouldAutoSubmitSuggestion(agentType, s)).toBe(true);
			}
		}
	});
});

describe("sendCommand framing (critic 1163)", () => {
	// Catches: the Codex bracketed branch leaking to other agents or to a plain
	// shell, which would paste "ls" into a shell prompt as literal escape bytes.
	it("frames a long single-line payload for codex only, and keeps short text plain", async () => {
		const long = "x".repeat(1000);
		for (const agent of ["claude", "gemini", "opencode", "grok", "pi", "future-agent", null]) {
			const { writeFn, calls } = makeRecorder();
			await sendCommand(writeFn, long, agent, "posix");
			expect(calls.at(-2)!, String(agent)).not.toContain("\x1b[200~");
		}
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, long, "codex", "posix");
		expect(calls).toEqual(["\x15", `\x1b[200~${long}\x1b[201~`, "\r"]);
		for (const short of ["y", "/status", "x".repeat(500)]) {
			const rec = makeRecorder();
			await sendCommand(rec.writeFn, short, "codex", "posix");
			expect(rec.calls).toEqual(["\x15", short, "\r"]);
		}
	});

	// Catches: the framed Codex prefill (submit=false) still sending Enter.
	it("types a framed codex prefill without Enter", async () => {
		const { writeFn, calls } = makeRecorder();
		const draft = "d".repeat(600);
		await sendCommand(writeFn, draft, "codex", "posix", false);
		expect(calls).toEqual(["\x15", `\x1b[200~${draft}\x1b[201~`]);
	});

	// Catches: an unverified agent losing its longer Enter gap (the constant is
	// asserted against a literal, not against itself).
	it("keeps an unverified gap longer than the verified one", () => {
		expect(AGENT_ENTER_GAP_MS).toBe(50);
		expect(UNVERIFIED_ENTER_GAP_MS).toBe(200);
	});
});
