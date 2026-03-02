import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { pluginStore } from "../../stores/pluginStore";

beforeEach(() => {
  pluginStore.clear();
});

// ---------------------------------------------------------------------------
// registerPlugin
// ---------------------------------------------------------------------------

describe("registerPlugin", () => {
  it("adds a new plugin entry", () => {
    pluginStore.registerPlugin("test-plugin", { builtIn: false, loaded: true });
    const plugin = pluginStore.getPlugin("test-plugin");
    expect(plugin).toBeDefined();
    expect(plugin!.id).toBe("test-plugin");
    expect(plugin!.builtIn).toBe(false);
    expect(plugin!.loaded).toBe(true);
    expect(plugin!.enabled).toBe(true); // default
    expect(plugin!.error).toBeNull();
  });

  it("upserts existing plugin (replaces entry)", () => {
    pluginStore.registerPlugin("p1", { loaded: false, error: "init failed" });
    expect(pluginStore.getPlugin("p1")!.error).toBe("init failed");

    pluginStore.registerPlugin("p1", { loaded: true });
    const updated = pluginStore.getPlugin("p1")!;
    expect(updated.loaded).toBe(true);
    expect(updated.error).toBeNull(); // reset because new entry
  });

  it("does not duplicate entries on upsert", () => {
    pluginStore.registerPlugin("p1");
    pluginStore.registerPlugin("p1");
    pluginStore.registerPlugin("p1");
    expect(pluginStore.getAll().filter((p) => p.id === "p1")).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// updatePlugin
// ---------------------------------------------------------------------------

describe("updatePlugin", () => {
  it("updates partial fields on existing plugin", () => {
    pluginStore.registerPlugin("p1", { loaded: false });
    pluginStore.updatePlugin("p1", { loaded: true, error: "warning" });
    const p = pluginStore.getPlugin("p1")!;
    expect(p.loaded).toBe(true);
    expect(p.error).toBe("warning");
  });

  it("is a no-op for unknown plugin id", () => {
    pluginStore.updatePlugin("nonexistent", { loaded: true });
    expect(pluginStore.getPlugin("nonexistent")).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// getOrCreateLogger (via getLogger)
// ---------------------------------------------------------------------------

describe("getLogger", () => {
  it("creates a logger lazily on first call", () => {
    const logger = pluginStore.getLogger("lazy-plugin");
    expect(logger).toBeDefined();
    expect(typeof logger.info).toBe("function");
  });

  it("returns the same logger instance on subsequent calls", () => {
    const a = pluginStore.getLogger("same-plugin");
    const b = pluginStore.getLogger("same-plugin");
    expect(a).toBe(b);
  });

  it("shares logger between getLogger and registerPlugin", () => {
    const loggerBefore = pluginStore.getLogger("shared");
    pluginStore.registerPlugin("shared");
    const plugin = pluginStore.getPlugin("shared")!;
    expect(plugin.logger).toBe(loggerBefore);
  });
});

// ---------------------------------------------------------------------------
// removePlugin
// ---------------------------------------------------------------------------

describe("removePlugin", () => {
  it("removes plugin from store", () => {
    pluginStore.registerPlugin("rm-me");
    expect(pluginStore.getPlugin("rm-me")).toBeDefined();
    pluginStore.removePlugin("rm-me");
    expect(pluginStore.getPlugin("rm-me")).toBeUndefined();
  });
});
