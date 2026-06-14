import type { AwaitingInputType } from "../../stores/terminals";

/** Inputs for deciding whether to fire a completion notification */
export interface CompletionContext {
	isActiveTerminal: boolean;
	isDebouncedBusy: boolean;
	activeSubTasks: number;
	awaitingInput: AwaitingInputType;
	durationMs: number;
	thresholdMs: number;
	/**
	 * Whether this terminal has OSC 133 shell integration (it has ever executed
	 * a real command, i.e. emitted a "C" marker). When false we cannot tell real
	 * work from a prompt-redraw / sleep-wake false-busy, so we do NOT gate — the
	 * terminal keeps the legacy behaviour. TUIC does not inject OSC 133, so plain
	 * shells without shell integration fall into this branch.
	 */
	usesShellIntegration: boolean;
	/**
	 * Whether a real command executed during the busy window that just ended
	 * (the OSC 133 "C" timestamp advanced since busy-start). Only meaningful when
	 * `usesShellIntegration` is true.
	 */
	ranCommandDuringBusy: boolean;
}

export type CompletionSuppressionReason =
	| "below-threshold"
	| "active-terminal"
	| "still-busy"
	| "active-sub-tasks"
	| "awaiting-input"
	| "no-command-ran"
	| null;

/**
 * Pure decision function: should we fire a completion notification?
 * Returns null if the notification should fire, or a reason string
 * explaining why it was suppressed.
 *
 * Extracted from App.tsx fireCompletion / onBusyToIdle logic so the
 * decision can be unit-tested without SolidJS reactivity.
 */
export function getCompletionSuppression(ctx: CompletionContext): CompletionSuppressionReason {
	if (ctx.durationMs < ctx.thresholdMs) return "below-threshold";
	if (ctx.isActiveTerminal) return "active-terminal";
	if (ctx.isDebouncedBusy) return "still-busy";
	if (ctx.activeSubTasks > 0) return "active-sub-tasks";
	if (ctx.awaitingInput) return "awaiting-input";
	// Shell-integrated terminal that went busy without running a command — this is
	// a prompt redraw or a sleep/wake false-busy cascade, not completed work.
	if (ctx.usesShellIntegration && !ctx.ranCommandDuringBusy) return "no-command-ran";
	return null;
}
