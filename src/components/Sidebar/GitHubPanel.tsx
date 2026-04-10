import { Component, For, Show, createEffect, createMemo, createSignal, onMount } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { githubStore } from "../../stores/github";
import { appLogger } from "../../stores/appLogger";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repoSettingsStore } from "../../stores/repoSettings";
import { effectiveMergeMethod, mergeWithFallback } from "../../utils/prMerge";
import { invoke } from "../../invoke";
import { cx } from "../../utils";
import { t } from "../../i18n";
import { PrDetailContent } from "../PrDetailPopover/PrDetailContent";
import { IssueDetailContent } from "../IssueDetailPopover/IssueDetailContent";
import { mdTabsStore } from "../../stores/mdTabs";
import { PostMergeCleanupDialog, type CleanupStep, type StepId, type StepStatus } from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import { PrStateBadge } from "./RepoSection";
import type { BranchPrStatus, GitHubIssue, IssueFilterMode } from "../../types";
import s from "./Sidebar.module.css";

/** Whether a PR is eligible for merge: open, approved, CI all green */
function canMergePr(pr: BranchPrStatus): boolean {
  return pr.state?.toUpperCase() === "OPEN"
    && !pr.is_draft
    && pr.review_decision === "APPROVED"
    && (pr.checks?.failed ?? 0) === 0
    && (pr.checks?.pending ?? 0) === 0;
}

const FILTER_OPTIONS: { value: IssueFilterMode; label: string }[] = [
  { value: "assigned", label: "Assigned" },
  { value: "created", label: "Created" },
  { value: "mentioned", label: "Mentioned" },
  { value: "all", label: "All open" },
];

/** Unified GitHub panel showing remote-only PRs and Issues */
export const GitHubPanel: Component<{
  prs: BranchPrStatus[];
  repoPath: string;
  onClose: () => void;
  onCheckout: (branchName: string) => void;
  onCreateWorktree?: (branchName: string) => void;
  onCleanupActive?: (active: boolean) => void;
}> = (props) => {
  // Section collapse state — persisted in localStorage
  const storageKey = `github-panel:${props.repoPath}`;
  const loadCollapsed = () => {
    try {
      const raw = localStorage.getItem(storageKey);
      if (raw) return JSON.parse(raw) as { prs: boolean; issues: boolean };
    } catch { /* ignore */ }
    return { prs: false, issues: false };
  };
  const [prCollapsed, setPrCollapsed] = createSignal(loadCollapsed().prs);
  const [issuesCollapsed, setIssuesCollapsed] = createSignal(loadCollapsed().issues);

  createEffect(() => {
    try {
      localStorage.setItem(storageKey, JSON.stringify({ prs: prCollapsed(), issues: issuesCollapsed() }));
    } catch { /* ignore */ }
  });

  // PR accordion state
  const [expandedPr, setExpandedPr] = createSignal<string | null>(null);
  const [mergingPr, setMergingPr] = createSignal<number | null>(null);
  const [mergeError, setMergeError] = createSignal<string | null>(null);
  const [diffLoading, setDiffLoading] = createSignal(false);
  const [approvingPr, setApprovingPr] = createSignal<number | null>(null);
  const [approveError, setApproveError] = createSignal<string | null>(null);
  const [dismissedPrs, setDismissedPrs] = createSignal<Set<number>>(new Set());

  // Issue accordion state
  const [expandedIssue, setExpandedIssue] = createSignal<number | null>(null);

  // Post-merge cleanup state
  const [cleanupCtx, setCleanupCtx] = createSignal<{ branchName: string; baseBranch: string; hasDirtyFiles: boolean } | null>(null);
  const [cleanupExecuting, setCleanupExecuting] = createSignal(false);
  const [cleanupStepStatuses, setCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
  const [cleanupStepErrors, setCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});

  createEffect(() => {
    props.onCleanupActive?.(!!cleanupCtx());
  });

  const cleanupIsOnBaseBranch = () => {
    const ctx = cleanupCtx();
    if (!ctx) return true;
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

  const issues = createMemo(() => githubStore.getRepoIssues(props.repoPath));
  const issuesLoading = () => githubStore.state.issuesLoading;
  const circuitOpen = () => githubStore.state.circuitBreakerOpen;

  // Collect all item keys for keyboard navigation
  const allItemKeys = createMemo(() => {
    const keys: { type: "pr"; key: string }[] | { type: "issue"; key: number }[] = [];
    if (!prCollapsed()) {
      for (const pr of visiblePrs()) keys.push({ type: "pr", key: pr.branch } as never);
    }
    if (!issuesCollapsed()) {
      for (const issue of issues()) keys.push({ type: "issue", key: issue.number } as never);
    }
    return keys as ({ type: "pr"; key: string } | { type: "issue"; key: number })[];
  });

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

  // Track focused item index for keyboard nav
  const [focusedIdx, setFocusedIdx] = createSignal(-1);

  const handleKeyDown = (e: KeyboardEvent) => {
    const items = allItemKeys();
    if (e.key === "Escape") {
      if (expandedPr() || expandedIssue()) {
        setExpandedPr(null);
        setExpandedIssue(null);
      } else {
        props.onClose();
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocusedIdx((prev) => Math.min(prev + 1, items.length - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocusedIdx((prev) => Math.max(prev - 1, 0));
      return;
    }
    if (e.key === "Enter") {
      const idx = focusedIdx();
      if (idx >= 0 && idx < items.length) {
        const item = items[idx];
        if (item.type === "pr") {
          setExpandedPr((prev) => prev === item.key ? null : item.key);
          setExpandedIssue(null);
        } else {
          setExpandedIssue((prev) => prev === item.key ? null : item.key);
          setExpandedPr(null);
        }
      }
      return;
    }
  };

  const handleMerge = async (pr: BranchPrStatus) => {
    setMergingPr(pr.number);
    setMergeError(null);
    try {
      const preferred = repoSettingsStore.getEffective(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
      const startMethod = effectiveMergeMethod(pr, preferred);
      const usedMethod = await mergeWithFallback(props.repoPath, pr.number, startMethod);
      if (usedMethod !== preferred) {
        const repo = repositoriesStore.get(props.repoPath);
        repoSettingsStore.getOrCreate(props.repoPath, repo?.displayName ?? props.repoPath);
        repoSettingsStore.update(props.repoPath, { prMergeStrategy: usedMethod });
      }
      appLogger.info("github", `Merged PR #${pr.number} via ${usedMethod}`);
      githubStore.pollRepo(props.repoPath);

      const baseBranch = pr.base_ref_name || "main";
      let hasDirtyFiles = false;
      try {
        const status = await invoke<{ stdout: string }>("run_git_command", {
          path: props.repoPath,
          args: ["status", "--porcelain"],
        });
        hasDirtyFiles = status.stdout.trim().length > 0;
      } catch { /* ignore */ }
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
        <div class={s.ghPanelOverlay} onClick={props.onClose} onKeyDown={handleKeyDown} tabIndex={-1} />
        <div class={s.ghPanel} onKeyDown={handleKeyDown} tabIndex={-1} ref={(el) => onMount(() => el.focus())}>
          {/* Rate limit warning */}
          <Show when={circuitOpen()}>
            <div class={s.ghRateLimitBanner}>
              <span>{t("github.rateLimited", "GitHub API unavailable")}</span>
              <button class={s.ghRetryBtn} onClick={() => { githubStore.pollIssues(); }}>
                {t("github.retry", "Retry")}
              </button>
            </div>
          </Show>
          {/* Panel header */}
          <div class={s.ghPanelHeader}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path fill-rule="evenodd" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
            </svg>
            <span>GitHub</span>
            <button class={s.ghPanelClose} onClick={props.onClose}>&times;</button>
          </div>

          <div class={s.ghPanelBody}>
            {/* ── Pull Requests section ── */}
            <div class={s.ghSection}>
              <div
                class={s.ghSectionHeader}
                onClick={() => setPrCollapsed((v) => !v)}
              >
                <span class={cx(s.ghSectionChevron, !prCollapsed() && s.ghSectionChevronOpen)}>{"\u203A"}</span>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M5.45 5.154A4.25 4.25 0 0 0 9.25 7.5h1.378a2.251 2.251 0 1 1 0 1.5H9.25A5.734 5.734 0 0 1 5 7.123v3.505a2.25 2.25 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.95-.218ZM4.25 13.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm8-8a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5ZM4.25 4a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Z"/>
                </svg>
                <span>{t("github.pullRequests", "Pull Requests")}</span>
                <Show when={visiblePrs().length > 0}>
                  <span class={s.ghSectionCount}>{visiblePrs().length}</span>
                </Show>
                <Show when={dismissedCount() > 0}>
                  <button class={s.ghShowDismissed} onClick={(e) => { e.stopPropagation(); handleShowDismissed(); }}>
                    {t("sidebar.showDismissed", "Show")} {dismissedCount()}
                  </button>
                </Show>
              </div>
              <Show when={!prCollapsed()}>
                <Show when={visiblePrs().length > 0} fallback={
                  <div class={s.ghEmpty}>{t("github.noPrs", "No remote-only PRs")}</div>
                }>
                  <div class={s.ghSectionList}>
                    <For each={visiblePrs()}>
                      {(pr, prIdx) => {
                        const itemIdx = () => prIdx();
                        return (
                        <div class={cx(s.ghItem, expandedPr() === pr.branch && s.ghItemExpanded)}>
                          <div class={cx(s.ghItemRow, focusedIdx() === itemIdx() && s.ghItemFocused)} onClick={() => setExpandedPr((prev) => prev === pr.branch ? null : pr.branch)}>
                            <span class={s.ghItemNum}>#{pr.number}</span>
                            <span class={s.ghItemTitle}>{pr.title}</span>
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
                          <Show when={expandedPr() === pr.branch}>
                            <div class={s.ghItemDetail}>
                              <button
                                class={s.ghItemDismiss}
                                onClick={() => handleDismiss(pr.number)}
                                title={t("sidebar.dismissPr", "Hide this PR from view")}
                              >
                                &times;
                              </button>
                              <PrDetailContent repoPath={props.repoPath} branch={pr.branch}>
                                <div class={s.ghItemActions}>
                                  <button
                                    class={s.ghActionBtn}
                                    onClick={() => props.onCheckout(pr.branch)}
                                    title={t("sidebar.checkoutBranch", "Check out this branch locally")}
                                  >
                                    {t("sidebar.checkout", "Checkout")}
                                  </button>
                                  <Show when={props.onCreateWorktree}>
                                    <button
                                      class={s.ghActionBtn}
                                      onClick={() => props.onCreateWorktree?.(pr.branch)}
                                      title={t("sidebar.createWorktreeFromBranch", "Create worktree from this branch")}
                                    >
                                      {t("sidebar.worktree", "Worktree")}
                                    </button>
                                  </Show>
                                  <Show when={pr.state?.toUpperCase() === "OPEN" && !pr.is_draft && pr.review_decision !== "APPROVED"}>
                                    <button
                                      class={cx(s.ghActionBtn, s.ghApproveBtn)}
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
                                      class={cx(s.ghActionBtn, s.ghMergeBtn)}
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
                                    class={s.ghActionBtn}
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
                                  <div class={s.ghActionError}>{approveError()}</div>
                                </Show>
                                <Show when={mergeError()}>
                                  <div class={s.ghActionError}>{mergeError()}</div>
                                </Show>
                              </PrDetailContent>
                            </div>
                          </Show>
                        </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </Show>
            </div>

            {/* ── Issues section ── */}
            <div class={s.ghSection}>
              <div
                class={s.ghSectionHeader}
                onClick={() => setIssuesCollapsed((v) => !v)}
              >
                <span class={cx(s.ghSectionChevron, !issuesCollapsed() && s.ghSectionChevronOpen)}>{"\u203A"}</span>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z"/>
                  <path d="M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Z"/>
                </svg>
                <span>{t("github.issues", "Issues")}</span>
                <Show when={issues().length > 0}>
                  <span class={s.ghSectionCount}>{issues().length}</span>
                </Show>
              </div>
              <Show when={!issuesCollapsed()}>
                {/* Loading skeleton */}
                <Show when={issuesLoading() && issues().length === 0}>
                  <div class={s.ghSectionList}>
                    <div class={s.ghSkeletonRow}><div class={s.ghSkeletonBar} /><div class={s.ghSkeletonBarShort} /></div>
                    <div class={s.ghSkeletonRow}><div class={s.ghSkeletonBar} /><div class={s.ghSkeletonBarShort} /></div>
                    <div class={s.ghSkeletonRow}><div class={s.ghSkeletonBar} /><div class={s.ghSkeletonBarShort} /></div>
                  </div>
                </Show>
                <Show when={!issuesLoading() || issues().length > 0}>
                <Show when={issues().length > 0} fallback={
                  <div class={s.ghEmpty}>{t("github.noIssues", "No issues found")}</div>
                }>
                  <div class={s.ghSectionList}>
                    <For each={issues()}>
                      {(issue: GitHubIssue, issueIdx) => {
                        const itemIdx = () => visiblePrs().length + issueIdx();
                        return (
                        <div class={cx(s.ghItem, expandedIssue() === issue.number && s.ghItemExpanded)}>
                          <div class={cx(s.ghItemRow, focusedIdx() === itemIdx() && s.ghItemFocused)} onClick={() => setExpandedIssue((prev) => prev === issue.number ? null : issue.number)}>
                            <span class={s.ghItemNum}>#{issue.number}</span>
                            <span class={s.ghItemTitle}>{issue.title}</span>
                            <span class={cx(s.ghIssueBadge, issue.state?.toUpperCase() === "OPEN" ? s.ghIssueOpen : s.ghIssueClosed)}>
                              {issue.state?.toUpperCase() === "OPEN" ? "Open" : "Closed"}
                            </span>
                          </div>
                          <Show when={expandedIssue() === issue.number}>
                            <div class={s.ghItemDetail}>
                              <IssueDetailContent issue={issue} repoPath={props.repoPath} />
                            </div>
                          </Show>
                        </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
                </Show>

                {/* Filter bar */}
                <div class={s.ghFilterBar}>
                  <select
                    class={s.ghFilterSelect}
                    value={githubStore.state.issueFilter}
                    onChange={(e) => githubStore.setIssueFilter(e.currentTarget.value as IssueFilterMode)}
                  >
                    <For each={FILTER_OPTIONS}>
                      {(opt) => <option value={opt.value}>{opt.label}</option>}
                    </For>
                  </select>
                </div>
              </Show>
            </div>
          </div>
        </div>
      </Show>
    </>
  );
};
