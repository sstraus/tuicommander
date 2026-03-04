import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@solidjs/testing-library";
import SuggestOverlay from "../../components/SuggestOverlay/SuggestOverlay";

afterEach(cleanup);

describe("SuggestOverlay", () => {
  it("renders a button for each item", () => {
    const items = ["Run tests", "Review diff", "Deploy"];
    const { container } = render(() => (
      <SuggestOverlay items={items} onSelect={() => {}} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(3);
    expect(buttons[0].textContent).toContain("Run tests");
    expect(buttons[1].textContent).toContain("Review diff");
    expect(buttons[2].textContent).toContain("Deploy");
  });

  it("shows keyboard shortcut number on each button", () => {
    const items = ["Run tests", "Review diff", "Deploy"];
    const { container } = render(() => (
      <SuggestOverlay items={items} onSelect={() => {}} onDismiss={() => {}} />
    ));
    const badges = container.querySelectorAll("[data-shortcut]");
    expect(badges.length).toBe(3);
    expect(badges[0].textContent).toBe("1");
    expect(badges[1].textContent).toBe("2");
    expect(badges[2].textContent).toBe("3");
  });

  it("calls onSelect with item text when chip is clicked", () => {
    const onSelect = vi.fn();
    const { container } = render(() => (
      <SuggestOverlay items={["Run tests", "Deploy"]} onSelect={onSelect} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    fireEvent.click(buttons[1]);
    expect(onSelect).toHaveBeenCalledWith("Deploy");
  });

  it("calls onSelect when number key 1-4 is pressed", () => {
    const onSelect = vi.fn();
    render(() => (
      <SuggestOverlay items={["Run tests", "Review diff", "Deploy"]} onSelect={onSelect} onDismiss={() => {}} />
    ));
    fireEvent.keyDown(document, { key: "2" });
    expect(onSelect).toHaveBeenCalledWith("Review diff");
  });

  it("ignores number keys beyond items length", () => {
    const onSelect = vi.fn();
    render(() => (
      <SuggestOverlay items={["Run tests"]} onSelect={onSelect} onDismiss={() => {}} />
    ));
    fireEvent.keyDown(document, { key: "2" });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("calls onDismiss when Escape is pressed", () => {
    const onDismiss = vi.fn();
    render(() => (
      <SuggestOverlay items={["Run tests"]} onSelect={() => {}} onDismiss={onDismiss} />
    ));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("auto-dismisses after timeout", () => {
    vi.useFakeTimers();
    const onDismiss = vi.fn();
    render(() => (
      <SuggestOverlay items={["Run tests"]} onSelect={() => {}} onDismiss={onDismiss} />
    ));
    expect(onDismiss).not.toHaveBeenCalled();
    vi.advanceTimersByTime(30_000);
    expect(onDismiss).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("renders empty when items array is empty", () => {
    const { container } = render(() => (
      <SuggestOverlay items={[]} onSelect={() => {}} onDismiss={() => {}} />
    ));
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(0);
  });
});
