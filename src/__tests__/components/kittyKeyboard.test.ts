import { describe, it, expect } from "vitest";
import { kittySequenceForKey } from "../../components/Terminal/kittyKeyboard";

describe("kittySequenceForKey", () => {
  // Plain keys without modifiers — should return null (legacy encoding)
  it("plain Enter returns null", () => {
    expect(kittySequenceForKey("Enter", false, false, false, false)).toBeNull();
  });

  it("plain Tab returns null", () => {
    expect(kittySequenceForKey("Tab", false, false, false, false)).toBeNull();
  });

  it("plain Backspace returns null", () => {
    expect(kittySequenceForKey("Backspace", false, false, false, false)).toBeNull();
  });

  // Escape is always encoded in kitty mode (even without modifiers)
  it("plain Escape returns CSI 27 u", () => {
    expect(kittySequenceForKey("Escape", false, false, false, false)).toBe("\x1b[27u");
  });

  // Modified Enter
  it("Shift+Enter returns CSI 13;2u", () => {
    expect(kittySequenceForKey("Enter", true, false, false, false)).toBe("\x1b[13;2u");
  });

  it("Ctrl+Enter returns CSI 13;5u", () => {
    expect(kittySequenceForKey("Enter", false, false, true, false)).toBe("\x1b[13;5u");
  });

  it("Ctrl+Shift+Enter returns CSI 13;6u", () => {
    expect(kittySequenceForKey("Enter", true, false, true, false)).toBe("\x1b[13;6u");
  });

  it("Alt+Enter returns CSI 13;3u", () => {
    expect(kittySequenceForKey("Enter", false, true, false, false)).toBe("\x1b[13;3u");
  });

  // Modified Escape
  it("Shift+Escape returns CSI 27;2u", () => {
    expect(kittySequenceForKey("Escape", true, false, false, false)).toBe("\x1b[27;2u");
  });

  // Modified Tab
  it("Shift+Tab returns CSI 9;2u", () => {
    expect(kittySequenceForKey("Tab", true, false, false, false)).toBe("\x1b[9;2u");
  });

  // Modified Backspace
  it("Ctrl+Backspace returns CSI 127;5u", () => {
    expect(kittySequenceForKey("Backspace", false, false, true, false)).toBe("\x1b[127;5u");
  });

  it("Shift+Backspace returns CSI 127;2u", () => {
    expect(kittySequenceForKey("Backspace", true, false, false, false)).toBe("\x1b[127;2u");
  });

  // Meta key (macOS Cmd) — always returns null (pass through to OS)
  it("Meta+Enter returns null", () => {
    expect(kittySequenceForKey("Enter", false, false, false, true)).toBeNull();
  });

  it("Meta+Escape returns null", () => {
    expect(kittySequenceForKey("Escape", false, false, false, true)).toBeNull();
  });

  // Unknown/unhandled keys return null
  it("regular letter 'a' returns null", () => {
    expect(kittySequenceForKey("a", false, false, false, false)).toBeNull();
  });

  it("Shift+A returns null", () => {
    expect(kittySequenceForKey("A", true, false, false, false)).toBeNull();
  });

  it("Ctrl+C returns null", () => {
    expect(kittySequenceForKey("c", false, false, true, false)).toBeNull();
  });

  it("F1 returns null", () => {
    expect(kittySequenceForKey("F1", false, false, false, false)).toBeNull();
  });

  // Combined modifier math: shift(1) + alt(2) + ctrl(4) = 7, encoded as 8 (7+1)
  it("Ctrl+Alt+Shift+Enter returns CSI 13;8u", () => {
    expect(kittySequenceForKey("Enter", true, true, true, false)).toBe("\x1b[13;8u");
  });
});
