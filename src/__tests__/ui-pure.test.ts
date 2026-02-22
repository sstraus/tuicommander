import { describe, it, expect } from "vitest";
import { stripAnsi } from "../utils/stripAnsi";
import { parseDiff } from "../components/ui/DiffViewer";
import { validateBranchName } from "../components/RenameBranchDialog/RenameBranchDialog";
import { cleanOscTitle } from "../components/Terminal/Terminal";

describe("stripAnsi", () => {
  it("strips color codes", () => {
    expect(stripAnsi("\x1B[31mred\x1B[0m")).toBe("red");
  });

  it("strips bold codes", () => {
    expect(stripAnsi("\x1B[1mbold\x1B[22m")).toBe("bold");
  });

  it("leaves plain text unchanged", () => {
    expect(stripAnsi("hello world")).toBe("hello world");
  });

  it("handles empty string", () => {
    expect(stripAnsi("")).toBe("");
  });

  it("strips multiple ANSI codes", () => {
    expect(stripAnsi("\x1B[32m\x1B[1mgreen bold\x1B[0m")).toBe("green bold");
  });

  it("strips codes with multiple parameters", () => {
    expect(stripAnsi("\x1B[38;5;196mred256\x1B[0m")).toBe("red256");
  });
});

describe("parseDiff", () => {
  it("identifies header lines", () => {
    const lines = parseDiff("diff --git a/file.ts b/file.ts");
    expect(lines[0].type).toBe("header");
    expect(lines[0].content).toBe("diff --git a/file.ts b/file.ts");
  });

  it("identifies hunk lines", () => {
    const lines = parseDiff("@@ -1,5 +1,6 @@");
    expect(lines[0].type).toBe("hunk");
  });

  it("identifies addition lines", () => {
    const lines = parseDiff("+new line");
    expect(lines[0].type).toBe("addition");
  });

  it("does not treat +++ as addition", () => {
    const lines = parseDiff("+++ b/file.ts");
    expect(lines[0].type).toBe("context");
  });

  it("identifies deletion lines", () => {
    const lines = parseDiff("-old line");
    expect(lines[0].type).toBe("deletion");
  });

  it("does not treat --- as deletion", () => {
    const lines = parseDiff("--- a/file.ts");
    expect(lines[0].type).toBe("context");
  });

  it("identifies context lines", () => {
    const lines = parseDiff(" unchanged line");
    expect(lines[0].type).toBe("context");
  });

  it("parses a full diff correctly", () => {
    const diff = [
      "diff --git a/file.ts b/file.ts",
      "--- a/file.ts",
      "+++ b/file.ts",
      "@@ -1,3 +1,4 @@",
      " unchanged",
      "-removed",
      "+added",
      "+also added",
    ].join("\n");

    const lines = parseDiff(diff);
    expect(lines[0].type).toBe("header");
    expect(lines[1].type).toBe("context"); // ---
    expect(lines[2].type).toBe("context"); // +++
    expect(lines[3].type).toBe("hunk");
    expect(lines[4].type).toBe("context");
    expect(lines[5].type).toBe("deletion");
    expect(lines[6].type).toBe("addition");
    expect(lines[7].type).toBe("addition");
  });

  it("handles empty diff", () => {
    const lines = parseDiff("");
    expect(lines).toHaveLength(1);
    expect(lines[0].content).toBe("");
  });
});

describe("validateBranchName", () => {
  it("returns null for valid names", () => {
    expect(validateBranchName("main")).toBeNull();
    expect(validateBranchName("feature/test")).toBeNull();
    expect(validateBranchName("wip/my-branch")).toBeNull();
  });

  it("rejects empty names", () => {
    expect(validateBranchName("")).toBeTruthy();
    expect(validateBranchName("  ")).toBeTruthy();
  });

  it("rejects names with spaces", () => {
    expect(validateBranchName("branch name")).toContain("spaces");
  });

  it("rejects names starting with hyphen", () => {
    expect(validateBranchName("-branch")).toContain("hyphen");
  });

  it("rejects names with double dots", () => {
    expect(validateBranchName("branch..name")).toContain("..");
  });

  it("rejects names ending with .lock", () => {
    expect(validateBranchName("branch.lock")).toContain(".lock");
  });

  it("rejects names with invalid characters", () => {
    expect(validateBranchName("branch~1")).toContain("invalid");
    expect(validateBranchName("branch^1")).toContain("invalid");
    expect(validateBranchName("branch:name")).toContain("invalid");
    expect(validateBranchName("branch?")).toContain("invalid");
    expect(validateBranchName("branch*")).toContain("invalid");
    expect(validateBranchName("branch[1]")).toContain("invalid");
    expect(validateBranchName("branch\\name")).toContain("invalid");
  });

  it("rejects invalid slash usage", () => {
    expect(validateBranchName("/branch")).toContain("slash");
    expect(validateBranchName("branch/")).toContain("slash");
    expect(validateBranchName("branch//name")).toContain("slash");
  });

  it("rejects names ending with a period", () => {
    expect(validateBranchName("branch.")).toContain("period");
  });

  it("rejects names with @{", () => {
    expect(validateBranchName("branch@{1}")).toContain("@{");
  });
});

describe("cleanOscTitle", () => {
  it("strips user@host: prefix and extracts basename from path", () => {
    expect(cleanOscTitle("user@myhost:~/projects")).toBe("projects");
  });

  it("strips single env var assignment", () => {
    expect(cleanOscTitle("ANTHROPIC_API_KEY=sk-xxx claude")).toBe("claude");
  });

  it("strips multiple env var assignments", () => {
    expect(cleanOscTitle("FOO=bar BAZ=qux npm test")).toBe("npm test");
  });

  it("strips env vars after user@host: prefix", () => {
    expect(cleanOscTitle("user@host:FOO=bar claude")).toBe("claude");
  });

  it("returns empty for bare user@host (no path or command)", () => {
    expect(cleanOscTitle("stefano.straus@DGQT92CJFP")).toBe("");
  });

  it("strips bare user@host prefix followed by space and command", () => {
    expect(cleanOscTitle("user@host npm start")).toBe("npm start");
  });

  it("extracts basename from path titles", () => {
    expect(cleanOscTitle("~/projects/foo")).toBe("foo");
    expect(cleanOscTitle("/Users/me/projects/bar")).toBe("bar");
    expect(cleanOscTitle("~/Gits/CC_Playground/abrowser")).toBe("abrowser");
  });

  it("returns empty for home directory (preserves original tab name)", () => {
    expect(cleanOscTitle("~")).toBe("");
    expect(cleanOscTitle("~/")).toBe("");
  });

  it("keeps subcommands but strips flags", () => {
    expect(cleanOscTitle("vim file.txt")).toBe("vim file.txt");
    expect(cleanOscTitle("npm test")).toBe("npm test");
    expect(cleanOscTitle("git commit")).toBe("git commit");
  });

  it("strips -- flags from commands", () => {
    expect(cleanOscTitle("claude --dangerously-skip-permissions")).toBe("claude");
    expect(cleanOscTitle("npm test --force")).toBe("npm test");
    expect(cleanOscTitle("git commit -m message")).toBe("git commit");
  });

  it("handles empty string", () => {
    expect(cleanOscTitle("")).toBe("");
  });

  it("handles title that is only env vars (no command)", () => {
    expect(cleanOscTitle("FOO=bar ")).toBe("");
  });

  it("keeps command subcommands before flags", () => {
    expect(cleanOscTitle("echo FOO=bar")).toBe("echo FOO=bar");
  });

  it("handles env var with path value", () => {
    expect(cleanOscTitle("PATH=/usr/bin:/bin node server.js")).toBe("node server.js");
  });

  it("rejects compound commands with semicolons", () => {
    expect(cleanOscTitle('cfg= ; if [ "$cfg" = "yml" ]; then lazygit')).toBe("");
    expect(cleanOscTitle("cd /foo; make")).toBe("");
  });

  it("rejects commands with && or ||", () => {
    expect(cleanOscTitle("test -f file && echo yes")).toBe("");
    expect(cleanOscTitle("cmd1 || cmd2")).toBe("");
  });

  it("rejects shell control flow keywords", () => {
    expect(cleanOscTitle("if test -f foo")).toBe("");
    expect(cleanOscTitle("for f in *.txt")).toBe("");
    expect(cleanOscTitle("while true")).toBe("");
    expect(cleanOscTitle("case $x in")).toBe("");
  });

  it("rejects subshell expressions", () => {
    expect(cleanOscTitle("echo $(whoami)")).toBe("");
  });

  it("does not reject commands containing keyword substrings", () => {
    expect(cleanOscTitle("docker compose up")).toBe("docker compose up");
    expect(cleanOscTitle("ifort build.f90")).toBe("ifort build.f90");
    expect(cleanOscTitle("terraform apply")).toBe("terraform apply");
  });

  it("strips env vars with empty values", () => {
    expect(cleanOscTitle("FOO= bar")).toBe("bar");
  });
});
