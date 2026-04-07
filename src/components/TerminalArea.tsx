import { Component, createEffect, createMemo, For, JSX, Show } from "solid-js";
import { Terminal } from "./Terminal";
import { DiffTab } from "./DiffTab";
import { MdTabContent } from "./shared/MdTabContent";
import { CodeEditorTab } from "./CodeEditorPanel";
import { PaneNodeView } from "./PaneTree/PaneTree";
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
import { paneLayoutStore } from "../stores/paneLayout";
import { useFileDrop } from "../hooks/useFileDrop";


export interface TerminalAreaProps {
  onTerminalFocus: (id: string) => void;
  onCloseTab: (id: string) => void;
  onOpenFilePath: (path: string, line?: number, col?: number) => void;
  onContextMenu: (e: MouseEvent) => void;
  onCwdChange?: (id: string, cwd: string) => void;
  onNewTerminal?: (groupId: string) => void;
  children?: JSX.Element;
}

/** Renders suggested follow-up actions for the active terminal. */
const SuggestOverlayContainer: Component = () => {
  const active = () => terminalsStore.getActive();
  const actions = () => active()?.suggestedActions;
  const activeId = () => terminalsStore.state.activeId;
  const dismissed = () => active()?.suggestDismissed;
  return (
    <Show when={actions()?.length && !dismissed()}>
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
              requestAnimationFrame(() => terminalsStore.getActive()?.ref?.focus());
            }}
            onDismiss={() => {
              terminalsStore.dismissSuggestedActions(capturedId);
              requestAnimationFrame(() => terminalsStore.getActive()?.ref?.focus());
            }}
          />
        );
      })()}
    </Show>
  );
};

export const TerminalArea: Component<TerminalAreaProps> = (props) => {
  const { isDragging, attachTo } = useFileDrop();

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
        ref={attachTo}
        onContextMenu={props.onContextMenu}
      >
        {/* Empty state when no tabs are open and no split active */}
        <Show when={!paneLayoutStore.isSplit() && !terminalsStore.state.activeId && !diffTabsStore.state.activeId && !mdTabsStore.state.activeId && !editorTabsStore.state.activeId}>
          <div class="empty-terminal-state">
            <img src={noTuiOpenImg} alt="No TUI Open" class="empty-terminal-icon" />
            <TipOfTheDay />
          </div>
        </Show>

        {/* PaneTree renderer — active when split mode is on */}
        <Show when={paneLayoutStore.isSplit() && paneLayoutStore.getRoot()}>
          {(root) => (
            <PaneNodeView
              node={root()}
              onCloseTab={props.onCloseTab}
              onOpenFilePath={props.onOpenFilePath}
              onTerminalFocus={props.onTerminalFocus}
              onCwdChange={props.onCwdChange}
              onNewTerminal={props.onNewTerminal}
            />
          )}
        </Show>

        {/* Flat rendering — active when NOT in split mode */}
        <Show when={!paneLayoutStore.isSplit()}>
          {/* Terminal panes */}
          <For each={terminalsStore.getIds()}>
            {(id) => {
              const terminal = terminalsStore.get(id);
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
                    detached: isDetached(),
                  }}
                  style={isDetached() ? { display: "none" } : undefined}
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
                  {mdTab && <MdTabContent tab={mdTab} onClose={() => props.onCloseTab(id)} />}
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
        </Show>

        {/* Suggest follow-up actions overlay — inside #terminal-panes for correct centering.
             Timer lives in the overlay: 30s after becoming visible → auto-dismiss.
             Tab switch unmounts the overlay (cancelling the timer); returning remounts it (restarting the timer).
             This way suggestions persist until the user actually sees them. */}
        <Show when={settingsStore.state.suggestFollowups}>
          <SuggestOverlayContainer />
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

      {/* Side panels (must be inside #terminal-container for flex row layout) */}
      {props.children}
    </div>
  );
};
