import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock @tauri-apps/api/core — aiChatStore dynamic-imports invoke from here.
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage: ((_msg: unknown) => void) | null = null;
  },
}));

// Force isTauri() = true so persistence paths execute.
vi.mock("../../transport", () => ({
  isTauri: () => true,
}));

vi.mock("../../stores/appLogger", () => ({
  appLogger: {
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn(),
    error: vi.fn(),
  },
}));

describe("aiChatStore persistence (1385-87c6)", () => {
  let store: typeof import("../../stores/aiChatStore").aiChatStore;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.resetModules();
    mockInvoke.mockReset();
    globalThis.localStorage?.clear();
    store = (await import("../../stores/aiChatStore")).aiChatStore;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("initFromDisk loads a saved conversation into messages()", async () => {
    globalThis.localStorage.setItem("ai-chat-active-id", "abc-123");
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_conversation") {
        return Promise.resolve({
          meta: {
            id: "abc-123",
            title: "t",
            created: 1,
            updated: 2,
            message_count: 2,
          },
          messages: [
            { role: "user", content: "hi", timestamp: 1 },
            { role: "assistant", content: "hello", timestamp: 2 },
          ],
          schema_version: 1,
        });
      }
      return Promise.resolve();
    });

    await store.initFromDisk();
    expect(store.messages().length).toBe(2);
    expect(store.messages()[0]?.role).toBe("user");
    expect(store.messages()[1]?.content).toBe("hello");
    expect(store.chatId()).toBe("abc-123");
  });

  it("initFromDisk falls back to new_conversation_id when no saved id", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "new_conversation_id") return Promise.resolve("fresh-id");
      return Promise.resolve();
    });

    await store.initFromDisk();
    expect(store.chatId()).toBe("fresh-id");
    expect(globalThis.localStorage.getItem("ai-chat-active-id")).toBe("fresh-id");
  });

  it("addAssistantMessage triggers a debounced save_conversation invoke", async () => {
    mockInvoke.mockResolvedValue(undefined);
    store.addAssistantMessage("hello world");
    // Before debounce window elapses, no save yet.
    expect(
      mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation").length,
    ).toBe(0);
    await vi.advanceTimersByTimeAsync(600);
    const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
    expect(saves.length).toBe(1);
    const conv = saves[0]?.[1]?.conversation;
    expect(conv.messages.length).toBe(1);
    expect(conv.messages[0].role).toBe("assistant");
    expect(conv.schema_version).toBe(1);
  });

  it("clearHistory resets streaming state and deletes from disk", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "new_conversation_id") return Promise.resolve("next-id");
      return Promise.resolve();
    });
    store.addUserMessage("question");
    store.appendStreamChunk("partial");
    store.setStreaming(true);
    expect(store.messages().length).toBe(1);
    expect(store.streamingText()).toBe("partial");

    const prevId = store.chatId();
    store.clearHistory();
    expect(store.messages().length).toBe(0);
    expect(store.streamingText()).toBe("");
    expect(store.isStreaming()).toBe(false);

    await vi.runAllTimersAsync();
    const deletes = mockInvoke.mock.calls.filter((c) => c[0] === "delete_conversation");
    expect(deletes.length).toBe(1);
    expect(deletes[0]?.[1]).toEqual({ id: prevId });
    expect(store.chatId()).toBe("next-id");
  });

  it("initFromDisk loads messages with missing content as empty string (1405-3464)", async () => {
    globalThis.localStorage.setItem("ai-chat-active-id", "corrupt-123");
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_conversation") {
        return Promise.resolve({
          meta: { id: "corrupt-123", title: "t", created: 1, updated: 2, message_count: 2 },
          messages: [
            { role: "user", content: "hello", timestamp: 1 },
            { role: "assistant", timestamp: 2 }, // missing content
          ],
          schema_version: 1,
        });
      }
      return Promise.resolve();
    });

    await store.initFromDisk();
    expect(store.messages().length).toBe(2);
    expect(store.messages()[1]?.content).toBe("");
  });

  it("round-trip: message with empty content persists without serde error (1405-3464)", async () => {
    globalThis.localStorage.setItem("ai-chat-active-id", "rt-123");
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_conversation") {
        return Promise.resolve({
          meta: { id: "rt-123", title: "t", created: 1, updated: 2, message_count: 1 },
          messages: [{ role: "user", timestamp: 1 }], // missing content
          schema_version: 1,
        });
      }
      return Promise.resolve();
    });

    await store.initFromDisk();
    // Trigger persist — should call save_conversation with content: ""
    store.addAssistantMessage("reply");
    await vi.advanceTimersByTimeAsync(600);

    const saves = mockInvoke.mock.calls.filter((c) => c[0] === "save_conversation");
    expect(saves.length).toBe(1);
    const msgs = saves[0]?.[1]?.conversation?.messages as Array<{ role: string; content: string }>;
    const userMsg = msgs.find((m) => m.role === "user");
    expect(userMsg?.content).toBe("");
  });
});
