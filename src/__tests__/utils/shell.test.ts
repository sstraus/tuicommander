import { describe, it, expect } from "vitest";
import { escapeShellArg, isValidBranchName, isValidPath } from "../../utils/shell";

describe("escapeShellArg", () => {
  it("wraps simple strings in single quotes", () => {
    expect(escapeShellArg("hello")).toBe("'hello'");
  });

  it("escapes embedded single quotes", () => {
    expect(escapeShellArg("it's")).toBe("'it'\\''s'");
  });

  it("handles empty string", () => {
    expect(escapeShellArg("")).toBe("''");
  });

  it("handles strings with spaces", () => {
    expect(escapeShellArg("hello world")).toBe("'hello world'");
  });

  it("handles strings with special shell characters", () => {
    expect(escapeShellArg("$(rm -rf /)")).toBe("'$(rm -rf /)'");
  });

  it("handles strings with backticks", () => {
    expect(escapeShellArg("`whoami`")).toBe("'`whoami`'");
  });

  it("handles strings with multiple single quotes", () => {
    expect(escapeShellArg("it's a 'test'")).toBe("'it'\\''s a '\\''test'\\'''");
  });
});

describe("isValidBranchName", () => {
  it("accepts simple branch names", () => {
    expect(isValidBranchName("main")).toBe(true);
    expect(isValidBranchName("feature/test")).toBe(true);
    expect(isValidBranchName("wip/my-branch")).toBe(true);
  });

  it("rejects empty string", () => {
    expect(isValidBranchName("")).toBe(false);
  });

  it("rejects names longer than 255 chars", () => {
    expect(isValidBranchName("a".repeat(256))).toBe(false);
  });

  it("accepts names exactly 255 chars", () => {
    expect(isValidBranchName("a".repeat(255))).toBe(true);
  });

  it("rejects names with tilde", () => {
    expect(isValidBranchName("branch~1")).toBe(false);
  });

  it("rejects names with caret", () => {
    expect(isValidBranchName("branch^1")).toBe(false);
  });

  it("rejects names with colon", () => {
    expect(isValidBranchName("branch:name")).toBe(false);
  });

  it("rejects names with backslash", () => {
    expect(isValidBranchName("branch\\name")).toBe(false);
  });

  it("rejects names with asterisk", () => {
    expect(isValidBranchName("branch*")).toBe(false);
  });

  it("rejects names with question mark", () => {
    expect(isValidBranchName("branch?")).toBe(false);
  });

  it("rejects names with bracket", () => {
    expect(isValidBranchName("branch[1]")).toBe(false);
  });

  it("rejects names with @{", () => {
    expect(isValidBranchName("branch@{1}")).toBe(false);
  });

  it("rejects names with double dots", () => {
    expect(isValidBranchName("branch..name")).toBe(false);
  });

  it("rejects names with double slashes", () => {
    expect(isValidBranchName("branch//name")).toBe(false);
  });

  it("rejects names starting with dot", () => {
    expect(isValidBranchName(".branch")).toBe(false);
  });

  it("rejects names with slash-dot", () => {
    expect(isValidBranchName("branch/.name")).toBe(false);
  });

  it("rejects names ending with .lock", () => {
    expect(isValidBranchName("branch.lock")).toBe(false);
  });

  it("rejects names starting with slash", () => {
    expect(isValidBranchName("/branch")).toBe(false);
  });

  it("rejects names ending with slash", () => {
    expect(isValidBranchName("branch/")).toBe(false);
  });

  it("rejects names with spaces", () => {
    expect(isValidBranchName("branch name")).toBe(false);
  });

  it("rejects names with control characters", () => {
    expect(isValidBranchName("branch\x00name")).toBe(false);
    expect(isValidBranchName("branch\x1fname")).toBe(false);
  });
});

describe("isValidPath", () => {
  it("accepts normal paths", () => {
    expect(isValidPath("/home/user/project")).toBe(true);
    expect(isValidPath("relative/path")).toBe(true);
  });

  it("rejects empty string", () => {
    expect(isValidPath("")).toBe(false);
  });

  it("rejects null bytes", () => {
    expect(isValidPath("/path\0with\0nulls")).toBe(false);
  });

  it("rejects paths starting with semicolon", () => {
    expect(isValidPath("; rm -rf /")).toBe(false);
  });

  it("rejects paths starting with ampersand", () => {
    expect(isValidPath("& rm -rf /")).toBe(false);
  });

  it("rejects paths starting with pipe", () => {
    expect(isValidPath("| cat /etc/passwd")).toBe(false);
  });

  it("rejects paths starting with backtick", () => {
    expect(isValidPath("`whoami`")).toBe(false);
  });

  it("rejects paths starting with dollar sign", () => {
    expect(isValidPath("$(whoami)")).toBe(false);
  });

  it("accepts paths starting with alphanumeric", () => {
    expect(isValidPath("myproject")).toBe(true);
  });
});
