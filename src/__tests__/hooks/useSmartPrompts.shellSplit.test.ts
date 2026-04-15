import { describe, it, expect } from "vitest";
import { shellSplit } from "../../hooks/useSmartPrompts";

describe("shellSplit", () => {
  it("splits plain whitespace-separated tokens", () => {
    expect(shellSplit("claude -p hello")).toEqual(["claude", "-p", "hello"]);
  });

  it("preserves double-quoted strings with spaces", () => {
    expect(shellSplit(`claude --system-prompt "be brief" -p "{prompt}"`)).toEqual([
      "claude",
      "--system-prompt",
      "be brief",
      "-p",
      "{prompt}",
    ]);
  });

  it("preserves single-quoted strings verbatim including backslashes", () => {
    expect(shellSplit(`cmd 'a\\nb' 'c d'`)).toEqual(["cmd", "a\\nb", "c d"]);
  });

  it("does NOT expand backticks, command substitution, or semicolons — metacharacters are literal", () => {
    // These would execute if handed to a shell; shellSplit returns them as literal tokens.
    const tokens = shellSplit("echo `whoami`; rm -rf /tmp/boom $(id)");
    expect(tokens).toEqual(["echo", "`whoami`;", "rm", "-rf", "/tmp/boom", "$(id)"]);
  });

  it("handles empty input", () => {
    expect(shellSplit("")).toEqual([]);
    expect(shellSplit("   ")).toEqual([]);
  });

  it("keeps empty quoted strings as a distinct token", () => {
    expect(shellSplit(`cmd "" arg`)).toEqual(["cmd", "", "arg"]);
  });

  it("unescapes backslash outside single quotes in double-quoted context", () => {
    expect(shellSplit(`cmd "a\\"b"`)).toEqual(["cmd", `a"b`]);
  });
});
