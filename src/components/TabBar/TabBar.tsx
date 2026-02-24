import { Component, For, Show, createSignal, createEffect } from "solid-js";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { settingsStore } from "../../stores/settings";
import { makeBranchKey } from "../../stores/tabManager";
import { getModifierSymbol } from "../../platform";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { t } from "../../i18n";
import { cx } from "../../utils";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import s from "./TabBar.module.css";

/** Map awaiting input type to module class */
const AWAITING_CLASSES: Record<string, string> = {
  question: s.awaitingQuestion,
  error: s.awaitingError,
  confirmation: s.awaitingConfirmation,
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
}

export const TabBar: Component<TabBarProps> = (props) => {
  const [dragOverId, setDragOverId] = createSignal<string | null>(null);
  const [dragOverSide, setDragOverSide] = createSignal<"left" | "right" | null>(null);
  const [draggingId, setDraggingId] = createSignal<string | null>(null);
  const [editingId, setEditingId] = createSignal<string | null>(null);

  // Context menu for tabs
  const tabMenu = createContextMenu();
  const [contextTabId, setContextTabId] = createSignal<string | null>(null);

  // Context menu for new tab + button
  const newTabMenu = createContextMenu();
  const layout = () => terminalsStore.state.layout;
  const isSplitActive = () => layout().direction !== "none";
  const isUnifiedMode = () => settingsStore.state.splitTabMode === "unified" && layout().direction !== "none";
  const mod = getModifierSymbol();

  const getNewTabMenuItems = (): ContextMenuItem[] => [
    { label: t("tabBar.newTab", "New Tab"), shortcut: `${mod}T`, action: () => props.onNewTab() },
    { label: "", separator: true, action: () => {} },
    { label: t("tabBar.splitVertical", "Split Vertically"), shortcut: `${mod}\\`, action: () => props.onSplitVertical?.(), disabled: isSplitActive() },
    { label: t("tabBar.splitHorizontal", "Split Horizontally"), shortcut: `${mod}Alt+\\`, action: () => props.onSplitHorizontal?.(), disabled: isSplitActive() },
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
      return [
        { label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"), action: () => diffTabsStore.setPinned(id, !isPinned) },
        { label: "", separator: true, action: () => {} },
        { label: t("tabBar.closeTab", "Close Tab"), action: () => { diffTabsStore.remove(id); props.onTabClose(id); } },
        { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
    }

    if (id.startsWith("md-")) {
      const tab = mdTabsStore.get(id);
      const ids = visibleMdIds();
      const idx = ids.indexOf(id);
      const isPinned = tab?.pinned ?? false;
      return [
        { label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"), action: () => mdTabsStore.setPinned(id, !isPinned) },
        { label: "", separator: true, action: () => {} },
        { label: t("tabBar.closeTab", "Close Tab"), action: () => { mdTabsStore.remove(id); props.onTabClose(id); } },
        { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
    }

    if (id.startsWith("edit-")) {
      const tab = editorTabsStore.get(id);
      const ids = visibleEditIds();
      const idx = ids.indexOf(id);
      const isPinned = tab?.pinned ?? false;
      return [
        { label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"), action: () => editorTabsStore.setPinned(id, !isPinned) },
        { label: "", separator: true, action: () => {} },
        { label: t("tabBar.closeTab", "Close Tab"), action: () => { editorTabsStore.remove(id); props.onTabClose(id); } },
        { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
    }

    // Terminal tab
    const ids = activeTerminals();
    const idx = ids.indexOf(id);
    const hasSession = !!terminalsStore.get(id)?.sessionId;
    return [
      { label: t("tabBar.closeTab", "Close Tab"), shortcut: `${getModifierSymbol()}W`, action: () => props.onTabClose(id) },
      { label: t("tabBar.closeOthers", "Close Other Tabs"), action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
      { label: t("tabBar.closeRight", "Close Tabs to the Right"), action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      { label: "", separator: true, action: () => {} },
      { label: t("tabBar.renameTab", "Rename Tab"), action: () => setEditingId(id) },
      { label: t("tabBar.detachToWindow", "Detach to Window"), action: () => props.onDetachTab?.(id), disabled: !hasSession },
    ];
  };

  const openTabContextMenu = (e: MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    setContextTabId(id);
    tabMenu.open(e);
  };

  // Get terminals for active branch only
  const activeTerminals = () => {
    const activeRepoPath = repositoriesStore.state.activeRepoPath;
    if (!activeRepoPath) return terminalsStore.getIds();
    const repo = repositoriesStore.state.repositories[activeRepoPath];
    if (!repo || !repo.activeBranch) return terminalsStore.getIds();
    const branch = repo.branches[repo.activeBranch];
    return branch?.terminals || [];
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

  const handleDragStart = (e: DragEvent, id: string) => {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
    // Use the tab element itself as drag image to suppress macOS green plus icon
    try {
      const el = e.currentTarget as HTMLElement;
      e.dataTransfer.setDragImage(el, el.offsetWidth / 2, el.offsetHeight / 2);
    } catch { /* happy-dom doesn't implement setDragImage */ }
    setDraggingId(id);
  };

  const handleDragOver = (e: DragEvent, id: string) => {
    e.preventDefault();
    if (!e.dataTransfer) return;
    e.dataTransfer.dropEffect = "move";

    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midpoint = rect.left + rect.width / 2;
    const side = e.clientX < midpoint ? "left" : "right";

    setDragOverId(id);
    setDragOverSide(side);
  };

  const handleDragLeave = () => {
    setDragOverId(null);
    setDragOverSide(null);
  };

  const handleDrop = (e: DragEvent, targetId: string) => {
    e.preventDefault();
    const sourceId = e.dataTransfer?.getData("text/plain");
    if (!sourceId || sourceId === targetId) {
      resetDragState();
      return;
    }

    const ids = activeTerminals();
    const fromIndex = ids.indexOf(sourceId);
    const toIndex = ids.indexOf(targetId);

    if (fromIndex !== -1 && toIndex !== -1 && fromIndex !== toIndex) {
      // Adjust target index based on drop side when available
      let adjustedTo = toIndex;
      const side = dragOverSide();
      if (side === "left" && fromIndex < toIndex) {
        adjustedTo = toIndex - 1;
      } else if (side === "right" && fromIndex > toIndex) {
        adjustedTo = toIndex + 1;
      }
      const clampedTo = Math.max(0, Math.min(adjustedTo, ids.length - 1));
      if (fromIndex !== clampedTo) {
        props.onReorder?.(fromIndex, clampedTo);
      }
    }

    resetDragState();
  };

  const handleDragEnd = () => {
    resetDragState();
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

  return (
    <div class={s.tabs}>
      {/* Terminal tabs */}
      <For each={activeTerminals()}>
        {(id, index) => {
          const terminal = () => terminalsStore.get(id);
          const isActive = () => terminalsStore.state.activeId === id;
          const hasActivity = () => !isActive() && terminal()?.activity;
          const isIdle = () => !isActive() && terminal()?.shellState === "idle";
          const awaitingInput = () => terminal()?.awaitingInput;
          const isDragging = () => draggingId() === id;
          const isDragOver = () => dragOverId() === id && draggingId() !== id;
          const progress = () => terminal()?.progress;
          const isEditing = () => editingId() === id;

          // Unified split tab mode: hide second pane's tab, show combined name on first
          const isSecondSplitPane = () => isUnifiedMode() && layout().panes[1] === id;
          const isFirstSplitPane = () => isUnifiedMode() && layout().panes[0] === id;
          const unifiedName = () => {
            if (!isFirstSplitPane()) return terminal()?.name;
            const secondId = layout().panes[1];
            const second = secondId ? terminalsStore.get(secondId) : undefined;
            return `${terminal()?.name} | ${second?.name ?? ""}`;
          };

          // In unified mode, hide the second split pane's tab
          const isActiveInUnified = () => isFirstSplitPane() && layout().panes.includes(terminalsStore.state.activeId || "");

          // Close this tab, and in unified mode also close the paired pane
          const handleCloseTab = (e: Event) => {
            e.preventDefault();
            e.stopPropagation();
            if (isFirstSplitPane()) {
              const secondId = layout().panes[1];
              if (secondId) props.onTabClose(secondId);
            }
            props.onTabClose(id);
          };

          return (
            <Show when={terminal() && !isSecondSplitPane()}>
              <div
                class={cx(
                  s.tab,
                  (isActive() || isActiveInUnified()) && s.active,
                  awaitingInput() && s.awaitingInput,
                  awaitingInput() && AWAITING_CLASSES[awaitingInput()!],
                  hasActivity() && s.hasActivity,
                  isIdle() && s.shellIdle,
                  isDragging() && s.dragging,
                  isDragOver() && dragOverSide() === "left" && s.dragOverLeft,
                  isDragOver() && dragOverSide() === "right" && s.dragOverRight,
                )}
                onClick={() => props.onTabSelect(id)}
                onAuxClick={(e) => {
                  if (e.button === 1) handleCloseTab(e);
                }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={`Terminal ${index() + 1} (${getModifierSymbol()}${index() + 1})`}
                draggable={!isEditing()}
                onDragStart={(e) => handleDragStart(e, id)}
                onDragOver={(e) => handleDragOver(e, id)}
                onDragLeave={handleDragLeave}
                onDrop={(e) => handleDrop(e, id)}
                onDragEnd={handleDragEnd}
                onDblClick={(e) => {
                  e.stopPropagation();
                  setEditingId(id);
                }}
              >
                <span class={s.tabIcon}>●</span>
                <Show when={isEditing()} fallback={
                  <span class={s.tabName}>
                    {isFirstSplitPane() ? unifiedName() : terminal()?.name}
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
                <Show when={props.quickSwitcherActive && index() < 9}>
                  <span class={s.shortcutBadge}>{getModifierSymbol()}{index() + 1}</span>
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
                onClick={() => {
                  diffTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); diffTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={diffTab()?.filePath}
              >
                <span class={s.tabIcon}>
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path fill-rule="evenodd" d="M3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5.5L9.5 1H3zm6.5 1.5v2.5H12L9.5 2.5zM8 6a.5.5 0 01.5.5v1h1a.5.5 0 010 1h-1v1a.5.5 0 01-1 0v-1h-1a.5.5 0 010-1h1v-1A.5.5 0 018 6zm-3 5a.5.5 0 000 1h5a.5.5 0 000-1H5z"/>
                  </svg>
                </span>
                <Show when={diffTab()?.pinned}><span class={s.pinIcon}><svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path d="M4.146.854a.5.5 0 1 1 .708-.708l4 4a.5.5 0 0 1-.708.708L7.5 4.208V6.5a.5.5 0 0 1-.146.354L5 9.207l1.146 1.147a.5.5 0 0 1-.353.853H2.5a.5.5 0 0 1-.354-.853L3.293 9.207 1 6.914a.5.5 0 0 1 0-.707L4.146.854z" transform="rotate(45 8 8)"/></svg></span></Show>
                <span class={s.tabName}>{diffTab()?.fileName}{diffTab()?.scope ? ` (${diffTab()?.scope?.slice(0, 7)})` : ""}</span>
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
                class={cx(s.tab, mdTab()?.type === "file" ? s.mdTab : s.panelTab, isActive() && s.active)}
                onClick={() => {
                  mdTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); mdTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={(() => { const tab = mdTab(); return tab?.type === "file" ? tab.filePath : tab?.title; })()}
              >
                <span class={s.tabIcon}>
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path fill-rule="evenodd" d="M3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5.5L9.5 1H3zm6.5 1.5v2.5H12L9.5 2.5zM4.5 7.5h7a.5.5 0 010 1h-7a.5.5 0 010-1zm0 2.5h7a.5.5 0 010 1h-7a.5.5 0 010-1zm0 2.5h4a.5.5 0 010 1h-4a.5.5 0 010-1z"/>
                  </svg>
                </span>
                <Show when={mdTab()?.pinned}><span class={s.pinIcon}><svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path d="M4.146.854a.5.5 0 1 1 .708-.708l4 4a.5.5 0 0 1-.708.708L7.5 4.208V6.5a.5.5 0 0 1-.146.354L5 9.207l1.146 1.147a.5.5 0 0 1-.353.853H2.5a.5.5 0 0 1-.354-.853L3.293 9.207 1 6.914a.5.5 0 0 1 0-.707L4.146.854z" transform="rotate(45 8 8)"/></svg></span></Show>
                <span class={s.tabName}>{(() => { const tab = mdTab(); return tab?.type === "file" ? tab.fileName : tab?.title; })()}</span>
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
                onClick={() => {
                  editorTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); editorTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={editTab()?.filePath}
              >
                <span class={s.tabIcon}>
                  {editTab()?.isDirty
                    ? <svg width="8" height="8" viewBox="0 0 8 8" fill="currentColor"><circle cx="4" cy="4" r="4"/></svg>
                    : <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M11.13 1.47a1.5 1.5 0 012.12 0l1.28 1.28a1.5 1.5 0 010 2.12L5.9 13.5a1 1 0 01-.5.27l-3.5.87a.5.5 0 01-.6-.6l.87-3.5a1 1 0 01.27-.5L11.13 1.47zm1.07 1.06L3.74 11l-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77z"/></svg>
                  }
                </span>
                <Show when={editTab()?.pinned}><span class={s.pinIcon}><svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path d="M4.146.854a.5.5 0 1 1 .708-.708l4 4a.5.5 0 0 1-.708.708L7.5 4.208V6.5a.5.5 0 0 1-.146.354L5 9.207l1.146 1.147a.5.5 0 0 1-.353.853H2.5a.5.5 0 0 1-.354-.853L3.293 9.207 1 6.914a.5.5 0 0 1 0-.707L4.146.854z" transform="rotate(45 8 8)"/></svg></span></Show>
                <span class={s.tabName}>{editTab()?.fileName}</span>
                <button
                  class={s.tabClose}
                  title={t("tabBar.close", "Close")}
                  onClick={(e) => {
                    e.stopPropagation();
                    editorTabsStore.remove(id);
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

      {/* New Tab button: click = new tab, right-click = split menu */}
      <button class={s.newBtn} onClick={() => props.onNewTab()} onContextMenu={openNewTabMenu} title={`${t("tabBar.newTab", "New Tab")} (${mod}T)`} style={{ position: "relative" }}>
        +
        <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{mod}T</span>
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
    </div>
  );
};

export default TabBar;
