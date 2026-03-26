import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

// Mock invoke so the MarkdownProvider doesn't need a real Tauri context
vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { sidebarPluginStore } from "../../stores/sidebarPluginStore";
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

/** Get plan items from the sidebar panel */
function getPlanItems() {
  const panels = sidebarPluginStore.getPanels();
  const planPanel = panels.find((p) => p.pluginId === "plan");
  return planPanel?.items ?? [];
}

/** Get plan panel from sidebar store */
function getPlanPanel() {
  return sidebarPluginStore.getPanels().find((p) => p.pluginId === "plan");
}

beforeEach(() => {
  pluginRegistry.clear();
  sidebarPluginStore.clear();
  markdownProviderRegistry.clear();
  mdTabsStore.clearAll();
  clearTerminals();
  mockedInvoke.mockReset().mockResolvedValue(undefined);
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
  repositoriesStore.setActive(null);
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe("planPlugin lifecycle", () => {
  it("registers a sidebar panel on load", () => {
    pluginRegistry.register(planPlugin);
    expect(getPlanPanel()).toBeDefined();
    expect(getPlanPanel()!.label).toBe("ACTIVE PLANS");
  });

  it("unregistering removes the sidebar panel", () => {
    pluginRegistry.register(planPlugin);
    pluginRegistry.unregister("plan");
    expect(getPlanPanel()).toBeUndefined();
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

  it("adds an item to sidebar panel on plan-file event", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    const items = getPlanItems();
    expect(items).toHaveLength(1);
    expect(items[0].id).toBe("plan:/repo/plans/foo.md");
  });

  it("item label is the plan display name (basename without extension)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/my-cool-plan.md" }, "s1");
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.label).toBe("my-cool-plan");
  });

  it("item subtitle is the full path initially", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.subtitle).toBe("/repo/plans/foo.md");
  });

  it("deduplicates: second event with same path does not duplicate", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(1);
  });

  it("different paths create separate items", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/a.md" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/b.md" }, "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(2);
  });

  it("item has an icon (non-empty SVG string)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/foo.md" }, "s1");
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.icon).toContain("<svg");
  });

  it("ignores null payload", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", null, "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(0);
  });

  it("ignores payload with non-string path", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: 42 }, "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(0);
  });

  it("ignores payload without path property", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { file: "/foo.md" }, "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(0);
  });

  it("ignores string payload (not object)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", "/foo.md", "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(0);
  });

  it("updates badge count when items are added", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/a.md" }, "s1");
    await flushMicrotasks();
    expect(getPlanPanel()!.badge).toBe("1");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/b.md" }, "s1");
    await flushMicrotasks();
    expect(getPlanPanel()!.badge).toBe("2");
  });
});

// ---------------------------------------------------------------------------
// repoPath attachment and path resolution
// ---------------------------------------------------------------------------

describe("plan-file repoPath and path resolution", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("resolves relative plan path to absolute using session CWD", async () => {
    addTerminalWithSession("sess-2", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "plans/feature.md" }, "sess-2");
    await flushMicrotasks();
    const item = getPlanItems()[0];
    // Subtitle shows the full path before enrichment replaces it
    expect(item.subtitle).toBe("/Users/dev/my-project/plans/feature.md");
  });

  it("keeps absolute path unchanged", async () => {
    addTerminalWithSession("sess-3", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/absolute/plans/bar.md" }, "sess-3");
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.subtitle).toBe("/absolute/plans/bar.md");
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
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.label).toBe("My Feature");
  });

  it("populates subtitle with status and effort after enrichment", async () => {
    mockedInvoke.mockResolvedValue("# Plan: Cool Feature\n\n**Status:** In Progress\n**Estimated Effort:** L-XL\n**Priority:** P1\n**Story:** 420-e0ea");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/cool.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.subtitle).toBe("In Progress · L-XL");
  });

  it("uses YAML frontmatter status over inline status", async () => {
    mockedInvoke.mockResolvedValue("---\nstatus: completed\n---\n# Plan\n\n**Status:** Draft\n**Estimated Effort:** S");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/done.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.subtitle).toContain("completed");
  });

  it("falls back to basename when file has no H1", async () => {
    mockedInvoke.mockResolvedValue("No heading here\n\n**Status:** Draft");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/orphan.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.label).toBe("orphan");
  });

  it("keeps basename title when file read fails", async () => {
    mockedInvoke.mockRejectedValue(new Error("file not found"));
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/missing.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const item = getPlanItems()[0];
    expect(item.label).toBe("missing");
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
    for (const path of repositoriesStore.getPaths()) {
      repositoriesStore.remove(path);
    }
  });

  it("auto-opens a new plan as a background virtual tab", async () => {
    addTerminalWithSession("sess-auto", "/repo");
    const spy = vi.spyOn(mdTabsStore, "addVirtualBackground");

    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/new.md" }, "sess-auto");
    await flushMicrotasks();

    expect(spy).toHaveBeenCalledWith("new", "plan:file?path=%2Frepo%2Fplans%2Fnew.md", "/repo");
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

// ---------------------------------------------------------------------------
// Session-to-repo filtering (via host.getSessionCwd / host.getActiveRepoPath)
// ---------------------------------------------------------------------------

describe("plan-file session filtering", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("shows plan when no active repo (pass-through)", async () => {
    const termId = terminalsStore.add({ sessionId: "s1", fontSize: 14, name: "T", cwd: "/some/path", awaitingInput: null });
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/some/path/plan.md" }, "s1");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(1);
    terminalsStore.remove(termId);
  });

  it("shows plan when session cwd starts with active repo path", async () => {
    repositoriesStore.add({ path: "/my/repo", displayName: "repo" });
    repositoriesStore.setActive("/my/repo");
    const termId = terminalsStore.add({ sessionId: "s-repo", fontSize: 14, name: "T", cwd: "/my/repo/subdir", awaitingInput: null });
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/my/repo/plan.md" }, "s-repo");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(1);
    terminalsStore.remove(termId);
    repositoriesStore.setActive(null);
    repositoriesStore.remove("/my/repo");
  });

  it("hides plan when session cwd does not belong to active repo", async () => {
    repositoriesStore.add({ path: "/my/repo", displayName: "repo" });
    repositoriesStore.setActive("/my/repo");
    const termId = terminalsStore.add({ sessionId: "s-other", fontSize: 14, name: "T", cwd: "/other/project", awaitingInput: null });
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/other/project/plan.md" }, "s-other");
    await flushMicrotasks();
    expect(getPlanItems()).toHaveLength(0);
    terminalsStore.remove(termId);
    repositoriesStore.setActive(null);
    repositoriesStore.remove("/my/repo");
  });
});
