import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  TERMINAL_THEMES,
  THEME_NAMES,
  getTerminalTheme,
  APP_THEMES,
  getAppTheme,
  applyAppTheme,
} from "../themes";

describe("themes", () => {
  it("TERMINAL_THEMES has entries for all THEME_NAMES keys", () => {
    for (const key of Object.keys(THEME_NAMES)) {
      expect(TERMINAL_THEMES[key]).toBeDefined();
    }
  });

  describe("getTerminalTheme()", () => {
    it("returns the requested theme", () => {
      const theme = getTerminalTheme("dracula");
      expect(theme.background).toBe("#282a36");
    });

    it("falls back to vscode-dark for unknown key", () => {
      const theme = getTerminalTheme("nonexistent-theme");
      expect(theme).toBe(TERMINAL_THEMES["vscode-dark"]);
    });
  });

  describe("APP_THEMES", () => {
    it("has an entry for every THEME_NAMES key", () => {
      for (const key of Object.keys(THEME_NAMES)) {
        expect(APP_THEMES[key]).toBeDefined();
      }
    });

    it("each entry has all required color properties", () => {
      const requiredKeys = [
        "bgPrimary", "bgSecondary", "bgTertiary", "bgHighlight",
        "fgPrimary", "fgSecondary", "fgMuted",
        "accent", "accentHover", "border",
        "success", "warning", "error",
      ];
      for (const [themeName, theme] of Object.entries(APP_THEMES)) {
        for (const key of requiredKeys) {
          expect(theme).toHaveProperty(key);
          expect((theme as unknown as Record<string, string>)[key], `${themeName}.${key}`).toMatch(/^#[0-9a-f]{6}$/i);
        }
      }
    });
  });

  describe("getAppTheme()", () => {
    it("returns the requested app theme", () => {
      const theme = getAppTheme("dracula");
      expect(theme.bgPrimary).toBe("#282a36");
    });

    it("falls back to vscode-dark for unknown key", () => {
      const theme = getAppTheme("nonexistent-theme");
      expect(theme).toBe(APP_THEMES["vscode-dark"]);
    });
  });

  describe("applyAppTheme()", () => {
    beforeEach(() => {
      // Reset inline styles between tests
      document.documentElement.style.cssText = "";
    });

    it("sets CSS custom properties on document.documentElement", () => {
      applyAppTheme("dracula");
      const style = document.documentElement.style;
      expect(style.getPropertyValue("--bg-primary")).toBe("#282a36");
      expect(style.getPropertyValue("--accent")).toBe("#bd93f9");
    });

    it("falls back to vscode-dark for unknown theme", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      applyAppTheme("nonexistent");
      const style = document.documentElement.style;
      expect(style.getPropertyValue("--bg-primary")).toBe("#1e1e1e");
      warnSpy.mockRestore();
    });

    it("warns when applying unknown theme", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      applyAppTheme("nonexistent-theme");
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("nonexistent-theme"));
      warnSpy.mockRestore();
    });

    it("applies all 13 CSS variables from theme", () => {
      applyAppTheme("nord");
      const style = document.documentElement.style;

      const expectedVars = [
        "--bg-primary", "--bg-secondary", "--bg-tertiary", "--bg-highlight",
        "--fg-primary", "--fg-secondary", "--fg-muted",
        "--accent", "--accent-hover", "--border",
        "--success", "--warning", "--error",
      ];

      for (const varName of expectedVars) {
        const value = style.getPropertyValue(varName);
        expect(value, `${varName} should be set`).toBeTruthy();
        expect(value, `${varName} should be a hex color`).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });

    it("overwrites previously applied theme", () => {
      applyAppTheme("dracula");
      applyAppTheme("nord");
      const style = document.documentElement.style;
      expect(style.getPropertyValue("--bg-primary")).toBe("#2e3440");
    });
  });
});
