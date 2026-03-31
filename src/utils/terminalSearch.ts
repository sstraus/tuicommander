import type { TerminalMatch } from "../types";

/**
 * Search an array of terminal buffer lines for a query string (case-insensitive).
 * Returns the first match per line to avoid flooding results.
 */
export function searchTerminalBuffer(
  lines: string[],
  query: string,
  terminalId: string,
  terminalName: string,
): TerminalMatch[] {
  if (!query) return [];
  const lowerQuery = query.toLowerCase();
  const results: TerminalMatch[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const idx = line.toLowerCase().indexOf(lowerQuery);
    if (idx !== -1) {
      results.push({
        terminalId,
        terminalName,
        lineIndex: i,
        lineText: line,
        matchStart: idx,
        matchEnd: idx + query.length,
      });
    }
  }

  return results;
}
