import { Component, For, Show, createMemo, createSignal, createEffect } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import type { RepositoryState } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { uiStore } from "../../stores/ui";
import { repoSettingsStore } from "../../stores/repoSettings";
import { githubStore } from "../../stores/github";
import type { ContextMenuItem } from "../ContextMenu";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { ParkedReposPopover } from "./ParkedReposPopover";
import { PromptDialog } from "../PromptDialog";
import { t } from "../../i18n";
import { RepoSection } from "./RepoSection";
import { GroupSection } from "./GroupSection";
import { useSidebarDragDrop } from "./useSidebarDragDrop";
import s from "./Sidebar.module.css";

export interface SidebarProps {
  quickSwitcherActive?: boolean;
  creatingWorktreeRepos?: Set<string>;
  onBranchSelect: (repoPath: string, branchName: string) => void;
  onAddTerminal: (repoPath: string, branchName: string) => void;
  onRemoveBranch: (repoPath: string, branchName: string) => void;
  onRenameBranch: (repoPath: string, branchName: string) => void;
  buildAgentMenuItems?: (repoPath: string, branchName: string) => ContextMenuItem[];
  onAddWorktree: (repoPath: string) => void;
  onAddRepo: () => void;
  onRepoSettings: (repoPath: string) => void;
  onRemoveRepo: (repoPath: string) => void;
  onOpenSettings: () => void;
  onOpenHelp?: () => void;
  onBackgroundGit?: (repoPath: string, op: string, args: string[]) => void;
  runningGitOps?: Set<string>;
  onRefreshBranchStats?: () => Promise<void>;
}

const DRAG_CLASSES: Record<string, string> = {
  top: s.dragOverTop,
  bottom: s.dragOverBottom,
  target: s.dragOverTarget,
};

export const Sidebar: Component<SidebarProps> = (props) => {
  const repos = createMemo(() => repositoriesStore.getOrderedRepos());
  const groupedLayout = createMemo(() => repositoriesStore.getGroupedLayout());

  const drag = useSidebarDragDrop();

  // PR detail popover state
  const [prDetailTarget, setPrDetailTarget] = createSignal<{ repoPath: string; branch: string } | null>(null);

  // Parked repos popover state
  const [parkedPopoverVisible, setParkedPopoverVisible] = createSignal(false);
  const parkedCount = createMemo(() => repositoriesStore.getParkedRepos().length);

  // Auto-show PR popover when active branch has PR data
  createEffect(() => {
    if (!settingsStore.state.autoShowPrPopover) return;
    const active = repositoriesStore.getActive();
    if (!active?.activeBranch) {
      setPrDetailTarget(null);
      return;
    }
    const prStatus = githubStore.getPrStatus(active.path, active.activeBranch);
    const prState = prStatus?.state?.toUpperCase();
    if (prStatus && prState !== "CLOSED" && prState !== "MERGED") {
      setPrDetailTarget({ repoPath: active.path, branch: active.activeBranch });
    } else {
      setPrDetailTarget(null);
    }
  });

  // Sync CSS variable so toolbar-left matches sidebar width
  createEffect(() => {
    document.documentElement.style.setProperty("--sidebar-width", `${uiStore.state.sidebarWidth}px`);
  });

  const handleResizeStart = (e: MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = uiStore.state.sidebarWidth;

    const sidebar = document.querySelector<HTMLElement>(`[data-testid="sidebar"]`);
    if (sidebar) sidebar.style.transition = "none";

    let lastWidth = startWidth;

    const onMove = (ev: MouseEvent) => {
      const raw = startWidth + (ev.clientX - startX);
      const clamped = Math.min(500, Math.max(200, raw));
      lastWidth = clamped;
      // Update CSS immediately for smooth visual feedback (no IPC)
      document.documentElement.style.setProperty("--sidebar-width", `${clamped}px`);
    };

    const cleanup = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", cleanup);
      window.removeEventListener("blur", cleanup);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      if (sidebar) sidebar.style.transition = "";
      // Persist final width via store (single IPC call)
      uiStore.setSidebarWidth(lastWidth);
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", cleanup);
    // Safety valve: if mouse released outside window, blur fires
    window.addEventListener("blur", cleanup);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  // Compute the starting shortcut index for each repo (1-based, cumulative across all groups then ungrouped)
  const repoShortcutStarts = createMemo(() => {
    const starts: Record<string, number> = {};
    let counter = 1;
    const layout = groupedLayout();
    // Groups first
    for (const entry of layout.groups) {
      for (const repo of entry.repos) {
        starts[repo.path] = counter;
        counter += Object.keys(repo.branches).length;
      }
    }
    // Then ungrouped
    for (const repo of layout.ungrouped) {
      starts[repo.path] = counter;
      counter += Object.keys(repo.branches).length;
    }
    return starts;
  });

  // Group rename and color change via PromptDialog
  const [renameGroupTarget, setRenameGroupTarget] = createSignal<string | null>(null);
  const [colorGroupTarget, setColorGroupTarget] = createSignal<string | null>(null);

  const handleGroupRename = (groupId: string) => {
    setRenameGroupTarget(groupId);
  };

  const handleGroupColorChange = (groupId: string) => {
    setColorGroupTarget(groupId);
  };

  /** Render a single RepoSection with all its props */
  const renderRepoSection = (repo: RepositoryState) => {
    // Color inheritance: repo color > group color > undefined
    const repoColor = () => repoSettingsStore.get(repo.path)?.color;
    const groupColor = () => repositoriesStore.getGroupForRepo(repo.path)?.color;
    const nameColor = () => repoColor() || groupColor() || undefined;

    return (
      <RepoSection
        repo={repo}
        nameColor={nameColor()}
        isDragging={drag.draggedRepoPath() === repo.path}
        isCreatingWorktree={props.creatingWorktreeRepos?.has(repo.path)}
        dragOverClass={
          drag.dragOverRepoPath() === repo.path && drag.draggedRepoPath() !== repo.path
            ? DRAG_CLASSES[drag.dragOverSide() ?? ""] ?? undefined
            : undefined
        }
        quickSwitcherActive={props.quickSwitcherActive}
        branchShortcutStart={repoShortcutStarts()[repo.path]}
        onBranchSelect={(branch) => props.onBranchSelect(repo.path, branch)}
        onAddTerminal={(branch) => props.onAddTerminal(repo.path, branch)}
        onRemoveBranch={(branch) => props.onRemoveBranch(repo.path, branch)}
        onRenameBranch={(branch) => props.onRenameBranch(repo.path, branch)}
        onShowPrDetail={(branch) => setPrDetailTarget({ repoPath: repo.path, branch })}
        buildAgentMenuItems={props.buildAgentMenuItems ? (branch) => props.buildAgentMenuItems!(repo.path, branch) : undefined}
        onAddWorktree={() => props.onAddWorktree(repo.path)}
        onSettings={() => props.onRepoSettings(repo.path)}
        onRemove={() => props.onRemoveRepo(repo.path)}
        onToggle={() => repositoriesStore.toggleExpanded(repo.path)}
        onToggleCollapsed={() => repositoriesStore.toggleCollapsed(repo.path)}
        onToggleShowAllBranches={async () => {
          repositoriesStore.toggleShowAllBranches(repo.path);
          await props.onRefreshBranchStats?.();
        }}
        onDragStart={(e) => drag.handleRepoDragStart(e, repo.path)}
        onDragOver={(e) => drag.handleRepoDragOver(e, repo.path)}
        onDrop={(e) => drag.handleRepoDrop(e, repo.path)}
        onDragEnd={drag.resetDragState}
      />
    );
  };

  return (
    <aside id="sidebar" class={s.sidebar} data-testid="sidebar">
      {/* Content */}
      <div class={s.content}>
        {/* Repository Section */}
        <div>
          <div class={s.repoList}>
            {/* Grouped repos */}
            <For each={groupedLayout().groups}>
              {(entry) => (
                <GroupSection
                  group={entry.group}
                  repos={entry.repos}
                  quickSwitcherActive={props.quickSwitcherActive}
                  onRename={handleGroupRename}
                  onColorChange={handleGroupColorChange}
                  onDragStart={(e) => drag.handleGroupDragStart(e, entry.group.id)}
                  onDragOver={(e) => drag.handleGroupDragOver(e, entry.group.id)}
                  onDrop={(e) => drag.handleGroupDrop(e, entry.group.id)}
                  onDragEnd={drag.resetDragState}
                  onHeaderDragOver={(e) => drag.handleGroupDragOver(e, entry.group.id)}
                  onHeaderDrop={(e) => drag.handleGroupDrop(e, entry.group.id)}
                  dragOverClass={
                    drag.dragOverGroupId() === entry.group.id && drag.dragPayload()?.type !== "repo"
                      ? DRAG_CLASSES[drag.dragOverGroupSide() ?? ""] ?? undefined
                      : drag.dragOverGroupId() === entry.group.id && drag.dragPayload()?.type === "repo"
                        ? DRAG_CLASSES["target"]
                        : undefined
                  }
                >
                  <For each={entry.repos}>
                    {(repo) => renderRepoSection(repo)}
                  </For>
                </GroupSection>
              )}
            </For>
            {/* Ungrouped repos */}
            <For each={groupedLayout().ungrouped}>
              {(repo) => renderRepoSection(repo)}
            </For>
            <Show when={repos().length === 0}>
              <div class={s.empty}>
                <p>{t("sidebar.noRepositories", "No repositories")}</p>
                <button onClick={props.onAddRepo}>{t("sidebar.addRepository", "Add Repository")}</button>
              </div>
            </Show>
          </div>
        </div>
      </div>

      {/* Git Quick Actions (Story 050) */}
      <Show when={repositoriesStore.getActive()}>
        <div class={s.gitQuickActions}>
          <svg class={s.gitQuickLabel} width="10" height="28" viewBox="0 0 10 28" aria-hidden="true">
            <text
              x="5" y="14"
              transform="rotate(-90 5 14)"
              text-anchor="middle"
              dominant-baseline="central"
              fill="currentColor"
              font-size="8.5"
              font-weight="700"
              letter-spacing="0.12em"
              font-family="system-ui,-apple-system,sans-serif"
            >GIT</text>
          </svg>
          <div class={s.gitQuickBtns}>
            <button
              class={s.gitQuickBtn}
              classList={{ [s.loading]: props.runningGitOps?.has("pull") }}
              disabled={props.runningGitOps?.has("pull")}
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onBackgroundGit?.(repo.path, "pull", ["pull"]);
              }}
              title={t("sidebar.gitPull", "Pull latest changes")}
            >
              <span class={s.gitQuickIcon}>
                {/* arrow-down-to-line */}
                <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M8 2v9M4 8l4 4 4-4M2 14h12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </span>
              {t("sidebar.gitPullLabel", "Pull")}
            </button>
            <button
              class={s.gitQuickBtn}
              classList={{ [s.loading]: props.runningGitOps?.has("push") }}
              disabled={props.runningGitOps?.has("push")}
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onBackgroundGit?.(repo.path, "push", ["push"]);
              }}
              title={t("sidebar.gitPush", "Push commits")}
            >
              <span class={s.gitQuickIcon}>
                {/* arrow-up-from-line */}
                <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M8 14V5M4 8l4-4 4 4M2 2h12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </span>
              {t("sidebar.gitPushLabel", "Push")}
            </button>
            <button
              class={s.gitQuickBtn}
              classList={{ [s.loading]: props.runningGitOps?.has("fetch") }}
              disabled={props.runningGitOps?.has("fetch")}
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onBackgroundGit?.(repo.path, "fetch", ["fetch", "--all"]);
              }}
              title={t("sidebar.gitFetch", "Fetch from all remotes")}
            >
              <span class={s.gitQuickIcon}>
                {/* refresh-cw */}
                <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M1 4s.5-1 3-2.5A7 7 0 0 1 15 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><path d="M15 12s-.5 1-3 2.5A7 7 0 0 1 1 8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><path d="M1 1v3h3M15 15v-3h-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </span>
              {t("sidebar.gitFetchLabel", "Fetch")}
            </button>
            <button
              class={s.gitQuickBtn}
              classList={{ [s.loading]: props.runningGitOps?.has("stash") }}
              disabled={props.runningGitOps?.has("stash")}
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onBackgroundGit?.(repo.path, "stash", ["stash"]);
              }}
              title={t("sidebar.gitStash", "Stash changes")}
            >
              <span class={s.gitQuickIcon}>
                {/* layers/stash */}
                <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M1.5 6 8 9.5 14.5 6M1.5 10 8 13.5 14.5 10M8 2.5 14.5 6 8 9.5 1.5 6 8 2.5Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/></svg>
              </span>
              {t("sidebar.gitStashLabel", "Stash")}
            </button>
          </div>
        </div>
      </Show>

      {/* Footer */}
      <div class={s.footer}>
        <button class={s.addRepo} onClick={props.onAddRepo} title={t("sidebar.addRepository", "Add Repository")}>
          <svg class={s.addRepoIcon} width="14" height="14" viewBox="0 0 16 16" fill="none">
            <path d="M1.5 2A1.5 1.5 0 0 1 3 .5h3.379a1.5 1.5 0 0 1 1.06.44l1.122 1.12H13A1.5 1.5 0 0 1 14.5 3.5v9a1.5 1.5 0 0 1-1.5 1.5H3A1.5 1.5 0 0 1 1.5 12.5V2Z" stroke="currentColor" stroke-width="1.2"/>
          </svg>
          {t("sidebar.addRepository", "Add Repository")}
        </button>
        <div class={s.footerIcons}>
          <Show when={parkedCount() > 0}>
            <button
              class={s.footerAction}
              onClick={() => setParkedPopoverVisible((v) => !v)}
              title={t("sidebar.parkedRepos", "Parked repositories")}
              style={{ position: "relative" }}
            >
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
                <path d="M2 3h12v2H2zM3 5v8h10V5" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
                <path d="M5 8h6" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              </svg>
              <span class={s.parkedBadge}>{parkedCount()}</span>
            </button>
          </Show>
          <button
            class={s.footerAction}
            onClick={props.onOpenHelp}
            title={t("sidebar.help", "Help")}
          >
            <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1.2"/>
              <path d="M6 6.2a2 2 0 0 1 3.9.6c0 1.2-1.9 1.2-1.9 2.2" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              <circle cx="8" cy="11.5" r="0.7" fill="currentColor"/>
            </svg>
          </button>
          <button
            class={s.footerAction}
            onClick={props.onOpenSettings}
            title={t("sidebar.settings", "Settings")}
          >
            <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
              <path d="M6.5 1.5h3l.4 1.8a5 5 0 011.2.7l1.7-.6 1.5 2.6-1.3 1.2a5 5 0 010 1.4l1.3 1.2-1.5 2.6-1.7-.6a5 5 0 01-1.2.7l-.4 1.8h-3l-.4-1.8a5 5 0 01-1.2-.7l-1.7.6-1.5-2.6 1.3-1.2a5 5 0 010-1.4L1.7 5.7l1.5-2.6 1.7.6a5 5 0 011.2-.7z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
              <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.2"/>
            </svg>
          </button>
        </div>
      </div>

      {/* Parked repos popover */}
      <Show when={parkedPopoverVisible()}>
        <ParkedReposPopover
          onClose={() => setParkedPopoverVisible(false)}
          onUnpark={(repoPath) => {
            repositoriesStore.setPark(repoPath, false);
            setParkedPopoverVisible(false);
            repositoriesStore.setActive(repoPath);
            const repo = repositoriesStore.get(repoPath);
            const branch = repo?.activeBranch || Object.keys(repo?.branches ?? {})[0];
            if (branch) props.onBranchSelect(repoPath, branch);
          }}
        />
      </Show>

      {/* PR detail popover (triggered from PrStateBadge click) */}
      <Show when={prDetailTarget()}>
        {(target) => (
          <PrDetailPopover
            repoPath={target().repoPath}
            branch={target().branch}
            onClose={() => setPrDetailTarget(null)}
          />
        )}
      </Show>

      {/* Group rename dialog */}
      <PromptDialog
        visible={renameGroupTarget() !== null}
        title="Rename Group"
        placeholder="New group name"
        confirmLabel="Rename"
        onClose={() => setRenameGroupTarget(null)}
        onConfirm={(name) => {
          const groupId = renameGroupTarget();
          if (groupId) repositoriesStore.renameGroup(groupId, name);
          setRenameGroupTarget(null);
        }}
      />

      {/* Group color dialog */}
      <PromptDialog
        visible={colorGroupTarget() !== null}
        title="Group Color"
        placeholder="#ff6b6b"
        confirmLabel="Apply"
        onClose={() => setColorGroupTarget(null)}
        onConfirm={(color) => {
          const groupId = colorGroupTarget();
          if (groupId) repositoriesStore.setGroupColor(groupId, color);
          setColorGroupTarget(null);
        }}
      />

      {/* Drag handle for resizing */}
      <div class={s.resizeHandle} onMouseDown={handleResizeStart} />
    </aside>
  );
};

export default Sidebar;
