/** A snapshot of an xterm buffer row — just the parts the overlay cares about. */
export interface RowSnapshot {
  text: string;
  isWrapped: boolean;
}

/** Re-declared here instead of imported: keeps the helper self-contained and
 *  avoids pulling the full Terminal module into unit tests. Must stay in
 *  sync with the patterns used in Terminal.tsx. */
const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
const INTENT_RE = /^intent:\s+\S/;
/** Match a NEW `suggest:` anchor for stop-detection during a continuation
 *  walk. Does NOT require `|` on the same row — the Rust parser allows the
 *  first `|` to arrive on a wrapped continuation line, so a row like
 *  `suggest: long item that wraps...` (with the pipe on the next row) is
 *  still a new block boundary and the walk MUST stop here. (#1380-3b9c) */
const SUGGEST_STOP_RE = /^[\t ]*(?:[●⏺][\t ]+)?suggest:\s+\S/;
/** Status spinner / shell prompt row: a strong terminator for the conceal
 *  walk even when no blank line separates it from the wrapped tail. */
const TERMINATOR_RE = /^[\t ]*[*$#❯>›●⏺]/;

/**
 * Given a suggest anchor row at `anchorIndex`, return the 0-based indexes of
 * subsequent rows that should be visually hidden as continuations of the
 * same suggest block.
 *
 * Rules (mirror Rust `parse_suggest_with_line`'s tail walk):
 *  1. xterm-wrapped rows (`isWrapped === true`) are always hidden.
 *  2. After the wrap chain ends, hide consecutive non-wrapped rows as
 *     pipeless tails when the suggest already has 2+ pipes (3+ items),
 *     until a terminator row (empty / new token / status spinner / prompt).
 *  3. A new `suggest:`, `intent:`, or terminator row stops the walk.
 *
 * Without these bounds, the overlay would swallow arbitrary Makefile/table/diff
 * rows that happen to contain pipes (story 1276-a3c2).
 */
export function continuationRowsAfterSuggest(
  anchorIndex: number,
  totalRows: number,
  getRow: (i: number) => RowSnapshot | null,
): number[] {
  const hidden: number[] = [];
  let pipeTailStarted = false;
  let hadWrapped = false;
  // Count pipes across anchor + hidden rows to decide whether a pipeless tail
  // is safe to consume (mirrors Rust tail-join logic in parse_suggest).
  const anchor = getRow(anchorIndex);
  let pipeCount = anchor ? (anchor.text.match(/\|/g) || []).length : 0;
  for (let i = anchorIndex + 1; i < totalRows; i++) {
    const row = getRow(i);
    if (!row) break;
    if (SUGGEST_STOP_RE.test(row.text) || INTENT_RE.test(row.text)) break;
    if (row.isWrapped) {
      hidden.push(i);
      hadWrapped = true;
      pipeCount += (row.text.match(/\|/g) || []).length;
      continue;
    }
    // Empty row terminates the block (matches Rust parser).
    if (row.text.trim() === "") break;
    // Strong terminators that appear right after the wrap chain.
    if (TERMINATOR_RE.test(row.text)) break;
    if (row.text.includes("|")) {
      // Pipe rows are valid continuations only as the FIRST tail row
      // (post-wrap pipe-flushed continuation). After the tail walk has
      // started, another pipe row is unrelated content (Makefile, etc.)
      // and must terminate (story 1276-a3c2).
      if (pipeTailStarted) break;
      hidden.push(i);
      pipeTailStarted = true;
      continue;
    }
    // Pipeless tail rows: hide them when the suggest is clearly complete
    // (wrapped + 2+ pipes already counted). Multiple consecutive pipeless
    // rows are accepted — long final items can wrap across several lines.
    if (hadWrapped && pipeCount >= 2) {
      hidden.push(i);
      pipeTailStarted = true;
      continue;
    }
    break;
  }
  return hidden;
}

/**
 * Determine whether the row at `anchorIndex` is the start of a suggest block.
 *
 * When the terminal is wide enough, `suggest: A | B | C` fits on one line and
 * the simple `SUGGEST_RE` (requires `|` on the same row) matches.  On narrow
 * terminals the pipe may be pushed onto a wrapped continuation row.  This
 * function handles both cases by checking wrapped rows for a `|` when the
 * anchor contains `suggest:` but no pipe.
 */
export function isSuggestBlock(
  anchorIndex: number,
  totalRows: number,
  getRow: (i: number) => RowSnapshot | null,
): boolean {
  const row = getRow(anchorIndex);
  if (!row) return false;

  // Must at least look like a suggest anchor at column 0
  if (!SUGGEST_ANCHOR_RE.test(row.text)) return false;

  // Fast path: pipe on same line — classic case
  if (row.text.includes("|")) return true;

  // Check wrapped continuation rows for a pipe separator
  for (let i = anchorIndex + 1; i < totalRows; i++) {
    const next = getRow(i);
    if (!next || !next.isWrapped) break;
    if (next.text.includes("|")) return true;
  }

  return false;
}
