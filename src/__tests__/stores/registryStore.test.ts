import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue([]);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("registryStore", () => {
  let store: typeof import("../../stores/registryStore").registryStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockInvoke.mockReset().mockResolvedValue([]);

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    // registryStore uses ../invoke which routes through @tauri-apps/api/core in Tauri mode
    // setup.ts sets __TAURI_INTERNALS__ so isTauri() returns true
    store = (await import("../../stores/registryStore")).registryStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeEntry(overrides: Partial<{ id: string; latestVersion: string }> = {}) {
    return {
      id: "my-plugin",
      name: "My Plugin",
      description: "A plugin",
      author: "alice",
      repo: "https://github.com/alice/my-plugin",
      latestVersion: "1.0.0",
      minAppVersion: "0.1.0",
      capabilities: ["terminal"],
      downloadUrl: "https://example.com/my-plugin.tar.gz",
      ...overrides,
    };
  }

  describe("hasUpdate()", () => {
    it("returns null when plugin is not in the registry", () => {
      createRoot((dispose) => {
        expect(store.hasUpdate("unknown-plugin", "1.0.0")).toBeNull();
        dispose();
      });
    });

    it("returns the registry entry when installed version differs from latest", async () => {
      const entry = makeEntry({ id: "my-plugin", latestVersion: "2.0.0" });
      mockInvoke.mockResolvedValueOnce([entry]);

      await createRoot(async (dispose) => {
        await store.fetch();
        const result = store.hasUpdate("my-plugin", "1.0.0");
        expect(result).not.toBeNull();
        expect(result!.latestVersion).toBe("2.0.0");
        dispose();
      });
    });

    it("returns null when installed version matches latest", async () => {
      const entry = makeEntry({ id: "my-plugin", latestVersion: "1.0.0" });
      mockInvoke.mockResolvedValueOnce([entry]);

      await createRoot(async (dispose) => {
        await store.fetch();
        const result = store.hasUpdate("my-plugin", "1.0.0");
        expect(result).toBeNull();
        dispose();
      });
    });

    it("returns entry when versions differ by minor version", async () => {
      const entry = makeEntry({ id: "my-plugin", latestVersion: "1.1.0" });
      mockInvoke.mockResolvedValueOnce([entry]);

      await createRoot(async (dispose) => {
        await store.fetch();
        const result = store.hasUpdate("my-plugin", "1.0.0");
        expect(result).not.toBeNull();
        dispose();
      });
    });
  });

  describe("fetch() TTL cache", () => {
    it("fetches from backend on first call", async () => {
      const entries = [makeEntry()];
      mockInvoke.mockResolvedValueOnce(entries);

      await createRoot(async (dispose) => {
        await store.fetch();
        expect(mockInvoke).toHaveBeenCalledWith("fetch_plugin_registry");
        expect(store.state.entries).toHaveLength(1);
        dispose();
      });
    });

    it("skips fetch when cache is within TTL", async () => {
      const entries = [makeEntry()];
      mockInvoke.mockResolvedValueOnce(entries);

      await createRoot(async (dispose) => {
        await store.fetch();
        mockInvoke.mockClear();

        // Second call within the TTL window should not invoke again
        await store.fetch();
        expect(mockInvoke).not.toHaveBeenCalled();
        dispose();
      });
    });

    it("re-fetches after TTL expires", async () => {
      const entries = [makeEntry()];
      mockInvoke.mockResolvedValue(entries);

      await createRoot(async (dispose) => {
        await store.fetch();
        mockInvoke.mockClear();

        // Advance time past the 1-hour TTL
        vi.advanceTimersByTime(61 * 60 * 1000);

        await store.fetch();
        expect(mockInvoke).toHaveBeenCalledWith("fetch_plugin_registry");
        dispose();
      });
    });

    it("sets loading=true during fetch and false after", async () => {
      let seenLoading = false;
      let resolve!: (v: unknown) => void;
      mockInvoke.mockReturnValueOnce(new Promise((r) => { resolve = r; }));

      await createRoot(async (dispose) => {
        const fetchPromise = store.fetch();

        // At this point the promise is pending — loading should be true
        seenLoading = store.state.loading;

        resolve([]);
        await fetchPromise;

        expect(seenLoading).toBe(true);
        expect(store.state.loading).toBe(false);
        dispose();
      });
    });

    it("sets error state on fetch failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("network error"));

      await createRoot(async (dispose) => {
        await store.fetch();
        expect(store.state.error).toContain("network error");
        expect(store.state.loading).toBe(false);
        dispose();
      });
    });
  });

  describe("fetch() concurrent dedup", () => {
    it("skips second fetch while first is in-flight", async () => {
      let resolve!: (v: unknown) => void;
      mockInvoke.mockReturnValueOnce(new Promise((r) => { resolve = r; }));

      await createRoot(async (dispose) => {
        const p1 = store.fetch();
        const p2 = store.fetch(); // concurrent call while first is loading

        resolve([makeEntry()]);
        await Promise.all([p1, p2]);

        // Only one backend call should have been made
        expect(mockInvoke).toHaveBeenCalledTimes(1);
        dispose();
      });
    });
  });

  describe("refresh()", () => {
    it("ignores TTL and fetches fresh data", async () => {
      const entries = [makeEntry()];
      mockInvoke.mockResolvedValue(entries);

      await createRoot(async (dispose) => {
        await store.fetch(); // warm cache
        mockInvoke.mockClear();

        await store.refresh(); // should bypass TTL
        expect(mockInvoke).toHaveBeenCalledWith("fetch_plugin_registry");
        dispose();
      });
    });
  });
});
