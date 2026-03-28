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

describe("CommandInput no live-sync to PTY", () => {
  it("handleInput does NOT write to PTY (no debouncedSync)", () => {
    // Mobile CommandInput must never live-sync to PTY — this caused echo duplication.
    // Input is only written on explicit send().
    expect(tsx).not.toContain("debouncedSync");
    expect(tsx).not.toContain("syncToPty");
    const handleInputBlock = tsx.match(/function handleInput[\s\S]*?^  \}/m);
    expect(handleInputBlock, "handleInput function not found").toBeTruthy();
    expect(handleInputBlock![0]).not.toContain("rpc(");
  });

  it("agentType prop is declared in CommandInputProps", () => {
    expect(tsx).toContain("agentType?: string | null");
  });
});

describe("CommandInput send() agent-aware write splitting", () => {
  it("send() delegates to shared sendCommand utility", () => {
    // Agent-aware write splitting is handled by the shared sendCommand utility.
    // CommandInput passes agentType so Ink agents get split writes.
    expect(tsx).toContain("sendCommand");
    expect(tsx).toContain("props.agentType");
  });

  it("sendCommand is imported from shared utils", () => {
    expect(tsx).toContain('from "../../utils/sendCommand"');
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
