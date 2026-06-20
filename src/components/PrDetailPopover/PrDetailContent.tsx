import { type Component, createEffect, For, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { cx } from "../../utils";
import { getCiClass, getCiIcon } from "../../utils/ciDisplay";
import { relativeTime } from "../../utils/time";
import { CiRing } from "../ui/CiRing";
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

/** Map CI state strings to module classes */
const CI_CLASSES: Record<string, string> = {
	success: s.success,
	failure: s.failure,
	pending: s.pending,
};

/** Inline toggle for CI auto-heal per branch */
const CiAutoHealToggle: Component<{ repoPath: string; branch: string }> = (props) => {
	const healState = () => repositoriesStore.state.repositories[props.repoPath]?.branches[props.branch]?.ciAutoHeal;
	const enabled = () => healState()?.enabled ?? false;
	const healing = () => healState()?.healing ?? false;
	const attempts = () => healState()?.attempts ?? 0;

	const toggle = () => {
		const turningOn = !enabled();
		const current = healState();
		repositoriesStore.setCiAutoHeal(props.repoPath, props.branch, {
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
		<div class={s.autoHealRow} onClick={toggle}>
			<label class={s.autoHealToggle} onClick={(e) => e.stopPropagation()}>
				<input type="checkbox" checked={enabled()} onChange={toggle} />
				<span class={s.autoHealSlider} />
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
	/** Extra content rendered after CI checks (e.g. action buttons) */
	children?: JSX.Element;
}

/** Shared PR detail body: status pills, labels, merge direction, timestamps, meta, CI, checks, and Open on GitHub link.
 *  Used by both PrDetailPopover (floating) and the remote-only PR accordion (inline). */
export const PrDetailContent: Component<PrDetailContentProps> = (props) => {
	const prData = () => githubStore.getBranchPrData(props.repoPath, props.branch);
	const checkSummary = () => githubStore.getCheckSummary(props.repoPath, props.branch);
	const checkDetails = () => githubStore.getCheckDetails(props.repoPath, props.branch);

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
		return { label: label.label, cssClass: label.css_class };
	};

	const reviewState = () => {
		if (isTerminalState()) return null;
		const label = prData()?.review_state_label;
		if (!label) return null;
		return { label: label.label, cssClass: label.css_class };
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
					{/* Merge + review status pills */}
					<Show when={mergeState() || reviewState()}>
						<div class={s.statusRow}>
							<Show when={mergeState()}>
								{(ms) => <span class={cx(s.mergeStateBadge, MERGE_STATE_CLASSES[ms().cssClass])}>{ms().label}</span>}
							</Show>
							<Show when={reviewState()}>
								{(rs) => <span class={cx(s.reviewStateBadge, REVIEW_STATE_CLASSES[rs().cssClass])}>{rs().label}</span>}
							</Show>
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

					{/* Check list */}
					<Show when={checkDetails().length > 0}>
						<div class={s.checks}>
							<For each={checkDetails()}>
								{(check) => (
									<div class={s.checkItem}>
										<span class={cx(s.checkIcon, CI_CLASSES[getCiClass(check.state)])}>{getCiIcon(check.state)}</span>
										<span class={s.checkName}>{check.context}</span>
										<span class={cx(s.checkStatus, CI_CLASSES[getCiClass(check.state)])}>{check.state}</span>
									</div>
								)}
							</For>
						</div>
					</Show>

					{/* Extra content (action buttons, smart prompts, open link) */}
					{props.children}
				</>
			)}
		</Show>
	);
};
