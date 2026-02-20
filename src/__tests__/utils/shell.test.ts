import { describe, it, expect, vi, afterEach } from "vitest";
import { escapeShellArg, isValidBranchName, isValidPath } from "../../utils/shell";
import * as platform from "../../platform";

// Spy on isWindows so we can toggle platform per test
const isWindowsSpy = vi.spyOn(platform, "isWindows");

afterEach(() => {
  isWindowsSpy.mockReset();
});

describe("escapeShellArg (POSIX)", () => {
  it("wraps simple strings in single quotes", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("hello")).toBe("'hello'");
  });

  it("escapes embedded single quotes", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("it's")).toBe("'it'\\''s'");
  });

  it("handles empty string", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("")).toBe("''");
  });

  it("handles strings with spaces", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("hello world")).toBe("'hello world'");
  });

  it("handles strings with special shell characters", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("$(rm -rf /)")).toBe("'$(rm -rf /)'");
  });

  it("handles strings with backticks", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("`whoami`")).toBe("'`whoami`'");
  });

  it("handles strings with multiple single quotes", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(escapeShellArg("it's a 'test'")).toBe("'it'\\''s a '\\''test'\\'''");
  });
});

describe("escapeShellArg (Windows)", () => {
  it("wraps simple strings in double quotes", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(escapeShellArg("hello")).toBe('"hello"');
  });

  it("doubles embedded double quotes", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(escapeShellArg('say "hi"')).toBe('"say ""hi"""');
  });

  it("escapes cmd.exe metacharacters with caret", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(escapeShellArg("a&b")).toBe('"a^&b"');
    expect(escapeShellArg("a|b")).toBe('"a^|b"');
    expect(escapeShellArg("a<b>c")).toBe('"a^<b^>c"');
    expect(escapeShellArg("100%")).toBe('"100^%"');
  });

  it("handles empty string", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(escapeShellArg("")).toBe('""');
  });

  it("handles strings with spaces", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(escapeShellArg("hello world")).toBe('"hello world"');
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

describe("isValidPath (POSIX)", () => {
  it("accepts normal paths", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("/home/user/project")).toBe(true);
    expect(isValidPath("relative/path")).toBe(true);
  });

  it("rejects empty string", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("")).toBe(false);
  });

  it("rejects null bytes", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("/path\0with\0nulls")).toBe(false);
  });

  it("rejects paths starting with semicolon", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("; rm -rf /")).toBe(false);
  });

  it("rejects paths starting with ampersand", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("& rm -rf /")).toBe(false);
  });

  it("rejects paths starting with pipe", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("| cat /etc/passwd")).toBe(false);
  });

  it("rejects paths starting with backtick", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("`whoami`")).toBe(false);
  });

  it("rejects paths starting with dollar sign", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("$(whoami)")).toBe(false);
  });

  it("accepts paths starting with alphanumeric", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("myproject")).toBe(true);
  });

  it("allows percent and caret on POSIX", () => {
    isWindowsSpy.mockReturnValue(false);
    expect(isValidPath("/path/with%20space")).toBe(true);
    expect(isValidPath("/path/caret^here")).toBe(true);
  });
});

describe("isValidPath (Windows)", () => {
  it("accepts normal Windows paths", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(isValidPath("C:\\Users\\me\\project")).toBe(true);
    expect(isValidPath("relative\\path")).toBe(true);
  });

  it("rejects paths with percent (env var expansion)", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(isValidPath("C:\\%PATH%\\file")).toBe(false);
  });

  it("rejects paths with caret (cmd.exe escape)", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(isValidPath("C:\\path^file")).toBe(false);
  });

  it("still rejects common injection on Windows", () => {
    isWindowsSpy.mockReturnValue(true);
    expect(isValidPath("; del /s /q C:\\")).toBe(false);
    expect(isValidPath("& net user")).toBe(false);
    expect(isValidPath("| type C:\\secret")).toBe(false);
  });
});
