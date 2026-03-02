import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

// Mock invoke so the MarkdownProvider doesn't need a real Tauri context
vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { mdTabsStore } from "../../stores/mdTabs";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { planPlugin } from "../../plugins/planPlugin";

const mockedInvoke = vi.mocked(invoke);

/** Flush pending queueMicrotask callbacks so deferred dispatch handlers run */
const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

/** Create a terminal entry in terminalsStore with the given sessionId and cwd */
function addTerminalWithSession(sessionId: string, cwd: string): string {
  const id = terminalsStore.add({ sessionId, cwd, name: "test", awaitingInput: null, fontSize: 14 });
  return id;
}

/** Remove all terminals from the store */
function clearTerminals(): void {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
}

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  mdTabsStore.clearAll();
  clearTerminals();
  mockedInvoke.mockReset().mockResolvedValue(undefined);
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe("planPlugin lifecycle", () => {
  it("registers a 'plan' section on load", () => {
    pluginRegistry.register(planPlugin);
    expect(activityStore.getSections().some((s) => s.id === "plan")).toBe(true);
  });

  it("unregistering removes the 'plan' section", () => {
    pluginRegistry.register(planPlugin);
    pluginRegistry.unregister("plan");
    expect(activityStore.getSections().some((s) => s.id === "plan")).toBe(false);
  });

  it("registers a plan markdown provider on load", async () => {
    mockedInvoke.mockResolvedValue("# Plan Content");
    pluginRegistry.register(planPlugin);
    // Provider is registered for the "plan" scheme
    const result = await markdownProviderRegistry.resolve("plan:file?path=/foo/bar.md");
    expect(result).not.toBeNull();
    expect(mockedInvoke).toHaveBeenCalledWith("plugin_read_file", { path: "/foo/bar.md", pluginId: "plan" });
  });

  it("unregistering removes the plan markdown provider", async () => {
    pluginRegistry.register(planPlugin);
    pluginRegistry.unregister("plan");
    const result = await markdownProviderRegistry.resolve("plan:file?path=/foo/bar.md");
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// plan-file event handling
// ---------------------------------------------------------------------------

describe("plan-file structured event", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("adds an ActivityItem to activityStore on plan-file event", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    const items = activityStore.getForSection("plan");
    expect(items).toHaveLength(1);
    expect(items[0].sectionId).toBe("plan");
    expect(items[0].contentUri).toBe("plan:file?path=%2Frepo%2Fplans%2Ffoo.md");
    expect(items[0].dismissible).toBe(true);
  });

  it("item title is the plan display name (basename without extension)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/my-cool-plan.md" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.title).toBe("my-cool-plan");
  });

  it("item subtitle is the full path", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.subtitle).toBe("/repo/plans/foo.md");
  });

  it("deduplicates: second event with same path updates, not adds", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("plan")).toHaveLength(1);
  });

  it("different paths create separate items", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/a.md" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/b.md" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("plan")).toHaveLength(2);
  });

  it("item has an icon (non-empty SVG string)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/foo.md" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.icon).toContain("<svg");
  });

  it("ignores null payload", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", null, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("plan")).toHaveLength(0);
  });

  it("ignores payload with non-string path", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: 42 }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("plan")).toHaveLength(0);
  });

  it("ignores payload without path property", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { file: "/foo.md" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("plan")).toHaveLength(0);
  });

  it("ignores string payload (not object)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", "/foo.md", "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("plan")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// repoPath attachment and path resolution
// ---------------------------------------------------------------------------

describe("plan-file repoPath and path resolution", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("attaches repoPath derived from session CWD to the activity item", async () => {
    addTerminalWithSession("sess-1", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/Users/dev/my-project/plans/foo.md" }, "sess-1");
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.repoPath).toBeDefined();
  });

  it("resolves relative plan path to absolute using session CWD", async () => {
    addTerminalWithSession("sess-2", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "plans/feature.md" }, "sess-2");
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.subtitle).toBe("/Users/dev/my-project/plans/feature.md");
    expect(item.contentUri).toContain(encodeURIComponent("/Users/dev/my-project/plans/feature.md"));
  });

  it("keeps absolute path unchanged", async () => {
    addTerminalWithSession("sess-3", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/absolute/plans/bar.md" }, "sess-3");
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.subtitle).toBe("/absolute/plans/bar.md");
  });

  it("rebuilds planItemIds from activityStore on re-register (simulates hydrate)", async () => {
    // Simulate hydrated items by adding directly to activityStore
    activityStore.addItem({
      id: "plan:/repo/plans/old.md",
      pluginId: "plan",
      sectionId: "plan",
      title: "old",
      icon: "<svg/>",
      dismissible: true,
      repoPath: "/repo",
    });
    // Register the plugin — it should rebuild planItemIds from existing items
    pluginRegistry.register(planPlugin);
    // Add MAX_PLAN_ITEMS more — the old one should be evicted
    for (let i = 1; i <= 3; i++) {
      pluginRegistry.dispatchStructuredEvent("plan-file", { path: `/repo/plans/new-${i}.md` }, "sess-x");
      await flushMicrotasks();
    }
    const items = activityStore.getForSection("plan");
    // Should have at most MAX_PLAN_ITEMS (3), the old one evicted
    expect(items.length).toBeLessThanOrEqual(3);
    expect(items.find((i) => i.id === "plan:/repo/plans/old.md")).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// MarkdownProvider content
// ---------------------------------------------------------------------------

describe("plan MarkdownProvider", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("reads file content via plugin_read_file invoke", async () => {
    mockedInvoke.mockResolvedValue("# Hello Plan");
    const result = await markdownProviderRegistry.resolve("plan:file?path=%2Frepo%2Fplan.md");
    expect(result).toBe("# Hello Plan");
    expect(mockedInvoke).toHaveBeenCalledWith("plugin_read_file", { path: "/repo/plan.md", pluginId: "plan" });
  });

  it("strips frontmatter from rendered content", async () => {
    mockedInvoke.mockResolvedValue("---\ntitle: My Plan\nstatus: draft\n---\n# Plan Content\nBody here");
    const result = await markdownProviderRegistry.resolve("plan:file?path=%2Frepo%2Fplan.md");
    expect(result).not.toContain("---");
    expect(result).toContain("# Plan Content");
    expect(result).toContain("Body here");
  });

  it("returns full content when no frontmatter present", async () => {
    mockedInvoke.mockResolvedValue("# No Frontmatter\nJust content");
    const result = await markdownProviderRegistry.resolve("plan:file?path=%2Frepo%2Fplan.md");
    expect(result).toBe("# No Frontmatter\nJust content");
  });

  it("returns null when path query param is missing", async () => {
    const result = await markdownProviderRegistry.resolve("plan:file");
    expect(result).toBeNull();
  });

  it("returns null when invoke throws", async () => {
    mockedInvoke.mockRejectedValue(new Error("file not found"));
    const result = await markdownProviderRegistry.resolve("plan:file?path=%2Ffoo%2Fbar.md");
    expect(result).toBeNull();
  });

  it("returns null for path traversal attempts", async () => {
    const result = await markdownProviderRegistry.resolve("plan:file?path=%2Ffoo%2F..%2F..%2Fetc%2Fpasswd");
    expect(result).toBeNull();
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Metadata enrichment
// ---------------------------------------------------------------------------

describe("plan-file metadata enrichment", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("enriches item title with H1 from file content", async () => {
    mockedInvoke.mockResolvedValue("# Implementation Plan: My Feature\n\n**Status:** Draft\n**Estimated Effort:** M");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/my-feature.md" }, "s1");
    await flushMicrotasks();
    // Wait for the async enrichment
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.title).toBe("My Feature");
  });

  it("populates metadata with status, effort, priority, story", async () => {
    mockedInvoke.mockResolvedValue("# Plan: Cool Feature\n\n**Status:** In Progress\n**Estimated Effort:** L-XL\n**Priority:** P1\n**Story:** 420-e0ea");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/cool.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.metadata).toEqual({
      status: "In Progress",
      effort: "L-XL",
      priority: "P1",
      story: "420-e0ea",
    });
  });

  it("uses YAML frontmatter status over inline status", async () => {
    mockedInvoke.mockResolvedValue("---\nstatus: completed\n---\n# Plan\n\n**Status:** Draft\n**Estimated Effort:** S");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/done.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.metadata?.status).toBe("completed");
  });

  it("falls back to basename when file has no H1", async () => {
    mockedInvoke.mockResolvedValue("No heading here\n\n**Status:** Draft");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/orphan.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.title).toBe("orphan");
  });

  it("keeps basename title when file read fails", async () => {
    mockedInvoke.mockRejectedValue(new Error("file not found"));
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/missing.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = activityStore.getForSection("plan")[0];
    expect(item.title).toBe("missing");
  });

  it("enriches hydrated items on plugin load", async () => {
    // Add an item without metadata (simulating hydration from disk)
    activityStore.addItem({
      id: "plan:/repo/plans/old.md",
      pluginId: "plan",
      sectionId: "plan",
      title: "old",
      subtitle: "/repo/plans/old.md",
      icon: "<svg/>",
      dismissible: true,
      repoPath: "/repo",
    });
    // Route plugin_read_file to return plan content, all others resolve normally
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "plugin_read_file") return Promise.resolve("# Implementation Plan: Old Feature\n\n**Status:** Draft\n**Estimated Effort:** S");
      return Promise.resolve(undefined);
    });

    pluginRegistry.register(planPlugin);
    await flushMicrotasks();
    await flushMicrotasks();

    const item = activityStore.getForSection("plan").find((i) => i.id === "plan:/repo/plans/old.md");
    expect(item?.title).toBe("Old Feature");
    expect(item?.metadata?.status).toBe("Draft");
    expect(item?.metadata?.effort).toBe("S");
  });
});

// ---------------------------------------------------------------------------
// Auto-open new plan in markdown tab
// ---------------------------------------------------------------------------

describe("plan auto-open", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
    repositoriesStore.add({ path: "/repo", displayName: "repo" });
    repositoriesStore.setActive("/repo");
  });

  afterEach(() => {
    repositoriesStore.setActive(null);
  });

  it("auto-opens a new plan as a background virtual tab", async () => {
    addTerminalWithSession("sess-auto", "/repo");
    const spy = vi.spyOn(mdTabsStore, "addVirtualBackground");

    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/new.md" }, "sess-auto");
    await flushMicrotasks();

    expect(spy).toHaveBeenCalledWith("new", "plan:file?path=%2Frepo%2Fplans%2Fnew.md");
    spy.mockRestore();
  });

  it("does NOT auto-open the same plan on repeated detection", async () => {
    addTerminalWithSession("sess-repeat", "/repo");
    const spy = vi.spyOn(mdTabsStore, "addVirtualBackground");

    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/dup.md" }, "sess-repeat");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/dup.md" }, "sess-repeat");
    await flushMicrotasks();

    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it("does NOT auto-open when plan is for a different repo", async () => {
    addTerminalWithSession("sess-other", "/other-repo");
    const spy = vi.spyOn(mdTabsStore, "addVirtualBackground");

    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/other-repo/plans/x.md" }, "sess-other");
    await flushMicrotasks();

    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});
