import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  TERMINAL_THEMES,
  THEME_NAMES,
  getTerminalTheme,
  APP_THEMES,
  getAppTheme,
  applyAppTheme,
  contrastRatio,
} from "../themes";

describe("themes", () => {
  it("TERMINAL_THEMES has entries for all THEME_NAMES keys", () => {
    for (const key of Object.keys(THEME_NAMES)) {
      expect(TERMINAL_THEMES[key]).toBeDefined();
    }
  });

  describe("getTerminalTheme()", () => {
    it("returns the commander theme", () => {
      const theme = getTerminalTheme("commander");
      expect(theme.background).toBe("#1e1e1e");
      expect(theme.foreground).toBe("#d4d4d4");
    });

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
        "textOnAccent", "textOnError", "textOnSuccess",
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

    it("applies all 16 CSS variables from theme", () => {
      applyAppTheme("nord");
      const style = document.documentElement.style;

      const expectedVars = [
        "--bg-primary", "--bg-secondary", "--bg-tertiary", "--bg-highlight",
        "--fg-primary", "--fg-secondary", "--fg-muted",
        "--accent", "--accent-hover", "--border",
        "--success", "--warning", "--error",
        "--text-on-accent", "--text-on-error", "--text-on-success",
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

  describe("contrastRatio()", () => {
    it("returns 21:1 for black on white", () => {
      expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 0);
    });

    it("returns 1:1 for identical colors", () => {
      expect(contrastRatio("#336699", "#336699")).toBeCloseTo(1, 1);
    });
  });

  describe("WCAG AA contrast compliance", () => {
    const MIN_CONTRAST = 4.5;
    const pairs: Array<{ bg: keyof typeof APP_THEMES extends string ? string : never; prop: "accent" | "error" | "success"; textProp: "textOnAccent" | "textOnError" | "textOnSuccess" }> = [
      { bg: "accent", prop: "accent", textProp: "textOnAccent" },
      { bg: "error", prop: "error", textProp: "textOnError" },
      { bg: "success", prop: "success", textProp: "textOnSuccess" },
    ];

    for (const [themeName, theme] of Object.entries(APP_THEMES)) {
      for (const { bg, prop, textProp } of pairs) {
        it(`${themeName}: ${textProp} on ${bg} has >= ${MIN_CONTRAST}:1 contrast`, () => {
          const bgColor = theme[prop];
          const fgColor = theme[textProp];
          const ratio = contrastRatio(bgColor, fgColor);
          expect(ratio, `${themeName} ${textProp}(${fgColor}) on ${bg}(${bgColor}) = ${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(MIN_CONTRAST);
        });
      }
    }
  });
});
