import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock invoke so the MarkdownProvider doesn't need a real Tauri context
vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { createWizStoriesPlugin } from "../../plugins/wizStoriesPlugin";

const mockedInvoke = vi.mocked(invoke);

/** Flush pending queueMicrotask callbacks so deferred dispatch handlers run */
const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

// Inject a controlled storiesDir so tests don't depend on real stores
const TEST_STORIES_DIR = "/test/repo/stories";
function makePlugin() {
  return createWizStoriesPlugin(() => TEST_STORIES_DIR);
}

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  mockedInvoke.mockReset().mockResolvedValue(undefined);
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe("wizStoriesPlugin lifecycle", () => {
  it("registers a 'stories' section on load", () => {
    pluginRegistry.register(makePlugin());
    expect(activityStore.getSections().some((s) => s.id === "stories")).toBe(true);
  });

  it("section has label STORIES, priority 20, canDismissAll false", () => {
    pluginRegistry.register(makePlugin());
    const section = activityStore.getSections().find((s) => s.id === "stories");
    expect(section).toMatchObject({ label: "STORIES", priority: 20, canDismissAll: false });
  });

  it("unregistering removes the 'stories' section", () => {
    const plugin = makePlugin();
    pluginRegistry.register(plugin);
    pluginRegistry.unregister(plugin.id);
    expect(activityStore.getSections().some((s) => s.id === "stories")).toBe(false);
  });

  it("registers a markdown provider for 'stories' scheme", async () => {
    mockedInvoke
      .mockResolvedValueOnce([{ path: "324-9b46-in_progress-P2-slug.md" }])
      .mockResolvedValueOnce("# Story Content");
    pluginRegistry.register(makePlugin());
    const result = await markdownProviderRegistry.resolve(
      `stories:detail?id=324-9b46&dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).not.toBeNull();
  });

  it("unregistering removes the stories markdown provider", async () => {
    const plugin = makePlugin();
    pluginRegistry.register(plugin);
    pluginRegistry.unregister(plugin.id);
    const result = await markdownProviderRegistry.resolve(
      `stories:detail?id=324-9b46&dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// PTY output watcher — status transitions
// ---------------------------------------------------------------------------

describe("STATUS_PATTERN watcher", () => {
  beforeEach(() => {
    pluginRegistry.register(makePlugin());
  });

  it("adds ActivityItem when status transition is detected", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    const items = activityStore.getForSection("stories");
    expect(items).toHaveLength(1);
  });

  it("item id is stable across updates (stories:{id})", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    const item = activityStore.getForSection("stories")[0];
    expect(item.id).toBe("stories:324-9b46");
  });

  it("item subtitle shows story id and new status", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    const item = activityStore.getForSection("stories")[0];
    expect(item.subtitle).toContain("324-9b46");
    expect(item.subtitle).toContain("in_progress");
  });

  it("item contentUri encodes id and dir", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    const item = activityStore.getForSection("stories")[0];
    expect(item.contentUri).toContain("stories:detail");
    expect(item.contentUri).toContain("id=324-9b46");
    expect(item.contentUri).toContain(encodeURIComponent(TEST_STORIES_DIR));
  });

  it("item has an icon (SVG)", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    const item = activityStore.getForSection("stories")[0];
    expect(item.icon).toContain("<svg");
  });

  it("item is dismissible", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    const item = activityStore.getForSection("stories")[0];
    expect(item.dismissible).toBe(true);
  });

  it("second status transition for same id updates, not duplicates", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 in_progress → complete\n",
      "session-1",
    );
    await flushMicrotasks();
    expect(activityStore.getForSection("stories")).toHaveLength(1);
    const item = activityStore.getForSection("stories")[0];
    expect(item.subtitle).toContain("complete");
  });

  it("different story ids create separate items", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    pluginRegistry.processRawOutput(
      "✓ Updated: 325-abcd pending → ready\n",
      "session-1",
    );
    await flushMicrotasks();
    expect(activityStore.getForSection("stories")).toHaveLength(2);
  });

  it("does not match unrelated PTY lines", async () => {
    pluginRegistry.processRawOutput("Running npm test...\n", "session-1");
    pluginRegistry.processRawOutput("All tests passed\n", "session-1");
    await flushMicrotasks();
    expect(activityStore.getForSection("stories")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// PTY output watcher — worklog
// ---------------------------------------------------------------------------

describe("WORKLOG_PATTERN watcher", () => {
  beforeEach(() => {
    pluginRegistry.register(makePlugin());
  });

  it("adds ActivityItem when worklog is detected", async () => {
    pluginRegistry.processRawOutput(
      "✓ Added worklog to 324-9b46: Starting implementation\n",
      "session-1",
    );
    await flushMicrotasks();
    const items = activityStore.getForSection("stories");
    expect(items).toHaveLength(1);
  });

  it("worklog for existing story updates the item", async () => {
    pluginRegistry.processRawOutput(
      "✓ Updated: 324-9b46 ready → in_progress\n",
      "session-1",
    );
    await flushMicrotasks();
    pluginRegistry.processRawOutput(
      "✓ Added worklog to 324-9b46: Making progress\n",
      "session-1",
    );
    await flushMicrotasks();
    expect(activityStore.getForSection("stories")).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// MarkdownProvider content
// ---------------------------------------------------------------------------

describe("wizStories MarkdownProvider", () => {
  beforeEach(() => {
    pluginRegistry.register(makePlugin());
  });

  it("finds story file via list_markdown_files and reads it", async () => {
    mockedInvoke
      .mockResolvedValueOnce([
        { path: "324-9b46-in_progress-P2-wiz-stories-slug.md", git_status: "" },
      ])
      .mockResolvedValueOnce("# Story Content\n\nDetails here.");

    const result = await markdownProviderRegistry.resolve(
      `stories:detail?id=324-9b46&dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).toBe("# Story Content\n\nDetails here.");
    expect(mockedInvoke).toHaveBeenCalledWith("list_markdown_files", { path: TEST_STORIES_DIR });
    expect(mockedInvoke).toHaveBeenCalledWith("read_file", {
      path: TEST_STORIES_DIR,
      file: "324-9b46-in_progress-P2-wiz-stories-slug.md",
    });
  });

  it("returns null when id param is missing", async () => {
    const result = await markdownProviderRegistry.resolve(
      `stories:detail?dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).toBeNull();
  });

  it("returns null when dir param is missing", async () => {
    const result = await markdownProviderRegistry.resolve("stories:detail?id=324-9b46");
    expect(result).toBeNull();
  });

  it("returns null when no matching file found", async () => {
    mockedInvoke.mockResolvedValueOnce([
      { path: "999-zzzz-ready-P1-other-story.md", git_status: "" },
    ]);
    const result = await markdownProviderRegistry.resolve(
      `stories:detail?id=324-9b46&dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).toBeNull();
  });

  it("returns null when list_markdown_files throws", async () => {
    mockedInvoke.mockRejectedValueOnce(new Error("dir not found"));
    const result = await markdownProviderRegistry.resolve(
      `stories:detail?id=324-9b46&dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).toBeNull();
  });

  it("returns null when read_file throws", async () => {
    mockedInvoke
      .mockResolvedValueOnce([{ path: "324-9b46-in_progress-P2-slug.md", git_status: "" }])
      .mockRejectedValueOnce(new Error("read error"));
    const result = await markdownProviderRegistry.resolve(
      `stories:detail?id=324-9b46&dir=${encodeURIComponent(TEST_STORIES_DIR)}`,
    );
    expect(result).toBeNull();
  });
});
