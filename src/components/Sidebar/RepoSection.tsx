import { Component, For, Show, createMemo, createSignal } from "solid-js";
import type { RepositoryState, BranchState } from "../../stores/repositories";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { githubStore } from "../../stores/github";
import { appLogger } from "../../stores/appLogger";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repoSettingsStore } from "../../stores/repoSettings";
import { activePrStatus, _resetMergedActivityAccum } from "../../utils/mergedPrGrace";
import { effectiveMergeMethod, mergeWithFallback } from "../../utils/prMerge";
export { effectiveMergeMethod };
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import { PromptDialog } from "../PromptDialog";
import { getModifierSymbol } from "../../platform";
import { compareBranches } from "../../utils/branchSort";
import { invoke } from "../../invoke";
import { cx } from "../../utils";
import { t } from "../../i18n";
import type { BranchPrStatus } from "../../types";
import { PrDetailContent } from "../PrDetailPopover/PrDetailContent";
import { mdTabsStore } from "../../stores/mdTabs";
import { PostMergeCleanupDialog, type CleanupStep, type StepId, type StepStatus } from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import s from "./Sidebar.module.css";

const BRANCH_ICON_CLASSES: Record<string, string> = {
  main: s.branchIconMain,
  worktree: s.branchIconWorktree,
  question: s.branchIconQuestion,
  activity: s.branchIconActivity,
  unseen: s.branchIconUnseen,
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
 *  2. activity  → --activity  (pulsing)
 *  3. busy      → --activity  (pulsing)
 *  4. unseen    → --unseen    (static purple)
 *  5. idle      → --success
 *  6. base      → --warning (main) or --success (worktree)
 */
export const BranchIcon: Component<{
  isMainBranch: boolean;
  isMainWorktree: boolean;
  isShell?: boolean;
  hasQuestion?: boolean;
  hasActivity?: boolean;
  hasBusy?: boolean;
  hasUnseen?: boolean;
}> = (props) => {
  const iconShape = () => {
    if (props.hasQuestion) return "question";
    if (props.isShell) return "shell";
    if (props.isMainWorktree && props.isMainBranch) return "star";
    if (props.isMainWorktree) return "branch";
    return "worktree";
  };

  /** Single source of truth for icon color — priority cascade.
   *  Activity/busy override the base color; idle does NOT — the base
   *  color (yellow for main, green for worktree) is already correct
   *  when nothing special is happening. */
  const colorClass = () => {
    if (props.hasQuestion) return "question";
    if (props.hasActivity) return "activity";
    if (props.hasBusy) return "activity";
    if (props.hasUnseen) return "unseen";
    if (props.isMainBranch) return "main";
    return "worktree";
  };

  return (
    <span class={cx(s.branchIcon, BRANCH_ICON_CLASSES[colorClass()])}>
      {(() => {
        switch (iconShape()) {
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
  onCreateWorktreeFromBranch?: () => void;
  onMergeAndArchive?: () => void;
  onSwitchBranch?: (branchName: string) => void;
  switchBranchList?: () => string[];
  currentBranch?: () => string;
}> = (props) => {
  const ctxMenu = createContextMenu();

  const pr = createMemo(() => activePrStatus(props.repoPath, props.branch.name));
  const checks = createMemo(() => githubStore.getCheckSummary(props.repoPath, props.branch.name));

  // Sidebar icon reflects live shell state, not the "unread" activity flag.
  // A terminal that is shellState=idle should show idle even if activity=true
  // (the activity flag only means the tab hasn't been viewed yet).
  const hasActivity = () =>
    props.branch.terminals.some((id) => {
      const t = terminalsStore.get(id);
      return t?.activity && t.shellState !== "idle";
    });



  const hasQuestion = () =>
    props.branch.terminals.some((id) => terminalsStore.get(id)?.awaitingInput != null);

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
        await navigator.clipboard.writeText(path);
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
    return items;
  };

  return (
    <div
      class={cx(s.branchItem, props.isActive && s.active, hasActivity() && s.hasActivity)}
      onClick={props.onSelect}
      onContextMenu={ctxMenu.open}
    >
      <BranchIcon
        isMainBranch={props.branch.isMain}
        isMainWorktree={props.branch.worktreePath === props.repoPath}
        isShell={props.branch.isShell}
        hasQuestion={hasQuestion()}
        hasActivity={hasActivity()}
        hasBusy={hasBusy()}
        hasUnseen={hasUnseen()}
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
      <StatsBadge additions={props.branch.additions} deletions={props.branch.deletions} />
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

/** Whether a PR is eligible for merge: open, approved, CI all green */
export function canMergePr(pr: BranchPrStatus): boolean {
  return pr.state?.toUpperCase() === "OPEN"
    && !pr.is_draft
    && pr.review_decision === "APPROVED"
    && (pr.checks?.failed ?? 0) === 0
    && (pr.checks?.pending ?? 0) === 0;
}


/** Popover listing open PRs on remote-only branches with inline accordion detail */
export const RemoteOnlyPrPopover: Component<{
  prs: BranchPrStatus[];
  repoPath: string;
  onClose: () => void;
  onCheckout: (branchName: string) => void;
  onCreateWorktree?: (branchName: string) => void;
}> = (props) => {
  const [expandedBranch, setExpandedBranch] = createSignal<string | null>(null);
  const [mergingPr, setMergingPr] = createSignal<number | null>(null);
  const [mergeError, setMergeError] = createSignal<string | null>(null);
  const [diffLoading, setDiffLoading] = createSignal(false);
  const [approvingPr, setApprovingPr] = createSignal<number | null>(null);
  const [approveError, setApproveError] = createSignal<string | null>(null);
  const [dismissedPrs, setDismissedPrs] = createSignal<Set<number>>(new Set());

  // Post-merge cleanup state
  const [cleanupCtx, setCleanupCtx] = createSignal<{ branchName: string; baseBranch: string } | null>(null);
  const [cleanupExecuting, setCleanupExecuting] = createSignal(false);
  const [cleanupStepStatuses, setCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
  const [cleanupStepErrors, setCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});

  const cleanupIsOnBaseBranch = () => {
    const ctx = cleanupCtx();
    if (!ctx) return true; // remote-only: user is not on merged branch
    const repo = repositoriesStore.get(props.repoPath);
    return repo?.activeBranch === ctx.baseBranch;
  };

  const closeTerminalsForBranch = async (repoPath: string, branchName: string) => {
    const repo = repositoriesStore.get(repoPath);
    const branch = repo?.branches[branchName];
    if (branch) {
      for (const termId of branch.terminals) {
        await invoke("close_pty", { id: termId });
      }
    }
  };

  const handleCleanupExecute = async (steps: CleanupStep[]) => {
    const ctx = cleanupCtx();
    if (!ctx) return;
    setCleanupExecuting(true);
    setCleanupStepStatuses({});
    setCleanupStepErrors({});

    await executeCleanup({
      repoPath: props.repoPath,
      branchName: ctx.branchName,
      baseBranch: ctx.baseBranch,
      steps: steps.map((st) => ({ id: st.id, checked: st.checked })),
      closeTerminalsForBranch,
      onStepStart: (id) => {
        setCleanupStepStatuses((prev) => ({ ...prev, [id]: "running" }));
      },
      onStepDone: (id, result, error) => {
        setCleanupStepStatuses((prev) => ({ ...prev, [id]: result }));
        if (error) setCleanupStepErrors((prev) => ({ ...prev, [id]: error }));
      },
    });

    setCleanupExecuting(false);
    setTimeout(() => {
      setCleanupCtx(null);
      props.onClose();
    }, 600);
  };

  const handleCleanupSkip = () => {
    setCleanupCtx(null);
  };

  const visiblePrs = createMemo(() =>
    props.prs.filter((pr) => !dismissedPrs().has(pr.number)),
  );

  const dismissedCount = createMemo(() => dismissedPrs().size);

  const handleDismiss = (prNumber: number) => {
    setDismissedPrs((prev) => {
      const next = new Set(prev);
      next.add(prNumber);
      return next;
    });
  };

  const handleShowDismissed = () => {
    setDismissedPrs(new Set<number>());
  };

  const handleRowClick = (branch: string) => {
    setExpandedBranch((prev) => prev === branch ? null : branch);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      if (expandedBranch()) {
        setExpandedBranch(null);
      } else {
        props.onClose();
      }
    }
  };

  const handleMerge = async (pr: BranchPrStatus) => {
    setMergingPr(pr.number);
    setMergeError(null);
    try {
      const preferred = repoSettingsStore.getEffective(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
      const startMethod = effectiveMergeMethod(pr, preferred);
      const usedMethod = await mergeWithFallback(props.repoPath, pr.number, startMethod);
      // Persist the working method so future merges use it directly
      if (usedMethod !== preferred) {
        const repo = repositoriesStore.get(props.repoPath);
        repoSettingsStore.getOrCreate(props.repoPath, repo?.displayName ?? props.repoPath);
        repoSettingsStore.update(props.repoPath, { prMergeStrategy: usedMethod });
      }
      appLogger.info("github", `Merged PR #${pr.number} via ${usedMethod}`);
      githubStore.pollRepo(props.repoPath);

      // Show cleanup dialog — for remote-only PRs the user is not on the branch
      const baseBranch = pr.base_ref_name || "main";
      setCleanupCtx({ branchName: pr.branch, baseBranch });
    } catch (e) {
      const msg = String(e);
      setMergeError(msg);
      appLogger.error("github", `Failed to merge PR #${pr.number}`, { error: msg });
    } finally {
      setMergingPr(null);
    }
  };

  const mergeLabel = (pr: BranchPrStatus) => {
    const preferred = repoSettingsStore.getEffective(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
    const method = effectiveMergeMethod(pr, preferred);
    if (method === "squash") return t("sidebar.mergeSquash", "Squash & Merge");
    if (method === "rebase") return t("sidebar.mergeRebase", "Rebase & Merge");
    return t("sidebar.merge", "Merge");
  };

  const handleApprove = async (pr: BranchPrStatus) => {
    setApprovingPr(pr.number);
    setApproveError(null);
    try {
      await invoke("approve_pr", {
        repoPath: props.repoPath,
        prNumber: pr.number,
      });
      appLogger.info("github", `Approved PR #${pr.number}`);
      githubStore.pollRepo(props.repoPath);
    } catch (e) {
      const msg = String(e);
      setApproveError(msg);
      appLogger.error("github", `Failed to approve PR #${pr.number}`, { error: msg });
    } finally {
      setApprovingPr(null);
    }
  };

  const handleViewDiff = async (pr: BranchPrStatus) => {
    setDiffLoading(true);
    try {
      const diff = await invoke<string>("get_pr_diff", {
        repoPath: props.repoPath,
        prNumber: pr.number,
      });
      mdTabsStore.addPrDiff(props.repoPath, pr.number, pr.title, diff);
    } catch (e) {
      appLogger.error("github", `Failed to load PR #${pr.number} diff`, { error: String(e) });
    } finally {
      setDiffLoading(false);
    }
  };

  return (
    <>
      <Show when={cleanupCtx()}>
        {(ctx) => (
          <PostMergeCleanupDialog
            branchName={ctx().branchName}
            baseBranch={ctx().baseBranch}
            repoPath={props.repoPath}
            isOnBaseBranch={cleanupIsOnBaseBranch()}
            isDefaultBranch={false}
            hasTerminals={false}
            onExecute={handleCleanupExecute}
            onSkip={handleCleanupSkip}
            executing={cleanupExecuting()}
            stepStatuses={cleanupStepStatuses()}
            stepErrors={cleanupStepErrors()}
          />
        )}
      </Show>
      <Show when={!cleanupCtx()}>
      <div class={s.remoteOnlyOverlay} onClick={props.onClose} onKeyDown={handleKeyDown} tabIndex={-1} />
      <div class={s.remoteOnlyPopover} onKeyDown={handleKeyDown} tabIndex={-1}>
        <div class={s.remoteOnlyHeader}>
          <span>{t("sidebar.remoteOnlyPrs", "Remote-only PRs")}</span>
          <Show when={dismissedCount() > 0}>
            <button class={s.remoteOnlyShowDismissed} onClick={handleShowDismissed}>
              {t("sidebar.showDismissed", "Show")} {dismissedCount()} {t("sidebar.dismissed", "dismissed")}
            </button>
          </Show>
          <button class={s.remoteOnlyClose} onClick={props.onClose}>&times;</button>
        </div>
        <div class={s.remoteOnlyList}>
          <For each={visiblePrs()}>
            {(pr) => (
              <div class={cx(s.remoteOnlyItem, expandedBranch() === pr.branch && s.remoteOnlyItemExpanded)}>
                <div class={s.remoteOnlyRow} onClick={() => handleRowClick(pr.branch)}>
                  <span class={s.remoteOnlyNum}>#{pr.number}</span>
                  <span class={s.remoteOnlyTitle}>{pr.title}</span>
                  <PrStateBadge
                    prNumber={pr.number}
                    state={pr.state}
                    isDraft={pr.is_draft}
                    mergeable={pr.mergeable}
                    reviewDecision={pr.review_decision}
                    ciFailed={pr.checks?.failed}
                    ciPending={pr.checks?.pending}
                  />
                </div>
                <Show when={expandedBranch() === pr.branch}>
                  <div class={s.remoteOnlyDetail}>
                    <PrDetailContent repoPath={props.repoPath} branch={pr.branch}>
                      <div class={s.remoteOnlyDetailActions}>
                        <button
                          class={s.remoteOnlyCheckout}
                          onClick={() => props.onCheckout(pr.branch)}
                          title={t("sidebar.checkoutBranch", "Check out this branch locally")}
                        >
                          {t("sidebar.checkout", "Checkout")}
                        </button>
                        <Show when={props.onCreateWorktree}>
                          <button
                            class={s.remoteOnlyWorktree}
                            onClick={() => props.onCreateWorktree?.(pr.branch)}
                            title={t("sidebar.createWorktreeFromBranch", "Create worktree from this branch")}
                          >
                            {t("sidebar.worktree", "Worktree")}
                          </button>
                        </Show>
                        <Show when={pr.state?.toUpperCase() === "OPEN" && !pr.is_draft && pr.review_decision !== "APPROVED"}>
                          <button
                            class={s.remoteOnlyApprove}
                            onClick={() => handleApprove(pr)}
                            disabled={approvingPr() === pr.number}
                            title={t("sidebar.approvePr", "Approve this pull request")}
                          >
                            {approvingPr() === pr.number
                              ? t("sidebar.approving", "Approving...")
                              : t("sidebar.approve", "Approve")}
                          </button>
                        </Show>
                        <Show when={canMergePr(pr)}>
                          <button
                            class={s.remoteOnlyMerge}
                            onClick={() => handleMerge(pr)}
                            disabled={mergingPr() === pr.number}
                            title={t("sidebar.mergePr", "Merge this pull request")}
                          >
                            {mergingPr() === pr.number
                              ? t("sidebar.merging", "Merging...")
                              : mergeLabel(pr)}
                          </button>
                        </Show>
                        <button
                          class={s.remoteOnlyViewDiff}
                          onClick={() => handleViewDiff(pr)}
                          disabled={diffLoading()}
                          title={t("sidebar.viewDiff", "View PR diff")}
                        >
                          {diffLoading()
                            ? t("sidebar.loadingDiff", "Loading...")
                            : t("sidebar.viewDiff", "View Diff")}
                        </button>
                        <button
                          class={s.remoteOnlyDismiss}
                          onClick={() => handleDismiss(pr.number)}
                          title={t("sidebar.dismissPr", "Hide this PR from view")}
                        >
                          {t("sidebar.dismiss", "Dismiss")}
                        </button>
                      </div>
                      <Show when={approveError()}>
                        <div class={s.remoteOnlyMergeError}>{approveError()}</div>
                      </Show>
                      <Show when={mergeError()}>
                        <div class={s.remoteOnlyMergeError}>{mergeError()}</div>
                      </Show>
                    </PrDetailContent>
                  </div>
                </Show>
              </div>
            )}
          </For>
        </div>
      </div>
      </Show>
    </>
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
  const canRemoveAny = createMemo(() => sortedBranches().length > 1);

  const localBranchNames = createMemo(() => new Set(Object.keys(props.repo.branches)));
  const remoteOnlyPrs = createMemo(() =>
    githubStore.getRemoteOnlyPrs(props.repo.path, localBranchNames()),
  );

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
            <Show when={remoteOnlyPrs().length > 0}>
              <button
                class={cx(s.repoActionBtn, s.remoteOnlyBadgeBtn)}
                onClick={(e) => { e.stopPropagation(); setRemoteOnlyPopoverVisible((v) => !v); }}
                title={t("sidebar.remoteOnlyPrsTitle", "Open PRs on remote-only branches")}
              >
                {remoteOnlyPrs().length}
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
                onCreateWorktreeFromBranch={props.onCreateWorktreeFromBranch ? () => props.onCreateWorktreeFromBranch!(branch.name) : undefined}
                onMergeAndArchive={props.onMergeAndArchive ? () => props.onMergeAndArchive!(branch.name) : undefined}
                onSwitchBranch={branch.worktreePath === props.repo.path ? (name) => props.onSwitchBranch(name) : undefined}
                switchBranchList={branch.worktreePath === props.repo.path ? props.switchBranchList : undefined}
                currentBranch={branch.worktreePath === props.repo.path ? props.currentBranch : undefined}
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
      <Show when={remoteOnlyPopoverVisible() && remoteOnlyPrs().length > 0}>
        <RemoteOnlyPrPopover
          prs={remoteOnlyPrs()}
          repoPath={props.repo.path}
          onClose={() => setRemoteOnlyPopoverVisible(false)}
          onCheckout={(branch) => {
            setRemoteOnlyPopoverVisible(false);
            props.onCheckoutRemoteBranch?.(branch);
          }}
          onCreateWorktree={props.onCreateWorktreeFromBranch}
        />
      </Show>
    </div>
  );
};
