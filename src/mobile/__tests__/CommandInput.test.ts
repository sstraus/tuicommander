import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

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
  // Simulate the sync flow with a simple state machine
  class SyncSimulator {
    syncedText = "";
    lastWriteAt = 0;
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

    /** Simulate PTY echo arriving */
    receivePtyInput(text: string) {
      if (text === this.syncedText) return "skip"; // echo dedup
      const recentWrite = Date.now() - this.lastWriteAt < 500;
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
    // Force lastWriteAt to be old
    sim.lastWriteAt = Date.now() - 1000;

    // Terminal sends "hello" — not from PWA
    const result = sim.receivePtyInput("hello");
    expect(result).toBe("full-update");
    expect(sim.syncedText).toBe("hello");

    // Now if PWA user types, delta is computed from "hello"
    sim.syncDelta("hello world");
    expect(sim.writes[sim.writes.length - 1]).toBe(" world");
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
