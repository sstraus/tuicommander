import { describe, expect, it } from "vitest";
import { type GutterChange, parseDiffToChanges } from "../../components/CodeEditorPanel/gitGutter";

/** Convenience: collect line numbers of a given change type. */
const linesOf = (changes: GutterChange[], type: GutterChange["type"]) =>
	changes
		.filter((c) => c.type === type)
		.map((c) => c.line)
		.sort((a, b) => a - b);

describe("parseDiffToChanges", () => {
	it("returns nothing for an empty diff", () => {
		expect(parseDiffToChanges("")).toEqual([]);
	});

	it("marks pure insertions as added", () => {
		// Two lines inserted after line 2 of the new file.
		const diff = `diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -2,1 +2,3 @@
 context
+new one
+new two`;
		expect(linesOf(parseDiffToChanges(diff), "added")).toEqual([3, 4]);
		expect(linesOf(parseDiffToChanges(diff), "modified")).toEqual([]);
		expect(linesOf(parseDiffToChanges(diff), "deleted")).toEqual([]);
	});

	it("marks replaced lines as modified", () => {
		// One old line replaced by one new line at line 5.
		const diff = `diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -5,1 +5,1 @@
-old text
+new text`;
		expect(linesOf(parseDiffToChanges(diff), "modified")).toEqual([5]);
		expect(linesOf(parseDiffToChanges(diff), "added")).toEqual([]);
		expect(linesOf(parseDiffToChanges(diff), "deleted")).toEqual([]);
	});

	it("treats add-heavy replacements (more new than old) as modified", () => {
		const diff = `@@ -3,1 +3,3 @@
-old
+a
+b
+c`;
		expect(linesOf(parseDiffToChanges(diff), "modified")).toEqual([3, 4, 5]);
	});

	it("marks a pure deletion with a single marker at the following line", () => {
		// Lines removed between context; the line now at position 4 carries the marker.
		const diff = `@@ -3,3 +3,1 @@
 keep
-gone one
-gone two`;
		expect(linesOf(parseDiffToChanges(diff), "deleted")).toEqual([4]);
		expect(linesOf(parseDiffToChanges(diff), "added")).toEqual([]);
		expect(linesOf(parseDiffToChanges(diff), "modified")).toEqual([]);
	});

	it("handles a new (untracked) file as all-added via --no-index output", () => {
		const diff = `diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+line1
+line2
+line3`;
		expect(linesOf(parseDiffToChanges(diff), "added")).toEqual([1, 2, 3]);
	});

	it("classifies multiple hunks independently", () => {
		const diff = `diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,2 +1,2 @@
-first old
+first new
 second
@@ -10,2 +10,3 @@
 ctx
+inserted
 tail`;
		const changes = parseDiffToChanges(diff);
		expect(linesOf(changes, "modified")).toEqual([1]);
		expect(linesOf(changes, "added")).toEqual([11]);
	});

	it("ignores the 'no newline at end of file' marker", () => {
		const diff = `@@ -1,1 +1,1 @@
-a
+b
\\ No newline at end of file`;
		expect(linesOf(parseDiffToChanges(diff), "modified")).toEqual([1]);
	});
});
