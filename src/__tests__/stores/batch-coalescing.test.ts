/**
 * Tests that verify batch() is used in store mutations that fire multiple
 * sequential setState calls, ensuring reactive notifications are coalesced.
 *
 * Strategy: use createEffect + a notification counter. Without batch(), the
 * effect runs once per setState (one microtask flush per re-run). With batch(),
 * it runs exactly once for the whole group.
 *
 * SolidJS createEffect schedules re-runs via queueMicrotask. After flushing
 * all pending microtasks, we count how many times the effect ran.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot, createEffect } from "solid-js";
import { makeTerminal } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

/** Flush all pending SolidJS microtask-scheduled effects */
function flushEffects(): Promise<void> {
  return new Promise<void>((resolve) => queueMicrotask(resolve));
}

// ─── terminals.update() with shellState ────────────────────────────────────

describe("terminalsStore.update() — batch() when shellState changes", () => {
  let store: typeof import("../../stores/terminals").terminalsStore;
  let dispose: () => void;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
    store = (await import("../../stores/terminals")).terminalsStore;
  });

  afterEach(() => {
    dispose?.();
  });

  it("coalesces shellState + debouncedBusy change into a single reactive notification", async () => {
    let notificationCount = 0;
    let id!: string;

    createRoot((d) => {
      dispose = d;
      id = store.add(makeTerminal({ name: "T" }));

      createEffect(() => {
        // Track both state paths changed by update() with shellState:
        // 1. state.debouncedBusy[id]  — set by handleShellStateChange
        // 2. state.terminals[id].shellState — set by the outer setState
        void store.state.debouncedBusy[id];
        void store.state.terminals[id]?.shellState;
        notificationCount++;
      });
    });

    // Flush the initial effect run
    await flushEffects();
    notificationCount = 0;

    // update() with shellState triggers:
    //   setState("debouncedBusy", id, true)  — via handleShellStateChange
    //   setState("terminals", id, data)       — the data update
    store.update(id, { shellState: "busy" });

    // Flush scheduled re-runs
    await flushEffects();

    // With batch(): 1 notification for both state changes
    expect(notificationCount).toBe(1);
  });

  it("coalesces shellState + other data fields into a single reactive notification", async () => {
    let notificationCount = 0;
    let id!: string;

    createRoot((d) => {
      dispose = d;
      id = store.add(makeTerminal({ name: "T" }));

      createEffect(() => {
        void store.state.debouncedBusy[id];
        void store.state.terminals[id]?.shellState;
        void store.state.terminals[id]?.name;
        notificationCount++;
      });
    });

    await flushEffects();
    notificationCount = 0;

    store.update(id, { shellState: "busy", name: "Updated" });

    await flushEffects();

    // With batch(): 1 notification for all changes
    expect(notificationCount).toBe(1);
  });
});

// ─── repositories.removeTerminalFromBranch() ─────────────────────────────

describe("repositoriesStore.removeTerminalFromBranch() — batch() coalescing", () => {
  let store: typeof import("../../stores/repositories").repositoriesStore;
  let dispose: () => void;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
    store = (await import("../../stores/repositories")).repositoriesStore;
  });

  afterEach(() => {
    dispose?.();
  });

  it("coalesces terminals filter + savedTerminals clear into one notification", async () => {
    let notificationCount = 0;

    createRoot((d) => {
      dispose = d;
      store.add({ path: "/repo", displayName: "test" });
      store.setBranch("/repo", "main", {
        savedTerminals: [{ name: "T1", cwd: "/repo", fontSize: 14, agentType: null }],
      });
      store.addTerminalToBranch("/repo", "main", "term-1");

      createEffect(() => {
        // Track both fields changed by removeTerminalFromBranch when last terminal removed:
        // 1. terminals array (filtered)
        // 2. savedTerminals array (cleared)
        void store.get("/repo")?.branches["main"]?.terminals?.length;
        void store.get("/repo")?.branches["main"]?.savedTerminals?.length;
        notificationCount++;
      });
    });

    await flushEffects();
    notificationCount = 0;

    // Removing the last terminal triggers:
    //   setState(terminals filter)    — removes terminal from list
    //   setState(savedTerminals = []) — clears stale saved list
    store.removeTerminalFromBranch("/repo", "main", "term-1");

    await flushEffects();

    // With batch(): 1 notification for both state changes
    expect(notificationCount).toBe(1);
  });
});

// ─── uiStore.resetLayout() ────────────────────────────────────────────────

describe("uiStore.resetLayout() — batch() coalescing", () => {
  let store: typeof import("../../stores/ui").uiStore;
  let dispose: () => void;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
    store = (await import("../../stores/ui")).uiStore;
  });

  afterEach(() => {
    dispose?.();
  });

  it("coalesces all 4 setState calls in resetLayout() into one notification", async () => {
    let notificationCount = 0;

    createRoot((d) => {
      dispose = d;
      // Mutate to non-default values first so each field actually changes
      store.setSidebarWidth(250);
      store.setMarkdownPanelWidth(500);
      store.setNotesPanelWidth(400);
      store.setSettingsNavWidth(200);

      createEffect(() => {
        // Track all 4 fields that resetLayout() resets
        void store.state.sidebarWidth;
        void store.state.markdownPanelWidth;
        void store.state.notesPanelWidth;
        void store.state.settingsNavWidth;
        notificationCount++;
      });
    });

    await flushEffects();
    notificationCount = 0;

    store.resetLayout();

    await flushEffects();

    // With batch(): 1 notification instead of 4
    expect(notificationCount).toBe(1);

    // Verify values are actually reset to defaults
    expect(store.state.sidebarWidth).toBe(300);
    expect(store.state.markdownPanelWidth).toBe(400);
    expect(store.state.notesPanelWidth).toBe(350);
    expect(store.state.settingsNavWidth).toBe(180);
  });
});
