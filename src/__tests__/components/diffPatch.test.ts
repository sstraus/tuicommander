import { describe, it, expect } from "vitest";
import { extractHunks, extractSelectedLines, buildPartialPatch } from "../../components/DiffTab/diffPatch";

const SAMPLE_DIFF = [
  "diff --git a/src/main.ts b/src/main.ts",
  "index abc1234..def5678 100644",
  "--- a/src/main.ts",
  "+++ b/src/main.ts",
  "@@ -1,5 +1,6 @@",
  " import { foo } from './foo';",
  "-import { bar } from './bar';",
  "+import { bar } from './baz';",
  "+import { qux } from './qux';",
  " ",
  " function main() {",
  "@@ -10,3 +11,4 @@",
  "   return 42;",
  "-  console.log('done');",
  "+  console.log('finished');",
  "+  process.exit(0);",
].join("\n");

describe("extractHunks", () => {
  it("splits a diff with two hunks", () => {
    const hunks = extractHunks(SAMPLE_DIFF);
    expect(hunks).toHaveLength(2);
  });

  it("each hunk includes the file header", () => {
    const hunks = extractHunks(SAMPLE_DIFF);
    for (const h of hunks) {
      expect(h).toContain("diff --git");
      expect(h).toContain("--- a/src/main.ts");
      expect(h).toContain("+++ b/src/main.ts");
    }
  });

  it("first hunk contains the first @@ block body", () => {
    const hunks = extractHunks(SAMPLE_DIFF);
    expect(hunks[0]).toContain("import { foo }");
    expect(hunks[0]).toContain("+import { qux }");
    expect(hunks[0]).not.toContain("return 42");
  });

  it("second hunk contains the second @@ block body", () => {
    const hunks = extractHunks(SAMPLE_DIFF);
    expect(hunks[1]).toContain("return 42");
    expect(hunks[1]).toContain("+  process.exit(0)");
    expect(hunks[1]).not.toContain("import { foo }");
  });

  it("returns empty for a diff with no @@ markers", () => {
    const noHunks = "diff --git a/f b/f\n--- a/f\n+++ b/f\n";
    expect(extractHunks(noHunks)).toHaveLength(0);
  });

  it("handles a single hunk", () => {
    const single = [
      "diff --git a/f b/f",
      "--- a/f",
      "+++ b/f",
      "@@ -1,2 +1,2 @@",
      "-old",
      "+new",
    ].join("\n");
    const hunks = extractHunks(single);
    expect(hunks).toHaveLength(1);
    expect(hunks[0]).toContain("-old");
    expect(hunks[0]).toContain("+new");
  });
});

describe("extractSelectedLines", () => {
  it("returns empty when no lines selected", () => {
    const result = extractSelectedLines(SAMPLE_DIFF, 0, new Set());
    expect(result.lines).toHaveLength(0);
    expect(result.startLine).toBe(0);
    expect(result.endLine).toBe(0);
  });

  it("returns empty for out-of-range hunk index", () => {
    const result = extractSelectedLines(SAMPLE_DIFF, 99, new Set([0]));
    expect(result.lines).toHaveLength(0);
  });

  it("extracts an added line with correct new-file line number", () => {
    // Hunk 0 body: line 0=" import foo", line 1="-import bar", line 2="+import baz", line 3="+import qux", line 4=" ", line 5=" function main"
    const result = extractSelectedLines(SAMPLE_DIFF, 0, new Set([2]));
    expect(result.lines).toHaveLength(1);
    expect(result.lines[0].type).toBe("+");
    expect(result.lines[0].content).toBe("import { bar } from './baz';");
    expect(result.lines[0].lineNumber).toBe(2);
  });

  it("extracts a deleted line with correct old-file line number", () => {
    // line 1 = "-import { bar } from './bar';" → old line 2
    const result = extractSelectedLines(SAMPLE_DIFF, 0, new Set([1]));
    expect(result.lines).toHaveLength(1);
    expect(result.lines[0].type).toBe("-");
    expect(result.lines[0].content).toBe("import { bar } from './bar';");
    expect(result.lines[0].lineNumber).toBe(2);
  });

  it("extracts a context line", () => {
    // line 0 = " import { foo } from './foo';" → new line 1
    const result = extractSelectedLines(SAMPLE_DIFF, 0, new Set([0]));
    expect(result.lines).toHaveLength(1);
    expect(result.lines[0].type).toBe(" ");
    expect(result.lines[0].lineNumber).toBe(1);
  });

  it("computes startLine and endLine across multiple selections", () => {
    // Select lines 0 (context, newLine=1) and 3 (addition, newLine=3)
    const result = extractSelectedLines(SAMPLE_DIFF, 0, new Set([0, 3]));
    expect(result.lines).toHaveLength(2);
    expect(result.startLine).toBe(1);
    expect(result.endLine).toBe(3);
  });

  it("works on second hunk", () => {
    // Hunk 1: @@ -10,3 +11,4 @@
    // line 0 = "  return 42;" (context, old=10, new=11)
    // line 1 = "-  console.log('done');" (del, old=11)
    // line 2 = "+  console.log('finished');" (add, new=12)
    // line 3 = "+  process.exit(0);" (add, new=13)
    const result = extractSelectedLines(SAMPLE_DIFF, 1, new Set([2]));
    expect(result.lines).toHaveLength(1);
    expect(result.lines[0].type).toBe("+");
    expect(result.lines[0].content).toBe("  console.log('finished');");
    expect(result.lines[0].lineNumber).toBe(12);
  });

  it("returns startLine=0 endLine=0 when all selected indices are out of body range", () => {
    const result = extractSelectedLines(SAMPLE_DIFF, 0, new Set([999]));
    expect(result.lines).toHaveLength(0);
    expect(result.startLine).toBe(0);
    expect(result.endLine).toBe(0);
  });
});

describe("buildPartialPatch", () => {
  it("returns empty for no selections", () => {
    expect(buildPartialPatch(SAMPLE_DIFF, 0, new Set())).toBe("");
  });

  it("returns empty for out-of-range hunk", () => {
    expect(buildPartialPatch(SAMPLE_DIFF, 99, new Set([0]))).toBe("");
  });

  it("builds a patch with only the selected addition", () => {
    // Select line 2 (+import baz) — line 1 (-import bar) is not selected so dropped,
    // line 3 (+import qux) is unselected addition → converted to context
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([2]));
    expect(patch).toContain("+import { bar } from './baz';");
    expect(patch).not.toContain("-import { bar } from './bar';");
    // Unselected addition becomes context
    expect(patch).toContain(" import { qux } from './qux';");
  });

  it("builds a patch with only the selected deletion", () => {
    // Select line 1 (-import bar) — the deletion to reverse
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([1]));
    expect(patch).toContain("-import { bar } from './bar';");
  });

  it("returns empty when only context lines are selected (no actual changes)", () => {
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([0, 4, 5]));
    expect(patch).toBe("");
  });

  it("includes correct @@ header with adjusted counts", () => {
    // Select just line 2 (+import baz)
    // Context lines: line 0 (context), line 3 (unselected add→context), line 4, line 5
    // Old count = context lines only (no selected deletions)
    // New count = context lines + 1 selected addition
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([2]));
    expect(patch).toMatch(/@@ -\d+,\d+ \+\d+,\d+ @@/);
  });

  it("preserves the file header", () => {
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([2]));
    expect(patch).toContain("diff --git a/src/main.ts b/src/main.ts");
    expect(patch).toContain("--- a/src/main.ts");
    expect(patch).toContain("+++ b/src/main.ts");
  });
});
