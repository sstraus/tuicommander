import { Component, For, Show, createMemo, createSignal } from "solid-js";
import type { RepositoryState, BranchState } from "../../stores/repositories";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { githubStore } from "../../stores/github";
import { shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { activePrStatus, _resetMergedActivityAccum } from "../../utils/mergedPrGrace";
import { effectiveMergeMethod } from "../../utils/prMerge";
export { effectiveMergeMethod };
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import { PromptDialog } from "../PromptDialog";
import { getModifierSymbol } from "../../platform";
import { compareBranches } from "../../utils/branchSort";
import { invoke } from "../../invoke";
import { cx } from "../../utils";
import { t } from "../../i18n";
import { sidebarPluginStore } from "../../stores/sidebarPluginStore";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { SidebarPluginSection } from "./SidebarPluginSection";
import { remoteUrlToGitHub } from "../GitPanel/BranchesTab";
import { handleOpenUrl } from "../../utils/openUrl";
import s from "./Sidebar.module.css";

const BRANCH_ICON_CLASSES: Record<string, string> = {
  main: s.branchIconMain,
  worktree: s.branchIconWorktree,
  error: s.branchIconError,
  question: s.branchIconQuestion,
  activity: s.branchIconActivity,
  unseen: s.branchIconUnseen,
  idle: s.branchIconIdle,
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

/** Branch icon component — icon shape and color driven by terminal state.
 *
 *  Icon shapes:
 *  - Main worktree + main branch → star
 *  - Main worktree + non-main branch (after switch) → branch icon
 *  - Linked worktree → worktree fork icon
 *  - Shell (non-git dir) → terminal icon
 *  - Question (awaiting input) → "?" (overrides all)
 *
 *  Color priority (highest wins):
 *  1. question  → --attention (pulsing)
 *  2. busy      → --activity  (pulsing)
 *  3. unseen    → --unseen    (static purple)
 *  4. idle      → --fg-muted  (no terminals in repo)
 *  5. base      → --warning (main) or --success (worktree)
 */
export const BranchIcon: Component<{
  isMainBranch: boolean;
  isMainWorktree: boolean;
  isShell?: boolean;
  hasError?: boolean;
  hasQuestion?: boolean;
  hasBusy?: boolean;
  hasUnseen?: boolean;
  repoHasTerminals?: boolean;
}> = (props) => {
  const iconShape = () => {
    if (props.hasError) return "error";
    if (props.hasQuestion) return "question";
    if (props.isShell) return "shell";
    if (props.isMainWorktree && props.isMainBranch) return "star";
    if (props.isMainWorktree) return "branch";
    return "worktree";
  };

  /** Single source of truth for icon color — priority cascade.
   *  Error > question > busy > unseen > base.
   *  Busy overrides the base color; idle does NOT — the base color
   *  (yellow for main, green for worktree) is already correct when
   *  nothing special is happening. */
  const colorClass = () => {
    if (props.hasError) return "error";
    if (props.hasQuestion) return "question";
    if (props.hasBusy) return "activity";
    if (props.hasUnseen) return "unseen";
    if (props.repoHasTerminals === false) return "idle";
    if (props.isMainBranch) return "main";
    return "worktree";
  };

  return (
    <span class={cx(s.branchIcon, BRANCH_ICON_CLASSES[colorClass()])}>
      {(() => {
        switch (iconShape()) {
          case "error": return "!";
          case "question": return "?";
          case "shell": return (
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M1 3l5 5-5 5h2l5-5-5-5H1zm7 9h7v2H8v-2z"/></svg>
          );
          case "star": return (
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M9.2 1.2v4.4L13 3.2a1.3 1.3 0 1 1 1.3 2.3L10.5 8l3.8 2.5a1.3 1.3 0 1 1-1.3 2.3L9.2 10.4v4.4a1.2 1.2 0 0 1-2.4 0v-4.4L3 13a1.3 1.3 0 1 1-1.3-2.3L5.5 8 1.7 5.5A1.3 1.3 0 0 1 3 3.2l3.8 2.4V1.2a1.2 1.2 0 0 1 2.4 0z"/></svg>
          );
          case "worktree": return (
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
              <path d="M5 1.5a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm0 10a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm6-4a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM5 5v2.5a2 2 0 0 0 2 2h2.5M5 10.5V8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            </svg>
          );
          default: return (
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z"/></svg>
          );
        }
      })()}
    </span>
  );
};

/** Stats badge component - shows additions/deletions */
export const StatsBadge: Component<{ additions: number; deletions: number; onClick?: (e: MouseEvent) => void }> = (props) => (
  <Show when={props.additions > 0 || props.deletions > 0}>
    <div class={s.branchStats} onClick={props.onClick} style={props.onClick ? { cursor: "pointer" } : undefined}>
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

export { _resetMergedActivityAccum };

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
  onShowChanges?: () => void;
  onCreateWorktreeFromBranch?: () => void;
  onMergeAndArchive?: () => void;
  onSwitchBranch?: (branchName: string) => void;
  switchBranchList?: () => string[];
  currentBranch?: () => string;
  githubBaseUrl?: string | null;
  repoHasTerminals: boolean;
}> = (props) => {
  const ctxMenu = createContextMenu();

  const pr = createMemo(() => activePrStatus(props.repoPath, props.branch.name));
  const checks = createMemo(() => githubStore.getCheckSummary(props.repoPath, props.branch.name));

  const hasError = () =>
    props.branch.terminals.some((id) => terminalsStore.get(id)?.awaitingInput === "error");

  const hasQuestion = () =>
    props.branch.terminals.some((id) => terminalsStore.get(id)?.awaitingInput === "question");

  // Debounced busy — centralized in terminalsStore with 2s hold
  const hasBusy = () =>
    props.branch.terminals.some((id) => terminalsStore.isBusy(id));

  const hasUnseen = () =>
    props.branch.terminals.some((id) => terminalsStore.get(id)?.unseen);

  const handleDoubleClick = (e: MouseEvent) => {
    e.stopPropagation();
    if (props.branch.isMain || props.branch.isShell) {
      // Saved terminals will be restored by handleBranchSelect (fired from the
      // click event that precedes dblclick). Don't add a duplicate terminal.
      if (props.branch.savedTerminals?.length) return;
      props.onAddTerminal();
      return;
    }
    props.onRename();
  };

  const handleCopyPath = async () => {
    const path = props.branch.worktreePath;
    if (path) {
      try {
        await navigator.clipboard.writeText(shortenHomePath(path));
      } catch (err) {
        appLogger.warn("app", "Failed to copy path to clipboard", err);
      }
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
      const isLinkedWorktree = !!props.branch.worktreePath && props.branch.worktreePath !== props.repoPath;
      items.push({ label: isLinkedWorktree ? "Rename Worktree" : "Rename Branch", action: props.onRename, disabled: props.branch.isMain });
      if (!props.branch.isMain && !props.branch.worktreePath && props.onCreateWorktreeFromBranch) {
        items.push({ label: "Create Worktree", action: props.onCreateWorktreeFromBranch });
      }
      if (!props.branch.isMain && isLinkedWorktree && props.onMergeAndArchive) {
        items.push({ label: "Merge & Archive", action: props.onMergeAndArchive });
      }
      if (!props.branch.isMain && isLinkedWorktree && props.canRemove) {
        items.push({ label: "Delete Worktree", action: props.onRemove, separator: true });
      }
    }
    // "Switch Branch" submenu — only on main worktree row (worktreePath === repoPath)
    const isMainWorktree = props.branch.worktreePath === props.repoPath;
    if (isMainWorktree && props.onSwitchBranch && props.switchBranchList && props.currentBranch) {
      const switchBranch = props.onSwitchBranch; // capture narrowed value before closure
      const current = props.currentBranch();
      const branchList = props.switchBranchList();
      if (branchList.length > 0) {
        const branchChildren: ContextMenuItem[] = branchList.map((name) => ({
          label: name === current ? `${name}  \u2713` : name,
          action: () => { if (name !== current) switchBranch(name); },
          disabled: name === current,
        }));
        items.push({ label: t("sidebar.switchBranch", "Switch Branch"), action: () => {}, children: branchChildren, separator: true });
      }
    }
    // GitHub links
    if (props.githubBaseUrl) {
      const ghBase = props.githubBaseUrl;
      const branchUrl = `${ghBase}/tree/${encodeURIComponent(props.branch.name)}`;
      items.push({ label: "Open in GitHub", action: () => handleOpenUrl(branchUrl), separator: true });
      // If branch has an open PR, add direct link
      const prStatus = githubStore.getPrStatus(props.repoPath, props.branch.name);
      if (prStatus?.url) {
        items.push({ label: "Open PR", action: () => handleOpenUrl(prStatus.url) });
      }
    }
    // Plugin-registered branch actions
    const branchActions = contextMenuActionsStore.getContextActions("branch");
    if (branchActions.length > 0) {
      const ctx = { target: "branch" as const, repoPath: props.repoPath, branchName: props.branch.name };
      for (const a of branchActions) {
        items.push({
          label: a.label,
          action: () => a.action(ctx),
          disabled: a.disabled?.(ctx),
          separator: branchActions.indexOf(a) === 0,
        });
      }
    }
    return items;
  };

  return (
    <div
      class={cx(s.branchItem, props.isActive && s.active)}
      onClick={props.onSelect}
      onContextMenu={ctxMenu.open}
    >
      <BranchIcon
        isMainBranch={props.branch.isMain}
        isMainWorktree={props.branch.worktreePath === props.repoPath}
        isShell={props.branch.isShell}
        hasError={hasError()}
        hasQuestion={hasQuestion()}
        hasBusy={hasBusy()}
        hasUnseen={hasUnseen()}
        repoHasTerminals={props.repoHasTerminals}
      />
      <div class={s.branchContent}>
        <span
          class={s.branchName}
          onDblClick={handleDoubleClick}
          title={props.branch.name}
        >
          {props.branch.name}
        </span>
      </div>
      <Show when={props.branch.isMerged && !props.branch.isMain && !props.branch.terminals.length && !(props.branch.additions + props.branch.deletions)}>
        <span class={s.mergedBadge} title="Branch is merged into main">Merged</span>
      </Show>
      <Show when={pr()}>
        <span
          class={(() => { const st = pr()?.state?.toLowerCase(); return st === "closed" || st === "merged" ? s.prBadgeDimmed : undefined; })()}
          onClick={(e) => { e.stopPropagation(); props.onShowPrDetail(); }}
        >
          <PrStateBadge
            prNumber={pr()!.number}
            state={pr()!.state}
            isDraft={pr()!.is_draft}
            mergeable={pr()!.mergeable}
            reviewDecision={pr()!.review_decision}
            ciPassed={checks()?.passed}
            ciFailed={checks()?.failed}
            ciPending={checks()?.pending}
          />
        </span>
      </Show>
      <StatsBadge additions={props.branch.additions} deletions={props.branch.deletions} onClick={props.onShowChanges ? (e) => { e.stopPropagation(); props.onShowChanges?.(); } : undefined} />
      <div class={s.branchActions} style={{ display: props.shortcutIndex !== undefined ? "none" : undefined }}>
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
      <span class={s.branchShortcut} style={{ display: props.shortcutIndex !== undefined ? undefined : "none" }}>
        {getModifierSymbol()}^{props.shortcutIndex}
      </span>
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

export { canMergePr } from "./RemoteOnlyPrPopover";
import { GitHubPanel } from "./GitHubPanel";


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
  onShowChanges?: () => void;
  buildAgentMenuItems?: (branchName: string) => ContextMenuItem[];
  onAddWorktree: () => void;
  onCreateWorktreeFromBranch?: (branchName: string) => void;
  onMergeAndArchive?: (branchName: string) => void;
  onSettings: () => void;
  onRemove: () => void;
  onToggle: () => void;
  onToggleCollapsed: () => void;
  onCheckoutRemoteBranch?: (branchName: string) => void;
  onSwitchBranch: (branchName: string) => void;
  switchBranchList: () => string[];
  currentBranch: () => string;
  onDragStart: (e: DragEvent) => void;
  onDragOver: (e: DragEvent) => void;
  onDrop: (e: DragEvent) => void;
  onDragEnd: () => void;
}> = (props) => {
  const repoMenu = createContextMenu();
  const [groupPromptVisible, setGroupPromptVisible] = createSignal(false);
  const [remoteOnlyPopoverVisible, setRemoteOnlyPopoverVisible] = createSignal(false);
  const [remoteCleanupActive, setRemoteCleanupActive] = createSignal(false);
  const [githubBaseUrl, setGithubBaseUrl] = createSignal<string | null>(null);

  // Fetch GitHub URL for "Open in GitHub" context menu actions
  if (props.repo.isGitRepo !== false) {
    invoke<string | null>("get_remote_url", { path: props.repo.path }).then((url) => {
      if (url) setGithubBaseUrl(remoteUrlToGitHub(url));
    }).catch(() => {});
  }

  const branches = createMemo(() => Object.values(props.repo.branches));
  const repoHasTerminals = createMemo(() => branches().some((b) => b.terminals.length > 0));
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
  const canRemoveAny = createMemo(() => sortedBranches().length > 1);

  const localBranchNames = createMemo(() => new Set(Object.keys(props.repo.branches)));
  const remoteOnlyPrs = createMemo(() =>
    githubStore.getRemoteOnlyPrs(props.repo.path, localBranchNames()),
  );
  const repoIssues = createMemo(() => githubStore.getRepoIssues(props.repo.path));
  const ghBadgeCount = createMemo(() => remoteOnlyPrs().length + repoIssues().length);

  const repoMenuItems = (): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      { label: "Repo Settings", action: () => props.onSettings() },
    ];

    if (props.repo.isGitRepo !== false) {
      items.push({ label: "Create Worktree", action: () => props.onAddWorktree() });
    }

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

    // GitHub link
    const ghUrl = githubBaseUrl();
    if (ghUrl) {
      items.push({ label: "Open in GitHub", action: () => handleOpenUrl(ghUrl), separator: true });
    }
    items.push({ label: "Park Repository", action: () => repositoriesStore.setPark(props.repo.path, true), separator: !ghUrl });
    items.push({ label: "Remove Repository", action: () => props.onRemove() });
    // Plugin-registered repo actions
    const repoActions = contextMenuActionsStore.getContextActions("repo");
    if (repoActions.length > 0) {
      const ctx = { target: "repo" as const, repoPath: props.repo.path };
      for (const a of repoActions) {
        items.push({
          label: a.label,
          action: () => a.action(ctx),
          disabled: a.disabled?.(ctx),
          separator: repoActions.indexOf(a) === 0,
        });
      }
    }
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
            <Show when={ghBadgeCount() > 0}>
              <button
                class={cx(s.repoActionBtn, s.ghBadgeBtn)}
                onClick={(e) => { e.stopPropagation(); setRemoteOnlyPopoverVisible((v) => !v); }}
                title={t("sidebar.githubPanelTitle", "GitHub: PRs & Issues")}
              >
                <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path fill-rule="evenodd" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
                {ghBadgeCount()}
              </button>
            </Show>
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

      {/* Branches */}
      <Show when={props.repo.expanded && !props.repo.collapsed}>
        <div class={s.repoBranches}>
          <For each={sortedBranches()}>
            {(branch, index) => (
              <BranchItem
                branch={branch}
                repoPath={props.repo.path}
                isActive={repositoriesStore.state.activeRepoPath === props.repo.path && props.repo.activeBranch === branch.name}
                canRemove={canRemoveAny()}
                shortcutIndex={props.quickSwitcherActive ? props.branchShortcutStart + index() : undefined}
                agentMenuItems={props.buildAgentMenuItems ? () => props.buildAgentMenuItems!(branch.name) : undefined}
                onSelect={() => props.onBranchSelect(branch.name)}
                onAddTerminal={() => props.onAddTerminal(branch.name)}
                onRemove={() => props.onRemoveBranch(branch.name)}
                onRename={() => props.onRenameBranch(branch.name)}
                onShowPrDetail={() => props.onShowPrDetail(branch.name)}
                onShowChanges={props.onShowChanges}
                onCreateWorktreeFromBranch={props.onCreateWorktreeFromBranch ? () => props.onCreateWorktreeFromBranch!(branch.name) : undefined}
                onMergeAndArchive={props.onMergeAndArchive ? () => props.onMergeAndArchive!(branch.name) : undefined}
                onSwitchBranch={branch.worktreePath === props.repo.path ? (name) => props.onSwitchBranch(name) : undefined}
                switchBranchList={branch.worktreePath === props.repo.path ? props.switchBranchList : undefined}
                currentBranch={branch.worktreePath === props.repo.path ? props.currentBranch : undefined}
                githubBaseUrl={githubBaseUrl()}
                repoHasTerminals={repoHasTerminals()}
              />
            )}
          </For>
          <Show when={sortedBranches().length === 0}>
            <div class={s.repoEmpty}>{t("sidebar.noBranches", "No branches loaded")}</div>
          </Show>
        </div>
      </Show>
      <Show when={props.repo.expanded && !props.repo.collapsed}>
        <For each={sidebarPluginStore.getPanels().filter((p) => p.items.length > 0)}>
          {(panel) => <SidebarPluginSection panel={panel} />}
        </For>
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
      <Show when={remoteOnlyPopoverVisible() && (ghBadgeCount() > 0 || remoteCleanupActive())}>
        <GitHubPanel
          prs={remoteOnlyPrs()}
          repoPath={props.repo.path}
          onClose={() => setRemoteOnlyPopoverVisible(false)}
          onCheckout={(branch) => {
            setRemoteOnlyPopoverVisible(false);
            props.onCheckoutRemoteBranch?.(branch);
          }}
          onCreateWorktree={props.onCreateWorktreeFromBranch}
          onCleanupActive={setRemoteCleanupActive}
        />
      </Show>
    </div>
  );
};
