import { type Component, createEffect, createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { useOutsideDismiss } from "../../hooks/useOutsideDismiss";
import { executeCleanup } from "../../hooks/usePostMergeCleanup";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { autofixBranchName, createPrArgsFromBranch } from "../../stores/autofix";
import { githubStore } from "../../stores/github";
import { mdTabsStore } from "../../stores/mdTabs";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { uiStore } from "../../stores/ui";
import type { BranchPrStatus, GitHubIssue, IssueFilterMode } from "../../types";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { writeClipboard } from "../../utils/clipboard";
import { handleOpenUrl } from "../../utils/openUrl";
import { AutofixDialog } from "../AutofixDialog/AutofixDialog";
import { ChangelogModal } from "../ChangelogModal/ChangelogModal";
import { IssueDetailContent } from "../IssueDetailPopover/IssueDetailContent";
import {
	type CleanupStep,
	PostMergeCleanupDialog,
	type StepId,
	type StepStatus,
} from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import { Button } from "../ui/Button";
import { ChevronIcon } from "../ui/ChevronIcon";
import { Select } from "../ui/Select";
import { PrSection } from "./PrSection";
import s from "./Sidebar.module.css";

const FILTER_OPTIONS: { value: IssueFilterMode; label: string }[] = [
	{ value: "disabled", label: "Disabled" },
	{ value: "assigned", label: "Assigned" },
	{ value: "created", label: "Created" },
	{ value: "mentioned", label: "Mentioned" },
	{ value: "all", label: "All open" },
];

/** Sections the panel can navigate, in visual order */
type SectionId = "my-prs" | "prs" | "issues";

/** A keyboard-navigable row: PR rows are keyed by branch, issues by number */
interface NavRow {
	section: SectionId;
	key: string;
}

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);

/** Unified GitHub panel showing remote-only PRs and Issues */
export const GitHubPanel: Component<{
	prs: BranchPrStatus[];
	allPrs: BranchPrStatus[];
	repoPath: string;
	onClose: () => void;
	onCheckout: (branchName: string) => void;
	onCreateWorktree?: (branchName: string) => void;
	onConflictAssist?: (prNumber: number) => void;
	onPushBranch?: (worktreePath: string) => void;
	onAutofix?: (issueNumber: number, prompt: string) => void;
	onCleanupActive?: (active: boolean) => void;
}> = (props) => {
	// Issue accordion state
	const [expandedIssue, setExpandedIssue] = createSignal<number | null>(null);
	// PR accordion state, lifted out of PrSection: Enter on a keyboard-navigated
	// row has to reach whichever section that row belongs to.
	const [expandedPr, setExpandedPr] = createSignal<string | null>(null);
	// Dismissed PRs per section — lifted for the same reason, so the navigable
	// row list cannot include a PR the section has hidden.
	const [dismissedPrs, setDismissedPrs] = createSignal<Record<string, number[]>>({});
	const [closingIssue, setClosingIssue] = createSignal<number | null>(null);
	const [issueActionError, setIssueActionError] = createSignal<{ num: number; msg: string } | null>(null);
	// Changelog modal visibility. The modal runs the ego turn itself on mount.
	const [showChangelog, setShowChangelog] = createSignal(false);
	// Auto-fix prompt dialog target (issue number), and in-flight PR create.
	const [autofixIssue, setAutofixIssue] = createSignal<number | null>(null);
	const [creatingPr, setCreatingPr] = createSignal<number | null>(null);

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
			// Backend verdict, not the raw field: a PR being recomputed must not vanish
			// from the list on a stale CONFLICTING (#8537).
			if (prHideConflicting && pr.conflict_state === "conflicting") return false;
			if (prHideCiFailing && (pr.checks?.failed ?? 0) > 0) return false;
			return true;
		});
	});

	const issues = createMemo(() => githubStore.getRepoIssues(props.repoPath));
	const issuesLoading = () => githubStore.state.issuesLoading;
	const circuitOpen = () => githubStore.state.circuitBreakerOpen;
	const issuesEnabled = () => settingsStore.state.issueFilter !== "disabled";

	// ── Section collapse (persisted in ui prefs, keyed by section id) ──────────
	// An untouched section falls back to its own default: PR sections start
	// collapsed when they are empty, Issues starts open.
	const sectionCollapsed = (id: SectionId, fallback: boolean) => uiStore.getGithubSectionCollapsed(id) ?? fallback;
	const toggleSection = (id: SectionId, fallback: boolean) =>
		uiStore.setGithubSectionCollapsed(id, !sectionCollapsed(id, fallback));

	const dismissedIn = (id: SectionId) => dismissedPrs()[id] ?? [];
	const visiblePrsFor = (id: SectionId, prs: BranchPrStatus[]) => {
		const hidden = dismissedIn(id);
		return hidden.length === 0 ? prs : prs.filter((pr) => !hidden.includes(pr.number));
	};

	const handleDismissPr = (id: SectionId, prNumber: number) =>
		setDismissedPrs((prev) => ({ ...prev, [id]: [...(prev[id] ?? []), prNumber] }));
	const handleShowDismissedPrs = (id: SectionId) => setDismissedPrs((prev) => ({ ...prev, [id]: [] }));

	const visibleMyPrs = createMemo(() => visiblePrsFor("my-prs", myPrs()));
	const visiblePrs = createMemo(() => visiblePrsFor("prs", filteredPrs()));

	// ── Keyboard navigation ───────────────────────────────────────────────────
	// Rows of every expanded section, in visual order. Arrow keys walk this list
	// while DOM focus stays on the panel, so no unrelated control steals it.
	const navRows = createMemo<NavRow[]>(() => {
		const rows: NavRow[] = [];
		if (viewerLogin() && !sectionCollapsed("my-prs", myPrs().length === 0)) {
			for (const pr of visibleMyPrs()) rows.push({ section: "my-prs", key: pr.branch });
		}
		if (!sectionCollapsed("prs", filteredPrs().length === 0)) {
			for (const pr of visiblePrs()) rows.push({ section: "prs", key: pr.branch });
		}
		if (issuesEnabled() && !sectionCollapsed("issues", false)) {
			for (const issue of issues()) rows.push({ section: "issues", key: String(issue.number) });
		}
		return rows;
	});

	const [activeRow, setActiveRow] = createSignal<NavRow | null>(null);

	const activeKeyFor = (section: SectionId) => {
		const row = activeRow();
		return row && row.section === section ? row.key : null;
	};

	/** Index of the active row in the current list, or -1 when it no longer exists. */
	const activeIndex = () => {
		const row = activeRow();
		if (!row) return -1;
		return navRows().findIndex((r) => r.section === row.section && r.key === row.key);
	};

	const moveActive = (delta: number) => {
		const rows = navRows();
		if (rows.length === 0) return;
		const current = activeIndex();
		// Nothing active (or the row vanished): Down starts at the top, Up at the bottom.
		const next = current === -1 ? (delta > 0 ? 0 : rows.length - 1) : clamp(current + delta, 0, rows.length - 1);
		setActiveRow(rows[next]);
	};

	const toggleExpandedIssue = (issueNumber: number) =>
		setExpandedIssue((prev) => (prev === issueNumber ? null : issueNumber));
	const toggleExpandedPr = (branch: string) => setExpandedPr((prev) => (prev === branch ? null : branch));

	const activateRow = () => {
		const row = activeRow();
		if (!row) return;
		if (row.section === "issues") {
			toggleExpandedIssue(Number(row.key));
		} else {
			toggleExpandedPr(row.key);
		}
	};

	let panelRef: HTMLDivElement | undefined;

	// The sidebar badge toggles the panel; its press must reach the toggle instead of closing first.
	useOutsideDismiss({
		inside: () => [panelRef],
		onClose: props.onClose,
		enabled: () => !cleanupCtx(),
		ignoreSelector: "[data-gh-panel-trigger]",
	});

	// Keep the keyboard row in view without moving DOM focus.
	createEffect(() => {
		if (!activeRow()) return;
		panelRef?.querySelector("[data-gh-active]")?.scrollIntoView({ block: "nearest" });
	});

	const handleKeyDown = (e: KeyboardEvent) => {
		// Escape keeps its existing behaviour: collapse an expanded issue first,
		// otherwise close the panel.
		if (e.key === "Escape") {
			if (expandedIssue()) {
				setExpandedIssue(null);
			} else {
				props.onClose();
			}
			return;
		}
		if (e.key === "ArrowDown") {
			e.preventDefault();
			moveActive(1);
			return;
		}
		if (e.key === "ArrowUp") {
			e.preventDefault();
			moveActive(-1);
			return;
		}
		if (e.key === "Enter" && activeRow()) {
			e.preventDefault();
			activateRow();
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
		writeClipboard(`#${issue.number}`).catch(() => {});
	};

	/** Create a gated draft PR from the issue's auto-fix branch. Pushing the
	 *  branch is manual (out of scope) — the user clicks this once the agent has
	 *  finished and the branch exists on the remote. */
	const handleCreatePr = async (issue: GitHubIssue) => {
		const branch = autofixBranchName(issue.number);
		if (!window.confirm(t("github.createPrConfirm", `Create a draft PR from ${branch}?`))) return;
		setCreatingPr(issue.number);
		setIssueActionError(null);
		try {
			// Resolve the repo's default branch as the PR base (fallback "main").
			let base = "main";
			try {
				const baseRefs = await invoke<{ name: string; is_default: boolean }[]>("list_base_ref_options", {
					repoPath: props.repoPath,
				});
				base = baseRefs.find((r) => r.is_default)?.name ?? baseRefs[0]?.name ?? "main";
			} catch (e) {
				appLogger.warn("github", "list_base_ref_options failed — defaulting PR base to main", { error: String(e) });
			}
			const args = createPrArgsFromBranch(props.repoPath, branch, issue, base);
			const pr = await invoke<{ number: number; url: string; title: string }>("create_pr", args);
			appLogger.info("github", `Created draft PR #${pr.number} for issue #${issue.number}`);
		} catch (e) {
			const msg = String(e);
			setIssueActionError({ num: issue.number, msg });
			appLogger.warn("github", `Failed to create PR for issue #${issue.number}`, { error: msg });
		} finally {
			setCreatingPr(null);
		}
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
				<div
					class={s.ghPanel}
					onKeyDown={handleKeyDown}
					tabIndex={-1}
					ref={(el) => {
						panelRef = el;
						onMount(() => el.focus());
					}}
				>
					{/* Rate limit warning */}
					<Show when={circuitOpen()}>
						<div class={s.ghRateLimitBanner}>
							<span>{t("github.rateLimited", "GitHub API unavailable")}</span>
							<Button
								size="compact"
								variant="danger"
								class={s.ghRetryBtn}
								onClick={() => {
									githubStore.pollIssues();
								}}
							>
								{t("github.retry", "Retry")}
							</Button>
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
						<Button
							size="compact"
							variant="ghost"
							class={s.ghChangelogBtn}
							onClick={() => mdTabsStore.addGithubOps(props.repoPath)}
							aria-label={t("github.opsDashboard", "Ops Dashboard")}
							data-tooltip={t("github.opsDashboard", "Ops Dashboard")}
							data-tooltip-pos="bottom"
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M1.5 1a.5.5 0 0 0-.5.5v13a.5.5 0 0 0 .5.5h13a.5.5 0 0 0 .5-.5v-13a.5.5 0 0 0-.5-.5h-13ZM2 14V2h12v12H2Zm2.5-3a.5.5 0 0 1 .5.5v.5a.5.5 0 0 1-1 0v-.5a.5.5 0 0 1 .5-.5Zm2.5-3a.5.5 0 0 1 .5.5V12a.5.5 0 0 1-1 0V8.5a.5.5 0 0 1 .5-.5Zm3-3a.5.5 0 0 1 .5.5V12a.5.5 0 0 1-1 0V5.5a.5.5 0 0 1 .5-.5Z" />
							</svg>
						</Button>
						<Button
							size="compact"
							variant="ghost"
							class={s.ghChangelogBtn}
							onClick={() => setShowChangelog(true)}
							aria-label={t("github.changelog", "Changelog")}
							data-tooltip={t("github.changelog", "Changelog")}
							data-tooltip-pos="bottom"
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M4 1.5A1.5 1.5 0 0 0 2.5 3v10A1.5 1.5 0 0 0 4 14.5h8a1.5 1.5 0 0 0 1.5-1.5V5.5L9.5 1.5H4Zm5 1.06L12.44 6H9.5a.5.5 0 0 1-.5-.5V2.56ZM5 8h6v1H5V8Zm0 2.5h6v1H5v-1ZM5 5.5h2v1H5v-1Z" />
							</svg>
						</Button>
						<Button size="compact" variant="ghost" class={s.ghPanelClose} onClick={props.onClose}>
							&times;
						</Button>
					</div>

					<Show when={showChangelog()}>
						<ChangelogModal repoPath={props.repoPath} onClose={() => setShowChangelog(false)} />
					</Show>

					<Show when={autofixIssue()}>
						{(num) => (
							<AutofixDialog
								repoPath={props.repoPath}
								issueNumber={num()}
								onConfirm={(prompt) => props.onAutofix?.(num(), prompt)}
								onClose={() => setAutofixIssue(null)}
							/>
						)}
					</Show>

					<div class={s.ghPanelBody}>
						{/* ── My Pull Requests section ── */}
						<Show when={viewerLogin()}>
							<PrSection
								title={t("github.myPullRequests", "My Pull Requests")}
								prs={visibleMyPrs()}
								repoPath={props.repoPath}
								icon="user"
								collapsed={sectionCollapsed("my-prs", myPrs().length === 0)}
								onToggleCollapsed={() => toggleSection("my-prs", myPrs().length === 0)}
								expandedKey={expandedPr()}
								onToggleExpanded={toggleExpandedPr}
								activeKey={activeKeyFor("my-prs")}
								dismissedCount={dismissedIn("my-prs").length}
								onDismiss={(n) => handleDismissPr("my-prs", n)}
								onShowDismissed={() => handleShowDismissedPrs("my-prs")}
								onCheckout={props.onCheckout}
								onCreateWorktree={props.onCreateWorktree}
								onConflictAssist={props.onConflictAssist}
								onPushBranch={props.onPushBranch}
								onMerged={handleMerged}
							/>
						</Show>

						{/* ── Pull Requests section ── */}
						<PrSection
							title={t("github.pullRequests", "Pull Requests")}
							prs={visiblePrs()}
							repoPath={props.repoPath}
							icon="pr"
							collapsed={sectionCollapsed("prs", filteredPrs().length === 0)}
							onToggleCollapsed={() => toggleSection("prs", filteredPrs().length === 0)}
							expandedKey={expandedPr()}
							onToggleExpanded={toggleExpandedPr}
							activeKey={activeKeyFor("prs")}
							dismissedCount={dismissedIn("prs").length}
							onDismiss={(n) => handleDismissPr("prs", n)}
							onShowDismissed={() => handleShowDismissedPrs("prs")}
							onCheckout={props.onCheckout}
							onCreateWorktree={props.onCreateWorktree}
							onConflictAssist={props.onConflictAssist}
							onPushBranch={props.onPushBranch}
							onMerged={handleMerged}
						/>

						{/* ── Issues section ── */}
						<div class={s.ghSection}>
							<Show when={issuesEnabled()}>
								<div
									class={s.ghSectionHeader}
									role="button"
									tabIndex={0}
									onClick={() => toggleSection("issues", false)}
									onKeyDown={onClickKeyDown(() => toggleSection("issues", false))}
								>
									<span class={cx(s.ghSectionChevron, !sectionCollapsed("issues", false) && s.ghSectionChevronOpen)}>
										<ChevronIcon />
									</span>
									<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
										<path d="M8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" />
										<path d="M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Z" />
									</svg>
									<span>{t("github.issues", "Issues")}</span>
									<Show when={issues().length > 0}>
										<span class={s.ghSectionCount}>{issues().length}</span>
									</Show>
								</div>
								<Show when={!sectionCollapsed("issues", false)}>
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
																class={cx(
																	s.ghItemRow,
																	activeKeyFor("issues") === String(issue.number) && s.ghItemRowActive,
																)}
																data-gh-active={activeKeyFor("issues") === String(issue.number) ? "" : undefined}
																onClick={() => toggleExpandedIssue(issue.number)}
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
																			<Button
																				size="compact"
																				variant="ghost"
																				class={cx(
																					s.ghActionBtn,
																					issue.state?.toUpperCase() === "OPEN" ? s.ghCloseBtn : s.ghReopenBtn,
																				)}
																				onClick={() => handleCloseReopenIssue(issue)}
																				disabled={closingIssue() === issue.number}
																				data-tooltip={
																					issue.state?.toUpperCase() === "OPEN"
																						? t("github.closeIssue", "Close issue")
																						: t("github.reopenIssue", "Reopen issue")
																				}
																				data-tooltip-pos="bottom"
																				data-tooltip-align="right"
																			>
																				{closingIssue() === issue.number
																					? "..."
																					: issue.state?.toUpperCase() === "OPEN"
																						? t("github.close", "Close")
																						: t("github.reopen", "Reopen")}
																			</Button>
																			<Button
																				size="compact"
																				variant="ghost"
																				class={s.ghActionBtn}
																				onClick={() => handleCopyIssueNumber(issue)}
																				data-tooltip={t("github.copyNumber", "Copy issue number")}
																				data-tooltip-pos="bottom"
																				data-tooltip-align="right"
																			>
																				#{issue.number}
																			</Button>
																			<Show when={issue.url}>
																				<Button
																					size="compact"
																					variant="ghost"
																					class={cx(s.ghActionBtn, s.ghLinkBtn)}
																					onClick={() => handleOpenUrl(issue.url)}
																					data-tooltip={t("prDetail.openOnGithub", "Open on GitHub")}
																					data-tooltip-pos="bottom"
																					data-tooltip-align="right"
																				>
																					GitHub {"↗"}
																				</Button>
																			</Show>
																			<Show when={issue.state?.toUpperCase() === "OPEN"}>
																				<Button
																					size="compact"
																					variant="ghost"
																					class={s.ghActionBtn}
																					onClick={() => setAutofixIssue(issue.number)}
																					data-tooltip={t("github.autofixTooltip", "Auto-fix in a worktree agent")}
																					data-tooltip-pos="bottom"
																					data-tooltip-align="right"
																				>
																					{t("github.autofix", "Auto-fix")}
																				</Button>
																				<Button
																					size="compact"
																					variant="ghost"
																					class={s.ghActionBtn}
																					onClick={() => handleCreatePr(issue)}
																					disabled={creatingPr() === issue.number}
																					data-tooltip={t(
																						"github.createPrTooltip",
																						"Create draft PR from auto-fix branch",
																					)}
																					data-tooltip-pos="bottom"
																					data-tooltip-align="right"
																				>
																					{creatingPr() === issue.number ? "..." : t("github.createPr", "Create PR")}
																				</Button>
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
								<Select
									class={s.ghFilterSelect}
									value={settingsStore.state.issueFilter}
									onChange={(e) => githubStore.setIssueFilter(e.currentTarget.value as IssueFilterMode)}
								>
									<For each={FILTER_OPTIONS}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
								</Select>
							</div>
						</div>
					</div>
				</div>
			</Show>
		</Portal>
	);
};
