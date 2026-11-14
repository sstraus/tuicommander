import type { SavedTerminal } from "../types";

/**
 * Opaque handle for one workspace. Never parse it: the only guarantee is that
 * it is unique within its repository and stable for the workspace's lifetime.
 *
 * Existing records carry their branch name as the id (see
 * `migrateRepoWorkspaces`), so the value *looks* parseable — reading a branch
 * back out of it couples callers to an implementation detail. Read
 * `WorkspaceState.branchName` instead.
 */
export type WorkspaceId = string;

/** How a workspace's files relate to the repository they came from. */
export type WorkspaceKind =
	/** The repository's own checkout. */
	| "main"
	/** A linked git worktree: refs and objects shared with the parent. */
	| "worktree";

/** `pushed_unmerged` = not merged, but every commit exists on the branch's own
 *  remote-tracking ref, so deleting the local branch loses nothing.
 *  `in_sync` = HEAD is the default branch's tip, so the workspace has no commits
 *  of its own and was never merged. Both it and `merged` satisfy the backend's
 *  `merge-base --is-ancestor` check; only `merged` describes commits that were
 *  actually integrated. */
export type WorkspaceCommitStatus =
	| "unrelated_history"
	| "unmerged"
	| "pushed_unmerged"
	| "in_sync"
	| "merged"
	| "unknown";
export type WorkspaceRemovalSafety = "safe" | "requires_force" | "unknown";

/** Backend-authored checklist; the frontend only renders its verdict and text. */
export interface ArchiveChecklist {
	checks: Array<{ passed: boolean; label: string }>;
	can_delete: boolean;
	archive_explanation: string;
	delete_explanation: string;
}

/** Backend-authored Git lifecycle verdict for one exact workspace id. */
export interface WorkspaceLifecycleStatus {
	archiveChecklist?: ArchiveChecklist;
	/** Files a removal would discard — staged, unstaged and untracked. `null`
	 *  when the inspection failed; never confuse that with 0. */
	dirtyFiles: number | null;
	/** Files not tracked by Git, included in dirtyFiles. */
	untrackedFiles?: number | null;
	/** Live backend PTYs whose working directory is in this checkout. */
	liveSessions?: Array<{ sessionId: string; name: string }>;
	/** Backend-authored removal hazards, shared with non-interactive clients. */
	warnings?: string[];
	/** Git still registers this workspace, but its checkout directory is gone. */
	missingCheckout?: boolean;
	dirtyFingerprint?: string;
	submoduleUnpushedCommits?: Array<{ path: string; count: number }>;
	commitStatus: WorkspaceCommitStatus;
	/** Which backend rule proved the verdict; absent when nothing proved it. */
	proofSource?: string;
	/** Exact branch tip and integration base the verdict was computed for. */
	tip?: string;
	base?: string;
	removalSafety: WorkspaceRemovalSafety;
	error?: string;
}

/** Why a workspace is or is not eligible for cleanup. Proof and safety stay separate. */
export type WorkspaceCleanupReason = "merged" | "uncertain" | "unmerged" | "safety-blocked";

/** What one fresh inspection proved, kept apart from what that permits. */
export interface WorkspaceCleanupRecord {
	proof: {
		workspaceId: WorkspaceId;
		tip: string | null;
		base: string | null;
		source: string | null;
		verdict: WorkspaceCommitStatus;
		/** Unix ms when the backend answered. */
		inspectedAt: number;
	};
	decision: { eligible: boolean; reason: WorkspaceCleanupReason; detail: string };
}

/** One workspace with its terminals. */
export interface WorkspaceState {
	/** Stable and opaque. Equal to the key that holds this record. */
	workspaceId: WorkspaceId;
	/** What is checked out here. Keep it explicit rather than inferring it from the key. */
	branchName: string;
	kind: WorkspaceKind;
	parentRepoPath: string | null;
	isMain: boolean; // true for main/master/develop
	isShell?: boolean; // true for non-git directory shell entries
	isRemoving?: boolean; // true while worktree removal is in progress
	worktreePath: string | null; // Path to worktree directory (null for main branch)
	terminals: string[]; // terminal IDs belonging to this workspace
	hadTerminals: boolean; // true once a terminal has been created — suppresses auto-spawn after close-all
	lastActiveTerminal: string | null; // last active terminal ID when leaving this workspace
	additions: number;
	deletions: number;
	isMerged: boolean; // true when branch is fully merged into the repo's main branch
	/** Derived on refresh; never persisted as user intent or trusted for deletion. */
	lifecycleStatus?: WorkspaceLifecycleStatus;
	/** Derived from the last fresh inspection; never persisted and never deletion authority. */
	cleanup?: WorkspaceCleanupRecord;
	lastCommitTs: number | null; // Unix milliseconds of last commit on this branch
	runCommand?: string; // Saved run command for this workspace
	savedTerminals?: SavedTerminal[]; // Persisted terminal metadata for session restore
	/** CI auto-heal: when enabled, CI failures trigger automatic agent fix cycles */
	ciAutoHeal?: { enabled: boolean; attempts: number; lastRunId?: number; healing?: boolean };
	/** True once the user collapsed the agents under this workspace row. Absent
	 *  means expanded: every branch shows its agents until someone hides them. */
	tabsCollapsed?: boolean;
}

/** One entry as it may arrive off disk: any field may be absent. */
type StoredWorkspace = Partial<WorkspaceState> & { name?: string };

/** A repo record as it may arrive off disk: pre-migration, post-migration, or empty. */
interface MigratableRepoRecord {
	branches?: Record<string, StoredWorkspace>;
	workspaces?: Record<string, StoredWorkspace>;
	/** Pre-migration name for the pointer below. Held a branch, which was the key. */
	activeBranch?: string | null;
	activeWorkspaceId?: WorkspaceId | null;
}

/**
 * Give one stored entry the identity fields the UI indexes without checking.
 *
 * Every field is filled from the key rather than defended at each read site, and
 * a record already carrying one keeps it. The `branchName` fill is the sharp
 * one: `compareBranches` calls
 * `branchName.localeCompare`, so a missing value throws inside the sidebar's
 * sort memo. Solid turns a throwing memo into an *undefined* one, so the crash
 * surfaces at the reader (`sortedBranches().length`) and names nothing that
 * leads back to the record that caused it.
 */
function repairIdentity(key: WorkspaceId, stored: StoredWorkspace): WorkspaceState {
	const { name, ...rest } = stored;
	// The legacy `name` field held the branch, and so did the key. Prefer the
	// key: it is what every existing lookup in the app already resolved by.
	void name;
	return {
		...(rest as Omit<WorkspaceState, "workspaceId" | "branchName" | "kind" | "parentRepoPath" | "worktreePath">),
		workspaceId: typeof rest.workspaceId === "string" ? rest.workspaceId : key,
		// A skewed write reaches `compareBranches`, which calls `localeCompare` on
		// it inside the sidebar's sort memo — the failure this repair exists for.
		branchName: typeof rest.branchName === "string" ? rest.branchName : key,
		kind: rest.kind ?? (rest.isMain ? "main" : "worktree"),
		parentRepoPath: asPath(rest.parentRepoPath),
		worktreePath: asPath(rest.worktreePath),
	};
}

/**
 * A path field as the UI indexes it: a string, or `null` for "none".
 *
 * The document is untrusted input, and the realistic corruption is not a hostile
 * write but a shape skew: `get_worktree_paths` became `{id: {branch, path}}` in
 * one commit, and a WebView still holding the pre-change module wrote the whole
 * record into `worktreePath` on its next refresh. That value round-trips through
 * every later load, and reaches `joinPath` — which calls `.replace` on it and
 * takes the whole app down with an error naming neither the field nor the repo.
 *
 * `null` is what every reader already handles, so a corrupt value degrades to
 * "this workspace has no separate checkout" and the next refresh writes the real
 * path back. Salvaging `.path` out of the object is deliberately not done: it
 * encodes one historical shape and would hide the next one.
 */
function asPath(value: unknown): string | null {
	return typeof value === "string" ? value : null;
}

/**
 * Bring one repository record onto workspace-keyed storage.
 *
 * The migration is the **identity function**: a record written before workspaces
 * existed keys its entries by branch name, and every one of those keys becomes
 * the workspace id verbatim. Nothing moves, nothing is renamed on disk, and no
 * id is invented for data that already works — so a document that round-trips
 * through an older build and back is still readable.
 *
 * A record already keyed by `workspaces` goes through the same repair rather
 * than straight through. It is not only the legacy shape that arrives
 * incomplete: a build that shipped a partial version of this migration writes a
 * `workspaces` document with fields missing, and skipping the repair for those
 * records means the gap survives every later load.
 *
 * Pure: returns a fresh map and never touches the record it was given. Callers
 * assign the result; `normalizeLoadedRepo` is the single seam that does so, for
 * both the hydrate path and every record adopted from another client.
 */
export function migrateRepoWorkspaces(repo: MigratableRepoRecord): Record<WorkspaceId, WorkspaceState> {
	const stored = repo.workspaces ?? repo.branches;
	if (!stored) return {};

	const workspaces: Record<WorkspaceId, WorkspaceState> = {};
	for (const [key, entry] of Object.entries(stored)) {
		workspaces[key] = repairIdentity(key, structuredClone(entry));
	}
	return workspaces;
}

/**
 * Resolve which workspace a record points at, across the rename of that pointer.
 *
 * `activeBranch` held a branch name back when the branch *was* the key, so the
 * value needs no translation — only the field name moved. An id naming no
 * workspace is dropped rather than carried: another window can remove a worktree
 * between the write and this read, and a dangling pointer indexes `workspaces`
 * to `undefined` on every access instead of failing once, here.
 */
export function migrateActiveWorkspaceId(repo: MigratableRepoRecord): WorkspaceId | null {
	const active = repo.activeWorkspaceId ?? repo.activeBranch ?? null;
	if (active === null) return null;
	const workspaces = repo.workspaces ?? repo.branches;
	return workspaces && active in workspaces ? active : null;
}

/** Branch names carry `/`, spaces and other characters an id should not. */
function sanitizeForId(branchName: string): string {
	return (
		branchName
			.replace(/[^A-Za-z0-9._-]+/g, "-")
			.replace(/^-+|-+$/g, "")
			.slice(0, 60) || "workspace"
	);
}

function randomSuffix(): string {
	const bytes = new Uint8Array(4);
	crypto.getRandomValues(bytes);
	return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Mint an id for a NEW workspace. Existing records keep the branch name they were
 * migrated with — this is only for the second workspace on a branch and beyond.
 *
 * The branch is kept in the id for legibility in logs and paths, but it is a
 * label, not data: `takenIds` is what makes the result unique, and the caller
 * must read `branchName` when it wants the branch.
 */
export function generateWorkspaceId(branchName: string, takenIds: readonly WorkspaceId[]): WorkspaceId {
	const stem = sanitizeForId(branchName);
	const taken = new Set(takenIds);
	// 32 bits of randomness collides at ~1 in 4 billion, but "unique" here is a
	// correctness property (two workspaces sharing an id lose each other's
	// terminals), so it is checked rather than assumed.
	for (let attempt = 0; attempt < 100; attempt++) {
		const id = `${stem}~${randomSuffix()}`;
		if (!taken.has(id)) return id;
	}
	throw new Error(`could not mint a unique workspace id for "${branchName}" after 100 attempts`);
}
