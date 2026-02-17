import { describe, it, expect, afterEach } from "vitest";
import { detectPlatform, isMacOS, isWindows, isLinux, applyPlatformClass, getModifierSymbol, isQuickSwitcherActive, isQuickSwitcherRelease } from "../platform";

describe("platform detection", () => {
  const originalPlatform = Object.getOwnPropertyDescriptor(navigator, "platform");

  function mockPlatform(value: string) {
    Object.defineProperty(navigator, "platform", {
      value,
      writable: true,
      configurable: true,
    });
  }

  afterEach(() => {
    if (originalPlatform) {
      Object.defineProperty(navigator, "platform", originalPlatform);
    }
  });

  describe("detectPlatform", () => {
    it("detects macOS", () => {
      mockPlatform("MacIntel");
      expect(detectPlatform()).toBe("macos");
    });

    it("detects Windows", () => {
      mockPlatform("Win32");
      expect(detectPlatform()).toBe("windows");
    });

    it("detects Linux", () => {
      mockPlatform("Linux x86_64");
      expect(detectPlatform()).toBe("linux");
    });

    it("detects X11 as Linux", () => {
      mockPlatform("X11");
      expect(detectPlatform()).toBe("linux");
    });

    it("returns unknown for unrecognized platforms", () => {
      mockPlatform("SomeOtherOS");
      expect(detectPlatform()).toBe("unknown");
    });
  });

  describe("isMacOS", () => {
    it("returns true on Mac", () => {
      mockPlatform("MacIntel");
      expect(isMacOS()).toBe(true);
    });

    it("returns false on Windows", () => {
      mockPlatform("Win32");
      expect(isMacOS()).toBe(false);
    });
  });

  describe("isWindows", () => {
    it("returns true on Windows", () => {
      mockPlatform("Win32");
      expect(isWindows()).toBe(true);
    });

    it("returns false on Mac", () => {
      mockPlatform("MacIntel");
      expect(isWindows()).toBe(false);
    });
  });

  describe("isLinux", () => {
    it("returns true on Linux", () => {
      mockPlatform("Linux x86_64");
      expect(isLinux()).toBe(true);
    });

    it("returns false on Mac", () => {
      mockPlatform("MacIntel");
      expect(isLinux()).toBe(false);
    });
  });

  describe("applyPlatformClass", () => {
    it("adds platform class to document root", () => {
      mockPlatform("MacIntel");
      const result = applyPlatformClass();
      expect(result).toBe("macos");
      expect(document.documentElement.classList.contains("platform-macos")).toBe(true);
      // Clean up
      document.documentElement.classList.remove("platform-macos");
    });
  });

  describe("getModifierSymbol", () => {
    it("returns ⌘ on macOS", () => {
      mockPlatform("MacIntel");
      expect(getModifierSymbol()).toBe("\u2318");
    });

    it("returns Ctrl+ on Windows", () => {
      mockPlatform("Win32");
      expect(getModifierSymbol()).toBe("Ctrl+");
    });

    it("returns Ctrl+ on Linux", () => {
      mockPlatform("Linux x86_64");
      expect(getModifierSymbol()).toBe("Ctrl+");
    });
  });

  describe("isQuickSwitcherActive", () => {
    it("detects Cmd+Ctrl on macOS", () => {
      mockPlatform("MacIntel");
      const e = new KeyboardEvent("keydown", { metaKey: true, ctrlKey: true });
      expect(isQuickSwitcherActive(e)).toBe(true);
    });

    it("rejects Cmd-only on macOS", () => {
      mockPlatform("MacIntel");
      const e = new KeyboardEvent("keydown", { metaKey: true, ctrlKey: false });
      expect(isQuickSwitcherActive(e)).toBe(false);
    });

    it("detects Ctrl+Alt on Windows", () => {
      mockPlatform("Win32");
      const e = new KeyboardEvent("keydown", { ctrlKey: true, altKey: true });
      expect(isQuickSwitcherActive(e)).toBe(true);
    });

    it("rejects Ctrl-only on Windows", () => {
      mockPlatform("Win32");
      const e = new KeyboardEvent("keydown", { ctrlKey: true, altKey: false });
      expect(isQuickSwitcherActive(e)).toBe(false);
    });
  });

  describe("isQuickSwitcherRelease", () => {
    it("detects Meta release on macOS", () => {
      mockPlatform("MacIntel");
      const e = new KeyboardEvent("keyup", { key: "Meta" });
      expect(isQuickSwitcherRelease(e)).toBe(true);
    });

    it("detects Control release on macOS", () => {
      mockPlatform("MacIntel");
      const e = new KeyboardEvent("keyup", { key: "Control" });
      expect(isQuickSwitcherRelease(e)).toBe(true);
    });

    it("detects Alt release on Windows", () => {
      mockPlatform("Win32");
      const e = new KeyboardEvent("keyup", { key: "Alt" });
      expect(isQuickSwitcherRelease(e)).toBe(true);
    });

    it("rejects non-modifier key on Windows", () => {
      mockPlatform("Win32");
      const e = new KeyboardEvent("keyup", { key: "a" });
      expect(isQuickSwitcherRelease(e)).toBe(false);
    });
  });
});
