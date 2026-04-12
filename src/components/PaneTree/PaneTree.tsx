import { Component, For, Show, Switch, Match, createMemo, createSignal } from "solid-js";
import { paneLayoutStore, type PaneNode, type PaneBranch, type PaneLeaf, type PaneTab, type PaneTabType, MIN_PANE_RATIO } from "../../stores/paneLayout";
import { Terminal } from "../Terminal";
import { DiffTab } from "../DiffTab";
import { MdTabContent } from "../shared/MdTabContent";
import { CodeEditorTab } from "../CodeEditorPanel";
import { terminalsStore } from "../../stores/terminals";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "../ContextMenu/ContextMenu";
import { getRepoColor } from "../../utils/repoColor";
import { markInternalDragStart, markInternalDragEnd } from "../../hooks/useFileDrop";
import { appLogger } from "../../stores/appLogger";
import { GlobeIcon } from "../GlobeIcon";
import "./PaneTree.css";

// ---- PaneNodeView: recursive tree renderer ----

export const PaneNodeView: Component<{
  node: PaneNode;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onTerminalFocus: (id: string) => void;
  onCwdChange?: (id: string, cwd: string) => void;
  onNewTerminal?: (groupId: string) => void;
}> = (props) => {
  return (
    <Switch>
      <Match when={props.node.type === "branch"}>
        <PaneBranchView
          branch={props.node as PaneBranch}
          onCloseTab={props.onCloseTab}
          onOpenFilePath={props.onOpenFilePath}
          onTerminalFocus={props.onTerminalFocus}
          onCwdChange={props.onCwdChange}
          onNewTerminal={props.onNewTerminal}
        />
      </Match>
      <Match when={props.node.type === "leaf"}>
        <PaneGroupView
          groupId={(props.node as PaneLeaf).id}
          onCloseTab={props.onCloseTab}
          onOpenFilePath={props.onOpenFilePath}
          onTerminalFocus={props.onTerminalFocus}
          onCwdChange={props.onCwdChange}
          onNewTerminal={props.onNewTerminal}
        />
      </Match>
    </Switch>
  );
};

// ---- PaneBranchView: flex container with resize handles ----

const PaneBranchView: Component<{
  branch: PaneBranch;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onTerminalFocus: (id: string) => void;
  onCwdChange?: (id: string, cwd: string) => void;
  onNewTerminal?: (groupId: string) => void;
}> = (props) => {
  const isVertical = () => props.branch.direction === "vertical";

  // Ratios are mutated in place by the resize handler for performance (avoids
  // rebuilding the whole tree on every mouse-move frame). Solid doesn't track
  // plain JS mutations, so we subscribe to treeRevision() explicitly to
  // re-evaluate the flex binding whenever ratios change.
  const flexFor = (idx: number): string => {
    paneLayoutStore.treeRevision();
    return `${(props.branch.ratios[idx] ?? 0.5) * 100} 1 0%`;
  };

  const handleMouseDown = (handleIndex: number, startEvent: MouseEvent) => {
    startEvent.preventDefault();
    const container = (startEvent.target as HTMLElement).parentElement;
    if (!container) return;

    const rect = container.getBoundingClientRect();
    const vertical = isVertical();

    let rafPending = false;
    const onMouseMove = (e: MouseEvent) => {
      if (rafPending) return;
      rafPending = true;
      requestAnimationFrame(() => {
        rafPending = false;
        const fraction = vertical
          ? (e.clientX - rect.left) / rect.width
          : (e.clientY - rect.top) / rect.height;

        // Update ratios in the tree — need to find this branch and update it
        // For now, update directly via store (Phase 3 will wire this properly)
        const newRatios = [...props.branch.ratios];
        let offset = 0;
        for (let i = 0; i < handleIndex; i++) offset += newRatios[i];
        const combined = newRatios[handleIndex] + newRatios[handleIndex + 1];
        const splitPoint = Math.max(MIN_PANE_RATIO, Math.min(fraction - offset, combined - MIN_PANE_RATIO));
        newRatios[handleIndex] = splitPoint;
        newRatios[handleIndex + 1] = combined - splitPoint;

        // Mutate the branch ratios directly (tree is plain JS, not proxied)
        props.branch.ratios[handleIndex] = newRatios[handleIndex];
        props.branch.ratios[handleIndex + 1] = newRatios[handleIndex + 1];
        // Trigger re-render
        paneLayoutStore.setRoot(paneLayoutStore.getRoot());
      });
    };

    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      // Re-fit terminals after resize
      for (const groupId of paneLayoutStore.getAllGroupIds()) {
        const group = paneLayoutStore.state.groups[groupId];
        if (!group) continue;
        for (const tab of group.tabs) {
          if (tab.type === "terminal") {
            terminalsStore.get(tab.id)?.ref?.fit();
          }
        }
      }
    };

    document.body.style.cursor = vertical ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  return (
    <div
      class="pane-branch"
      classList={{
        "pane-branch-vertical": isVertical(),
        "pane-branch-horizontal": !isVertical(),
      }}
    >
      <For each={props.branch.children}>
        {(child, i) => (
          <>
            <div
              class="pane-slot"
              style={{ flex: flexFor(i()) }}
            >
              <PaneNodeView
                node={child}
                onCloseTab={props.onCloseTab}
                onOpenFilePath={props.onOpenFilePath}
                onTerminalFocus={props.onTerminalFocus}
                onCwdChange={props.onCwdChange}
                onNewTerminal={props.onNewTerminal}
              />
            </div>
            <Show when={i() < props.branch.children.length - 1}>
              <div
                class="pane-resize-handle"
                classList={{
                  "pane-resize-vertical": isVertical(),
                  "pane-resize-horizontal": !isVertical(),
                }}
                onMouseDown={(e) => handleMouseDown(i(), e)}
              />
            </Show>
          </>
        )}
      </For>
    </div>
  );
};

// ---- PaneGroupView: leaf renderer with optional mini tab bar ----

const PaneGroupView: Component<{
  groupId: string;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onTerminalFocus: (id: string) => void;
  onCwdChange?: (id: string, cwd: string) => void;
  onNewTerminal?: (groupId: string) => void;
}> = (props) => {
  const group = () => paneLayoutStore.state.groups[props.groupId];
  const isActive = () => paneLayoutStore.state.activeGroupId === props.groupId;
  /** Tabs that reference resources still alive in their respective stores */
  const aliveTabs = createMemo(() => {
    const g = group();
    if (!g) return [];
    return g.tabs.filter((t) => {
      if (t.type === "terminal") return !!terminalsStore.get(t.id);
      if (t.type === "diff") return !!diffTabsStore.get(t.id);
      if (t.type === "markdown") return !!mdTabsStore.get(t.id);
      return !!editorTabsStore.get(t.id);
    });
  });
  const showTabBar = () => aliveTabs().length > 1;
  const subtabMenu = createContextMenu();
  const [contextTabId, setContextTabId] = createSignal<string | null>(null);

  const openSubtabMenu = (e: MouseEvent, tabId: string) => {
    e.preventDefault();
    e.stopPropagation();
    setContextTabId(tabId);
    subtabMenu.open(e);
  };

  const subtabMenuItems = (): ContextMenuItem[] => {
    const id = contextTabId();
    if (!id) return [];
    const tab = aliveTabs().find((t) => t.id === id);
    if (!tab) return [];
    const items: ContextMenuItem[] = [
      {
        label: "Close Tab",
        action: () => {
          props.onCloseTab(id);
        },
      },
    ];
    if (tab.type === "terminal") {
      const isGlobal = globalWorkspaceStore.isPromoted(id);
      items.push(
        { label: "", separator: true, action: () => {} },
        {
          label: isGlobal ? "Remove from Global Workspace" : "Promote to Global Workspace",
          action: () => globalWorkspaceStore.togglePromote(id),
        },
      );
    }
    return items;
  };

  const handleGroupClick = () => {
    paneLayoutStore.setActiveGroup(props.groupId);
    // Focus the active terminal in this group so keyboard input goes here
    const g = group();
    const activeTab = g?.tabs.find((t) => t.id === g.activeTabId);
    if (activeTab?.type === "terminal") {
      props.onTerminalFocus(activeTab.id);
      terminalsStore.get(activeTab.id)?.ref?.focus();
    }
  };

  /** Handle drop of a pane-tab onto this group (from mini bar, main TabBar, or content area) */
  const handlePaneDrop = (e: DragEvent) => {
    e.preventDefault();
    const data = e.dataTransfer?.getData("application/pane-tab");
    if (!data) return;
    let parsed: { tabId: string; fromGroupId: string | null; type: PaneTabType };
    try {
      parsed = JSON.parse(data);
    } catch {
      appLogger.warn("app", "handlePaneDrop: invalid pane-tab drag payload");
      return;
    }
    const { tabId, fromGroupId, type } = parsed;
    if (fromGroupId === props.groupId) return; // drop onto own group — no-op
    if (fromGroupId) {
      paneLayoutStore.moveTab(fromGroupId, props.groupId, tabId);
    } else {
      // Orphan tab dragged from main TabBar — adopt into this group
      paneLayoutStore.addTab(props.groupId, { id: tabId, type: type ?? "terminal" });
    }
    // Follow the dropped tab: the destination group becomes active so the
    // focus ring (pane-group-active) moves with the user's action instead
    // of staying on the source pane.
    paneLayoutStore.setActiveGroup(props.groupId);
  };

  return (
    <div
      class="pane-group"
      classList={{ "pane-group-active": isActive() }}
      onClick={handleGroupClick}
      onContextMenu={handleGroupClick}
    >
      {/* Mini tab bar — auto-show when 2+ tabs, always accept drops */}
      <Show when={showTabBar()}>
        <div
          class="pane-tab-bar"
          onDragOver={(e) => { e.preventDefault(); e.dataTransfer!.dropEffect = "move"; }}
          onDrop={handlePaneDrop}
        >
          <For each={aliveTabs()}>
            {(tab) => (
              <button
                class={`pane-tab ${tabColorClass(tab)}`}
                classList={{ "pane-tab-active": tab.id === group()?.activeTabId }}
                draggable={true}
                onDragStart={(e) => {
                  markInternalDragStart();
                  e.dataTransfer!.setData("application/pane-tab", JSON.stringify({
                    tabId: tab.id,
                    fromGroupId: props.groupId,
                    type: tab.type,
                  }));
                  e.dataTransfer!.effectAllowed = "move";
                }}
                onDragEnd={() => markInternalDragEnd()}
                onClick={() => {
                  paneLayoutStore.setActiveTab(props.groupId, tab.id);
                  if (tab.type === "terminal") {
                    requestAnimationFrame(() => terminalsStore.get(tab.id)?.ref?.focus());
                  }
                }}
                onContextMenu={(e) => openSubtabMenu(e, tab.id)}
                title={tabTitle(tab)}
              >
                <Show when={globalWorkspaceStore.isActive() && tab.type === "terminal"}>
                  {(() => {
                    const repoPath = repositoriesStore.getRepoPathForTerminal(tab.id);
                    const color = repoPath ? getRepoColor(repoPath) : undefined;
                    const repo = repoPath ? repositoriesStore.state.repositories[repoPath] : null;
                    const name = repo?.displayName ?? "";
                    return (
                      <span class="pane-tab-repo-badge" title={name}>
                        <span class="pane-tab-repo-dot" style={{ background: color || "var(--fg-muted)" }} />
                        <span class="pane-tab-repo-name">{name}</span>
                      </span>
                    );
                  })()}
                </Show>
                <span class="pane-tab-label">{tabTitle(tab)}</span>
                <Show when={tab.type === "terminal" && globalWorkspaceStore.isPromoted(tab.id)}>
                  <span
                    class="pane-tab-globe"
                    title="Remove from Global Workspace"
                    onClick={(e) => {
                      e.stopPropagation();
                      globalWorkspaceStore.unpromote(tab.id);
                    }}
                  >
                    <GlobeIcon size={10} />
                  </span>
                </Show>
                <span
                  class="pane-tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onCloseTab(tab.id);
                  }}
                >
                  ×
                </span>
              </button>
            )}
          </For>
        </div>
      </Show>

      {/* Content area — also a drop target for empty panes.
           Mount ALL tabs but only display the active one — terminals have
           imperative state (xterm/WebGL) that must persist across tab switches. */}
      <div
        class="pane-content"
        onDragOver={(e) => { e.preventDefault(); e.dataTransfer!.dropEffect = "move"; }}
        onDrop={handlePaneDrop}
      >
        <Show when={aliveTabs().length > 0} fallback={
          <PanePlaceholder onNewTerminal={() => props.onNewTerminal?.(props.groupId)} />
        }>
          <For each={aliveTabs()}>
            {(tab) => (
              <div
                class="pane-tab-content"
                classList={{ "pane-tab-content-active": tab.id === group()?.activeTabId }}
              >
                <PaneTabContent
                  tab={tab}
                  onCloseTab={props.onCloseTab}
                  onOpenFilePath={props.onOpenFilePath}
                  onTerminalFocus={props.onTerminalFocus}
                  onCwdChange={props.onCwdChange}
                />
              </div>
            )}
          </For>
        </Show>
      </div>

      <ContextMenu
        items={subtabMenuItems()}
        x={subtabMenu.position().x}
        y={subtabMenu.position().y}
        visible={subtabMenu.visible()}
        onClose={subtabMenu.close}
      />
    </div>
  );
};

// ---- PanePlaceholder: empty pane state ----

const PanePlaceholder: Component<{ onNewTerminal?: () => void }> = (props) => (
  <div class="pane-placeholder" onDblClick={() => props.onNewTerminal?.()}>
    <span class="pane-placeholder-text">Double-click or drop a tab here</span>
  </div>
);

// ---- PaneTabContent: renders the correct component based on tab type ----

const PaneTabContent: Component<{
  tab: PaneTab;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onTerminalFocus: (id: string) => void;
  onCwdChange?: (id: string, cwd: string) => void;
}> = (props) => {
  return (
    <Switch>
      <Match when={props.tab.type === "terminal"}>
        <TerminalPane
          tabId={props.tab.id}
          onFocus={props.onTerminalFocus}
          onOpenFilePath={props.onOpenFilePath}
          onCwdChange={props.onCwdChange}
        />
      </Match>
      <Match when={props.tab.type === "markdown"}>
        <MarkdownPane tabId={props.tab.id} onClose={props.onCloseTab} />
      </Match>
      <Match when={props.tab.type === "diff"}>
        <DiffPane tabId={props.tab.id} onClose={props.onCloseTab} />
      </Match>
      <Match when={props.tab.type === "editor"}>
        <EditorPane tabId={props.tab.id} onClose={props.onCloseTab} />
      </Match>
    </Switch>
  );
};

// ---- Type-specific pane content renderers ----

const TerminalPane: Component<{
  tabId: string;
  onFocus: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onCwdChange?: (id: string, cwd: string) => void;
}> = (props) => {
  const terminal = () => terminalsStore.get(props.tabId);
  const metaHotkeys = createMemo(() => {
    const path = repositoriesStore.getRepoPathForTerminal(props.tabId);
    if (!path) return undefined;
    return repoSettingsStore.getEffective(path)?.terminalMetaHotkeys;
  });

  return (
    <Show when={terminal()}>
      <Terminal
        id={props.tabId}
        cwd={terminal()?.cwd || null}
        onFocus={props.onFocus}
        onSessionCreated={() => {}}
        onOpenFilePath={props.onOpenFilePath}
        metaHotkeys={metaHotkeys()}
        onCwdChange={props.onCwdChange}
      />
    </Show>
  );
};

const MarkdownPane: Component<{ tabId: string; onClose: (id: string) => void }> = (props) => {
  const mdTab = () => mdTabsStore.get(props.tabId);
  return (
    <Show when={mdTab()} keyed>
      {(tab) => <MdTabContent tab={tab} onClose={() => props.onClose(props.tabId)} />}
    </Show>
  );
};

const DiffPane: Component<{ tabId: string; onClose: (id: string) => void }> = (props) => {
  const tab = () => diffTabsStore.get(props.tabId);
  return (
    <Show when={tab()}>
      {(diffTab) => (
        <DiffTab
          tabId={props.tabId}
          repoPath={diffTab().repoPath}
          filePath={diffTab().filePath}
          scope={diffTab().scope}
          untracked={diffTab().untracked}
          onClose={() => props.onClose(props.tabId)}
        />
      )}
    </Show>
  );
};

const EditorPane: Component<{ tabId: string; onClose: (id: string) => void }> = (props) => {
  const tab = () => editorTabsStore.get(props.tabId);
  return (
    <Show when={tab()}>
      {(editTab) => (
        <CodeEditorTab
          id={props.tabId}
          repoPath={editTab().repoPath}
          filePath={editTab().filePath}
          initialLine={editTab().initialLine}
          externalEditable={editTab().externalEditable}
          onClose={() => props.onClose(props.tabId)}
        />
      )}
    </Show>
  );
};

// ---- Helpers ----

/** CSS class suffix for tab-type coloring in mini tab bar */
function tabColorClass(tab: PaneTab): string {
  switch (tab.type) {
    case "terminal": return "";
    case "diff": return "pane-tab-diff";
    case "editor": return "pane-tab-edit";
    case "markdown": {
      const m = mdTabsStore.get(tab.id);
      if (!m) return "pane-tab-panel";
      if (m.type === "file") return "pane-tab-md";
      if (m.type === "pr-diff") return "pane-tab-diff";
      return "pane-tab-panel";
    }
  }
}

function tabTitle(tab: PaneTab): string {
  switch (tab.type) {
    case "terminal": {
      const t = terminalsStore.get(tab.id);
      return t?.name ?? tab.id;
    }
    case "markdown": {
      const m = mdTabsStore.get(tab.id);
      if (!m) return tab.id;
      if (m.type === "claude-usage") return "Claude Usage";
      if (m.type === "plugin-panel") return (m as { title?: string }).title ?? "Plugin";
      if (m.type === "pr-diff") return `PR #${(m as { prNumber: number }).prNumber}`;
      if ("title" in m && m.title) return m.title as string;
      return tab.id;
    }
    case "diff": {
      const d = diffTabsStore.get(tab.id);
      return d?.filePath?.split("/").pop() ?? tab.id;
    }
    case "editor": {
      const e = editorTabsStore.get(tab.id);
      return e?.filePath?.split("/").pop() ?? tab.id;
    }
  }
}
