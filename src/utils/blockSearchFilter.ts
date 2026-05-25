interface SearchMatch {
	row: number;
	col_start: number;
	col_end: number;
}

interface BlockRange {
	promptLine: number;
	endLine: number | null;
}

export function filterMatchesToBlock(
	matches: SearchMatch[],
	blocks: readonly BlockRange[],
	viewportCenter: number,
): SearchMatch[] {
	const block = blocks.find((b) => viewportCenter >= b.promptLine && (b.endLine == null || viewportCenter < b.endLine));
	if (!block) return matches;
	const end = block.endLine;
	if (end == null) {
		return matches.filter((m) => m.row >= block.promptLine);
	}
	return matches.filter((m) => m.row >= block.promptLine && m.row < end);
}
