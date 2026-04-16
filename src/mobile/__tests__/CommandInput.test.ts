import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  isPostSendGuardActive,
  isSupersetEcho,
  POST_SEND_GUARD_MS,
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
    const delta = computeDelta("/pla", "/wiz:changelog");
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

/**
 * PTY echo contract: the PWA textarea is the source of truth.
 * The sync effect accepts a PTY echo ONLY when it is a strict extension
 * of what we've already sent — i.e. tab completion. This simulator
 * mirrors the production effect in CommandInput.tsx so behavior stays
 * verifiable without a DOM.
 */
class InputSimulator {
  /** Textarea-visible value. */
  displayed = "";
  /** Last text the PWA sent to the PTY. Only PTY echoes that strictly
   *  extend this are accepted back into `displayed`. */
  syncedText = "";
  /** Timestamp of the last send() — used by the post-send guard to ignore
   *  lagging echoes of the just-sent command. */
  lastSendAt = 0;
  /** Virtual clock (ms). Defaults to Date.now() but tests can fast-forward
   *  it to simulate real elapsed time without setTimeout. */
  now = Date.now();
  /** Raw writes sent to the PTY (for assertions on delta correctness). */
  writes: string[] = [];

  /** User typing in the textarea: updates display + streams delta to PTY. */
  type(newText: string) {
    const oldText = this.syncedText;
    if (newText.startsWith(oldText)) {
      const delta = newText.slice(oldText.length);
      if (delta) this.writes.push(delta);
    } else if (oldText.startsWith(newText)) {
      this.writes.push("\x7f".repeat(oldText.length - newText.length));
    } else {
      this.writes.push("\x7f".repeat(oldText.length) + newText);
    }
    this.syncedText = newText;
    this.displayed = newText;
  }

  /** PTY echoes `text` as the current input-line value. Mirrors the
   *  production effect: post-send guard first, then strict-extension rule. */
  receivePtyInput(text: string): "accepted" | "ignored" {
    if (isPostSendGuardActive(this.now, this.lastSendAt)) return "ignored";
    if (!isSupersetEcho(text, this.syncedText)) return "ignored";
    this.syncedText = text;
    this.displayed = text;
    return "accepted";
  }

  /** User presses Enter: textarea clears, syncedText resets, guard armed. */
  pressEnter() {
    this.writes.push("\r");
    this.syncedText = "";
    this.displayed = "";
    this.lastSendAt = this.now;
  }

  /** Advance the virtual clock by `ms`. */
  advance(ms: number) {
    this.now += ms;
  }
}

describe("CommandInput echo handling (PWA textarea is source of truth)", () => {
  it("BUG REPRO: fast typing with slow echo does not delete chars on screen", () => {
    // The reported bug: on high-latency PWA, user types "hello" quickly.
    // The PTY echoes "h" back while the user is already at "hell" — old
    // code would overwrite the textarea with "h", deleting "ell" from
    // what the user sees. New rule: a shorter/equal echo is always ignored.
    const sim = new InputSimulator();

    sim.type("h");
    sim.type("he");
    sim.type("hel");
    sim.type("hell");
    expect(sim.displayed).toBe("hell");

    // Lagging echoes arrive — all shorter than what's displayed
    expect(sim.receivePtyInput("h")).toBe("ignored");
    expect(sim.displayed).toBe("hell");
    expect(sim.receivePtyInput("he")).toBe("ignored");
    expect(sim.displayed).toBe("hell");
    expect(sim.receivePtyInput("hel")).toBe("ignored");
    expect(sim.displayed).toBe("hell");

    // Echo catches up to current text — still ignored (equal, not strict)
    expect(sim.receivePtyInput("hell")).toBe("ignored");
    expect(sim.displayed).toBe("hell");

    // User continues typing, delta is computed from "hell" correctly
    sim.type("hello");
    expect(sim.writes[sim.writes.length - 1]).toBe("o");
  });

  it("tab completion: PTY-driven strict extension is accepted", () => {
    const sim = new InputSimulator();

    sim.type("g");
    sim.type("gi");
    expect(sim.syncedText).toBe("gi");

    // User presses Tab — PTY expands "gi" → "git "
    expect(sim.receivePtyInput("git ")).toBe("accepted");
    expect(sim.displayed).toBe("git ");
    expect(sim.syncedText).toBe("git ");

    // Continued typing deltas from the expanded value
    sim.type("git status");
    expect(sim.writes[sim.writes.length - 1]).toBe("status");
  });

  it("history nav from empty textarea: PTY insert is accepted (empty is prefix of all)", () => {
    const sim = new InputSimulator();

    // Textarea empty, user presses Up on external keybar
    expect(sim.receivePtyInput("git log --oneline")).toBe("accepted");
    expect(sim.displayed).toBe("git log --oneline");
  });

  it("history nav while typing: replacement is IGNORED (textarea wins)", () => {
    const sim = new InputSimulator();

    // User typed "xy", then triggers history recall
    sim.type("xy");
    expect(sim.displayed).toBe("xy");

    // PTY replaces input line with a historical command — NOT an extension
    expect(sim.receivePtyInput("ls -la")).toBe("ignored");
    expect(sim.displayed).toBe("xy");
  });

  it("post-send guard: ghost echo of just-sent command is suppressed", () => {
    const sim = new InputSimulator();

    sim.type("ls");
    sim.pressEnter();
    expect(sim.displayed).toBe("");
    expect(sim.syncedText).toBe("");

    // xterm reports prompt as empty again — ignored regardless of guard
    expect(sim.receivePtyInput("")).toBe("ignored");
    expect(sim.displayed).toBe("");

    // Ghost "ls" echo arrives while guard is still active — suppressed
    expect(sim.receivePtyInput("ls")).toBe("ignored");
    expect(sim.displayed).toBe("");
    expect(sim.syncedText).toBe("");

    // After the guard window expires, new prompt content can be accepted
    sim.advance(POST_SEND_GUARD_MS + 1);
    expect(sim.receivePtyInput("new-prompt> ")).toBe("accepted");
    expect(sim.displayed).toBe("new-prompt> ");
  });

  it("post-send guard: typing during guard still works (user wins)", () => {
    // Guard only blocks PTY echoes — local typing is always authoritative.
    const sim = new InputSimulator();
    sim.type("ls");
    sim.pressEnter();

    // Guard active. User starts typing next command immediately.
    sim.type("ec");
    sim.type("echo");
    expect(sim.displayed).toBe("echo");
    expect(sim.writes.slice(-2)).toEqual(["ec", "ho"]);
  });

  it("exact echo is ignored (equal is not strict)", () => {
    const sim = new InputSimulator();
    sim.type("abc");
    expect(sim.receivePtyInput("abc")).toBe("ignored");
    expect(sim.displayed).toBe("abc");
  });

  it("typing over unrelated PTY output is protected", () => {
    const sim = new InputSimulator();

    sim.type("hello");
    expect(sim.displayed).toBe("hello");

    // Background output / prompt noise that happens to produce some
    // ptyInputLine value unrelated to what we typed
    expect(sim.receivePtyInput("unexpected")).toBe("ignored");
    expect(sim.displayed).toBe("hello");

    sim.type("hello world");
    expect(sim.writes[sim.writes.length - 1]).toBe(" world");
  });
});

describe("CommandInput code structure", () => {
  it("uses isSupersetEcho as the single acceptance rule", () => {
    expect(tsx).toContain("isSupersetEcho");
  });

  it("no longer depends on the old grace/classifyEcho helpers", () => {
    expect(tsx).not.toContain("isSendGuardActive");
    expect(tsx).not.toContain("classifyEcho");
    expect(tsx).not.toContain("lastWriteSettledAt");
    expect(tsx).not.toContain("pendingWrites");
  });

  it("arms a post-send guard inside send() and honours it in the sync effect", () => {
    expect(tsx).toContain("isPostSendGuardActive");
    expect(tsx).toContain("lastSendAt");
    // Guard against re-introducing the old inline `Date.now() - lastSendAt < 1000`.
    expect(tsx).not.toMatch(/Date\.now\(\)\s*-\s*lastSendAt\s*<\s*1000/);
  });

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

describe("isPostSendGuardActive", () => {
  it("is active strictly inside the POST_SEND_GUARD_MS window", () => {
    const t0 = 1_000_000;
    expect(isPostSendGuardActive(t0, t0)).toBe(true);
    expect(isPostSendGuardActive(t0 + POST_SEND_GUARD_MS - 1, t0)).toBe(true);
    expect(isPostSendGuardActive(t0 + POST_SEND_GUARD_MS, t0)).toBe(false);
    expect(isPostSendGuardActive(t0 + POST_SEND_GUARD_MS + 1, t0)).toBe(false);
  });

  it("is closed when no send has occurred (lastSendAt = 0)", () => {
    // A 0 sentinel must NOT open the guard — otherwise the very first
    // PTY update after mount would be suppressed for 500ms.
    expect(isPostSendGuardActive(Date.now(), 0)).toBe(false);
  });
});

describe("isSupersetEcho", () => {
  it("accepts strict supersets", () => {
    expect(isSupersetEcho("git ", "gi")).toBe(true);
    expect(isSupersetEcho("abcd", "abc")).toBe(true);
    expect(isSupersetEcho("hello", "")).toBe(true);
  });

  it("rejects equal, shorter, and unrelated strings", () => {
    expect(isSupersetEcho("abc", "abc")).toBe(false); // equal
    expect(isSupersetEcho("ab", "abc")).toBe(false); // shorter
    expect(isSupersetEcho("", "abc")).toBe(false); // shorter (empty)
    expect(isSupersetEcho("xyz", "abc")).toBe(false); // unrelated
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
