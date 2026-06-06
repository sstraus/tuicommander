import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const {
	mockSubscribe,
	mockUnsubscribe,
	mockChatId,
	mockDetachPanel,
	mockReattachPanel,
	mockClosePanel,
	mockReasoningChunks,
	mockIsThinking,
	mockMessages,
} = vi.hoisted(() => ({
	mockSubscribe: vi.fn().mockResolvedValue(undefined),
	mockUnsubscribe: vi.fn().mockResolvedValue(undefined),
	mockChatId: vi.fn(() => "chat-abc123"),
	mockDetachPanel: vi.fn().mockResolvedValue(undefined),
	mockReattachPanel: vi.fn().mockResolvedValue(undefined),
	mockClosePanel: vi.fn().mockResolvedValue(undefined),
	mockReasoningChunks: vi.fn(() => ""),
	mockIsThinking: vi.fn(() => false),
	mockMessages: vi.fn(() => [] as Array<{ role: string; content: string }>),
}));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	Channel: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../panelRouter", () => ({
	detachPanel: mockDetachPanel,
	reattachPanel: mockReattachPanel,
	closePanel: mockClosePanel,
}));

vi.mock("../../stores/conversationStore", () => ({
	conversationStore: {
		messages: mockMessages,
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
		agentState: () => "idle",
		toolCalls: () => [],
		textChunks: () => null,
		unrestricted: () => false,
		setUnrestricted: vi.fn(),
		startAgent: vi.fn(),
		pauseAgent: vi.fn(),
		resumeAgent: vi.fn(),
		cancelAgent: vi.fn(),
		pendingApproval: () => null,
		approveAction: vi.fn(),
		currentIteration: () => 0,
		reset: vi.fn(),
		reasoningChunks: mockReasoningChunks,
		isThinking: mockIsThinking,
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

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: { detachedPanels: {} },
		isDetached: vi.fn(() => false),
		setDetached: vi.fn(),
		clearDetached: vi.fn(),
	},
}));

vi.mock("../../transport", () => ({
	isTauri: () => true,
}));

import { AIChatPanel } from "../../components/AIChatPanel/AIChatPanel";

describe("AIChatPanel lifecycle", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockMessages.mockReturnValue([]);
		mockReasoningChunks.mockReturnValue("");
		mockIsThinking.mockReturnValue(false);
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

	it("renders detach button in main window mode", () => {
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		const detachBtn = container.querySelector('button[title="Open in separate window"]');
		expect(detachBtn).not.toBeNull();
	});

	it("detach button calls detachPanel", () => {
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		const detachBtn = container.querySelector('button[title="Open in separate window"]') as HTMLButtonElement;
		detachBtn.click();
		expect(mockDetachPanel).toHaveBeenCalledWith("ai-chat");
	});
});

describe("AIChatPanel extended-thinking disclosure", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Reasoning only streams after a user turn exists, so keep a user message present.
		mockMessages.mockReturnValue([{ role: "user", content: "hi" }]);
		mockReasoningChunks.mockReturnValue("");
		mockIsThinking.mockReturnValue(false);
	});

	afterEach(() => {
		cleanup();
	});

	it("does not render the disclosure when there is no reasoning", () => {
		mockReasoningChunks.mockReturnValue("");
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		expect(container.querySelector("details")).toBeNull();
	});

	it("renders the Thinking disclosure when reasoning is present", () => {
		mockReasoningChunks.mockReturnValue("planning the steps");
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		const details = container.querySelector("details");
		expect(details).not.toBeNull();
		expect(details?.querySelector("summary")?.textContent).toBe("Thinking");
		expect(details?.textContent).toContain("planning the steps");
	});

	it("auto-opens the disclosure while the model is thinking", () => {
		mockReasoningChunks.mockReturnValue("still reasoning");
		mockIsThinking.mockReturnValue(true);
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		expect(container.querySelector("details")?.hasAttribute("open")).toBe(true);
	});

	it("collapses the disclosure once thinking has finished", () => {
		mockReasoningChunks.mockReturnValue("done reasoning");
		mockIsThinking.mockReturnValue(false);
		const { container } = render(() => <AIChatPanel visible={true} onClose={() => {}} />);
		expect(container.querySelector("details")?.hasAttribute("open")).toBe(false);
	});
});
