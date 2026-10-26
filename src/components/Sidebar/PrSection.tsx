import { type Component, createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { mdTabsStore } from "../../stores/mdTabs";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { toastsStore } from "../../stores/toasts";
import type { BranchPrStatus } from "../../types";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { writeClipboard } from "../../utils/clipboard";
import { handleOpenUrl } from "../../utils/openUrl";
import { canApprovePr, effectiveMergeMethod, mergeWithFallback } from "../../utils/prMerge";
import { prContextVariables } from "../../utils/promptContext";
import { canUpdatePrBranch, prAgeMarker, prReference } from "../../utils/prRow";
import { PrDetailContent } from "../PrDetailPopover/PrDetailContent";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import { Button } from "../ui/Button";
import { ChevronIcon } from "../ui/ChevronIcon";
import { PrStateBadge } from "./PrStateBadge";
import { canMergePr } from "./prMergeEligibility";
import s from "./Sidebar.module.css";

export interface PrSectionProps {
	title: string;
	/** Already filtered by the parent — dismissed PRs are removed upstream so the
	 *  keyboard navigation list and the rendered rows cannot drift apart. */
	prs: BranchPrStatus[];
	repoPath: string;
	icon?: "pr" | "user";
	/** Collapse, expansion and keyboard highlight are owned by GitHubPanel:
	 *  collapse is persisted and the arrow keys walk rows across sections. */
	collapsed: boolean;
	onToggleCollapsed: () => void;
	expandedKey: string | null;
	onToggleExpanded: (branch: string) => void;
	/** Branch of the row the arrow keys currently sit on, or null. */
	activeKey: string | null;
	dismissedCount: number;
	onDismiss: (prNumber: number) => void;
	onShowDismissed: () => void;
	onCheckout: (branchName: string) => void;
	onCreateWorktree?: (branchName: string) => void;
	onConflictAssist?: (prNumber: number) => void;
	onPushBranch?: (worktreePath: string) => void;
	onMerged: (branchName: string, baseBranch: string, hasDirtyFiles: boolean) => void;
}

/** Text of a failed action; an empty rejection must not render an empty line. */
const failureMessage = (error: unknown, action: string) =>
	(error instanceof Error ? error.message : String(error)).trim() || `${action} failed`;

const createPrSet = () => {
	const [prs, setPrs] = createSignal<ReadonlySet<number>>(new Set());
	const set = (prNumber: number, on: boolean) =>
		setPrs((current) => {
			const next = new Set(current);
			if (on) next.add(prNumber);
			else next.delete(prNumber);
			return next;
		});
	return [prs, set] as const;
};

/** One GitHub poll interval (`BASE_INTERVAL` in github_poller.rs). */
export const UPDATE_BRANCH_HOLD_MS = 60_000;

export const PrSection: Component<PrSectionProps> = (props) => {
	const [diffLoadingPr, setDiffLoadingPr] = createSignal<number | null>(null);
	/** PRs with an action in flight: per PR, so one finishing never re-enables another's buttons. */
	const [busyPrs, setBusy] = createPrSet();
	const [mergingPrs, setMerging] = createPrSet();
	const [approvingPrs, setApproving] = createPrSet();
	/** Error of a row action, kept per PR so another row never shows it and two failures coexist. */
	const [rowErrors, setRowErrors] = createSignal<ReadonlyMap<number, string>>(new Map());
	const setRowError = (prNumber: number, message: string | null) =>
		setRowErrors((errors) => {
			const next = new Map(errors);
			if (message === null) next.delete(prNumber);
			else next.set(prNumber, message);
			return next;
		});
	/** `prNumber:head` of update-branch requests accepted: GitHub answers 202 and the merge lands
	 *  later, so the row keeps reading BEHIND until a poll brings a new head. The hold lasts one
	 *  poll interval: if the async update failed silently the head is unchanged and the button
	 *  must come back. */
	const [updateRequestedFor, setUpdateRequestedFor] = createSignal<ReadonlySet<string>>(new Set());
	const updateHoldKey = (pr: BranchPrStatus) => `${pr.number}:${pr.head_ref_oid}`;
	const updateHoldTimers = new Set<ReturnType<typeof setTimeout>>();
	onCleanup(() => {
		for (const timer of updateHoldTimers) clearTimeout(timer);
	});
	const holdUpdateBranch = (pr: BranchPrStatus) => {
		const key = updateHoldKey(pr);
		setUpdateRequestedFor((held) => new Set(held).add(key));
		const timer = setTimeout(() => {
			updateHoldTimers.delete(timer);
			setUpdateRequestedFor((held) => {
				const next = new Set(held);
				next.delete(key);
				return next;
			});
		}, UPDATE_BRANCH_HOLD_MS);
		updateHoldTimers.add(timer);
	};
	// An error lives until its PR starts its next action or leaves the list; expanding or
	// collapsing rows never clears it.
	createEffect(
		on(
			() => props.prs.map((pr) => pr.number),
			(numbers) =>
				setRowErrors((errors) => {
					if ([...errors.keys()].every((n) => numbers.includes(n))) return errors;
					return new Map([...errors].filter(([n]) => numbers.includes(n)));
				}),
			{ defer: true },
		),
	);

	const visiblePrs = () => props.prs;

	const mergeLabel = (pr: BranchPrStatus) => {
		const preferred =
			repoSettingsStore.getEffectiveField(props.repoPath, "prMergeStrategy") ?? repoDefaultsStore.state.prMergeStrategy;
		const method = effectiveMergeMethod(pr, preferred);
		if (method === "squash") return t("sidebar.mergeSquash", "Squash & Merge");
		if (method === "rebase") return t("sidebar.mergeRebase", "Rebase & Merge");
		return t("sidebar.merge", "Merge");
	};

	const handleMerge = async (pr: BranchPrStatus) => {
		setMerging(pr.number, true);
		setRowError(pr.number, null);
		try {
			const preferred =
				repoSettingsStore.getEffectiveField(props.repoPath, "prMergeStrategy") ??
				repoDefaultsStore.state.prMergeStrategy;
			const startMethod = effectiveMergeMethod(pr, preferred);
			const usedMethod = await mergeWithFallback(props.repoPath, pr.number, startMethod, pr.head_ref_oid);
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
			} catch {
				/* ignore */
			}
			props.onMerged(pr.branch, baseBranch, hasDirtyFiles);
		} catch (e) {
			const msg = failureMessage(e, "Merge");
			setRowError(pr.number, msg);
			appLogger.error("github", `Failed to merge PR #${pr.number}`, { error: msg });
		} finally {
			setMerging(pr.number, false);
		}
	};

	const handleApprove = async (pr: BranchPrStatus) => {
		setApproving(pr.number, true);
		setRowError(pr.number, null);
		try {
			await invoke("approve_pr", { repoPath: props.repoPath, prNumber: pr.number });
			appLogger.info("github", `Approved PR #${pr.number}`);
			githubStore.pollRepo(props.repoPath);
		} catch (e) {
			const msg = failureMessage(e, "Approve");
			setRowError(pr.number, msg);
			appLogger.error("github", `Failed to approve PR #${pr.number}`, { error: msg });
		} finally {
			setApproving(pr.number, false);
		}
	};

	const handleCopyReference = (pr: BranchPrStatus) => {
		const ref = prReference(pr);
		if (ref) writeClipboard(ref).catch(() => {});
	};

	const handleUpdateBranch = async (pr: BranchPrStatus) => {
		setBusy(pr.number, true);
		setRowError(pr.number, null);
		try {
			await invoke("update_pr_branch", {
				repoPath: props.repoPath,
				prNumber: pr.number,
				expectedHeadSha: pr.head_ref_oid,
			});
			holdUpdateBranch(pr);
			appLogger.info("github", `Requested branch update for PR #${pr.number}`);
			githubStore.pollRepo(props.repoPath);
		} catch (e) {
			const msg = failureMessage(e, "Update branch");
			setRowError(pr.number, msg);
			appLogger.error("github", `Failed to update branch of PR #${pr.number}`, { error: msg });
		} finally {
			setBusy(pr.number, false);
		}
	};

	const handleClosePr = async (pr: BranchPrStatus) => {
		if (
			!window.confirm(t("sidebar.closePrConfirm", "Close PR #{number} without merging?", { number: String(pr.number) }))
		)
			return;
		setBusy(pr.number, true);
		setRowError(pr.number, null);
		try {
			await invoke("close_pr", { repoPath: props.repoPath, prNumber: pr.number });
			appLogger.info("github", `Closed PR #${pr.number}`);
			githubStore.pollRepo(props.repoPath);
		} catch (e) {
			const msg = failureMessage(e, "Close PR");
			setRowError(pr.number, msg);
			appLogger.error("github", `Failed to close PR #${pr.number}`, { error: msg });
		} finally {
			setBusy(pr.number, false);
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

	const PrIcon = () =>
		props.icon === "user" ? (
			<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
				<path d="M10.5 5a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0ZM8 8a4 4 0 1 0 0-8A4 4 0 0 0 8 8Zm-5.5 7.5h11a.5.5 0 0 0 .5-.5v-.5A5.5 5.5 0 0 0 2.5 9h-.02A5.5 5.5 0 0 0 2 14.5v.5c0 .28.22.5.5.5Z" />
			</svg>
		) : (
			<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
				<path d="M5.45 5.154A4.25 4.25 0 0 0 9.25 7.5h1.378a2.251 2.251 0 1 1 0 1.5H9.25A5.734 5.734 0 0 1 5 7.123v3.505a2.25 2.25 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.95-.218ZM4.25 13.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm8-8a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5ZM4.25 4a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Z" />
			</svg>
		);

	return (
		<div class={s.ghSection}>
			<div
				class={s.ghSectionHeader}
				role="button"
				tabIndex={0}
				onClick={() => props.onToggleCollapsed()}
				onKeyDown={onClickKeyDown(() => props.onToggleCollapsed())}
			>
				<span class={cx(s.ghSectionChevron, !props.collapsed && s.ghSectionChevronOpen)}>
					<ChevronIcon />
				</span>
				<PrIcon />
				<span>{props.title}</span>
				<Show when={visiblePrs().length > 0}>
					<span class={s.ghSectionCount}>{visiblePrs().length}</span>
				</Show>
				<Show when={props.dismissedCount > 0}>
					<Button
						size="compact"
						variant="ghost"
						class={s.ghShowDismissed}
						onClick={(e) => {
							e.stopPropagation();
							props.onShowDismissed();
						}}
					>
						{t("sidebar.showDismissed", "Show")} {props.dismissedCount}
					</Button>
				</Show>
			</div>
			<Show when={!props.collapsed}>
				<Show
					when={visiblePrs().length > 0}
					fallback={<div class={s.ghEmpty}>{t("github.noPrs", "No remote-only PRs")}</div>}
				>
					<div class={s.ghSectionList}>
						<For each={visiblePrs()}>
							{(pr) => (
								<div class={cx(s.ghItem, props.expandedKey === pr.branch && s.ghItemExpanded)}>
									<div
										class={cx(s.ghItemRow, props.activeKey === pr.branch && s.ghItemRowActive)}
										data-gh-active={props.activeKey === pr.branch ? "" : undefined}
										onClick={() => props.onToggleExpanded(pr.branch)}
									>
										<span class={s.ghItemNum}>#{pr.number}</span>
										<span class={s.ghItemTitle}>{pr.title}</span>
										<Show when={pr.state?.toUpperCase() === "OPEN" ? prAgeMarker(pr.created_at) : null}>
											{(age) => (
												<span
													class={s.ghAgeMarker}
													data-tooltip={t("sidebar.prAge", "Open for {age} or more", { age: age() })}
													data-tooltip-pos="bottom"
												>
													{age()}
												</span>
											)}
										</Show>
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
									<Show when={props.expandedKey === pr.branch}>
										<div class={s.ghItemDetail} data-compact>
											<Button
												size="compact"
												variant="ghost"
												class={s.ghItemDismiss}
												onClick={() => props.onDismiss(pr.number)}
												aria-label={t("sidebar.dismissPr", "Hide this PR from view")}
												data-tooltip={t("sidebar.dismissPr", "Hide this PR from view")}
												data-tooltip-pos="bottom"
												data-tooltip-align="right"
											>
												&times;
											</Button>
											<PrDetailContent
												repoPath={props.repoPath}
												branch={pr.branch}
												onConflictAssist={props.onConflictAssist}
												onPushBranch={props.onPushBranch}
											>
												<div class={s.ghItemActions}>
													<Button
														size="compact"
														variant="ghost"
														class={s.ghActionBtn}
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
															class={s.ghActionBtn}
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
															class={cx(s.ghActionBtn, s.ghApproveBtn)}
															onClick={() => handleApprove(pr)}
															disabled={approvingPrs().has(pr.number)}
															data-tooltip={t("sidebar.approvePr", "Approve this pull request")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{approvingPrs().has(pr.number)
																? t("sidebar.approving", "Approving...")
																: t("sidebar.approve", "Approve")}
														</Button>
													</Show>
													<Show when={canUpdatePrBranch(pr) && !updateRequestedFor().has(updateHoldKey(pr))}>
														<Button
															size="compact"
															variant="ghost"
															class={s.ghActionBtn}
															onClick={() => handleUpdateBranch(pr)}
															disabled={busyPrs().has(pr.number)}
															data-tooltip={t("sidebar.updateBranchTitle", "Merge the base branch into this PR branch")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{t("sidebar.updateBranch", "Update branch")}
														</Button>
													</Show>
													<Show when={canMergePr(pr)}>
														<Button
															size="compact"
															variant="ghost"
															class={cx(s.ghActionBtn, s.ghMergeBtn)}
															onClick={() => handleMerge(pr)}
															disabled={mergingPrs().has(pr.number)}
															data-tooltip={t("sidebar.mergePr", "Merge this pull request")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{mergingPrs().has(pr.number) ? t("sidebar.merging", "Merging...") : mergeLabel(pr)}
														</Button>
													</Show>
													<Button
														size="compact"
														variant="ghost"
														class={s.ghActionBtn}
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
													<Show when={prReference(pr)}>
														<Button
															size="compact"
															variant="ghost"
															class={s.ghActionBtn}
															onClick={() => handleCopyReference(pr)}
															data-tooltip={t("sidebar.copyReference", "Copy owner/repo#number")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{t("sidebar.copyRef", "Copy ref")}
														</Button>
													</Show>
													<Show when={pr.state?.toUpperCase() === "OPEN"}>
														<Button
															size="compact"
															variant="ghost"
															class={cx(s.ghActionBtn, s.ghCloseBtn)}
															onClick={() => handleClosePr(pr)}
															disabled={busyPrs().has(pr.number)}
															data-tooltip={t("sidebar.closePr", "Close this pull request without merging")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															{t("sidebar.close", "Close")}
														</Button>
													</Show>
													<Show when={pr.url}>
														<Button
															size="compact"
															variant="ghost"
															class={cx(s.ghActionBtn, s.ghLinkBtn)}
															onClick={() => handleOpenUrl(pr.url)}
															data-tooltip={t("prDetail.openOnGithub", "Open on GitHub")}
															data-tooltip-pos="bottom"
															data-tooltip-align="right"
														>
															GitHub {"↗"}
														</Button>
													</Show>
													<SmartButtonStrip
														placement="pr-popover"
														repoPath={props.repoPath}
														defaultPromptId="smart-review-pr"
														extraFilter={(p: SavedPrompt) => {
															const cs = githubStore.getCheckSummary(props.repoPath, pr.branch);
															if (p.id === "smart-fix-ci") return (cs?.failed ?? 0) > 0;
															if (p.id === "smart-resolve-conflicts") return pr.conflict_state === "conflicting";
															if (p.id === "smart-review-comments") return pr.review_decision === "CHANGES_REQUESTED";
															return true;
														}}
														contextVariables={() => prContextVariables(pr)}
													/>
												</div>
												<Show when={rowErrors().get(pr.number)}>
													{(message) => <div class={s.ghActionError}>{message()}</div>}
												</Show>
											</PrDetailContent>
										</div>
									</Show>
								</div>
							)}
						</For>
					</div>
				</Show>
			</Show>
		</div>
	);
};
