import { Component, For, Show, createSignal } from "solid-js";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { settingsStore } from "../../stores/settings";
import { getModifierSymbol } from "../../platform";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";

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
  const isSplitActive = () => terminalsStore.state.layout.direction !== "none";
  const mod = getModifierSymbol();

  const getNewTabMenuItems = (): ContextMenuItem[] => [
    { label: "New Tab", shortcut: `${mod}T`, action: () => props.onNewTab() },
    { label: "", separator: true, action: () => {} },
    { label: "Split Vertically", shortcut: `${mod}\\`, action: () => props.onSplitVertical?.(), disabled: isSplitActive() },
    { label: "Split Horizontally", shortcut: `${mod}Alt+\\`, action: () => props.onSplitHorizontal?.(), disabled: isSplitActive() },
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
      const ids = diffTabsStore.getIds();
      const idx = ids.indexOf(id);
      return [
        { label: "Close Tab", action: () => { diffTabsStore.remove(id); props.onTabClose(id); } },
        { label: "Close Other Tabs", action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: "Close Tabs to the Right", action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
    }

    if (id.startsWith("md-")) {
      const ids = mdTabsStore.getIds();
      const idx = ids.indexOf(id);
      return [
        { label: "Close Tab", action: () => { mdTabsStore.remove(id); props.onTabClose(id); } },
        { label: "Close Other Tabs", action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: "Close Tabs to the Right", action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
    }

    if (id.startsWith("edit-")) {
      const ids = editorTabsStore.getIds();
      const idx = ids.indexOf(id);
      return [
        { label: "Close Tab", action: () => { editorTabsStore.remove(id); props.onTabClose(id); } },
        { label: "Close Other Tabs", action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
        { label: "Close Tabs to the Right", action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      ];
    }

    // Terminal tab
    const ids = activeTerminals();
    const idx = ids.indexOf(id);
    return [
      { label: "Close Tab", shortcut: `${getModifierSymbol()}W`, action: () => props.onTabClose(id) },
      { label: "Close Other Tabs", action: () => props.onCloseOthers(id), disabled: ids.length <= 1 },
      { label: "Close Tabs to the Right", action: () => props.onCloseToRight(id), disabled: idx >= ids.length - 1 },
      { label: "", separator: true, action: () => {} },
      { label: "Rename Tab", action: () => setEditingId(id) },
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
    <div id="tabs">
      {/* Terminal tabs */}
      {(() => {
        const layout = () => terminalsStore.state.layout;
        const isUnifiedMode = () => settingsStore.state.splitTabMode === "unified" && layout().direction !== "none";
        return (
      <For each={activeTerminals()}>
        {(id, index) => {
          const terminal = () => terminalsStore.get(id);
          const isActive = () => terminalsStore.state.activeId === id;
          const hasActivity = () => !isActive() && terminal()?.activity;
          const isIdle = () => !isActive() && terminal()?.shellState === "idle";
          const awaitingInput = () => terminal()?.awaitingInput;
          const awaitingClass = () => {
            const type = awaitingInput();
            if (!type) return "";
            return `awaiting-input awaiting-${type}`;
          };
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
                class={`tab ${(isActive() || isActiveInUnified()) ? "active" : ""} ${awaitingClass()} ${hasActivity() ? "has-activity" : ""} ${isIdle() ? "shell-idle" : ""} ${isDragging() ? "dragging" : ""} ${isDragOver() && dragOverSide() === "left" ? "drag-over-left" : ""} ${isDragOver() && dragOverSide() === "right" ? "drag-over-right" : ""}`}
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
                <span class="tab-icon">●</span>
                <Show when={isEditing()} fallback={
                  <span class="tab-name">
                    {isFirstSplitPane() ? unifiedName() : terminal()?.name}
                    {progress() !== null && progress() !== undefined && (
                      <span class="tab-progress-label">{progress()}%</span>
                    )}
                  </span>
                }>
                  <input
                    class="tab-name-input"
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
                  <div class="tab-progress" style={{ width: `${progress()}%` }} />
                )}
                <Show when={props.quickSwitcherActive && index() < 9}>
                  <span class="tab-shortcut-badge">{getModifierSymbol()}{index() + 1}</span>
                </Show>
                <button
                  class="tab-close"
                  title="Close"
                  onClick={handleCloseTab}
                >
                  ×
                </button>
              </div>
            </Show>
          );
        }}
      </For>
        );
      })()}

      {/* Diff tabs */}
      <For each={diffTabsStore.getIds()}>
        {(id) => {
          const diffTab = () => diffTabsStore.get(id);
          const isActive = () => diffTabsStore.state.activeId === id;

          return (
            <Show when={diffTab()}>
              <div
                class={`tab diff-tab ${isActive() ? "active" : ""}`}
                onClick={() => {
                  diffTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); diffTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={diffTab()?.filePath}
              >
                <span class="tab-icon">📄</span>
                <span class="tab-name">{diffTab()?.fileName}{diffTab()?.scope ? ` (${diffTab()!.scope.slice(0, 7)})` : ""}</span>
                <button
                  class="tab-close"
                  title="Close"
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
      <For each={mdTabsStore.getIds()}>
        {(id) => {
          const mdTab = () => mdTabsStore.get(id);
          const isActive = () => mdTabsStore.state.activeId === id;

          return (
            <Show when={mdTab()}>
              <div
                class={`tab md-tab ${isActive() ? "active" : ""}`}
                onClick={() => {
                  mdTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); mdTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={mdTab()?.filePath}
              >
                <span class="tab-icon">📝</span>
                <span class="tab-name">{mdTab()?.fileName}</span>
                <button
                  class="tab-close"
                  title="Close"
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
      <For each={editorTabsStore.getIds()}>
        {(id) => {
          const editTab = () => editorTabsStore.get(id);
          const isActive = () => editorTabsStore.state.activeId === id;

          return (
            <Show when={editTab()}>
              <div
                class={`tab edit-tab ${isActive() ? "active" : ""}`}
                onClick={() => {
                  editorTabsStore.setActive(id);
                  props.onTabSelect(id);
                }}
                onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); editorTabsStore.remove(id); props.onTabClose(id); } }}
                onContextMenu={(e) => openTabContextMenu(e, id)}
                title={editTab()?.filePath}
              >
                <span class="tab-icon">{editTab()?.isDirty ? "●" : "✎"}</span>
                <span class="tab-name">{editTab()?.fileName}</span>
                <button
                  class="tab-close"
                  title="Close"
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
      <button class="tab-new-btn" onClick={() => props.onNewTab()} onContextMenu={openNewTabMenu} title={`New Tab (${mod}T)`} style={{ position: "relative" }}>
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
