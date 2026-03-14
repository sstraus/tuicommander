import { describe, it, expect } from "vitest";
import {
  getMobileCompletionAction,
  BUSY_THRESHOLD_MS,
  DEFERRED_COMPLETION_MS,
  type MobileCompletionContext,
} from "../utils/mobileCompletionDecision";

/** Helper: default context where completion SHOULD fire (non-agent). */
function baseCtx(overrides: Partial<MobileCompletionContext> = {}): MobileCompletionContext {
  return {
    busyDurationMs: 10_000,
    activeSubTasks: 0,
    awaiting: false,
    error: false,
    isAgent: false,
    ...overrides,
  };
}

describe("getMobileCompletionAction", () => {
  describe("fires immediately", () => {
    it("fires for non-agent with all conditions clear", () => {
      expect(getMobileCompletionAction(baseCtx())).toEqual({ action: "fire" });
    });

    it("fires at exactly the busy threshold", () => {
      expect(getMobileCompletionAction(baseCtx({ busyDurationMs: BUSY_THRESHOLD_MS }))).toEqual({
        action: "fire",
      });
    });
  });

  describe("defers for agent sessions", () => {
    it("returns defer with correct delay for agent sessions", () => {
      expect(getMobileCompletionAction(baseCtx({ isAgent: true }))).toEqual({
        action: "defer",
        delayMs: DEFERRED_COMPLETION_MS,
      });
    });
  });

  describe("suppression", () => {
    it("suppresses below busy duration threshold", () => {
      expect(getMobileCompletionAction(baseCtx({ busyDurationMs: 2_000 }))).toEqual({
        action: "suppress",
        reason: "below-threshold",
      });
    });

    it("suppresses when active sub-tasks are running", () => {
      expect(getMobileCompletionAction(baseCtx({ activeSubTasks: 2 }))).toEqual({
        action: "suppress",
        reason: "active-sub-tasks",
      });
    });

    it("suppresses when awaiting input", () => {
      expect(getMobileCompletionAction(baseCtx({ awaiting: true }))).toEqual({
        action: "suppress",
        reason: "awaiting-input",
      });
    });

    it("suppresses when there is an error", () => {
      expect(getMobileCompletionAction(baseCtx({ error: true }))).toEqual({
        action: "suppress",
        reason: "error",
      });
    });
  });

  describe("priority order: threshold > sub-tasks > awaiting > error > agent-defer", () => {
    it("below-threshold beats active-sub-tasks", () => {
      expect(
        getMobileCompletionAction(baseCtx({ busyDurationMs: 1_000, activeSubTasks: 3 })),
      ).toEqual({ action: "suppress", reason: "below-threshold" });
    });

    it("active-sub-tasks beats awaiting", () => {
      expect(
        getMobileCompletionAction(baseCtx({ activeSubTasks: 1, awaiting: true })),
      ).toEqual({ action: "suppress", reason: "active-sub-tasks" });
    });

    it("awaiting beats error", () => {
      expect(
        getMobileCompletionAction(baseCtx({ awaiting: true, error: true })),
      ).toEqual({ action: "suppress", reason: "awaiting-input" });
    });

    it("error beats agent defer", () => {
      expect(
        getMobileCompletionAction(baseCtx({ error: true, isAgent: true })),
      ).toEqual({ action: "suppress", reason: "error" });
    });

    it("agent defers only when all suppression checks pass", () => {
      expect(
        getMobileCompletionAction(
          baseCtx({ isAgent: true, activeSubTasks: 0, awaiting: false, error: false }),
        ),
      ).toEqual({ action: "defer", delayMs: DEFERRED_COMPLETION_MS });
    });
  });
});
