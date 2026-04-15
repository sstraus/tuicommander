import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import { ChoicePromptOverlay } from "../components/ChoicePromptOverlay";
import type { ChoicePrompt } from "../useSessions";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const CLAUDE_EDIT_CONFIRM: ChoicePrompt = {
  title: "Do you want to make this edit to CLAUDE.md?",
  options: [
    { key: "1", label: "Yes", highlighted: true, destructive: false },
    { key: "2", label: "Yes, allow all edits during this session", highlighted: false, destructive: false, hint: "shift+tab" },
    { key: "3", label: "No", highlighted: false, destructive: true },
  ],
  dismiss_key: "cancel",
  amend_key: "amend",
};

describe("ChoicePromptOverlay", () => {
  it("renders the prompt title", () => {
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={() => {}} />
    ));
    expect(container.textContent).toContain("Do you want to make this edit to CLAUDE.md?");
  });

  it("renders a button per option with label text", () => {
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(3);
    expect(buttons[0].textContent).toContain("Yes");
    expect(buttons[1].textContent).toContain("Yes, allow all edits");
    expect(buttons[2].textContent).toContain("No");
  });

  it("includes the option key on each button", () => {
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons[0].textContent).toContain("1");
    expect(buttons[1].textContent).toContain("2");
    expect(buttons[2].textContent).toContain("3");
  });

  it("displays optional hint when present", () => {
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons[1].textContent).toContain("shift+tab");
  });

  it("calls onSelect with the key on click", async () => {
    const onSelect = vi.fn();
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={onSelect} />
    ));
    const buttons = container.querySelectorAll("button");
    await fireEvent.click(buttons[2]);
    expect(onSelect).toHaveBeenCalledWith("3");
  });

  it("marks the highlighted option", () => {
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons[0].className).toContain("Highlighted");
    expect(buttons[1].className).not.toContain("Highlighted");
  });

  it("marks destructive options distinctly", () => {
    const { container } = render(() => (
      <ChoicePromptOverlay prompt={CLAUDE_EDIT_CONFIRM} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    // Destructive option ("No") must carry a class identifier for styling.
    expect(buttons[2].className).toContain("Destructive");
    expect(buttons[0].className).not.toContain("Destructive");
  });
});
