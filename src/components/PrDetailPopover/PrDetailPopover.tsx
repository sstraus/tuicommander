import { Component, Show, createMemo, createSignal, onMount, onCleanup } from "solid-js";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { getRepoColor } from "../../utils/repoColor";
import { mdTabsStore } from "../../stores/mdTabs";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { terminalsStore } from "../../stores/terminals";
import { appLogger } from "../../stores/appLogger";
import { invoke } from "../../invoke";
import { canMergePr, effectiveMergeMethod } from "../Sidebar/RepoSection";
import { mergeWithFallback, isAlreadyMerged } from "../../utils/prMerge";
import { interpolateTemplate } from "../../utils/templateInterpolation";
import { handleOpenUrl } from "../../utils/openUrl";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { PrDetailContent } from "./PrDetailContent";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { PostMergeCleanupDialog, type CleanupStep, type StepId, type StepStatus } from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import type { AgentType } from "../../agents";
import s from "./PrDetailPopover.module.css";

/** Extract "owner/repo" from a GitHub PR URL, e.g. https://github.com/owner/repo/pull/67 */
function extractGithubRepo(url: string): string | null {
  try {
    const parts = new URL(url).pathname.split("/").filter(Boolean);
    if (parts.length >= 2) return `${parts[0]}/${parts[1]}`;
  } catch { /* ignore malformed URL */ }
  return null;
}

/** Map backend PR state strings to CSS module classes */
const STATE_CLASSES: Record<string, string> = {
  open: s.open,
  merged: s.merged,
  closed: s.closed,
  draft: s.draft,
};

export interface PrDetailPopoverProps {
  repoPath: string;
  branch: string;
  onClose: () => void;
  /** Anchor to top-right (toolbar) or bottom-right (status bar, default) */
  anchor?: "top" | "bottom";
  /** Called when user clicks "Review" — provides the interpolated command string */
  onReview?: (repoPath: string, branch: string, command: string) => void;
}

/** Rich PR detail popover showing PR metadata, diff stats, and CI checks */
export const PrDetailPopover: Component<PrDetailPopoverProps> = (props) => {
  const prData = () => githubStore.getBranchPrData(props.repoPath, props.branch);
  const [diffLoading, setDiffLoading] = createSignal(false);
  const [merging, setMerging] = createSignal(false);
  const [mergeError, setMergeError] = createSignal<string | null>(null);

  // Post-merge cleanup dialog state
  const [cleanupCtx, setCleanupCtx] = createSignal<{ branchName: string; baseBranch: string; hasDirtyFiles: boolean } | null>(null);
  const [cleanupExecuting, setCleanupExecuting] = createSignal(false);
  const [cleanupStepStatuses, setCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
  const [cleanupStepErrors, setCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});

  /** Find a "review" run config from the branch's detected agent type */
  const reviewCommand = createMemo(() => {
    if (!props.onReview) return null;
    const p = prData();
    if (!p || p.state === "merged" || p.state === "closed") return null;

    const repo = repositoriesStore.get(props.repoPath);
    const branch = repo?.branches[props.branch];
    if (!branch?.terminals?.length) return null;

    // Find first terminal with a detected agent
    let agentType: AgentType | null = null;
    for (const termId of branch.terminals) {
      const term = terminalsStore.get(termId);
      if (term?.agentType) { agentType = term.agentType; break; }
    }
    if (!agentType) return null;

    const configs = agentConfigsStore.getRunConfigs(agentType);
    const reviewCfg = configs.find((c) => c.name.toLowerCase() === "review");
    if (!reviewCfg) return null;

    // Build the interpolated command
    const vars = {
      pr_number: String(p.number),
      branch: props.branch,
      base_branch: p.base_ref_name ?? null,
      repo: extractGithubRepo(p.url) ?? null,
      pr_url: p.url ?? null,
    };
    const args = reviewCfg.args.map((a) => interpolateTemplate(a, vars)).join(" ");
    return `${reviewCfg.command}${args ? " " + args : ""}`;
  });

  const isOnBaseBranch = () => {
    const ctx = cleanupCtx();
    if (!ctx) return false;
    const repo = repositoriesStore.get(props.repoPath);
    return repo?.activeBranch === ctx.baseBranch;
  };

  const hasTerminals = () => {
    const ctx = cleanupCtx();
    if (!ctx) return false;
    const repo = repositoriesStore.get(props.repoPath);
    const branch = repo?.branches[ctx.branchName];
    return (branch?.terminals?.length ?? 0) > 0;
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
      steps: steps.map((s) => ({ id: s.id, checked: s.checked })),
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
    // Auto-close after a short delay so user can see final statuses
    setTimeout(() => {
      setCleanupCtx(null);
      props.onClose();
    }, 600);
  };

  const handleCleanupSkip = () => {
    setCleanupCtx(null);
    props.onClose();
  };

  const handleMerge = async () => {
    const pr = prData();
    if (!pr) return;
    setMerging(true);
    setMergeError(null);
    try {
      const preferred = repoSettingsStore.getEffective?.(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
      const startMethod = effectiveMergeMethod(pr, preferred);
      try {
        const usedMethod = await mergeWithFallback(props.repoPath, pr.number, startMethod);
        // Persist the working method so future merges use it directly
        if (usedMethod !== preferred) {
          const repo = repositoriesStore.get(props.repoPath);
          repoSettingsStore.getOrCreate(props.repoPath, repo?.displayName ?? props.repoPath);
          repoSettingsStore.update(props.repoPath, { prMergeStrategy: usedMethod });
        }
        appLogger.info("github", `Merged PR #${pr.number} via ${usedMethod}`);
      } catch (mergeErr) {
        if (isAlreadyMerged(mergeErr)) {
          appLogger.info("github", `PR #${pr.number} was already merged — proceeding to cleanup`);
        } else {
          throw mergeErr;
        }
      }
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
      setCleanupCtx({ branchName: props.branch, baseBranch, hasDirtyFiles });
      // Poll AFTER cleanup state is established — polling earlier could trigger
      // reactive store updates that attempt to close the popover before the
      // cleanup dialog is mounted.
      githubStore.pollRepo(props.repoPath);
    } catch (e) {
      setMergeError(String(e));
      appLogger.error("github", `Failed to merge PR #${pr.number}`, { error: String(e) });
    } finally {
      setMerging(false);
    }
  };

  const mergeLabel = () => {
    const pr = prData();
    if (!pr) return t("prDetail.merge", "Merge");
    const preferred = repoSettingsStore.getEffective?.(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
    const method = effectiveMergeMethod(pr, preferred);
    if (method === "squash") return t("prDetail.mergeSquash", "Squash & Merge");
    if (method === "rebase") return t("prDetail.mergeRebase", "Rebase & Merge");
    return t("prDetail.merge", "Merge");
  };

  const handleViewDiff = async () => {
    const pr = prData();
    if (!pr) return;
    setDiffLoading(true);
    try {
      const diff = await invoke<string>("get_pr_diff", {
        repoPath: props.repoPath,
        prNumber: pr.number,
      });
      mdTabsStore.addPrDiff(props.repoPath, pr.number, pr.title, diff);
      props.onClose();
    } catch (e) {
      appLogger.error("github", `Failed to load PR #${pr.number} diff`, { error: String(e) });
    } finally {
      setDiffLoading(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onClose();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  const repoColor = createMemo(() => getRepoColor(props.repoPath));

  const stateClass = () => {
    if (prData()?.is_draft) return "draft";
    const state = prData()?.state?.toUpperCase();
    switch (state) {
      case "MERGED": return "merged";
      case "CLOSED": return "closed";
      default: return "open";
    }
  };

  const stateLabel = () => {
    if (prData()?.is_draft) return "Draft";
    return prData()?.state || "";
  };

  return (
    <>
      {/* Post-merge cleanup dialog (replaces the popover after merge) */}
      <Show when={cleanupCtx()}>
        {(ctx) => (
          <PostMergeCleanupDialog
            branchName={ctx().branchName}
            baseBranch={ctx().baseBranch}
            repoPath={props.repoPath}
            isOnBaseBranch={isOnBaseBranch()}
            isDefaultBranch={false}
            hasTerminals={hasTerminals()}
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
      <div class={s.overlay} onClick={props.onClose} />
      <div class={cx(s.popover, props.anchor === "top" && s.anchorTop)}>
        <Show when={prData()} fallback={
          <div class={s.empty}>{t("prDetail.noData", "No PR data available for")} {props.branch}</div>
        }>
          {(pr) => (
            <>
              {/* Repo label: GitHub owner/repo (from PR url) with optional repo color */}
              <div
                class={s.repo}
                style={repoColor() ? { color: repoColor() } : undefined}
              >
                {extractGithubRepo(pr().url)
                  ?? repositoriesStore.get(props.repoPath)?.displayName
                  ?? props.repoPath.split("/").pop()}
              </div>

              {/* Header: state badge + title + number */}
              <div class={s.header}>
                <span class={cx(s.stateBadge, STATE_CLASSES[stateClass()])}>{stateLabel()}</span>
                <span class={s.title}>{pr().title}</span>
                <span
                  class={cx(s.number, s.link)}
                  onClick={() => pr().url && handleOpenUrl(pr().url)}
                  title={t("prDetail.openOnGithub", "Open PR on GitHub")}
                >
                  #{pr().number}
                </span>
                <button class={s.close} onClick={props.onClose}>&times;</button>
              </div>

              {/* Shared body content: status pills, labels, meta, CI, checks, open link */}
              <PrDetailContent repoPath={props.repoPath} branch={props.branch}>
                <div class={s.actions}>
                  <button
                    class={s.viewDiffBtn}
                    onClick={handleViewDiff}
                    disabled={diffLoading()}
                  >
                    {diffLoading()
                      ? t("prDetail.loadingDiff", "Loading...")
                      : t("prDetail.viewDiff", "View Diff")}
                  </button>
                  <Show when={reviewCommand()}>
                    <button
                      class={s.viewDiffBtn}
                      onClick={() => {
                        const cmd = reviewCommand();
                        if (cmd && props.onReview) {
                          props.onReview(props.repoPath, props.branch, cmd);
                          props.onClose();
                        }
                      }}
                    >
                      {t("prDetail.review", "Review")}
                    </button>
                  </Show>
                  <Show when={canMergePr(pr())}>
                    <button
                      class={s.mergeBtn}
                      onClick={() => handleMerge()}
                      disabled={merging()}
                    >
                      {merging()
                        ? t("prDetail.merging", "Merging...")
                        : mergeLabel()}
                    </button>
                  </Show>
                  <Show when={pr().url}>
                    <button
                      class={s.viewDiffBtn}
                      onClick={() => handleOpenUrl(pr().url)}
                      title={t("prDetail.openOnGithub", "Open on GitHub")}
                    >
                      GitHub {"\u2197"}
                    </button>
                  </Show>
                  <SmartButtonStrip
                    placement="pr-popover"
                    repoPath={props.repoPath}
                    defaultPromptId="smart-review-pr"
                    extraFilter={(p: SavedPrompt) => {
                      const cs = githubStore.getCheckSummary(props.repoPath, props.branch);
                      if (p.id === "smart-fix-ci") return (cs?.failed ?? 0) > 0;
                      if (p.id === "smart-resolve-conflicts") return pr().mergeable === "CONFLICTING";
                      if (p.id === "smart-review-comments") return pr().review_decision === "CHANGES_REQUESTED";
                      return true;
                    }}
                  />
                </div>
                <Show when={mergeError()}>
                  <div class={s.errorMsg}>{mergeError()}</div>
                </Show>
              </PrDetailContent>
            </>
          )}
        </Show>
      </div>
      </Show>
    </>
  );
};
