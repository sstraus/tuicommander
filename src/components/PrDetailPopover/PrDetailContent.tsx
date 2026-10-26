import { type Component, createEffect, createMemo, createResource, createSignal, For, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { flattenReviewFindings, postableFindings, prReviewStore, type SelectableFinding } from "../../stores/prReview";
import { repositoriesStore } from "../../stores/repositories";
import { rpc } from "../../transport";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { getCiClass, getCiIcon } from "../../utils/ciDisplay";
import { handleOpenUrl } from "../../utils/openUrl";
import { PR_READINESS_LABELS, PR_READINESS_SEVERITY, prReadinessOf } from "../../utils/prReadiness";
import { stripAnsi } from "../../utils/stripAnsi";
import { relativeTime } from "../../utils/time";
import { SeverityIcon } from "../shared/SeverityIcon";
import { Button } from "../ui/Button";
import { CiRing } from "../ui/CiRing";
import { Switch } from "../ui/Switch";
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

/** Inline toggle for CI auto-heal per branch */
const CiAutoHealToggle: Component<{ repoPath: string; branch: string }> = (props) => {
	// The PR popover knows a branch; the auto-heal counters live on a workspace
	// row, reached through the single branch->id seam (`useCiHeal` resolves the
	// same way, so the toggle and the healer address the same record).
	const workspaceId = () => repositoriesStore.workspaceIdOnBranch(props.repoPath, props.branch) ?? props.branch;
	const healState = () => repositoriesStore.state.repositories[props.repoPath]?.workspaces[workspaceId()]?.ciAutoHeal;
	const enabled = () => healState()?.enabled ?? false;
	const healing = () => healState()?.healing ?? false;
	const attempts = () => healState()?.attempts ?? 0;

	const toggle = () => {
		const turningOn = !enabled();
		const current = healState();
		repositoriesStore.setCiAutoHeal(props.repoPath, workspaceId(), {
			enabled: turningOn,
			attempts: turningOn ? (current?.attempts ?? 0) : 0,
		});
		// Enabling while the PR is already blocked: the green→red / →conflict transition
		// event has already passed, so kick off a heal attempt now instead of waiting for
		// the next one. Prefer CI failures over conflicts when both are present.
		if (turningOn) {
			const pr = githubStore.getBranchPrData(props.repoPath, props.branch);
			if (pr) {
				const failed = githubStore.getCheckSummary(props.repoPath, props.branch)?.failed ?? 0;
				if (failed > 0) {
					githubStore.triggerCiHeal(props.repoPath, props.branch, pr.number);
				} else if (pr.mergeable === "CONFLICTING") {
					githubStore.triggerConflictHeal(props.repoPath, props.branch, pr.number);
				}
			}
		}
	};

	return (
		<div class={s.autoHealRow} role="button" tabIndex={0} onClick={toggle} onKeyDown={onClickKeyDown(toggle)}>
			<label onClick={(e) => e.stopPropagation()}>
				<Switch role="checkbox" checked={enabled()} onChange={toggle} />
			</label>
			<span class={s.autoHealLabel}>{t("prDetail.autoHeal", "Auto-heal")}</span>
			<Show when={healing()}>
				<span class={s.autoHealStatus}>
					{t("prDetail.healing", "Healing")} ({attempts()}/3)
				</span>
			</Show>
			<Show when={!healing() && enabled() && attempts() > 0}>
				<span class={s.autoHealStatus}>
					{attempts()}/3 {t("prDetail.attempts", "attempts")}
				</span>
			</Show>
		</div>
	);
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
	/** Extra content rendered after CI checks (e.g. action buttons) */
	children?: JSX.Element;
}

/** Shared PR detail body: status pills, labels, merge direction, timestamps, meta, CI, checks, and Open on GitHub link.
 *  Used by both PrDetailPopover (floating) and the remote-only PR accordion (inline). */
export const PrDetailContent: Component<PrDetailContentProps> = (props) => {
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
					{/* Merge + review status pills */}
					{/* Terminal PRs show no pill, so the row would be empty */}
					<Show when={!isTerminalState() && readiness()}>
						<div class={s.statusRow}>
							<Show when={readiness()}>
								{(kind) => (
									<span
										class={cx(s.readinessBadge, READINESS_CLASSES[PR_READINESS_SEVERITY[kind()]])}
										data-readiness={kind()}
									>
										{PR_READINESS_LABELS[kind()]}
									</span>
								)}
							</Show>
							<Show when={mergeState()}>
								{(ms) => <span class={cx(s.mergeStateBadge, MERGE_STATE_CLASSES[ms().cssClass])}>{ms().label}</span>}
							</Show>
							<Show when={reviewState()}>
								{(rs) => <span class={cx(s.reviewStateBadge, REVIEW_STATE_CLASSES[rs().cssClass])}>{rs().label}</span>}
							</Show>
						</div>
					</Show>

					<Show when={!isTerminalState() && (pr().unresolved_threads > 0 || pr().unresolved_threads_truncated)}>
						<div class={s.statusRow} data-testid="unresolved-threads">
							<span class={s.reviewStateBadge}>
								{threadSplit()
									? t("prDetail.unresolvedSplit", "{human} human, {bot} bot, {ego} ego unresolved threads", {
											human: String(threadSplit()?.human),
											bot: String(threadSplit()?.bot),
											ego: String(threadSplit()?.ego),
										})
									: t("prDetail.unresolved", "{count} unresolved threads", {
											count: `${pr().unresolved_threads}${pr().unresolved_threads_truncated ? "+" : ""}`,
										})}
							</span>
						</div>
					</Show>

					{/* Labels */}
					<Show when={pr().labels?.length > 0}>
						<div class={s.labels}>
							<For each={pr().labels}>
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
						</div>
					</Show>

					{/* Merge direction */}
					<Show when={pr().base_ref_name}>
						<div class={s.mergeDirection}>
							<span class={s.branchName}>{pr().branch}</span>
							<span class={s.arrow}>{"\u2192"}</span>
							<span class={s.branchName}>{pr().base_ref_name}</span>
						</div>
					</Show>

					{/* Timestamps */}
					<Show when={pr().created_at}>
						<div class={s.timestamps}>
							<span>
								{t("prDetail.created", "Created")} {relativeTime(pr().created_at)}
							</span>
							<Show when={pr().updated_at && pr().updated_at !== pr().created_at}>
								<span class={s.separator}>&middot;</span>
								<span>
									{t("prDetail.updated", "Updated")} {relativeTime(pr().updated_at)}
								</span>
							</Show>
						</div>
					</Show>

					{/* Author + commits + diff stats */}
					<div class={s.meta}>
						<span class={s.author}>{pr().author}</span>
						<span class={s.separator}>&middot;</span>
						<span>
							{pr().commits} commit{pr().commits !== 1 ? "s" : ""}
						</span>
						<span class={s.separator}>&middot;</span>
						<span class={s.additions}>+{pr().additions}</span>
						<span class={s.deletions}>-{pr().deletions}</span>
					</div>

					{/* CI summary */}
					<Show when={checkSummary()?.total ? checkSummary() : null}>
						{(cs) => (
							<div class={s.ciSummary}>
								<CiRing passed={cs().passed} failed={cs().failed} pending={cs().pending} />
								<span class={s.ciText}>
									<Show when={cs().failed > 0}>
										<span class={cx(s.ciCount, s.failure)}>
											{cs().failed} {t("prDetail.failed", "failed")}
										</span>
									</Show>
									<Show when={cs().pending > 0}>
										<span class={cx(s.ciCount, s.pending)}>
											{cs().pending} {t("prDetail.pending", "pending")}
										</span>
									</Show>
									<Show when={cs().passed > 0}>
										<span class={cx(s.ciCount, s.success)}>
											{cs().passed} {t("prDetail.passed", "passed")}
										</span>
									</Show>
								</span>
							</div>
						)}
					</Show>

					{/* Auto-heal toggle — shown when CI is failing or the PR is conflicting */}
					<Show when={checkSummary()?.failed || pr().mergeable === "CONFLICTING"}>
						<CiAutoHealToggle repoPath={props.repoPath} branch={props.branch} />
					</Show>

					{/* Conflict resolution assist — only when the PR merge state is conflicting */}
					<Show when={isConflicting() && (props.onConflictAssist || (props.onPushBranch && conflictWorktreePath()))}>
						<div class={s.actions}>
							<Show when={props.onConflictAssist}>
								<Button
									type="button"
									onClick={() => props.onConflictAssist?.(pr().number)}
									title={t(
										"prDetail.resolveConflictsHint",
										"Rebase the PR branch and open an agent to resolve conflicts",
									)}
								>
									{t("prDetail.resolveConflicts", "Resolve conflicts")}
								</Button>
							</Show>
							<Show when={props.onPushBranch && conflictWorktreePath()}>
								<Button
									type="button"
									onClick={handlePush}
									title={t("prDetail.pushConflictHint", "Push the rebased/resolved branch to origin")}
								>
									{t("prDetail.push", "Push")}
								</Button>
							</Show>
						</div>
					</Show>

					<PrActionButtons repoPath={props.repoPath} pr={pr()} checks={checkDetails()} />

					{/* Check list */}
					<Show when={checkDetails().length > 0}>
						<div class={s.checks}>
							<For each={checkDetails()}>
								{(check) => {
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
								}}
							</For>
						</div>
					</Show>

					<div class={s.aiReview}>
						<div class={s.aiReviewHeader}>
							<span class={s.aiReviewTitle}>{t("prDetail.aiReview", "AI Review")}</span>
							<Button type="button" disabled={reviewLoading()} onClick={runAiReview}>
								{reviewLoading() ? t("prDetail.reviewing", "Reviewing") : t("prDetail.runReview", "Run")}
							</Button>
						</div>
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

					{/* Extra content (action buttons, smart prompts, open link) */}
					{props.children}
				</>
			)}
		</Show>
	);
};
