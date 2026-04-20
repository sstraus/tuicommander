import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock invoke before importing the store
vi.mock("../../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../transport", () => ({
  isTauri: () => false,
}));

vi.mock("../../stores/appLogger", () => ({
  appLogger: {
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn(),
    error: vi.fn(),
  },
}));

import { aiChatStore } from "../../stores/aiChatStore";

describe("aiChatStore", () => {
  beforeEach(() => {
    aiChatStore.clearHistory();
    vi.clearAllMocks();
  });

  // -- Messages --

  it("starts with empty messages", () => {
    expect(aiChatStore.messages()).toEqual([]);
  });

  it("addUserMessage appends a user message", () => {
    aiChatStore.addUserMessage("hello");
    const msgs = aiChatStore.messages();
    expect(msgs).toHaveLength(1);
    expect(msgs[0].role).toBe("user");
    expect(msgs[0].content).toBe("hello");
    expect(msgs[0].timestamp).toBeGreaterThan(0);
  });

  it("addAssistantMessage appends an assistant message", () => {
    aiChatStore.addAssistantMessage("hi there");
    const msgs = aiChatStore.messages();
    expect(msgs).toHaveLength(1);
    expect(msgs[0].role).toBe("assistant");
    expect(msgs[0].content).toBe("hi there");
  });

  it("clearHistory removes all messages", () => {
    aiChatStore.addUserMessage("msg1");
    aiChatStore.addUserMessage("msg2");
    expect(aiChatStore.messages()).toHaveLength(2);
    aiChatStore.clearHistory();
    expect(aiChatStore.messages()).toEqual([]);
  });

  it("caps messages at 100, dropping oldest", () => {
    for (let i = 0; i < 110; i++) {
      aiChatStore.addUserMessage(`msg-${i}`);
    }
    const msgs = aiChatStore.messages();
    expect(msgs).toHaveLength(100);
    // First message should be msg-10 (oldest 10 dropped)
    expect(msgs[0].content).toBe("msg-10");
    expect(msgs[99].content).toBe("msg-109");
  });

  // -- Streaming state --

  it("isStreaming starts as false", () => {
    expect(aiChatStore.isStreaming()).toBe(false);
  });

  it("streamingText starts empty", () => {
    expect(aiChatStore.streamingText()).toBe("");
  });

  it("appendStreamChunk accumulates streaming text", () => {
    aiChatStore.setStreaming(true);
    aiChatStore.appendStreamChunk("hello ");
    aiChatStore.appendStreamChunk("world");
    expect(aiChatStore.streamingText()).toBe("hello world");
  });

  it("finalizeStream moves streamingText to assistant message", () => {
    aiChatStore.setStreaming(true);
    aiChatStore.appendStreamChunk("full response");
    aiChatStore.finalizeStream("full response");
    expect(aiChatStore.isStreaming()).toBe(false);
    expect(aiChatStore.streamingText()).toBe("");
    const msgs = aiChatStore.messages();
    expect(msgs).toHaveLength(1);
    expect(msgs[0].role).toBe("assistant");
    expect(msgs[0].content).toBe("full response");
  });

  // -- Error state --

  it("error starts as null", () => {
    expect(aiChatStore.error()).toBeNull();
  });

  it("setError sets and clears error", () => {
    aiChatStore.setError("something failed");
    expect(aiChatStore.error()).toBe("something failed");
    aiChatStore.setError(null);
    expect(aiChatStore.error()).toBeNull();
  });

  // -- Chat ID --

  it("chatId starts as non-empty string", () => {
    expect(aiChatStore.chatId()).toBeTruthy();
  });

  it("resetChatId generates a new id", () => {
    const first = aiChatStore.chatId();
    aiChatStore.resetChatId();
    const second = aiChatStore.chatId();
    expect(second).not.toBe(first);
  });
});

describe("aiChatStore — session usage accumulator (1415-239d)", () => {
  beforeEach(() => {
    aiChatStore.setActiveTerminal("__default__");
    aiChatStore.clearHistory();
    vi.clearAllMocks();
  });

  it("sessionUsage starts as null", () => {
    expect(aiChatStore.sessionUsage()).toBeNull();
  });

  it("accumulateUsage sums tokens across calls", () => {
    aiChatStore.accumulateUsage({ promptTokens: 100, completionTokens: 50, totalTokens: 150 });
    aiChatStore.accumulateUsage({ promptTokens: 200, completionTokens: 80, totalTokens: 280 });
    const u = aiChatStore.sessionUsage();
    expect(u?.promptTokens).toBe(300);
    expect(u?.completionTokens).toBe(130);
  });

  it("accumulateUsage sums cost_usd", () => {
    aiChatStore.accumulateUsage({ promptTokens: 100, completionTokens: 50, costUsd: 0.001 });
    aiChatStore.accumulateUsage({ promptTokens: 100, completionTokens: 50, costUsd: 0.002 });
    const u = aiChatStore.sessionUsage();
    expect(u?.costUsd).toBeCloseTo(0.003);
  });

  it("accumulateUsage accumulates cachedTokens", () => {
    aiChatStore.accumulateUsage({ promptTokens: 500, completionTokens: 100, cachedTokens: 300 });
    aiChatStore.accumulateUsage({ promptTokens: 200, completionTokens: 50, cachedTokens: 100 });
    const u = aiChatStore.sessionUsage();
    expect(u?.cachedTokens).toBe(400);
  });

  it("clearHistory resets sessionUsage", () => {
    aiChatStore.accumulateUsage({ promptTokens: 100, completionTokens: 50 });
    aiChatStore.clearHistory();
    expect(aiChatStore.sessionUsage()).toBeNull();
  });

  it("sessionUsage is independent per terminal", () => {
    aiChatStore.setActiveTerminal("termA");
    aiChatStore.accumulateUsage({ promptTokens: 100, completionTokens: 50 });

    aiChatStore.setActiveTerminal("termB");
    expect(aiChatStore.sessionUsage()).toBeNull();

    aiChatStore.setActiveTerminal("termA");
    expect(aiChatStore.sessionUsage()?.promptTokens).toBe(100);
  });
});

describe("aiChatStore — per-terminal state (1406-c679)", () => {
  beforeEach(() => {
    // Reset to default terminal between tests
    aiChatStore.setActiveTerminal("__default__");
    aiChatStore.clearHistory();
    vi.clearAllMocks();
  });

  it("getOrCreate returns independent state for different keys", () => {
    const stateA = aiChatStore.getOrCreate("termA");
    const stateB = aiChatStore.getOrCreate("termB");
    expect(stateA).not.toBe(stateB);
  });

  it("messages() reflects the active terminal only", () => {
    aiChatStore.setActiveTerminal("termA");
    aiChatStore.addUserMessage("hello from A");

    aiChatStore.setActiveTerminal("termB");
    expect(aiChatStore.messages()).toEqual([]);

    aiChatStore.setActiveTerminal("termA");
    expect(aiChatStore.messages()).toHaveLength(1);
    expect(aiChatStore.messages()[0]?.content).toBe("hello from A");
  });

  it("isStreaming() is independent per terminal", () => {
    aiChatStore.setActiveTerminal("termA");
    aiChatStore.setStreaming(true);

    aiChatStore.setActiveTerminal("termB");
    expect(aiChatStore.isStreaming()).toBe(false);

    aiChatStore.setActiveTerminal("termA");
    expect(aiChatStore.isStreaming()).toBe(true);
  });

  it("error() is independent per terminal", () => {
    aiChatStore.setActiveTerminal("termA");
    aiChatStore.setError("termA error");

    aiChatStore.setActiveTerminal("termB");
    expect(aiChatStore.error()).toBeNull();
  });

  it("activeChat() returns the PerTerminalChatState for the active terminal", () => {
    aiChatStore.setActiveTerminal("termX");
    const state = aiChatStore.activeChat();
    expect(typeof state.messages).toBe("function");
    expect(typeof state.isStreaming).toBe("function");
    expect(typeof state.streamingText).toBe("function");
    expect(typeof state.error).toBe("function");
    expect(typeof state.chatId).toBe("function");
  });
});
