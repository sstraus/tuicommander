/** Minimum busy duration before a completion notification fires. */
export const BUSY_THRESHOLD_MS = 5_000;
/** Deferral for agent sessions — wait this long after idle to confirm truly done. */
export const DEFERRED_COMPLETION_MS = 10_000;

export interface MobileCompletionContext {
  busyDurationMs: number;
  activeSubTasks: number;
  awaiting: boolean;
  error: boolean;
  isAgent: boolean;
}

export type MobileCompletionAction =
  | { action: "fire" }
  | { action: "defer"; delayMs: number }
  | { action: "suppress"; reason: MobileCompletionSuppression };

export type MobileCompletionSuppression =
  | "below-threshold"
  | "active-sub-tasks"
  | "awaiting-input"
  | "error";

/**
 * Pure decision function: given a busy→idle transition, decide whether to
 * fire the completion sound immediately, defer it, or suppress it.
 *
 * Mirrors the desktop logic in App.tsx / completionDecision.ts but adapted
 * for the mobile polling model (no "active terminal" or "debounced busy").
 */
export function getMobileCompletionAction(
  ctx: MobileCompletionContext,
): MobileCompletionAction {
  if (ctx.busyDurationMs < BUSY_THRESHOLD_MS)
    return { action: "suppress", reason: "below-threshold" };
  if (ctx.activeSubTasks > 0)
    return { action: "suppress", reason: "active-sub-tasks" };
  if (ctx.awaiting)
    return { action: "suppress", reason: "awaiting-input" };
  if (ctx.error)
    return { action: "suppress", reason: "error" };
  if (ctx.isAgent)
    return { action: "defer", delayMs: DEFERRED_COMPLETION_MS };
  return { action: "fire" };
}
