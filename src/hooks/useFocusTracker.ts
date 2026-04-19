import { createEffect, on, onCleanup } from "solid-js";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import {
  type FocusTarget,
  getForRepo,
  getLastGlobal,
  isRepoScoped,
  recordFocus,
  recordTerminalRepo,
} from "../stores/focusRegistry";

/**
 * Walk up the DOM from `el` looking for the nearest `[data-focus-target]`
 * host and decode the FocusTarget it represents. Returns null when the
 * element is not inside a tracked surface.
 */
function describeFocus(el: Element | null): FocusTarget | null {
  if (!el) return null;
  const host = el.closest<HTMLElement>("[data-focus-target]");
  if (!host) return null;
  const kind = host.dataset.focusTarget;
  switch (kind) {
    case "terminal": {
      const terminalId = host.dataset.terminalId;
      return terminalId ? { kind: "terminal", terminalId } : null;
    }
    case "md-tab": {
      const tabId = host.dataset.tabId;
      return tabId ? { kind: "md-tab", tabId } : null;
    }
    case "plugin-iframe": {
      const tabId = host.dataset.tabId;
      return tabId ? { kind: "plugin-iframe", tabId } : null;
    }
    case "ai-chat":
      return { kind: "ai-chat" };
    case "notes":
      return { kind: "notes" };
    case "git-commit": {
      const repoPath = host.dataset.repoPath;
      return repoPath ? { kind: "git-commit", repoPath } : null;
    }
    case "git-branches-search": {
      const repoPath = host.dataset.repoPath;
      return repoPath ? { kind: "git-branches-search", repoPath } : null;
    }
    case "file-browser-search": {
      const repoPath = host.dataset.repoPath;
      return repoPath ? { kind: "file-browser-search", repoPath } : null;
    }
  }
  return null;
}

function focusTarget(target: FocusTarget): boolean {
  switch (target.kind) {
    case "terminal": {
      const ref = terminalsStore.get(target.terminalId)?.ref;
      if (!ref) return false;
      ref.focus();
      return true;
    }
    case "md-tab":
    case "plugin-iframe": {
      const el = document.querySelector<HTMLElement>(
        `[data-focus-target="${target.kind}"][data-tab-id="${CSS.escape(target.tabId)}"]`,
      );
      if (!el) return false;
      el.focus({ preventScroll: true });
      return true;
    }
    case "ai-chat":
    case "notes": {
      const el = document.querySelector<HTMLElement>(`[data-focus-target="${target.kind}"]`);
      if (!el) return false;
      el.focus();
      return true;
    }
    case "git-commit":
    case "git-branches-search":
    case "file-browser-search": {
      const el = document.querySelector<HTMLElement>(
        `[data-focus-target="${target.kind}"][data-repo-path="${CSS.escape(target.repoPath)}"]`,
      );
      if (!el) return false;
      el.focus();
      return true;
    }
  }
}

/** Resolve the right target to restore when entering `newRepoPath`. */
function resolveRestoreTarget(newRepoPath: string | null): FocusTarget | null {
  const global = getLastGlobal();
  // Non-repo-scoped surface (AI chat, notes, md-tab, plugin-iframe) — stay put.
  if (global && !isRepoScoped(global)) return global;

  if (!newRepoPath) return null;

  const remembered = getForRepo(newRepoPath);
  if (remembered) return remembered;

  // Fallback: active terminal of the new repo's active branch.
  const repo = repositoriesStore.get(newRepoPath);
  if (repo?.activeBranch) {
    const branch = repo.branches[repo.activeBranch];
    const termId = branch?.lastActiveTerminal ?? branch?.terminals[0];
    if (termId) return { kind: "terminal", terminalId: termId };
  }
  return null;
}

/**
 * Return true when the current DOM focus already belongs to `newRepoPath`
 * (or to a non-repo-scoped surface we shouldn't disturb).
 */
function focusAlreadySettled(newRepoPath: string | null): boolean {
  const active = describeFocus(document.activeElement);
  if (!active) return false;
  if (!isRepoScoped(active)) return true;
  if ("repoPath" in active) return active.repoPath === newRepoPath;
  if (active.kind === "terminal") {
    const termRepo = repositoriesStore.getRepoPathForTerminal(active.terminalId);
    return termRepo === newRepoPath;
  }
  return false;
}

/**
 * Install the focus tracker. Records every focus transition into the
 * registry and restores focus when the active repo changes or the window
 * regains focus. Call once at app init.
 */
export function useFocusTracker(): void {
  const onFocusIn = (e: FocusEvent) => {
    const target = describeFocus(e.target as Element | null);
    if (!target) return;
    recordFocus(target);
    if (target.kind === "terminal") {
      const repoPath = repositoriesStore.getRepoPathForTerminal(target.terminalId);
      if (repoPath) recordTerminalRepo(target.terminalId, repoPath);
    }
  };

  const onWindowFocus = () => {
    const target = getLastGlobal();
    if (!target) return;
    // Only restore when focus has fallen to body — don't steal from whatever
    // legitimately has focus already.
    if (document.activeElement && document.activeElement !== document.body) return;
    requestAnimationFrame(() => focusTarget(target));
  };

  document.addEventListener("focusin", onFocusIn);
  window.addEventListener("focus", onWindowFocus);

  createEffect(on(() => repositoriesStore.state.activeRepoPath, (newPath) => {
    if (focusAlreadySettled(newPath)) return;
    const target = resolveRestoreTarget(newPath);
    if (!target) return;
    // Defer so tab/panel re-renders triggered by the repo switch settle first.
    requestAnimationFrame(() => focusTarget(target));
  }, { defer: true }));

  onCleanup(() => {
    document.removeEventListener("focusin", onFocusIn);
    window.removeEventListener("focus", onWindowFocus);
  });
}
