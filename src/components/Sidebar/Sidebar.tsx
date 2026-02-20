import { Component, For, Show, createMemo, createSignal, createEffect } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import type { RepositoryState, BranchState, RepoGroup } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { githubStore } from "../../stores/github";
import { settingsStore } from "../../stores/settings";
import { uiStore } from "../../stores/ui";
import { repoSettingsStore } from "../../stores/repoSettings";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import { getModifierSymbol } from "../../platform";
import { compareBranches } from "../../utils/branchSort";

export interface SidebarProps {
  quickSwitcherActive?: boolean;
  creatingWorktreeRepos?: Set<string>;
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

/** PR state badge — single badge replacing both old PR number badge and CI ring.
 *  Shows the most critical state as short text with color/animation. */
const PrStateBadge: Component<{
  prNumber: number;
  state?: string;
  isDraft?: boolean;
  mergeable?: string;
  reviewDecision?: string;
  ciPassed?: number;
  ciFailed?: number;
  ciPending?: number;
}> = (props) => {
  const badge = (): { label: string; cls: string } => {
    // Terminal states
    if (props.isDraft) return { label: "Draft", cls: "draft" };
    const s = props.state?.toLowerCase();
    if (s === "merged") return { label: "Merged", cls: "merged" };
    if (s === "closed") return { label: "Closed", cls: "closed" };
    // Action-required states (priority order)
    if (props.mergeable === "CONFLICTING") return { label: "Conflicts", cls: "conflict" };
    if ((props.ciFailed ?? 0) > 0) return { label: "CI Failed", cls: "ci-failed" };
    if (props.reviewDecision === "CHANGES_REQUESTED") return { label: "Changes Req.", cls: "changes-requested" };
    if (props.reviewDecision === "REVIEW_REQUIRED") return { label: "Review Req.", cls: "review-required" };
    if ((props.ciPending ?? 0) > 0) return { label: "CI Running", cls: "ci-pending" };
    if (props.mergeable === "MERGEABLE" && props.reviewDecision === "APPROVED") return { label: "Ready", cls: "ready" };
    return { label: `#${props.prNumber}`, cls: "open" };
  };

  return (
    <span class={`branch-pr-badge ${badge().cls}`} title={`PR #${props.prNumber}`}>
      {badge().label}
    </span>
  );
};

/** Repository section component */
const RepoSection: Component<{
  repo: RepositoryState;
  nameColor?: string;
  isDragging?: boolean;
  dragOverClass?: string;
  isCreatingWorktree?: boolean;
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
  onDragStart: (e: DragEvent) => void;
  onDragOver: (e: DragEvent) => void;
  onDrop: (e: DragEvent) => void;
  onDragEnd: () => void;
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

  const repoMenuItems = (): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      { label: "Repo Settings", action: () => props.onSettings() },
    ];

    // "Move to Group" submenu — always available (includes "New Group...")
    const layout = repositoriesStore.getGroupedLayout();
    const currentGroup = repositoriesStore.getGroupForRepo(props.repo.path);
    const children: ContextMenuItem[] = layout.groups
      .filter((entry) => entry.group.id !== currentGroup?.id)
      .map((entry) => ({
        label: entry.group.name,
        action: () => repositoriesStore.addRepoToGroup(props.repo.path, entry.group.id),
      }));
    if (currentGroup) {
      children.push({
        label: "Ungrouped",
        action: () => repositoriesStore.removeRepoFromGroup(props.repo.path),
      });
    }
    children.push({
      separator: children.length > 0,
      label: "New Group\u2026",
      action: () => {
        const name = window.prompt("Group name:");
        if (!name?.trim()) return;
        const groupId = repositoriesStore.createGroup(name.trim());
        if (groupId) {
          repositoriesStore.addRepoToGroup(props.repo.path, groupId);
        }
      },
    });
    items.push({ label: "Move to Group", action: () => {}, children });

    items.push({ label: "Remove Repository", action: () => props.onRemove() });
    return items;
  };

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
    <div
      class={`repo-section ${props.repo.collapsed ? "collapsed" : ""} ${props.isDragging ? "dragging" : ""} ${props.dragOverClass || ""}`}
      draggable={true}
      onDragStart={(e) => { e.stopPropagation(); props.onDragStart(e); }}
      onDragOver={(e) => { e.stopPropagation(); props.onDragOver(e); }}
      onDrop={(e) => { e.stopPropagation(); props.onDrop(e); }}
      onDragEnd={props.onDragEnd}
    >
      {/* Repo header */}
      <div class="repo-header" onClick={props.onToggle} onContextMenu={repoMenu.open}>
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
          <span class="repo-name" style={props.nameColor ? { color: props.nameColor } : undefined}>{props.repo.displayName}</span>
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
              disabled={props.isCreatingWorktree}
              onClick={(e) => {
                e.stopPropagation();
                props.onAddWorktree();
              }}
              title={props.isCreatingWorktree ? "Creating worktree…" : "Add worktree"}
            >
              {props.isCreatingWorktree ? "…" : "+"}
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
                canRemove={sortedBranches().length > 1}
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
  canRemove: boolean;
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
    if (!props.branch.isMain && props.branch.worktreePath && props.canRemove) {
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
          title={props.branch.name}
        >
          {props.branch.name}
        </span>
      </div>
      <Show when={githubStore.getPrStatus(props.repoPath, props.branch.name)}>
        {(prData) => {
          const checks = () => githubStore.getCheckSummary(props.repoPath, props.branch.name);
          return (
            <span onClick={(e) => { e.stopPropagation(); props.onShowPrDetail(); }}>
              <PrStateBadge
                prNumber={prData().number}
                state={prData().state}
                isDraft={prData().is_draft}
                mergeable={prData().mergeable}
                reviewDecision={prData().review_decision}
                ciPassed={checks()?.passed}
                ciFailed={checks()?.failed}
                ciPending={checks()?.pending}
              />
            </span>
          );
        }}
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
          <Show when={!props.branch.isMain && props.branch.worktreePath && props.canRemove}>
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

/** Group section component — accordion header with collapsible repo list */
const GroupSection: Component<{
  group: RepoGroup;
  repos: RepositoryState[];
  quickSwitcherActive?: boolean;
  onRename: (groupId: string) => void;
  onColorChange: (groupId: string) => void;
  onDragStart?: (e: DragEvent) => void;
  onDragOver?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
  onDragEnd?: () => void;
  onHeaderDragOver?: (e: DragEvent) => void;
  onHeaderDrop?: (e: DragEvent) => void;
  dragOverClass?: string;
  children: any;
}> = (props) => {
  const groupMenu = createContextMenu();

  const groupMenuItems = (): ContextMenuItem[] => [
    { label: "Rename Group", action: () => props.onRename(props.group.id) },
    { label: "Change Color", action: () => props.onColorChange(props.group.id) },
    { label: "Delete Group", action: () => repositoriesStore.deleteGroup(props.group.id) },
  ];

  return (
    <div
      class={`group-section ${props.dragOverClass || ""}`}
      draggable={true}
      onDragStart={props.onDragStart}
      onDragOver={props.onDragOver}
      onDrop={props.onDrop}
      onDragEnd={props.onDragEnd}
    >
      <div
        class="group-header"
        onClick={() => repositoriesStore.toggleGroupCollapsed(props.group.id)}
        onContextMenu={groupMenu.open}
        onDragOver={(e: DragEvent) => { e.stopPropagation(); props.onHeaderDragOver?.(e); }}
        onDrop={(e: DragEvent) => { e.stopPropagation(); props.onHeaderDrop?.(e); }}
      >
        <Show when={props.group.color}>
          <span class="group-color-dot" style={{ background: props.group.color }} />
        </Show>
        <span class="group-name">{props.group.name}</span>
        <span class="group-count">{props.repos.length}</span>
        <span class={`group-chevron ${props.group.collapsed ? "" : "expanded"}`}>{"\u203A"}</span>
      </div>
      <Show when={!props.group.collapsed || props.quickSwitcherActive}>
        <div class="group-repos">
          <Show when={props.repos.length === 0}>
            <div class="group-empty-hint">Drag repos here</div>
          </Show>
          {props.children}
        </div>
      </Show>
      <ContextMenu
        items={groupMenuItems()}
        x={groupMenu.position().x}
        y={groupMenu.position().y}
        visible={groupMenu.visible()}
        onClose={groupMenu.close}
      />
    </div>
  );
};

export const Sidebar: Component<SidebarProps> = (props) => {
  const repos = createMemo(() => repositoriesStore.getOrderedRepos());
  const groupedLayout = createMemo(() => repositoriesStore.getGroupedLayout());

  // --- Typed drag-and-drop system ---
  type DragPayload =
    | { type: "repo"; path: string; fromGroupId: string | null }
    | { type: "group"; groupId: string };

  const [dragPayload, setDragPayload] = createSignal<DragPayload | null>(null);
  const [dragOverRepoPath, setDragOverRepoPath] = createSignal<string | null>(null);
  const [dragOverSide, setDragOverSide] = createSignal<"top" | "bottom" | null>(null);
  const [dragOverGroupId, setDragOverGroupId] = createSignal<string | null>(null);
  const [dragOverGroupSide, setDragOverGroupSide] = createSignal<"top" | "bottom" | null>(null);

  const draggedRepoPath = () => {
    const p = dragPayload();
    return p?.type === "repo" ? p.path : null;
  };

  const resetRepoDragState = () => {
    setDragPayload(null);
    setDragOverRepoPath(null);
    setDragOverSide(null);
    setDragOverGroupId(null);
    setDragOverGroupSide(null);
  };

  const handleRepoDragStart = (e: DragEvent, path: string) => {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `repo:${path}`);
    try {
      const el = e.currentTarget as HTMLElement;
      e.dataTransfer.setDragImage(el, el.offsetWidth / 2, el.offsetHeight / 2);
    } catch { /* not supported in all envs */ }
    const fromGroup = repositoriesStore.getGroupForRepo(path);
    setDragPayload({ type: "repo", path, fromGroupId: fromGroup?.id ?? null });
  };

  const handleRepoDragOver = (e: DragEvent, path: string) => {
    e.preventDefault();
    if (!e.dataTransfer) return;
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midpoint = rect.top + rect.height / 2;
    setDragOverRepoPath(path);
    setDragOverSide(e.clientY < midpoint ? "top" : "bottom");
    // Clear group-level hover when hovering a repo
    setDragOverGroupId(null);
    setDragOverGroupSide(null);
  };

  const handleRepoDrop = (e: DragEvent, targetPath: string) => {
    e.preventDefault();
    const payload = dragPayload();
    if (!payload || payload.type !== "repo" || payload.path === targetPath) {
      resetRepoDragState();
      return;
    }

    const sourcePath = payload.path;
    const sourceGroupId = payload.fromGroupId;
    const targetGroup = repositoriesStore.getGroupForRepo(targetPath);
    const targetGroupId = targetGroup?.id ?? null;
    const side = dragOverSide();

    if (sourceGroupId === targetGroupId) {
      // Same context (same group or both ungrouped) — reorder
      if (sourceGroupId) {
        // Reorder within group
        const group = repositoriesStore.state.groups[sourceGroupId];
        if (group) {
          const fromIndex = group.repoOrder.indexOf(sourcePath);
          const toIndex = group.repoOrder.indexOf(targetPath);
          if (fromIndex !== -1 && toIndex !== -1 && fromIndex !== toIndex) {
            let adjustedTo = toIndex;
            if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
            else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
            const clampedTo = Math.max(0, Math.min(adjustedTo, group.repoOrder.length - 1));
            if (fromIndex !== clampedTo) {
              repositoriesStore.reorderRepoInGroup(sourceGroupId, fromIndex, clampedTo);
            }
          }
        }
      } else {
        // Reorder within ungrouped
        const order = repositoriesStore.state.repoOrder;
        const fromIndex = order.indexOf(sourcePath);
        const toIndex = order.indexOf(targetPath);
        if (fromIndex !== -1 && toIndex !== -1 && fromIndex !== toIndex) {
          let adjustedTo = toIndex;
          if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
          else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
          const clampedTo = Math.max(0, Math.min(adjustedTo, order.length - 1));
          if (fromIndex !== clampedTo) {
            repositoriesStore.reorderRepo(fromIndex, clampedTo);
          }
        }
      }
    } else if (targetGroupId === null) {
      // Dragged from group to ungrouped area — remove from group
      repositoriesStore.removeRepoFromGroup(sourcePath);
    } else {
      // Dragged to different group — move between groups
      const targetGroupObj = repositoriesStore.state.groups[targetGroupId];
      const targetIndex = targetGroupObj ? targetGroupObj.repoOrder.indexOf(targetPath) : 0;
      let insertIndex = targetIndex;
      if (side === "bottom") insertIndex = targetIndex + 1;
      repositoriesStore.moveRepoBetweenGroups(sourcePath, sourceGroupId ?? "", targetGroupId, insertIndex);
    }

    resetRepoDragState();
  };

  // --- Group-level drag handlers ---
  const handleGroupDragStart = (e: DragEvent, groupId: string) => {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `group:${groupId}`);
    try {
      const el = e.currentTarget as HTMLElement;
      e.dataTransfer.setDragImage(el, el.offsetWidth / 2, el.offsetHeight / 2);
    } catch { /* not supported in all envs */ }
    setDragPayload({ type: "group", groupId });
  };

  const handleGroupDragOver = (e: DragEvent, groupId: string) => {
    e.preventDefault();
    if (!e.dataTransfer) return;
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midpoint = rect.top + rect.height / 2;
    setDragOverGroupId(groupId);
    setDragOverGroupSide(e.clientY < midpoint ? "top" : "bottom");
  };

  const handleGroupDrop = (e: DragEvent, targetGroupId: string) => {
    e.preventDefault();
    const payload = dragPayload();
    if (!payload) {
      resetRepoDragState();
      return;
    }

    if (payload.type === "group") {
      // Group-to-group reorder
      if (payload.groupId !== targetGroupId) {
        const order = repositoriesStore.state.groupOrder;
        const fromIndex = order.indexOf(payload.groupId);
        const toIndex = order.indexOf(targetGroupId);
        if (fromIndex !== -1 && toIndex !== -1) {
          let adjustedTo = toIndex;
          const side = dragOverGroupSide();
          if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
          else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
          const clampedTo = Math.max(0, Math.min(adjustedTo, order.length - 1));
          if (fromIndex !== clampedTo) {
            repositoriesStore.reorderGroups(fromIndex, clampedTo);
          }
        }
      }
    } else if (payload.type === "repo") {
      // Repo dropped on group header — assign to group
      repositoriesStore.addRepoToGroup(payload.path, targetGroupId);
    }

    resetRepoDragState();
  };

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

  // Placeholder handlers for group rename and color change (will be fleshed out in settings tab)
  const handleGroupRename = (_groupId: string) => {
    // TODO: inline rename or open rename dialog
  };

  const handleGroupColorChange = (_groupId: string) => {
    // TODO: open color picker
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
        isDragging={draggedRepoPath() === repo.path}
        isCreatingWorktree={props.creatingWorktreeRepos?.has(repo.path)}
        dragOverClass={
          dragOverRepoPath() === repo.path && draggedRepoPath() !== repo.path
            ? `drag-over-${dragOverSide()}`
            : undefined
        }
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
        onDragStart={(e) => handleRepoDragStart(e, repo.path)}
        onDragOver={(e) => handleRepoDragOver(e, repo.path)}
        onDrop={(e) => handleRepoDrop(e, repo.path)}
        onDragEnd={resetRepoDragState}
      />
    );
  };

  return (
    <aside id="sidebar">
      {/* Content */}
      <div class="sidebar-content">
        {/* Repository Section */}
        <div class="section">
          <div id="repo-list">
            {/* Grouped repos */}
            <For each={groupedLayout().groups}>
              {(entry) => (
                <GroupSection
                  group={entry.group}
                  repos={entry.repos}
                  quickSwitcherActive={props.quickSwitcherActive}
                  onRename={handleGroupRename}
                  onColorChange={handleGroupColorChange}
                  onDragStart={(e) => handleGroupDragStart(e, entry.group.id)}
                  onDragOver={(e) => handleGroupDragOver(e, entry.group.id)}
                  onDrop={(e) => handleGroupDrop(e, entry.group.id)}
                  onDragEnd={resetRepoDragState}
                  onHeaderDragOver={(e) => handleGroupDragOver(e, entry.group.id)}
                  onHeaderDrop={(e) => handleGroupDrop(e, entry.group.id)}
                  dragOverClass={
                    dragOverGroupId() === entry.group.id && dragPayload()?.type !== "repo"
                      ? `drag-over-${dragOverGroupSide()}`
                      : dragOverGroupId() === entry.group.id && dragPayload()?.type === "repo"
                        ? "drag-over-target"
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

      {/* Drag handle for resizing */}
      <div class="sidebar-resize-handle" onMouseDown={handleResizeStart} />
    </aside>
  );
};

export default Sidebar;
