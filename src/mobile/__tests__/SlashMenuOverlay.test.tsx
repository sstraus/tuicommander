import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import { SlashMenuOverlay } from "../components/SlashMenuOverlay";
import type { SlashMenuItem } from "../useSessions";

vi.mock("../../transport", () => ({
  rpc: vi.fn().mockResolvedValue(undefined),
}));

import { rpc } from "../../transport";

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
      <SlashMenuOverlay sessionId="s1" items={ITEMS} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(3);
  });

  it("displays command and description text", () => {
    const { container } = render(() => (
      <SlashMenuOverlay sessionId="s1" items={ITEMS} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons[0].textContent).toContain("/help");
    expect(buttons[0].textContent).toContain("Get help with using Claude Code");
    expect(buttons[1].textContent).toContain("/review");
  });

  it("sends Ctrl-U + command + newline via write_pty on click", async () => {
    const onDismiss = vi.fn();
    const { container } = render(() => (
      <SlashMenuOverlay sessionId="s1" items={ITEMS} onDismiss={onDismiss} />
    ));
    const buttons = container.querySelectorAll("button");
    await fireEvent.click(buttons[1]); // click /review
    expect(rpc).toHaveBeenCalledWith("write_pty", {
      sessionId: "s1",
      data: "\x15/review\n",
    });
    expect(onDismiss).toHaveBeenCalled();
  });

  it("calls onDismiss when backdrop is clicked", async () => {
    const onDismiss = vi.fn();
    const { container } = render(() => (
      <SlashMenuOverlay sessionId="s1" items={ITEMS} onDismiss={onDismiss} />
    ));
    // The backdrop is the outermost div
    const backdrop = container.firstElementChild as HTMLElement;
    await fireEvent.click(backdrop);
    expect(onDismiss).toHaveBeenCalled();
  });

  it("does NOT dismiss when clicking inside the sheet", async () => {
    const onDismiss = vi.fn();
    const { container } = render(() => (
      <SlashMenuOverlay sessionId="s1" items={ITEMS} onDismiss={onDismiss} />
    ));
    // Click a button — onDismiss is called from select(), not from backdrop
    vi.clearAllMocks(); // clear any prior calls
    // Click inside the sheet (not the backdrop)
    const sheet = container.querySelector("button")!.parentElement!;
    await fireEvent.click(sheet);
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("renders empty list gracefully", () => {
    const { container } = render(() => (
      <SlashMenuOverlay sessionId="s1" items={[]} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(0);
  });
});
