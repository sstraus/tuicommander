import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { PromptOption } from "../../components/ui/PromptOption";

describe("PromptOption", () => {
  it("renders label text", () => {
    const { container } = render(() => (
      <PromptOption index={0} label="Option A" selected={false} onClick={() => {}} />
    ));
    const text = container.querySelector(".prompt-option-text");
    expect(text).not.toBeNull();
    expect(text!.textContent).toBe("Option A");
  });

  it("renders index + 1 as key", () => {
    const { container } = render(() => (
      <PromptOption index={2} label="Third" selected={false} onClick={() => {}} />
    ));
    const key = container.querySelector(".prompt-option-key");
    expect(key).not.toBeNull();
    expect(key!.textContent).toBe("3");
  });

  it("applies selected class when selected is true", () => {
    const { container } = render(() => (
      <PromptOption index={0} label="Selected" selected={true} onClick={() => {}} />
    ));
    const div = container.querySelector(".prompt-option");
    expect(div!.classList.contains("selected")).toBe(true);
  });

  it("does not apply selected class when selected is false", () => {
    const { container } = render(() => (
      <PromptOption index={0} label="Not selected" selected={false} onClick={() => {}} />
    ));
    const div = container.querySelector(".prompt-option");
    expect(div!.classList.contains("selected")).toBe(false);
  });

  it("calls onClick when clicked", () => {
    const handleClick = vi.fn();
    const { container } = render(() => (
      <PromptOption index={0} label="Clickable" selected={false} onClick={handleClick} />
    ));
    const div = container.querySelector(".prompt-option")!;
    fireEvent.click(div);
    expect(handleClick).toHaveBeenCalledOnce();
  });
});
