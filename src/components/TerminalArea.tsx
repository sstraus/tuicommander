import { Component, createEffect, For, JSX, Show } from "solid-js";
import { Terminal } from "./Terminal";
import { DiffTab } from "./DiffTab";
import { MarkdownTab } from "./MarkdownTab";
import { CodeEditorTab } from "./CodeEditorPanel";
import noTuiOpenImg from "../assets/no-tui-open.png";
import { terminalsStore } from "../stores/terminals";
import { repositoriesStore } from "../stores/repositories";
import { repoSettingsStore } from "../stores/repoSettings";
import { diffTabsStore } from "../stores/diffTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";

export interface TerminalAreaProps {
  onTerminalFocus: (id: string) => void;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onContextMenu: (e: MouseEvent) => void;
  /** Lazygit split pane */
  lazygitPaneVisible: boolean;
  lazygitTermId: string | null;
  lazygitFloating: boolean;
  lazygitRepoPath: string | null;
  lazygitCmd: string | null;
  onLazygitFloat: () => void;
  onLazygitClose: () => void;
  children?: JSX.Element;
}

export const TerminalArea: Component<TerminalAreaProps> = (props) => {
  /** Resolve which repo a terminal belongs to */
  const repoPathForTerminal = (termId: string) => {
    for (const [path, repo] of Object.entries(repositoriesStore.state.repositories)) {
      for (const branch of Object.values(repo.branches)) {
        if (branch.terminals.includes(termId)) return path;
      }
    }
    return null;
  };

  /** Get terminal meta hotkeys setting for a terminal's repo */
  const terminalMetaHotkeys = (termId: string) => {
    const path = repoPathForTerminal(termId);
    if (!path) return undefined;
    return repoSettingsStore.getEffective(path)?.terminalMetaHotkeys;
  };

  // When a non-terminal tab becomes active, release focus from xterm's textarea.
  // On macOS WKWebView, wheel events follow focus rather than cursor position,
  // so xterm retains focus (even inside display:none) and captures wheel events.
  // A simple blur() releases it, allowing normal cursor-position wheel routing.
  createEffect(() => {
    const hasFocus =
      mdTabsStore.state.activeId !== null ||
      diffTabsStore.state.activeId !== null ||
      editorTabsStore.state.activeId !== null;
    if (hasFocus) {
      (document.activeElement as HTMLElement | null)?.blur();
    }
  });

  return (
    <div id="terminal-container">
      {/* Empty state when no tabs are open */}
      <Show when={!terminalsStore.state.activeId && !diffTabsStore.state.activeId && !mdTabsStore.state.activeId && !editorTabsStore.state.activeId}>
        <div class="empty-terminal-state">
          <img src={noTuiOpenImg} alt="No TUI Open" class="empty-terminal-icon" />
        </div>
      </Show>

      <div
        id="terminal-panes"
        classList={{
          "split-vertical": terminalsStore.state.layout.direction === "vertical",
          "split-horizontal": terminalsStore.state.layout.direction === "horizontal",
        }}
        onContextMenu={props.onContextMenu}
      >
        {/* Terminal panes */}
        <For each={terminalsStore.getIds()}>
          {(id) => {
            const terminal = terminalsStore.get(id);
            const isSplitPane = () => terminalsStore.state.layout.panes.includes(id);
            const isActivePaneInSplit = () => {
              const layout = terminalsStore.state.layout;
              return layout.direction !== "none" && layout.panes[layout.activePaneIndex] === id;
            };
            const paneIndex = () => terminalsStore.state.layout.panes.indexOf(id);
            const splitRatio = () => terminalsStore.state.layout.ratio;

            const isDetached = () => terminalsStore.isDetached(id);

            return (
              <div
                class="terminal-pane"
                classList={{
                  active: terminalsStore.state.activeId === id && !isDetached(),
                  "split-pane": isSplitPane(),
                  "split-pane-active": isActivePaneInSplit(),
                  detached: isDetached(),
                }}
                style={isDetached() ? { display: "none" } : isSplitPane() ? {
                  flex: paneIndex() === 0
                    ? `${splitRatio() * 100} 1 0%`
                    : `${(1 - splitRatio()) * 100} 1 0%`,
                  order: paneIndex() === 0 ? 0 : 2,
                } : undefined}
              >
                <Terminal
                  id={id}
                  cwd={terminal?.cwd || null}
                  onFocus={props.onTerminalFocus}
                  onSessionCreated={() => {}}
                  onOpenFilePath={props.onOpenFilePath}
                  metaHotkeys={terminalMetaHotkeys(id)}
                />
              </div>
            );
          }}
        </For>

        {/* Resize handle between split panes */}
        <Show when={terminalsStore.state.layout.direction !== "none"}>
          <div
            class="split-resize-handle"
            classList={{
              vertical: terminalsStore.state.layout.direction === "vertical",
              horizontal: terminalsStore.state.layout.direction === "horizontal",
            }}
            style={{ order: 1 }}
            onMouseDown={(startEvent) => {
              startEvent.preventDefault();
              const container = document.getElementById("terminal-panes");
              if (!container) return;

              const isVertical = terminalsStore.state.layout.direction === "vertical";
              const rect = container.getBoundingClientRect();

              const onMouseMove = (e: MouseEvent) => {
                const ratio = isVertical
                  ? (e.clientX - rect.left) / rect.width
                  : (e.clientY - rect.top) / rect.height;
                terminalsStore.setSplitRatio(ratio);
              };

              const onMouseUp = () => {
                document.removeEventListener("mousemove", onMouseMove);
                document.removeEventListener("mouseup", onMouseUp);
                document.body.style.cursor = "";
                document.body.style.userSelect = "";
                for (const paneId of terminalsStore.state.layout.panes) {
                  terminalsStore.get(paneId)?.ref?.fit();
                }
              };

              document.body.style.cursor = isVertical ? "col-resize" : "row-resize";
              document.body.style.userSelect = "none";
              document.addEventListener("mousemove", onMouseMove);
              document.addEventListener("mouseup", onMouseUp);
            }}
          />
        </Show>

        {/* Diff tabs */}
        <For each={diffTabsStore.getIds()}>
          {(id) => {
            const diffTab = diffTabsStore.get(id);
            return (
              <div
                class="terminal-pane diff-pane"
                classList={{ active: diffTabsStore.state.activeId === id }}
              >
                {diffTab && (
                  <DiffTab
                    repoPath={diffTab.repoPath}
                    filePath={diffTab.filePath}
                    scope={diffTab.scope}
                    onClose={() => props.onCloseTab(id)}
                  />
                )}
              </div>
            );
          }}
        </For>

        {/* Markdown tabs */}
        <For each={mdTabsStore.getIds()}>
          {(id) => {
            const mdTab = mdTabsStore.get(id);
            return (
              <div
                class="terminal-pane md-pane"
                classList={{ active: mdTabsStore.state.activeId === id }}
              >
                {mdTab && (
                  <MarkdownTab
                    tab={mdTab}
                    onClose={() => props.onCloseTab(id)}
                  />
                )}
              </div>
            );
          }}
        </For>

        {/* Editor tabs */}
        <For each={editorTabsStore.getIds()}>
          {(id) => {
            const editTab = editorTabsStore.get(id);
            return (
              <div
                class="terminal-pane edit-pane"
                classList={{ active: editorTabsStore.state.activeId === id }}
              >
                {editTab && (
                  <CodeEditorTab
                    id={id}
                    repoPath={editTab.repoPath}
                    filePath={editTab.filePath}
                    onClose={() => props.onCloseTab(id)}
                  />
                )}
              </div>
            );
          }}
        </For>
      </div>

      {/* Lazygit split pane (Story 047) */}
      <Show when={props.lazygitPaneVisible && props.lazygitTermId && !props.lazygitFloating}>
        <div class="lazygit-pane">
          <div class="lazygit-pane-header">
            <span class="lazygit-pane-title">
              <span>⎇</span> lazygit
            </span>
            <div style={{ display: "flex", gap: "4px" }}>
              <button
                class="lazygit-pane-close"
                onClick={props.onLazygitFloat}
                title="Float (detach)"
              >
                ⇱
              </button>
              <button class="lazygit-pane-close" onClick={props.onLazygitClose}>
                &times;
              </button>
            </div>
          </div>
          <div class="lazygit-pane-content">
            <Terminal
              id={props.lazygitTermId!}
              cwd={props.lazygitRepoPath}
              alwaysVisible
              onSessionCreated={(id, _sid) => {
                requestAnimationFrame(() => {
                  const lgTerm = terminalsStore.get(id);
                  if (lgTerm?.ref && props.lazygitCmd) {
                    lgTerm.ref.write(`${props.lazygitCmd}\r`);
                  }
                });
              }}
            />
          </div>
        </div>
      </Show>

      {/* Side panels (must be inside #terminal-container for flex row layout) */}
      {props.children}
    </div>
  );
};
