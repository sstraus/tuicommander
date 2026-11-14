// Shared types for TUICommander

/** Repository info from git */
export interface RepoInfo {
	path: string;
	name: string;
	initials: string;
	branch: string;
	status: "clean" | "dirty" | "conflict" | "merge" | "not-git" | "unknown";
	is_git_repo: boolean;
}

/** Saved repository reference */
export interface Repository {
	path: string;
	displayName: string;
}

/** PTY configuration for creating sessions */
export interface PtyConfig {
	rows: number;
	cols: number;
	shell: string | null;
	cwd: string | null;
	/** Pre-generated stable session UUID — injected as `TUIC_SESSION` env var in the PTY. */
	tuic_session?: string | null;
	/** True only when recovering a saved identity, not when minting one. */
	restored?: boolean;
	/** Extra environment variables injected into the PTY process (e.g. agent feature flags). */
	env?: Record<string, string>;
	/** Pre-set agent type for sessions launched from a run config. Enables intent
	 *  parsing from the start even when the binary name doesn't match classify_agent. */
	agent_type?: string | null;
	/** Terminal alias this tab held before the restart (e.g. `tu-3`). Reserved by
	 *  Rust so a restored tab keeps the address other agents already know; a
	 *  malformed or taken value is ignored and a fresh alias is generated. */
	alias?: string | null;
	/** Client-provided PTY session id (browser mode only). Generated and locally
	 *  registered before the create RPC so the `session-created` SSE echo is
	 *  recognized as locally-created and does not spawn a duplicate "PTY:" tab. */
	session_id?: string;
}

/** PTY exit event data */
export interface PtyExit {
	session_id: string;
	code: number | null;
}

/**
 * IPty interface matching tauri-plugin-pty style API.
 * Implemented via usePty hook and Tauri event listeners.
 */
export interface IPty {
	/** Session identifier */
	readonly sessionId: string;
	/** Write data to the PTY */
	write(data: string): Promise<void>;
	/** Resize the PTY terminal */
	resize(rows: number, cols: number): Promise<void>;
	/** Kill/close the PTY session */
	kill(cleanupWorktree?: boolean): Promise<void>;
}

/**
 * PTY event handler types for Tauri event listeners.
 * Usage: listen<PtyExit>(`pty-exit-${sessionId}`, handler)
 */
export type PtyExitHandler = (data: PtyExit) => void;

/** Git remote + branch status (PR/CI data comes from githubStore via batch query) */
export interface GitHubStatus {
	has_remote: boolean;
	current_branch: string;
	ahead: number;
	behind: number;
}

/** CI check summary counts */
export interface CheckSummary {
	passed: number;
	failed: number;
	pending: number;
	total: number;
}

/** Individual CI check detail */
export interface CheckDetail {
	context: string;
	state: string;
	/** External details URL (CheckRun.detailsUrl / StatusContext.targetUrl); empty when the provider gives none. */
	html_url: string;
}

/** Merge state: MERGEABLE, CONFLICTING, UNKNOWN */
export type MergeableState = "MERGEABLE" | "CONFLICTING" | "UNKNOWN";
/** Backend verdict on whether the branch conflicts. `checking` means GitHub is
 *  still recomputing and `mergeable` is a stale value nobody may render. */
export type ConflictState = "conflicting" | "checking" | "clear";

/** Merge state status: BEHIND, BLOCKED, CLEAN, DIRTY, DRAFT, HAS_HOOKS, UNKNOWN, UNSTABLE */
export type MergeStateStatus =
	| "BEHIND"
	| "BLOCKED"
	| "CLEAN"
	| "DIRTY"
	| "DRAFT"
	| "HAS_HOOKS"
	| "UNKNOWN"
	| "UNSTABLE";

/** PR status for a branch from batch endpoint */
export type ReviewDecision = "APPROVED" | "CHANGES_REQUESTED" | "REVIEW_REQUIRED" | "";

/** PR label with name, color, and pre-computed display colors from Rust */
export interface PrLabel {
	name: string;
	color: string;
	text_color: string;
	background_color: string;
}

export interface BranchPrStatus {
	branch: string;
	number: number;
	title: string;
	state: string;
	url: string;
	additions: number;
	deletions: number;
	checks: CheckSummary;
	/** CI check rows. Absent from the batch (get_all_pr_statuses) payload —
	 *  populated on-demand by githubStore.loadCheckDetails() via get_ci_checks. */
	check_details?: CheckDetail[];
	author: string;
	commits: number;
	mergeable: MergeableState;
	merge_state_status: MergeStateStatus;
	review_decision: ReviewDecision;
	/** True if the authenticated viewer's latest review on this PR is APPROVED. */
	viewer_did_approve: boolean;
	labels: PrLabel[];
	is_draft: boolean;
	base_ref_name: string;
	head_ref_oid: string;
	created_at: string;
	updated_at: string;
	/** Pre-computed by the backend so badge and popover cannot disagree. */
	conflict_state: ConflictState;
	merge_state_label: { label: string; css_class: string } | null;
	review_state_label: { label: string; css_class: string } | null;
	merge_commit_allowed: boolean;
	squash_merge_allowed: boolean;
	rebase_merge_allowed: boolean;
	/** Unresolved review threads (first 50 per PR). Counts only; the bot/human split is fetched on demand. */
	unresolved_threads: number;
	/** More than 50 threads exist: `unresolved_threads` is a lower bound, 0 does not mean none open. */
	unresolved_threads_truncated: boolean;
}

/** A single match from cross-terminal buffer search */
export interface TerminalMatch {
	terminalId: string;
	terminalName: string;
	lineIndex: number; // absolute buffer line index (for scrollToLine)
	lineText: string; // full line text (translateToString)
	matchStart: number; // column offset of match start
	matchEnd: number; // column offset of match end
}

/** Agent statistics */
export interface AgentStats {
	toolUses: number;
	tokens: number;
	duration: number;
}

/** Detected agent prompt */
export interface DetectedPrompt {
	question: string;
	options: string[];
	sessionId: string;
}

/** Session state for persistence */
export interface SessionState {
	terminals: SavedTerminal[];
	savedAt: string;
}

/** Saved terminal for session restore */
export interface SavedTerminal {
	/** Presentation identity for reconnecting pane layouts after WebView recreation. */
	frontendId?: string;
	/** Live backend PTY at snapshot time, used only for same-process reconnect. */
	sessionId?: string | null;
	name: string;
	cwd: string | null;
	fontSize: number;
	agentType: import("../agents").AgentType | null;
	/** Agent session ID — enables session-specific resume (e.g. claude --resume <uuid>) */
	agentSessionId?: string | null;
	/** Stable tab UUID — injected as TUIC_SESSION env var, persists across restarts */
	tuicSession?: string | null;
	/** Run-config command used to launch (e.g. "c"), ensures resume uses the same binary */
	agentLaunchCommand?: string | null;
	/** Terminal alias (e.g. `tu-3`) — the address other agents already hold, so a
	 *  restore reserves it instead of taking a fresh number */
	alias?: string | null;
	/** Last `intent:` the agent declared — restored so the resume banner can say what
	 *  the session was doing, not just that it existed */
	agentIntent?: string | null;
	/** Last user prompt seen before the snapshot, truncated at persist time */
	lastPrompt?: string | null;
	/** Ended on purpose (Suspend): restored without a PTY, resumed by the user */
	suspended?: boolean;
}

/** GitHub Issue from GraphQL API */
export interface GitHubIssue {
	author_association?: string;
	number: number;
	title: string;
	state: string; // OPEN, CLOSED
	url: string;
	author: string;
	labels: PrLabel[]; // Reuses PrLabel — same GitHub schema
	assignees: string[];
	milestone: string | null;
	comments_count: number;
	created_at: string;
	updated_at: string;
}

/** Issue filter mode for the GitHub panel */
export interface CreatedIssue {
	number: number;
	url: string;
	title: string;
}

export type ImprovementFocus = "refactor" | "testing" | "perf";

export interface ImprovementProposal {
	title: string;
	summary: string;
	rationale: string;
	issue_title: string;
	issue_body: string;
	labels: string[];
	impact: string;
	effort: string;
}

export interface ImprovementScanResult {
	repo_path: string;
	focus: ImprovementFocus;
	proposals: ImprovementProposal[];
}

/** How bad ego said it is. */
export type FindingSeverity = "bug" | "risk" | "nit";

export interface ReviewFinding {
	path: string;
	line: number | null;
	hunk: string | null;
	severity: FindingSeverity;
	/** How sure ego said it was. Under the backend's gate it is never sent at all. */
	confidence: number;
	message: string;
}

export interface ReviewedFile {
	path: string;
	summary: string;
	findings: ReviewFinding[];
}

/**
 * One ego PR review.
 *
 * `head_sha` is a hash of the diff that was reviewed, not a git sha — it exists
 * to tell a stale review from a current one. There is deliberately no model
 * field: which model ran is ego's configuration and this side is not told.
 */
export interface PrReviewResult {
	repo_path: string;
	pr_number: number;
	head_sha: string;
	summary: string | null;
	files: ReviewedFile[];
}

export interface ChangelogResult {
	markdown: string;
	/**
	 * The structured half of the split, `null` when ego answered in prose only.
	 * The modal renders the markdown and never reads this — it is here because
	 * it is on the wire for HTTP and MCP callers, which do consume it.
	 */
	json: unknown;
}

export type IssueFilterMode = "assigned" | "created" | "mentioned" | "all" | "disabled";

/** Orchestrator stats from backend */
export interface OrchestratorStats {
	active_sessions: number;
	max_sessions: number;
	available_slots: number;
}

/**
 * Which half of a repo changed, as reported by `repo-changed`.
 *
 * `git-state` means `.git/` was written (commit, refs, index) — and, because
 * the backend's git-state emit cancels the pending working-tree one, it also
 * covers any working-tree write in the same change. `working-tree` therefore
 * means "files changed and `.git` did not", which is what lets the committed-
 * history panels skip the refresh.
 */
export type RepoChangeKind = "git-state" | "working-tree";
