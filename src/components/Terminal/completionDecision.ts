import type { AwaitingInputType } from "../../stores/terminals";

/** Inputs for deciding whether to fire a completion notification */
export interface CompletionContext {
  isActiveTerminal: boolean;
  isDebouncedBusy: boolean;
  activeSubTasks: number;
  awaitingInput: AwaitingInputType;
  durationMs: number;
  thresholdMs: number;
}

export type CompletionSuppressionReason =
  | "below-threshold"
  | "active-terminal"
  | "still-busy"
  | "active-sub-tasks"
  | "awaiting-input"
  | null;

/**
 * Pure decision function: should we fire a completion notification?
 * Returns null if the notification should fire, or a reason string
 * explaining why it was suppressed.
 *
 * Extracted from App.tsx fireCompletion / onBusyToIdle logic so the
 * decision can be unit-tested without SolidJS reactivity.
 */
export function getCompletionSuppression(
  ctx: CompletionContext,
): CompletionSuppressionReason {
  if (ctx.durationMs < ctx.thresholdMs) return "below-threshold";
  if (ctx.isActiveTerminal) return "active-terminal";
  if (ctx.isDebouncedBusy) return "still-busy";
  if (ctx.activeSubTasks > 0) return "active-sub-tasks";
  if (ctx.awaitingInput) return "awaiting-input";
  return null;
}
