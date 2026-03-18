import { describe, it, expect } from "vitest";
import {
  hotkeyToTauriShortcut, tauriShortcutToHotkey, isValidHotkey,
  parseHotkey, isPluginModifierKey, updateModifierState, modifiersMatch,
} from "../../utils/hotkey";
import type { ModifierState } from "../../utils/hotkey";

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

describe("isValidHotkey", () => {
  it("accepts function keys", () => {
    expect(isValidHotkey("F5")).toBe(true);
  });

  it("accepts modifier + key", () => {
    expect(isValidHotkey("Cmd+D")).toBe(true);
  });

  it("accepts multiple modifiers + key", () => {
    expect(isValidHotkey("Cmd+Shift+D")).toBe(true);
  });

  it("rejects modifier-only combinations", () => {
    expect(isValidHotkey("Shift+Cmd")).toBe(false);
  });

  it("rejects single modifier", () => {
    expect(isValidHotkey("Shift")).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isValidHotkey("")).toBe(false);
  });

  it("accepts standalone letter key", () => {
    expect(isValidHotkey("K")).toBe(true);
  });

  it("rejects Ctrl+Alt (modifiers only)", () => {
    expect(isValidHotkey("Ctrl+Alt")).toBe(false);
  });

  it("accepts macOS symbol modifiers with key", () => {
    expect(isValidHotkey("⌘+⇧+D")).toBe(true);
  });

  it("rejects macOS symbol modifiers only", () => {
    expect(isValidHotkey("⌘+⇧")).toBe(false);
  });
});

describe("parseHotkey", () => {
  it("parses standalone function key", () => {
    const p = parseHotkey("F5");
    expect(p).toEqual({ key: "F5", needCmd: false, needShift: false, needAlt: false, needCtrl: false });
  });

  it("parses Space", () => {
    const p = parseHotkey("Space");
    expect(p).toEqual({ key: "Space", needCmd: false, needShift: false, needAlt: false, needCtrl: false });
  });

  it("parses single letter key", () => {
    const p = parseHotkey("D");
    expect(p).toEqual({ key: "KeyD", needCmd: false, needShift: false, needAlt: false, needCtrl: false });
  });

  it("parses single digit key", () => {
    const p = parseHotkey("5");
    expect(p).toEqual({ key: "Num5", needCmd: false, needShift: false, needAlt: false, needCtrl: false });
  });

  it("parses Cmd+key", () => {
    const p = parseHotkey("Cmd+D");
    expect(p).toEqual({ key: "KeyD", needCmd: true, needShift: false, needAlt: false, needCtrl: false });
  });

  it("parses Cmd+Shift+key", () => {
    const p = parseHotkey("Cmd+Shift+D");
    expect(p).toEqual({ key: "KeyD", needCmd: true, needShift: true, needAlt: false, needCtrl: false });
  });

  it("parses macOS symbols", () => {
    const p = parseHotkey("⌘+⇧+D");
    expect(p).toEqual({ key: "KeyD", needCmd: true, needShift: true, needAlt: false, needCtrl: false });
  });

  it("returns null for invalid hotkey", () => {
    expect(parseHotkey("")).toBeNull();
    expect(parseHotkey("Shift")).toBeNull();
    expect(parseHotkey("Cmd+Shift")).toBeNull();
  });

  it("parses punctuation keys", () => {
    expect(parseHotkey("/")?.key).toBe("Slash");
    expect(parseHotkey("`")?.key).toBe("Grave");
  });
});

describe("isPluginModifierKey", () => {
  it("identifies modifier keys", () => {
    expect(isPluginModifierKey("MetaLeft")).toBe(true);
    expect(isPluginModifierKey("MetaRight")).toBe(true);
    expect(isPluginModifierKey("ShiftLeft")).toBe(true);
    expect(isPluginModifierKey("ControlRight")).toBe(true);
    expect(isPluginModifierKey("AltLeft")).toBe(true);
  });

  it("rejects non-modifier keys", () => {
    expect(isPluginModifierKey("F5")).toBe(false);
    expect(isPluginModifierKey("Space")).toBe(false);
    expect(isPluginModifierKey("KeyA")).toBe(false);
  });
});

describe("updateModifierState + modifiersMatch", () => {
  function freshMods(): ModifierState {
    return { cmd: false, shift: false, alt: false, ctrl: false };
  }

  it("tracks Meta press/release", () => {
    const m = freshMods();
    updateModifierState(m, "MetaLeft", true);
    expect(m.cmd).toBe(true);
    updateModifierState(m, "MetaLeft", false);
    expect(m.cmd).toBe(false);
  });

  it("tracks Shift press/release", () => {
    const m = freshMods();
    updateModifierState(m, "ShiftRight", true);
    expect(m.shift).toBe(true);
  });

  it("matches when all required modifiers are held", () => {
    const parsed = parseHotkey("Cmd+Shift+D")!;
    const m = freshMods();
    updateModifierState(m, "MetaLeft", true);
    updateModifierState(m, "ShiftLeft", true);
    expect(modifiersMatch(parsed, m)).toBe(true);
  });

  it("fails when extra modifier is held", () => {
    const parsed = parseHotkey("Cmd+D")!;
    const m = freshMods();
    updateModifierState(m, "MetaLeft", true);
    updateModifierState(m, "ShiftLeft", true); // extra
    expect(modifiersMatch(parsed, m)).toBe(false);
  });

  it("fails when required modifier is missing", () => {
    const parsed = parseHotkey("Cmd+D")!;
    const m = freshMods();
    // Meta not held
    expect(modifiersMatch(parsed, m)).toBe(false);
  });

  it("matches no-modifier hotkey when no modifiers held", () => {
    const parsed = parseHotkey("F5")!;
    const m = freshMods();
    expect(modifiersMatch(parsed, m)).toBe(true);
  });

  it("fails no-modifier hotkey when modifier held", () => {
    const parsed = parseHotkey("F5")!;
    const m = freshMods();
    updateModifierState(m, "AltLeft", true);
    expect(modifiersMatch(parsed, m)).toBe(false);
  });
});
