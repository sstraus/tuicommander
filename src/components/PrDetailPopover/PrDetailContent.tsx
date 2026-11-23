import { type Component, createEffect, createMemo, createResource, createSignal, For, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import { openTaskConversation } from "../../services/taskNavigation";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { flattenReviewFindings, postableFindings, prReviewStore, type SelectableFinding } from "../../stores/prReview";
import { repositoriesStore } from "../../stores/repositories";
import { rpc } from "../../transport";
import type { CheckDetail } from "../../types";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { getCiClass, getCiIcon } from "../../utils/ciDisplay";
import { handleOpenUrl } from "../../utils/openUrl";
import { prNextAction } from "../../utils/prNextAction";
import { prContextVariables } from "../../utils/promptContext";
import { PR_READINESS_LABELS, PR_READINESS_SEVERITY, prReadinessOf } from "../../utils/prReadiness";
import { stripAnsi } from "../../utils/stripAnsi";
import { relativeTime } from "../../utils/time";
import { PrReviewWorkflowAction } from "../PrReviewWorkflow/PrReviewWorkflowAction";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import na from "../shared/NextAction.module.css";
import { ToolbarIcons } from "../shared/PanelToolbar";
import { SeverityIcon } from "../shared/SeverityIcon";
import {
	createWorkItemPreview,
	descriptionSummary,
	PreviewComments,
	PreviewDescription,
} from "../shared/WorkItemPreview";
import { Button } from "../ui/Button";
import { PanelSection } from "../ui/PanelSection";
import { PrActionButtons } from "./PrActionButtons";
import s from "./PrDetailPopover.module.css";

/** Map backend merge state CSS class strings to module classes */
const MERGE_STATE_CLASSES: Record<string, string> = {
	clean: s.clean,
	behind: s.behind,
	blocked: s.blocked,
	conflicting: s.conflicting,
};

/** Map backend review state CSS class strings to module classes */
const REVIEW_STATE_CLASSES: Record<string, string> = {
	approved: s.approved,
	"changes-requested": s.changesRequested,
	"review-required": s.reviewRequired,
};

/** Readiness severity -> module class (the verdict itself comes from prReadiness) */
const READINESS_CLASSES = {
	ok: s.readinessOk,
	warn: s.readinessWarn,
	critical: s.readinessCritical,
	muted: s.readinessMuted,
};

/** Map CI state strings to module classes */
const CI_CLASSES: Record<string, string> = {
	success: s.success,
	failure: s.failure,
	pending: s.pending,
};

export interface PrDetailContentProps {
	repoPath: string;
	branch: string;
	/** Shown only when the PR merge state is conflicting — rebases the PR head
	 *  branch onto its base in a worktree and opens an agent to resolve conflicts. */
	onConflictAssist?: (prNumber: number) => void;
	/** Push the (rebased/resolved) conflict worktree branch to origin. Shown when a
	 *  local worktree exists for this branch. Gated behind a confirm() by the caller. */
	onPushBranch?: (worktreePath: string) => void;
	onConversationOpen?: () => void;
	/** Approve, offered as the next action when the viewer can review (ui-v3). */
	onApprove?: () => void;
	approving?: boolean;
	/** Merge, offered as the next action when the PR is ready. */
	onMerge?: () => void;
	merging?: boolean;
	mergeLabel?: string;
	/** Extra content rendered after CI checks (e.g. action buttons) */
	children?: JSX.Element;
}

/** Shared PR detail body: status pills, labels, merge direction, timestamps, meta, CI, checks, and Open on GitHub link.
 *  Used by both PrDetailPopover (floating) and the remote-only PR accordion (inline). */
export const PrDetailContent: Component<PrDetailContentProps> = (props) => {
	const openConversation = () => {
		const url = prData()?.url;
		if (!url) return;
		void openTaskConversation(url);
		props.onConversationOpen?.();
	};
	const prData = () => githubStore.getBranchPrData(props.repoPath, props.branch);
	const checkSummary = () => githubStore.getCheckSummary(props.repoPath, props.branch);
	const checkDetails = () => githubStore.getCheckDetails(props.repoPath, props.branch);
	const [expandedCircleCiLog, setExpandedCircleCiLog] = createSignal<string | null>(null);
	const [circleCiLogs, setCircleCiLogs] = createSignal<
		Record<string, { text?: string; error?: string; loading?: boolean }>
	>({});
	// DEFERRED (2026-09-25) — check details expose a URL but no CI provider. This predicate only
	// controls whether to show the Log button; the backend verifies the selected
	// check URL against the PR's head before fetching logs. Use a provider field
	// here when the check-details protocol supplies one.
	const isCircleCiFailure = (check: { state: string; html_url: string }) =>
		getCiClass(check.state) === "failure" && /(^|\/\/)(app\.)?circleci\.com\//.test(check.html_url);
	const toggleCircleCiLog = async (url: string) => {
		if (expandedCircleCiLog() === url) return setExpandedCircleCiLog(null);
		setExpandedCircleCiLog(url);
		if (circleCiLogs()[url]?.text || circleCiLogs()[url]?.loading) return;
		setCircleCiLogs((logs) => ({ ...logs, [url]: { loading: true } }));
		try {
			const text = await rpc<string>("fetch_ci_failure_logs", {
				repoPath: props.repoPath,
				branch: props.branch,
				checkUrl: url,
				headSha: prData()?.head_ref_oid,
			});
			setCircleCiLogs((logs) => ({ ...logs, [url]: { text } }));
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			setCircleCiLogs((logs) => ({ ...logs, [url]: { error: message } }));
		}
	};

	// AI-review state lives in prReviewStore keyed by repo+PR, not in local
	// signals — so an in-flight review survives the popover being closed and
	// reopening shows its running/done/error state instead of resetting to "Run".
	const reviewEntry = () => {
		const pr = prData();
		return pr ? prReviewStore.get(props.repoPath, pr.number) : undefined;
	};
	const review = () => reviewEntry()?.result ?? null;
	const reviewLoading = () => reviewEntry()?.status === "running";
	const reviewPosting = () => reviewEntry()?.posting ?? false;
	const reviewError = () => reviewEntry()?.error ?? null;
	const selectedFindingIds = () => reviewEntry()?.selectedIds ?? [];

	const reviewFindings = createMemo<SelectableFinding[]>(() => flattenReviewFindings(review()?.files ?? []));

	const selectedFindings = createMemo(() => postableFindings(reviewFindings(), new Set(selectedFindingIds())));

	function runAiReview() {
		const pr = prData();
		if (!pr) return;
		void prReviewStore.run(props.repoPath, pr.number);
	}

	function toggleFinding(id: string) {
		const pr = prData();
		if (!pr) return;
		prReviewStore.toggleFinding(props.repoPath, pr.number, id);
	}

	function postSelectedFindings() {
		const pr = prData();
		const findings = selectedFindings();
		if (!pr || findings.length === 0 || reviewPosting()) return;
		if (!window.confirm(t("prDetail.postReviewConfirm", "Post selected AI review findings to GitHub?"))) return;
		void prReviewStore.post(props.repoPath, pr.number, findings);
	}

	// Lazy-load CI check details when this content mounts.
	// Deferred via queueMicrotask so the popover renders instantly with cached
	// data — the IPC call runs after the first paint, not during mount.
	//
	// UX trade-off: during the fetch window (typically sub-second) the panel
	// shows stale cached checks. We accept that over a spinner because the cache
	// is usually fresh (polled elsewhere) and flashing a loading state every
	// hover would be noisier than the occasional stale render.
	createEffect(() => {
		const pr = prData();
		if (pr) {
			const { repoPath, branch } = props;
			const prNumber = pr.number;
			queueMicrotask(() => {
				githubStore
					.loadCheckDetails(repoPath, branch, prNumber)
					.catch((e) => appLogger.warn("github", "Failed to load check details", { error: String(e) }));
			});
		}
	});

	const isTerminalState = () => {
		const state = prData()?.state?.toUpperCase();
		return state === "CLOSED" || state === "MERGED";
	};

	const mergeState = () => {
		if (isTerminalState()) return null;
		const label = prData()?.merge_state_label;
		if (!label) return null;
		// GitHub's CLEAN knows nothing of unresolved threads or running checks: while the shared
		// verdict names a blocker, "Ready to merge" would contradict the chip beside it.
		const kind = readiness();
		if (
			label.css_class === "clean" &&
			kind &&
			PR_READINESS_SEVERITY[kind] !== "ok" &&
			PR_READINESS_SEVERITY[kind] !== "muted"
		)
			return null;
		// The readiness chip already names it ("Conflicts" twice reads as two problems).
		if (kind && label.label === PR_READINESS_LABELS[kind]) return null;
		return { label: label.label, cssClass: label.css_class };
	};

	const reviewState = () => {
		if (isTerminalState()) return null;
		const label = prData()?.review_state_label;
		if (!label) return null;
		return { label: label.label, cssClass: label.css_class };
	};

	const readiness = () => {
		const pr = prData();
		return pr ? prReadinessOf(pr) : null;
	};

	// The bot/human split costs one GraphQL query per PR, so it is fetched only
	// while the panel is open on a PR that has unresolved threads.
	const [threadSplit] = createResource(
		() => {
			const pr = prData();
			return pr && (pr.unresolved_threads > 0 || pr.unresolved_threads_truncated) && !isTerminalState()
				? { path: props.repoPath, prNumber: pr.number, unresolved: pr.unresolved_threads }
				: null;
		},
		({ path, prNumber }) =>
			rpc<{ bot: number; human: number; ego: number }>("get_pr_review_threads", { path, prNumber }).catch((e) => {
				appLogger.warn("github", "Failed to load review threads", { error: String(e) });
				return null;
			}),
	);

	// Keyed on the conflict state, not the readiness verdict: the verdict ranks "draft" above
	// "conflict", but a conflicting draft PR still needs Resolve conflicts / Push.
	const isConflicting = () => prData()?.conflict_state === "conflicting";

	/** Local worktree path for this PR's head branch, if one exists (e.g. after
	 *  conflict-assist created it). Drives the Push button's visibility. */
	const conflictWorktreePath = () =>
		repositoriesStore.state.repositories[props.repoPath]?.workspaces[
			repositoriesStore.workspaceIdOnBranch(props.repoPath, props.branch) ?? props.branch
		]?.worktreePath ?? null;

	function handlePush() {
		const wt = conflictWorktreePath();
		if (!wt) return;
		if (!window.confirm(t("github.pushConflictConfirm", "Push the rebased/resolved branch to origin?"))) return;
		props.onPushBranch?.(wt);
	}

	const nextAction = () => {
		const pr = prData();
		return pr ? prNextAction(pr, githubStore.state.viewerLogin) : null;
	};
	const failedChecks = () => checkDetails().filter((check) => getCiClass(check.state) === "failure");
	const unfinishedChecks = () => checkDetails().filter((check) => getCiClass(check.state) !== "success");
	const passedChecks = () => checkDetails().filter((check) => getCiClass(check.state) === "success");
	const [showPassed, setShowPassed] = createSignal(false);
	const checksFailed = () => (checkSummary()?.failed ?? 0) > 0 || failedChecks().length > 0;
	/** Red CI opens Checks, except for a reviewer: CI is the author's job and the pill already says it. */
	const checksLead = () => checksFailed() && nextAction() !== "review";
	const hasOpenThreads = () => {
		const pr = prData();
		return !!pr && (pr.unresolved_threads > 0 || pr.unresolved_threads_truncated);
	};
	const preview = createWorkItemPreview(() => prData()?.url || undefined);
	// Loaded with the panel so the Description summary can show its length (ui-v3 reference).
	preview.load();

	/** Agent prompts and workflows that fit the next action, as one split button (ui-v3). */
	const promptFor = (): string => {
		switch (nextAction()) {
			case "resolve-conflicts":
				return "smart-resolve-conflicts";
			case "fix-ci":
				return "smart-fix-ci";
			case "address-comments":
				return "smart-review-comments";
			default:
				return "smart-review-pr";
		}
	};
	/** The agent split button leads the row when the next step is agent work (ui-v3 table). */
	const agentLeads = () => nextAction() === "fix-ci" || nextAction() === "address-comments";
	const agentLabel = (): string | undefined => {
		switch (nextAction()) {
			case "review":
				return t("prNext.agentReview", "Review");
			case "fix-ci":
				return t("prNext.agentFixCi", "Fix CI");
			case "address-comments":
				return t("prNext.agentComments", "Address comments");
			default:
				return undefined;
		}
	};
	/** CI as one status pill: failures win, then running checks, then all green. */
	const ciPill = (): { text: string; severity: keyof typeof READINESS_CLASSES } | null => {
		const cs = checkSummary() ?? prData()?.checks;
		if (!cs?.total) return null;
		if (cs.failed > 0) return { text: `✗ ${cs.failed}/${cs.total}`, severity: "critical" };
		if (cs.pending > 0) return { text: `◌ ${cs.pending}/${cs.total}`, severity: "warn" };
		return { text: `✓ ${cs.passed}/${cs.total}`, severity: "ok" };
	};
	const nextActionReason = (): string | null => {
		const pr = prData();
		if (!pr) return null;
		switch (nextAction()) {
			case "review":
				return t("prNext.review", "Waiting for your review");
			case "resolve-conflicts":
				return t("prNext.conflicts", "Conflicts with {base} block the merge", { base: pr.base_ref_name || "main" });
			case "fix-ci":
				return t("prNext.fixCi", "{count} failed checks block the merge", {
					count: String(checkSummary()?.failed ?? pr.checks?.failed ?? 0),
				});
			case "address-comments":
				return pr.review_decision === "CHANGES_REQUESTED"
					? t("prNext.changesRequested", "Changes requested")
					: t("prNext.threads", "{count} unresolved threads", {
							count: `${pr.unresolved_threads}${pr.unresolved_threads_truncated ? "+" : ""}`,
						});
			case "merge":
				return t("prNext.merge", "Approved, green and mergeable");
			case "wait-checks":
				return t("prNext.waitChecks", "{count} checks still running", {
					count: String(checkSummary()?.pending ?? pr.checks?.pending ?? 0),
				});
			default:
				return null;
		}
	};

	const renderCheck = (check: CheckDetail) => {
		// Only rows with a real details URL are interactive — an empty url keeps
		// the row inert (no dead cursor-pointer / hover), per story 096-2ac0.
		const open = check.html_url ? () => handleOpenUrl(check.html_url) : undefined;
		return (
			<div
				class={cx(s.checkItem, open && s.clickable)}
				role={open ? "button" : undefined}
				tabIndex={open ? 0 : undefined}
				title={open ? check.html_url : undefined}
				onClick={open}
				onKeyDown={open ? onClickKeyDown(open) : undefined}
			>
				<span class={cx(s.checkIcon, CI_CLASSES[getCiClass(check.state)])}>{getCiIcon(check.state)}</span>
				<span class={s.checkName}>{check.context}</span>
				<span class={cx(s.checkStatus, CI_CLASSES[getCiClass(check.state)])}>{check.state}</span>
				<Show when={isCircleCiFailure(check)}>
					<Button
						type="button"
						onKeyDown={(event) => event.stopPropagation()}
						onClick={(event) => {
							event.stopPropagation();
							void toggleCircleCiLog(check.html_url);
						}}
					>
						{t("prDetail.circleCiLog", "Log")}
					</Button>
				</Show>
				<Show when={expandedCircleCiLog() === check.html_url}>
					<Show when={circleCiLogs()[check.html_url]?.loading}>
						<span>{t("prDetail.loading", "Loading…")}</span>
					</Show>
					<Show when={circleCiLogs()[check.html_url]?.error}>
						<span>{circleCiLogs()[check.html_url]?.error}</span>
					</Show>
					<Show when={circleCiLogs()[check.html_url]?.text}>
						<pre onClick={(event) => event.stopPropagation()}>
							{stripAnsi(circleCiLogs()[check.html_url]?.text ?? "")}
						</pre>
					</Show>
				</Show>
			</div>
		);
	};

	return (
		<Show
			when={prData()}
			fallback={
				<div class={s.empty}>
					{t("prDetail.noData", "No PR data available for")} {props.branch}
				</div>
			}
		>
			{(pr) => (
				<>
					{/* Meta line (ui-v3 visual rules): author · head → base · diff · age */}
					<div class={s.meta}>
						<span class={s.author}>{pr().author}</span>
						<Show when={pr().base_ref_name}>
							<span class={s.separator}>&middot;</span>
							<span class={s.mergeDirection} title={`${pr().branch} → ${pr().base_ref_name}`}>
								<span class={s.branchName}>{pr().branch}</span>
								<span class={s.arrow}>{"→"}</span>
								<span class={s.branchName}>{pr().base_ref_name}</span>
							</span>
						</Show>
						<span class={s.separator}>&middot;</span>
						<span class={s.additions}>+{pr().additions}</span>
						<span class={s.deletions}>-{pr().deletions}</span>
						<Show when={pr().updated_at || pr().created_at}>
							<span class={s.separator}>&middot;</span>
							<span>
								{t("prDetail.updated", "Updated")} {relativeTime(pr().updated_at || pr().created_at)}
							</span>
						</Show>
					</div>

					{/* Status row: each fact once, labels inline, then the comments pill */}
					<div class={s.statusRow}>
						<Show when={!isTerminalState() && readiness()}>
							{(kind) => (
								<span
									class={cx(s.readinessBadge, READINESS_CLASSES[PR_READINESS_SEVERITY[kind()]])}
									data-readiness={kind()}
								>
									{PR_READINESS_LABELS[kind()]}
								</span>
							)}
						</Show>
						<Show when={!isTerminalState() && ciPill()}>
							{(ci) => (
								<span class={cx(s.readinessBadge, READINESS_CLASSES[ci().severity])} data-testid="pr-ci-pill">
									{ci().text}
								</span>
							)}
						</Show>
						<Show when={mergeState()}>
							{(ms) => <span class={cx(s.mergeStateBadge, MERGE_STATE_CLASSES[ms().cssClass])}>{ms().label}</span>}
						</Show>
						<Show when={reviewState()}>
							{(rs) => <span class={cx(s.reviewStateBadge, REVIEW_STATE_CLASSES[rs().cssClass])}>{rs().label}</span>}
						</Show>
						<For each={pr().labels ?? []}>
							{(label) => (
								<span
									class={s.label}
									style={{
										"background-color": label.background_color || undefined,
										"border-color": label.color ? `#${label.color}` : undefined,
										color: label.text_color || undefined,
									}}
								>
									{label.name}
								</span>
							)}
						</For>
						<button
							type="button"
							class={cx(na.commentsPill, hasOpenThreads() && na.commentsPillOpen)}
							data-control-layout="row"
							data-testid="unresolved-threads"
							disabled={!pr().url}
							onClick={openConversation}
							title={
								workItemProviderRegistry.list("github").length
									? t("prDetail.openConversation", "Open comments and review threads in Tasks")
									: t("prDetail.openConversationGithub", "Open comments on GitHub")
							}
						>
							<ToolbarIcons.comment />
							{hasOpenThreads()
								? t("prDetail.unresolvedShort", "{count} unresolved", {
										count: `${pr().unresolved_threads}${pr().unresolved_threads_truncated ? "+" : ""}`,
									})
								: "0"}
						</button>
					</div>

					{/* Next action: one row, chosen by PR state (docs/frontend/ui-v3.md) */}
					<Show when={!isTerminalState()}>
						<div class={na.nextAction} data-testid="pr-next-action" data-next-action={nextAction() ?? "none"}>
							<Show when={nextActionReason()}>{(why) => <div class={na.nextActionWhy}>{why()}</div>}</Show>
							<div class={na.nextActionRow}>
								<Show when={nextAction() === "review" && props.onApprove}>
									<Button
										size="compact"
										variant="primary"
										disabled={props.approving}
										onClick={() => props.onApprove?.()}
									>
										<ToolbarIcons.check />
										{props.approving ? t("sidebar.approving", "Approving...") : t("sidebar.approve", "Approve")}
									</Button>
								</Show>
								<Show when={nextAction() === "merge" && props.onMerge}>
									<Button
										size="compact"
										variant="primary"
										data-testid="pr-merge"
										disabled={props.merging}
										onClick={() => props.onMerge?.()}
									>
										{props.merging
											? t("sidebar.merging", "Merging...")
											: (props.mergeLabel ?? t("sidebar.merge", "Merge"))}
									</Button>
								</Show>
								<Show when={isConflicting() && nextAction() !== "review" && props.onConflictAssist}>
									<Button
										size="compact"
										variant={nextAction() === "resolve-conflicts" ? "primary" : "secondary"}
										onClick={() => props.onConflictAssist?.(pr().number)}
										title={t(
											"prDetail.resolveConflictsHint",
											"Rebase the PR branch and open an agent to resolve conflicts",
										)}
									>
										{t("prDetail.resolveConflicts", "Resolve conflicts")}
									</Button>
								</Show>
								<Show when={isConflicting() && props.onPushBranch && conflictWorktreePath()}>
									<Button
										size="compact"
										onClick={handlePush}
										title={t("prDetail.pushConflictHint", "Push the rebased/resolved branch to origin")}
									>
										{t("prDetail.push", "Push")}
									</Button>
								</Show>
								<SmartButtonStrip
									placement="pr-popover"
									repoPath={props.repoPath}
									variant={agentLeads() ? "primary" : "secondary"}
									label={agentLabel()}
									defaultPromptId={promptFor()}
									extraFilter={(p: SavedPrompt) => {
										if (p.id === "smart-fix-ci") return (checkSummary()?.failed ?? pr().checks?.failed ?? 0) > 0;
										if (p.id === "smart-resolve-conflicts") return isConflicting();
										if (p.id === "smart-review-comments")
											return pr().review_decision === "CHANGES_REQUESTED" || hasOpenThreads();
										return true;
									}}
									contextVariables={() => prContextVariables(pr())}
									workflowSubject={() => ({
										kind: "pull_request",
										number: pr().number,
										title: pr().title,
										url: pr().url,
										branch: pr().branch,
										base: pr().base_ref_name,
									})}
								/>
								<Show when={nextAction() === "address-comments"}>
									<Button size="compact" disabled={!pr().url} onClick={openConversation}>
										<ToolbarIcons.external />
										{t("prNext.openInTasks", "Open in Tasks")}
									</Button>
								</Show>
								<PrActionButtons
									repoPath={props.repoPath}
									pr={pr()}
									checks={checkDetails()}
									actions={
										nextAction() === "review"
											? ["request-changes"]
											: nextAction() === "fix-ci"
												? ["rerun"]
												: nextAction() === "merge" || nextAction() === "wait-checks"
													? ["auto-merge"]
													: []
									}
								/>
								<Show when={pr().state?.toUpperCase() === "OPEN" && pr().url}>
									{(url) => <PrReviewWorkflowAction project={props.repoPath} pullRequestUrl={url()} />}
								</Show>
							</div>
						</div>
					</Show>

					{/* Fixed toolbar and row errors, from the caller */}
					{props.children}

					{/* Accordion sections: fixed order, one auto-opens by state */}
					<PanelSection
						title={t("prDetail.description", "Description")}
						summary={descriptionSummary(preview)}
						onOpen={preview.load}
					>
						<PreviewDescription preview={preview} />
					</PanelSection>

					<Show when={checkSummary()?.total || checkDetails().length > 0}>
						<PanelSection
							title={t("prDetail.checks", "Checks")}
							autoOpen={checksLead()}
							summary={
								<Show when={checkSummary()}>
									{(cs) => (
										<span class={s.ciText}>
											<Show when={cs().failed > 0}>
												<span class={cx(s.ciCount, s.failure)}>{cs().failed} ✗</span>
											</Show>
											<Show when={cs().pending > 0}>
												<span class={cx(s.ciCount, s.pending)}>{cs().pending} ◌</span>
											</Show>
											<Show when={cs().passed > 0}>
												<span class={cx(s.ciCount, s.success)}>{cs().passed} ✓</span>
											</Show>
										</span>
									)}
								</Show>
							}
						>
							<div class={s.checks}>
								<For each={unfinishedChecks()}>{renderCheck}</For>
								<Show when={passedChecks().length > 0}>
									<Show
										when={showPassed() || unfinishedChecks().length === 0}
										fallback={
											<button
												type="button"
												class={s.showPassed}
												data-control-layout="row"
												onClick={() => setShowPassed(true)}
											>
												{t("prDetail.showPassed", "Show {count} passed", { count: String(passedChecks().length) })}
											</button>
										}
									>
										<For each={passedChecks()}>{renderCheck}</For>
									</Show>
								</Show>
							</div>
						</PanelSection>
					</Show>

					<PanelSection
						title={t("prDetail.threads", "Threads")}
						summary={
							threadSplit()
								? t("prDetail.unresolvedSplit", "{human} human, {bot} bot, {ego} ego unresolved threads", {
										human: String(threadSplit()?.human),
										bot: String(threadSplit()?.bot),
										ego: String(threadSplit()?.ego),
									})
								: hasOpenThreads()
									? t("prDetail.threadsOpen", "{count} open", {
											count: `${pr().unresolved_threads}${pr().unresolved_threads_truncated ? "+" : ""}`,
										})
									: "0"
						}
						autoOpen={hasOpenThreads() && !checksLead()}
						onOpen={preview.load}
					>
						<PreviewComments
							preview={preview}
							filter={(entry) => entry.kind === "review_thread" && !entry.value.resolved}
							empty={t("prDetail.noOpenThreads", "No open threads.")}
							onOpenAll={openConversation}
							threadRepo={props.repoPath}
						/>
					</PanelSection>

					<PanelSection
						title={t("prDetail.aiReview", "AI Review")}
						summary={
							reviewLoading()
								? t("prDetail.reviewing", "Reviewing")
								: review()
									? t("prDetail.findings", "{count} findings", { count: String(reviewFindings().length) })
									: t("prDetail.notRun", "not run")
						}
						actions={
							<Button type="button" size="compact" disabled={reviewLoading()} onClick={runAiReview}>
								{reviewLoading() ? t("prDetail.reviewing", "Reviewing") : t("prDetail.runReview", "Run")}
							</Button>
						}
						autoOpen={!!review() || !!reviewError()}
					>
						<div class={s.aiReview}>
							<Show when={reviewError()}>{(err) => <div class={s.aiReviewError}>{err()}</div>}</Show>
							<Show when={review()}>
								{/* Proof-of-work line: without it a clean review is indistinguishable
							    from a review that silently did nothing. The model is deliberately
							    not named — ego owns that choice and never tells us. */}
								{(r) => (
									<div class={s.aiReviewMeta}>
										<Show when={r().summary}>{(sum) => <div class={s.aiReviewSummary}>{sum()}</div>}</Show>
										<div>
											{r().files.length === 1
												? t("prDetail.reviewedOneFile", "1 file reviewed")
												: `${r().files.length} ${t("prDetail.reviewedFiles", "files reviewed")}`}
											{" · "}
											{t("prDetail.byEgo", "by ego")}
										</div>
									</div>
								)}
							</Show>
							<Show when={review()}>
								<Show
									when={reviewFindings().length > 0}
									fallback={<div class={s.aiReviewEmpty}>{t("prDetail.noFindings", "No findings")}</div>}
								>
									<div class={s.findingsList}>
										<For each={reviewFindings()}>
											{(finding) => (
												<label class={s.findingItem}>
													<input
														type="checkbox"
														checked={selectedFindingIds().includes(finding.id)}
														disabled={finding.line == null}
														onChange={() => toggleFinding(finding.id)}
													/>
													<SeverityIcon severity={finding.severity} />
													<span class={s.findingBody}>
														<span class={s.findingPath}>
															{finding.path}
															<Show when={finding.line}>:{finding.line}</Show>
														</span>
														<span class={s.findingMessage}>{finding.message}</span>
													</span>
												</label>
											)}
										</For>
									</div>
									<Button
										type="button"
										disabled={selectedFindings().length === 0 || reviewPosting()}
										onClick={postSelectedFindings}
									>
										{reviewPosting() ? t("prDetail.posting", "Posting") : t("prDetail.postReview", "Post review")}
									</Button>
								</Show>
							</Show>
						</div>
					</PanelSection>
				</>
			)}
		</Show>
	);
};
