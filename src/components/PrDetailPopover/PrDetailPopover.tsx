import { Component, Show, createMemo, createSignal, onMount, onCleanup } from "solid-js";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { mdTabsStore } from "../../stores/mdTabs";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { appLogger } from "../../stores/appLogger";
import { invoke } from "../../invoke";
import { canMergePr, effectiveMergeMethod } from "../Sidebar/RepoSection";
import { mergeWithFallback } from "../../utils/prMerge";
import { handleOpenUrl } from "../../utils/openUrl";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { PrDetailContent } from "./PrDetailContent";
import { PostMergeCleanupDialog, type CleanupStep, type StepId, type StepStatus } from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
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
}

/** Rich PR detail popover showing PR metadata, diff stats, and CI checks */
export const PrDetailPopover: Component<PrDetailPopoverProps> = (props) => {
  const prData = () => githubStore.getBranchPrData(props.repoPath, props.branch);
  const [diffLoading, setDiffLoading] = createSignal(false);
  const [merging, setMerging] = createSignal(false);
  const [mergeError, setMergeError] = createSignal<string | null>(null);

  // Post-merge cleanup dialog state
  const [cleanupCtx, setCleanupCtx] = createSignal<{ branchName: string; baseBranch: string } | null>(null);
  const [cleanupExecuting, setCleanupExecuting] = createSignal(false);
  const [cleanupStepStatuses, setCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
  const [cleanupStepErrors, setCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});

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
      steps: steps.map((s) => ({ id: s.id, checked: s.checked })),
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
      const usedMethod = await mergeWithFallback(props.repoPath, pr.number, startMethod);
      // Persist the working method so future merges use it directly
      if (usedMethod !== preferred) {
        const repo = repositoriesStore.get(props.repoPath);
        repoSettingsStore.getOrCreate(props.repoPath, repo?.displayName ?? props.repoPath);
        repoSettingsStore.update(props.repoPath, { prMergeStrategy: usedMethod as "merge" | "squash" | "rebase" });
      }
      appLogger.info("github", `Merged PR #${pr.number} via ${usedMethod}`);
      githubStore.pollRepo(props.repoPath);

      // Show cleanup dialog instead of closing
      const baseBranch = pr.base_ref_name || "main";
      setCleanupCtx({ branchName: props.branch, baseBranch });
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

  const repoColor = createMemo(() =>
    repoSettingsStore.get(props.repoPath)?.color
      || repositoriesStore.getGroupForRepo(props.repoPath)?.color
      || undefined,
  );

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
