import { describe, it, expect } from "vitest";
import { deriveStatus } from "../utils/deriveStatus";
import type { SessionInfo } from "../useSessions";

function makeSession(overrides: Partial<NonNullable<SessionInfo["state"]>> = {}): SessionInfo {
  return {
    session_id: "test",
    cwd: "/tmp",
    worktree_path: null,
    worktree_branch: null,
    state: {
      awaiting_input: false,
      rate_limited: false,
      shell_state: "idle",
      last_activity_ms: Date.now(),
      ...overrides,
    },
  };
}

describe("deriveStatus", () => {
  it("returns idle when state is undefined", () => {
    const session: SessionInfo = { session_id: "x", cwd: null, worktree_path: null, worktree_branch: null };
    expect(deriveStatus(session)).toBe("idle");
  });

  it("returns idle when all flags are false", () => {
    expect(deriveStatus(makeSession())).toBe("idle");
  });

  it("returns busy when shell_state is busy", () => {
    expect(deriveStatus(makeSession({ shell_state: "busy" }))).toBe("busy");
  });

  it("returns question when awaiting_input is true", () => {
    expect(deriveStatus(makeSession({ awaiting_input: true }))).toBe("question");
  });

  it("returns error when last_error is set", () => {
    expect(deriveStatus(makeSession({ last_error: "something went wrong" }))).toBe("error");
  });

  it("returns rate-limited when rate_limited is true", () => {
    expect(deriveStatus(makeSession({ rate_limited: true }))).toBe("rate-limited");
  });

  it("returns sub-tasks when active_sub_tasks > 0 and shell is idle", () => {
    expect(deriveStatus(makeSession({ active_sub_tasks: 2 }))).toBe("sub-tasks");
  });

  it("returns busy when shell_state is busy even with active_sub_tasks", () => {
    expect(deriveStatus(makeSession({ shell_state: "busy", active_sub_tasks: 3 }))).toBe("busy");
  });

  describe("priority order: rate_limited > error > question > busy > sub-tasks > idle", () => {
    it("rate_limited beats error", () => {
      expect(deriveStatus(makeSession({ rate_limited: true, last_error: "err" }))).toBe("rate-limited");
    });

    it("rate_limited beats question", () => {
      expect(deriveStatus(makeSession({ rate_limited: true, awaiting_input: true }))).toBe("rate-limited");
    });

    it("rate_limited beats busy", () => {
      expect(deriveStatus(makeSession({ rate_limited: true, shell_state: "busy" }))).toBe("rate-limited");
    });

    it("error beats question", () => {
      expect(deriveStatus(makeSession({ last_error: "err", awaiting_input: true }))).toBe("error");
    });

    it("error beats busy", () => {
      expect(deriveStatus(makeSession({ last_error: "err", shell_state: "busy" }))).toBe("error");
    });

    it("question beats busy", () => {
      expect(deriveStatus(makeSession({ awaiting_input: true, shell_state: "busy" }))).toBe("question");
    });

    it("question beats sub-tasks", () => {
      expect(deriveStatus(makeSession({ awaiting_input: true, active_sub_tasks: 2 }))).toBe("question");
    });

    it("error beats sub-tasks", () => {
      expect(deriveStatus(makeSession({ last_error: "err", active_sub_tasks: 1 }))).toBe("error");
    });

    it("busy beats sub-tasks", () => {
      expect(deriveStatus(makeSession({ shell_state: "busy", active_sub_tasks: 1 }))).toBe("busy");
    });
  });
});
