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

describe("CommandInput send() agent-aware write splitting", () => {
  it("send() uses two separate writes for agent sessions", () => {
    // Ink-based TUIs treat \\r as newline when combined with text in one write.
    // Agent sessions must split: (1) \\x15 + text, (2) \\r — matching CommandWidget pattern.
    const sendBlock = tsx.match(/async function send\(\)[\s\S]*?^  \}/m);
    expect(sendBlock, "send() function not found").toBeTruthy();
    const sendCode = sendBlock![0];
    // Must check agentType to decide write strategy
    expect(sendCode).toContain("props.agentType");
  });

  it("send() uses single atomic write for shell sessions", () => {
    // Shell sessions (no agentType) use Ctrl-U + text + \\r in one write — kernel line discipline handles it.
    const sendBlock = tsx.match(/async function send\(\)[\s\S]*?^  \}/m);
    expect(sendBlock, "send() function not found").toBeTruthy();
    const sendCode = sendBlock![0];
    // Must still have the single-write path with \\x15 + text + \\r
    expect(sendCode).toContain('"\\x15" + text + "\\r"');
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
