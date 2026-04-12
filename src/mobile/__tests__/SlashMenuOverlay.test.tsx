import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import { SlashMenuOverlay } from "../components/SlashMenuOverlay";
import type { SlashMenuItem } from "../useSessions";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const ITEMS: SlashMenuItem[] = [
  { command: "/help", description: "Get help with using Claude Code", highlighted: false },
  { command: "/review", description: "Review your code", highlighted: true },
  { command: "/clear", description: "Clear conversation history", highlighted: false },
];

describe("SlashMenuOverlay", () => {
  it("renders a button for each menu item", () => {
    const { container } = render(() => (
      <SlashMenuOverlay items={ITEMS} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(3);
  });

  it("displays command and description text", () => {
    const { container } = render(() => (
      <SlashMenuOverlay items={ITEMS} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons[0].textContent).toContain("/help");
    expect(buttons[0].textContent).toContain("Get help with using Claude Code");
    expect(buttons[1].textContent).toContain("/review");
  });

  it("calls onSelect with command on click", async () => {
    const onSelect = vi.fn();
    const { container } = render(() => (
      <SlashMenuOverlay items={ITEMS} onSelect={onSelect} />
    ));
    const buttons = container.querySelectorAll("button");
    await fireEvent.click(buttons[1]); // click /review
    expect(onSelect).toHaveBeenCalledWith("/review");
  });

  it("highlights the item with highlighted=true", () => {
    const { container } = render(() => (
      <SlashMenuOverlay items={ITEMS} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    // /review has highlighted: true
    expect(buttons[1].className).toContain("Highlighted");
    // /help does not
    expect(buttons[0].className).not.toContain("Highlighted");
  });

  it("renders empty list when no items", () => {
    const { container } = render(() => (
      <SlashMenuOverlay items={[]} onSelect={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(0);
  });
});
