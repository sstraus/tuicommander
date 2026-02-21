import { describe, it, expect, beforeEach, vi } from "vitest";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import type { TuiPlugin, PluginHost, OutputWatcher } from "../../plugins/types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
  it("calls matching watcher with match and sessionId", () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello (\w+)/, onMatch });
    }));
    pluginRegistry.dispatchLine("hello world", "session-1");
    expect(onMatch).toHaveBeenCalledOnce();
    expect(onMatch.mock.calls[0][0][1]).toBe("world");
    expect(onMatch.mock.calls[0][1]).toBe("session-1");
  });

  it("does not call watcher when pattern does not match", () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    pluginRegistry.dispatchLine("goodbye world", "s1");
    expect(onMatch).not.toHaveBeenCalled();
  });

  it("resets lastIndex on global regex before each test", () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /foo/g, onMatch });
    }));
    pluginRegistry.dispatchLine("foo bar", "s1");
    pluginRegistry.dispatchLine("foo bar", "s1");
    expect(onMatch).toHaveBeenCalledTimes(2);
  });

  it("catches and does not rethrow watcher exceptions", () => {
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({
        pattern: /anything/,
        onMatch: () => { throw new Error("watcher boom"); },
      });
    }));
    expect(() => pluginRegistry.dispatchLine("anything", "s1")).not.toThrow();
  });

  it("continues dispatching to other watchers after one throws", () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({
        pattern: /anything/,
        onMatch: () => { throw new Error("boom"); },
      });
      host.registerOutputWatcher({ pattern: /anything/, onMatch });
    }));
    pluginRegistry.dispatchLine("anything", "s1");
    expect(onMatch).toHaveBeenCalledOnce();
  });

  it("unregistering plugin removes its watchers", () => {
    const onMatch = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerOutputWatcher({ pattern: /hello/, onMatch });
    }));
    pluginRegistry.unregister("p1");
    pluginRegistry.dispatchLine("hello", "s1");
    expect(onMatch).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Structured event dispatch
// ---------------------------------------------------------------------------

describe("dispatchStructuredEvent", () => {
  it("calls handler for matching type", () => {
    const handler = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", handler);
    }));
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/foo.md" }, "s1");
    expect(handler).toHaveBeenCalledWith({ path: "/foo.md" }, "s1");
  });

  it("does not call handler for non-matching type", () => {
    const handler = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", handler);
    }));
    pluginRegistry.dispatchStructuredEvent("rate-limit", {}, "s1");
    expect(handler).not.toHaveBeenCalled();
  });

  it("calls all handlers registered for the same type", () => {
    const h1 = vi.fn();
    const h2 = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", h1);
    }));
    pluginRegistry.register(makePlugin("p2", (host) => {
      host.registerStructuredEventHandler("plan-file", h2);
    }));
    pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
    expect(h1).toHaveBeenCalledOnce();
    expect(h2).toHaveBeenCalledOnce();
  });

  it("unregistering plugin removes its structured event handlers", () => {
    const handler = vi.fn();
    pluginRegistry.register(makePlugin("p1", (host) => {
      host.registerStructuredEventHandler("plan-file", handler);
    }));
    pluginRegistry.unregister("p1");
    pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
    expect(handler).not.toHaveBeenCalled();
  });
});
