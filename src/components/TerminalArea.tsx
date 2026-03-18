import { Component, createEffect, createMemo, For, JSX, Show } from "solid-js";
import { Terminal } from "./Terminal";
import { DiffTab } from "./DiffTab";
import { PrDiffTab } from "./PrDiffTab";
import { MarkdownTab } from "./MarkdownTab";
import { PluginPanel } from "./PluginPanel";
import { ClaudeUsageDashboard } from "./ClaudeUsageDashboard";
import { CodeEditorTab } from "./CodeEditorPanel";
import SuggestOverlay from "./SuggestOverlay/SuggestOverlay";
import { rpc } from "../transport";
import noTuiOpenImg from "../assets/no-tui-open.png";
import TipOfTheDay from "./TipOfTheDay/TipOfTheDay";
import { terminalsStore } from "../stores/terminals";
import { settingsStore } from "../stores/settings";
import { repositoriesStore } from "../stores/repositories";
import { repoSettingsStore } from "../stores/repoSettings";
import { diffTabsStore } from "../stores/diffTabs";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { useFileDrop } from "../hooks/useFileDrop";


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
  onCwdChange?: (id: string, cwd: string) => void;
  children?: JSX.Element;
}

export const TerminalArea: Component<TerminalAreaProps> = (props) => {
  const { isDragging } = useFileDrop();

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
      <div
        id="terminal-panes"
        classList={{
          "split-vertical": terminalsStore.state.layout.direction === "vertical",
          "split-horizontal": terminalsStore.state.layout.direction === "horizontal",
        }}
        onContextMenu={props.onContextMenu}
      >
        {/* Empty state when no tabs are open */}
        <Show when={!terminalsStore.state.activeId && !diffTabsStore.state.activeId && !mdTabsStore.state.activeId && !editorTabsStore.state.activeId}>
          <div class="empty-terminal-state">
            <img src={noTuiOpenImg} alt="No TUI Open" class="empty-terminal-icon" />
            <TipOfTheDay />
          </div>
        </Show>

        {/* Terminal panes */}
        <For each={terminalsStore.getIds()}>
          {(id) => {
            const terminal = terminalsStore.get(id);
            const isSplitPane = () => terminalsStore.state.layout.direction !== "none" && terminalsStore.state.layout.panes.includes(id);
            const isActivePaneInSplit = () => {
              const layout = terminalsStore.state.layout;
              return layout.direction !== "none" && layout.panes[layout.activePaneIndex] === id;
            };
            const paneIndex = () => terminalsStore.state.layout.panes.indexOf(id);
            const paneRatio = () => terminalsStore.state.layout.ratios[paneIndex()] ?? 0;

            const isDetached = () => terminalsStore.isDetached(id);

            const metaHotkeys = createMemo(() => {
              const path = repositoriesStore.getRepoPathForTerminal(id);
              if (!path) return undefined;
              return repoSettingsStore.getEffective(path)?.terminalMetaHotkeys;
            });

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
                  flex: `${paneRatio() * 100} 1 0%`,
                  order: paneIndex() * 2,
                } : undefined}
              >
                <Terminal
                  id={id}
                  cwd={terminal?.cwd || null}
                  onFocus={props.onTerminalFocus}
                  onSessionCreated={() => {}}
                  onOpenFilePath={props.onOpenFilePath}
                  metaHotkeys={metaHotkeys()}
                  onCwdChange={props.onCwdChange}
                />
              </div>
            );
          }}
        </For>

        {/* Resize handles between split panes — one handle per adjacent pair */}
        <For each={Array.from({ length: Math.max(0, terminalsStore.state.layout.panes.length - 1) }, (_, i) => i)}>
          {(handleIndex) => (
            <div
              class="split-resize-handle"
              classList={{
                vertical: terminalsStore.state.layout.direction === "vertical",
                horizontal: terminalsStore.state.layout.direction === "horizontal",
              }}
              style={{ order: handleIndex * 2 + 1 }}
              onMouseDown={(startEvent) => {
                startEvent.preventDefault();
                const container = document.getElementById("terminal-panes");
                if (!container) return;

                const isVertical = terminalsStore.state.layout.direction === "vertical";
                const rect = container.getBoundingClientRect();
                const capturedIndex = handleIndex;

                let rafPending = false;
                const onMouseMove = (e: MouseEvent) => {
                  if (rafPending) return;
                  rafPending = true;
                  requestAnimationFrame(() => {
                    rafPending = false;
                    const fraction = isVertical
                      ? (e.clientX - rect.left) / rect.width
                      : (e.clientY - rect.top) / rect.height;
                    terminalsStore.setHandleRatio(capturedIndex, fraction);
                  });
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
          )}
        </For>

        {/* Diff tabs */}
        <For each={diffTabsStore.getIds()}>
          {(id) => {
            const diffTab = diffTabsStore.get(id);
            return (
              <div
                class="terminal-pane diff-pane"
                classList={{ active: diffTabsStore.state.activeId === id }}
                onContextMenu={(e) => e.stopPropagation()}
              >
                {diffTab && (
                  <DiffTab
                    tabId={id}
                    repoPath={diffTab.repoPath}
                    filePath={diffTab.filePath}
                    scope={diffTab.scope}
                    untracked={diffTab.untracked}
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
                onContextMenu={(e) => e.stopPropagation()}
              >
                {mdTab && mdTab.type === "claude-usage" ? (
                  <ClaudeUsageDashboard />
                ) : mdTab && mdTab.type === "plugin-panel" ? (
                  <PluginPanel
                    tab={mdTab}
                    onClose={() => props.onCloseTab(id)}
                  />
                ) : mdTab && mdTab.type === "pr-diff" ? (
                  <PrDiffTab
                    prNumber={mdTab.prNumber}
                    prTitle={mdTab.prTitle}
                    diff={mdTab.diff}
                  />
                ) : mdTab ? (
                  <MarkdownTab
                    tab={mdTab}
                    onClose={() => props.onCloseTab(id)}
                  />
                ) : null}
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
                onContextMenu={(e) => e.stopPropagation()}
              >
                {editTab && (
                  <CodeEditorTab
                    id={id}
                    repoPath={editTab.repoPath}
                    filePath={editTab.filePath}
                    initialLine={editTab.initialLine}
                    onClose={() => props.onCloseTab(id)}
                  />
                )}
              </div>
            );
          }}
        </For>

        {/* Suggest follow-up actions overlay — inside #terminal-panes for correct centering.
             Timer lives in the overlay: 30s after becoming visible → auto-dismiss.
             Tab switch unmounts the overlay (cancelling the timer); returning remounts it (restarting the timer).
             This way suggestions persist until the user actually sees them. */}
        <Show when={settingsStore.state.suggestFollowups}>
          {(() => {
            const active = () => terminalsStore.getActive();
            const actions = () => active()?.suggestedActions;
            const activeId = () => terminalsStore.state.activeId;
            return (
              <Show when={actions()?.length}>
                {(() => {
                  // Capture terminal ID at render time so dismiss always targets the right terminal
                  const capturedId = activeId()!;
                  const capturedSid = active()?.sessionId ?? null;
                  return (
                    <SuggestOverlay
                      items={actions()!}
                      onSelect={async (text) => {
                        terminalsStore.dismissSuggestedActions(capturedId);
                        if (capturedSid) {
                          await rpc("write_pty", { sessionId: capturedSid, data: text });
                          await rpc("write_pty", { sessionId: capturedSid, data: "\r" });
                        }
                      }}
                      onDismiss={() => {
                        terminalsStore.dismissSuggestedActions(capturedId);
                      }}
                    />
                  );
                })()}
              </Show>
            );
          })()}
        </Show>

        {/* Drop overlay for external file drag & drop */}
        <Show when={isDragging()}>
          <div class="file-drop-overlay">
            <div class="file-drop-overlay-content">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
                <line x1="12" y1="18" x2="12" y2="12" />
                <polyline points="9 15 12 12 15 15" />
              </svg>
              <span>Drop files to open</span>
            </div>
          </div>
        </Show>
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
                  if (lgTerm?.ref) {
                    if (props.lazygitCmd) {
                      lgTerm.ref.write(`${props.lazygitCmd}\r`);
                    }
                    lgTerm.ref.focus();
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
