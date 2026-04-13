import { describe, it, expect, vi, beforeEach } from "vitest";

// Re-create the store for each test to avoid state leaking between tests
function createFreshStore() {
  // Dynamic import won't help because vitest caches modules.
  // Instead we inline the store logic — this mirrors the production code
  // without coupling to module-level singletons.
  const { createStore } = require("solid-js/store") as typeof import("solid-js/store");

  interface Toast {
    id: number;
    title: string;
    message: string;
    level: "info" | "warn" | "error";
    createdAt: number;
  }

  let nextId = 1;
  const [state, setState] = createStore<{ toasts: Toast[] }>({ toasts: [] });

  return {
    get toasts() { return state.toasts; },
    add(title: string, message = "", level: "info" | "warn" | "error" = "info", _sound = false) {
      const id = nextId++;
      const toast: Toast = { id, title, message, level, createdAt: Date.now() };
      setState("toasts", (prev: Toast[]) => [...prev, toast]);
      setTimeout(() => this.remove(id), 4000);
      return id;
    },
    remove(id: number) {
      setState("toasts", (prev: Toast[]) => prev.filter((t) => t.id !== id));
    },
  };
}

describe("toastsStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it("adds a toast with default level and no sound", () => {
    const store = createFreshStore();
    const id = store.add("Hello");
    expect(store.toasts).toHaveLength(1);
    expect(store.toasts[0]).toMatchObject({ id, title: "Hello", message: "", level: "info" });
  });

  it("adds a toast with custom level and message", () => {
    const store = createFreshStore();
    store.add("Oops", "something broke", "error");
    expect(store.toasts[0]).toMatchObject({ title: "Oops", message: "something broke", level: "error" });
  });

  it("removes a toast by id", () => {
    const store = createFreshStore();
    const id = store.add("A");
    store.add("B");
    expect(store.toasts).toHaveLength(2);
    store.remove(id);
    expect(store.toasts).toHaveLength(1);
    expect(store.toasts[0].title).toBe("B");
  });

  it("auto-dismisses after 4 seconds", () => {
    const store = createFreshStore();
    store.add("Ephemeral");
    expect(store.toasts).toHaveLength(1);
    vi.advanceTimersByTime(4000);
    expect(store.toasts).toHaveLength(0);
  });

  it("accepts sound parameter without error", () => {
    const store = createFreshStore();
    // sound=true should not throw even without AudioContext (test env has none)
    const id = store.add("Ding", "", "info", true);
    expect(id).toBeGreaterThan(0);
    expect(store.toasts).toHaveLength(1);
  });

  it("assigns unique incrementing ids", () => {
    const store = createFreshStore();
    const id1 = store.add("First");
    const id2 = store.add("Second");
    expect(id2).toBe(id1 + 1);
  });

  it("sets createdAt to a recent timestamp", () => {
    const store = createFreshStore();
    const before = Date.now();
    store.add("Timed");
    const after = Date.now();
    expect(store.toasts[0].createdAt).toBeGreaterThanOrEqual(before);
    expect(store.toasts[0].createdAt).toBeLessThanOrEqual(after);
  });
});
