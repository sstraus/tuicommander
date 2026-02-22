import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

// Capture the mock write function so tests can assert on it
const mockWrite = vi.fn().mockResolvedValue(undefined);

// Mock usePty hook to avoid Tauri invoke calls
vi.mock("../../hooks/usePty", () => ({
  usePty: () => ({
    canSpawn: vi.fn(),
    createSession: vi.fn(),
    createSessionWithWorktree: vi.fn(),
    write: mockWrite,
    resize: vi.fn(),
    pause: vi.fn(),
    resume: vi.fn(),
    close: vi.fn(),
    getStats: vi.fn(),
    getMetrics: vi.fn(),
    listWorktrees: vi.fn(),
    getWorktreesDir: vi.fn(),
  }),
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    listen: vi.fn().mockResolvedValue(vi.fn()),
    setTitle: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { PromptOverlay } from "../../components/PromptOverlay/PromptOverlay";
import { promptStore } from "../../stores/prompt";

function showTestPrompt(options: string[] = ["Alpha", "Beta", "Gamma"], question = "Pick one:") {
  promptStore.showPrompt({
    question,
    options,
    sessionId: "test-session",
  });
}

describe("PromptOverlay", () => {
  beforeEach(() => {
    promptStore.hidePrompt();
    mockWrite.mockClear();
  });

  it("has hidden class when no prompt is active", () => {
    const { container } = render(() => <PromptOverlay />);
    const overlay = container.querySelector(".overlay");
    expect(overlay).not.toBeNull();
    expect(overlay!.classList.contains("hidden")).toBe(true);
  });

  it("shows prompt dialog when prompt is active", () => {
    showTestPrompt(["Hammer", "Wrench", "Screwdriver"], "Choose your tool:");

    const { container } = render(() => <PromptOverlay />);
    const overlay = container.querySelector(".overlay");
    expect(overlay!.classList.contains("hidden")).toBe(false);

    const question = container.querySelector(".question");
    expect(question).not.toBeNull();
    expect(question!.textContent).toBe("Choose your tool:");
  });

  it("renders prompt options", () => {
    showTestPrompt(["Alpha", "Beta", "Gamma"], "Pick one:");

    const { container } = render(() => <PromptOverlay />);
    const options = container.querySelectorAll(".option");
    expect(options.length).toBe(3);

    const texts = Array.from(options).map(
      (o) => o.querySelector(".optionText")!.textContent
    );
    expect(texts).toEqual(["Alpha", "Beta", "Gamma"]);
  });

  it("renders option keys as 1-indexed numbers", () => {
    showTestPrompt(["A", "B"]);

    const { container } = render(() => <PromptOverlay />);
    const keys = container.querySelectorAll(".optionKey");
    expect(keys[0].textContent).toBe("1");
    expect(keys[1].textContent).toBe("2");
  });

  it("renders hint text with option count", () => {
    showTestPrompt(["X", "Y", "Z"]);

    const { container } = render(() => <PromptOverlay />);
    const hint = container.querySelector(".hint");
    expect(hint).not.toBeNull();
    expect(hint!.textContent).toContain("1-3");
  });

  it("first option is selected by default", () => {
    showTestPrompt();

    const { container } = render(() => <PromptOverlay />);
    const options = container.querySelectorAll(".option");
    expect(options[0].classList.contains("selected")).toBe(true);
    expect(options[1].classList.contains("selected")).toBe(false);
    expect(options[2].classList.contains("selected")).toBe(false);
  });

  it("ArrowDown moves selection down", () => {
    showTestPrompt();

    const { container } = render(() => <PromptOverlay />);

    fireEvent.keyDown(document, { key: "ArrowDown" });

    const options = container.querySelectorAll(".option");
    expect(options[0].classList.contains("selected")).toBe(false);
    expect(options[1].classList.contains("selected")).toBe(true);
  });

  it("ArrowUp moves selection up", () => {
    showTestPrompt();

    const { container } = render(() => <PromptOverlay />);

    // Move down first, then up
    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "ArrowUp" });

    const options = container.querySelectorAll(".option");
    expect(options[0].classList.contains("selected")).toBe(true);
  });

  it("ArrowUp at index 0 stays at 0", () => {
    showTestPrompt();

    const { container } = render(() => <PromptOverlay />);

    fireEvent.keyDown(document, { key: "ArrowUp" });

    const options = container.querySelectorAll(".option");
    expect(options[0].classList.contains("selected")).toBe(true);
  });

  it("ArrowDown at last index stays at last", () => {
    showTestPrompt(["A", "B"]);

    const { container } = render(() => <PromptOverlay />);

    // Move down twice on a 2-item list
    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "ArrowDown" });

    const options = container.querySelectorAll(".option");
    expect(options[1].classList.contains("selected")).toBe(true);
  });

  it("Enter confirms selection and calls pty.write", async () => {
    showTestPrompt();

    render(() => <PromptOverlay />);

    // Default selection is index 0, so Enter sends "1\n"
    fireEvent.keyDown(document, { key: "Enter" });

    // Wait for the async confirm to complete
    await vi.waitFor(() => {
      expect(mockWrite).toHaveBeenCalledWith("test-session", "1\n");
    });

    // Prompt should be hidden
    expect(promptStore.state.activePrompt).toBeNull();
  });

  it("Enter after ArrowDown sends correct selection number", async () => {
    showTestPrompt();

    render(() => <PromptOverlay />);

    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "Enter" });

    await vi.waitFor(() => {
      expect(mockWrite).toHaveBeenCalledWith("test-session", "2\n");
    });
  });

  it("Escape dismisses prompt and calls onDismiss", () => {
    const handleDismiss = vi.fn();
    showTestPrompt();

    render(() => <PromptOverlay onDismiss={handleDismiss} />);

    fireEvent.keyDown(document, { key: "Escape" });

    expect(promptStore.state.activePrompt).toBeNull();
    expect(handleDismiss).toHaveBeenCalledOnce();
  });

  it("number key selects and confirms", async () => {
    showTestPrompt();

    render(() => <PromptOverlay />);

    fireEvent.keyDown(document, { key: "2" });

    await vi.waitFor(() => {
      expect(mockWrite).toHaveBeenCalledWith("test-session", "2\n");
    });
  });

  it("number key out of range does nothing", async () => {
    showTestPrompt(["A", "B"]); // Only 2 options

    render(() => <PromptOverlay />);

    fireEvent.keyDown(document, { key: "5" });

    // Small delay to ensure nothing happens
    await new Promise((r) => setTimeout(r, 50));
    expect(mockWrite).not.toHaveBeenCalled();
    // Prompt should still be visible
    expect(promptStore.state.activePrompt).not.toBeNull();
  });

  it("clicking option selects and confirms", async () => {
    showTestPrompt();

    const { container } = render(() => <PromptOverlay />);

    const options = container.querySelectorAll(".option");
    fireEvent.click(options[2]); // Click third option

    await vi.waitFor(() => {
      expect(mockWrite).toHaveBeenCalledWith("test-session", "3\n");
    });
  });

  it("shows default question when prompt has no question", () => {
    promptStore.showPrompt({
      question: "",
      options: ["A"],
      sessionId: "test-session",
    });

    const { container } = render(() => <PromptOverlay />);
    const question = container.querySelector(".question");
    // The component uses prompt()?.question || "Select an option:"
    expect(question!.textContent).toBe("Select an option:");
  });

  it("keyboard events not handled when prompt is hidden", () => {
    // No prompt shown
    render(() => <PromptOverlay />);

    // These should not crash
    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "Enter" });
    fireEvent.keyDown(document, { key: "1" });

    expect(mockWrite).not.toHaveBeenCalled();
  });

  it("confirm resets state and hides prompt", async () => {
    showTestPrompt();

    const { container } = render(() => <PromptOverlay />);

    // Move down and confirm
    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "Enter" });

    await vi.waitFor(() => {
      expect(mockWrite).toHaveBeenCalledWith("test-session", "2\n");
    });

    // After confirming, prompt is hidden
    expect(promptStore.state.activePrompt).toBeNull();
    const overlay = container.querySelector(".overlay");
    expect(overlay!.classList.contains("hidden")).toBe(true);
  });
});
