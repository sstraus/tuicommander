import { Component, Show, For, createSignal, createEffect, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { updaterStore } from "../../stores/updater";
import { useGitHub } from "../../hooks/useGitHub";
import { uiStore } from "../../stores/ui";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { prNotificationsStore, type PrNotificationType } from "../../stores/prNotifications";
import { activityStore } from "../../stores/activityStore";
import { getModifierSymbol } from "../../platform";
import { IdeLauncher } from "../IdeLauncher";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { t } from "../../i18n";
import { cx } from "../../utils";
import type { ActivityItem } from "../../plugins/types";
import type { PrNotification } from "../../stores/prNotifications";
import s from "./Toolbar.module.css";

const NOTIFICATION_LABELS: Record<PrNotificationType, { label: string; icon: string; cls: string }> = {
  merged: { label: "Merged", icon: "\u2714", cls: s.notifMerged },
  closed: { label: "Closed", icon: "\u2716", cls: s.notifClosed },
  blocked: { label: "Conflicts", icon: "\u26A0", cls: s.notifBlocked },
  ci_failed: { label: "CI Failed", icon: "\u2716", cls: s.notifCiFailed },
  changes_requested: { label: "Changes Req.", icon: "\u270E", cls: s.notifChanges },
  ready: { label: "Ready", icon: "\u2713", cls: s.notifReady },
};

// ---------------------------------------------------------------------------
// Last-item shortcut helpers
// ---------------------------------------------------------------------------

type LastItemSource =
  | { kind: "activity"; item: ActivityItem }
  | { kind: "pr"; notif: PrNotification }
  | { kind: "update"; version: string };

/** Find the most recently created item across all notification sources. */
function getLastItemAcrossStores(): LastItemSource | null {
  const activityItem = activityStore.getLastItem();
  const prNotifs = prNotificationsStore.getActive();
  const prLast = prNotifs.length > 0
    ? prNotifs.reduce((latest, n) => n.createdAt >= latest.createdAt ? n : latest)
    : null;

  // Update available is always "newest" — it's the most important notification
  const upd = updaterStore.state;
  if (upd.available && upd.version) {
    return { kind: "update", version: upd.version };
  }

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
  const hasUpdate = () => updaterStore.state.available && !!updaterStore.state.version;
  const totalBadgeCount = () => activeNotifs().length + activeActivityItems().length + (hasUpdate() ? 1 : 0);
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

  /** Color inheritance: repo color > group color > default */
  const activeRepoColor = () => {
    const activeRepoPath = repositoriesStore.state.activeRepoPath;
    if (!activeRepoPath) return undefined;
    return repoSettingsStore.get(activeRepoPath)?.color
      || repositoriesStore.getGroupForRepo(activeRepoPath)?.color
      || undefined;
  };

  const getRepoPath = () => props.repoPath;
  const github = useGitHub(getRepoPath);

  const aheadBehind = () => {
    const gs = github.status();
    if (!gs) return null;
    if (gs.ahead > 0 && gs.behind > 0) return ` ↑${gs.ahead} ↓${gs.behind}`;
    if (gs.ahead > 0) return ` ↑${gs.ahead}`;
    if (gs.behind > 0) return ` ↓${gs.behind}`;
    return null;
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
    if (src.kind === "update") {
      updaterStore.downloadAndInstall();
    } else if (src.kind === "activity") {
      openActivityItem(src.item);
    } else {
      // PR notification: open detail popover
      requestAnimationFrame(() => {
        setPrDetailTarget({ repoPath: src.notif.repoPath, branch: src.notif.branch });
      });
    }
  };

  return (
    <div id="toolbar" class={s.toolbar} data-tauri-drag-region>
      <div class={s.left} data-tauri-drag-region>
        {/* Embossed app name — dark shadow below, lighter highlight above; TUIC slightly brighter */}
        <svg class={s.appName} data-tauri-drag-region viewBox="0 0 110 16" width="110" height="16" aria-label="TUICommander">
          <defs>
            <linearGradient id="toolbar-name-grad" x1="0" y1="0" x2="110" y2="0" gradientUnits="userSpaceOnUse">
              <stop offset="0%" stop-color="#909090" />
              <stop offset="32%" stop-color="#767676" />
              <stop offset="100%" stop-color="#5a5a5a" />
            </linearGradient>
          </defs>
          <text x="0" y="12" fill="#060606" font-size="11" font-weight="700" letter-spacing="0.09em" font-family="system-ui,-apple-system,sans-serif" dx="1" dy="1">TUICommander</text>
          <text x="0" y="12" fill="#3e3e3e" font-size="11" font-weight="700" letter-spacing="0.09em" font-family="system-ui,-apple-system,sans-serif" dx="-0.5" dy="-0.5">TUICommander</text>
          <text x="0" y="12" fill="url(#toolbar-name-grad)" font-size="11" font-weight="700" letter-spacing="0.09em" font-family="system-ui,-apple-system,sans-serif">TUICommander</text>
        </svg>
        <button
          class={s.sidebarToggle}
          onClick={() => uiStore.toggleSidebar()}
          title={uiStore.state.sidebarVisible ? `${t("toolbar.hideSidebar", "Hide Sidebar")} (${getModifierSymbol()}[)` : `${t("toolbar.showSidebar", "Show Sidebar")} (${getModifierSymbol()}[)`}
          style={{ position: "relative" }}
        >
          ◧
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}[</span>
        </button>
      </div>

      <div class={s.center} data-tauri-drag-region>
        <Show when={activeBranchName()}>
          <button
            class={s.branch}
            onClick={(e) => {
              e.stopPropagation();
              props.onBranchClick?.();
            }}
            title={t("toolbar.renameBranch", "Rename branch")}
          >
            <svg class={s.branchIcon} viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z"/></svg>
            <Show when={activeRepoName()}>
              <span class={s.repoName} style={activeRepoColor() ? { color: activeRepoColor() } : undefined}>{activeRepoName()}</span>
              <span class={s.branchSeparator}>/</span>
            </Show>
            <span class={s.branchName}>{activeBranchName()}</span>
            <Show when={aheadBehind()}>
              {(ab) => <span class={s.aheadBehind}>{ab()}</span>}
            </Show>
          </button>
        </Show>
      </div>

      <div class={s.right}>
        {/* Notification group: last-item shortcut + bell */}
        <Show when={hasAnyItems()}>
          <div class={s.notifGroup} ref={notifRef}>
            {/* Last-item shortcut */}
            <Show when={lastItem()}>
              {(src) => {
                const activitySrc = () => src().kind === "activity" ? src() as { kind: "activity"; item: ActivityItem } : null;
                const prSrc = () => src().kind === "pr" ? src() as { kind: "pr"; notif: PrNotification } : null;
                const updateSrc = () => src().kind === "update" ? src() as { kind: "update"; version: string } : null;
                return (
                  <button
                    class={s.lastItemBtn}
                    onClick={handleLastItemClick}
                    title={(() => {
                      const v = src();
                      if (v.kind === "update") return `Update to v${v.version}`;
                      if (v.kind === "activity") return v.item.title;
                      return v.notif.branch;
                    })()}
                  >
                    <Show when={updateSrc()} keyed>
                      {(us) => (
                        <>
                          <svg class={s.lastItemIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M8 16A8 8 0 1 1 8 0a8 8 0 0 1 0 16zm.93-9.412-1 4.705c-.07.34.029.533.304.533.194 0 .487-.07.686-.246l-.088.416c-.287.346-.92.598-1.465.598-.703 0-1.002-.422-.808-1.319l.738-3.468c.064-.293.006-.399-.287-.399l-.008-.078.012-.058h1.916zm.01-2.54a.96.96 0 1 0 0-1.92.96.96 0 0 0 0 1.92z"/></svg>
                          <span class={s.lastItemTitle}>Update v{us.version}</span>
                        </>
                      )}
                    </Show>
                    <Show when={activitySrc()} keyed>
                      {(as) => (
                        <>
                          <span class={s.lastItemIcon} innerHTML={as.item.icon} />
                          <span class={s.lastItemTitle}>{as.item.title}</span>
                        </>
                      )}
                    </Show>
                    <Show when={prSrc()} keyed>
                      {(ps) => (
                        <>
                          <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354Z"/></svg>
                          <span class={s.lastItemTitle}>{ps.notif.branch}</span>
                        </>
                      )}
                    </Show>
                  </button>
                );
              }}
            </Show>

            {/* Bell */}
            <button
              class={s.bell}
              onClick={() => setShowNotifPopover(!showNotifPopover())}
              title={`${totalBadgeCount()} ${t("toolbar.notifications", "notification(s)")}`}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                <path d="M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z"/>
              </svg>
              <span class={s.notifCount}>{totalBadgeCount()}</span>
            </button>
            <Show when={showNotifPopover()}>
              <div class={s.popover}>
                {/* App update section */}
                <Show when={hasUpdate()}>
                  <div class={s.notifHeader}>
                    <span class={s.notifTitle}>{t("toolbar.appUpdate", "APP UPDATE")}</span>
                    <button class={s.dismissAll} onClick={() => updaterStore.dismiss()}>
                      {t("toolbar.dismiss", "Dismiss")}
                    </button>
                  </div>
                  <div
                    class={s.notifItem}
                    onClick={(e) => {
                      e.stopPropagation();
                      setShowNotifPopover(false);
                      updaterStore.downloadAndInstall();
                    }}
                  >
                    <span class={s.notifIcon}>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="#4ec9b0"><path d="M8 16A8 8 0 1 1 8 0a8 8 0 0 1 0 16zm1-11H7v4H5l3 3 3-3H9V5z"/></svg>
                    </span>
                    <div class={s.notifDetails}>
                      <span class={s.notifPr}>
                        {updaterStore.state.downloading
                          ? `${t("statusBar.updating", "Updating")} ${updaterStore.state.progress}%`
                          : `v${updaterStore.state.version} ${t("toolbar.available", "available")}`}
                      </span>
                      <span class={s.notifBranch}>{t("toolbar.clickToUpdate", "Click to update")}</span>
                    </div>
                  </div>
                </Show>

                {/* Plugin activity sections (shown above PR updates) */}
                <For each={activitySections()}>
                  {(section) => {
                    const sectionItems = () => activityStore.getForSection(section.id);
                    return (
                      <Show when={sectionItems().length > 0}>
                        <div class={s.sectionHeader}>
                          <span class={s.sectionLabel}>{section.label}</span>
                          <Show when={section.canDismissAll}>
                            <button
                              class={s.activityDismissAll}
                              onClick={() => activityStore.dismissSection(section.id)}
                            >
                              {t("toolbar.dismissAll", "Dismiss All")}
                            </button>
                          </Show>
                        </div>
                        <For each={sectionItems()}>
                          {(item) => (
                            <div
                              class={s.activityItem}
                              onClick={(e) => {
                                e.stopPropagation();
                                setShowNotifPopover(false);
                                openActivityItem(item);
                              }}
                            >
                              <span class={s.activityItemIcon} innerHTML={item.icon} />
                              <div class={s.activityItemBody}>
                                <span class={s.activityItemTitle}>{item.title}</span>
                                <Show when={item.subtitle}>
                                  <span class={s.activityItemSubtitle}>{item.subtitle}</span>
                                </Show>
                              </div>
                              <Show when={item.dismissible}>
                                <button
                                  class={s.activityItemDismiss}
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

                {/* PR Updates section */}
                <Show when={activeNotifs().length > 0}>
                  <div class={s.notifHeader}>
                    <span class={s.notifTitle}>{t("toolbar.prUpdates", "PR UPDATES")}</span>
                    <button class={s.dismissAll} onClick={() => { prNotificationsStore.dismissAll(); setShowNotifPopover(false); }}>
                      {t("toolbar.dismissAll", "Dismiss All")}
                    </button>
                  </div>
                  <For each={activeNotifs()}>
                    {(notif) => {
                      const info = NOTIFICATION_LABELS[notif.type];
                      return (
                        <div
                          class={cx(s.notifItem, info.cls)}
                          onClick={(e) => {
                            e.stopPropagation();
                            setShowNotifPopover(false);
                            requestAnimationFrame(() => {
                              setPrDetailTarget({ repoPath: notif.repoPath, branch: notif.branch });
                            });
                          }}
                        >
                          <span class={s.notifIcon}>{info.icon}</span>
                          <div class={s.notifDetails}>
                            <span
                              class={s.notifRepo}
                              style={(() => {
                                const color = repoSettingsStore.get(notif.repoPath)?.color
                                  || repositoriesStore.getGroupForRepo(notif.repoPath)?.color;
                                return color ? { color } : undefined;
                              })()}
                            >
                              {repositoriesStore.get(notif.repoPath)?.displayName ?? notif.repoPath.split("/").pop()}
                            </span>
                            <span class={s.notifPr}>#{notif.prNumber} {info.label}</span>
                            <span class={s.notifBranch} title={notif.title}>{notif.branch}</span>
                          </div>
                          <button class={s.notifClose} onClick={(e) => { e.stopPropagation(); prNotificationsStore.dismiss(notif.id); }}>&times;</button>
                        </div>
                      );
                    }}
                  </For>
                </Show>
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
