import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";

const mockKeybindingsStore = vi.hoisted(() => ({
  getActionForCombo: vi.fn().mockReturnValue(undefined),
  getKeyForAction: vi.fn().mockReturnValue(undefined),
  getAllBindings: vi.fn().mockReturnValue({}),
  hydrate: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../stores/keybindings", () => ({
  keybindingsStore: mockKeybindingsStore,
}));

import { KeyComboCapture } from "../../components/shared/KeyComboCapture";

describe("KeyComboCapture", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockKeybindingsStore.getActionForCombo.mockReturnValue(undefined);
  });

  it("shows current value in display button", () => {
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+K" onChange={() => {}} />
    ));
    const btn = container.querySelector(".display");
    expect(btn).not.toBeNull();
    expect(btn!.textContent).toContain("Cmd+K");
  });

  it("shows placeholder when value is empty", () => {
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={() => {}} placeholder="Click to set" />
    ));
    const btn = container.querySelector(".display");
    expect(btn!.textContent).toContain("Click to set");
  });

  it("enters capture mode when display button is clicked", () => {
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+K" onChange={() => {}} />
    ));
    const btn = container.querySelector(".display")!;
    fireEvent.click(btn);
    // Input should appear, button should disappear
    expect(container.querySelector(".input")).not.toBeNull();
    expect(container.querySelector(".display")).toBeNull();
  });

  it("calls onChange with captured key combo", () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={onChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    const input = container.querySelector(".input")!;
    fireEvent.keyDown(input, { key: "K", code: "KeyK", metaKey: true });
    expect(onChange).toHaveBeenCalledWith("Cmd+K");
  });

  it("normalizes key combos: space → Space, single chars uppercase", () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={onChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.keyDown(container.querySelector(".input")!, { key: " ", metaKey: true });
    expect(onChange).toHaveBeenCalledWith("Cmd+Space");
  });

  it("ignores bare modifier key presses", () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={onChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    const input = container.querySelector(".input")!;
    fireEvent.keyDown(input, { key: "Meta", metaKey: true });
    fireEvent.keyDown(input, { key: "Control", ctrlKey: true });
    fireEvent.keyDown(input, { key: "Alt", altKey: true });
    fireEvent.keyDown(input, { key: "Shift", shiftKey: true });
    expect(onChange).not.toHaveBeenCalled();
    // Still in capture mode
    expect(container.querySelector(".input")).not.toBeNull();
  });

  it("exits capture mode after a key is captured", () => {
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={() => {}} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.keyDown(container.querySelector(".input")!, { key: "K", metaKey: true });
    expect(container.querySelector(".display")).not.toBeNull();
    expect(container.querySelector(".input")).toBeNull();
  });

  it("exits capture mode on Escape without calling onChange", () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+K" onChange={onChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.keyDown(container.querySelector(".input")!, { key: "Escape" });
    expect(onChange).not.toHaveBeenCalled();
    expect(container.querySelector(".display")).not.toBeNull();
  });

  it("exits capture mode on blur without calling onChange", () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+K" onChange={onChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.blur(container.querySelector(".input")!);
    expect(onChange).not.toHaveBeenCalled();
    expect(container.querySelector(".display")).not.toBeNull();
  });

  it("calls onCapturingChange(true) when entering capture mode", () => {
    const onCapturingChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={() => {}} onCapturingChange={onCapturingChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    expect(onCapturingChange).toHaveBeenCalledWith(true);
  });

  it("calls onCapturingChange(false) when exiting capture mode via Escape", () => {
    const onCapturingChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={() => {}} onCapturingChange={onCapturingChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.keyDown(container.querySelector(".input")!, { key: "Escape" });
    expect(onCapturingChange).toHaveBeenCalledWith(false);
  });

  it("calls onCapturingChange(false) when exiting after successful capture", () => {
    const onCapturingChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={() => {}} onCapturingChange={onCapturingChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.keyDown(container.querySelector(".input")!, { key: "K", metaKey: true });
    expect(onCapturingChange).toHaveBeenCalledWith(false);
  });

  it("shows collision warning when combo conflicts with a keybinding action", () => {
    mockKeybindingsStore.getActionForCombo.mockReturnValue("toggle-diff");
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+Shift+D" onChange={() => {}} />
    ));
    const warning = container.querySelector(".collision");
    expect(warning).not.toBeNull();
    expect(warning!.textContent).toContain("toggle-diff");
  });

  it("suppresses collision warning for excluded actions", () => {
    mockKeybindingsStore.getActionForCombo.mockReturnValue("toggle-diff");
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+Shift+D" onChange={() => {}} exclude={["toggle-diff"]} />
    ));
    expect(container.querySelector(".collision")).toBeNull();
  });

  it("shows no collision warning when combo is unbound", () => {
    mockKeybindingsStore.getActionForCombo.mockReturnValue(undefined);
    const { container } = render(() => (
      <KeyComboCapture value="Cmd+K" onChange={() => {}} />
    ));
    expect(container.querySelector(".collision")).toBeNull();
  });

  it("captures multi-modifier combos correctly", () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <KeyComboCapture value="" onChange={onChange} />
    ));
    fireEvent.click(container.querySelector(".display")!);
    fireEvent.keyDown(container.querySelector(".input")!, {
      key: "D",
      metaKey: true,
      shiftKey: true,
    });
    expect(onChange).toHaveBeenCalledWith("Cmd+Shift+D");
  });
});
