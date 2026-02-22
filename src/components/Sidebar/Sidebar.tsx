import { Component, For, Show, createMemo, createSignal, createEffect, type JSX } from "solid-js";
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
import { PromptDialog } from "../PromptDialog";
import { getModifierSymbol } from "../../platform";
import { compareBranches } from "../../utils/branchSort";
import { escapeShellArg, cx } from "../../utils";
import { t } from "../../i18n";
import s from "./Sidebar.module.css";

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

const BRANCH_ICON_CLASSES: Record<string, string> = {
  main: s.branchIconMain,
  feature: s.branchIconFeature,
  question: s.branchIconQuestion,
};

const PR_BADGE_CLASSES: Record<string, string> = {
  ready: s.prReady,
  open: s.prOpen,
  merged: s.prMerged,
  closed: s.prClosed,
  draft: s.prDraft,
  conflict: s.prConflict,
  "ci-failed": s.prCiFailed,
  "changes-requested": s.prChangesRequested,
  "review-required": s.prReviewRequired,
  "ci-pending": s.prCiPending,
};

const DRAG_CLASSES: Record<string, string> = {
  top: s.dragOverTop,
  bottom: s.dragOverBottom,
  target: s.dragOverTarget,
};

/** Branch icon component — shows ? when any terminal in the branch awaits input */
const BranchIcon: Component<{ isMain: boolean; hasQuestion?: boolean }> = (props) => (
  <span class={cx(s.branchIcon, BRANCH_ICON_CLASSES[props.hasQuestion ? "question" : props.isMain ? "main" : "feature"])}>
    {props.hasQuestion ? "?" : props.isMain
      ? <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M9.2 1.2v4.4L13 3.2a1.3 1.3 0 1 1 1.3 2.3L10.5 8l3.8 2.5a1.3 1.3 0 1 1-1.3 2.3L9.2 10.4v4.4a1.2 1.2 0 0 1-2.4 0v-4.4L3 13a1.3 1.3 0 1 1-1.3-2.3L5.5 8 1.7 5.5A1.3 1.3 0 0 1 3 3.2l3.8 2.4V1.2a1.2 1.2 0 0 1 2.4 0z"/></svg>
      : <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z"/></svg>}
  </span>
);

/** Stats badge component - shows additions/deletions */
const StatsBadge: Component<{ additions: number; deletions: number }> = (props) => (
  <Show when={props.additions > 0 || props.deletions > 0}>
    <div class={s.branchStats}>
      <span class={s.statAdd}>+{props.additions}</span>
      <span class={s.statDel}>-{props.deletions}</span>
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
    const state = props.state?.toLowerCase();
    if (state === "merged") return { label: "Merged", cls: "merged" };
    if (state === "closed") return { label: "Closed", cls: "closed" };
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
    <span class={cx(s.prBadge, PR_BADGE_CLASSES[badge().cls])} title={`PR #${props.prNumber}`}>
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
  const [groupPromptVisible, setGroupPromptVisible] = createSignal(false);

  const branches = createMemo(() => Object.values(props.repo.branches));
  // Pre-compute PR statuses once per poll cycle; avoids calling getPrStatus inside sort comparator
  const prStatuses = createMemo(() => {
    const map = new Map<string, ReturnType<typeof githubStore.getPrStatus>>();
    for (const b of branches()) {
      map.set(b.name, githubStore.getPrStatus(props.repo.path, b.name));
    }
    return map;
  });
  const sortedBranches = createMemo(() => {
    const statuses = prStatuses();
    return [...branches()].sort((a, b) =>
      compareBranches(a, b, statuses.get(a.name), statuses.get(b.name)),
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
      action: () => setGroupPromptVisible(true),
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
      class={cx(s.repoSection, props.repo.collapsed && s.collapsed, props.isDragging && s.dragging, props.dragOverClass)}
      draggable={true}
      onDragStart={(e) => { e.stopPropagation(); props.onDragStart(e); }}
      onDragOver={(e) => { e.stopPropagation(); props.onDragOver(e); }}
      onDrop={(e) => { e.stopPropagation(); props.onDrop(e); }}
      onDragEnd={props.onDragEnd}
    >
      {/* Repo header */}
      <div class={s.repoHeader} onClick={props.onToggle} onContextMenu={repoMenu.open}>
        <Show when={props.repo.collapsed}>
          <span
            class={s.repoInitials}
            onClick={(e) => {
              e.stopPropagation();
              props.onToggleCollapsed();
            }}
            title={t("sidebar.clickToExpand", "Click to expand")}
          >
            {props.repo.initials}
          </span>
        </Show>
        <Show when={!props.repo.collapsed}>
          <span class={s.repoName} style={props.nameColor ? { color: props.nameColor } : undefined}>{props.repo.displayName}</span>
          <div class={s.repoActions}>
              <button
                class={s.repoActionBtn}
                onClick={handleMenuToggle}
                title={t("sidebar.repoOptions", "Repository options")}
              >
                ⋯
              </button>
            <button
              class={cx(s.repoActionBtn, s.addBtn)}
              disabled={props.isCreatingWorktree}
              onClick={(e) => {
                e.stopPropagation();
                props.onAddWorktree();
              }}
              title={props.isCreatingWorktree ? t("sidebar.creatingWorktree", "Creating worktree…") : t("sidebar.addWorktree", "Add worktree")}
            >
              {props.isCreatingWorktree ? "…" : "+"}
            </button>
          </div>
          <span class={cx(s.repoChevron, props.repo.expanded && s.expanded)}>{"\u203A"}</span>
        </Show>
      </div>

      {/* Branches - force expanded in quick switcher mode */}
      <Show when={(props.repo.expanded && !props.repo.collapsed) || props.quickSwitcherActive}>
        <div class={s.repoBranches}>
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
            <div class={s.repoEmpty}>{t("sidebar.noBranches", "No branches loaded")}</div>
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
      <PromptDialog
        visible={groupPromptVisible()}
        title="New Group"
        placeholder="Group name"
        confirmLabel="Create"
        onClose={() => setGroupPromptVisible(false)}
        onConfirm={(name) => {
          const groupId = repositoriesStore.createGroup(name);
          if (groupId) {
            repositoriesStore.addRepoToGroup(props.repo.path, groupId);
          }
        }}
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
      class={cx(s.branchItem, props.isActive && s.active, hasActivity() && s.hasActivity, hasIdle() && s.shellIdle)}
      onClick={props.onSelect}
      onContextMenu={ctxMenu.open}
    >
      <BranchIcon isMain={props.branch.isMain} hasQuestion={hasQuestion()} />
      <div class={s.branchContent}>
        <span
          class={s.branchName}
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
        <div class={s.branchActions}>
          <button
            class={s.branchAddBtn}
            onClick={(e) => {
              e.stopPropagation();
              props.onAddTerminal();
            }}
            title={t("sidebar.addTerminal", "Add terminal")}
          >
            +
          </button>
          <Show when={!props.branch.isMain && props.branch.worktreePath && props.canRemove}>
            <button
              class={s.branchRemoveBtn}
              onClick={(e) => {
                e.stopPropagation();
                props.onRemove();
              }}
              title={t("sidebar.removeWorktree", "Remove worktree")}
            >
              ×
            </button>
          </Show>
        </div>
      }>
        <span class={s.branchShortcut}>{getModifierSymbol()}^{props.shortcutIndex}</span>
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
  children: JSX.Element;
}> = (props) => {
  const groupMenu = createContextMenu();

  const groupMenuItems = (): ContextMenuItem[] => [
    { label: "Rename Group", action: () => props.onRename(props.group.id) },
    { label: "Change Color", action: () => props.onColorChange(props.group.id) },
    { label: "Delete Group", action: () => repositoriesStore.deleteGroup(props.group.id) },
  ];

  return (
    <div
      class={cx(s.groupSection, props.dragOverClass)}
      draggable={true}
      onDragStart={props.onDragStart}
      onDragOver={props.onDragOver}
      onDrop={props.onDrop}
      onDragEnd={props.onDragEnd}
    >
      <div
        class={s.groupHeader}
        onClick={() => repositoriesStore.toggleGroupCollapsed(props.group.id)}
        onContextMenu={groupMenu.open}
        onDragOver={(e: DragEvent) => { e.stopPropagation(); props.onHeaderDragOver?.(e); }}
        onDrop={(e: DragEvent) => { e.stopPropagation(); props.onHeaderDrop?.(e); }}
      >
        <Show when={props.group.color}>
          <span class={s.groupColorDot} style={{ background: props.group.color }} />
        </Show>
        <span class={s.groupName}>{props.group.name}</span>
        <span class={s.groupCount}>{props.repos.length}</span>
        <span class={cx(s.groupChevron, !props.group.collapsed && s.expanded)}>{"\u203A"}</span>
      </div>
      <Show when={!props.group.collapsed || props.quickSwitcherActive}>
        <div class={s.groupRepos}>
          <Show when={props.repos.length === 0}>
            <div class={s.groupEmptyHint}>{t("sidebar.dragReposHere", "Drag repos here")}</div>
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
      // Dragged to different group
      if (sourceGroupId) {
        // Between two groups — preserves insert position
        const targetGroupObj = repositoriesStore.state.groups[targetGroupId];
        const targetIndex = targetGroupObj ? targetGroupObj.repoOrder.indexOf(targetPath) : 0;
        let insertIndex = targetIndex;
        if (side === "bottom") insertIndex = targetIndex + 1;
        repositoriesStore.moveRepoBetweenGroups(sourcePath, sourceGroupId, targetGroupId, insertIndex);
      } else {
        // From ungrouped into a group
        repositoriesStore.addRepoToGroup(sourcePath, targetGroupId);
      }
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
        isDragging={draggedRepoPath() === repo.path}
        isCreatingWorktree={props.creatingWorktreeRepos?.has(repo.path)}
        dragOverClass={
          dragOverRepoPath() === repo.path && draggedRepoPath() !== repo.path
            ? DRAG_CLASSES[dragOverSide() ?? ""] ?? undefined
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
                  onDragStart={(e) => handleGroupDragStart(e, entry.group.id)}
                  onDragOver={(e) => handleGroupDragOver(e, entry.group.id)}
                  onDrop={(e) => handleGroupDrop(e, entry.group.id)}
                  onDragEnd={resetRepoDragState}
                  onHeaderDragOver={(e) => handleGroupDragOver(e, entry.group.id)}
                  onHeaderDrop={(e) => handleGroupDrop(e, entry.group.id)}
                  dragOverClass={
                    dragOverGroupId() === entry.group.id && dragPayload()?.type !== "repo"
                      ? DRAG_CLASSES[dragOverGroupSide() ?? ""] ?? undefined
                      : dragOverGroupId() === entry.group.id && dragPayload()?.type === "repo"
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
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${escapeShellArg(repo.path)} && git pull`);
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
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${escapeShellArg(repo.path)} && git push`);
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
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${escapeShellArg(repo.path)} && git fetch --all`);
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
              onClick={() => {
                const repo = repositoriesStore.getActive();
                if (repo) props.onGitCommand?.(`cd ${escapeShellArg(repo.path)} && git stash`);
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
