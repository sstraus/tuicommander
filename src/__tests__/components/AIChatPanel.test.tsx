import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@solidjs/testing-library";

const { mockSubscribe, mockUnsubscribe, mockChatId } = vi.hoisted(() => ({
  mockSubscribe: vi.fn().mockResolvedValue(undefined),
  mockUnsubscribe: vi.fn().mockResolvedValue(undefined),
  mockChatId: vi.fn(() => "chat-abc123"),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  Channel: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../stores/aiChatStore", () => ({
  aiChatStore: {
    messages: () => [],
    isStreaming: () => false,
    streamingText: () => "",
    error: () => null,
    chatId: mockChatId,
    sessionUsage: () => null,
    sendMessage: vi.fn(),
    cancelStream: vi.fn(),
    clearHistory: vi.fn(),
    subscribeToRegistry: mockSubscribe,
    unsubscribeFromRegistry: mockUnsubscribe,
    listAllConversations: vi.fn().mockResolvedValue([]),
    loadConversation: vi.fn(),
    resetChatId: vi.fn(),
  },
}));

vi.mock("../../stores/aiAgentStore", () => ({
  aiAgentStore: {
    agentState: () => "idle",
    toolCalls: () => [],
    textChunks: () => null,
    agentType: () => null,
    unrestricted: () => false,
    setUnrestricted: vi.fn(),
    startAgent: vi.fn(),
    stopAgent: vi.fn(),
    pauseAgent: vi.fn(),
    resumeAgent: vi.fn(),
    cancelAgent: vi.fn(),
    pendingApproval: () => null,
    approveAction: vi.fn(),
    currentIteration: () => 0,
  },
}));

vi.mock("../../stores/terminals", () => ({
  terminalsStore: {
    state: { activeId: "t1", terminals: {} },
    getIds: () => ["t1"],
    get: () => ({ sessionId: "sess-1", tuicSession: "sess-1", name: "Terminal 1", ref: null }),
  },
}));

vi.mock("../../stores/appLogger", () => ({
  appLogger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

vi.mock("../../utils/sendCommand", () => ({
  sendCommand: vi.fn(),
  getShellFamily: vi.fn(() => "posix"),
}));

vi.mock("../../transport", () => ({
  isTauri: () => true,
}));

import { AIChatPanel } from "../../components/AIChatPanel/AIChatPanel";

describe("AIChatPanel lifecycle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("subscribes to registry on mount with current chatId", async () => {
    render(() => <AIChatPanel visible={true} onClose={() => {}} />);
    await vi.waitFor(() => {
      expect(mockSubscribe).toHaveBeenCalledWith("chat-abc123");
    });
  });

  it("unsubscribes from registry on unmount", async () => {
    const { unmount } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
    await vi.waitFor(() => {
      expect(mockSubscribe).toHaveBeenCalled();
    });
    unmount();
    expect(mockUnsubscribe).toHaveBeenCalled();
  });
});
