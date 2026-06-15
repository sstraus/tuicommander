import { describe, expect, it } from "vitest";
import { type CompletionContext, getCompletionSuppression } from "../../components/Terminal/completionDecision";

/** Helper: default context where completion SHOULD fire */
function baseCtx(overrides: Partial<CompletionContext> = {}): CompletionContext {
	return {
		isActiveTerminal: false,
		isDebouncedBusy: false,
		activeSubTasks: 0,
		awaitingInput: null,
		durationMs: 10_000,
		thresholdMs: 5_000,
		usesShellIntegration: false,
		ranCommandDuringBusy: false,
		...overrides,
	};
}

describe("getCompletionSuppression", () => {
	describe("fires completion (returns null)", () => {
		it("fires when all conditions are clear", () => {
			expect(getCompletionSuppression(baseCtx())).toBeNull();
		});

		it("fires when awaitingInput is null", () => {
			expect(getCompletionSuppression(baseCtx({ awaitingInput: null }))).toBeNull();
		});
	});

	describe("suppresses when awaitingInput is set", () => {
		it("suppresses on awaitingInput='question'", () => {
			expect(getCompletionSuppression(baseCtx({ awaitingInput: "question" }))).toBe("awaiting-input");
		});

		it("suppresses on awaitingInput='error'", () => {
			expect(getCompletionSuppression(baseCtx({ awaitingInput: "error" }))).toBe("awaiting-input");
		});
	});

	describe("awaitingInput transitions", () => {
		it("suppresses while awaitingInput is set, then fires after cleared", () => {
			// Simulates: agent finishes but is waiting for input → suppressed
			const whileWaiting = baseCtx({ awaitingInput: "question" });
			expect(getCompletionSuppression(whileWaiting)).toBe("awaiting-input");

			// User answers the question, awaitingInput cleared → fires
			const afterCleared = baseCtx({ awaitingInput: null });
			expect(getCompletionSuppression(afterCleared)).toBeNull();
		});

		it("suppresses when awaitingInput changes from question to error", () => {
			// Both truthy values suppress — the specific type doesn't matter
			expect(getCompletionSuppression(baseCtx({ awaitingInput: "question" }))).toBe("awaiting-input");
			expect(getCompletionSuppression(baseCtx({ awaitingInput: "error" }))).toBe("awaiting-input");
		});
	});

	describe("other suppression reasons take priority over awaitingInput", () => {
		it("below-threshold suppresses before awaitingInput is checked", () => {
			expect(getCompletionSuppression(baseCtx({ durationMs: 2_000, awaitingInput: "question" }))).toBe(
				"below-threshold",
			);
		});

		it("active-terminal suppresses before awaitingInput is checked", () => {
			expect(getCompletionSuppression(baseCtx({ isActiveTerminal: true, awaitingInput: "error" }))).toBe(
				"active-terminal",
			);
		});

		it("still-busy suppresses before awaitingInput is checked", () => {
			expect(getCompletionSuppression(baseCtx({ isDebouncedBusy: true, awaitingInput: "question" }))).toBe(
				"still-busy",
			);
		});

		it("active-sub-tasks suppresses before awaitingInput is checked", () => {
			expect(getCompletionSuppression(baseCtx({ activeSubTasks: 2, awaitingInput: "error" }))).toBe("active-sub-tasks");
		});
	});

	describe("other suppression reasons (no awaitingInput)", () => {
		it("suppresses below duration threshold", () => {
			expect(getCompletionSuppression(baseCtx({ durationMs: 3_000 }))).toBe("below-threshold");
		});

		it("suppresses when terminal is active", () => {
			expect(getCompletionSuppression(baseCtx({ isActiveTerminal: true }))).toBe("active-terminal");
		});

		it("suppresses when still debounced-busy", () => {
			expect(getCompletionSuppression(baseCtx({ isDebouncedBusy: true }))).toBe("still-busy");
		});

		it("suppresses when sub-tasks are active", () => {
			expect(getCompletionSuppression(baseCtx({ activeSubTasks: 1 }))).toBe("active-sub-tasks");
		});
	});

	describe("OSC 133 command gate (sleep/wake & prompt-redraw false-busy)", () => {
		it("suppresses a shell-integrated terminal that went busy without running a command", () => {
			// The mdkb/sleep-wake case: terminal has shell integration but no command
			// executed during this busy window → false-busy, must not fire.
			expect(getCompletionSuppression(baseCtx({ usesShellIntegration: true, ranCommandDuringBusy: false }))).toBe(
				"no-command-ran",
			);
		});

		it("fires when a shell-integrated terminal actually ran a command", () => {
			expect(getCompletionSuppression(baseCtx({ usesShellIntegration: true, ranCommandDuringBusy: true }))).toBeNull();
		});

		it("does NOT gate terminals without shell integration (fallback to legacy behaviour)", () => {
			// No OSC 133 → cannot tell real work from redraw → keep firing as before.
			expect(
				getCompletionSuppression(baseCtx({ usesShellIntegration: false, ranCommandDuringBusy: false })),
			).toBeNull();
		});

		it("earlier suppression reasons take priority over the command gate", () => {
			expect(
				getCompletionSuppression(
					baseCtx({ isActiveTerminal: true, usesShellIntegration: true, ranCommandDuringBusy: false }),
				),
			).toBe("active-terminal");
		});
	});
});
