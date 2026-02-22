import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock invoke so the MarkdownProvider doesn't need a real Tauri context
vi.mock("../../invoke", () => ({
  invoke: vi.fn(),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { planPlugin } from "../../plugins/planPlugin";

const mockedInvoke = vi.mocked(invoke);

/** Flush pending queueMicrotask callbacks so deferred dispatch handlers run */
const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  mockedInvoke.mockReset();
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
    expect(mockedInvoke).toHaveBeenCalledWith("read_file", { path: "/foo", file: "bar.md" });
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
// MarkdownProvider content
// ---------------------------------------------------------------------------

describe("plan MarkdownProvider", () => {
  beforeEach(() => {
    pluginRegistry.register(planPlugin);
  });

  it("reads file content via read_file invoke", async () => {
    mockedInvoke.mockResolvedValue("# Hello Plan");
    const result = await markdownProviderRegistry.resolve("plan:file?path=%2Frepo%2Fplan.md");
    expect(result).toBe("# Hello Plan");
    expect(mockedInvoke).toHaveBeenCalledWith("read_file", { path: "/repo", file: "plan.md" });
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
