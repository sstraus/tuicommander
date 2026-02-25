import { Component, For, Show, createMemo, createSignal } from "solid-js";
import type { RepositoryState, BranchState } from "../../stores/repositories";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { githubStore } from "../../stores/github";
import { userActivityStore } from "../../stores/userActivity";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import { PromptDialog } from "../PromptDialog";
import { getModifierSymbol } from "../../platform";
import { compareBranches } from "../../utils/branchSort";
import { cx } from "../../utils";
import { t } from "../../i18n";
import s from "./Sidebar.module.css";

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

/** Branch icon component — shows ? when any terminal in the branch awaits input */
export const BranchIcon: Component<{ isMain: boolean; isShell?: boolean; hasQuestion?: boolean }> = (props) => (
  <span class={cx(s.branchIcon, BRANCH_ICON_CLASSES[props.hasQuestion ? "question" : props.isShell ? "feature" : props.isMain ? "main" : "feature"])}>
    {props.hasQuestion ? "?" : props.isShell
      ? <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M1 3l5 5-5 5h2l5-5-5-5H1zm7 9h7v2H8v-2z"/></svg>
      : props.isMain
      ? <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M9.2 1.2v4.4L13 3.2a1.3 1.3 0 1 1 1.3 2.3L10.5 8l3.8 2.5a1.3 1.3 0 1 1-1.3 2.3L9.2 10.4v4.4a1.2 1.2 0 0 1-2.4 0v-4.4L3 13a1.3 1.3 0 1 1-1.3-2.3L5.5 8 1.7 5.5A1.3 1.3 0 0 1 3 3.2l3.8 2.4V1.2a1.2 1.2 0 0 1 2.4 0z"/></svg>
      : <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z"/></svg>}
  </span>
);

/** Stats badge component - shows additions/deletions */
export const StatsBadge: Component<{ additions: number; deletions: number }> = (props) => (
  <Show when={props.additions > 0 || props.deletions > 0}>
    <div class={s.branchStats}>
      <span class={s.statAdd}>+{props.additions}</span>
      <span class={s.statDel}>-{props.deletions}</span>
    </div>
  </Show>
);

/** PR state badge — single badge replacing both old PR number badge and CI ring.
 *  Shows the most critical state as short text with color/animation. */
export const PrStateBadge: Component<{
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

/** Accumulated activity time (ms) per merged PR, keyed by `repoPath:branch:prNumber`.
 *  Tracks how long the user has been active since the merged PR was first seen. */
const mergedActivityAccum = new Map<string, { ms: number; lastCheck: number }>();

/** Activity-based grace period (ms) before hiding merged PRs */
const MERGED_GRACE_MS = 5 * 60 * 1000; // 5 minutes

/** Get PR status with lifecycle rules:
 *  - CLOSED: hidden immediately
 *  - MERGED: hidden after 5 min of accumulated user activity
 *  - OPEN: shown as-is */
function activePrStatus(repoPath: string, branch: string) {
  const pr = githubStore.getPrStatus(repoPath, branch);
  if (!pr) return null;

  const state = pr.state?.toUpperCase();

  // CLOSED: never show
  if (state === "CLOSED") return null;

  // MERGED: activity-based grace period
  if (state === "MERGED") {
    const prKey = `${repoPath}:${branch}:${pr.number}`;
    const now = Date.now();
    const lastActivity = userActivityStore.lastActivityAt();

    let entry = mergedActivityAccum.get(prKey);
    if (!entry) {
      entry = { ms: 0, lastCheck: now };
      mergedActivityAccum.set(prKey, entry);
    }

    // Accumulate: if user was active within the last 2s, add elapsed since last check
    if (lastActivity > 0 && now - lastActivity < 2000) {
      const elapsed = now - entry.lastCheck;
      if (elapsed > 0 && elapsed < 60_000) { // cap at 60s to avoid jumps
        entry.ms += elapsed;
      }
    }
    entry.lastCheck = now;

    if (entry.ms >= MERGED_GRACE_MS) {
      mergedActivityAccum.delete(prKey);
      return null;
    }
  }

  return pr;
}

/** Reset merged activity accumulators (for testing) */
export function _resetMergedActivityAccum(): void {
  mergedActivityAccum.clear();
}

/** Branch item component */
export const BranchItem: Component<{
  branch: BranchState;
  repoPath: string;
  isActive: boolean;
  canRemove: boolean;
  shortcutIndex?: number;
  agentMenuItems?: () => ContextMenuItem[];
  onSelect: () => void;
  onAddTerminal: () => void;
  onRemove: () => void;
  onRename: () => void;
  onShowPrDetail: () => void;
  onCreateWorktreeFromBranch?: () => void;
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
    if (props.branch.isMain || props.branch.isShell) {
      props.onAddTerminal();
      return;
    }
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
    ];
    if (!props.branch.isShell) {
      const agentItems = props.agentMenuItems?.();
      if (agentItems && agentItems.length > 0) {
        items.push(...agentItems);
      }
      items.push({ label: "Rename Branch", action: props.onRename, disabled: props.branch.isMain });
      if (!props.branch.isMain && !props.branch.worktreePath && props.onCreateWorktreeFromBranch) {
        items.push({ label: "Create Worktree", action: props.onCreateWorktreeFromBranch });
      }
      if (!props.branch.isMain && props.branch.worktreePath && props.canRemove) {
        items.push({ label: "Delete Worktree", action: props.onRemove, separator: true });
      }
    }
    return items;
  };

  return (
    <div
      class={cx(s.branchItem, props.isActive && s.active, hasActivity() && s.hasActivity, hasIdle() && s.shellIdle)}
      onClick={props.onSelect}
      onContextMenu={ctxMenu.open}
    >
      <BranchIcon isMain={props.branch.isMain} isShell={props.branch.isShell} hasQuestion={hasQuestion()} />
      <div class={s.branchContent}>
        <span
          class={s.branchName}
          onDblClick={handleDoubleClick}
          title={props.branch.name}
        >
          {props.branch.name}
        </span>
      </div>
      <Show when={activePrStatus(props.repoPath, props.branch.name)}>
        {(() => {
          const prData = () => activePrStatus(props.repoPath, props.branch.name)!;
          const checks = () => githubStore.getCheckSummary(props.repoPath, props.branch.name);
          const isTerminal = () => {
            const st = prData().state?.toLowerCase();
            return st === "closed" || st === "merged";
          };
          return (
            <span
              class={isTerminal() ? s.prBadgeDimmed : undefined}
              onClick={(e) => { e.stopPropagation(); props.onShowPrDetail(); }}
            >
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
        })()}
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

/** Repository section component */
export const RepoSection: Component<{
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
  buildAgentMenuItems?: (branchName: string) => ContextMenuItem[];
  onAddWorktree: () => void;
  onCreateWorktreeFromBranch?: (branchName: string) => void;
  onSettings: () => void;
  onRemove: () => void;
  onToggle: () => void;
  onToggleCollapsed: () => void;
  onToggleShowAllBranches: () => void;
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

    if (props.repo.isGitRepo !== false) {
      items.push({
        label: props.repo.showAllBranches ? "Show Active Only" : "Show All Branches",
        action: () => props.onToggleShowAllBranches(),
      });
    }
    items.push({ label: "Park Repository", action: () => repositoriesStore.setPark(props.repo.path, true) });
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
            <Show when={props.repo.isGitRepo !== false}>
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
            </Show>
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
                agentMenuItems={props.buildAgentMenuItems ? () => props.buildAgentMenuItems!(branch.name) : undefined}
                onSelect={() => props.onBranchSelect(branch.name)}
                onAddTerminal={() => props.onAddTerminal(branch.name)}
                onRemove={() => props.onRemoveBranch(branch.name)}
                onRename={() => props.onRenameBranch(branch.name)}
                onShowPrDetail={() => props.onShowPrDetail(branch.name)}
                onCreateWorktreeFromBranch={props.onCreateWorktreeFromBranch ? () => props.onCreateWorktreeFromBranch!(branch.name) : undefined}
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
