import { SearchQuery } from "@codemirror/search";
import { Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { collectMatchLines } from "../../components/CodeEditorPanel/searchOverview";

function docOf(...lines: string[]): Text {
	return Text.of(lines);
}

describe("collectMatchLines", () => {
	it("returns one line per match, deduped across multiple matches on a line", () => {
		const doc = docOf("foo bar foo", "baz", "foo end");
		const lines = collectMatchLines(new SearchQuery({ search: "foo" }), doc);
		expect([...lines].sort((a, b) => a - b)).toEqual([1, 3]);
	});

	it("ignores case-sensitivity per the query flag", () => {
		const doc = docOf("Foo", "foo", "FOO");
		expect(collectMatchLines(new SearchQuery({ search: "foo" }), doc).size).toBe(3);
		expect(collectMatchLines(new SearchQuery({ search: "foo", caseSensitive: true }), doc)).toEqual(
			new Set([2]),
		);
	});

	it("supports regexp queries", () => {
		const doc = docOf("line 1", "line 22", "nope", "line 333");
		const lines = collectMatchLines(new SearchQuery({ search: "line \\d+", regexp: true }), doc);
		expect([...lines].sort((a, b) => a - b)).toEqual([1, 2, 4]);
	});

	it("returns an empty set for an invalid (empty) query", () => {
		const doc = docOf("anything");
		expect(collectMatchLines(new SearchQuery({ search: "" }), doc).size).toBe(0);
	});

	it("honors the iteration cap without exceeding the line count", () => {
		const doc = docOf("a a a", "a a a", "a a a");
		// Cap below the match count: collection stops early, never more lines than exist.
		const lines = collectMatchLines(new SearchQuery({ search: "a" }), doc, 2);
		expect(lines.size).toBeLessThanOrEqual(doc.lines);
		expect(lines.size).toBeGreaterThan(0);
	});
});
