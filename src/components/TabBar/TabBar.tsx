import { Component, For, Show, batch, createSignal, createEffect, createMemo, onCleanup, onMount } from "solid-js";
import { terminalsStore } from "../../stores/terminals";
import { paneLayoutStore } from "../../stores/paneLayout";
import { computeLeafRects } from "../../utils/paneTreeGeometry";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { makeBranchKey } from "../../stores/tabManager";
import { getModifierSymbol, shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { GlobeIcon } from "../GlobeIcon";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { initMouseDrag } from "../../hooks/useMouseDrag";
import { findPaneGroupAtPoint } from "../../stores/dragDrop";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { fileContextSmartMenuItem } from "../../utils/promptContext";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import s from "./TabBar.module.css";

import type { LeafRect } from "../../utils/paneTreeGeometry";

/** Mini-map SVG showing pane layout with one pane highlighted */
const PanePositionIcon: Component<{ tabId: string; rects: LeafRect[] }> = (props) => {
  const groupId = () => paneLayoutStore.getGroupForTab(props.tabId);

  const W = 14;
  const H = 10;
  const PAD = 0.5;
  const GAP = 0.8;

  return (
    <Show when={paneLayoutStore.isSplit() && groupId()}>
      <svg
        width={W}
        height={H}
        viewBox={`0 0 ${W} ${H}`}
        class={s.panePositionIcon}
        fill="none"
      >
        <For each={props.rects}>
          {(rect) => {
            const active = () => rect.groupId === groupId();
            return (
              <rect
                x={rect.x * (W - GAP) + PAD}
                y={rect.y * (H - GAP) + PAD}
                width={rect.w * (W - GAP) - GAP}
                height={rect.h * (H - GAP) - GAP}
                rx="1"
                fill={active() ? "currentColor" : "none"}
                stroke="currentColor"
                stroke-width="0.7"
                opacity={active() ? 0.9 : 0.35}
              />
            );
          }}
        </For>
      </svg>
    </Show>
  );
};

/** Map awaiting input type to module class */
const AWAITING_CLASSES: Record<string, string> = {
  question: s.awaitingQuestion,
  error: s.awaitingError,
};

export interface TabBarProps {
  quickSwitcherActive?: boolean;
  onTabSelect: (id: string) => void;
  onTabClose: (id: string) => void;
  onCloseOthers: (id: string) => void;
  onCloseToRight: (id: string) => void;
  onNewTab: () => void;
  onSplitVertical?: () => void;
  onSplitHorizontal?: () => void;
  onReorder?: (fromIndex: number, toIndex: number) => void;
  onDetachTab?: (id: string) => void;
  onMoveToWorktree?: (terminalId: string, worktreePath: string) => void;
  getWorktreeTargets?: (terminalId: string) => Array<{ branchName: string; path: string }>;
}

export const TabBar: Component<TabBarProps> = (props) => {
  const [dragOverId, setDragOverId] = createSignal<string | null>(null);
  const [dragOverSide, setDragOverSide] = createSignal<"left" | "right" | null>(null);
  const [draggingId, setDraggingId] = createSignal<string | null>(null);
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const smartPrompts = useSmartPrompts();

  /** Append a Smart Prompts submenu to a tab context menu if any file-context
   *  prompts exist. Mutates `items` in-place. */
  const pushFileContextItem = (items: ContextMenuItem[], filePath: string | undefined) => {
    if (!filePath) return;
    const repoRoot = repositoriesStore.getActive()?.path ?? null;
    const item = fileContextSmartMenuItem(
      { absPath: filePath, repoRoot, isDir: false },
      smartPrompts,
      { separator: true },
    );
    if (item) items.push(item);
  };

  // Context menu for tabs
  const tabMenu = createContextMenu();
  const [contextTabId, setContextTabId] = createSignal<string | null>(null);

  // Context menu for new tab + button
  const newTabMenu = createContextMenu();

  // Context menu for overflow tabs (right-click on scroll arrows)
  const overflowMenu = createContextMenu();

  // Shared memo for pane position mini-map (avoids N redundant tree walks)
  const paneRects = createMemo(() => {
    const root = paneLayoutStore.getRoot();
    return root ? computeLeafRects(root) : [];
  });
  const mod = getModifierSymbol();

  const getNewTabMenuItems = (): ContextMenuItem[] => [
    { label: t("tabBar.newTab", "New Tab"), shortcut: `${mod}T`, action: () => props.onNewTab() },
    { label: "", separator: true, action: () => {} },
    { label: t("tabBar.splitVertical", "Split Vertically"), shortcut: `${mod}\\`, action: () => props.onSplitVertical?.(), disabled: !paneLayoutStore.isSplit() && !terminalsStore.state.activeId },
    { label: t("tabBar.splitHorizontal", "Split Horizontally"), shortcut: `${mod}Alt+\\`, action: () => props.onSplitHorizontal?.(), disabled: !paneLayoutStore.isSplit() && !terminalsStore.state.activeId },
  ];

  const openNewTabMenu = (e: MouseEvent) => {
    e.stopPropagation();
    const btn = e.currentTarget as HTMLElement;
    const rect = btn.getBoundingClientRect();
    newTabMenu.openAt(rect.left, rect.bottom + 4);
  };

  const getTabContextMenuItems = (): ContextMenuItem[] => {
    const id = contextTabId();
    if (!id) return [];

    if (id.startsWith("diff-")) {
      const tab = diffTabsStore.get(id);
      const ids = visibleDiffIds();
      const idx = ids.indexOf(id);
      const isPinned = tab?.pinned ?? false;
      const diffItems: ContextMenuItem[] = [
        { label: t("tabBar.copyPath", "Copy Path"), action: () => { if (tab?.filePath) navigator.clipboard.writeText(shortenHomePath(tab.filePath)).catch((err) => appLogger.error("app", "Failed to copy path", err)); } },
        { label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"), action: () => diffTabsStore.setPinned(id, !isPinned) },
        { label: "", separator: true, action: () => {} },
        { label: t("tabBar.closeTab", "Close Tab"), action: () => { diffTabsStore.remove(id); props.onTabClose(id); } },
        { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
      pushFileContextItem(diffItems, tab?.filePath);
      return diffItems;
    }

    if (id.startsWith("md-")) {
      const tab = mdTabsStore.get(id);
      const ids = visibleMdIds();
      const idx = ids.indexOf(id);
      const isPinned = tab?.pinned ?? false;
      const hasPath = tab?.type === "file" && tab.filePath;
      const mdItems: ContextMenuItem[] = [
        ...(hasPath ? [{ label: t("tabBar.copyPath", "Copy Path"), action: () => { navigator.clipboard.writeText(shortenHomePath(tab.filePath)).catch((err) => appLogger.error("app", "Failed to copy path", err)); } }] : []),
        { label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"), action: () => mdTabsStore.setPinned(id, !isPinned) },
        { label: t("tabBar.print", "Print…"), action: () => window.print() },
        { label: "", separator: true, action: () => {} },
        { label: t("tabBar.closeTab", "Close Tab"), action: () => { mdTabsStore.remove(id); props.onTabClose(id); } },
        { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
      pushFileContextItem(mdItems, hasPath ? tab.filePath : undefined);
      return mdItems;
    }

    if (id.startsWith("edit-")) {
      const tab = editorTabsStore.get(id);
      const ids = visibleEditIds();
      const idx = ids.indexOf(id);
      const isPinned = tab?.pinned ?? false;
      const editItems: ContextMenuItem[] = [
        { label: t("tabBar.copyPath", "Copy Path"), action: () => { if (tab?.filePath) navigator.clipboard.writeText(shortenHomePath(tab.filePath)).catch((err) => appLogger.error("app", "Failed to copy path", err)); } },
        { label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"), action: () => editorTabsStore.setPinned(id, !isPinned) },
        { label: "", separator: true, action: () => {} },
        { label: t("tabBar.closeTab", "Close Tab"), action: () => props.onTabClose(id) },
        { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
      pushFileContextItem(editItems, tab?.filePath);
      return editItems;
    }

    // Terminal tab
    const ids = activeTerminals();
    const idx = ids.indexOf(id);
    const hasSession = !!terminalsStore.get(id)?.sessionId;
    const worktreeTargets = props.getWorktreeTargets?.(id) ?? [];
    const items: ContextMenuItem[] = [
      { label: t("tabBar.closeTab", "Close Tab"), shortcut: `${getModifierSymbol()}W`, action: () => props.onTabClose(id) },
      { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
      { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      { label: "", separator: true, action: () => {} },
      { label: t("tabBar.renameTab", "Rename Tab"), action: () => setEditingId(id) },
      { label: t("tabBar.detachToWindow", "Detach to Window"), action: () => props.onDetachTab?.(id), disabled: !hasSession },
    ];
    if (worktreeTargets.length > 0) {
      items.push(
        { label: "", separator: true, action: () => {} },
        {
          label: t("tabBar.moveToWorktree", "Move to Worktree"),
          action: () => {},
          children: worktreeTargets.map((wt) => ({
            label: wt.branchName,
            action: () => props.onMoveToWorktree?.(id, wt.path),
          })),
        },
      );
    }
    // Global workspace promote/unpromote
    const isPromoted = globalWorkspaceStore.isPromoted(id);
    items.push(
      { label: "", separator: true, action: () => {} },
      {
        label: isPromoted
          ? t("tabBar.removeFromWorkspace", "Remove from Global Workspace")
          : t("tabBar.promoteToWorkspace", "Add to Global Workspace"),
        action: () => globalWorkspaceStore.togglePromote(id),
      },
    );
    // Plugin-registered tab actions
    const tabActions = contextMenuActionsStore.getContextActions("tab");
    if (tabActions.length > 0) {
      const ctx = { target: "tab" as const, tabId: id, sessionId: terminalsStore.get(id)?.sessionId ?? undefined };
      for (const a of tabActions) {
        items.push({
          label: a.label,
          action: () => a.action(ctx),
          disabled: a.disabled?.(ctx),
          separator: tabActions.indexOf(a) === 0,
        });
      }
    }
    return items;
  };

  const openTabContextMenu = (e: MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    setContextTabId(id);
    tabMenu.open(e);
  };

  // Get terminals for active branch only, ordered by pane layout when split.
  // When global workspace is active, show only promoted terminals instead.
  let prevTerminalIds: string[] = [];
  const activeTerminals = () => {
    let ids: string[];
    if (globalWorkspaceStore.isActive()) {
      ids = globalWorkspaceStore.getPromotedIds();
    } else {
      const activeRepoPath = repositoriesStore.state.activeRepoPath;
      if (!activeRepoPath) {
        ids = terminalsStore.getIds();
      } else {
        const repo = repositoriesStore.state.repositories[activeRepoPath];
        if (!repo || !repo.activeBranch) {
          ids = terminalsStore.getIds();
        } else {
          const branch = repo.branches[repo.activeBranch];
          ids = branch?.terminals || [];
        }
      }
    }

    // During branch switch, hold previous tab list to prevent flash of empty tabs
    if (ids.length === 0 && repositoriesStore.state.branchSwitching) {
      return prevTerminalIds;
    }

    prevTerminalIds = ids;

    // Diagnostic: detect when TabBar shows 0 terminals but the store has some
    // that belong to the current branch (ignore terminals from other repos/branches)
    if (ids.length === 0 && !repositoriesStore.state.branchSwitching) {
      const activeRepoPath = repositoriesStore.state.activeRepoPath;
      const repo = activeRepoPath ? repositoriesStore.state.repositories[activeRepoPath] : null;
      const activeBranch = repo?.activeBranch;
      const branchTerminals = activeBranch ? repo!.branches[activeBranch]?.terminals : null;
      if (branchTerminals && branchTerminals.length > 0) {
        appLogger.warn("app", "TabBar: activeTerminals empty but branch has terminals", {
          branchTerminals,
          activeRepoPath,
          activeBranch,
          activeId: terminalsStore.state.activeId,
        });
      }
    }

    // In split mode, reorder tabs to match the spatial pane layout (DFS order)
    if (!paneLayoutStore.isSplit()) return ids;
    const idSet = new Set(ids);
    const ordered: string[] = [];
    for (const groupId of paneLayoutStore.getAllGroupIds()) {
      const group = paneLayoutStore.state.groups[groupId];
      if (!group) continue;
      for (const tab of group.tabs) {
        if (tab.type === "terminal" && idSet.has(tab.id)) {
          ordered.push(tab.id);
          idSet.delete(tab.id);
        }
      }
    }
    // Append any orphan terminals not assigned to a pane group
    for (const id of ids) {
      if (idSet.has(id)) ordered.push(id);
    }
    return ordered;
  };

  // Branch key for filtering non-terminal tabs
  const activeBranchKey = () => {
    const repoPath = repositoriesStore.state.activeRepoPath;
    if (!repoPath) return null;
    const repo = repositoriesStore.state.repositories[repoPath];
    if (!repo?.activeBranch) return null;
    return makeBranchKey(repoPath, repo.activeBranch);
  };

  const visibleDiffIds = () => diffTabsStore.getVisibleIds(activeBranchKey());
  const visibleMdIds = () => mdTabsStore.getVisibleIds(activeBranchKey());
  const visibleEditIds = () => editorTabsStore.getVisibleIds(activeBranchKey());

  // Deactivate non-terminal tabs that become invisible after branch switch
  createEffect(() => {
    const diffActive = diffTabsStore.state.activeId;
    if (diffActive && !visibleDiffIds().includes(diffActive)) {
      diffTabsStore.setActive(null);
    }
    const mdActive = mdTabsStore.state.activeId;
    if (mdActive && !visibleMdIds().includes(mdActive)) {
      mdTabsStore.setActive(null);
    }
    const editActive = editorTabsStore.state.activeId;
    if (editActive && !visibleEditIds().includes(editActive)) {
      editorTabsStore.setActive(null);
    }
  });

  // Evict non-pinned plugin-panel tabs from other repos on repo switch — they
  // would otherwise pile up forever, invisible but still holding HTML in memory.
  createEffect(() => {
    const current = repositoriesStore.state.activeRepoPath;
    mdTabsStore.evictNonPinnedPluginPanelsForOtherRepos(current);
  });

  // Mouse-based drag — replaces HTML5 DnD which conflicts with Tauri dragDropEnabled=true
  const handleMouseDrag = (e: MouseEvent, id: string, _tabType: "terminal" | "markdown" | "diff" | "editor" = "terminal") => {
    initMouseDrag(e, e.currentTarget as HTMLElement, {
      onStart: () => setDraggingId(id),
      onMove: (x, y) => {
        const el = document.elementFromPoint(x, y);
        const tabEl = el?.closest("[data-tab-id]") as HTMLElement | null;
        if (tabEl && tabEl.dataset.tabId !== id) {
          const rect = tabEl.getBoundingClientRect();
          setDragOverId(tabEl.dataset.tabId!);
          setDragOverSide(x < rect.left + rect.width / 2 ? "left" : "right");
        } else {
          setDragOverId(null);
          setDragOverSide(null);
        }
      },
      onDrop: (x, y) => {
        const sourceId = id;
        // 1. Cross-pane move (split mode)
        if (paneLayoutStore.isSplit()) {
          const targetGroup = findPaneGroupAtPoint(x, y);
          if (targetGroup) {
            const fromGroup = paneLayoutStore.getGroupForTab(sourceId);
            if (fromGroup && fromGroup !== targetGroup) {
              paneLayoutStore.moveTab(fromGroup, targetGroup, sourceId);
              paneLayoutStore.setActiveGroup(targetGroup);
              resetDragState();
              return;
            }
            if (!fromGroup) {
              // Orphan tab (created before split) — assign to drop target
              const type = terminalsStore.get(sourceId) ? "terminal" as const
                : diffTabsStore.get(sourceId) ? "diff" as const
                : editorTabsStore.get(sourceId) ? "editor" as const
                : "markdown" as const;
              paneLayoutStore.addTab(targetGroup, { id: sourceId, type });
              paneLayoutStore.setActiveGroup(targetGroup);
              resetDragState();
              return;
            }
          }
        }
        // 2. Tab reorder
        const el = document.elementFromPoint(x, y);
        const tabEl = el?.closest("[data-tab-id]") as HTMLElement | null;
        if (tabEl) {
          const targetId = tabEl.dataset.tabId!;
          if (targetId !== sourceId) {
            const termIds = activeTerminals();
            const fromIndex = termIds.indexOf(sourceId);
            const toIndex = termIds.indexOf(targetId);
            if (fromIndex !== -1 && toIndex !== -1) {
              // Terminal → terminal reorder
              const rect = tabEl.getBoundingClientRect();
              const side = x < rect.left + rect.width / 2 ? "left" : "right";
              let adjustedTo = toIndex;
              if (side === "left" && fromIndex < toIndex) adjustedTo--;
              else if (side === "right" && fromIndex > toIndex) adjustedTo++;
              const clampedTo = Math.max(0, Math.min(adjustedTo, termIds.length - 1));
              if (fromIndex !== clampedTo) {
                props.onReorder?.(fromIndex, clampedTo);
              }
            } else if (visibleMdIds().includes(sourceId) && visibleMdIds().includes(targetId)) {
              // Non-terminal (diff/markdown/plugin-panel) → same-type reorder
              const rect = tabEl.getBoundingClientRect();
              const side = x < rect.left + rect.width / 2 ? "before" : "after";
              mdTabsStore.reorderByIds(sourceId, targetId, side);
            }
          }
        }
        resetDragState();
      },
      onCancel: () => resetDragState(),
    });
  };

  const resetDragState = () => {
    setDraggingId(null);
    setDragOverId(null);
    setDragOverSide(null);
  };

  const commitRename = (id: string, input: HTMLInputElement) => {
    const newName = input.value.trim();
    if (newName) {
      terminalsStore.update(id, { name: newName, nameIsCustom: true });
    }
    setEditingId(null);
  };

  // Scroll state for arrows and fade gradients
  let tabsRef: HTMLDivElement | undefined;
  const [canScrollLeft, setCanScrollLeft] = createSignal(false);
  const [canScrollRight, setCanScrollRight] = createSignal(false);

  const updateScrollState = () => {
    const el = tabsRef;
    if (!el) return;
    const threshold = 2; // Avoid floating-point edge cases
    batch(() => {
      setCanScrollLeft(el.scrollLeft > threshold);
      setCanScrollRight(el.scrollLeft + el.clientWidth < el.scrollWidth - threshold);
    });
  };

  const scrollBy = (delta: number) => {
    tabsRef?.scrollBy({ left: delta, behavior: "smooth" });
  };

  onMount(() => {
    updateScrollState();
    const ro = new ResizeObserver(updateScrollState);
    if (tabsRef) ro.observe(tabsRef);
    onCleanup(() => ro.disconnect());
  });

  // Scroll active tab into view when selection changes
  createEffect(() => {
    const activeId = terminalsStore.state.activeId;
    const diffActive = diffTabsStore.state.activeId;
    const mdActive = mdTabsStore.state.activeId;
    const editActive = editorTabsStore.state.activeId;
    // Track any active id to trigger the effect
    void (activeId || diffActive || mdActive || editActive);
    requestAnimationFrame(() => {
      if (!tabsRef) return;
      const activeEl = tabsRef.querySelector(`.${s.active}`) as HTMLElement | null;
      activeEl?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
    });
  });

  /** Collect tab IDs and names that are clipped in the given direction */
  const getOverflowItems = (direction: "left" | "right"): ContextMenuItem[] => {
    const el = tabsRef;
    if (!el) return [];
    const containerRect = el.getBoundingClientRect();
    const items: ContextMenuItem[] = [];

    const tabEls = el.querySelectorAll(`.${s.tab}`) as NodeListOf<HTMLElement>;
    for (const tabEl of tabEls) {
      const rect = tabEl.getBoundingClientRect();
      const clipped = direction === "left"
        ? rect.left < containerRect.left - 1
        : rect.right > containerRect.right + 1;
      if (!clipped) continue;

      const nameEl = tabEl.querySelector(`.${s.tabName}`) as HTMLElement | null;
      const label = nameEl?.textContent ?? "Tab";

      // Determine which tab this DOM element represents by finding its click handler
      // We read the data from the element title or text content
      items.push({
        label,
        action: () => {
          tabEl.click();
          tabEl.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
        },
      });
    }

    return items;
  };

  const [overflowItems, setOverflowItems] = createSignal<ContextMenuItem[]>([]);

  const openOverflowMenu = (e: MouseEvent, direction: "left" | "right") => {
    e.preventDefault();
    e.stopPropagation();
    const items = getOverflowItems(direction);
    if (items.length === 0) return;
    setOverflowItems(items);
    const btn = e.currentTarget as HTMLElement;
    const rect = btn.getBoundingClientRect();
    overflowMenu.openAt(rect.left, rect.bottom + 4);
  };

  const chevronLeft = <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path fill-rule="evenodd" d="M11.354 1.646a.5.5 0 010 .708L5.707 8l5.647 5.646a.5.5 0 01-.708.708l-6-6a.5.5 0 010-.708l6-6a.5.5 0 01.708 0z"/></svg>;
  const chevronRight = <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path fill-rule="evenodd" d="M4.646 1.646a.5.5 0 01.708 0l6 6a.5.5 0 010 .708l-6 6a.5.5 0 01-.708-.708L10.293 8 4.646 2.354a.5.5 0 010-.708z"/></svg>;

  return (
    <div class={s.tabBarWrapper}>
      <div class={s.scrollRegion}>
      {/* Left scroll arrow */}
      <button
        class={cx(s.scrollArrow, s.scrollArrowLeft, canScrollLeft() && s.visible)}
        onClick={() => scrollBy(-200)}
        onContextMenu={(e) => openOverflowMenu(e, "left")}
        title="Scroll tabs left"
      >{chevronLeft}</button>

      {/* Left fade gradient */}
      <div class={cx(s.fadeGradient, s.fadeLeft, canScrollLeft() && s.visible)} />

      <div class={s.tabs} ref={tabsRef} onScroll={updateScrollState}>
      {/* Terminal tabs */}
      <For each={activeTerminals()}>
        {(id, index) => {
          const terminal = () => terminalsStore.get(id);
          const isActive = () => terminalsStore.state.activeId === id;
          const isBusy = () => terminalsStore.isBusy(id);
          const isIdle = () => !isBusy() && terminal()?.shellState === "idle";
          const isExited = () => terminal()?.shellState === "exited";
          const isUnseen = () => !isActive() && terminal()?.unseen;
          const awaitingInput = () => terminal()?.awaitingInput;
          const isDragging = () => draggingId() === id;
          const isDragOver = () => dragOverId() === id && draggingId() !== id;
          const progress = () => terminal()?.progress;
          const isRemote = () => terminal()?.isRemote;
          const isEditing = () => editingId() === id;

          const isPromoted = () => globalWorkspaceStore.isPromoted(id);
          const [hovered, setHovered] = createSignal(false);
          const repoName = () => repositoriesStore.getRepoForTerminal(id);

          const handleCloseTab = (e: Event) => {
            e.preventDefault();
            e.stopPropagation();
            props.onTabClose(id);
          };

          return (
            <Show when={terminal()}>
              <div
                class={cx(
                  s.tab,
                  isActive() && s.active,
                  awaitingInput() && s.awaitingInput,
                  awaitingInput() && AWAITING_CLASSES[awaitingInput()!],
                  // Suppress busy/idle indicators when awaiting input —
                  // the awaiting state (orange/red) takes visual priority.
                  !awaitingInput() && isBusy() && s.shellBusy,
                  !awaitingInput() && !isBusy() && isUnseen() && s.shellUnseen,
                  !awaitingInput() && isIdle() && !isUnseen() && s.shellIdle,
                  isExited() && s.shellExited,
                  isRemote() && s.remoteTab,
                  isDragging() && s.dragging,
                  isDragOver() && dragOverSide() === "left" && s.dragOverLeft,
                  isDragOver() && dragOverSide() === "right" && s.dragOverRight,
                )}
                data-tab-id={id}
                onClick={() => props.onTabSelect(id)}
                onAuxClick={(e) => {
                  if (e.button === 1) handleCloseTab(e);
                }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={`Terminal ${index() + 1} (${getModifierSymbol()}${index() + 1})`}
                onMouseDown={(e) => !isEditing() && handleMouseDrag(e, id)}
                onMouseEnter={() => setHovered(true)}
                onMouseLeave={() => setHovered(false)}
                onDblClick={(e) => {
                  e.stopPropagation();
                  setEditingId(id);
                }}
              >
                <span class={s.tabIcon}>●</span>
                <Show when={isEditing()} fallback={
                  <span class={s.tabName}>
                    {terminal()?.name}
                    {progress() !== null && progress() !== undefined && (
                      <span class={s.progressLabel}>{progress()}%</span>
                    )}
                  </span>
                }>
                  <input
                    class={s.tabNameInput}
                    type="text"
                    value={terminal()?.name || ""}
                    ref={(el) => {
                      // Auto-focus and select all text when entering edit mode
                      requestAnimationFrame(() => {
                        el.focus();
                        el.select();
                      });
                    }}
                    onClick={(e) => e.stopPropagation()}
                    onBlur={(e) => commitRename(id, e.currentTarget)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        commitRename(id, e.currentTarget);
                      } else if (e.key === "Escape") {
                        setEditingId(null);
                      }
                    }}
                  />
                </Show>
                {progress() !== null && progress() !== undefined && (
                  <div class={s.progress} style={{ width: `${progress()}%` }} />
                )}
                <PanePositionIcon tabId={id} rects={paneRects()} />
                <Show when={isPromoted() && !globalWorkspaceStore.isActive()}>
                  <button
                    class={s.globeIcon}
                    title={t("tabBar.removeFromWorkspace", "Remove from Global Workspace")}
                    onClick={(e) => {
                      e.stopPropagation();
                      globalWorkspaceStore.unpromote(id);
                    }}
                  >
                    <GlobeIcon size={11} />
                  </button>
                </Show>
                <Show when={props.quickSwitcherActive && index() < 9}>
                  <span class={s.shortcutBadge}>{getModifierSymbol()}{index() + 1}</span>
                </Show>
                <Show when={hovered() && globalWorkspaceStore.isActive() && repoName()}>
                  <span class={s.repoOverlay}>{repoName()}</span>
                </Show>
                <button
                  class={s.tabClose}
                  title={t("tabBar.close", "Close")}
                  onClick={handleCloseTab}
                >
                  ×
                </button>
              </div>
            </Show>
          );
        }}
      </For>

      {/* Diff tabs */}
      <For each={visibleDiffIds()}>
        {(id) => {
          const diffTab = () => diffTabsStore.get(id);
          const isActive = () => diffTabsStore.state.activeId === id;

          return (
            <Show when={diffTab()}>
              <div
                class={cx(s.tab, s.diffTab, isActive() && s.active)}
                data-tab-id={id}
                onClick={() => {
                  diffTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); diffTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={diffTab()?.filePath}
                onMouseDown={(e) => handleMouseDrag(e, id, "diff")}
              >
                <span class={s.tabIcon}>
                  {diffTab()?.filePath
                    ? <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                        <path fill-rule="evenodd" d="M3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5.5L9.5 1H3zm6.5 1.5v2.5H12L9.5 2.5zM8 6a.5.5 0 01.5.5v1h1a.5.5 0 010 1h-1v1a.5.5 0 01-1 0v-1h-1a.5.5 0 010-1h1v-1A.5.5 0 018 6zm-3 5a.5.5 0 000 1h5a.5.5 0 000-1H5z"/>
                      </svg>
                    : <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                        <path d="M2 2h12v1H2zm0 3h12v1H2zm0 3h10v1H2zm0 3h8v1H2z" />
                      </svg>
                  }
                </span>
                <Show when={diffTab()?.pinned}><span class={s.pinIcon}><svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="4" r="3.5"/><path d="M6.2 7l-1.7 4.5a.5.5 0 0 0 .13.54l2.84 2.84a.75.75 0 0 0 1.06 0l2.84-2.84a.5.5 0 0 0 .13-.54L9.8 7H6.2z"/></svg></span></Show>
                <span class={s.tabName}>{diffTab()?.fileName}{diffTab()?.scope ? ` (${diffTab()?.scope?.slice(0, 7)})` : ""}</span>
                <PanePositionIcon tabId={id} rects={paneRects()} />
                <button
                  class={s.tabClose}
                  title={t("tabBar.close", "Close")}
                  onClick={(e) => {
                    e.stopPropagation();
                    diffTabsStore.remove(id);
                    props.onTabClose(id);
                  }}
                >
                  ×
                </button>
              </div>
            </Show>
          );
        }}
      </For>

      {/* Markdown tabs */}
      <For each={visibleMdIds()}>
        {(id) => {
          const mdTab = () => mdTabsStore.get(id);
          const isActive = () => mdTabsStore.state.activeId === id;

          return (
            <Show when={mdTab()}>
              <div
                class={cx(s.tab, mdTab()?.type === "file" ? s.mdTab : mdTab()?.type === "pr-diff" ? s.diffTab : s.panelTab, isActive() && s.active)}
                data-tab-id={id}
                onClick={() => {
                  mdTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); mdTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={(() => { const tab = mdTab(); return tab?.type === "file" ? tab.filePath : tab?.type === "pr-diff" ? `PR #${tab.prNumber}: ${tab.prTitle}` : tab?.title; })()}
                onMouseDown={(e) => handleMouseDrag(e, id, "markdown")}
              >
                <span class={s.tabIcon}>
                  {mdTab()?.type === "pr-diff" ? (
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                      <path fill-rule="evenodd" d="M7.177 3.073L9.573.677A.25.25 0 0110 .854v4.792a.25.25 0 01-.427.177L7.177 3.427a.25.25 0 010-.354zM3.75 2.5a.75.75 0 100 1.5.75.75 0 000-1.5zm-2.25.75a2.25 2.25 0 113 2.122v5.256a2.251 2.251 0 11-1.5 0V5.372A2.25 2.25 0 011.5 3.25zM11 2.5h-1V4h1a1 1 0 011 1v5.628a2.251 2.251 0 101.5 0V5A2.5 2.5 0 0011 2.5zm1 10.25a.75.75 0 111.5 0 .75.75 0 01-1.5 0zM3.75 12a.75.75 0 100 1.5.75.75 0 000-1.5z"/>
                    </svg>
                  ) : (
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                      <path fill-rule="evenodd" d="M3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5.5L9.5 1H3zm6.5 1.5v2.5H12L9.5 2.5zM4.5 7.5h7a.5.5 0 010 1h-7a.5.5 0 010-1zm0 2.5h7a.5.5 0 010 1h-7a.5.5 0 010-1zm0 2.5h4a.5.5 0 010 1h-4a.5.5 0 010-1z"/>
                    </svg>
                  )}
                </span>
                <Show when={mdTab()?.pinned}><span class={s.pinIcon}><svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="4" r="3.5"/><path d="M6.2 7l-1.7 4.5a.5.5 0 0 0 .13.54l2.84 2.84a.75.75 0 0 0 1.06 0l2.84-2.84a.5.5 0 0 0 .13-.54L9.8 7H6.2z"/></svg></span></Show>
                <span class={s.tabName}>{(() => { const tab = mdTab(); return tab?.type === "file" ? tab.fileName : tab?.title; })()}</span>
                <PanePositionIcon tabId={id} rects={paneRects()} />
                <button
                  class={s.tabClose}
                  title={t("tabBar.close", "Close")}
                  onClick={(e) => {
                    e.stopPropagation();
                    mdTabsStore.remove(id);
                    props.onTabClose(id);
                  }}
                >
                  ×
                </button>
              </div>
            </Show>
          );
        }}
      </For>

      {/* Editor tabs */}
      <For each={visibleEditIds()}>
        {(id) => {
          const editTab = () => editorTabsStore.get(id);
          const isActive = () => editorTabsStore.state.activeId === id;

          return (
            <Show when={editTab()}>
              <div
                class={cx(s.tab, s.editTab, isActive() && s.active)}
                data-tab-id={id}
                onClick={() => {
                  editorTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={editTab()?.filePath}
                onMouseDown={(e) => handleMouseDrag(e, id, "editor")}
              >
                <span class={s.tabIcon}>
                  {editTab()?.isDirty
                    ? <svg width="8" height="8" viewBox="0 0 8 8" fill="currentColor"><circle cx="4" cy="4" r="4"/></svg>
                    : <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M11.13 1.47a1.5 1.5 0 012.12 0l1.28 1.28a1.5 1.5 0 010 2.12L5.9 13.5a1 1 0 01-.5.27l-3.5.87a.5.5 0 01-.6-.6l.87-3.5a1 1 0 01.27-.5L11.13 1.47zm1.07 1.06L3.74 11l-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77z"/></svg>
                  }
                </span>
                <Show when={editTab()?.pinned}><span class={s.pinIcon}><svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="4" r="3.5"/><path d="M6.2 7l-1.7 4.5a.5.5 0 0 0 .13.54l2.84 2.84a.75.75 0 0 0 1.06 0l2.84-2.84a.5.5 0 0 0 .13-.54L9.8 7H6.2z"/></svg></span></Show>
                <span class={s.tabName}>{editTab()?.fileName}</span>
                <PanePositionIcon tabId={id} rects={paneRects()} />
                <button
                  class={s.tabClose}
                  title={t("tabBar.close", "Close")}
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onTabClose(id);
                  }}
                >
                  ×
                </button>
              </div>
            </Show>
          );
        }}
      </For>

      </div>{/* end .tabs */}

      {/* Right fade gradient */}
      <div class={cx(s.fadeGradient, s.fadeRight, canScrollRight() && s.visible)} />

      {/* Right scroll arrow */}
      <button
        class={cx(s.scrollArrow, s.scrollArrowRight, canScrollRight() && s.visible)}
        onClick={() => scrollBy(200)}
        onContextMenu={(e) => openOverflowMenu(e, "right")}
        title="Scroll tabs right"
      >{chevronRight}</button>
      </div>{/* end .scrollRegion */}

      {/* New Tab button: outside scroll region so arrows don't overlap it */}
      <button class={s.newBtn} onClick={() => props.onNewTab()} onContextMenu={openNewTabMenu} title={`${t("tabBar.newTab", "New Tab")} (${mod}T)`}>
        +
      </button>

      <ContextMenu
        items={getTabContextMenuItems()}
        x={tabMenu.position().x}
        y={tabMenu.position().y}
        visible={tabMenu.visible()}
        onClose={tabMenu.close}
      />
      <ContextMenu
        items={getNewTabMenuItems()}
        x={newTabMenu.position().x}
        y={newTabMenu.position().y}
        visible={newTabMenu.visible()}
        onClose={newTabMenu.close}
      />
      <ContextMenu
        items={overflowItems()}
        x={overflowMenu.position().x}
        y={overflowMenu.position().y}
        visible={overflowMenu.visible()}
        onClose={overflowMenu.close}
      />
    </div>
  );
};

export default TabBar;
