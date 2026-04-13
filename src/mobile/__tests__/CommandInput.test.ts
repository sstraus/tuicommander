import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  isSendGuardActive,
  isWithinEchoWindow,
  SEND_GUARD_MS,
  ECHO_WINDOW_MS,
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
  // Mirrors CommandInput.tsx sync semantics: lastWriteAt (500ms echo window
  // for per-char deltas) and lastSendAt (1000ms post-Enter suppression of
  // incoming ptyInputLine so the cleared prompt doesn't reappear).
  class SyncSimulator {
    syncedText = "";
    lastWriteAt = 0;
    lastSendAt = 0;
    writes: string[] = [];

    writePty(data: string) {
      this.lastWriteAt = Date.now();
      this.writes.push(data);
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
      const recentWrite = isWithinEchoWindow(now, this.lastWriteAt);
      if (!recentWrite) {
        this.syncedText = text;
      }
      return recentWrite ? "display-only" : "full-update";
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

    // PTY sends empty during menu redraw (within 500ms of our write)
    const result = sim.receivePtyInput("");
    expect(result).toBe("display-only"); // recent write → don't update syncedText
    expect(sim.syncedText).toBe("/pl"); // NOT reset to ""

    // User selects /wiz:plan — delta computed from /pl, not ""
    sim.syncDelta("/wiz:plan ");
    expect(sim.writes[sim.writes.length - 1]).toBe("\x7f\x7f\x7f/wiz:plan ");
  });

  it("terminal input updates syncedText after echo window", async () => {
    const sim = new SyncSimulator();
    // Force lastWriteAt to be old (and lastSendAt older still so the send
    // guard doesn't swallow the update)
    sim.lastWriteAt = Date.now() - 1000;
    sim.lastSendAt = Date.now() - 2000;

    // Terminal sends "hello" — not from PWA
    const result = sim.receivePtyInput("hello");
    expect(result).toBe("full-update");
    expect(sim.syncedText).toBe("hello");

    // Now if PWA user types, delta is computed from "hello"
    sim.syncDelta("hello world");
    expect(sim.writes[sim.writes.length - 1]).toBe(" world");
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
    // syncedText and must NOT reach the textarea (mirrors CommandInput.tsx:49
    // `if (Date.now() - lastSendAt < 1000) return;`).
    const immediate = sim.receivePtyInput("");
    expect(immediate).toBe("send-guard");
    expect(sim.syncedText).toBe("");

    // Even a ghost "ls" echo within the window is suppressed
    const ghost = sim.receivePtyInput("ls");
    expect(ghost).toBe("send-guard");
    expect(sim.syncedText).toBe("");

    // After the guard window expires, normal sync resumes. Also push
    // lastWriteAt past the 500ms echo window so this is classified as a
    // terminal-driven update, not a PWA echo.
    sim.lastSendAt = Date.now() - 1100;
    sim.lastWriteAt = Date.now() - 600;
    const after = sim.receivePtyInput("new-prompt> ");
    expect(after).toBe("full-update");
    expect(sim.syncedText).toBe("new-prompt> ");
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

  it("echo window is active strictly inside ECHO_WINDOW_MS", () => {
    const t0 = 1_000_000;
    expect(isWithinEchoWindow(t0, t0)).toBe(true);
    expect(isWithinEchoWindow(t0 + ECHO_WINDOW_MS - 1, t0)).toBe(true);
    expect(isWithinEchoWindow(t0 + ECHO_WINDOW_MS, t0)).toBe(false);
  });

  it("CommandInput.tsx wires the real guards (no inline duplication)", () => {
    // Guard against re-introducing inline `Date.now() - lastSendAt < 1000`.
    expect(tsx).toContain("isSendGuardActive");
    expect(tsx).toContain("isWithinEchoWindow");
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
