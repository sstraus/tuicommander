import { describe, it, expect, vi, beforeEach } from "vitest";
import { keyEventToCombo, validateGlobalHotkeyCombo } from "../../utils/keyRecorder";

// Mock platform detection — default to macOS
vi.mock("../../platform", () => ({
  isMacOS: vi.fn(() => true),
  getModifierSymbol: vi.fn(() => "⌘"),
}));

function makeKeyEvent(overrides: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    key: "a",
    code: "KeyA",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe("keyEventToCombo (macOS)", () => {
  it("returns null for modifier-only presses", () => {
    expect(keyEventToCombo(makeKeyEvent({ key: "Meta", code: "MetaLeft" }))).toBeNull();
    expect(keyEventToCombo(makeKeyEvent({ key: "Shift", code: "ShiftLeft" }))).toBeNull();
    expect(keyEventToCombo(makeKeyEvent({ key: "Control", code: "ControlLeft" }))).toBeNull();
    expect(keyEventToCombo(makeKeyEvent({ key: "Alt", code: "AltLeft" }))).toBeNull();
  });

  it("converts Cmd+D (Meta key on macOS)", () => {
    expect(keyEventToCombo(makeKeyEvent({ key: "d", code: "KeyD", metaKey: true }))).toBe("Cmd+D");
  });

  it("converts Cmd+Shift+D", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "D", code: "KeyD", metaKey: true, shiftKey: true,
    }))).toBe("Cmd+Shift+D");
  });

  it("converts Cmd+Alt+\\", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "\\", code: "Backslash", metaKey: true, altKey: true,
    }))).toBe("Cmd+Alt+\\");
  });

  it("converts function keys", () => {
    expect(keyEventToCombo(makeKeyEvent({ key: "F5", code: "F5" }))).toBe("F5");
  });

  it("converts Shift+F5", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "F5", code: "F5", shiftKey: true,
    }))).toBe("Shift+F5");
  });

  it("converts space to Space", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: " ", code: "Space", metaKey: true, shiftKey: true,
    }))).toBe("Cmd+Shift+Space");
  });

  it("converts arrow keys", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "ArrowUp", code: "ArrowUp", altKey: true,
    }))).toBe("Alt+Up");
  });

  it("converts standalone letter key (no modifiers)", () => {
    expect(keyEventToCombo(makeKeyEvent({ key: "a", code: "KeyA" }))).toBe("A");
  });

  it("handles Ctrl as separate from Cmd on macOS", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "1", code: "Digit1", metaKey: true, ctrlKey: true,
    }))).toBe("Cmd+Ctrl+1");
  });
});

describe("validateGlobalHotkeyCombo", () => {
  it("accepts letter keys", () => {
    expect(validateGlobalHotkeyCombo("Cmd+Shift+T")).toBe("Cmd+Shift+T");
  });

  it("accepts digit keys", () => {
    expect(validateGlobalHotkeyCombo("Cmd+1")).toBe("Cmd+1");
  });

  it("accepts function keys", () => {
    expect(validateGlobalHotkeyCombo("F5")).toBe("F5");
    expect(validateGlobalHotkeyCombo("Cmd+F12")).toBe("Cmd+F12");
    expect(validateGlobalHotkeyCombo("F24")).toBe("F24");
  });

  it("accepts named keys", () => {
    expect(validateGlobalHotkeyCombo("Cmd+Space")).toBe("Cmd+Space");
    expect(validateGlobalHotkeyCombo("Cmd+Enter")).toBe("Cmd+Enter");
    expect(validateGlobalHotkeyCombo("Cmd+Backspace")).toBe("Cmd+Backspace");
  });

  it("maps punctuation characters to global-hotkey names", () => {
    expect(validateGlobalHotkeyCombo("Cmd+,")).toBe("Cmd+Comma");
    expect(validateGlobalHotkeyCombo("Cmd+.")).toBe("Cmd+Period");
    expect(validateGlobalHotkeyCombo("Cmd+/")).toBe("Cmd+Slash");
    expect(validateGlobalHotkeyCombo("Cmd+;")).toBe("Cmd+Semicolon");
    expect(validateGlobalHotkeyCombo("Cmd+'")).toBe("Cmd+Quote");
    expect(validateGlobalHotkeyCombo("Cmd+[")).toBe("Cmd+BracketLeft");
    expect(validateGlobalHotkeyCombo("Cmd+]")).toBe("Cmd+BracketRight");
    expect(validateGlobalHotkeyCombo("Cmd+\\")).toBe("Cmd+Backslash");
    expect(validateGlobalHotkeyCombo("Cmd+`")).toBe("Cmd+Backquote");
    expect(validateGlobalHotkeyCombo("Cmd+-")).toBe("Cmd+Minus");
    expect(validateGlobalHotkeyCombo("Cmd+=")).toBe("Cmd+Equal");
  });

  it("rejects unsupported keys like <", () => {
    expect(() => validateGlobalHotkeyCombo("Cmd+<")).toThrow('The key "<" is not supported');
  });

  it("rejects other unsupported characters", () => {
    expect(() => validateGlobalHotkeyCombo("Cmd+>")).toThrow('The key ">" is not supported');
    expect(() => validateGlobalHotkeyCombo("Cmd+@")).toThrow('The key "@" is not supported');
  });

  it("rejects modifier-only combos", () => {
    expect(() => validateGlobalHotkeyCombo("Cmd+Shift")).toThrow("No key specified");
  });
});

describe("keyEventToCombo (Windows/Linux)", () => {
  beforeEach(async () => {
    const platform = await import("../../platform");
    (platform.isMacOS as ReturnType<typeof vi.fn>).mockReturnValue(false);
  });

  it("maps Ctrl to Cmd (primary modifier) on non-macOS", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "d", code: "KeyD", ctrlKey: true,
    }))).toBe("Cmd+D");
  });

  it("maps Ctrl+Shift+D on non-macOS", () => {
    expect(keyEventToCombo(makeKeyEvent({
      key: "D", code: "KeyD", ctrlKey: true, shiftKey: true,
    }))).toBe("Cmd+Shift+D");
  });
});
