import { describe, it, expect } from "vitest";
import { buildPartialPatch, extractHunks } from "../../components/DiffTab/diffPatch";

const SAMPLE_DIFF = `diff --git a/file.txt b/file.txt
index abc1234..def5678 100644
--- a/file.txt
+++ b/file.txt
@@ -1,5 +1,6 @@
 line 1
-old line 2
+new line 2a
+new line 2b
 line 3
 line 4
 line 5`;

const NEW_FILE_DIFF = `diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3`;

const MULTI_HUNK_DIFF = `diff --git a/file.txt b/file.txt
index abc1234..def5678 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 line 1
+inserted
 line 2
 line 3
@@ -10,3 +11,4 @@
 line 10
-removed
+replaced
 line 12`;

describe("extractHunks", () => {
  it("extracts hunks from a single-hunk diff", () => {
    const hunks = extractHunks(SAMPLE_DIFF);
    expect(hunks).toHaveLength(1);
    expect(hunks[0]).toContain("@@ -1,5 +1,6 @@");
    expect(hunks[0]).toContain("diff --git");
  });

  it("extracts hunks from a multi-hunk diff", () => {
    const hunks = extractHunks(MULTI_HUNK_DIFF);
    expect(hunks).toHaveLength(2);
    expect(hunks[0]).toContain("@@ -1,3 +1,4 @@");
    expect(hunks[1]).toContain("@@ -10,3 +11,4 @@");
  });
});

describe("buildPartialPatch", () => {
  it("returns full hunk when all change lines are selected", () => {
    // Select all changed lines: -old line 2 (idx 1), +new line 2a (idx 2), +new line 2b (idx 3)
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([1, 2, 3]));
    expect(patch).toContain("diff --git");
    expect(patch).toContain("-old line 2");
    expect(patch).toContain("+new line 2a");
    expect(patch).toContain("+new line 2b");
  });

  it("selects only one addition — others become context", () => {
    // Select only +new line 2b (idx 3)
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([3]));
    // The selected addition stays as +
    expect(patch).toContain("+new line 2b");
    // Unselected addition becomes context (no + prefix)
    expect(patch).toMatch(/^ new line 2a$/m);
    // Unselected deletion is dropped entirely
    expect(patch).not.toContain("-old line 2");
    expect(patch).not.toContain("old line 2");
  });

  it("selects only one deletion — unselected additions become context", () => {
    // Select only -old line 2 (idx 1)
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([1]));
    expect(patch).toContain("-old line 2");
    // Unselected additions become context
    expect(patch).toMatch(/^ new line 2a$/m);
    expect(patch).toMatch(/^ new line 2b$/m);
  });

  it("recalculates @@ header for partial selection", () => {
    // Select only +new line 2b (idx 3)
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set([3]));
    // old side: context(4) = line 1, new line 2a (context), line 3, line 4, line 5
    // new side: context(4) + 1 selected addition = line 1, new line 2a, +new line 2b, line 3, line 4, line 5
    // But we need to figure out the actual correct counts
    const headerMatch = patch.match(/@@ -(\d+),(\d+) \+(\d+),(\d+) @@/);
    expect(headerMatch).toBeTruthy();
    const [, , oldCount, , newCount] = headerMatch!;
    // new side should have 1 more line than old side (the selected addition)
    expect(parseInt(newCount)).toBe(parseInt(oldCount) + 1);
  });

  it("works with a specific hunk in multi-hunk diff", () => {
    // Select +replaced (change line idx 2 in hunk 1)
    const patch = buildPartialPatch(MULTI_HUNK_DIFF, 1, new Set([2]));
    expect(patch).toContain("+replaced");
    expect(patch).toContain("diff --git");
    // Should not contain hunk 0 content
    expect(patch).not.toContain("+inserted");
  });

  it("returns empty string when no lines selected", () => {
    const patch = buildPartialPatch(SAMPLE_DIFF, 0, new Set());
    expect(patch).toBe("");
  });

  it("handles new file diffs (all additions)", () => {
    // Select only line 2 (idx 1)
    const patch = buildPartialPatch(NEW_FILE_DIFF, 0, new Set([1]));
    expect(patch).toContain("+line 2");
    // Other lines become context
    expect(patch).toMatch(/^ line 1$/m);
    expect(patch).toMatch(/^ line 3$/m);
  });
});
