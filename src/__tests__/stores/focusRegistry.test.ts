import { beforeEach, describe, expect, it } from "vitest";
import {
  clearForRepo,
  getForRepo,
  getLastGlobal,
  isRepoScoped,
  recordFocus,
  recordTerminalRepo,
} from "../../stores/focusRegistry";

describe("focusRegistry", () => {
  beforeEach(() => {
    // Module-scoped singleton — reset by clearing each repo key and overwriting global.
    // Cheapest safe reset: overwrite lastGlobal to a benign value, then wipe repo entries
    // exercised by previous tests via clearForRepo().
    recordFocus({ kind: "ai-chat" });
    clearForRepo("/repo/a");
    clearForRepo("/repo/b");
  });

  it("classifies repo-scoped vs global targets", () => {
    expect(isRepoScoped({ kind: "terminal", terminalId: "t1" })).toBe(true);
    expect(isRepoScoped({ kind: "git-commit", repoPath: "/r" })).toBe(true);
    expect(isRepoScoped({ kind: "git-branches-search", repoPath: "/r" })).toBe(true);
    expect(isRepoScoped({ kind: "file-browser-search", repoPath: "/r" })).toBe(true);
    expect(isRepoScoped({ kind: "ai-chat" })).toBe(false);
    expect(isRepoScoped({ kind: "notes" })).toBe(false);
    expect(isRepoScoped({ kind: "md-tab", tabId: "x" })).toBe(false);
    expect(isRepoScoped({ kind: "plugin-iframe", tabId: "x" })).toBe(false);
  });

  it("records last global target", () => {
    recordFocus({ kind: "notes" });
    expect(getLastGlobal()).toEqual({ kind: "notes" });
  });

  it("stores repo-scoped targets that carry repoPath under their repo", () => {
    recordFocus({ kind: "git-commit", repoPath: "/repo/a" });
    expect(getForRepo("/repo/a")).toEqual({ kind: "git-commit", repoPath: "/repo/a" });
    expect(getForRepo("/repo/b")).toBeNull();
  });

  it("records terminal-to-repo association via recordTerminalRepo", () => {
    recordFocus({ kind: "terminal", terminalId: "t1" });
    // Terminals don't carry repoPath — association is explicit.
    expect(getForRepo("/repo/a")).toBeNull();
    recordTerminalRepo("t1", "/repo/a");
    expect(getForRepo("/repo/a")).toEqual({ kind: "terminal", terminalId: "t1" });
  });

  it("keeps per-repo memory independent across repos", () => {
    recordFocus({ kind: "git-commit", repoPath: "/repo/a" });
    recordFocus({ kind: "git-branches-search", repoPath: "/repo/b" });
    expect(getForRepo("/repo/a")).toEqual({ kind: "git-commit", repoPath: "/repo/a" });
    expect(getForRepo("/repo/b")).toEqual({ kind: "git-branches-search", repoPath: "/repo/b" });
  });

  it("overwrites the per-repo entry when a newer target is recorded for the same repo", () => {
    recordFocus({ kind: "git-commit", repoPath: "/repo/a" });
    recordFocus({ kind: "file-browser-search", repoPath: "/repo/a" });
    expect(getForRepo("/repo/a")).toEqual({ kind: "file-browser-search", repoPath: "/repo/a" });
  });

  it("clearForRepo removes only that repo's entry", () => {
    recordFocus({ kind: "git-commit", repoPath: "/repo/a" });
    recordFocus({ kind: "git-commit", repoPath: "/repo/b" });
    clearForRepo("/repo/a");
    expect(getForRepo("/repo/a")).toBeNull();
    expect(getForRepo("/repo/b")).toEqual({ kind: "git-commit", repoPath: "/repo/b" });
  });

  it("global pointer updates on every record, regardless of scope", () => {
    recordFocus({ kind: "git-commit", repoPath: "/repo/a" });
    expect(getLastGlobal()).toEqual({ kind: "git-commit", repoPath: "/repo/a" });
    recordFocus({ kind: "ai-chat" });
    expect(getLastGlobal()).toEqual({ kind: "ai-chat" });
    // Non-repo-scoped record must not pollute per-repo memory.
    expect(getForRepo("/repo/a")).toEqual({ kind: "git-commit", repoPath: "/repo/a" });
  });
});
