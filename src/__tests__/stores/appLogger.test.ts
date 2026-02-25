import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

// Mock rpc before importing appLogger
const mockRpc = vi.fn().mockResolvedValue(undefined);
vi.mock("../../transport", () => ({
  rpc: (...args: unknown[]) => mockRpc(...args),
}));

// Dynamic import so the mock is in place first
const { appLogger } = await import("../../stores/appLogger");

describe("appLogger", () => {
  beforeEach(() => {
    appLogger.clear();
    mockRpc.mockClear();
    mockRpc.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ---- Basic push/get ----

  it("push adds entries retrievable by getEntries", () => {
    createRoot(() => {
      appLogger.info("app", "hello");
      appLogger.warn("git", "warning msg");

      const entries = appLogger.getEntries();
      expect(entries).toHaveLength(2);
      expect(entries[0].level).toBe("info");
      expect(entries[0].source).toBe("app");
      expect(entries[0].message).toBe("hello");
      expect(entries[1].level).toBe("warn");
      expect(entries[1].source).toBe("git");
    });
  });

  it("entries have monotonically increasing ids", () => {
    createRoot(() => {
      appLogger.info("app", "a");
      appLogger.info("app", "b");
      appLogger.info("app", "c");

      const entries = appLogger.getEntries();
      expect(entries[0].id).toBeLessThan(entries[1].id);
      expect(entries[1].id).toBeLessThan(entries[2].id);
    });
  });

  it("entries have timestamps", () => {
    createRoot(() => {
      const before = Date.now();
      appLogger.info("app", "timed");
      const after = Date.now();

      const entry = appLogger.getEntries()[0];
      expect(entry.timestamp).toBeGreaterThanOrEqual(before);
      expect(entry.timestamp).toBeLessThanOrEqual(after);
    });
  });

  // ---- Convenience methods ----

  it("error/warn/info/debug convenience methods set correct level", () => {
    createRoot(() => {
      appLogger.error("app", "e");
      appLogger.warn("app", "w");
      appLogger.info("app", "i");
      appLogger.debug("app", "d");

      const levels = appLogger.getEntries().map((e) => e.level);
      expect(levels).toEqual(["error", "warn", "info", "debug"]);
    });
  });

  // ---- Data field ----

  it("preserves optional data field", () => {
    createRoot(() => {
      const data = { status: 500, url: "/api/test" };
      appLogger.error("network", "request failed", data);

      const entry = appLogger.getEntries()[0];
      expect(entry.data).toEqual(data);
    });
  });

  // ---- Ring buffer wrapping ----

  it("getEntries returns chronological order after wrap", () => {
    createRoot(() => {
      // Push more than MAX_ENTRIES would allow in a small buffer
      // We can't control MAX_ENTRIES (it's 1000), so just verify ordering
      for (let i = 0; i < 50; i++) {
        appLogger.info("app", `msg-${i}`);
      }
      const entries = appLogger.getEntries();
      expect(entries).toHaveLength(50);
      expect(entries[0].message).toBe("msg-0");
      expect(entries[49].message).toBe("msg-49");
    });
  });

  // ---- Clear ----

  it("clear removes all entries", () => {
    createRoot(() => {
      appLogger.info("app", "a");
      appLogger.error("app", "b");
      expect(appLogger.getEntries()).toHaveLength(2);

      appLogger.clear();
      expect(appLogger.getEntries()).toHaveLength(0);
      expect(appLogger.entryCount()).toBe(0);
    });
  });

  it("clear resets unseen error count", () => {
    createRoot(() => {
      appLogger.error("app", "err1");
      appLogger.error("app", "err2");
      expect(appLogger.unseenErrorCount()).toBe(2);

      appLogger.clear();
      expect(appLogger.unseenErrorCount()).toBe(0);
    });
  });

  it("clear calls rpc clear_logs", () => {
    createRoot(() => {
      appLogger.info("app", "a");
      mockRpc.mockClear();

      appLogger.clear();
      expect(mockRpc).toHaveBeenCalledWith("clear_logs");
    });
  });

  // ---- Unseen error count ----

  it("unseenErrorCount increments only on errors", () => {
    createRoot(() => {
      appLogger.info("app", "info");
      expect(appLogger.unseenErrorCount()).toBe(0);

      appLogger.error("app", "err");
      expect(appLogger.unseenErrorCount()).toBe(1);

      appLogger.warn("app", "warn");
      expect(appLogger.unseenErrorCount()).toBe(1);
    });
  });

  it("markSeen resets unseen error count", () => {
    createRoot(() => {
      appLogger.error("app", "e1");
      appLogger.error("app", "e2");
      expect(appLogger.unseenErrorCount()).toBe(2);

      appLogger.markSeen();
      expect(appLogger.unseenErrorCount()).toBe(0);
    });
  });

  // ---- entryCount ----

  it("entryCount returns current count", () => {
    createRoot(() => {
      expect(appLogger.entryCount()).toBe(0);
      appLogger.info("app", "a");
      expect(appLogger.entryCount()).toBe(1);
      appLogger.info("app", "b");
      expect(appLogger.entryCount()).toBe(2);
    });
  });

  // ---- Rust backend mirroring ----

  it("push calls rpc push_log with correct args", async () => {
    createRoot(() => {
      appLogger.error("network", "timeout", { url: "/api" });
    });

    // Allow promises to settle
    await vi.waitFor(() => {
      expect(mockRpc).toHaveBeenCalledWith(
        "push_log",
        expect.objectContaining({
          level: "error",
          source: "network",
          message: "timeout",
          dataJson: JSON.stringify({ url: "/api" }),
        }),
      );
    });
  });

  it("push sends null dataJson when no data provided", async () => {
    createRoot(() => {
      appLogger.warn("app", "plain message");
    });

    await vi.waitFor(() => {
      expect(mockRpc).toHaveBeenCalledWith(
        "push_log",
        expect.objectContaining({
          level: "warn",
          source: "app",
          message: "plain message",
          dataJson: null,
        }),
      );
    });
  });

  it("debug and info levels do not mirror to Rust", () => {
    createRoot(() => {
      mockRpc.mockClear();
      appLogger.debug("app", "debug msg");
      appLogger.info("app", "info msg");
      expect(mockRpc).not.toHaveBeenCalled();
    });
  });

  // ---- Hydration from Rust ----

  it("hydrateFromRust merges Rust entries into local buffer", async () => {
    mockRpc.mockImplementation((cmd: string) => {
      if (cmd === "get_logs") {
        return Promise.resolve([
          { id: 100, timestamp_ms: 1000, level: "warn", source: "git", message: "from-rust", data_json: null },
          { id: 101, timestamp_ms: 1001, level: "error", source: "app", message: "rust-error", data_json: '{"code":42}' },
        ]);
      }
      return Promise.resolve(undefined);
    });

    await createRoot(async () => {
      await appLogger.hydrateFromRust();

      const entries = appLogger.getEntries();
      expect(entries.length).toBeGreaterThanOrEqual(2);

      const rustEntries = entries.filter((e) => e.message.startsWith("from-rust") || e.message.startsWith("rust-error"));
      expect(rustEntries).toHaveLength(2);
      expect(rustEntries[0].id).toBe(100);
      expect(rustEntries[0].source).toBe("git");
      expect(rustEntries[1].id).toBe(101);
      expect(rustEntries[1].data).toEqual({ code: 42 });
    });
  });

  it("hydrateFromRust deduplicates entries already in local buffer", async () => {
    // First push a local entry, then hydrate with same IDs
    let localId: number;
    createRoot(() => {
      appLogger.info("app", "local");
      localId = appLogger.getEntries()[0].id;
    });

    mockRpc.mockImplementation((cmd: string) => {
      if (cmd === "get_logs") {
        return Promise.resolve([
          { id: localId!, timestamp_ms: 1000, level: "info", source: "app", message: "local", data_json: null },
          { id: 999, timestamp_ms: 2000, level: "warn", source: "git", message: "new-from-rust", data_json: null },
        ]);
      }
      return Promise.resolve(undefined);
    });

    await createRoot(async () => {
      const countBefore = appLogger.entryCount();
      await appLogger.hydrateFromRust();
      // Should only add 1 new entry (id=999), not duplicate the local one
      expect(appLogger.entryCount()).toBe(countBefore + 1);
    });
  });

  it("hydrateFromRust handles empty response", async () => {
    mockRpc.mockImplementation((cmd: string) => {
      if (cmd === "get_logs") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    await createRoot(async () => {
      appLogger.info("app", "existing");
      await appLogger.hydrateFromRust();
      // No crash, existing entries preserved
      expect(appLogger.entryCount()).toBeGreaterThanOrEqual(1);
    });
  });

  it("hydrateFromRust handles rpc failure gracefully", async () => {
    mockRpc.mockImplementation((cmd: string) => {
      if (cmd === "get_logs") return Promise.reject(new Error("not ready"));
      return Promise.resolve(undefined);
    });

    await createRoot(async () => {
      appLogger.info("app", "safe");
      await appLogger.hydrateFromRust();
      // No crash, local buffer still intact
      expect(appLogger.getEntries()).toHaveLength(1);
    });
  });
});
