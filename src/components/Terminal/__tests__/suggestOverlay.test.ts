import { describe, it, expect } from "vitest";
import {
  continuationRowsAfterSuggest,
  isSuggestBlock,
  type RowSnapshot,
} from "../suggestOverlay";

/** Build a `getRow` lookup from a compact string/bool list, with null past end. */
function rows(snapshots: Array<[string, boolean]>): (i: number) => RowSnapshot | null {
  return (i) => {
    if (i < 0 || i >= snapshots.length) return null;
    const [text, isWrapped] = snapshots[i];
    return { text, isWrapped };
  };
}

describe("continuationRowsAfterSuggest", () => {
  it("returns [] when the row after suggest is unrelated (no pipe, not wrapped)", () => {
    const get = rows([
      ["suggest: A | B | C", false], // anchor
      ["bash: some unrelated next line", false],
    ]);
    expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
  });

  it("includes all xterm-wrapped continuation rows after the anchor", () => {
    const get = rows([
      ["suggest: very long first item that xterm wraps onto", false], // anchor
      [" the next row as a wrap continuation", true],
      [" and one more wrap row", true],
      ["bash$ ", false],
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1, 2]);
  });

  it("accepts exactly one non-wrapped pipe tail after wrapped continuations", () => {
    const get = rows([
      ["suggest: A | B |", false], // anchor
      [" wrapped part of anchor", true],
      ["C | D | E", false], // one pipe tail — swallow
      ["unrelated next row |", false], // would be swallowed by the old unbounded loop
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1, 2]);
  });

  it("does not consume Makefile pipe lines that follow a suggest-less shell prompt", () => {
    // Here there is no suggest anchor at row 0 — the caller wouldn't invoke
    // this helper. But if it did, anchor=0 pretends row 0 is the suggest.
    // The point: unrelated consecutive pipe rows are not all hidden.
    const get = rows([
      ["suggest: X | Y", false], // pretend anchor
      ["all: clean | build | test", false], // first pipe tail — accept
      ["build: src/*.c | $(DEPS)", false], // second pipe tail — must NOT consume
      ["clean: | rm -rf build", false],
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1]);
  });

  it("stops at a new SUGGEST_RE row even if it contains pipes", () => {
    const get = rows([
      ["suggest: A | B", false], // anchor
      ["suggest: X | Y | Z", false], // new suggest — stop before this
    ]);
    expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
  });

  // #1380-3b9c: when the new suggest's first `|` lands on a wrapped
  // continuation row (narrow terminal, long first item), the prior
  // SUGGEST_RE check would have missed it and continued swallowing rows.
  it("stops at a new suggest anchor whose pipes are on a wrapped row", () => {
    const get = rows([
      ["suggest: A | B | C", false], // anchor
      ["suggest: long first item that the terminal wrapped before", false], // new anchor, no pipe yet
      ["the pipe arrives | 2) second item | 3) third item", true],
    ]);
    expect(continuationRowsAfterSuggest(0, 3, get)).toEqual([]);
  });

  it("stops at a new suggest anchor with bullet prefix and no pipe on first row", () => {
    const get = rows([
      ["suggest: A | B | C", false], // anchor
      ["● suggest: another that wraps the pipe onto the next row", false], // new anchor with bullet
      ["| 2) second", true],
    ]);
    expect(continuationRowsAfterSuggest(0, 3, get)).toEqual([]);
  });

  it("stops at a new INTENT_RE row", () => {
    const get = rows([
      ["suggest: A | B", false], // anchor
      ["intent: doing something new", false], // new intent — stop
    ]);
    expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
  });

  it("handles empty buffer past the anchor", () => {
    const get = rows([["suggest: A | B", false]]);
    expect(continuationRowsAfterSuggest(0, 1, get)).toEqual([]);
  });

  it("stops when getRow returns null (gap in the buffer)", () => {
    const get = (i: number) => {
      if (i === 0) return { text: "suggest: A | B", isWrapped: false };
      return null;
    };
    expect(continuationRowsAfterSuggest(0, 5, get)).toEqual([]);
  });

  it("accepts pipe tail directly after anchor when there are no wrapped rows", () => {
    const get = rows([
      ["suggest: A | B |", false], // anchor (no wrap)
      ["C | D", false], // pipe tail directly after — accept
      ["unrelated | next", false], // must NOT consume
    ]);
    expect(continuationRowsAfterSuggest(0, 3, get)).toEqual([1]);
  });

  it("does not swallow a non-wrapped row without pipes", () => {
    const get = rows([
      ["suggest: A | B", false], // anchor
      ["plain prose here", false],
    ]);
    expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
  });

  it("hides a pipeless tail after a wrap chain when the suggest has 2+ pipes", () => {
    // Bug: a long suggest wraps, and the last fragment lands on a non-wrapped
    // row without a pipe. Without the fix, this tail row leaks through.
    const get = rows([
      ["suggest: 1) Do something | 2) Another thing |", false], // anchor (2 pipes)
      [" 3) Yet another thing that is also very long", true],   // wrapped
      ["keeps going", false],                                    // pipeless tail — should be hidden
      ["$ ", false],                                             // shell prompt — must NOT be hidden
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1, 2]);
  });

  it("does not hide a pipeless tail when the suggest has only 1 pipe (2 items)", () => {
    // Short 2-item suggests should NOT grab unrelated prose that follows.
    const get = rows([
      ["suggest: A | B", false],    // anchor (1 pipe)
      ["unrelated text", false],     // not a tail — must NOT be hidden
    ]);
    expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
  });

  it("hides pipeless tail after wrapped rows end", () => {
    // Suggest wraps across 3 xterm rows; the tail of the 3rd item
    // starts a new non-wrapped row without any pipe character.
    const get = rows([
      ["suggest: 1) First option here | 2) Second option he", false], // anchor
      ["re | 3) Third option that is extremely long and wrap", true],  // wrapped
      ["s onto yet another row", true],                                // wrapped
      ["and finishes here", false],                                    // pipeless tail
      ["$ ", false],                                                   // prompt
    ]);
    expect(continuationRowsAfterSuggest(0, 5, get)).toEqual([1, 2, 3]);
  });

  it("hides multiple consecutive pipeless tail rows after wrap chain", () => {
    // Reported bug: long final item wraps across multiple physical rows.
    // The single-tail-only rule left trailing words ("app") visible.
    const get = rows([
      ["suggest: 1) A | 2) B | 3) Questo cambia completamente la d", false],
      ["iagnosi del fix.", true],                                      // wrapped
      ["app", false],                                                  // pipeless tail #2
      ["* Cooked for 3m 30s", false],                                  // status — terminator
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1, 2]);
  });

  it("stops pipeless tail walk at a status spinner row", () => {
    const get = rows([
      ["suggest: 1) A | 2) B | 3) C with long tail", false],
      ["wrapped tail", true],
      ["* Cooking...", false], // status spinner — terminator
      ["app", false],          // must NOT be hidden (after terminator)
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1]);
  });

  it("stops tail walk at a pipe row that arrives after a pipeless tail", () => {
    // After a pipeless tail has been consumed, a later pipe-bearing row is
    // unrelated content (Makefile, table, etc.) and must terminate the walk,
    // even when the prior tail was valid. Guards the `if (pipeTailStarted)
    // break` branch in the pipe-row handler.
    const get = rows([
      ["suggest: 1) A | 2) B | 3) C with wrap", false], // anchor (2 pipes)
      ["wrapped tail", true],                            // wrapped row
      ["pipeless tail", false],                          // pipeless tail #1 — accept
      ["all: clean | build | test", false],              // pipe row after tail — must NOT be hidden
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1, 2]);
  });

  it("stops pipeless tail walk at an empty row", () => {
    const get = rows([
      ["suggest: 1) A | 2) B | 3) C wraps", false],
      ["onto next", true],
      ["", false],   // empty terminator
      ["leftover", false], // must NOT be hidden
    ]);
    expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1]);
  });
});

describe("isSuggestBlock", () => {
  it("returns true for normal suggest with pipe on same line", () => {
    const get = rows([
      ["suggest: A | B | C", false],
    ]);
    expect(isSuggestBlock(0, 1, get)).toBe(true);
  });

  it("returns true when pipe is on a wrapped continuation row", () => {
    const get = rows([
      ["suggest: 1) Testa il popup con Shift+Cmd+I su un upstream r", false],
      ["eale | 2) Continua con la story 1324-0319 (clippy cleanup) ", true],
      ["| 3) Crea una PR per questi cambiamenti", true],
    ]);
    expect(isSuggestBlock(0, 3, get)).toBe(true);
  });

  it("returns false for prose starting with suggest: but no pipe anywhere", () => {
    const get = rows([
      ["suggest: we should refactor the codebase", false],
      ["to improve performance and readability", true],
    ]);
    expect(isSuggestBlock(0, 2, get)).toBe(false);
  });

  it("returns false for a row that does not start with suggest:", () => {
    const get = rows([
      ["I suggest: we try something else | maybe", false],
    ]);
    expect(isSuggestBlock(0, 1, get)).toBe(false);
  });

  it("returns true with Ink bullet prefix", () => {
    const get = rows([
      ["● suggest: Run tests | Check logs", false],
    ]);
    expect(isSuggestBlock(0, 1, get)).toBe(true);
  });

  it("returns false when row is not the anchor index", () => {
    const get = rows([
      ["unrelated row", false],
      ["suggest: A | B", false],
    ]);
    // row 0 is not a suggest anchor
    expect(isSuggestBlock(0, 2, get)).toBe(false);
  });
});
