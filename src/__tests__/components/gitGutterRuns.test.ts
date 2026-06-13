import { describe, expect, it } from "vitest";
import { coalesceChangeRuns, type GutterChange } from "../../components/CodeEditorPanel/gitGutter";

const added = (line: number): GutterChange => ({ line, type: "added" });
const modified = (line: number): GutterChange => ({ line, type: "modified" });

describe("coalesceChangeRuns", () => {
	it("collapses a whole-new file (all contiguous additions) to a single tick", () => {
		const changes = Array.from({ length: 50 }, (_, i) => added(i + 1));
		expect(coalesceChangeRuns(changes, 50)).toEqual([{ line: 1, type: "added" }]);
	});

	it("starts a new run on a line gap", () => {
		const changes = [added(1), added(2), added(10), added(11)];
		expect(coalesceChangeRuns(changes, 20)).toEqual([
			{ line: 1, type: "added" },
			{ line: 10, type: "added" },
		]);
	});

	it("starts a new run on a type change even when lines are contiguous", () => {
		const changes = [added(5), added(6), modified(7), modified(8)];
		expect(coalesceChangeRuns(changes, 20)).toEqual([
			{ line: 5, type: "added" },
			{ line: 7, type: "modified" },
		]);
	});

	it("sorts unordered input before coalescing", () => {
		const changes = [added(3), added(1), added(2)];
		expect(coalesceChangeRuns(changes, 10)).toEqual([{ line: 1, type: "added" }]);
	});

	it("returns nothing for no changes", () => {
		expect(coalesceChangeRuns([], 10)).toEqual([]);
	});
});
