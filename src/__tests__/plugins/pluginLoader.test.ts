import { describe, it, expect, beforeEach, vi, type Mock } from "vitest";
import {
  validateManifest,
  validateModule,
  loadUserPlugins,
} from "../../plugins/pluginLoader";
import type { PluginManifest } from "../../plugins/pluginLoader";

// Mock invoke
vi.mock("../../invoke", () => ({
  invoke: vi.fn(() => Promise.resolve([])),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// Mock pluginRegistry
vi.mock("../../plugins/pluginRegistry", () => ({
  pluginRegistry: {
    register: vi.fn(),
    unregister: vi.fn(),
  },
}));

import { invoke, listen } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function validManifest(overrides: Partial<PluginManifest> = {}): PluginManifest {
  return {
    id: "test-plugin",
    name: "Test Plugin",
    version: "1.0.0",
    min_app_version: "0.1.0",
    main: "main.js",
    capabilities: [],
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// validateManifest
// ---------------------------------------------------------------------------

describe("validateManifest", () => {
  it("accepts a valid manifest", () => {
    expect(validateManifest(validManifest())).toBeNull();
  });

  it("rejects missing id", () => {
    expect(validateManifest(validManifest({ id: "" }))).toContain("id");
  });

  it("rejects missing name", () => {
    expect(validateManifest(validManifest({ name: "" }))).toContain("name");
  });

  it("rejects missing version", () => {
    expect(validateManifest(validManifest({ version: "" }))).toContain("version");
  });

  it("rejects missing main", () => {
    expect(validateManifest(validManifest({ main: "" }))).toContain("main");
  });

  it("rejects minAppVersion higher than current app version", () => {
    const error = validateManifest(validManifest({ min_app_version: "99.0.0" }));
    expect(error).toContain("requires app version");
  });

  it("accepts minAppVersion equal to current", () => {
    // App is 0.3.0
    expect(validateManifest(validManifest({ min_app_version: "0.3.0" }))).toBeNull();
  });

  it("accepts minAppVersion lower than current", () => {
    expect(validateManifest(validManifest({ min_app_version: "0.1.0" }))).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// validateModule
// ---------------------------------------------------------------------------

describe("validateModule", () => {
  it("accepts a valid module with default export", () => {
    const mod = {
      default: { id: "test", onload: () => {}, onunload: () => {} },
    };
    expect(validateModule(mod, "test")).toBeNull();
  });

  it("rejects module without default export", () => {
    expect(validateModule({}, "test")).toContain("default export");
  });

  it("rejects module where default is not an object", () => {
    expect(validateModule({ default: "string" }, "test")).toContain("default export");
  });

  it("rejects module missing onload", () => {
    const mod = { default: { id: "test", onunload: () => {} } };
    expect(validateModule(mod, "test")).toContain("onload");
  });

  it("rejects module missing onunload", () => {
    const mod = { default: { id: "test", onload: () => {} } };
    expect(validateModule(mod, "test")).toContain("onunload");
  });

  it("rejects module where id doesn't match manifest", () => {
    const mod = {
      default: { id: "wrong-id", onload: () => {}, onunload: () => {} },
    };
    expect(validateModule(mod, "expected-id")).toContain("id mismatch");
  });
});

// ---------------------------------------------------------------------------
// loadUserPlugins
// ---------------------------------------------------------------------------

describe("loadUserPlugins", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockResolvedValue([]);
  });

  it("calls list_user_plugins to discover plugins", async () => {
    await loadUserPlugins();
    expect(invoke).toHaveBeenCalledWith("list_user_plugins");
  });

  it("registers listen handler for plugin-changed events", async () => {
    await loadUserPlugins();
    expect(listen).toHaveBeenCalledWith("plugin-changed", expect.any(Function));
  });

  it("skips plugins that fail manifest validation", async () => {
    (invoke as Mock).mockResolvedValue([
      validManifest({ min_app_version: "99.0.0" }),
    ]);
    await loadUserPlugins();
    expect(pluginRegistry.register).not.toHaveBeenCalled();
  });

  it("does not crash when list_user_plugins rejects", async () => {
    (invoke as Mock).mockRejectedValue(new Error("Tauri not available"));
    // Should not throw — just log and return
    await expect(loadUserPlugins()).resolves.toBeUndefined();
  });
});
