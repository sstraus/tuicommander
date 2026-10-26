import { type Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { githubStore } from "../../stores/github";
import { githubOpsStore } from "../../stores/githubOps";
import { repositoriesStore } from "../../stores/repositories";
import { type ShellState, terminalsStore } from "../../stores/terminals";
import type { BranchPrStatus, ImprovementFocus, ImprovementProposal } from "../../types";
import { pathBasename } from "../../utils/pathUtils";
import { PR_READINESS_LABELS, PR_READINESS_SEVERITY, prReadinessOf } from "../../utils/prReadiness";
import { Button } from "../ui/Button";
import s from "./GithubOpsDashboard.module.css";

/** Live auto-fix session derived from the terminals store. */
interface AutofixSession {
	id: string;
	name: string;
	state: ShellState;
	agentType: string;
}

/** Readiness verdict for a PR's CI + merge state. */
type Verdict = "ok" | "warn" | "critical" | "muted";

const AUTOFIX_BRANCH_RE = /^autofix\/issue-/;
const IMPROVEMENT_FOCUSES: ImprovementFocus[] = ["refactor", "testing", "perf"];

/** Map a shell state to a readiness verdict for the badge coloring. */
function shellStateVerdict(state: ShellState): Verdict {
	if (state === "exited") return "muted";
	if (state === "busy") return "warn";
	return "ok"; // idle / null — waiting for input or ready
}

/** Human label for a shell state. */
function shellStateLabel(state: ShellState): string {
	if (state === "busy") return t("github.ops.running", "Running");
	if (state === "idle") return t("github.ops.idle", "Idle");
	if (state === "exited") return t("github.ops.exited", "Exited");
	return t("github.ops.starting", "Starting");
}

/** Pick the CSS badge class for a verdict. */
function badgeClass(verdict: Verdict): string {
	switch (verdict) {
		case "ok":
			return `${s.badge} ${s.badgeOk}`;
		case "warn":
			return `${s.badge} ${s.badgeWarn}`;
		case "critical":
			return `${s.badge} ${s.badgeCritical}`;
		default:
			return `${s.badge} ${s.badgeMuted}`;
	}
}

/** The card heading for a proposal: its own title, or the issue title it would file. */
function proposalTitle(p: ImprovementProposal): string {
	return p.title || p.issue_title;
}

/** GitHub Ops dashboard — five live columns for a single repo.
 *
 * Review findings and Proposals went with the embedded engine (#784-0aec) and
 * came back on ego with #795-320b. A review is now one unattended ego turn, so
 * the findings card shows Working or Done and no per-file phase, and never the
 * model — that is ego's configuration and this side is not told it. */
export const GithubOpsDashboard: Component<{ repoPath: string }> = (props) => {
	const opsState = () => githubOpsStore.getState(props.repoPath);

	const reviews = createMemo(() => Object.values(opsState().reviews).sort((a, b) => a.pr_number - b.pr_number));
	const conflicts = createMemo(() => Object.values(opsState().conflicts).sort((a, b) => a.pr_number - b.pr_number));
	const proposals = createMemo(() => opsState().proposals);
	const scanRunning = () => opsState().improvementScanRunning;
	const scanError = () => opsState().improvementScanError;
	const [creatingIssueFor, setCreatingIssueFor] = createSignal<number | null>(null);
	const [issueError, setIssueError] = createSignal<string | null>(null);

	// The store already records the scan error and the column renders it, so the
	// rejection is swallowed here rather than left to become an unhandled one.
	const runScan = (focus: ImprovementFocus) => {
		void githubOpsStore.runImprovementScan(props.repoPath, focus).catch(() => {});
	};

	const createProposalIssue = async (proposal: ImprovementProposal, index: number) => {
		setCreatingIssueFor(index);
		setIssueError(null);
		try {
			await githubOpsStore.createIssueFromProposal(props.repoPath, proposal);
		} catch (err) {
			// gh's own sentence — "gh: Not Found" names the missing repo scope and a
			// button that silently does nothing names nothing.
			setIssueError(err instanceof Error ? err.message : String(err));
		} finally {
			setCreatingIssueFor(null);
		}
	};
	// Auto-fix sessions — LIVE from the terminals store, scoped to this repo via
	// the branch → terminals mapping (branches named `autofix/issue-*`).
	const autofixSessions = createMemo<AutofixSession[]>(() => {
		const repo = repositoriesStore.get(props.repoPath);
		if (!repo) return [];
		const out: AutofixSession[] = [];
		for (const [branchName, branch] of Object.entries(repo.workspaces)) {
			if (!AUTOFIX_BRANCH_RE.test(branchName)) continue;
			for (const termId of branch.terminals) {
				const term = terminalsStore.get(termId);
				if (term?.agentType) {
					out.push({ id: termId, name: term.name, state: term.shellState, agentType: term.agentType });
				}
			}
		}
		return out;
	});

	// CI / merge readiness — re-derive when repo data changes (repo-changed →
	// bumpRevision). No polling.
	const [ciPrs, setCiPrs] = createSignal<BranchPrStatus[]>([]);
	createEffect(() => {
		repositoriesStore.getRevision(props.repoPath); // track repo revision
		setCiPrs(githubStore.getAllOpenPrs(props.repoPath));
	});

	const repoName = () => pathBasename(props.repoPath) || props.repoPath;

	return (
		<div class={s.dashboard}>
			<div class={s.header}>
				<span class={s.title}>{t("github.opsDashboard", "Ops Dashboard")}</span>
				<span class={s.subtitle}>{repoName()}</span>
			</div>

			<div class={s.columns}>
				{/* ── Review findings ── */}
				<div class={s.column}>
					<div class={s.columnTitle}>
						<span>{t("github.ops.reviewFindings", "Review findings")}</span>
						<Show when={reviews().length > 0}>
							<span class={s.columnCount}>{reviews().length}</span>
						</Show>
					</div>
					<Show
						when={reviews().length > 0}
						fallback={<div class={s.empty}>{t("github.ops.noReviews", "No reviews yet")}</div>}
					>
						<div class={s.list}>
							<For each={reviews()}>
								{(r) => (
									<div class={s.card}>
										<div class={s.cardRow}>
											<span class={s.cardLabel}>PR #{r.pr_number}</span>
											<span class={badgeClass(r.error ? "critical" : r.done ? "ok" : "warn")}>
												{r.error
													? t("github.ops.failed", "failed")
													: r.done
														? t("github.ops.done", "Done")
														: t("github.ops.working", "Working")}
											</span>
										</div>
										<span class={s.cardSub}>
											{t("github.ops.findings", "Findings")}: {r.findingsCount}
										</span>
										<Show when={r.error}>
											<span class={s.errorText}>{r.error}</span>
										</Show>
									</div>
								)}
							</For>
						</div>
					</Show>
				</div>

				{/* ── Auto-fix sessions ── */}
				<div class={s.column}>
					<div class={s.columnTitle}>
						<span>{t("github.ops.autofixSessions", "Auto-fix sessions")}</span>
						<Show when={autofixSessions().length > 0}>
							<span class={s.columnCount}>{autofixSessions().length}</span>
						</Show>
					</div>
					<Show
						when={autofixSessions().length > 0}
						fallback={<div class={s.empty}>{t("github.ops.noAutofix", "No auto-fix sessions")}</div>}
					>
						<div class={s.list}>
							<For each={autofixSessions()}>
								{(sess) => (
									<div class={s.card}>
										<div class={s.cardRow}>
											<span class={s.cardLabel}>{sess.name}</span>
											<span class={badgeClass(shellStateVerdict(sess.state))}>{shellStateLabel(sess.state)}</span>
										</div>
										<span class={s.cardSub}>{sess.agentType}</span>
									</div>
								)}
							</For>
						</div>
					</Show>
				</div>

				{/* ── Conflict assists ── */}
				<div class={s.column}>
					<div class={s.columnTitle}>
						<span>{t("github.ops.conflictAssists", "Conflict assists")}</span>
						<Show when={conflicts().length > 0}>
							<span class={s.columnCount}>{conflicts().length}</span>
						</Show>
					</div>
					<Show
						when={conflicts().length > 0}
						fallback={<div class={s.empty}>{t("github.ops.noConflicts", "No conflict assists")}</div>}
					>
						<div class={s.list}>
							<For each={conflicts()}>
								{(c) => (
									<div class={s.card}>
										<div class={s.cardRow}>
											<span class={s.cardLabel}>PR #{c.pr_number}</span>
											<span class={badgeClass(c.status === "clean" ? "ok" : "warn")}>
												{c.status ?? t("github.ops.pending", "Pending")}
											</span>
										</div>
										<Show when={c.conflicted_files.length > 0}>
											<span class={s.cardSub}>
												{c.conflicted_files.length} {t("github.ops.files", "files")}
											</span>
										</Show>
									</div>
								)}
							</For>
						</div>
					</Show>
				</div>

				{/* ── Proposals ── */}
				<div class={s.column}>
					<div class={s.columnTitle}>
						<span>{t("github.ops.proposals", "Proposals")}</span>
						<Show when={proposals().length > 0}>
							<span class={s.columnCount}>{proposals().length}</span>
						</Show>
					</div>
					<div class={s.scanButtons}>
						<For each={IMPROVEMENT_FOCUSES}>
							{(focus) => (
								<Button class={s.scanButton} disabled={scanRunning()} onClick={() => runScan(focus)}>
									{focus}
								</Button>
							)}
						</For>
					</div>
					<Show when={scanError()}>
						<div class={s.errorText}>{scanError()}</div>
					</Show>
					<Show when={issueError()}>
						<div class={s.errorText}>{issueError()}</div>
					</Show>
					<Show
						when={proposals().length > 0}
						fallback={
							<div class={s.empty}>
								{scanRunning() ? t("github.ops.scanning", "Scanning") : t("github.ops.noProposals", "No proposals")}
							</div>
						}
					>
						<div class={s.list}>
							<For each={proposals()}>
								{(p, i) => (
									<div class={s.card}>
										<div class={s.cardRow}>
											<span class={s.cardLabel}>{proposalTitle(p)}</span>
											<span class={badgeClass(p.impact === "high" ? "warn" : "muted")}>{p.impact}</span>
										</div>
										<Show when={p.summary}>
											<span class={s.cardSub}>{p.summary}</span>
										</Show>
										<div class={s.proposalMeta}>
											<span>{p.effort}</span>
											<Show when={p.labels.length > 0}>
												<span>{p.labels.slice(0, 2).join(", ")}</span>
											</Show>
										</div>
										<Button
											class={s.issueButton}
											disabled={creatingIssueFor() === i()}
											onClick={() => void createProposalIssue(p, i())}
										>
											<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
												<path d="M8 1.5A6.5 6.5 0 1 0 8 14.5 6.5 6.5 0 0 0 8 1.5ZM7.5 4h1v3.5H12v1H8.5V12h-1V8.5H4v-1h3.5V4Z" />
											</svg>
											{creatingIssueFor() === i()
												? t("github.ops.creatingIssue", "Creating")
												: t("github.ops.createIssue", "Create issue")}
										</Button>
									</div>
								)}
							</For>
						</div>
					</Show>
				</div>

				{/* ── CI / merge readiness ── */}
				<div class={s.column}>
					<div class={s.columnTitle}>
						<span>{t("github.ops.ciReadiness", "CI / merge readiness")}</span>
						<Show when={ciPrs().length > 0}>
							<span class={s.columnCount}>{ciPrs().length}</span>
						</Show>
					</div>
					<Show
						when={ciPrs().length > 0}
						fallback={<div class={s.empty}>{t("github.ops.noOpenPrs", "No open PRs")}</div>}
					>
						<div class={s.list}>
							<For each={ciPrs()}>
								{(pr) => (
									<div class={s.card}>
										<div class={s.cardRow}>
											<span class={s.cardLabel}>
												PR #{pr.number} {pr.title}
											</span>
											<span class={badgeClass(PR_READINESS_SEVERITY[prReadinessOf(pr)])}>
												{PR_READINESS_LABELS[prReadinessOf(pr)]}
											</span>
										</div>
										<span class={s.cardSub}>
											{pr.checks.passed}/{pr.checks.total} {t("github.ops.checks", "checks")}
											<Show when={pr.checks.failed > 0}>
												{" "}
												· {pr.checks.failed} {t("github.ops.failed", "failed")}
											</Show>
										</span>
									</div>
								)}
							</For>
						</div>
					</Show>
				</div>
			</div>
		</div>
	);
};
