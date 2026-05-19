import { describe, it, expect } from "vitest";
import { filterMatchesToBlock } from "../utils/blockSearchFilter";

interface Match {
	row: number;
	col_start: number;
	col_end: number;
}

interface Block {
	promptLine: number;
	endLine: number;
}

describe("filterMatchesToBlock", () => {
	const blocks: Block[] = [
		{ promptLine: 0, endLine: 10 },
		{ promptLine: 10, endLine: 25 },
		{ promptLine: 25, endLine: 50 },
	];

	const allMatches: Match[] = [
		{ row: 3, col_start: 0, col_end: 5 },
		{ row: 7, col_start: 2, col_end: 8 },
		{ row: 12, col_start: 0, col_end: 4 },
		{ row: 20, col_start: 1, col_end: 6 },
		{ row: 30, col_start: 0, col_end: 3 },
		{ row: 45, col_start: 5, col_end: 10 },
	];

	it("returns only matches within the block containing viewport center", () => {
		const result = filterMatchesToBlock(allMatches, blocks, 15);
		expect(result).toEqual([
			{ row: 12, col_start: 0, col_end: 4 },
			{ row: 20, col_start: 1, col_end: 6 },
		]);
	});

	it("returns all matches when viewport is outside any block", () => {
		const result = filterMatchesToBlock(allMatches, blocks, 55);
		expect(result).toEqual(allMatches);
	});

	it("handles viewport at block boundary (promptLine)", () => {
		const result = filterMatchesToBlock(allMatches, blocks, 25);
		expect(result).toEqual([
			{ row: 30, col_start: 0, col_end: 3 },
			{ row: 45, col_start: 5, col_end: 10 },
		]);
	});

	it("returns empty when block has no matches", () => {
		const noMatchBlocks: Block[] = [{ promptLine: 100, endLine: 200 }];
		const result = filterMatchesToBlock(allMatches, noMatchBlocks, 150);
		expect(result).toEqual([]);
	});

	it("handles empty blocks array", () => {
		const result = filterMatchesToBlock(allMatches, [], 15);
		expect(result).toEqual(allMatches);
	});
});
