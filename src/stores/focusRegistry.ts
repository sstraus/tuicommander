/**
 * Focus registry — remembers the last-focused UI surface, globally and per-repo.
 *
 * Targets are identified via `data-focus-target` attributes on DOM elements.
 * A global `focusin` listener records every transition; on view transitions
 * (repo switch, window focus regain) the registry is consulted to restore
 * focus to the element the user was last typing in.
 */

export type FocusTarget =
  | { kind: "terminal"; terminalId: string }
  | { kind: "md-tab"; tabId: string }
  | { kind: "plugin-iframe"; tabId: string }
  | { kind: "ai-chat" }
  | { kind: "notes" }
  | { kind: "git-commit"; repoPath: string }
  | { kind: "git-branches-search"; repoPath: string }
  | { kind: "file-browser-search"; repoPath: string };

/** Targets tied to a specific repo — lose meaning when the user switches away. */
const REPO_SCOPED_KINDS = new Set<FocusTarget["kind"]>([
  "terminal",
  "git-commit",
  "git-branches-search",
  "file-browser-search",
]);

export function isRepoScoped(target: FocusTarget): boolean {
  return REPO_SCOPED_KINDS.has(target.kind);
}

interface Registry {
  lastGlobal: FocusTarget | null;
  byRepo: Map<string, FocusTarget>;
}

const registry: Registry = {
  lastGlobal: null,
  byRepo: new Map(),
};

export function recordFocus(target: FocusTarget): void {
  registry.lastGlobal = target;
  if ("repoPath" in target) {
    registry.byRepo.set(target.repoPath, target);
  } else if (target.kind === "terminal") {
    // Repo association for terminals is resolved by the caller and passed via
    // a separate recordTerminalRepo() call (terminals don't carry repoPath).
  }
}

/** Associate a repoPath with the last-recorded terminal target. */
export function recordTerminalRepo(terminalId: string, repoPath: string): void {
  registry.byRepo.set(repoPath, { kind: "terminal", terminalId });
}

export function getLastGlobal(): FocusTarget | null {
  return registry.lastGlobal;
}

export function getForRepo(repoPath: string): FocusTarget | null {
  return registry.byRepo.get(repoPath) ?? null;
}

export function clearForRepo(repoPath: string): void {
  registry.byRepo.delete(repoPath);
}
