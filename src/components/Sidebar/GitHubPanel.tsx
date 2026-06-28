import { type Component, createEffect, createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import type { BranchPrStatus, GitHubIssue, IssueFilterMode } from "../../types";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { handleOpenUrl } from "../../utils/openUrl";
import { IssueDetailContent } from "../IssueDetailPopover/IssueDetailContent";
import {
	type CleanupStep,
	PostMergeCleanupDialog,
	type StepId,
	type StepStatus,
} from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import { PrSection } from "./PrSection";
import s from "./Sidebar.module.css";

const FILTER_OPTIONS: { value: IssueFilterMode; label: string }[] = [
	{ value: "disabled", label: "Disabled" },
	{ value: "assigned", label: "Assigned" },
	{ value: "created", label: "Created" },
	{ value: "mentioned", label: "Mentioned" },
	{ value: "all", label: "All open" },
];

/** Unified GitHub panel showing remote-only PRs and Issues */
export const GitHubPanel: Component<{
	prs: BranchPrStatus[];
	allPrs: BranchPrStatus[];
	repoPath: string;
	onClose: () => void;
	onCheckout: (branchName: string) => void;
	onCreateWorktree?: (branchName: string) => void;
	onCleanupActive?: (active: boolean) => void;
}> = (props) => {
	const [issuesCollapsed, setIssuesCollapsed] = createSignal(false);

	// Issue accordion state
	const [expandedIssue, setExpandedIssue] = createSignal<number | null>(null);
	const [closingIssue, setClosingIssue] = createSignal<number | null>(null);
	const [issueActionError, setIssueActionError] = createSignal<{ num: number; msg: string } | null>(null);

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
		setCleanupStepNotes({});

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

	const handleMerged = (branchName: string, baseBranch: string, hasDirtyFiles: boolean) => {
		setCleanupCtx({ branchName, baseBranch, hasDirtyFiles });
	};

	const viewerLogin = () => githubStore.state.viewerLogin;

	const myPrs = createMemo(() => {
		const login = viewerLogin();
		if (!login) return [];
		return props.allPrs.filter((pr) => pr.author === login);
	});

	const filteredPrs = createMemo(() => {
		const effective = repoSettingsStore.getEffective(props.repoPath);
		const prHideDrafts = effective?.prHideDrafts ?? settingsStore.state.prHideDrafts;
		const prHideConflicting = effective?.prHideConflicting ?? settingsStore.state.prHideConflicting;
		const prHideCiFailing = effective?.prHideCiFailing ?? settingsStore.state.prHideCiFailing;
		return props.prs.filter((pr) => {
			if (prHideDrafts && pr.is_draft) return false;
			if (prHideConflicting && pr.mergeable === "CONFLICTING") return false;
			if (prHideCiFailing && (pr.checks?.failed ?? 0) > 0) return false;
			return true;
		});
	});

	const issues = createMemo(() => githubStore.getRepoIssues(props.repoPath));
	const issuesLoading = () => githubStore.state.issuesLoading;
	const circuitOpen = () => githubStore.state.circuitBreakerOpen;

	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Escape") {
			if (expandedIssue()) {
				setExpandedIssue(null);
			} else {
				props.onClose();
			}
		}
	};

	const handleCloseReopenIssue = async (issue: GitHubIssue) => {
		const isOpen = issue.state?.toUpperCase() === "OPEN";
		const command = isOpen ? "close_issue" : "reopen_issue";
		setClosingIssue(issue.number);
		setIssueActionError(null);
		try {
			await invoke(command, {
				repoPath: props.repoPath,
				issueNumber: issue.number,
			});
			appLogger.info("github", `${isOpen ? "Closed" : "Reopened"} issue #${issue.number}`);
			githubStore.pollIssues();
		} catch (e) {
			const msg = String(e);
			setIssueActionError({ num: issue.number, msg });
			appLogger.error("github", `Failed to ${command} issue #${issue.number}`, { error: msg });
		} finally {
			setClosingIssue(null);
		}
	};

	const handleCopyIssueNumber = (issue: GitHubIssue) => {
		navigator.clipboard.writeText(`#${issue.number}`).catch(() => {});
	};

	return (
		<Portal>
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
				<div class={s.ghPanelOverlay} onClick={props.onClose} onKeyDown={handleKeyDown} tabIndex={-1} />
				<div class={s.ghPanel} onKeyDown={handleKeyDown} tabIndex={-1} ref={(el) => onMount(() => el.focus())}>
					{/* Rate limit warning */}
					<Show when={circuitOpen()}>
						<div class={s.ghRateLimitBanner}>
							<span>{t("github.rateLimited", "GitHub API unavailable")}</span>
							<button
								class={s.ghRetryBtn}
								onClick={() => {
									githubStore.pollIssues();
								}}
							>
								{t("github.retry", "Retry")}
							</button>
						</div>
					</Show>
					{/* Panel header */}
					<div class={s.ghPanelHeader}>
						<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
							<path
								fill-rule="evenodd"
								d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"
							/>
						</svg>
						<span>GitHub</span>
						<button class={s.ghPanelClose} onClick={props.onClose}>
							&times;
						</button>
					</div>

					<div class={s.ghPanelBody}>
						{/* ── My Pull Requests section ── */}
						<Show when={viewerLogin()}>
							<PrSection
								title={t("github.myPullRequests", "My Pull Requests")}
								prs={myPrs()}
								repoPath={props.repoPath}
								icon="user"
								onCheckout={props.onCheckout}
								onCreateWorktree={props.onCreateWorktree}
								onMerged={handleMerged}
							/>
						</Show>

						{/* ── Pull Requests section ── */}
						<PrSection
							title={t("github.pullRequests", "Pull Requests")}
							prs={filteredPrs()}
							repoPath={props.repoPath}
							icon="pr"
							onCheckout={props.onCheckout}
							onCreateWorktree={props.onCreateWorktree}
							onMerged={handleMerged}
						/>

						{/* ── Issues section ── */}
						<div class={s.ghSection}>
							<Show when={settingsStore.state.issueFilter !== "disabled"}>
								<div class={s.ghSectionHeader} role="button" tabIndex={0} onClick={() => setIssuesCollapsed((v) => !v)} onKeyDown={onClickKeyDown(() => setIssuesCollapsed((v) => !v))}>
									<span class={cx(s.ghSectionChevron, !issuesCollapsed() && s.ghSectionChevronOpen)}>{"›"}</span>
									<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
										<path d="M8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" />
										<path d="M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Z" />
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
											<div class={s.ghSkeletonRow}>
												<div class={s.ghSkeletonBar} />
												<div class={s.ghSkeletonBarShort} />
											</div>
											<div class={s.ghSkeletonRow}>
												<div class={s.ghSkeletonBar} />
												<div class={s.ghSkeletonBarShort} />
											</div>
											<div class={s.ghSkeletonRow}>
												<div class={s.ghSkeletonBar} />
												<div class={s.ghSkeletonBarShort} />
											</div>
										</div>
									</Show>
									<Show when={!issuesLoading() || issues().length > 0}>
										<Show
											when={issues().length > 0}
											fallback={<div class={s.ghEmpty}>{t("github.noIssues", "No issues found")}</div>}
										>
											<div class={s.ghSectionList}>
												<For each={issues()}>
													{(issue: GitHubIssue) => (
														<div class={cx(s.ghItem, expandedIssue() === issue.number && s.ghItemExpanded)}>
															<div
																class={s.ghItemRow}
																onClick={() =>
																	setExpandedIssue((prev) => (prev === issue.number ? null : issue.number))
																}
															>
																<span class={s.ghItemNum}>#{issue.number}</span>
																<span class={s.ghItemTitle}>{issue.title}</span>
																<span
																	class={cx(
																		s.ghIssueBadge,
																		issue.state?.toUpperCase() === "OPEN" ? s.ghIssueOpen : s.ghIssueClosed,
																	)}
																>
																	{issue.state?.toUpperCase() === "OPEN" ? "Open" : "Closed"}
																</span>
															</div>
															<Show when={expandedIssue() === issue.number}>
																<div class={s.ghItemDetail} data-compact>
																	<IssueDetailContent issue={issue} repoPath={props.repoPath}>
																		<div class={s.ghItemActions}>
																			<button
																				class={cx(
																					s.ghActionBtn,
																					issue.state?.toUpperCase() === "OPEN" ? s.ghCloseBtn : s.ghReopenBtn,
																				)}
																				onClick={() => handleCloseReopenIssue(issue)}
																				disabled={closingIssue() === issue.number}
																				title={
																					issue.state?.toUpperCase() === "OPEN"
																						? t("github.closeIssue", "Close issue")
																						: t("github.reopenIssue", "Reopen issue")
																				}
																			>
																				{closingIssue() === issue.number
																					? "..."
																					: issue.state?.toUpperCase() === "OPEN"
																						? t("github.close", "Close")
																						: t("github.reopen", "Reopen")}
																			</button>
																			<button
																				class={s.ghActionBtn}
																				onClick={() => handleCopyIssueNumber(issue)}
																				title={t("github.copyNumber", "Copy issue number")}
																			>
																				#{issue.number}
																			</button>
																			<Show when={issue.url}>
																				<button
																					class={cx(s.ghActionBtn, s.ghLinkBtn)}
																					onClick={() => handleOpenUrl(issue.url)}
																					title={t("prDetail.openOnGithub", "Open on GitHub")}
																				>
																					GitHub {"↗"}
																				</button>
																			</Show>
																			<SmartButtonStrip
																				placement="issue-popover"
																				repoPath={props.repoPath}
																				defaultPromptId="smart-review-issue"
																				contextVariables={() => ({
																					issue_number: String(issue.number),
																					issue_title: issue.title,
																					issue_author: issue.author,
																					issue_state: issue.state,
																					issue_url: issue.url,
																					issue_labels: issue.labels?.map((l) => l.name).join(", ") || "none",
																					issue_assignees: issue.assignees?.join(", ") || "none",
																					issue_milestone: issue.milestone || "none",
																					issue_comments_count: String(issue.comments_count),
																				})}
																			/>
																		</div>
																		<Show when={issueActionError()?.num === issue.number}>
																			<div class={s.ghActionError}>{issueActionError()!.msg}</div>
																		</Show>
																	</IssueDetailContent>
																</div>
															</Show>
														</div>
													)}
												</For>
											</div>
										</Show>
									</Show>
								</Show>
							</Show>

							{/* Filter bar — always visible so user can re-enable issues */}
							<div class={s.ghFilterBar}>
								<select
									class={s.ghFilterSelect}
									value={settingsStore.state.issueFilter}
									onChange={(e) => githubStore.setIssueFilter(e.currentTarget.value as IssueFilterMode)}
								>
									<For each={FILTER_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
								</select>
							</div>
						</div>
					</div>
				</div>
			</Show>
		</Portal>
	);
};
