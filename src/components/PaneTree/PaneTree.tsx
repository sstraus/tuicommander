import { Component, For, Show, Switch, Match, createMemo } from "solid-js";
import { paneLayoutStore, type PaneNode, type PaneBranch, type PaneLeaf, type PaneTab, MIN_PANE_RATIO } from "../../stores/paneLayout";
import { Terminal } from "../Terminal";
import { DiffTab } from "../DiffTab";
import { PrDiffTab } from "../PrDiffTab";
import { MarkdownTab } from "../MarkdownTab";
import { PluginPanel } from "../PluginPanel";
import { HtmlPreviewTab } from "../HtmlPreviewTab";
import { ClaudeUsageDashboard } from "../ClaudeUsageDashboard";
import { CodeEditorTab } from "../CodeEditorPanel";
import { terminalsStore } from "../../stores/terminals";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import "./PaneTree.css";

// ---- PaneNodeView: recursive tree renderer ----

export const PaneNodeView: Component<{
  node: PaneNode;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onTerminalFocus: (id: string) => void;
  onCwdChange?: (id: string, cwd: string) => void;
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
        />
      </Match>
      <Match when={props.node.type === "leaf"}>
        <PaneGroupView
          groupId={(props.node as PaneLeaf).id}
          onCloseTab={props.onCloseTab}
          onOpenFilePath={props.onOpenFilePath}
          onTerminalFocus={props.onTerminalFocus}
          onCwdChange={props.onCwdChange}
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
}> = (props) => {
  const isVertical = () => props.branch.direction === "vertical";

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
              style={{ flex: `${(props.branch.ratios[i()] ?? 0.5) * 100} 1 0%` }}
            >
              <PaneNodeView
                node={child}
                onCloseTab={props.onCloseTab}
                onOpenFilePath={props.onOpenFilePath}
                onTerminalFocus={props.onTerminalFocus}
                onCwdChange={props.onCwdChange}
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
}> = (props) => {
  const group = () => paneLayoutStore.state.groups[props.groupId];
  const isActive = () => paneLayoutStore.state.activeGroupId === props.groupId;
  const activeTab = () => {
    const g = group();
    if (!g || !g.activeTabId) return null;
    return g.tabs.find(t => t.id === g.activeTabId) ?? null;
  };
  const showTabBar = () => {
    const g = group();
    return g && g.tabs.length > 1;
  };

  const handleGroupClick = () => {
    paneLayoutStore.setActiveGroup(props.groupId);
  };

  return (
    <div
      class="pane-group"
      classList={{ "pane-group-active": isActive() }}
      onClick={handleGroupClick}
    >
      {/* Mini tab bar — auto-show only when 2+ tabs */}
      <Show when={showTabBar()}>
        <div class="pane-tab-bar">
          <For each={group()?.tabs ?? []}>
            {(tab) => (
              <button
                class="pane-tab"
                classList={{ "pane-tab-active": tab.id === group()?.activeTabId }}
                onClick={() => paneLayoutStore.setActiveTab(props.groupId, tab.id)}
                title={tabTitle(tab)}
              >
                <span class="pane-tab-label">{tabTitle(tab)}</span>
                <span
                  class="pane-tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onCloseTab(tab.id);
                    paneLayoutStore.removeTab(props.groupId, tab.id);
                  }}
                >
                  ×
                </span>
              </button>
            )}
          </For>
        </div>
      </Show>

      {/* Content area */}
      <div class="pane-content">
        <Show when={activeTab()} fallback={<PanePlaceholder />}>
          {(tab) => (
            <PaneTabContent
              tab={tab()}
              onCloseTab={props.onCloseTab}
              onOpenFilePath={props.onOpenFilePath}
              onTerminalFocus={props.onTerminalFocus}
              onCwdChange={props.onCwdChange}
            />
          )}
        </Show>
      </div>
    </div>
  );
};

// ---- PanePlaceholder: empty pane state ----

const PanePlaceholder: Component = () => (
  <div class="pane-placeholder">
    <span class="pane-placeholder-text">Open or drop a tab here</span>
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
  // Use ternary chain (like TerminalArea) for proper type narrowing
  return (
    <Show when={mdTab()} keyed>
      {(tab) =>
        tab.type === "claude-usage" ? (
          <ClaudeUsageDashboard />
        ) : tab.type === "plugin-panel" ? (
          <PluginPanel tab={tab} onClose={() => props.onClose(props.tabId)} />
        ) : tab.type === "pr-diff" ? (
          <PrDiffTab prNumber={tab.prNumber} prTitle={tab.prTitle} diff={tab.diff} />
        ) : tab.type === "html-preview" ? (
          <HtmlPreviewTab tab={tab} onClose={() => props.onClose(props.tabId)} />
        ) : (
          <MarkdownTab tab={tab} onClose={() => props.onClose(props.tabId)} />
        )
      }
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
          onClose={() => props.onClose(props.tabId)}
        />
      )}
    </Show>
  );
};

// ---- Helpers ----

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
