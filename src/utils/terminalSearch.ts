import type { TerminalMatch } from "../types";
import { buildIndex } from "./bm25";

/**
 * Search an array of terminal buffer lines for a query string.
 *
 * Lines are filtered by case-insensitive substring match (required so we can
 * compute accurate `matchStart`/`matchEnd` offsets for the highlight UI), then
 * reranked with BM25 so the most relevant hit floats to the top instead of
 * whichever line happens to be earliest in the scrollback. Works directly on
 * xterm's authoritative buffer — deliberately not moved to Rust because the
 * caller navigates via `scrollToLine(lineIndex)`, which is an xterm coordinate
 * and diverges from any server-side byte/line buffer.
 */
export function searchTerminalBuffer(
  lines: string[],
  query: string,
  terminalId: string,
  terminalName: string,
): TerminalMatch[] {
  if (!query) return [];
  const lowerQuery = query.toLowerCase();

  const hits: TerminalMatch[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const idx = line.toLowerCase().indexOf(lowerQuery);
    if (idx !== -1) {
      hits.push({
        terminalId,
        terminalName,
        lineIndex: i,
        lineText: line,
        matchStart: idx,
        matchEnd: idx + query.length,
      });
    }
  }

  if (hits.length <= 1) return hits;

  // Rerank by BM25 relevance; fall back to scrollback order for equal scores.
  const { score } = buildIndex(hits.map((h) => ({ item: h, text: h.lineText })));
  const ranked = score(query);
  if (ranked.length === 0) return hits;

  const seen = new Set<TerminalMatch>();
  const reordered: TerminalMatch[] = [];
  for (const r of ranked) {
    reordered.push(r.item);
    seen.add(r.item);
  }
  for (const h of hits) {
    if (!seen.has(h)) reordered.push(h);
  }
  return reordered;
}
