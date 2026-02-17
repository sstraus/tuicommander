import { Component, For, Show, createMemo, createSignal, createEffect } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import type { RepositoryState, BranchState } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { githubStore } from "../../stores/github";
import { settingsStore } from "../../stores/settings";
import { uiStore } from "../../stores/ui";
import { CiRing } from "../ui/CiRing";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import { getModifierSymbol } from "../../platform";
import { compareBranches } from "../../utils/branchSort";

export interface SidebarProps {
  quickSwitcherActive?: boolean;
  onBranchSelect: (repoPath: string, branchName: string) => void;
  onAddTerminal: (repoPath: string, branchName: string) => void;
  onRemoveBranch: (repoPath: string, branchName: string) => void;
  onRenameBranch: (repoPath: string, branchName: string) => void;
  onAddWorktree: (repoPath: string) => void;
  onAddRepo: () => void;
  onRepoSettings: (repoPath: string) => void;
  onRemoveRepo: (repoPath: string) => void;
  onOpenSettings: () => void;
  onOpenHelp?: () => void;
  onGitCommand?: (command: string) => void;
}

/** Branch icon component — shows ? when any terminal in the branch awaits input */
const BranchIcon: Component<{ isMain: boolean; hasQuestion?: boolean }> = (props) => (
  <span class={`branch-icon ${props.hasQuestion ? "question" : props.isMain ? "main" : "feature"}`}>
    {props.hasQuestion ? "?" : props.isMain ? "★" : "Y"}
  </span>
);

/** Stats badge component - shows additions/deletions */
const StatsBadge: Component<{ additions: number; deletions: number }> = (props) => (
  <Show when={props.additions > 0 || props.deletions > 0}>
    <div class="branch-stats">
      <span class="stat-add">+{props.additions}</span>
      <span class="stat-del">-{props.deletions}</span>
    </div>
  </Show>
);

/** PR badge component - shows PR number with state-aware styling */
const PrBadgeSidebar: Component<{ prNumber: number; state?: string; isDraft?: boolean }> = (props) => {
  const stateClass = () => {
    if (props.isDraft) return " draft";
    const s = props.state?.toLowerCase();
    if (s === "merged") return " merged";
    if (s === "closed") return " closed";
    return "";
  };

  return (
    <span class={`branch-pr-badge${stateClass()}`} title={`PR #${props.prNumber}`}>
      #{props.prNumber}
    </span>
  );
};

/** Repository section component */
const RepoSection: Component<{
  repo: RepositoryState;
  quickSwitcherActive?: boolean;
  branchShortcutStart: number;
  onBranchSelect: (branchName: string) => void;
  onAddTerminal: (branchName: string) => void;
  onRemoveBranch: (branchName: string) => void;
  onRenameBranch: (branchName: string) => void;
  onShowPrDetail: (branchName: string) => void;
  onAddWorktree: () => void;
  onSettings: () => void;
  onRemove: () => void;
  onToggle: () => void;
  onToggleCollapsed: () => void;
}> = (props) => {
  const repoMenu = createContextMenu();

  const branches = createMemo(() => Object.values(props.repo.branches));
  const sortedBranches = createMemo(() => {
    return [...branches()].sort((a, b) =>
      compareBranches(
        a, b,
        githubStore.getPrStatus(props.repo.path, a.name),
        githubStore.getPrStatus(props.repo.path, b.name),
      ),
    );
  });

  const repoMenuItems = (): ContextMenuItem[] => [
    { label: "Repo Settings", action: () => props.onSettings() },
    { label: "Remove Repository", action: () => props.onRemove() },
  ];

  const handleMenuToggle = (e: MouseEvent) => {
    e.stopPropagation();
    if (repoMenu.visible()) {
      repoMenu.close();
    } else {
      // Position below the button
      const btn = e.currentTarget as HTMLElement;
      const rect = btn.getBoundingClientRect();
      repoMenu.open({ preventDefault: () => {}, clientX: rect.right - 160, clientY: rect.bottom + 4 } as MouseEvent);
    }
  };

  return (
    <div class={`repo-section ${props.repo.collapsed ? "collapsed" : ""}`}>
      {/* Repo header */}
      <div class="repo-header" onClick={props.onToggle}>
        <Show when={props.repo.collapsed}>
          <span
            class="repo-initials"
            onClick={(e) => {
              e.stopPropagation();
              props.onToggleCollapsed();
            }}
            title="Click to expand"
          >
            {props.repo.initials}
          </span>
        </Show>
        <Show when={!props.repo.collapsed}>
          <span class="repo-name">{props.repo.displayName}</span>
          <div class="repo-actions">
              <button
                class="repo-action-btn"
                onClick={handleMenuToggle}
                title="Repository options"
              >
                ⋯
              </button>
            <button
              class="repo-action-btn add-btn"
              onClick={(e) => {
                e.stopPropagation();
                props.onAddWorktree();
              }}
              title="Add worktree"
            >
              +
            </button>
          </div>
          <span class={`repo-chevron ${props.repo.expanded ? "expanded" : ""}`}>{"\u203A"}</span>
        </Show>
      </div>

      {/* Branches - force expanded in quick switcher mode */}
      <Show when={(props.repo.expanded && !props.repo.collapsed) || props.quickSwitcherActive}>
        <div class="repo-branches">
          <For each={sortedBranches()}>
            {(branch, index) => (
              <BranchItem
                branch={branch}
                repoPath={props.repo.path}
                isActive={repositoriesStore.state.activeRepoPath === props.repo.path && props.repo.activeBranch === branch.name}
                shortcutIndex={props.quickSwitcherActive ? props.branchShortcutStart + index() : undefined}
                onSelect={() => props.onBranchSelect(branch.name)}
                onAddTerminal={() => props.onAddTerminal(branch.name)}
                onRemove={() => props.onRemoveBranch(branch.name)}
                onRename={() => props.onRenameBranch(branch.name)}
                onShowPrDetail={() => props.onShowPrDetail(branch.name)}
              />
            )}
          </For>
          <Show when={sortedBranches().length === 0}>
            <div class="repo-empty">No branches loaded</div>
          </Show>
        </div>
      </Show>
      <ContextMenu
        items={repoMenuItems()}
        x={repoMenu.position().x}
        y={repoMenu.position().y}
        visible={repoMenu.visible()}
        onClose={repoMenu.close}
      />
    </div>
  );
};

/** Branch item component */
const BranchItem: Component<{
  branch: BranchState;
  repoPath: string;
  isActive: boolean;
  shortcutIndex?: number;
  onSelect: () => void;
  onAddTerminal: () => void;
  onRemove: () => void;
  onRename: () => void;
  onShowPrDetail: () => void;
}> = (props) => {
  const ctxMenu = createContextMenu();

  const hasActivity = () =>
    props.branch.terminals.some((id) => terminalsStore.get(id)?.activity);

  const hasIdle = () =>
    !hasActivity() && props.branch.terminals.some((id) => terminalsStore.get(id)?.shellState === "idle");

  const hasQuestion = () =>
    props.branch.terminals.some((id) => terminalsStore.get(id)?.awaitingInput != null);

  const handleDoubleClick = (e: MouseEvent) => {
    e.stopPropagation();
    props.onRename();
  };

  const handleCopyPath = async () => {
    const path = props.branch.worktreePath;
    if (path) {
      await navigator.clipboard.writeText(path);
    }
  };

  const contextMenuItems = (): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      { label: "Copy Path", action: handleCopyPath, disabled: !props.branch.worktreePath },
      { label: "Add Terminal", action: props.onAddTerminal },
      { label: "Rename Branch", action: props.onRename },
    ];
    if (!props.branch.isMain && props.branch.worktreePath) {
      items.push({ label: "Delete Worktree", action: props.onRemove, separator: true });
    }
    return items;
  };

  return (
    <div
      class={`branch-item ${props.isActive ? "active" : ""} ${hasActivity() ? "has-activity" : ""} ${hasIdle() ? "shell-idle" : ""}`}
      onClick={props.onSelect}
      onContextMenu={ctxMenu.open}
    >
      <BranchIcon isMain={props.branch.isMain} hasQuestion={hasQuestion()} />
      <div class="branch-content">
        <span
          class="branch-name"
          onDblClick={handleDoubleClick}
          title="Double-click to rename"
        >
          {props.branch.name}
        </span>
      </div>
      <Show when={githubStore.getPrStatus(props.repoPath, props.branch.name)}>
        {(prData) => (
          <span onClick={(e) => { e.stopPropagation(); props.onShowPrDetail(); }}>
            <PrBadgeSidebar
              prNumber={prData().number}
              state={prData().state}
              isDraft={prData().is_draft}
            />
          </span>
        )}
      </Show>
      <Show when={githubStore.getCheckSummary(props.repoPath, props.branch.name)}>
        {(summary) => (
          <CiRing
            passed={summary().passed}
            failed={summary().failed}
            pending={summary().pending}
            onClick={() => props.onShowPrDetail()}
          />
        )}
      </Show>
      <StatsBadge additions={props.branch.additions} deletions={props.branch.deletions} />
      <Show when={props.shortcutIndex !== undefined} fallback={
        <div class="branch-actions">
          <button
            class="branch-add-btn"
            onClick={(e) => {
              e.stopPropagation();
              props.onAddTerminal();
            }}
            title="Add terminal"
          >
            +
          </button>
          <Show when={!props.branch.isMain && props.branch.worktreePath}>
            <button
              class="branch-remove-btn"
              onClick={(e) => {
                e.stopPropagation();
                props.onRemove();
              }}
              title="Remove worktree"
            >
              ×
            </button>
          </Show>
        </div>
      }>
        <span class="branch-shortcut">{getModifierSymbol()}^{props.shortcutIndex}</span>
      </Show>
      <ContextMenu
        items={contextMenuItems()}
        x={ctxMenu.position().x}
        y={ctxMenu.position().y}
        visible={ctxMenu.visible()}
        onClose={ctxMenu.close}
      />
    </div>
  );
};

export const Sidebar: Component<SidebarProps> = (props) => {
  const repos = createMemo(() => Object.values(repositoriesStore.state.repositories));

  // PR detail popover state
  const [prDetailTarget, setPrDetailTarget] = createSignal<{ repoPath: string; branch: string } | null>(null);

  // Auto-show PR popover when active branch has PR data
  createEffect(() => {
    if (!settingsStore.state.autoShowPrPopover) return;
    const active = repositoriesStore.getActive();
    if (!active?.activeBranch) {
      setPrDetailTarget(null);
      return;
    }
    const prStatus = githubStore.getPrStatus(active.path, active.activeBranch);
    if (prStatus) {
      setPrDetailTarget({ repoPath: active.path, branch: active.activeBranch });
    } else {
      setPrDetailTarget(null);
    }
  });

  // Sync CSS variable so toolbar-left matches sidebar width
  document.documentElement.style.setProperty("--sidebar-width", `${uiStore.state.sidebarWidth}px`);

  const handleResizeStart = (e: MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = uiStore.state.sidebarWidth;

    const sidebar = document.getElementById("sidebar");
    if (sidebar) sidebar.style.transition = "none";

    let lastWidth = startWidth;

    const onMove = (ev: MouseEvent) => {
      const raw = startWidth + (ev.clientX - startX);
      const clamped = Math.min(500, Math.max(200, raw));
      lastWidth = clamped;
      // Update CSS immediately for smooth visual feedback (no IPC)
      document.documentElement.style.setProperty("--sidebar-width", `${clamped}px`);
    };

    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      if (sidebar) sidebar.style.transition = "";
      // Persist final width via store (single IPC call)
      uiStore.setSidebarWidth(lastWidth);
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  // Compute the starting shortcut index for each repo (1-based, cumulative across repos)
  const repoShortcutStarts = createMemo(() => {
    const starts: Record<string, number> = {};
    let counter = 1;
    for (const repo of repos()) {
      starts[repo.path] = counter;
      counter += Object.keys(repo.branches).length;
    }
    return starts;
  });

  return (
    <aside id="sidebar">
      {/* Content */}
      <div class="sidebar-content">
        {/* Repository Section */}
        <div class="section">
          <div id="repo-list">
            <For each={repos()}>
              {(repo) => (
                <RepoSection
                  repo={repo}
                  quickSwitcherActive={props.quickSwitcherActive}
                  branchShortcutStart={repoShortcutStarts()[repo.path]}
                  onBranchSelect={(branch) => props.onBranchSelect(repo.path, branch)}
                  onAddTerminal={(branch) => props.onAddTerminal(repo.path, branch)}
                  onRemoveBranch={(branch) => props.onRemoveBranch(repo.path, branch)}
                  onRenameBranch={(branch) => props.onRenameBranch(repo.path, branch)}
                  onShowPrDetail={(branch) => setPrDetailTarget({ repoPath: repo.path, branch })}
                  onAddWorktree={() => props.onAddWorktree(repo.path)}
                  onSettings={() => props.onRepoSettings(repo.path)}
                  onRemove={() => props.onRemoveRepo(repo.path)}
                  onToggle={() => repositoriesStore.toggleExpanded(repo.path)}
                  onToggleCollapsed={() => repositoriesStore.toggleCollapsed(repo.path)}
                />
              )}
            </For>
            <Show when={repos().length === 0}>
              <div class="sidebar-empty">
                <p>No repositories</p>
                <button onClick={props.onAddRepo}>Add Repository</button>
              </div>
            </Show>
          </div>
        </div>
      </div>

      {/* Git Quick Actions (Story 050) */}
      <Show when={repositoriesStore.getActive()}>
        <div class="git-quick-actions">
          <div class="git-quick-actions-title">Git</div>
          <div class="git-quick-actions-btns">
            <button
              class="git-quick-btn"
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${repo.path} && git pull`);
              }}
              title="Pull latest changes"
            >
              <span class="git-quick-icon">{"\u2193"}</span> Pull
            </button>
            <button
              class="git-quick-btn"
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${repo.path} && git push`);
              }}
              title="Push commits"
            >
              <span class="git-quick-icon">{"\u2191"}</span> Push
            </button>
            <button
              class="git-quick-btn"
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${repo.path} && git fetch --all`);
              }}
              title="Fetch from all remotes"
            >
              <span class="git-quick-icon">{"\u27F3"}</span> Fetch
            </button>
            <button
              class="git-quick-btn"
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${repo.path} && git stash`);
              }}
              title="Stash changes"
            >
              <span class="git-quick-icon">{"\u2261"}</span> Stash
            </button>
          </div>
        </div>
      </Show>

      {/* Footer */}
      <div class="sidebar-footer">
        <button class="sidebar-add-repo" onClick={props.onAddRepo} title="Add Repository">
          <svg class="sidebar-add-repo-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M1.5 2A1.5 1.5 0 0 1 3 .5h3.379a1.5 1.5 0 0 1 1.06.44l1.122 1.12H13A1.5 1.5 0 0 1 14.5 3.5v9a1.5 1.5 0 0 1-1.5 1.5H3A1.5 1.5 0 0 1 1.5 12.5V2Z" stroke="currentColor" stroke-width="1.2"/>
          </svg>
          Add Repository
        </button>
        <div class="sidebar-footer-icons">
          <button
            class="sidebar-footer-action"
            onClick={props.onOpenHelp}
            title="Help"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1.2"/>
              <path d="M6 6.2a2 2 0 0 1 3.9.6c0 1.2-1.9 1.2-1.9 2.2" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              <circle cx="8" cy="11.5" r="0.7" fill="currentColor"/>
            </svg>
          </button>
          <button
            class="sidebar-footer-action"
            title="Notifications"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path d="M8 1.5L1.5 13h13L8 1.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
              <path d="M8 6v3.5M8 11.5v.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            </svg>
          </button>
          <button
            class="sidebar-footer-action"
            title="Tasks"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <rect x="2" y="2" width="12" height="12" rx="2" stroke="currentColor" stroke-width="1.2"/>
              <path d="M5 8h6M5 5.5h6M5 10.5h4" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            </svg>
          </button>
          <button
            class="sidebar-footer-action"
            onClick={props.onOpenSettings}
            title="Settings"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path d="M6.5 1.5h3l.4 1.8a5 5 0 011.2.7l1.7-.6 1.5 2.6-1.3 1.2a5 5 0 010 1.4l1.3 1.2-1.5 2.6-1.7-.6a5 5 0 01-1.2.7l-.4 1.8h-3l-.4-1.8a5 5 0 01-1.2-.7l-1.7.6-1.5-2.6 1.3-1.2a5 5 0 010-1.4L1.7 5.7l1.5-2.6 1.7.6a5 5 0 011.2-.7z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
              <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.2"/>
            </svg>
          </button>
        </div>
      </div>

      {/* PR detail popover (triggered from CiRing or PrBadge clicks) */}
      <Show when={prDetailTarget()}>
        {(target) => (
          <PrDetailPopover
            repoPath={target().repoPath}
            branch={target().branch}
            onClose={() => setPrDetailTarget(null)}
          />
        )}
      </Show>

      {/* Drag handle for resizing */}
      <div class="sidebar-resize-handle" onMouseDown={handleResizeStart} />
    </aside>
  );
};

export default Sidebar;
