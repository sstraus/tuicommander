import { Component, Show, For, createSignal, createEffect, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { prNotificationsStore, type PrNotificationType } from "../../stores/prNotifications";
import { getModifierSymbol } from "../../platform";
import { IdeLauncher } from "../IdeLauncher";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";

const NOTIFICATION_LABELS: Record<PrNotificationType, { label: string; icon: string; cls: string }> = {
  merged: { label: "Merged", icon: "\u2714", cls: "notif-merged" },
  closed: { label: "Closed", icon: "\u2716", cls: "notif-closed" },
  blocked: { label: "Conflicts", icon: "\u26A0", cls: "notif-blocked" },
  ci_failed: { label: "CI Failed", icon: "\u2716", cls: "notif-ci-failed" },
  changes_requested: { label: "Changes Req.", icon: "\u270E", cls: "notif-changes" },
  ready: { label: "Ready", icon: "\u2713", cls: "notif-ready" },
};

/** Extract a short display name from a plan file path */
function planDisplayName(path: string): string {
  const parts = path.split("/");
  const file = parts[parts.length - 1] || path;
  // Remove .md/.mdx extension
  return file.replace(/\.mdx?$/, "");
}

export interface ToolbarProps {
  repoPath?: string;
  runCommand?: string;
  quickSwitcherActive?: boolean;
  onBranchClick?: () => void;
  onRun?: (shiftKey: boolean) => void;
  onOpenPlan?: (path: string) => void;
}

export const Toolbar: Component<ToolbarProps> = (props) => {
  const [showNotifPopover, setShowNotifPopover] = createSignal(false);
  const [prDetailTarget, setPrDetailTarget] = createSignal<{ repoPath: string; branch: string } | null>(null);
  let notifRef: HTMLDivElement | undefined;

  // Close popover on outside click
  createEffect(() => {
    if (!showNotifPopover()) return;
    const handler = (e: MouseEvent) => {
      if (notifRef && !notifRef.contains(e.target as Node)) {
        setShowNotifPopover(false);
      }
    };
    document.addEventListener("mousedown", handler);
    onCleanup(() => document.removeEventListener("mousedown", handler));
  });

  const activeNotifs = () => prNotificationsStore.getActive();
  const activeBranch = () => {
    const activeRepoPath = repositoriesStore.state.activeRepoPath;
    if (!activeRepoPath) return null;
    const repo = repositoriesStore.state.repositories[activeRepoPath];
    if (!repo?.activeBranch) return null;
    return repo.branches[repo.activeBranch] || null;
  };

  const activeBranchName = () => activeBranch()?.name || null;

  const activeRepoName = () => {
    const activeRepoPath = repositoriesStore.state.activeRepoPath;
    if (!activeRepoPath) return null;
    const repo = repositoriesStore.state.repositories[activeRepoPath];
    return repo?.displayName || null;
  };

  // Use the branch's worktree path (falls back to repo path)
  const launchPath = () => activeBranch()?.worktreePath || props.repoPath;

  // Absolute path of the focused file in editor or MD tab (if any)
  const focusedFilePath = (): string | undefined => {
    const editTab = editorTabsStore.getActive();
    if (editTab) {
      // filePath is relative to repoPath — join them
      return `${editTab.repoPath}/${editTab.filePath}`;
    }
    const mdTab = mdTabsStore.getActive();
    if (mdTab) {
      return `${mdTab.repoPath}/${mdTab.filePath}`;
    }
    return undefined;
  };

  return (
    <div id="toolbar" data-tauri-drag-region>
      <div class="toolbar-left" data-tauri-drag-region>
        <button
          class="toolbar-sidebar-toggle"
          onClick={() => uiStore.toggleSidebar()}
          title={uiStore.state.sidebarVisible ? `Hide Sidebar (${getModifierSymbol()}[)` : `Show Sidebar (${getModifierSymbol()}[)`}
          style={{ position: "relative" }}
        >
          ◧
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}[</span>
        </button>
      </div>

      <div class="toolbar-center" data-tauri-drag-region>
        <Show when={activeBranchName()}>
          <button
            class="toolbar-branch"
            onClick={(e) => {
              e.stopPropagation();
              props.onBranchClick?.();
            }}
            title="Rename branch"
          >
            <span class="toolbar-branch-icon">Y</span>
            <Show when={activeRepoName()}>
              <span class="toolbar-repo-name">{activeRepoName()}</span>
              <span class="toolbar-branch-separator">/</span>
            </Show>
            <span class="toolbar-branch-name">{activeBranchName()}</span>
          </button>
        </Show>
      </div>

      <div class="toolbar-right">
        <Show when={uiStore.state.planFilePath}>
          <div class="plan-button-group">
            <button
              class="plan-button"
              onClick={() => props.onOpenPlan?.(uiStore.state.planFilePath!)}
              title={uiStore.state.planFilePath!}
            >
              <svg class="plan-button-icon" viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm-1 7V3.5L18.5 9H13zM8 13h8v2H8v-2zm0 4h8v2H8v-2zm0-8h4v2H8V9z"/></svg>
              <span class="plan-button-name">{planDisplayName(uiStore.state.planFilePath!)}</span>
            </button>
            <button
              class="plan-button-close"
              onClick={(e) => { e.stopPropagation(); uiStore.clearPlanFile(); }}
              title="Dismiss"
            >
              &times;
            </button>
          </div>
        </Show>
        <Show when={activeNotifs().length > 0}>
          <div class="pr-notif-wrapper" ref={notifRef}>
            <button
              class="pr-notif-bell"
              onClick={() => setShowNotifPopover(!showNotifPopover())}
              title={`${activeNotifs().length} PR update(s)`}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                <path d="M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z"/>
              </svg>
              <span class="pr-notif-count">{activeNotifs().length}</span>
            </button>
            <Show when={showNotifPopover()}>
              <div class="pr-notif-popover">
                <div class="pr-notif-header">
                  <span class="pr-notif-title">PR Updates</span>
                  <button class="pr-notif-dismiss-all" onClick={() => { prNotificationsStore.dismissAll(); setShowNotifPopover(false); }}>
                    Dismiss All
                  </button>
                </div>
                <For each={activeNotifs()}>
                  {(notif) => {
                    const info = NOTIFICATION_LABELS[notif.type];
                    return (
                      <div
                        class={`pr-notif-item ${info.cls}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          setShowNotifPopover(false);
                          // Defer to next frame so the overlay doesn't catch this click
                          requestAnimationFrame(() => {
                            setPrDetailTarget({ repoPath: notif.repoPath, branch: notif.branch });
                          });
                        }}
                      >
                        <span class="pr-notif-icon">{info.icon}</span>
                        <div class="pr-notif-details">
                          <span class="pr-notif-pr">#{notif.prNumber} {info.label}</span>
                          <span class="pr-notif-branch" title={notif.title}>{notif.branch}</span>
                        </div>
                        <button class="pr-notif-close" onClick={(e) => { e.stopPropagation(); prNotificationsStore.dismiss(notif.id); }}>&times;</button>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </div>
        </Show>
        <IdeLauncher repoPath={launchPath()} focusedFilePath={focusedFilePath()} runCommand={props.runCommand} onRun={props.onRun} />
      </div>

      {/* PR detail popover triggered from notification click */}
      <Show when={prDetailTarget()}>
        <PrDetailPopover
          repoPath={prDetailTarget()!.repoPath}
          branch={prDetailTarget()!.branch}
          anchor="top"
          onClose={() => setPrDetailTarget(null)}
        />
      </Show>
    </div>
  );
};

export default Toolbar;
