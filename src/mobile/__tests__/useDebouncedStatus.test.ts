import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { useDebouncedStatus } from "../utils/useDebouncedStatus";
import type { SessionInfo, SessionState } from "../useSessions";

function makeSession(overrides: Partial<SessionState> = {}): SessionInfo {
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

describe("useDebouncedStatus", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it("returns idle for an idle session", () => {
    let status!: () => string;
    const dispose = createRoot((d) => {
      status = useDebouncedStatus(() => makeSession());
      return d;
    });
    expect(status()).toBe("idle");
    dispose();
  });

  it("returns busy immediately when initialized busy", () => {
    let status!: () => string;
    const dispose = createRoot((d) => {
      status = useDebouncedStatus(() => makeSession({ shell_state: "busy" }));
      return d;
    });
    expect(status()).toBe("busy");
    dispose();
  });

  it("transitions to busy when signal changes", () => {
    let status!: () => string;
    let set!: (s: SessionInfo) => void;
    const dispose = createRoot((d) => {
      const [session, setSession] = createSignal(makeSession());
      set = setSession;
      status = useDebouncedStatus(session);
      return d;
    });
    expect(status()).toBe("idle");
    set(makeSession({ shell_state: "busy" }));
    expect(status()).toBe("busy");
    dispose();
  });

  it("holds busy for 2s after transition to idle", () => {
    let status!: () => string;
    let set!: (s: SessionInfo) => void;
    const dispose = createRoot((d) => {
      const [session, setSession] = createSignal(makeSession({ shell_state: "busy" }));
      set = setSession;
      status = useDebouncedStatus(session);
      return d;
    });
    expect(status()).toBe("busy");

    set(makeSession({ shell_state: "idle" }));
    expect(status()).toBe("busy"); // Still in hold

    vi.advanceTimersByTime(1500);
    expect(status()).toBe("busy"); // Still holding

    vi.advanceTimersByTime(500);
    expect(status()).toBe("idle"); // Hold expired at 2s

    dispose();
  });

  it("cancels cooldown if busy resumes during hold", () => {
    let status!: () => string;
    let set!: (s: SessionInfo) => void;
    const dispose = createRoot((d) => {
      const [session, setSession] = createSignal(makeSession({ shell_state: "busy" }));
      set = setSession;
      status = useDebouncedStatus(session);
      return d;
    });

    set(makeSession({ shell_state: "idle" }));
    expect(status()).toBe("busy");

    vi.advanceTimersByTime(500);
    set(makeSession({ shell_state: "busy" }));
    expect(status()).toBe("busy");

    vi.advanceTimersByTime(2000);
    expect(status()).toBe("busy"); // Timer was cancelled

    dispose();
  });

  it("question overrides busy hold immediately", () => {
    let status!: () => string;
    let set!: (s: SessionInfo) => void;
    const dispose = createRoot((d) => {
      const [session, setSession] = createSignal(makeSession({ shell_state: "busy" }));
      set = setSession;
      status = useDebouncedStatus(session);
      return d;
    });

    set(makeSession({ awaiting_input: true }));
    expect(status()).toBe("question");

    dispose();
  });

  it("error overrides busy hold immediately", () => {
    let status!: () => string;
    let set!: (s: SessionInfo) => void;
    const dispose = createRoot((d) => {
      const [session, setSession] = createSignal(makeSession({ shell_state: "busy" }));
      set = setSession;
      status = useDebouncedStatus(session);
      return d;
    });

    set(makeSession({ last_error: "oops" }));
    expect(status()).toBe("error");

    dispose();
  });

  it("rate-limited overrides busy hold immediately", () => {
    let status!: () => string;
    let set!: (s: SessionInfo) => void;
    const dispose = createRoot((d) => {
      const [session, setSession] = createSignal(makeSession({ shell_state: "busy" }));
      set = setSession;
      status = useDebouncedStatus(session);
      return d;
    });

    set(makeSession({ rate_limited: true }));
    expect(status()).toBe("rate-limited");

    dispose();
  });

  it("sub-tasks passes through when not in busy hold", () => {
    let status!: () => string;
    const dispose = createRoot((d) => {
      status = useDebouncedStatus(() => makeSession({ active_sub_tasks: 2 }));
      return d;
    });
    expect(status()).toBe("sub-tasks");
    dispose();
  });
});
