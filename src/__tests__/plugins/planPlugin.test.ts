import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

// Mock invoke so the MarkdownProvider doesn't need a real Tauri context
vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
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
  return terminalsStore.add({ sessionId, cwd, name: "test", awaitingInput: null, fontSize: 14 });
}

/** Get the internal plans map from the plugin instance */
function getPlans() {
  return (planPlugin as unknown as { getPlans(): Map<string, unknown> }).getPlans();
}

beforeEach(() => {
  pluginRegistry.clear();
  markdownProviderRegistry.clear();
  mdTabsStore.clearAll();
  for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
  mockedInvoke.mockReset().mockResolvedValue(undefined);
  repositoriesStore.setActive(null);
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe("planPlugin lifecycle", () => {
  it("clears plans on unload", async () => {
    pluginRegistry.register(planPlugin);
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(1);
    pluginRegistry.unregister("plan");
    expect(getPlans().size).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// plan-file event handling
// ---------------------------------------------------------------------------

describe("plan-file structured event", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("adds a plan to internal tracking on plan-file event", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(1);
  });

  it("deduplicates: second event with same path does not duplicate", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/foo.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(1);
  });

  it("different paths create separate entries", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/a.md" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/plans/b.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(2);
  });

  it("ignores null payload", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", null, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(0);
  });

  it("ignores payload with non-string path", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: 42 }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(0);
  });

  it("ignores payload without path property", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { file: "/foo.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(0);
  });

  it("ignores string payload (not object)", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", "/foo.md", "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

describe("plan-file path resolution", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("resolves relative plan path to absolute using session CWD", async () => {
    addTerminalWithSession("sess-2", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "plans/feature.md" }, "sess-2");
    await flushMicrotasks();
    expect(getPlans().has("/Users/dev/my-project/plans/feature.md")).toBe(true);
  });

  it("keeps absolute path unchanged", async () => {
    addTerminalWithSession("sess-3", "/Users/dev/my-project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/absolute/plans/bar.md" }, "sess-3");
    await flushMicrotasks();
    expect(getPlans().has("/absolute/plans/bar.md")).toBe(true);
  });

  it("resolves relative path with Windows CWD", async () => {
    addTerminalWithSession("sess-win", "C:\\DATA\\repos\\arcane");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "plans/feature.md" }, "sess-win");
    await flushMicrotasks();
    expect(getPlans().has("C:\\DATA\\repos\\arcane/plans/feature.md")).toBe(true);
  });

  it("keeps Windows absolute path unchanged", async () => {
    addTerminalWithSession("sess-win2", "C:\\DATA\\repos\\arcane");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "C:\\DATA\\repos\\arcane\\plans\\bar.md" }, "sess-win2");
    await flushMicrotasks();
    expect(getPlans().has("C:\\DATA\\repos\\arcane\\plans\\bar.md")).toBe(true);
  });

  it("does NOT skip Windows absolute paths as unresolved", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "D:\\plans\\orphan.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Metadata enrichment
// ---------------------------------------------------------------------------

describe("plan-file metadata enrichment", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("enriches entry title with H1 from file content", async () => {
    mockedInvoke.mockResolvedValue("# Implementation Plan: My Feature\n\n**Status:** Draft");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/my-feature.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const entry = getPlans().get("/repo/plans/my-feature.md") as { title: string };
    expect(entry.title).toBe("My Feature");
  });

  it("extracts status and effort from content", async () => {
    mockedInvoke.mockResolvedValue("---\nstatus: in_progress\n---\n# Plan\n\n**Estimated Effort:** L-XL");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/cool.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const entry = getPlans().get("/repo/plans/cool.md") as { status: string; effort: string };
    expect(entry.status).toBe("in_progress");
    expect(entry.effort).toBe("L-XL");
  });

  it("falls back to basename when file has no H1", async () => {
    mockedInvoke.mockResolvedValue("No heading here");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/orphan.md" }, "s1");
    await flushMicrotasks();
    await flushMicrotasks();
    const entry = getPlans().get("/repo/plans/orphan.md") as { title: string };
    expect(entry.title).toBe("orphan");
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
    for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
  });

  it("auto-opens a new plan as a background file tab (uses the markdown editor, not a read-only virtual view)", async () => {
    addTerminalWithSession("sess-auto", "/repo");
    const spy = vi.spyOn(mdTabsStore, "addFileBackground");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/new.md" }, "sess-auto");
    await flushMicrotasks();
    expect(spy).toHaveBeenCalledWith("/repo", "plans/new.md");
    spy.mockRestore();
  });

  it("does NOT auto-open the same plan on repeated detection", async () => {
    addTerminalWithSession("sess-repeat", "/repo");
    const spy = vi.spyOn(mdTabsStore, "addFileBackground");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/dup.md" }, "sess-repeat");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/repo/plans/dup.md" }, "sess-repeat");
    await flushMicrotasks();
    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it("does NOT auto-open when plan is for a different repo", async () => {
    addTerminalWithSession("sess-other", "/other-repo");
    const spy = vi.spyOn(mdTabsStore, "addFileBackground");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/other-repo/plans/x.md" }, "sess-other");
    await flushMicrotasks();
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// Session-to-repo filtering
// ---------------------------------------------------------------------------

describe("plan-file session filtering", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("accepts plan when no active repo (pass-through)", async () => {
    addTerminalWithSession("s1", "/some/path");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/some/path/plan.md" }, "s1");
    await flushMicrotasks();
    expect(getPlans().size).toBe(1);
  });

  it("accepts plan when session cwd starts with active repo path", async () => {
    repositoriesStore.add({ path: "/my/repo", displayName: "repo" });
    repositoriesStore.setActive("/my/repo");
    addTerminalWithSession("s-repo", "/my/repo/subdir");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/my/repo/plan.md" }, "s-repo");
    await flushMicrotasks();
    expect(getPlans().size).toBe(1);
    repositoriesStore.setActive(null);
    repositoriesStore.remove("/my/repo");
  });

  it("rejects plan when session cwd does not belong to active repo", async () => {
    repositoriesStore.add({ path: "/my/repo", displayName: "repo" });
    repositoriesStore.setActive("/my/repo");
    addTerminalWithSession("s-other", "/other/project");
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/other/project/plan.md" }, "s-other");
    await flushMicrotasks();
    expect(getPlans().size).toBe(0);
    repositoriesStore.setActive(null);
    repositoriesStore.remove("/my/repo");
  });
});
