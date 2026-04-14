import { Component, For, Show, createEffect, createMemo, createSignal } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { githubStore } from "../../stores/github";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repoSettingsStore } from "../../stores/repoSettings";
import { effectiveMergeMethod, mergeWithFallback } from "../../utils/prMerge";
import { invoke } from "../../invoke";
import { cx } from "../../utils";
import { t } from "../../i18n";
import { PrDetailContent } from "../PrDetailPopover/PrDetailContent";
import { mdTabsStore } from "../../stores/mdTabs";
import { PostMergeCleanupDialog, type CleanupStep, type StepId, type StepStatus } from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import { PrStateBadge } from "./RepoSection";
import type { BranchPrStatus } from "../../types";
import s from "./Sidebar.module.css";

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
  /** Notify parent when post-merge cleanup is active (prevents unmount during cleanup) */
  onCleanupActive?: (active: boolean) => void;
}> = (props) => {
  const [expandedBranch, setExpandedBranch] = createSignal<string | null>(null);
  const [mergingPr, setMergingPr] = createSignal<number | null>(null);
  const [mergeError, setMergeError] = createSignal<string | null>(null);
  const [diffLoading, setDiffLoading] = createSignal(false);
  const [approvingPr, setApprovingPr] = createSignal<number | null>(null);
  const [approveError, setApproveError] = createSignal<string | null>(null);
  const [dismissedPrs, setDismissedPrs] = createSignal<Set<number>>(new Set());

  // Post-merge cleanup state
  const [cleanupCtx, setCleanupCtx] = createSignal<{ branchName: string; baseBranch: string; hasDirtyFiles: boolean } | null>(null);
  const [cleanupExecuting, setCleanupExecuting] = createSignal(false);
  const [cleanupStepStatuses, setCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
  const [cleanupStepErrors, setCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});

  // Notify parent when cleanup dialog is active so it keeps us mounted
  createEffect(() => {
    props.onCleanupActive?.(!!cleanupCtx());
  });

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
        try {
          await invoke("close_pty", { sessionId: termId, cleanupWorktree: false });
        } catch (err) {
          appLogger.warn("git", `close_pty failed for terminal ${termId}`, err);
        }
      }
    }
  };

  const handleCleanupExecute = async (steps: CleanupStep[], options?: { unstash?: boolean }) => {
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
      unstash: options?.unstash,
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

      // Show cleanup dialog — pre-check dirty state for stash UX
      const baseBranch = pr.base_ref_name || "main";
      let hasDirtyFiles = false;
      try {
        const status = await invoke<{ stdout: string }>("run_git_command", {
          path: props.repoPath,
          args: ["status", "--porcelain"],
        });
        hasDirtyFiles = status.stdout.trim().length > 0;
      } catch { /* ignore — assume clean */ }
      setCleanupCtx({ branchName: pr.branch, baseBranch, hasDirtyFiles });
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
      const msg = String(e);
      appLogger.error("github", `Failed to load PR #${pr.number} diff`, { error: msg });
      toastsStore.add(`PR #${pr.number} diff failed`, msg.includes("too_large") ? "Diff too large (>300 files)" : msg, "error");
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
            hasDirtyFiles={ctx().hasDirtyFiles}
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
                    <button
                      class={s.remoteOnlyDetailDismiss}
                      onClick={() => handleDismiss(pr.number)}
                      title={t("sidebar.dismissPr", "Hide this PR from view")}
                    >
                      &times;
                    </button>
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
                            : t("sidebar.diff", "Diff")}
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
