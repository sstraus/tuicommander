import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const css = readFileSync(
  resolve(__dirname, "../components/CommandInput.module.css"),
  "utf-8",
);

const tsx = readFileSync(
  resolve(__dirname, "../components/CommandInput.tsx"),
  "utf-8",
);

describe("CommandInput agent live-sync guard", () => {
  it("handleInput skips debouncedSync when agentType prop is set", () => {
    // Agent sessions must NOT live-sync to PTY (Ctrl-U doesn't work in custom line editors).
    // Verify the guard: debouncedSync is only called when agentType is falsy.
    expect(tsx).toContain("if (!props.agentType)");
    // The debouncedSync call must be inside the guard, not unconditional
    const handleInputBlock = tsx.match(/function handleInput[\s\S]*?^  \}/m);
    expect(handleInputBlock, "handleInput function not found").toBeTruthy();
    expect(handleInputBlock![0]).toContain("if (!props.agentType)");
    expect(handleInputBlock![0]).toContain("debouncedSync(text)");
  });

  it("agentType prop is declared in CommandInputProps", () => {
    expect(tsx).toContain("agentType?: string | null");
  });
});

describe("CommandInput iOS auto-zoom prevention", () => {
  it("input font-size is >= 16px to prevent iOS auto-zoom", () => {
    // iOS Safari zooms when the focused input has font-size < 16px.
    const match = css.match(/\.input\s*\{[^}]*font-size:\s*(\d+)px/s);
    expect(match, "font-size not found in .input rule of CommandInput.module.css").toBeTruthy();
    const fontSizePx = parseInt(match![1], 10);
    expect(fontSizePx).toBeGreaterThanOrEqual(16);
  });

  it('input element has inputmode="text"', () => {
    // inputmode="text" prevents keyboard layout switching on mobile.
    expect(tsx).toContain('inputmode="text"');
  });
});
