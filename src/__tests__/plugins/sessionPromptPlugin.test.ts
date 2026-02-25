import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock invoke so the MarkdownProvider doesn't need a real Tauri context
vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { sessionPromptPlugin } from "../../plugins/sessionPromptPlugin";

/** Flush pending queueMicrotask callbacks so deferred dispatch handlers run */
const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe("sessionPromptPlugin lifecycle", () => {
  it("registers a 'session-prompts' section on load", () => {
    pluginRegistry.register(sessionPromptPlugin);
    expect(activityStore.getSections().some((s) => s.id === "session-prompts")).toBe(true);
  });

  it("section has correct label", () => {
    pluginRegistry.register(sessionPromptPlugin);
    const section = activityStore.getSections().find((s) => s.id === "session-prompts");
    expect(section?.label).toBe("USER PROMPTS");
  });

  it("section supports dismiss all", () => {
    pluginRegistry.register(sessionPromptPlugin);
    const section = activityStore.getSections().find((s) => s.id === "session-prompts");
    expect(section?.canDismissAll).toBe(true);
  });

  it("unregistering removes the section", () => {
    pluginRegistry.register(sessionPromptPlugin);
    pluginRegistry.unregister("session-prompts");
    expect(activityStore.getSections().some((s) => s.id === "session-prompts")).toBe(false);
  });

  it("registers a session-prompt markdown provider on load", () => {
    pluginRegistry.register(sessionPromptPlugin);
    // Provider is registered for the "session-prompt" scheme
    // We'll verify it works in the MarkdownProvider tests below
  });

  it("unregistering removes the markdown provider", async () => {
    pluginRegistry.register(sessionPromptPlugin);
    pluginRegistry.unregister("session-prompts");
    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=0");
    expect(result).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// user-input event handling
// ---------------------------------------------------------------------------

describe("user-input structured event", () => {
  beforeEach(() => {
    pluginRegistry.register(sessionPromptPlugin);
  });

  it("adds an ActivityItem on user-input event", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "hello world" }, "s1");
    await flushMicrotasks();
    const items = activityStore.getForSection("session-prompts");
    expect(items).toHaveLength(1);
    expect(items[0].sectionId).toBe("session-prompts");
    expect(items[0].dismissible).toBe(true);
  });

  it("item title is the truncated prompt content", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "fix the auth bug" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("session-prompts")[0];
    expect(item.title).toBe("fix the auth bug");
  });

  it("long prompts are truncated with ellipsis", async () => {
    const longContent = "a".repeat(100);
    pluginRegistry.dispatchStructuredEvent("user-input", { content: longContent }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("session-prompts")[0];
    expect(item.title.length).toBeLessThanOrEqual(61); // 60 chars + ellipsis
    expect(item.title).toContain("\u2026");
  });

  it("multiline content is collapsed to single line in title", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "line one\nline two\nline three" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("session-prompts")[0];
    expect(item.title).not.toContain("\n");
    expect(item.title).toBe("line one line two line three");
  });

  it("item has an icon (SVG)", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "test" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("session-prompts")[0];
    expect(item.icon).toContain("<svg");
  });

  it("item has a subtitle with time", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "test" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("session-prompts")[0];
    expect(item.subtitle).toBeDefined();
    // Should contain time-like format (digits and colons)
    expect(item.subtitle).toMatch(/\d/);
  });

  it("item has a contentUri for markdown resolution", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "test" }, "s1");
    await flushMicrotasks();
    const item = activityStore.getForSection("session-prompts")[0];
    expect(item.contentUri).toMatch(/^session-prompt:entry\?idx=\d+$/);
  });

  it("multiple prompts create separate items", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "first" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "second" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(2);
  });

  it("prompts from different sessions are tracked", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "from session 1" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "from session 2" }, "s2");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(2);
  });

  it("evicts oldest items beyond limit (10)", async () => {
    for (let i = 0; i < 15; i++) {
      pluginRegistry.dispatchStructuredEvent("user-input", { content: `prompt ${i}` }, "s1");
      await flushMicrotasks();
    }
    const items = activityStore.getForSection("session-prompts");
    expect(items.length).toBeLessThanOrEqual(10);
  });

  // -----------------------------------------------------------------------
  // Payload validation
  // -----------------------------------------------------------------------

  it("ignores null payload", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", null, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  it("ignores string payload (not object)", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", "just a string", "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  it("ignores payload with non-string content", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: 42 }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  it("ignores payload without content property", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { text: "hello" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  it("ignores empty/whitespace-only content", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "   " }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  it("ignores empty string content", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  // -----------------------------------------------------------------------
  // Does not respond to other event types
  // -----------------------------------------------------------------------

  it("does not respond to plan-file events", async () => {
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/foo/bar.md" }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });

  it("does not respond to rate-limit events", async () => {
    pluginRegistry.dispatchStructuredEvent("rate-limit", { pattern_name: "test", matched_text: "test", retry_after_ms: null }, "s1");
    await flushMicrotasks();
    expect(activityStore.getForSection("session-prompts")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// MarkdownProvider content
// ---------------------------------------------------------------------------

describe("session-prompt MarkdownProvider", () => {
  beforeEach(() => {
    pluginRegistry.register(sessionPromptPlugin);
  });

  it("returns formatted markdown for a valid prompt entry", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "refactor auth" }, "abcdef12-3456-7890");
    await flushMicrotasks();

    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=0");
    expect(result).not.toBeNull();
    expect(result).toContain("# User Prompt");
    expect(result).toContain("refactor auth");
    expect(result).toContain("abcdef12"); // First 8 chars of session ID
  });

  it("returns null when idx query param is missing", async () => {
    const result = await markdownProviderRegistry.resolve("session-prompt:entry");
    expect(result).toBeNull();
  });

  it("returns null for out-of-bounds idx", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "test" }, "s1");
    await flushMicrotasks();

    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=99");
    expect(result).toBeNull();
  });

  it("returns null for negative idx", async () => {
    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=-1");
    expect(result).toBeNull();
  });

  it("returns null for non-numeric idx", async () => {
    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=abc");
    expect(result).toBeNull();
  });

  it("markdown contains code block with prompt content", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "git status --short" }, "s1");
    await flushMicrotasks();

    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=0");
    expect(result).toContain("```");
    expect(result).toContain("git status --short");
  });

  it("markdown contains timestamp", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "test" }, "s1");
    await flushMicrotasks();

    const result = await markdownProviderRegistry.resolve("session-prompt:entry?idx=0");
    expect(result).toContain("**Time:**");
  });

  it("resolves correct entry by index", async () => {
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "first prompt" }, "s1");
    await flushMicrotasks();
    pluginRegistry.dispatchStructuredEvent("user-input", { content: "second prompt" }, "s1");
    await flushMicrotasks();

    const first = await markdownProviderRegistry.resolve("session-prompt:entry?idx=0");
    const second = await markdownProviderRegistry.resolve("session-prompt:entry?idx=1");
    expect(first).toContain("first prompt");
    expect(second).toContain("second prompt");
  });
});
