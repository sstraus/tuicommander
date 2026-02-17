import { describe, it, expect } from "vitest";
import { hotkeyToTauriShortcut, tauriShortcutToHotkey } from "../../utils/hotkey";

describe("hotkeyToTauriShortcut", () => {
  it("converts Cmd modifier to CommandOrControl", () => {
    expect(hotkeyToTauriShortcut("Cmd+D")).toBe("CommandOrControl+D");
  });

  it("converts Cmd in multi-modifier hotkeys", () => {
    expect(hotkeyToTauriShortcut("Cmd+Shift+D")).toBe("CommandOrControl+Shift+D");
  });

  it("passes through standalone function keys unchanged", () => {
    expect(hotkeyToTauriShortcut("F5")).toBe("F5");
  });

  it("passes through Ctrl modifier unchanged", () => {
    expect(hotkeyToTauriShortcut("Ctrl+K")).toBe("Ctrl+K");
  });

  it("passes through Alt modifier unchanged", () => {
    expect(hotkeyToTauriShortcut("Alt+F5")).toBe("Alt+F5");
  });

  it("handles Cmd+Alt combination", () => {
    expect(hotkeyToTauriShortcut("Cmd+Alt+M")).toBe("CommandOrControl+Alt+M");
  });

  it("handles empty string", () => {
    expect(hotkeyToTauriShortcut("")).toBe("");
  });

  it("converts macOS % symbol (Cmd) to CommandOrControl", () => {
    expect(hotkeyToTauriShortcut("%+D")).toBe("CommandOrControl+D");
  });

  it("converts macOS ⌘ symbol to CommandOrControl", () => {
    expect(hotkeyToTauriShortcut("⌘+D")).toBe("CommandOrControl+D");
  });

  it("converts macOS ⇧ symbol to Shift", () => {
    expect(hotkeyToTauriShortcut("⌘+⇧+D")).toBe("CommandOrControl+Shift+D");
  });

  it("converts macOS ⌥ symbol to Alt", () => {
    expect(hotkeyToTauriShortcut("⌥+F5")).toBe("Alt+F5");
  });

  it("converts macOS ⌃ symbol to Ctrl", () => {
    expect(hotkeyToTauriShortcut("⌃+K")).toBe("Ctrl+K");
  });
});

describe("tauriShortcutToHotkey", () => {
  it("converts CommandOrControl to Cmd", () => {
    expect(tauriShortcutToHotkey("CommandOrControl+D")).toBe("Cmd+D");
  });

  it("passes through standalone function keys unchanged", () => {
    expect(tauriShortcutToHotkey("F5")).toBe("F5");
  });

  it("handles empty string", () => {
    expect(tauriShortcutToHotkey("")).toBe("");
  });
});
