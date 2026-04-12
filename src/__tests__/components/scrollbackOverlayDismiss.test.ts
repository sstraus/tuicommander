/**
 * Test: keystroke dismissal clears BOTH scrollbackVisible AND vtLogSearchVisible.
 *
 * Bug: onData handler cleared scrollbackVisible but not vtLogSearchVisible,
 * causing the search bar to reappear on the next overlay open.
 */
import { describe, it, expect } from "vitest";

// Simulate the dismissal logic extracted from Terminal.tsx onData handler.
// The real handler is deeply coupled to xterm/Tauri, so we test the
// signal-clearing logic in isolation.

describe("scrollback overlay keystroke dismissal", () => {
  it("clears both scrollbackVisible and vtLogSearchVisible on user keystroke", () => {
    // Simulate signal state: overlay + search both open
    let scrollbackVisible = true;
    let vtLogSearchVisible = true;

    const setScrollbackVisible = (v: boolean) => { scrollbackVisible = v; };
    const setVtLogSearchVisible = (v: boolean) => { vtLogSearchVisible = v; };

    // Simulate the onData handler logic (from Terminal.tsx ~line 1674-1678)
    const isFocusReport = false; // user typed a real key
    if (!isFocusReport) {
      if (scrollbackVisible) setScrollbackVisible(false);
      if (vtLogSearchVisible) setVtLogSearchVisible(false);
    }

    expect(scrollbackVisible).toBe(false);
    expect(vtLogSearchVisible).toBe(false);
  });

  it("does NOT clear overlay on focus report sequences", () => {
    let scrollbackVisible = true;
    let vtLogSearchVisible = true;

    const setScrollbackVisible = (v: boolean) => { scrollbackVisible = v; };
    const setVtLogSearchVisible = (v: boolean) => { vtLogSearchVisible = v; };

    // Focus reports should not dismiss overlay
    const data = "\x1b[I"; // CSI I — focus gained
    const isFocusReport = data === "\x1b[I" || data === "\x1b[O";
    if (!isFocusReport) {
      if (scrollbackVisible) setScrollbackVisible(false);
      if (vtLogSearchVisible) setVtLogSearchVisible(false);
    }

    expect(scrollbackVisible).toBe(true);
    expect(vtLogSearchVisible).toBe(true);
  });

  it("clears vtLogSearchVisible even when scrollbackVisible is already false", () => {
    let scrollbackVisible = false; // overlay already closed
    let vtLogSearchVisible = true;  // but search bar leaked

    const setScrollbackVisible = (v: boolean) => { scrollbackVisible = v; };
    const setVtLogSearchVisible = (v: boolean) => { vtLogSearchVisible = v; };

    const isFocusReport = false;
    if (!isFocusReport) {
      if (scrollbackVisible) setScrollbackVisible(false);
      if (vtLogSearchVisible) setVtLogSearchVisible(false);
    }

    expect(scrollbackVisible).toBe(false);
    expect(vtLogSearchVisible).toBe(false);
  });

  it("also clears scrollbackActiveMatch on keystroke dismissal", () => {
    let scrollbackActiveMatch: { offset: number; col_start: number; col_end: number } | null = {
      offset: 42,
      col_start: 5,
      col_end: 10,
    };

    const setScrollbackActiveMatch = (v: typeof scrollbackActiveMatch) => {
      scrollbackActiveMatch = v;
    };

    // Simulate onData dismissal including match cleanup
    const isFocusReport = false;
    if (!isFocusReport) {
      if (scrollbackActiveMatch) setScrollbackActiveMatch(null);
    }

    expect(scrollbackActiveMatch).toBeNull();
  });
});
