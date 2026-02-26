import { createMemo, createSignal } from "solid-js";
import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { diffTabsStore } from "../stores/diffTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { settingsStore } from "../stores/settings";
import { appLogger } from "../stores/appLogger";
import { filterValidTerminals } from "../utils/terminalFilter";

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
      fontSize: deps.getDefaultFontSize(),
      name: `Terminal ${count + 1}`,
      cwd: activeTerminal?.cwd ?? null,
      awaitingInput: null,
    });

    terminalsStore.setActive(id);
    return id;
  };

  /** After closing a non-terminal tab, select a sibling or fall back to the last terminal */
  const selectAfterNonTerminalClose = (store: { getIds: () => string[]; setActive: (id: string | null) => void }, closedId: string) => {
    const remaining = store.getIds().filter((i) => i !== closedId);
    if (remaining.length > 0) {
      handleTerminalSelect(remaining[remaining.length - 1]);
    } else {
      // No more tabs of this type — return to last terminal on active branch
      const activeRepo = repositoriesStore.getActive();
      const branchTerminals = activeRepo?.activeBranch
        ? (activeRepo.branches[activeRepo.activeBranch]?.terminals ?? [])
        : [];
      const nextTerminal = branchTerminals.length > 0
        ? branchTerminals[branchTerminals.length - 1]
        : terminalsStore.getIds()[0] ?? null;
      if (nextTerminal) {
        handleTerminalSelect(nextTerminal);
      }
    }
  };

  const closeTerminal = async (id: string, skipConfirm = false) => {
    if (id.startsWith("diff-")) {
      selectAfterNonTerminalClose(diffTabsStore, id);
      diffTabsStore.remove(id);
      return;
    }

    if (id.startsWith("md-")) {
      selectAfterNonTerminalClose(mdTabsStore, id);
      mdTabsStore.remove(id);
      return;
    }

    if (id.startsWith("edit-")) {
      selectAfterNonTerminalClose(editorTabsStore, id);
      editorTabsStore.remove(id);
      return;
    }

    const terminal = terminalsStore.get(id);
    if (!terminal) return;

    if (!skipConfirm && terminal.sessionId && settingsStore.state.confirmBeforeClosingTab) {
      const confirmed = await deps.dialogs.confirmCloseTerminal(terminal.name);
      if (!confirmed) return;
    }

    setClosedTabs((prev) => [...prev.slice(-9), { name: terminal.name, fontSize: terminal.fontSize, cwd: terminal.cwd }]);

    if (terminal.sessionId) {
      try {
        await deps.pty.close(terminal.sessionId);
      } catch (err) {
        appLogger.error("terminal", "Failed to close PTY", err);
      }
    }

    // Collapse split layout if closing a pane that belongs to a split
    const layout = terminalsStore.state.layout;
    const splitIndex = layout.direction !== "none" ? layout.panes.indexOf(id) : -1;
    let survivorId: string | null = null;
    if (splitIndex !== -1 && layout.panes.length === 2) {
      survivorId = layout.panes[splitIndex === 0 ? 1 : 0];
      const paneIndex: 0 | 1 = splitIndex === 0 ? 0 : 1;
      terminalsStore.closeSplitPane(paneIndex);
    }

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

  const copyFromTerminal = async () => {
    try {
      const selection = window.getSelection()?.toString();
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
  };

  const handleTerminalSelect = (id: string) => {
    if (id.startsWith("diff-")) {
      diffTabsStore.setActive(id);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      terminalsStore.setActive(null);
    } else if (id.startsWith("md-")) {
      mdTabsStore.setActive(id);
      diffTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      terminalsStore.setActive(null);
    } else if (id.startsWith("edit-")) {
      editorTabsStore.setActive(id);
      diffTabsStore.setActive(null);
      mdTabsStore.setActive(null);
      terminalsStore.setActive(null);
    } else {
      terminalsStore.setActive(id);
      diffTabsStore.setActive(null);
      mdTabsStore.setActive(null);
      editorTabsStore.setActive(null);
      const terminal = terminalsStore.get(id);
      terminal?.ref?.focus();
    }
  };

  return {
    activeFontSize,
    zoomIn,
    zoomOut,
    zoomReset,
    createNewTerminal,
    closeTerminal,
    closeOtherTabs,
    closeTabsToRight,
    reopenClosedTab,
    navigateTab,
    clearTerminal,
    copyFromTerminal,
    pasteToTerminal,
    handleTerminalFocus,
    handleTerminalSelect,
    terminalIds,
  };
}
