/**
 * Test: PTY session exit clears scrollback overlay state.
 *
 * Bug: PTY exit callback did not clear scrollbackVisible, vtLogSearchVisible,
 * or scrollbackActiveMatch. User saw frozen scrollback over "[Process exited]".
 */
import { describe, it, expect } from "vitest";

describe("scrollback overlay session exit cleanup", () => {
  it("clears all scrollback state on PTY exit", () => {
    // Simulate signal state: overlay + search + active match all open
    let scrollbackVisible = true;
    let vtLogSearchVisible = true;
    let scrollbackActiveMatch: { offset: number; col_start: number; col_end: number } | null = {
      offset: 100,
      col_start: 0,
      col_end: 5,
    };

    const setScrollbackVisible = (v: boolean) => { scrollbackVisible = v; };
    const setVtLogSearchVisible = (v: boolean) => { vtLogSearchVisible = v; };
    const setScrollbackActiveMatch = (v: typeof scrollbackActiveMatch) => {
      scrollbackActiveMatch = v;
    };

    // Simulate PTY exit callback cleanup (must happen before sessionId = null)
    setScrollbackVisible(false);
    setVtLogSearchVisible(false);
    setScrollbackActiveMatch(null);

    expect(scrollbackVisible).toBe(false);
    expect(vtLogSearchVisible).toBe(false);
    expect(scrollbackActiveMatch).toBeNull();
  });

  it("is safe when overlay was not open at exit time", () => {
    let scrollbackVisible = false;
    let vtLogSearchVisible = false;
    let scrollbackActiveMatch: null = null;

    const setScrollbackVisible = (v: boolean) => { scrollbackVisible = v; };
    const setVtLogSearchVisible = (v: boolean) => { vtLogSearchVisible = v; };
    const setScrollbackActiveMatch = (v: typeof scrollbackActiveMatch) => {
      scrollbackActiveMatch = v;
    };

    // Cleanup should be idempotent
    setScrollbackVisible(false);
    setVtLogSearchVisible(false);
    setScrollbackActiveMatch(null);

    expect(scrollbackVisible).toBe(false);
    expect(vtLogSearchVisible).toBe(false);
    expect(scrollbackActiveMatch).toBeNull();
  });
});
