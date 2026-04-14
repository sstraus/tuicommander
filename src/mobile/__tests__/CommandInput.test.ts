import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  isSendGuardActive,
  SEND_GUARD_MS,
} from "../components/syncGuards";

const css = readFileSync(
  resolve(__dirname, "../components/CommandInput.module.css"),
  "utf-8",
);

const tsx = readFileSync(
  resolve(__dirname, "../components/CommandInput.tsx"),
  "utf-8",
);

/**
 * Extract the sync logic for unit testing. The delta algorithm is pure:
 * given oldText (what PTY has) and newText (what we want), compute the
 * minimal sequence of writes (append, backspace, or clear+retype).
 */
function computeDelta(oldText: string, newText: string): string {
  if (newText.startsWith(oldText)) {
    return newText.slice(oldText.length);
  } else if (oldText.startsWith(newText)) {
    const count = oldText.length - newText.length;
    return "\x7f".repeat(count);
  } else {
    return "\x7f".repeat(oldText.length) + newText;
  }
}

describe("CommandInput delta sync algorithm", () => {
  it("appends characters when new text extends old", () => {
    expect(computeDelta("/", "/wiz")).toBe("wiz");
    expect(computeDelta("/wiz", "/wiz:plan")).toBe(":plan");
    expect(computeDelta("", "/")).toBe("/");
  });

  it("sends backspaces when characters are deleted", () => {
    expect(computeDelta("/wiz", "/wi")).toBe("\x7f");
    expect(computeDelta("/wiz:plan", "/wiz")).toBe("\x7f\x7f\x7f\x7f\x7f");
    expect(computeDelta("/", "")).toBe("\x7f");
  });

  it("deletes all then retypes for complex edits (no common prefix)", () => {
    // /pla → /wiz:changelog (different after /)
    const delta = computeDelta("/pla", "/wiz:changelog");
    // Should be: 4 backspaces (delete "/pla") + "/wiz:changelog"
    expect(delta).toBe("\x7f\x7f\x7f\x7f/wiz:changelog");
  });

  it("handles slash menu selection: /pl → /wiz:plan (space)", () => {
    const delta = computeDelta("/pl", "/wiz:plan ");
    expect(delta).toBe("\x7f\x7f\x7f/wiz:plan ");
  });

  it("handles empty to command", () => {
    const delta = computeDelta("", "/wiz:plan ");
    expect(delta).toBe("/wiz:plan ");
  });

  it("handles same text (no-op)", () => {
    expect(computeDelta("/wiz", "/wiz")).toBe("");
  });
});

describe("CommandInput sync state management", () => {
  // Simulate the sync flow with a simple state machine.
  // Mirrors CommandInput.tsx sync semantics: pendingWrites counter (while > 0
  // incoming ptyInputLine is echo — don't touch syncedText) and lastSendAt
  // (1000ms post-Enter suppression so the cleared prompt doesn't reappear).
  class SyncSimulator {
    syncedText = "";
    pendingWrites = 0;
    lastSendAt = 0;
    writes: string[] = [];

    writePty(data: string) {
      this.pendingWrites++;
      this.writes.push(data);
    }

    /** Simulate write_pty RPC completing (promise resolved). */
    ackWrite() {
      this.pendingWrites--;
    }

    /** Acknowledge all pending writes at once. */
    ackAllWrites() {
      this.pendingWrites = 0;
    }

    syncDelta(newText: string) {
      const oldText = this.syncedText;
      if (newText.startsWith(oldText)) {
        const delta = newText.slice(oldText.length);
        if (delta) this.writePty(delta);
      } else if (oldText.startsWith(newText)) {
        const count = oldText.length - newText.length;
        this.writePty("\x7f".repeat(count));
      } else {
        this.writePty("\x7f".repeat(oldText.length) + newText);
      }
      this.syncedText = newText;
    }

    /** Simulate user pressing Enter — sends \r and arms the 1000ms guard. */
    send() {
      this.writePty("\r");
      this.lastSendAt = Date.now();
      this.syncedText = "";
    }

    /** Simulate PTY echo arriving. Uses the real production guards from
     *  syncGuards.ts so this simulator stays in lockstep with CommandInput.tsx. */
    receivePtyInput(text: string) {
      const now = Date.now();
      if (isSendGuardActive(now, this.lastSendAt)) return "send-guard";
      if (text === this.syncedText) return "skip"; // echo dedup
      if (this.pendingWrites === 0) {
        // No in-flight writes — terminal is driving, accept sync
        this.syncedText = text;
        return "full-update";
      }
      return "display-only";
    }
  }

  it("typing / then selecting /wiz:plan does not produce double slash", () => {
    const sim = new SyncSimulator();

    // User types /
    sim.syncDelta("/");
    expect(sim.syncedText).toBe("/");
    expect(sim.writes).toEqual(["/"]);

    // PTY echoes back / (within 500ms)
    const result = sim.receivePtyInput("/");
    expect(result).toBe("skip"); // exact match — skipped entirely

    // User selects /wiz:plan from menu
    sim.syncDelta("/wiz:plan ");
    expect(sim.syncedText).toBe("/wiz:plan ");
    // Should append "wiz:plan " (not send the full command with double /)
    expect(sim.writes).toEqual(["/", "wiz:plan "]);
  });

  it("typing /pl then selecting /wiz:plan sends correct backspaces", () => {
    const sim = new SyncSimulator();

    sim.syncDelta("/");
    sim.syncDelta("/p");
    sim.syncDelta("/pl");
    expect(sim.syncedText).toBe("/pl");

    // Select /wiz:plan — different prefix, needs backspace+retype
    sim.syncDelta("/wiz:plan ");
    expect(sim.writes[sim.writes.length - 1]).toBe("\x7f\x7f\x7f/wiz:plan ");
  });

  it("PTY redraw during interaction does not corrupt syncedText", () => {
    const sim = new SyncSimulator();

    // User types /pl
    sim.syncDelta("/");
    sim.syncDelta("/p");
    sim.syncDelta("/pl");
    expect(sim.syncedText).toBe("/pl");

    // PTY sends empty during menu redraw — writes still pending
    const result = sim.receivePtyInput("");
    expect(result).toBe("display-only"); // pending writes → don't update syncedText
    expect(sim.syncedText).toBe("/pl"); // NOT reset to ""

    // User selects /wiz:plan — delta computed from /pl, not ""
    sim.syncDelta("/wiz:plan ");
    expect(sim.writes[sim.writes.length - 1]).toBe("\x7f\x7f\x7f/wiz:plan ");
  });

  it("terminal input updates syncedText when no writes are pending", () => {
    const sim = new SyncSimulator();
    sim.lastSendAt = Date.now() - 2000; // send guard expired

    // No pending writes — terminal sends "hello" (e.g. tab completion)
    const result = sim.receivePtyInput("hello");
    expect(result).toBe("full-update");
    expect(sim.syncedText).toBe("hello");

    // Now if PWA user types, delta is computed from "hello"
    sim.syncDelta("hello world");
    expect(sim.writes[sim.writes.length - 1]).toBe(" world");
  });

  it("tab completion accepted after writes settle", () => {
    const sim = new SyncSimulator();

    // User types "gi" then presses Tab
    sim.syncDelta("g");
    sim.syncDelta("gi");
    expect(sim.pendingWrites).toBe(2);

    // Tab sends \t
    sim.syncDelta("gi"); // no change, but tab is separate:
    sim.writePty("\t");
    expect(sim.pendingWrites).toBe(3);

    // PTY echoes back "git " (tab completion) — still pending
    const r1 = sim.receivePtyInput("git ");
    expect(r1).toBe("display-only");
    expect(sim.syncedText).toBe("gi"); // NOT updated

    // All RPCs resolve
    sim.ackAllWrites();
    expect(sim.pendingWrites).toBe(0);

    // PTY sends "git " again (stable) — now accepted
    const r2 = sim.receivePtyInput("git ");
    expect(r2).toBe("full-update");
    expect(sim.syncedText).toBe("git ");

    // User continues typing — delta from "git ", not "gi"
    sim.syncDelta("git status");
    expect(sim.writes[sim.writes.length - 1]).toBe("status");
  });

  it("send() arms a 1000ms guard that blocks ptyInputLine updates", () => {
    const sim = new SyncSimulator();

    // User types "ls" then presses Enter
    sim.syncDelta("l");
    sim.syncDelta("ls");
    sim.send();
    expect(sim.syncedText).toBe(""); // cleared on send
    expect(sim.writes[sim.writes.length - 1]).toBe("\r");

    // Within 1000ms, xterm reports the prompt as empty → must NOT overwrite
    // syncedText and must NOT reach the textarea.
    const immediate = sim.receivePtyInput("");
    expect(immediate).toBe("send-guard");
    expect(sim.syncedText).toBe("");

    // Even a ghost "ls" echo within the window is suppressed
    const ghost = sim.receivePtyInput("ls");
    expect(ghost).toBe("send-guard");
    expect(sim.syncedText).toBe("");

    // After the guard window expires, normal sync resumes.
    sim.lastSendAt = Date.now() - 1100;
    sim.ackAllWrites();
    const after = sim.receivePtyInput("new-prompt> ");
    expect(after).toBe("full-update");
    expect(sim.syncedText).toBe("new-prompt> ");
  });

  it("BUG REPRO: fast typing with network latency does not delete chars", () => {
    // The original bug: user types "abc" fast on mobile. Each keystroke
    // sends a write_pty RPC. With network latency, ptyInputLine echoes
    // arrive late — e.g. "a" arrives when user is already at "abc".
    // Old code: echo window expired → syncedText = "a" → next delta
    // computed from "a" instead of "abc" → duplicate chars sent to PTY.
    const sim = new SyncSimulator();

    // User types a, b, c rapidly
    sim.syncDelta("a");   // write "a" → pendingWrites=1
    sim.syncDelta("ab");  // write "b" → pendingWrites=2
    sim.syncDelta("abc"); // write "c" → pendingWrites=3
    expect(sim.pendingWrites).toBe(3);
    expect(sim.syncedText).toBe("abc");
    expect(sim.writes).toEqual(["a", "b", "c"]);

    // PTY echoes "a" (first RPC response arrives, but echo is stale)
    const r1 = sim.receivePtyInput("a");
    expect(r1).toBe("display-only"); // pendingWrites > 0 → ignored
    expect(sim.syncedText).toBe("abc"); // NOT overwritten to "a"

    // First RPC resolves
    sim.ackWrite(); // pendingWrites=2

    // PTY echoes "ab"
    const r2 = sim.receivePtyInput("ab");
    expect(r2).toBe("display-only"); // still pending
    expect(sim.syncedText).toBe("abc");

    // Second RPC resolves
    sim.ackWrite(); // pendingWrites=1

    // PTY echoes "abc" — matches syncedText exactly, so dedup fires
    const r3 = sim.receivePtyInput("abc");
    expect(r3).toBe("skip"); // exact match — harmless no-op
    expect(sim.syncedText).toBe("abc");

    // Last RPC resolves
    sim.ackWrite(); // pendingWrites=0

    // PTY sends "abc" again (stable state) — now accepted
    const r4 = sim.receivePtyInput("abc");
    expect(r4).toBe("skip"); // exact match → no-op
    expect(sim.syncedText).toBe("abc");

    // User types "d" — delta computed correctly from "abc"
    sim.syncDelta("abcd");
    expect(sim.writes[sim.writes.length - 1]).toBe("d");
  });

  it("late echo after all writes settled does not corrupt state", () => {
    const sim = new SyncSimulator();

    // User types "xy"
    sim.syncDelta("x");
    sim.syncDelta("xy");
    expect(sim.pendingWrites).toBe(2);

    // Both RPCs resolve before echo arrives
    sim.ackAllWrites();
    expect(sim.pendingWrites).toBe(0);

    // Now echo "x" arrives — pendingWrites is 0, so this would be
    // accepted as terminal-driven. This is the edge case: a stale echo
    // arriving after writes settled. syncedText becomes "x" which is
    // wrong — but the display also shows "x", so the user sees the
    // regression and the next keystroke re-syncs.
    const r1 = sim.receivePtyInput("x");
    expect(r1).toBe("full-update"); // accepted (unavoidable without seq numbers)
    expect(sim.syncedText).toBe("x");

    // But the final echo "xy" arrives and corrects it
    const r2 = sim.receivePtyInput("xy");
    expect(r2).toBe("full-update");
    expect(sim.syncedText).toBe("xy");
  });

  it("terminal-driven history navigation updates syncedText when idle", () => {
    const sim = new SyncSimulator();
    sim.lastSendAt = Date.now() - 2000; // send guard expired

    // PWA is idle — no pending writes. User presses Up arrow on
    // physical keyboard or terminal keybar → terminal shows previous cmd.
    // The Up arrow itself is not a syncDelta — it's sent via writePty("\x1b[A").
    // But the RPC resolves quickly, so by the time ptyInputLine arrives,
    // pendingWrites is back to 0.
    sim.writePty("\x1b[A"); // arrow up
    expect(sim.pendingWrites).toBe(1);

    // RPC resolves
    sim.ackWrite();
    expect(sim.pendingWrites).toBe(0);

    // Terminal sends the recalled command
    const r = sim.receivePtyInput("git log --oneline");
    expect(r).toBe("full-update");
    expect(sim.syncedText).toBe("git log --oneline");

    // User edits the recalled command
    sim.syncDelta("git log --oneline -5");
    expect(sim.writes[sim.writes.length - 1]).toBe(" -5");
  });

  it("terminal output while PWA is typing is display-only", () => {
    const sim = new SyncSimulator();

    // User is typing "hello"
    sim.syncDelta("h");
    sim.syncDelta("he");
    // pendingWrites = 2

    // Meanwhile terminal sends something unexpected (e.g. background
    // process appended to prompt, or agent inserted text)
    const r = sim.receivePtyInput("unexpected");
    expect(r).toBe("display-only");
    expect(sim.syncedText).toBe("he"); // PWA state preserved

    // User continues typing — delta correct
    sim.syncDelta("hel");
    expect(sim.writes[sim.writes.length - 1]).toBe("l");
  });
});

describe("syncGuards (production helpers used by CommandInput)", () => {
  it("send guard is active strictly inside the SEND_GUARD_MS window", () => {
    const t0 = 1_000_000;
    expect(isSendGuardActive(t0, t0)).toBe(true);
    expect(isSendGuardActive(t0 + SEND_GUARD_MS - 1, t0)).toBe(true);
    expect(isSendGuardActive(t0 + SEND_GUARD_MS, t0)).toBe(false);
    expect(isSendGuardActive(t0 + SEND_GUARD_MS + 1, t0)).toBe(false);
  });

  it("send guard is closed when no send has occurred (lastSendAt = 0)", () => {
    // Far-future now vs lastSendAt=0 → guard must be closed.
    expect(isSendGuardActive(Date.now(), 0)).toBe(false);
  });

  it("CommandInput.tsx wires the real guards (no inline duplication)", () => {
    // Guard against re-introducing inline `Date.now() - lastSendAt < 1000`.
    expect(tsx).toContain("isSendGuardActive");
    expect(tsx).toContain("pendingWrites");
    expect(tsx).not.toMatch(/Date\.now\(\)\s*-\s*lastSendAt\s*<\s*1000/);
  });
});

describe("CommandInput code structure", () => {
  it("uses live delta sync (syncDelta), not sendCommand for slash", () => {
    expect(tsx).toContain("syncDelta");
  });

  it("agentType prop is declared in CommandInputProps", () => {
    expect(tsx).toContain("agentType?: string | null");
  });

  it("Tab key sends \\t to PTY instead of changing focus", () => {
    expect(tsx).toContain('"Tab"');
    expect(tsx).toContain("\\t");
  });

});

describe("CommandInput iOS auto-zoom prevention", () => {
  it("input font-size is >= 16px to prevent iOS auto-zoom", () => {
    const match = css.match(/\.input\s*\{[^}]*font-size:\s*(\d+)px/s);
    expect(match, "font-size not found in .input rule").toBeTruthy();
    const fontSizePx = parseInt(match![1], 10);
    expect(fontSizePx).toBeGreaterThanOrEqual(16);
  });

  it('input element has inputmode="text"', () => {
    expect(tsx).toContain('inputmode="text"');
  });
});
