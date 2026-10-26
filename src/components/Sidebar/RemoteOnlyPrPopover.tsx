import { type Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { useOutsideDismiss } from "../../hooks/useOutsideDismiss";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { mdTabsStore } from "../../stores/mdTabs";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { toastsStore } from "../../stores/toasts";
import type { BranchPrStatus } from "../../types";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { canApprovePr, effectiveMergeMethod, mergeWithFallback } from "../../utils/prMerge";
import {
	type CleanupStep,
	PostMergeCleanupDialog,
	type StepId,
	type StepStatus,
} from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { PrDetailContent } from "../PrDetailPopover/PrDetailContent";
import { Button } from "../ui/Button";
import { PrStateBadge } from "./PrStateBadge";
import { canMergePr } from "./prMergeEligibility";
import s from "./Sidebar.module.css";

export { canMergePr } from "./prMergeEligibility";

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
	const [diffLoadingPr, setDiffLoadingPr] = createSignal<number | null>(null);
	const [approvingPr, setApprovingPr] = createSignal<number | null>(null);
	const [approveError, setApproveError] = createSignal<string | null>(null);
	const [dismissedPrs, setDismissedPrs] = createSignal<Set<number>>(new Set());

	// Post-merge cleanup state
	const [cleanupCtx, setCleanupCtx] = createSignal<{
		branchName: string;
		baseBranch: string;
		hasDirtyFiles: boolean;
	} | null>(null);
	const [cleanupExecuting, setCleanupExecuting] = createSignal(false);
	const [cleanupStepStatuses, setCleanupStepStatuses] = createSignal<Partial<Record<StepId, StepStatus>>>({});
	const [cleanupStepErrors, setCleanupStepErrors] = createSignal<Partial<Record<StepId, string>>>({});
	const [cleanupStepNotes, setCleanupStepNotes] = createSignal<Partial<Record<StepId, string>>>({});

	// Notify parent when cleanup dialog is active so it keeps us mounted
	createEffect(() => {
		props.onCleanupActive?.(!!cleanupCtx());
	});

	const cleanupIsOnBaseBranch = () => {
		const ctx = cleanupCtx();
		if (!ctx) return true; // remote-only: user is not on merged branch
		const repo = repositoriesStore.get(props.repoPath);
		return repo?.activeWorkspaceId === ctx.baseBranch;
	};

	/** Called by `executeCleanup`, which passes the workspace id it was given. */
	const closeTerminalsForBranch = async (repoPath: string, workspaceId: string) => {
		const repo = repositoriesStore.get(repoPath);
		const branch = repo?.workspaces[workspaceId];
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
		setCleanupStepNotes({});

		await executeCleanup({
			repoPath: props.repoPath,
			// A PR names its head branch, so the workspace is resolved through the
			// single documented branch->id seam; the id addresses the row and the
			// checkout, the branch stays the ref to delete.
			workspaceId: repositoriesStore.workspaceIdOnBranch(props.repoPath, ctx.branchName) ?? ctx.branchName,
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
			onStepNote: (id, note) => setCleanupStepNotes((prev) => ({ ...prev, [id]: note })),
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

	let popoverRef: HTMLDivElement | undefined;
	useOutsideDismiss({ inside: () => [popoverRef], onClose: props.onClose, enabled: () => !cleanupCtx() });

	const visiblePrs = createMemo(() => props.prs.filter((pr) => !dismissedPrs().has(pr.number)));

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
		setExpandedBranch((prev) => (prev === branch ? null : branch));
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
			const preferred =
				repoSettingsStore.getEffective(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
			const startMethod = effectiveMergeMethod(pr, preferred);
			const usedMethod = await mergeWithFallback(props.repoPath, pr.number, startMethod, pr.head_ref_oid);
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
			} catch {
				/* ignore — assume clean */
			}
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
		const preferred =
			repoSettingsStore.getEffective(props.repoPath)?.prMergeStrategy ?? repoDefaultsStore.state.prMergeStrategy;
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
		setDiffLoadingPr(pr.number);
		try {
			const { diff, headSha } = await invoke<{ diff: string; headSha: string }>("get_pr_diff", {
				repoPath: props.repoPath,
				prNumber: pr.number,
			});
			mdTabsStore.addPrDiff(props.repoPath, pr.number, pr.title, diff, headSha);
		} catch (e) {
			const msg = String(e);
			appLogger.error("github", `Failed to load PR #${pr.number} diff`, { error: msg });
			toastsStore.add(
				`PR #${pr.number} diff failed`,
				msg.includes("too_large") ? "Diff too large (>300 files)" : msg,
				"error",
			);
		} finally {
			setDiffLoadingPr((current) => (current === pr.number ? null : current));
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
						stepNotes={cleanupStepNotes()}
					/>
				)}
			</Show>
			<Show when={!cleanupCtx()}>
				<div ref={popoverRef} class={s.remoteOnlyPopover} onKeyDown={handleKeyDown} tabIndex={-1}>
					<div class={s.remoteOnlyHeader}>
						<span>{t("sidebar.remoteOnlyPrs", "Remote-only PRs")}</span>
						<Show when={dismissedCount() > 0}>
							<Button size="compact" variant="ghost" class={s.remoteOnlyShowDismissed} onClick={handleShowDismissed}>
								{t("sidebar.showDismissed", "Show")} {dismissedCount()} {t("sidebar.dismissed", "dismissed")}
							</Button>
						</Show>
						<Button size="compact" variant="ghost" class={s.remoteOnlyClose} onClick={props.onClose}>
							&times;
						</Button>
					</div>
					<div class={s.remoteOnlyList}>
						<For each={visiblePrs()}>
							{(pr) => (
								<div class={cx(s.remoteOnlyItem, expandedBranch() === pr.branch && s.remoteOnlyItemExpanded)}>
									<div
										class={s.remoteOnlyRow}
										role="button"
										tabIndex={0}
										onClick={() => handleRowClick(pr.branch)}
										onKeyDown={onClickKeyDown(() => handleRowClick(pr.branch))}
									>
										<span class={s.remoteOnlyNum}>#{pr.number}</span>
										<span class={s.remoteOnlyTitle}>{pr.title}</span>
										<PrStateBadge
											prNumber={pr.number}
											state={pr.state}
											isDraft={pr.is_draft}
											mergeable={pr.mergeable}
											conflictState={pr.conflict_state}
											reviewDecision={pr.review_decision}
											ciFailed={pr.checks?.failed}
											ciPending={pr.checks?.pending}
											unresolvedThreads={pr.unresolved_threads}
											unresolvedThreadsTruncated={pr.unresolved_threads_truncated}
										/>
									</div>
									<Show when={expandedBranch() === pr.branch}>
										<div class={s.remoteOnlyDetail}>
											<Button
												size="compact"
												variant="ghost"
												class={s.remoteOnlyDetailDismiss}
												onClick={() => handleDismiss(pr.number)}
												aria-label={t("sidebar.dismissPr", "Hide this PR from view")}
												data-tooltip={t("sidebar.dismissPr", "Hide this PR from view")}
												data-tooltip-pos="bottom"
												data-tooltip-align="right"
											>
												&times;
											</Button>
											<PrDetailContent repoPath={props.repoPath} branch={pr.branch}>
												<div class={s.remoteOnlyDetailActions}>
													<Button
														size="compact"
														variant="ghost"
														class={s.remoteOnlyCheckout}
														onClick={() => props.onCheckout(pr.branch)}
														data-tooltip={t("sidebar.checkoutBranch", "Check out this branch locally")}
														data-tooltip-pos="bottom"
														data-tooltip-align="right"
													>
														{t("sidebar.checkout", "Checkout")}
													</Button>
													<Show when={props.onCreateWorktree}>
														<Button
															size="compact"
															variant="ghost"
															class={s.remoteOnlyWorktree}
															onClick={() => props.onCreateWorktree?.(pr.branch)}
															data-tooltip={t("sidebar.createWorktreeFromBranch", "Create worktree from this branch")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{t("sidebar.worktree", "Worktree")}
														</Button>
													</Show>
													<Show when={canApprovePr(pr, githubStore.state.viewerLogin)}>
														<Button
															size="compact"
															variant="primary"
															class={s.remoteOnlyApprove}
															onClick={() => handleApprove(pr)}
															disabled={approvingPr() === pr.number}
															data-tooltip={t("sidebar.approvePr", "Approve this pull request")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{approvingPr() === pr.number
																? t("sidebar.approving", "Approving...")
																: t("sidebar.approve", "Approve")}
														</Button>
													</Show>
													<Show when={canMergePr(pr)}>
														<Button
															size="compact"
															variant="ghost"
															class={s.remoteOnlyMerge}
															onClick={() => handleMerge(pr)}
															disabled={mergingPr() === pr.number}
															data-tooltip={t("sidebar.mergePr", "Merge this pull request")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{mergingPr() === pr.number ? t("sidebar.merging", "Merging...") : mergeLabel(pr)}
														</Button>
													</Show>
													<Button
														size="compact"
														variant="ghost"
														class={s.remoteOnlyViewDiff}
														onClick={() => handleViewDiff(pr)}
														disabled={diffLoadingPr() === pr.number}
														data-tooltip={t("sidebar.viewDiff", "View PR diff")}
														data-tooltip-pos="bottom"
														data-tooltip-align="right"
													>
														{diffLoadingPr() === pr.number
															? t("sidebar.loadingDiff", "Loading...")
															: t("sidebar.diff", "Diff")}
													</Button>
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
