import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const tsx = readFileSync(
  resolve(__dirname, "../components/TerminalKeybar.tsx"),
  "utf-8",
);

describe("TerminalKeybar agent-aware confirm keys", () => {
  it("defines INK_AGENTS set with claude, codex, opencode", () => {
    expect(tsx).toContain('new Set(["claude", "codex", "opencode"])');
  });

  it("getConfirmKeys returns Enter/Escape for Ink agents with confident questions", () => {
    // Ink multiselect menus: Enter selects, Escape cancels
    expect(tsx).toContain('{ label: "Yes", seq: "\\r", confirm: true }');
    expect(tsx).toContain('{ label: "No", seq: "\\x1b", confirm: true }');
  });

  it("getConfirmKeys returns y+Enter / n+Enter for text-based prompts", () => {
    // Aider Y/N, generic shell prompts, non-confident detections
    expect(tsx).toContain('{ label: "Yes", seq: "y\\r", confirm: true }');
    expect(tsx).toContain('{ label: "No", seq: "n\\r", confirm: true }');
  });

  it("passes questionConfident prop to determine key behavior", () => {
    expect(tsx).toContain("questionConfident");
    expect(tsx).toContain("getConfirmKeys(props.agentType, props.questionConfident)");
  });

  it("falls back to text-based keys when agent is unknown", () => {
    // Non-Ink agents or unknown agent types use y/n text input
    expect(tsx).toContain("const isInkAgent = agentType ? INK_AGENTS.has(agentType) : false");
  });
});
