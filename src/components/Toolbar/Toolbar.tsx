import { Component, Show, For, createSignal, createEffect, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { uiStore } from "../../stores/ui";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { prNotificationsStore, type PrNotificationType } from "../../stores/prNotifications";
import { activityStore } from "../../stores/activityStore";
import { getModifierSymbol } from "../../platform";
import { IdeLauncher } from "../IdeLauncher";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import type { ActivityItem } from "../../plugins/types";
import type { PrNotification } from "../../stores/prNotifications";

const NOTIFICATION_LABELS: Record<PrNotificationType, { label: string; icon: string; cls: string }> = {
  merged: { label: "Merged", icon: "\u2714", cls: "notif-merged" },
  closed: { label: "Closed", icon: "\u2716", cls: "notif-closed" },
  blocked: { label: "Conflicts", icon: "\u26A0", cls: "notif-blocked" },
  ci_failed: { label: "CI Failed", icon: "\u2716", cls: "notif-ci-failed" },
  changes_requested: { label: "Changes Req.", icon: "\u270E", cls: "notif-changes" },
  ready: { label: "Ready", icon: "\u2713", cls: "notif-ready" },
};

// ---------------------------------------------------------------------------
// Last-item shortcut helpers
// ---------------------------------------------------------------------------

type LastItemSource =
  | { kind: "activity"; item: ActivityItem }
  | { kind: "pr"; notif: PrNotification };

/** Find the most recently created item across both stores. */
function getLastItemAcrossStores(): LastItemSource | null {
  const activityItem = activityStore.getLastItem();
  const prNotifs = prNotificationsStore.getActive();
  const prLast = prNotifs.length > 0
    ? prNotifs.reduce((latest, n) => n.createdAt >= latest.createdAt ? n : latest)
    : null;

  if (!activityItem && !prLast) return null;
  if (!prLast) return { kind: "activity", item: activityItem! };
  if (!activityItem) return { kind: "pr", notif: prLast };
  return activityItem.createdAt >= prLast.createdAt
    ? { kind: "activity", item: activityItem }
    : { kind: "pr", notif: prLast };
}

export interface ToolbarProps {
  repoPath?: string;
  runCommand?: string;
  quickSwitcherActive?: boolean;
  onBranchClick?: () => void;
  onRun?: (shiftKey: boolean) => void;
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
  const activeActivityItems = () => activityStore.getActive();
  const activitySections = () => activityStore.getSections();
  const totalBadgeCount = () => activeNotifs().length + activeActivityItems().length;
  const hasAnyItems = () => totalBadgeCount() > 0;
  const lastItem = () => getLastItemAcrossStores();

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

  const launchPath = () => activeBranch()?.worktreePath || props.repoPath;

  const focusedFilePath = (): string | undefined => {
    const editTab = editorTabsStore.getActive();
    if (editTab) {
      return `${editTab.repoPath}/${editTab.filePath}`;
    }
    const mdTab = mdTabsStore.getActive();
    if (mdTab?.type === "file") {
      return `${mdTab.repoPath}/${mdTab.filePath}`;
    }
    return undefined;
  };

  /** Open an activity item: virtual content tab or direct action */
  const openActivityItem = (item: ActivityItem) => {
    if (item.contentUri) {
      mdTabsStore.addVirtual(item.title, item.contentUri);
      uiStore.setMarkdownPanelVisible(true);
    } else if (item.onClick) {
      item.onClick();
    }
  };

  /** Execute the last-item shortcut action */
  const handleLastItemClick = () => {
    const src = lastItem();
    if (!src) return;
    if (src.kind === "activity") {
      openActivityItem(src.item);
    } else {
      // PR notification: open detail popover
      requestAnimationFrame(() => {
        setPrDetailTarget({ repoPath: src.notif.repoPath, branch: src.notif.branch });
      });
    }
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
            <svg class="toolbar-branch-icon" viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z"/></svg>
            <Show when={activeRepoName()}>
              <span class="toolbar-repo-name">{activeRepoName()}</span>
              <span class="toolbar-branch-separator">/</span>
            </Show>
            <span class="toolbar-branch-name">{activeBranchName()}</span>
          </button>
        </Show>
      </div>

      <div class="toolbar-right">
        {/* Last-item shortcut: shows the most recently added item from any source */}
        <Show when={lastItem()}>
          {(src) => {
            const activitySrc = () => src().kind === "activity" ? src() as { kind: "activity"; item: ActivityItem } : null;
            const prSrc = () => src().kind === "pr" ? src() as { kind: "pr"; notif: PrNotification } : null;
            return (
              <button
                class="activity-last-item-btn"
                onClick={handleLastItemClick}
                title={(() => { const s = src(); return s.kind === "activity" ? s.item.title : s.notif.branch; })()}
              >
                <Show when={activitySrc()} keyed>
                  {(s) => (
                    <>
                      <span class="activity-last-item-icon" innerHTML={s.item.icon} />
                      <span class="activity-last-item-title">{s.item.title}</span>
                    </>
                  )}
                </Show>
                <Show when={prSrc()} keyed>
                  {(s) => (
                    <>
                      <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354Z"/></svg>
                      <span class="activity-last-item-title">{s.notif.branch}</span>
                    </>
                  )}
                </Show>
              </button>
            );
          }}
        </Show>

        {/* Bell: aggregates PR notifications + activity store items */}
        <Show when={hasAnyItems()}>
          <div class="pr-notif-wrapper" ref={notifRef}>
            <button
              class="pr-notif-bell"
              onClick={() => setShowNotifPopover(!showNotifPopover())}
              title={`${totalBadgeCount()} notification(s)`}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                <path d="M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z"/>
              </svg>
              <span class="pr-notif-count">{totalBadgeCount()}</span>
            </button>
            <Show when={showNotifPopover()}>
              <div class="pr-notif-popover">
                {/* PR Updates section (native, always first) */}
                <Show when={activeNotifs().length > 0}>
                  <div class="pr-notif-header">
                    <span class="pr-notif-title">PR UPDATES</span>
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
                            requestAnimationFrame(() => {
                              setPrDetailTarget({ repoPath: notif.repoPath, branch: notif.branch });
                            });
                          }}
                        >
                          <span class="pr-notif-icon">{info.icon}</span>
                          <div class="pr-notif-details">
                            <span
                              class="pr-notif-repo"
                              style={(() => {
                                const color = repoSettingsStore.get(notif.repoPath)?.color
                                  || repositoriesStore.getGroupForRepo(notif.repoPath)?.color;
                                return color ? { color } : undefined;
                              })()}
                            >
                              {repositoriesStore.get(notif.repoPath)?.displayName ?? notif.repoPath.split("/").pop()}
                            </span>
                            <span class="pr-notif-pr">#{notif.prNumber} {info.label}</span>
                            <span class="pr-notif-branch" title={notif.title}>{notif.branch}</span>
                          </div>
                          <button class="pr-notif-close" onClick={(e) => { e.stopPropagation(); prNotificationsStore.dismiss(notif.id); }}>&times;</button>
                        </div>
                      );
                    }}
                  </For>
                </Show>

                {/* Plugin activity sections */}
                <For each={activitySections()}>
                  {(section) => {
                    const sectionItems = () => activityStore.getForSection(section.id);
                    return (
                      <Show when={sectionItems().length > 0}>
                        <div class="activity-section-header">
                          <span class="activity-section-label">{section.label}</span>
                          <Show when={section.canDismissAll}>
                            <button
                              class="activity-dismiss-all"
                              onClick={() => activityStore.dismissSection(section.id)}
                            >
                              Dismiss All
                            </button>
                          </Show>
                        </div>
                        <For each={sectionItems()}>
                          {(item) => (
                            <div
                              class="activity-item"
                              onClick={(e) => {
                                e.stopPropagation();
                                setShowNotifPopover(false);
                                openActivityItem(item);
                              }}
                            >
                              <span class="activity-item-icon" innerHTML={item.icon} />
                              <div class="activity-item-body">
                                <span class="activity-item-title">{item.title}</span>
                                <Show when={item.subtitle}>
                                  <span class="activity-item-subtitle">{item.subtitle}</span>
                                </Show>
                              </div>
                              <Show when={item.dismissible}>
                                <button
                                  class="activity-item-dismiss"
                                  onClick={(e) => { e.stopPropagation(); activityStore.dismissItem(item.id); }}
                                >
                                  &times;
                                </button>
                              </Show>
                            </div>
                          )}
                        </For>
                      </Show>
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
        {(target) => (
          <PrDetailPopover
            repoPath={target().repoPath}
            branch={target().branch}
            anchor="top"
            onClose={() => setPrDetailTarget(null)}
          />
        )}
      </Show>
    </div>
  );
};

export default Toolbar;
