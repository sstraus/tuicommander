import { describe, it, expect } from "vitest";
import { keyToSequence } from "../../components/Terminal/terminalInput";

describe("keyToSequence", () => {
  const evt = (key: string, opts: Partial<KeyboardEvent> = {}): KeyboardEvent =>
    ({ key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...opts } as unknown as KeyboardEvent);

  // --- Printable characters ---
  it("passes printable ASCII as-is", () => {
    expect(keyToSequence(evt("a"))).toBe("a");
    expect(keyToSequence(evt("Z"))).toBe("Z");
    expect(keyToSequence(evt("1"))).toBe("1");
    expect(keyToSequence(evt(" "))).toBe(" ");
  });

  // --- Enter / Tab / Backspace / Escape ---
  it("maps Enter to CR", () => {
    expect(keyToSequence(evt("Enter"))).toBe("\r");
  });

  it("maps Tab to HT", () => {
    expect(keyToSequence(evt("Tab"))).toBe("\t");
  });

  it("maps Backspace to DEL", () => {
    expect(keyToSequence(evt("Backspace"))).toBe("\x7f");
  });

  it("maps Escape to ESC", () => {
    expect(keyToSequence(evt("Escape"))).toBe("\x1b");
  });

  // --- Arrow keys ---
  it("maps arrow keys to CSI sequences", () => {
    expect(keyToSequence(evt("ArrowUp"))).toBe("\x1b[A");
    expect(keyToSequence(evt("ArrowDown"))).toBe("\x1b[B");
    expect(keyToSequence(evt("ArrowRight"))).toBe("\x1b[C");
    expect(keyToSequence(evt("ArrowLeft"))).toBe("\x1b[D");
  });

  it("maps Shift+ArrowUp to modified CSI sequence", () => {
    expect(keyToSequence(evt("ArrowUp", { shiftKey: true }))).toBe("\x1b[1;2A");
  });

  it("maps Ctrl+ArrowRight", () => {
    expect(keyToSequence(evt("ArrowRight", { ctrlKey: true }))).toBe("\x1b[1;5C");
  });

  // --- Function keys ---
  it("maps F1-F4", () => {
    expect(keyToSequence(evt("F1"))).toBe("\x1bOP");
    expect(keyToSequence(evt("F2"))).toBe("\x1bOQ");
    expect(keyToSequence(evt("F3"))).toBe("\x1bOR");
    expect(keyToSequence(evt("F4"))).toBe("\x1bOS");
  });

  it("maps F5-F12", () => {
    expect(keyToSequence(evt("F5"))).toBe("\x1b[15~");
    expect(keyToSequence(evt("F6"))).toBe("\x1b[17~");
    expect(keyToSequence(evt("F7"))).toBe("\x1b[18~");
    expect(keyToSequence(evt("F8"))).toBe("\x1b[19~");
    expect(keyToSequence(evt("F9"))).toBe("\x1b[20~");
    expect(keyToSequence(evt("F10"))).toBe("\x1b[21~");
    expect(keyToSequence(evt("F11"))).toBe("\x1b[23~");
    expect(keyToSequence(evt("F12"))).toBe("\x1b[24~");
  });

  // --- Ctrl+letter ---
  it("maps Ctrl+C to ETX (0x03)", () => {
    expect(keyToSequence(evt("c", { ctrlKey: true }))).toBe("\x03");
  });

  it("maps Ctrl+A to SOH (0x01)", () => {
    expect(keyToSequence(evt("a", { ctrlKey: true }))).toBe("\x01");
  });

  it("maps Ctrl+Z to SUB (0x1a)", () => {
    expect(keyToSequence(evt("z", { ctrlKey: true }))).toBe("\x1a");
  });

  // --- Alt+letter ---
  it("maps Alt+letter to ESC + char", () => {
    expect(keyToSequence(evt("d", { altKey: true }))).toBe("\x1bd");
    expect(keyToSequence(evt("f", { altKey: true }))).toBe("\x1bf");
  });

  // --- Navigation keys ---
  it("maps Home/End/Insert/Delete/PageUp/PageDown", () => {
    expect(keyToSequence(evt("Home"))).toBe("\x1b[H");
    expect(keyToSequence(evt("End"))).toBe("\x1b[F");
    expect(keyToSequence(evt("Insert"))).toBe("\x1b[2~");
    expect(keyToSequence(evt("Delete"))).toBe("\x1b[3~");
    expect(keyToSequence(evt("PageUp"))).toBe("\x1b[5~");
    expect(keyToSequence(evt("PageDown"))).toBe("\x1b[6~");
  });

  // --- Meta (Cmd) keys return null (not intercepted) ---
  it("returns null for meta-modified keys", () => {
    expect(keyToSequence(evt("c", { metaKey: true }))).toBeNull();
    expect(keyToSequence(evt("v", { metaKey: true }))).toBeNull();
  });

  // --- Non-character keys return null ---
  it("returns null for modifier-only keys", () => {
    expect(keyToSequence(evt("Shift"))).toBeNull();
    expect(keyToSequence(evt("Control"))).toBeNull();
    expect(keyToSequence(evt("Alt"))).toBeNull();
    expect(keyToSequence(evt("Meta"))).toBeNull();
    expect(keyToSequence(evt("CapsLock"))).toBeNull();
  });
});
