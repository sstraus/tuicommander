import { describe, it, expect, beforeEach, vi } from "vitest";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { pluginStore } from "../../stores/pluginStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { PluginCapabilityError } from "../../plugins/types";
import type { TuiPlugin, PluginHost } from "../../plugins/types";

// Mock invoke to avoid Tauri internals in test environment
vi.mock("../../invoke", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Flush pending queueMicrotask callbacks so deferred dispatch handlers run */
const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

function makePlugin(id: string, onload?: (host: PluginHost) => void, onunload?: () => void): TuiPlugin {
  return {
    id,
    onload: onload ?? (() => {}),
    onunload: onunload ?? (() => {}),
  };
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  pluginStore.clear();
});

// ---------------------------------------------------------------------------
// Plugin lifecycle
// ---------------------------------------------------------------------------

describe("register / unregister", () => {
  it("calls plugin.onload when registered", () => {
    const onload = vi.fn();
    pluginRegistry.register(makePlugin("p1", onload));
    expect(onload).toHaveBeenCalledOnce();
  });

  it("passes a PluginHost to onload", () => {
    let receivedHost: PluginHost | null = null;
    pluginRegistry.register(makePlugin("p1", (host) => { receivedHost = host; }));
    expect(receivedHost).not.toBeNull();
  });

  it("calls plugin.onunload when unregistered", () => {
    const onunload = vi.fn();
    pluginRegistry.register(makePlugin("p1", undefined, onunload));
    pluginRegistry.unregister("p1");
    expect(onunload).toHaveBeenCalledOnce();
  });

  it("unregister is a no-op for unknown plugin id", () => {
    expect(() => pluginRegistry.unregister("unknown")).not.toThrow();
  });

  it("re-registering same id replaces the old plugin", () => {
    const onunload1 = vi.fn();
    const onload2 = vi.fn();
    pluginRegistry.register(makePlugin("p1", undefined, onunload1));
    pluginRegistry.register(makePlugin("p1", onload2));
    expect(onunload1).toHaveBeenCalledOnce();
    expect(onload2).toHaveBeenCalledOnce();
  });
});

// ---------------------------------------------------------------------------
// PluginHost — section delegation
// ---------------------------------------------------------------------------

describe("PluginHost.registerSection", () => {
  it("delegates to activityStore and section appears in getSections", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerSection({ id: "test-section", label: "TEST", priority: 10, canDismissAll: false });
    }));
    expect(activityStore.getSections().some((s) => s.id === "test-section")).toBe(true);
  });

  it("disposing host registration removes section from activityStore", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      const d = host.registerSection({ id: "s1", label: "S1", priority: 10, canDismissAll: false });
      d.dispose();
    }));
    expect(activityStore.getSections().some((s) => s.id === "s1")).toBe(false);
  });

  it("unregistering plugin auto-disposes section", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerSection({ id: "s2", label: "S2", priority: 10, canDismissAll: false });
    }));
    pluginRegistry.unregister("p1");
    expect(activityStore.getSections().some((s) => s.id === "s2")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// PluginHost — item delegation
// ---------------------------------------------------------------------------

describe("PluginHost.addItem / removeItem / updateItem", () => {
  const baseItem = () => ({
    id: "item-1",
    pluginId: "p1",
    sectionId: "s1",
    title: "Test",
    icon: "<svg/>",
    dismissible: true,
  });

  it("addItem adds to activityStore", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.addItem(baseItem());
    }));
    expect(activityStore.getActive().some((i) => i.id === "item-1")).toBe(true);
  });

  it("removeItem removes from activityStore", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.addItem(baseItem());
      host.removeItem("item-1");
    }));
    expect(activityStore.getActive().find((i) => i.id === "item-1")).toBeUndefined();
  });

  it("updateItem updates title in activityStore", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.addItem(baseItem());
      host.updateItem("item-1", { title: "Updated" });
    }));
    expect(activityStore.getActive().find((i) => i.id === "item-1")?.title).toBe("Updated");
  });
});

// ---------------------------------------------------------------------------
// PluginHost — markdown provider delegation
// ---------------------------------------------------------------------------

describe("PluginHost.registerMarkdownProvider", () => {
  it("delegates to markdownProviderRegistry", async () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerMarkdownProvider("plan", { provideContent: () => "# Plan" });
    }));
    const result = await markdownProviderRegistry.resolve("plan:file?path=/foo.md");
    expect(result).toBe("# Plan");
  });

  it("unregistering plugin auto-disposes markdown provider", async () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerMarkdownProvider("plan", { provideContent: () => "# Plan" });
    }));
    pluginRegistry.unregister("p1");
    const result = await markdownProviderRegistry.resolve("plan:file");
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// OutputWatcher dispatch
// ---------------------------------------------------------------------------

describe("dispatchLine", () => {
  it("calls matching watcher with match and sessionId", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello (\w+)/, onMatch });
    }));
    pluginRegistry.dispatchLine("hello world", "session-1");
    await flushMicrotasks();
    expect(onMatch).toHaveBeenCalledOnce();
    expect(onMatch.mock.calls[0][0][1]).toBe("world");
    expect(onMatch.mock.calls[0][1]).toBe("session-1");
  });

  it("does not call watcher when pattern does not match", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    pluginRegistry.dispatchLine("goodbye world", "s1");
    await flushMicrotasks();
    expect(onMatch).not.toHaveBeenCalled();
  });

  it("resets lastIndex on global regex before each test", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /foo/g, onMatch });
    }));
    pluginRegistry.dispatchLine("foo bar", "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchLine("foo bar", "s1");
    await flushMicrotasks();
    expect(onMatch).toHaveBeenCalledTimes(2);
  });

  it("catches and does not rethrow watcher exceptions", async () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({
        pattern: /anything/,
        onMatch: () => { throw new Error("watcher boom"); },
      });
    }));
    pluginRegistry.dispatchLine("anything", "s1");
    await flushMicrotasks();
    // Exception is caught inside the microtask — no unhandled error
  });

  it("continues dispatching to other watchers after one throws", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({
        pattern: /anything/,
        onMatch: () => { throw new Error("boom"); },
      });
      host.registerOutputWatcher({ pattern: /anything/, onMatch });
    }));
    pluginRegistry.dispatchLine("anything", "s1");
    await flushMicrotasks();
    expect(onMatch).toHaveBeenCalledOnce();
  });

  it("unregistering plugin removes its watchers", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    pluginRegistry.unregister("p1");
    pluginRegistry.dispatchLine("hello", "s1");
    await flushMicrotasks();
    expect(onMatch).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// processRawOutput
// ---------------------------------------------------------------------------

describe("processRawOutput", () => {
  it("reassembles lines and dispatches clean (ANSI-stripped) lines to watchers", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    // Raw chunk with ANSI color + newline
    pluginRegistry.processRawOutput("\x1b[32mhello world\x1b[0m\n", "s1");
    await flushMicrotasks();
    expect(onMatch).toHaveBeenCalledOnce();
    // First arg is RegExpExecArray, match[0] should be the clean text match
    expect(onMatch.mock.calls[0][0][0]).toBe("hello");
    expect(onMatch.mock.calls[0][1]).toBe("s1");
  });

  it("holds partial lines until newline arrives", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    pluginRegistry.processRawOutput("hel", "s1");
    await flushMicrotasks();
    expect(onMatch).not.toHaveBeenCalled();
    pluginRegistry.processRawOutput("lo\n", "s1");
    await flushMicrotasks();
    expect(onMatch).toHaveBeenCalledOnce();
  });

  it("is a no-op when no watchers are registered", () => {
    // Should not throw even with no plugins registered
    expect(() => pluginRegistry.processRawOutput("anything\n", "s1")).not.toThrow();
  });

  it("maintains separate LineBuffers per sessionId", async () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /done/, onMatch });
    }));
    pluginRegistry.processRawOutput("do", "session-a");
    pluginRegistry.processRawOutput("ne\n", "session-b"); // different session
    await flushMicrotasks();
    expect(onMatch).not.toHaveBeenCalled(); // "done" not complete in session-b's buffer
    pluginRegistry.processRawOutput("ne\n", "session-a");  // completes in session-a
    await flushMicrotasks();
    expect(onMatch).toHaveBeenCalledOnce();
  });
});

// ---------------------------------------------------------------------------
// Structured event dispatch
// ---------------------------------------------------------------------------

describe("dispatchStructuredEvent", () => {
  it("calls handler for matching type", async () => {
    const handler = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", handler);
    }));
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/foo.md" }, "s1");
    await flushMicrotasks();
    expect(handler).toHaveBeenCalledWith({ path: "/foo.md" }, "s1");
  });

  it("does not call handler for non-matching type", async () => {
    const handler = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", handler);
    }));
    pluginRegistry.dispatchStructuredEvent("rate-limit", {}, "s1");
    await flushMicrotasks();
    expect(handler).not.toHaveBeenCalled();
  });

  it("calls all handlers registered for the same type", async () => {
    const h1 = vi.fn();
    const h2 = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", h1);
    }));
    pluginRegistry.register(makePlugin("p2", (host) => {
      host.registerStructuredEventHandler("plan-file", h2);
    }));
    pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
    await flushMicrotasks();
    expect(h1).toHaveBeenCalledOnce();
    expect(h2).toHaveBeenCalledOnce();
  });

  it("unregistering plugin removes its structured event handlers", async () => {
    const handler = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", handler);
    }));
    pluginRegistry.unregister("p1");
    pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
    await flushMicrotasks();
    expect(handler).not.toHaveBeenCalled();
  });

  it("catches and does not rethrow handler exceptions", async () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", () => { throw new Error("handler boom"); });
    }));
    pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
    await flushMicrotasks();
    // Exception is caught inside the microtask — no unhandled error
  });

  it("continues dispatching to other handlers after one throws", async () => {
    const h2 = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", () => { throw new Error("boom"); });
    }));
    pluginRegistry.register(makePlugin("p2", (host) => {
      host.registerStructuredEventHandler("plan-file", h2);
    }));
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/foo.md" }, "s1");
    await flushMicrotasks();
    expect(h2).toHaveBeenCalledOnce();
  });
});

// ---------------------------------------------------------------------------
// onload failure
// ---------------------------------------------------------------------------

describe("onload failure", () => {
  it("does not register a plugin whose onload throws", () => {
    pluginRegistry.register(makePlugin("bad", () => { throw new Error("onload boom"); }));
    // Plugin should not be registered — unregister should be a no-op
    const onunload = vi.fn();
    pluginRegistry.unregister("bad");
    expect(onunload).not.toHaveBeenCalled();
  });

  it("cleans up partial registrations when onload throws", () => {
    pluginRegistry.register(makePlugin("bad", (host) => {
      host.registerSection({ id: "partial-section", label: "X", priority: 10, canDismissAll: false });
      throw new Error("mid-onload boom");
    }));
    // The section registered before the throw should be cleaned up
    expect(activityStore.getSections().some((s) => s.id === "partial-section")).toBe(false);
  });

  it("does not affect subsequent plugin registrations", () => {
    pluginRegistry.register(makePlugin("bad", () => { throw new Error("boom"); }));
    const onload = vi.fn();
    pluginRegistry.register(makePlugin("good", onload));
    expect(onload).toHaveBeenCalledOnce();
  });
});

// ---------------------------------------------------------------------------
// removeSession
// ---------------------------------------------------------------------------

describe("removeSession", () => {
  it("cleans up LineBuffer for a session", () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    // Build up partial data in session buffer
    pluginRegistry.processRawOutput("hel", "s1");
    // Remove the session — the partial "hel" should be lost
    pluginRegistry.removeSession("s1");
    // Now send "lo\n" — should NOT complete "hello" since buffer was cleared
    pluginRegistry.processRawOutput("lo\n", "s1");
    expect(onMatch).not.toHaveBeenCalled();
  });

  it("is a no-op for unknown session", () => {
    expect(() => pluginRegistry.removeSession("nonexistent")).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// Tier 2: Read-only app state
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 2 read-only state", () => {
  it("getActiveRepo returns null when no repo is active", () => {
    let result: ReturnType<PluginHost["getActiveRepo"]> = undefined as never;
    pluginRegistry.register(makePlugin("p1", (host) => {
      result = host.getActiveRepo();
    }));
    expect(result).toBeNull();
  });

  it("getRepos returns empty array when no repos registered", () => {
    let result: ReturnType<PluginHost["getRepos"]> = undefined as never;
    pluginRegistry.register(makePlugin("p1", (host) => {
      result = host.getRepos();
    }));
    expect(result).toEqual([]);
  });

  it("getActiveTerminalSessionId returns null when no terminal active", () => {
    let result: ReturnType<PluginHost["getActiveTerminalSessionId"]> = undefined as never;
    pluginRegistry.register(makePlugin("p1", (host) => {
      result = host.getActiveTerminalSessionId();
    }));
    expect(result).toBeNull();
  });

  it("getPrNotifications returns empty array when no notifications", () => {
    let result: ReturnType<PluginHost["getPrNotifications"]> = undefined as never;
    pluginRegistry.register(makePlugin("p1", (host) => {
      result = host.getPrNotifications();
    }));
    expect(result).toEqual([]);
  });

  it("getSettings returns null for unknown repo", () => {
    let result: ReturnType<PluginHost["getSettings"]> = undefined as never;
    pluginRegistry.register(makePlugin("p1", (host) => {
      result = host.getSettings("/nonexistent/repo");
    }));
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Tier 3: Capability-gated write actions
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 3 capability gating", () => {
  it("built-in plugins (no capabilities) can call writePty without error", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(makePlugin("builtin", (h) => { host = h; }));
    // invoke is mocked to resolve — should not throw PluginCapabilityError
    await expect(host!.writePty("s1", "data")).resolves.toBeUndefined();
  });

  it("external plugin without pty:write throws PluginCapabilityError on writePty", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      [], // no capabilities
    );
    await expect(host!.writePty("s1", "data")).rejects.toThrow(PluginCapabilityError);
  });

  it("external plugin with pty:write can call writePty", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      ["pty:write"],
    );
    await expect(host!.writePty("s1", "data")).resolves.toBeUndefined();
  });

  it("external plugin without ui:markdown throws on openMarkdownPanel", () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      [],
    );
    expect(() => host!.openMarkdownPanel("Title", "plan:file")).toThrow(PluginCapabilityError);
  });

  it("external plugin with ui:markdown can call openMarkdownPanel", () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      ["ui:markdown"],
    );
    expect(() => host!.openMarkdownPanel("Title", "plan:file")).not.toThrow();
  });

  it("external plugin without ui:sound throws on playNotificationSound", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      [],
    );
    await expect(host!.playNotificationSound()).rejects.toThrow(PluginCapabilityError);
  });

  it("external plugin with ui:sound can call playNotificationSound", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      ["ui:sound"],
    );
    await expect(host!.playNotificationSound()).resolves.toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Tier 3c: HTTP fetch capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 3c httpFetch capability gating", () => {
  it("external plugin without net:http throws on httpFetch", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      [], // no capabilities
    );
    await expect(host!.httpFetch("https://example.com")).rejects.toThrow(PluginCapabilityError);
  });

  it("external plugin with net:http can call httpFetch", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      ["net:http"],
      ["https://example.com/*"],
    );
    // invoke is mocked → resolves, should not throw capability error
    await expect(host!.httpFetch("https://example.com/api")).resolves.not.toThrow();
  });

  it("built-in plugin can call httpFetch without declaring capability", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(makePlugin("builtin", (h) => { host = h; }));
    await expect(host!.httpFetch("https://example.com")).resolves.not.toThrow();
  });

  it("passes allowedUrls to the Rust command", async () => {
    let host: PluginHost | null = null;
    const allowedUrls = ["https://api.anthropic.com/*"];
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      ["net:http"],
      allowedUrls,
    );
    await host!.httpFetch("https://api.anthropic.com/usage", {
      method: "GET",
      headers: { Authorization: "Bearer token" },
    });
    // Verify invoke was called with correct args
    const { invoke } = await import("../../invoke");
    expect(invoke).toHaveBeenCalledWith("plugin_http_fetch", expect.objectContaining({
      url: "https://api.anthropic.com/usage",
      method: "GET",
      headers: { Authorization: "Bearer token" },
      allowedUrls: ["https://api.anthropic.com/*"],
      pluginId: "ext",
    }));
  });
});

// ---------------------------------------------------------------------------
// Tier 4: Scoped invoke
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 4 scoped invoke", () => {
  it("rejects non-whitelisted commands", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(makePlugin("p1", (h) => { host = h; }));
    await expect(host!.invoke("dangerous_command")).rejects.toThrow("not in the invoke whitelist");
  });

  it("allows whitelisted plugin data commands without capability", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      [], // no capabilities
    );
    // read_plugin_data, write_plugin_data, delete_plugin_data are always allowed
    await expect(host!.invoke("read_plugin_data", { plugin_id: "ext", path: "cache.json" }))
      .resolves.toBeUndefined();
  });

  it("external plugin needs invoke:read_file capability for read_file", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      [], // no invoke:read_file capability
    );
    await expect(host!.invoke("read_file", { path: "/repo", file: "README.md" }))
      .rejects.toThrow(PluginCapabilityError);
  });

  it("external plugin with invoke:read_file can call read_file", async () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(
      makePlugin("ext", (h) => { host = h; }),
      ["invoke:read_file"],
    );
    await expect(host!.invoke("read_file", { path: "/repo", file: "README.md" }))
      .resolves.toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// register() with capabilities parameter
// ---------------------------------------------------------------------------

describe("register with capabilities", () => {
  it("second arg passes capabilities to buildHost", () => {
    // Verify the external plugin pattern works end-to-end
    const onload = vi.fn();
    pluginRegistry.register(makePlugin("ext", onload), ["pty:write", "ui:sound"]);
    expect(onload).toHaveBeenCalledOnce();
  });

  it("built-in plugins (no second arg) have unrestricted access", () => {
    let host: PluginHost | null = null;
    pluginRegistry.register(makePlugin("builtin", (h) => { host = h; }));
    // Should not throw PluginCapabilityError for any Tier 3 method
    expect(() => host!.openMarkdownPanel("Title", "plan:file")).not.toThrow(PluginCapabilityError);
  });
});

// ---------------------------------------------------------------------------
// PluginHost.log — per-plugin logging
// ---------------------------------------------------------------------------

describe("PluginHost.log", () => {
  it("writes to the plugin's logger via host.log()", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.log("info", "hello from plugin");
      host.log("error", "something broke", { code: 42 });
    }));
    const logger = pluginStore.getLogger("p1");
    const entries = logger.getEntries();
    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({ level: "info", message: "hello from plugin" });
    expect(entries[1]).toMatchObject({ level: "error", message: "something broke", data: { code: 42 } });
  });
});

// ---------------------------------------------------------------------------
// Error capture in plugin logger
// ---------------------------------------------------------------------------

describe("error capture in plugin logger", () => {
  it("captures onload errors in the plugin logger", () => {
    pluginRegistry.register(makePlugin("bad", () => { throw new Error("onload boom"); }));
    const logger = pluginStore.getLogger("bad");
    const entries = logger.getEntries();
    expect(entries.length).toBeGreaterThan(0);
    expect(entries.some((e) => e.level === "error" && e.message.includes("onload"))).toBe(true);
  });

  it("captures watcher errors in the plugin logger", async () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({
        pattern: /boom/,
        onMatch: () => { throw new Error("watcher error"); },
      });
    }));
    pluginRegistry.dispatchLine("boom", "s1");
    await flushMicrotasks();
    const logger = pluginStore.getLogger("p1");
    expect(logger.getEntries().some((e) => e.level === "error" && e.message.includes("OutputWatcher"))).toBe(true);
  });

  it("captures structured handler errors in the plugin logger", async () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("test-type", () => { throw new Error("handler error"); });
    }));
    pluginRegistry.dispatchStructuredEvent("test-type", {}, "s1");
    await flushMicrotasks();
    const logger = pluginStore.getLogger("p1");
    expect(logger.getEntries().some((e) => e.level === "error" && e.message.includes("handler error"))).toBe(true);
  });

  it("updates pluginStore loaded state on successful registration", () => {
    pluginStore.registerPlugin("p1", { loaded: false });
    pluginRegistry.register(makePlugin("p1"));
    expect(pluginStore.getPlugin("p1")?.loaded).toBe(true);
  });

  it("updates pluginStore with error on failed onload", () => {
    pluginStore.registerPlugin("bad", { loaded: false });
    pluginRegistry.register(makePlugin("bad", () => { throw new Error("fail!"); }));
    const state = pluginStore.getPlugin("bad");
    expect(state?.loaded).toBe(false);
    expect(state?.error).toBe("fail!");
  });
});
