import { describe, it, expect, beforeEach, vi } from "vitest";
import "../../../__tests__/mocks/tauri";
import { contextMenuActionsStore } from "../../../stores/contextMenuActionsStore";
import { truncateText, registerAiChatContextActions } from "../contextMenuActions";

// Mock stores
vi.mock("../../../stores/ui", () => ({
  uiStore: { setAiChatPanelVisible: vi.fn() },
}));

vi.mock("../../../stores/aiChatStore", () => ({
  aiChatStore: {
    setActiveTerminal: vi.fn(),
    sendMessage: vi.fn(),
  },
}));

vi.mock("../../../stores/terminals", () => ({
  terminalsStore: {
    state: { terminals: {} },
    getIds: () => [],
    get: () => undefined,
    setActive: vi.fn(),
  },
}));

vi.mock("../../../stores/appLogger", () => ({
  appLogger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

import { uiStore } from "../../../stores/ui";
import { aiChatStore } from "../../../stores/aiChatStore";

describe("truncateText", () => {
  it("returns text as-is when under limit", () => {
    expect(truncateText("short", 100)).toBe("short");
  });

  it("returns text as-is when exactly at limit", () => {
    const text = "a".repeat(50);
    expect(truncateText(text, 50)).toBe(text);
  });

  it("truncates and adds marker when over limit", () => {
    const text = "a".repeat(200);
    const result = truncateText(text, 100);
    expect(result.length).toBeLessThan(200);
    expect(result).toContain("[... truncated]");
    expect(result.startsWith("a".repeat(100))).toBe(true);
  });
});

describe("registerAiChatContextActions", () => {
  beforeEach(() => {
    contextMenuActionsStore.clear();
    vi.clearAllMocks();
  });

  it("registers two actions with correct ids and target", () => {
    const disposables = registerAiChatContextActions();
    const actions = contextMenuActionsStore.getContextActions("terminal");
    expect(actions).toHaveLength(2);

    const ids = actions.map((a) => a.id);
    expect(ids).toContain("ai-chat:explain");
    expect(ids).toContain("ai-chat:fix-error");

    expect(actions.every((a) => a.target === "terminal")).toBe(true);

    disposables.forEach((d) => d.dispose());
  });

  it("dispose removes both actions", () => {
    const disposables = registerAiChatContextActions();
    expect(contextMenuActionsStore.getContextActions("terminal")).toHaveLength(2);

    disposables.forEach((d) => d.dispose());
    expect(contextMenuActionsStore.getContextActions("terminal")).toHaveLength(0);
  });

  it("explain action opens AI Chat panel and sends with sessionId", () => {
    // Provide a selection so the action has text to work with
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "some selected output",
    } as Selection);

    registerAiChatContextActions();
    const actions = contextMenuActionsStore.getContextActions("terminal");
    const explain = actions.find((a) => a.id === "ai-chat:explain")!;

    explain.action({ target: "terminal", sessionId: "sess-1" });

    expect(uiStore.setAiChatPanelVisible).toHaveBeenCalledWith(true);
    expect(aiChatStore.sendMessage).toHaveBeenCalledWith(
      expect.stringContaining("Explain this terminal output"),
      "sess-1",
    );
    expect(aiChatStore.sendMessage).toHaveBeenCalledWith(
      expect.stringContaining("some selected output"),
      "sess-1",
    );
  });

  it("fix-error action opens AI Chat panel and sends error prompt with sessionId", () => {
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "Error: command not found",
    } as Selection);

    registerAiChatContextActions();
    const actions = contextMenuActionsStore.getContextActions("terminal");
    const fixError = actions.find((a) => a.id === "ai-chat:fix-error")!;

    fixError.action({ target: "terminal", sessionId: "sess-2" });

    expect(uiStore.setAiChatPanelVisible).toHaveBeenCalledWith(true);
    expect(aiChatStore.sendMessage).toHaveBeenCalledWith(
      expect.stringContaining("Analyze this terminal error"),
      "sess-2",
    );
    expect(aiChatStore.sendMessage).toHaveBeenCalledWith(
      expect.stringContaining("How to fix it"),
      "sess-2",
    );
  });

  it("does not send message when no text is available", () => {
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "",
    } as Selection);

    registerAiChatContextActions();
    const actions = contextMenuActionsStore.getContextActions("terminal");
    const explain = actions.find((a) => a.id === "ai-chat:explain")!;

    explain.action({ target: "terminal", sessionId: "sess-3" });

    expect(aiChatStore.sendMessage).not.toHaveBeenCalled();
  });
});
