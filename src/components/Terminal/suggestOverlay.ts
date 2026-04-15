/** A snapshot of an xterm buffer row — just the parts the overlay cares about. */
export interface RowSnapshot {
  text: string;
  isWrapped: boolean;
}

/** Re-declared here instead of imported: keeps the helper self-contained and
 *  avoids pulling the full Terminal module into unit tests. Must stay in
 *  sync with the patterns used in Terminal.tsx. */
const SUGGEST_RE = /suggest:\s+.+\|/;
const INTENT_RE = /^intent:\s+\S/;

/**
 * Given a suggest anchor row at `anchorIndex`, return the 0-based indexes of
 * subsequent rows that should be visually hidden as continuations of the
 * same suggest block.
 *
 * Rules (mirrors Rust `parse_suggest`'s bounded joined_tail):
 *  1. xterm-wrapped rows (`isWrapped === true`) are always hidden — they are
 *     the continuation of the same logical line.
 *  2. After the wrap chain ends, at most *one* non-wrapped row is hidden,
 *     and only if it still contains a `|` separator (a pipe-tail the terminal
 *     renderer flushed onto its own logical line).
 *  3. A new `suggest:` or `intent:` anchor stops the walk immediately.
 *
 * Without this bound, the overlay would swallow arbitrary Makefile/table/diff
 * rows that happen to contain pipes (story 1276-a3c2).
 */
export function continuationRowsAfterSuggest(
  anchorIndex: number,
  totalRows: number,
  getRow: (i: number) => RowSnapshot | null,
): number[] {
  const hidden: number[] = [];
  let pipeTailUsed = false;
  for (let i = anchorIndex + 1; i < totalRows; i++) {
    const row = getRow(i);
    if (!row) break;
    if (SUGGEST_RE.test(row.text) || INTENT_RE.test(row.text)) break;
    if (row.isWrapped) {
      hidden.push(i);
      continue;
    }
    if (!pipeTailUsed && row.text.includes("|")) {
      hidden.push(i);
      pipeTailUsed = true;
      continue;
    }
    break;
  }
  return hidden;
}
