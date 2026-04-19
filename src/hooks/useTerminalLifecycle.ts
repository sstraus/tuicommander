import { createMemo, createSignal } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { repositoriesStore, currentBranchKey } from "../stores/repositories";
import { diffTabsStore } from "../stores/diffTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { settingsStore } from "../stores/settings";
import { appLogger } from "../stores/appLogger";
import { filterValidTerminals } from "../utils/terminalFilter";
import { assignTabToActiveGroup } from "../utils/paneTabAssign";
import { paneLayoutStore } from "../stores/paneLayout";
import { invoke } from "../invoke";

const MIN_FONT_SIZE = 8;
const MAX_FONT_SIZE = 32;
const FONT_STEP = 2;

/** Dependencies injected into useTerminalLifecycle */
export interface TerminalLifecycleDeps {
  pty: {
    canSpawn: () => Promise<boolean>;
    close: (sessionId: string) => Promise<void>;
  };
  dialogs: {
    confirmCloseTerminal: (name: string) => Promise<boolean>;
    confirm: (options: {
      title: string;
      message: string;
      okLabel?: string;
      cancelLabel?: string;
      kind?: "info" | "warning" | "error";
    }) => Promise<boolean>;
  };
  setStatusInfo: (msg: string) => void;
  getDefaultFontSize: () => number;
}

/** Terminal lifecycle management: zoom, create/close, navigate, clipboard */
export function useTerminalLifecycle(deps: TerminalLifecycleDeps) {
  const [closedTabs, setClosedTabs] = createSignal<Array<{ name: string; fontSize: number; cwd: string | null }>>([]);

  const activeFontSize = () => {
    const active = terminalsStore.getActive();
    return active?.fontSize ?? deps.getDefaultFontSize();
  };

  const setZoom = (fontSize: number) => {
    const activeId = terminalsStore.state.activeId;
    if (!activeId) return;
    const clamped = Math.max(MIN_FONT_SIZE, Math.min(MAX_FONT_SIZE, fontSize));
    terminalsStore.setFontSize(activeId, clamped);
  };

  const zoomIn = () => setZoom(activeFontSize() + FONT_STEP);
  const zoomOut = () => setZoom(activeFontSize() - FONT_STEP);
  const zoomReset = () => setZoom(deps.getDefaultFontSize());

  const setZoomAll = (getFontSize: (current: number) => number) => {
    for (const id of terminalsStore.getIds()) {
      const current = terminalsStore.state.terminals[id]?.fontSize ?? deps.getDefaultFontSize();
      const clamped = Math.max(MIN_FONT_SIZE, Math.min(MAX_FONT_SIZE, getFontSize(current)));
      terminalsStore.setFontSize(id, clamped);
    }
  };
  const zoomInAll = () => setZoomAll((cur) => cur + FONT_STEP);
  const zoomOutAll = () => setZoomAll((cur) => cur - FONT_STEP);
  const zoomResetAll = () => setZoomAll(() => deps.getDefaultFontSize());

  const createNewTerminal = async () => {
    const canSpawn = await deps.pty.canSpawn();
    if (!canSpawn) {
      deps.setStatusInfo("Max sessions reached (50)");
      return;
    }

    const activeTerminal = terminalsStore.getActive();
    const count = terminalsStore.getCount();
    const id = terminalsStore.add({
      sessionId: null,
      fontSize: activeTerminal?.fontSize ?? deps.getDefaultFontSize(),
      name: `Terminal ${count + 1}`,
      cwd: activeTerminal?.cwd ?? repositoriesStore.state.activeRepoPath ?? null,
      awaitingInput: null,
    });

    assignTabToActiveGroup(id, "terminal");
    terminalsStore.setActive(id);
    return id;
  };

  /** After closing a non-terminal tab, select a sibling on the same branch or fall back to the last terminal */
  /** Remove a tab from its pane group; if the group becomes empty, collapse the split. */
  const removeTabFromPane = (tabId: string) => {
    const containingGroup = paneLayoutStore.getGroupForTab(tabId);
    if (!containingGroup) return;
    paneLayoutStore.removeTab(containingGroup, tabId);
    const updated = paneLayoutStore.state.groups[containingGroup];
    if (!updated || updated.tabs.length === 0) {
      paneLayoutStore.closePane(containingGroup);
    }
  };

  const selectAfterNonTerminalClose = (store: { getIds: () => string[]; getVisibleIds: (key: string | null) => string[]; setActive: (id: string | null) => void }, closedId: string) => {
    const branchKey = currentBranchKey() ?? null;
    const remaining = store.getVisibleIds(branchKey).filter((i) => i !== closedId);
    if (remaining.length > 0) {
      handleTerminalSelect(remaining[remaining.length - 1]);
    } else {
      // No more tabs of this type — return to last terminal on active branch (never pick orphans)
      const activeRepo = repositoriesStore.getActive();
      const branchTerminals = activeRepo?.activeBranch
        ? (activeRepo.branches[activeRepo.activeBranch]?.terminals ?? [])
        : [];
      const nextTerminal = branchTerminals.length > 0
        ? branchTerminals[branchTerminals.length - 1]
        : null;
      if (nextTerminal) {
        handleTerminalSelect(nextTerminal);
      }
    }
  };

  const closeTerminal = async (id: string, skipConfirm = false) => {
    if (id.startsWith("diff-")) {
      selectAfterNonTerminalClose(diffTabsStore, id);
      diffTabsStore.remove(id);
      removeTabFromPane(id);
      return;
    }

    if (id.startsWith("md-")) {
      selectAfterNonTerminalClose(mdTabsStore, id);
      mdTabsStore.remove(id);
      removeTabFromPane(id);
      return;
    }

    if (id.startsWith("edit-")) {
      const tab = editorTabsStore.get(id);
      if (!skipConfirm && tab?.isDirty) {
        const confirmed = await deps.dialogs.confirm({
          title: "Unsaved changes",
          message: `"${tab.fileName}" has unsaved changes.\nClose without saving?`,
          okLabel: "Close without saving",
          cancelLabel: "Cancel",
          kind: "warning",
        });
        if (!confirmed) return;
      }
      selectAfterNonTerminalClose(editorTabsStore, id);
      editorTabsStore.remove(id);
      removeTabFromPane(id);
      return;
    }

    const terminal = terminalsStore.get(id);
    if (!terminal) return;

    // Confirm only when a non-shell process is running (claude, htop, node, etc.).
    // Plain idle shells close immediately. We ask the backend for the foreground
    // process name — if it's a shell (zsh/bash/fish), it returns null.
    if (!skipConfirm && terminal.sessionId && settingsStore.state.confirmBeforeClosingTab) {
      try {
        const fg = await invoke<string | null>("has_foreground_process", { sessionId: terminal.sessionId });
        if (fg) {
          const confirmed = await deps.dialogs.confirmCloseTerminal(terminal.name);
          if (!confirmed) return;
        }
      } catch {
        // Backend call failed (session already gone) — close without confirmation
      }
    }

    setClosedTabs((prev) => [...prev.slice(-9), { name: terminal.name, fontSize: terminal.fontSize, cwd: terminal.cwd }]);

    if (terminal.sessionId) {
      try {
        await deps.pty.close(terminal.sessionId);
      } catch (err) {
        appLogger.error("terminal", "Failed to close PTY", err);
      }
    }

    // Remove terminal tab from its pane group (if any)
    removeTabFromPane(id);
    let survivorId: string | null = null;
    const activeGroup = paneLayoutStore.getActiveGroup();
    const termTab = activeGroup?.tabs.find(t => t.type === "terminal");
    if (termTab) survivorId = termTab.id;

    const activeRepo = repositoriesStore.getActive();
    if (activeRepo && activeRepo.activeBranch) {
      repositoriesStore.removeTerminalFromBranch(activeRepo.path, activeRepo.activeBranch, id);
    }

    const wasActive = terminalsStore.state.activeId === id;
    terminalsStore.remove(id);

    // Focus the next tab when closing the active one — handleTerminalSelect
    // sets activeId AND calls ref.focus() (terminalsStore.remove sets activeId to null).
    // Only select terminals from the same branch to avoid cross-repo activation.
    if (wasActive) {
      const branchTerminals = activeRepo?.activeBranch
        ? (activeRepo.branches[activeRepo.activeBranch]?.terminals ?? [])
        : [];
      const nextId = survivorId
        ?? (branchTerminals.length > 0 ? branchTerminals[branchTerminals.length - 1] : null);
      if (nextId) {
        // Defer one frame: SolidJS renders the next terminal on remove(),
        // but ref.focus() inside handleTerminalSelect needs the DOM node present.
        // Guard: the target may have been closed by a concurrent close-all loop.
        requestAnimationFrame(() => {
          if (terminalsStore.get(nextId) || nextId.startsWith("diff-") || nextId.startsWith("md-") || nextId.startsWith("edit-")) {
            handleTerminalSelect(nextId);
          }
        });
      }
    }
  };

  const closeOtherTabs = async (keepId: string) => {
    if (keepId.startsWith("diff-")) {
      for (const id of diffTabsStore.getIds()) {
        if (id !== keepId) { diffTabsStore.remove(id); }
      }
      diffTabsStore.setActive(keepId);
      handleTerminalSelect(keepId);
      return;
    }
    if (keepId.startsWith("md-")) {
      for (const id of mdTabsStore.getIds()) {
        if (id !== keepId) { mdTabsStore.remove(id); }
      }
      mdTabsStore.setActive(keepId);
      handleTerminalSelect(keepId);
      return;
    }
    if (keepId.startsWith("edit-")) {
      for (const id of editorTabsStore.getIds()) {
        if (id !== keepId) { editorTabsStore.remove(id); }
      }
      editorTabsStore.setActive(keepId);
      handleTerminalSelect(keepId);
      return;
    }
    const ids = filterValidTerminals(repositoriesStore.getActiveTerminals(), terminalsStore.getIds());
    for (const id of ids) {
      if (id !== keepId) {
        await closeTerminal(id, true);
      }
    }
    handleTerminalSelect(keepId);
  };

  const closeTabsToRight = async (afterId: string) => {
    if (afterId.startsWith("diff-")) {
      const ids = diffTabsStore.getIds();
      const idx = ids.indexOf(afterId);
      for (const id of ids.slice(idx + 1)) { diffTabsStore.remove(id); }
      return;
    }
    if (afterId.startsWith("md-")) {
      const ids = mdTabsStore.getIds();
      const idx = ids.indexOf(afterId);
      for (const id of ids.slice(idx + 1)) { mdTabsStore.remove(id); }
      return;
    }
    if (afterId.startsWith("edit-")) {
      const ids = editorTabsStore.getIds();
      const idx = ids.indexOf(afterId);
      for (const id of ids.slice(idx + 1)) { editorTabsStore.remove(id); }
      return;
    }
    const ids = filterValidTerminals(repositoriesStore.getActiveTerminals(), terminalsStore.getIds());
    const idx = ids.indexOf(afterId);
    for (const id of ids.slice(idx + 1)) {
      await closeTerminal(id, true);
    }
  };

  const reopenClosedTab = async () => {
    const closed = closedTabs();
    if (closed.length === 0) return;

    const last = closed[closed.length - 1];
    setClosedTabs((prev) => prev.slice(0, -1));

    const canSpawn = await deps.pty.canSpawn();
    if (!canSpawn) {
      deps.setStatusInfo("Max sessions reached (50)");
      return;
    }

    const id = terminalsStore.add({
      sessionId: null,
      fontSize: last.fontSize,
      name: last.name,
      cwd: last.cwd,
      awaitingInput: null,
    });

    terminalsStore.setActive(id);
  };

  /** Terminal IDs for the active branch (memoized) */
  const terminalIds = createMemo(() => {
    return filterValidTerminals(repositoriesStore.getActiveTerminals(), terminalsStore.getIds());
  });

  const navigateTab = (direction: "prev" | "next") => {
    const ids = terminalIds();
    if (ids.length <= 1) return;

    const currentIndex = ids.indexOf(terminalsStore.state.activeId || "");
    if (currentIndex === -1) return;

    let newIndex: number;
    if (direction === "prev") {
      newIndex = currentIndex === 0 ? ids.length - 1 : currentIndex - 1;
    } else {
      newIndex = currentIndex === ids.length - 1 ? 0 : currentIndex + 1;
    }

    handleTerminalSelect(ids[newIndex]);
  };

  const clearTerminal = () => {
    const active = terminalsStore.getActive();
    if (active?.ref) {
      active.ref.clear();
    }
  };

  const clearScrollback = () => {
    const active = terminalsStore.getActive();
    if (active?.ref) {
      active.ref.clear();
    }
  };

  const scrollToTop = () => terminalsStore.getActive()?.ref?.scrollToTop();
  const scrollToBottom = () => terminalsStore.getActive()?.ref?.scrollToBottom();
  const scrollPageUp = () => terminalsStore.getActive()?.ref?.scrollPages(-1);
  const scrollPageDown = () => terminalsStore.getActive()?.ref?.scrollPages(1);

  const copyFromTerminal = async () => {
    try {
      // Prefer xterm's selection (canvas-rendered, invisible to DOM).
      // Fall back to DOM selection for non-terminal panels (code editor, etc.).
      const active = terminalsStore.getActive();
      const selection = active?.ref?.getSelection() || window.getSelection()?.toString();
      if (selection) {
        await navigator.clipboard.writeText(selection);
        deps.setStatusInfo("Copied to clipboard");
      }
    } catch (err) {
      appLogger.error("terminal", "Failed to copy", err);
    }
  };

  const pasteToTerminal = async () => {
    try {
      const text = await navigator.clipboard.readText();
      const active = terminalsStore.getActive();
      if (active?.ref && text) {
        active.ref.write(text);
      }
    } catch (err) {
      appLogger.error("terminal", "Failed to paste", err);
    }
  };

  /** Guard against cross-repo terminal activation from stray DOM focus events.
   *  Hidden terminals (display:none) can receive xterm focus events — block those
   *  and restore focus to the legitimate active terminal. */
  const handleTerminalFocus = (id: string) => {
    const activeTerminals = repositoriesStore.getActiveTerminals();
    if (activeTerminals.length > 0 && !activeTerminals.includes(id)) {
      appLogger.warn("terminal", `handleTerminalFocus BLOCKED: ${id} not in active branch`, { activeTerminals });
      terminalsStore.getActive()?.ref?.focus();
      return;
    }
    terminalsStore.setActive(id);
    activateInPaneGroup(id, "terminal");
  };

  /** Activate a tab inside its pane group when split is on.
   *  Orphan tabs (not in any group) are left alone — they render
   *  full-screen via the flat rendering path in TerminalArea. */
  const activateInPaneGroup = (id: string, _type: "terminal" | "markdown" | "diff" | "editor") => {
    if (!paneLayoutStore.isSplit()) return;
    const groupId = paneLayoutStore.getGroupForTab(id);
    if (groupId) {
      paneLayoutStore.setActiveGroup(groupId);
      paneLayoutStore.setActiveTab(groupId, id);
    }
  };

  const handleTerminalSelect = (id: string) => {
    if (id.startsWith("diff-")) {
      diffTabsStore.setActive(id);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      terminalsStore.setActive(null);
      activateInPaneGroup(id, "diff");
    } else if (id.startsWith("md-")) {
      mdTabsStore.setActive(id);
      diffTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      terminalsStore.setActive(null);
      activateInPaneGroup(id, "markdown");
    } else if (id.startsWith("edit-")) {
      editorTabsStore.setActive(id);
      diffTabsStore.setActive(null);
      mdTabsStore.setActive(null);
      terminalsStore.setActive(null);
      activateInPaneGroup(id, "editor");
    } else {
      // Switch repo/branch context if the terminal belongs to a different one
      const repoPath = repositoriesStore.getRepoPathForTerminal(id);
      if (repoPath) {
        const repo = repositoriesStore.state.repositories[repoPath];
        if (repo) {
          // Find which branch owns this terminal
          for (const [branchName, branch] of Object.entries(repo.branches)) {
            if (branch.terminals.includes(id)) {
              if (repositoriesStore.state.activeRepoPath !== repoPath) {
                repositoriesStore.setActive(repoPath);
              }
              if (repo.activeBranch !== branchName) {
                repositoriesStore.setActiveBranch(repoPath, branchName);
              }
              break;
            }
          }
        }
      }
      terminalsStore.setActive(id);
      diffTabsStore.setActive(null);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      activateInPaneGroup(id, "terminal");
      const terminal = terminalsStore.get(id);
      terminal?.ref?.focus();
    }
  };

  return {
    activeFontSize,
    zoomIn,
    zoomOut,
    zoomReset,
    zoomInAll,
    zoomOutAll,
    zoomResetAll,
    createNewTerminal,
    closeTerminal,
    closeOtherTabs,
    closeTabsToRight,
    reopenClosedTab,
    navigateTab,
    clearTerminal,
    clearScrollback,
    scrollToTop,
    scrollToBottom,
    scrollPageUp,
    scrollPageDown,
    copyFromTerminal,
    pasteToTerminal,
    handleTerminalFocus,
    handleTerminalSelect,
    terminalIds,
  };
}
