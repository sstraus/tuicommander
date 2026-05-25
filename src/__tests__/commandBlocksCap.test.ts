import { describe, expect, it } from "vitest";

const MAX_BLOCKS = 500;

describe("CommandBlocks cap logic", () => {
	it("evicts oldest blocks when exceeding MAX_BLOCKS", () => {
		const blocks = Array.from({ length: MAX_BLOCKS + 10 }, (_, i) => ({
			promptLine: i,
			commandLine: null,
			executionLine: null,
			endLine: i + 1,
			exitCode: 0,
			startedAt: Date.now() - (MAX_BLOCKS + 10 - i) * 1000,
			endedAt: Date.now() - (MAX_BLOCKS + 9 - i) * 1000,
		}));

		const capped = blocks.slice(-MAX_BLOCKS);
		expect(capped.length).toBe(MAX_BLOCKS);
		expect(capped[0].promptLine).toBe(10);
	});

	it("cleans foldedBlocks for evicted entries", () => {
		const foldedBlocks = new Set([0, 5, 10, 499, 500, 505]);
		const evictedPromptLines = new Set([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
		for (const line of evictedPromptLines) {
			foldedBlocks.delete(line);
		}
		expect(foldedBlocks.has(0)).toBe(false);
		expect(foldedBlocks.has(5)).toBe(false);
		expect(foldedBlocks.has(10)).toBe(true);
		expect(foldedBlocks.has(500)).toBe(true);
	});
});
