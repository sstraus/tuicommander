import { batch, untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { AGENT_TYPES } from "../agents";
import type { DetachedWorktree, WorktreeDiagnostic } from "../hooks/useRepository";
import { invoke, listen } from "../invoke";
import { settingsRefresh } from "../services/settingsRefresh";
import { setRepoConnectionLookup } from "../transportRuntime";
import type { SavedTerminal } from "../types";
import { pathBasename, pathStartsWith, pathStripPrefix } from "../utils/pathUtils";
import { markPerf } from "../utils/perfTrace";
import { type RepoOwner, resolveRepoOwnerIn, sameDir } from "../utils/repoOwnership";
import { appLogger } from "./appLogger";
import { makeBranchKey } from "./tabManager";
import {
	migrateActiveWorkspaceId,
	migrateRepoWorkspaces,
	type WorkspaceId,
	type WorkspaceState,
} from "./workspaceIdentity";

const LEGACY_STORAGE_KEY = "tui-commander-repos";

/** Returns paths of repos that have at least one active terminal. */
function getHotRepoPaths(repositories: Record<string, RepositoryState>): string[] {
	return Object.entries(repositories)
		.filter(([, repo]) => Object.values(repo.workspaces).some((b) => b.terminals.length > 0))
		.map(([path]) => path);
}

function syncHotRepos(repositories: Record<string, RepositoryState>): void {
	invoke("set_hot_repos", { paths: getHotRepoPaths(repositories) }).catch((err: unknown) =>
		appLogger.warn("store", "Failed to sync hot repos", err),
	);
}

export type { WorkspaceId, WorkspaceKind, WorkspaceState } from "./workspaceIdentity";

/** Repository with workspaces */
export interface RepositoryState {
	path: string;
	displayName: string;
	initials: string;
	isGitRepo?: boolean; // false for plain directories (defaults to true for backward compat)
	expanded: boolean; // Whether workspaces are expanded/collapsed
	collapsed: boolean; // Whether entire repo is collapsed to icon only
	parked: boolean; // Whether repo is hidden from sidebar (recallable via popover)
	workspaces: Record<WorkspaceId, WorkspaceState>;
	/** Transient backend inventory; never used as workspace identity or persisted. */
	detachedWorktrees?: DetachedWorktree[];
	worktreeDiagnostics?: WorktreeDiagnostic[];
	/** Which workspace is on screen. Indexes `workspaces`, so it is an id — read
	 *  `workspaces[activeWorkspaceId].branchName` when you want the branch. */
	activeWorkspaceId: WorkspaceId | null;
	/** Backend-authored stable agent placement; retained by repository persistence. */
	declaredWorktrees?: Record<string, { workspaceId: string; branch: string; worktreePath: string }>;
	/** Which remote connection this repo belongs to (undefined = local) */
	connectionId?: string;
}

/**
 * A repository row the backend classifier found stale: a non-existent local
 * path under a recognized temp root, holding nothing but an empty shell
 * workspace (#763-d219). Quarantined from the normal sidebar listing —
 * `getGroupedLayout`/`getOrderedRepos` both exclude it — until an explicit
 * `repairStaleTemp` call removes it; never deleted implicitly.
 */
export interface StaleTempCandidate {
	path: string;
	displayName: string;
}

/** Paths currently quarantined by the stale-temp classifier (#763-d219) —
 *  shared by `getGroupedLayout` and `getOrderedRepos`, the two listings that
 *  must exclude them. */
function quarantinedPaths(candidates: StaleTempCandidate[]): Set<string> {
	return new Set(candidates.map((c) => c.path));
}

/** What `repairStaleTemp` actually did, from the backend's own transactional write. */
export interface StaleTempRepairSummary {
	removed: string[];
	backupPath: string;
}

/** A named, colored group of repositories */
export interface RepoGroup {
	id: string;
	name: string;
	color: string; // hex color or "" for default
	collapsed: boolean; // accordion state
	repoOrder: string[]; // ordered repo paths in this group
}

/** Repositories store state */
interface RepositoriesStoreState {
	repositories: Record<string, RepositoryState>;
	repoOrder: string[]; // ungrouped repo order
	activeRepoPath: string | null;
	/** Per-repo monotonic revision counter, bumped by repo-changed events */
	revisions: Record<string, number>;
	/**
	 * Narrower counter, bumped only by the `git-state` half of repo-changed
	 * (`.git/` writes: commits, refs, index). Panels that read committed history
	 * subscribe to this one so a plain file save no longer re-runs their git
	 * processes. A strict subset of `revisions`, which still bumps on every
	 * event — a panel left on `getRevision` therefore cannot go stale.
	 */
	gitRevisions: Record<string, number>;
	groups: Record<string, RepoGroup>;
	groupOrder: string[]; // display order of group IDs
	/** True while a branch switch is in progress — TabBar holds previous tabs */
	branchSwitching: boolean;
	/** Backend-classified stale-temp ghost rows, quarantined from the sidebar
	 *  pending an explicit repair (#763-d219). Populated by
	 *  `refreshStaleTempCandidates`, never computed on the frontend — see
	 *  AGENTS.md "Architecture". */
	staleTempCandidates: StaleTempCandidate[];
}

/** Grouped layout returned by getGroupedLayout() */
export interface GroupedLayout {
	groups: Array<{ group: RepoGroup; repos: RepositoryState[] }>;
	ungrouped: RepositoryState[];
}

/** Same length, same element identities. Cheap enough to run per layout call,
 *  and the only question the layout cache below has to answer. */
function sameRefs<T>(a: readonly T[], b: readonly T[]): boolean {
	if (a.length !== b.length) return false;
	for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
	return true;
}

/** Fallback check when Rust-provided is_main is not available (e.g. local rename). */
function isMainBranch(branchName: string): boolean {
	const mainBranches = ["main", "master", "develop", "development", "dev"];
	return mainBranches.includes(branchName.toLowerCase());
}

const SAVE_DEBOUNCE_MS = 500;

interface RepositorySnapshot {
	repos: Record<string, RepositoryState>;
	repoOrder: string[];
	activeRepoPath: string | null;
	groups: Record<string, RepoGroup>;
	groupOrder: string[];
}

interface KeyedRepositoryMutation {
	id: string;
	before: RepositoryState | RepoGroup | null;
	after: RepositoryState | RepoGroup | null;
}

interface RepositoryFieldMutation<T> {
	before: T;
	after: T;
}

interface RepositoryMutationBatch {
	mutationVersion: 1;
	repos: KeyedRepositoryMutation[];
	groups: KeyedRepositoryMutation[];
	repoOrder?: RepositoryFieldMutation<string[]>;
	activeRepoPath?: RepositoryFieldMutation<string | null>;
	groupOrder?: RepositoryFieldMutation<string[]>;
}

function cloneJson<T>(value: T): T {
	return JSON.parse(JSON.stringify(value)) as T;
}

/**
 * The fields a partial update actually names. `undefined` means "leave it
 * alone", never "blank it".
 *
 * A caller that reads a field off a payload which changed shape writes
 * `undefined` without noticing, and a spread puts that over a required field.
 * This is how `branchName` disappeared from 31 of 38 repos on disk: `make dev`
 * never restarts the Rust side, so a running backend still answered
 * `worktree_paths` as branch → path while the reloaded frontend read
 * `wt.branch` off a string. Every refresh then blanked the field, and every
 * save persisted it. The skew is the caller's bug; letting it reach disk was
 * this store's.
 */
function definedFields<T extends object>(data: T): Partial<T> {
	return Object.fromEntries(Object.entries(data).filter(([, value]) => value !== undefined)) as Partial<T>;
}

function emptyRepositorySnapshot(): RepositorySnapshot {
	return { repos: {}, repoOrder: [], activeRepoPath: null, groups: {}, groupOrder: [] };
}

/** Capture the exact document loaded from disk before in-memory migrations add
 * defaults. The next mutation then persists those migrations as part of its
 * keyed delta instead of assuming the old fields were already written. */
function snapshotFromLoaded(value: Partial<RepositorySnapshot> | null | undefined): RepositorySnapshot {
	return cloneJson({
		repos: value?.repos ?? {},
		repoOrder: value?.repoOrder ?? [],
		activeRepoPath: value?.activeRepoPath ?? null,
		groups: value?.groups ?? {},
		groupOrder: value?.groupOrder ?? [],
	});
}

/** One repository as it goes to disk: the fields that live only in this window's
 *  memory are stripped, so two clients holding the same document agree on it. */
function serializableRepo(repo: RepositoryState): RepositoryState {
	const workspaces: Record<string, WorkspaceState> = {};
	for (const [name, branch] of Object.entries(repo.workspaces)) {
		const persisted: WorkspaceState = { ...branch, terminals: [] };
		// `healing` is a transient runtime flag; never persist it (a crash mid-heal
		// would otherwise leave the toggle showing "Healing" forever after reload).
		if (persisted.ciAutoHeal?.healing) {
			persisted.ciAutoHeal = { ...persisted.ciAutoHeal, healing: false };
		}
		workspaces[name] = persisted;
	}
	const { detachedWorktrees: _inventory, worktreeDiagnostics: _diagnostics, ...persistedRepo } = repo;
	return { ...persistedRepo, workspaces };
}

/**
 * Bring a record read from `repositories.json` up to the shape the UI indexes.
 *
 * Applied on hydrate *and* on every record adopted from another client: a second
 * client can be an older build, or can echo back a document a legacy migration
 * wrote, and the fields defaulted here are read as always-present. The `agentType`
 * scrub is the sharp one — an unknown name throws inside a render no ErrorBoundary
 * covers.
 *
 * This is also the single seam the `branches` → `workspaces` migration runs on.
 * Both the hydrate path and every adopted record pass through here, and
 * `repositoryIntentView` normalises the on-disk baseline the same way — so a
 * document still holding the old key compares equal to its migrated self and the
 * compare-and-swap does not read the migration as a competing edit.
 */
function normalizeLoadedRepo(repo: RepositoryState): void {
	// A record persisted (or adopted from another client) can carry a missing,
	// non-string, or blank `displayName` — every consumer (sidebar, Command Palette
	// sort/labels) reads it as always-present text, and `CommandPalette`'s
	// `baseSort` crashes the whole app on `undefined.localeCompare` (#763-d219).
	// The path-derived fallback is deterministic across restarts/clients, unlike a
	// random or counter-based placeholder, and cross-platform via `pathBasename`.
	if (typeof repo.displayName !== "string" || repo.displayName.trim() === "") {
		const fallback = pathBasename(repo.path) || repo.path || "Unnamed Repository";
		appLogger.warn("store", "Repository record had an invalid displayName; using path-derived fallback", {
			path: repo.path,
			displayName: repo.displayName,
			fallback,
		});
		repo.displayName = fallback;
	}
	if (repo.collapsed === undefined) repo.collapsed = false;
	if (repo.expanded === undefined) repo.expanded = true;
	if (repo.parked === undefined) repo.parked = false;
	// Migration: remove legacy showAllBranches field
	delete (repo as unknown as Record<string, unknown>).showAllBranches;
	repo.activeWorkspaceId = migrateActiveWorkspaceId(repo);
	repo.workspaces = migrateRepoWorkspaces(repo);
	delete (repo as unknown as Record<string, unknown>).branches;
	delete (repo as unknown as Record<string, unknown>).activeBranch;
	for (const branch of Object.values(repo.workspaces)) {
		branch.terminals = [];
		// Reset hadTerminals on startup: the flag only suppresses auto-spawn
		// within a session (after user closes all terminals). Across restarts,
		// auto-spawn should work unless savedTerminals will restore them.
		branch.hadTerminals = !!branch.savedTerminals?.length;
		if (branch.savedTerminals === undefined) {
			branch.savedTerminals = [];
		}
		// A build that drops an agent leaves its name behind on disk — `fx`
		// was first-class for five days before being reverted. `AGENT_DISPLAY`
		// and `AGENTS` are exhaustive `Record<AgentType, …>` indexed without an
		// existence check, so a stale name throws inside a render that no
		// ErrorBoundary covers. Drop it once here rather than making every
		// index site defend itself.
		for (const saved of branch.savedTerminals) {
			if (saved.agentType !== null && !AGENT_TYPES.includes(saved.agentType)) {
				saved.agentType = null;
			}
		}
		if (branch.isMerged === undefined) {
			branch.isMerged = false;
		}
	}
}

function serializableSnapshot(
	repositories: Record<string, RepositoryState>,
	repoOrder: string[],
	activeRepoPath: string | null | undefined,
	groups: Record<string, RepoGroup>,
	groupOrder: string[],
): RepositorySnapshot {
	const serializable: Record<string, RepositoryState> = {};
	for (const [path, repo] of Object.entries(repositories)) {
		serializable[path] = serializableRepo(repo);
	}
	return cloneJson({
		repos: serializable,
		repoOrder: [...repoOrder],
		activeRepoPath: activeRepoPath ?? null,
		groups: Object.fromEntries(
			Object.entries(groups).map(([id, group]) => [id, { ...group, repoOrder: [...group.repoOrder] }]),
		),
		groupOrder: [...groupOrder],
	});
}

function jsonEqual(left: unknown, right: unknown): boolean {
	return JSON.stringify(left) === JSON.stringify(right);
}

/** The workspaces of `repo` this window still has an open terminal in. */
function liveBranchNames(repo: RepositoryState | undefined): string[] {
	if (!repo) return [];
	return Object.entries(repo.workspaces)
		.filter(([, branch]) => branch.terminals.length > 0)
		.map(([name]) => name);
}

/** True while any branch of the repo still holds an open terminal. */
function hasLiveTerminals(repo: RepositoryState | undefined): boolean {
	return liveBranchNames(repo).length > 0;
}

/**
 * Branch fields a window caches rather than intends: every client recomputes them
 * from the repository itself, on its own refresh cadence. Mirrors
 * `DERIVED_BRANCH_FIELDS` in `config.rs` — the backend already excludes them from
 * its compare-and-swap, for the same reason this file excludes them from the
 * "does this window have unsaved intent" test.
 */
const DERIVED_BRANCH_FIELDS = [
	"additions",
	"deletions",
	"isMerged",
	"lifecycleStatus",
	"lastActiveTerminal",
	"lastCommitTs",
	// Persisted, but session state all the same: it suppresses auto-spawn after the
	// user closes every pane, and `hydrate` recomputes it from `savedTerminals` on
	// the next start. Another window's copy is not an edit to defend.
	"hadTerminals",
];

/**
 * What this window *meant* a record to say, for comparison only.
 *
 * Two differences must not read as an edit. `updateWorkspaceStats` moves a diffstat
 * without saving, so a repo under active work drifts from its own baseline every few
 * seconds — comparing whole records would refuse every remote change for exactly the
 * repos the user is working in. And the baseline is the document as it came off disk,
 * *before* `normalizeLoadedRepo` added the migration defaults the store holds, so a
 * record written by an older build differs on fields nobody touched.
 */
function repositoryIntentView(record: RepositoryState | null): unknown {
	if (!record) return null;
	const view = cloneJson(record);
	normalizeLoadedRepo(view);
	const workspaces = (view as unknown as { workspaces: Record<string, Record<string, unknown>> }).workspaces;
	for (const branch of Object.values(workspaces)) {
		for (const field of DERIVED_BRANCH_FIELDS) delete branch[field];
	}
	return view;
}

/** Re-apply the fields this window owns onto a record read from disk.
 *  Tab placement, an in-flight CI heal and the derived stats exist only in this
 *  window's memory or its own refresh cycle, so a record another client wrote would
 *  otherwise blank them or set them back. */
function withLiveBranchFields(fresh: RepositoryState, live: RepositoryState | undefined): RepositoryState {
	const incoming = cloneJson(fresh);
	if (!live) return incoming;
	const workspaces: Record<string, WorkspaceState> = {};
	for (const [name, branch] of Object.entries(incoming.workspaces)) {
		const liveBranch = live.workspaces[name];
		if (!liveBranch) {
			workspaces[name] = branch;
			continue;
		}
		workspaces[name] = {
			...branch,
			terminals: [...liveBranch.terminals],
			hadTerminals: liveBranch.hadTerminals,
			additions: liveBranch.additions,
			deletions: liveBranch.deletions,
			isMerged: liveBranch.isMerged,
			lifecycleStatus: liveBranch.lifecycleStatus,
			lastActiveTerminal: liveBranch.lastActiveTerminal,
			lastCommitTs: liveBranch.lastCommitTs,
			ciAutoHeal:
				branch.ciAutoHeal && liveBranch.ciAutoHeal?.healing
					? { ...branch.ciAutoHeal, healing: true }
					: branch.ciAutoHeal,
		};
	}
	// The repo-level live-terminal rule, at branch granularity: a branch another
	// client deleted while this window still has a pane open in it stays. Dropping it
	// leaves the pane running with nothing in `workspaces` owning it — invisible to the
	// tab strip, and unreachable through `findOwnerForTerminal`.
	for (const name of liveBranchNames(live)) {
		if (!workspaces[name]) workspaces[name] = cloneJson(live.workspaces[name]);
	}
	return { ...incoming, workspaces };
}

function keyedMutations<T extends RepositoryState | RepoGroup>(
	before: Record<string, T>,
	after: Record<string, T>,
): KeyedRepositoryMutation[] {
	const mutations: KeyedRepositoryMutation[] = [];
	for (const id of new Set([...Object.keys(before), ...Object.keys(after)])) {
		const previous = before[id] ?? null;
		const next = after[id] ?? null;
		if (!jsonEqual(previous, next)) {
			mutations.push({ id, before: previous, after: next });
		}
	}
	return mutations;
}

function repositoryMutationBatch(before: RepositorySnapshot, after: RepositorySnapshot): RepositoryMutationBatch {
	const mutation: RepositoryMutationBatch = {
		mutationVersion: 1,
		repos: keyedMutations(before.repos, after.repos),
		groups: keyedMutations(before.groups, after.groups),
	};
	if (!jsonEqual(before.repoOrder, after.repoOrder)) {
		mutation.repoOrder = { before: before.repoOrder, after: after.repoOrder };
	}
	if (before.activeRepoPath !== after.activeRepoPath) {
		mutation.activeRepoPath = { before: before.activeRepoPath, after: after.activeRepoPath };
	}
	if (!jsonEqual(before.groupOrder, after.groupOrder)) {
		mutation.groupOrder = { before: before.groupOrder, after: after.groupOrder };
	}
	return mutation;
}

function hasRepositoryMutations(mutation: RepositoryMutationBatch): boolean {
	return (
		mutation.repos.length > 0 ||
		mutation.groups.length > 0 ||
		mutation.repoOrder !== undefined ||
		mutation.activeRepoPath !== undefined ||
		mutation.groupOrder !== undefined
	);
}

/**
 * True when `repositories.json` holds a mutation delta instead of a repository
 * document.
 *
 * A backend too old to understand the keyed delta decodes it as the whole file and
 * writes it back wholesale. That is not hypothetical: on 2026-08-21 a hot-reloaded
 * frontend met a stale Rust process and 35 repositories went from an 11 KB document
 * to `{"mutationVersion":1,"repos":[]}`. The tell is `mutationVersion` at the root,
 * or `repos` as an array where a record belongs.
 */
function isMutationDeltaDocument(loaded: unknown): boolean {
	if (!loaded || typeof loaded !== "object") return false;
	const doc = loaded as { mutationVersion?: unknown; repos?: unknown };
	return doc.mutationVersion !== undefined || Array.isArray(doc.repos);
}

const DELTA_DOCUMENT_MESSAGE =
	"repositories.json holds a mutation delta, not a repository document — a backend too old for the delta protocol wrote it wholesale. Refusing to hydrate, and saves stay blocked. Stop the app, restore a backup, then restart the backend.";

/** The backend rejects a keyed mutation whose `before` no longer matches disk. */
function isRepositoryConflict(error: unknown): boolean {
	const message = error instanceof Error ? error.message : String(error);
	return message.includes("repository configuration conflict");
}

type LoadedRepositoryDocument = {
	repos?: Record<string, RepositoryState>;
	repoOrder?: string[];
	activeRepoPath?: string | null;
	groups?: Record<string, RepoGroup>;
	groupOrder?: string[];
};

/** Re-read the persisted document. Returns null for a delta document, which must
 * never be treated as repository state (see `isMutationDeltaDocument`). */
async function loadPersistedSnapshot(): Promise<RepositorySnapshot | null> {
	const loaded = await invoke<LoadedRepositoryDocument>("load_repositories");
	if (isMutationDeltaDocument(loaded)) {
		appLogger.error("store", DELTA_DOCUMENT_MESSAGE);
		return null;
	}
	return snapshotFromLoaded(loaded);
}

function rebaseKeyed<T extends RepositoryState | RepoGroup>(
	mutations: KeyedRepositoryMutation[],
	records: Record<string, T>,
): KeyedRepositoryMutation[] {
	return mutations
		.map((mutation) => ({ ...mutation, before: records[mutation.id] ?? null }))
		.filter((mutation) => !jsonEqual(mutation.before, mutation.after));
}

/** Re-express a rejected batch against the document currently on disk: every
 * `after` is kept (this client's intent) and every `before` is taken from disk.
 * Only the records this client meant to change are touched — resetting the whole
 * baseline instead would make the next diff revert whatever another writer changed
 * in the meantime, because this client's in-memory copy of those records is stale. */
function rebaseMutationBatch(batch: RepositoryMutationBatch, fresh: RepositorySnapshot): RepositoryMutationBatch {
	const rebased: RepositoryMutationBatch = {
		mutationVersion: 1,
		repos: rebaseKeyed(batch.repos, fresh.repos),
		groups: rebaseKeyed(batch.groups, fresh.groups),
	};
	if (batch.repoOrder && !jsonEqual(fresh.repoOrder, batch.repoOrder.after)) {
		rebased.repoOrder = { before: fresh.repoOrder, after: batch.repoOrder.after };
	}
	if (batch.activeRepoPath && fresh.activeRepoPath !== batch.activeRepoPath.after) {
		rebased.activeRepoPath = { before: fresh.activeRepoPath, after: batch.activeRepoPath.after };
	}
	if (batch.groupOrder && !jsonEqual(fresh.groupOrder, batch.groupOrder.after)) {
		rebased.groupOrder = { before: fresh.groupOrder, after: batch.groupOrder.after };
	}
	return rebased;
}

/** Guard: prevent saves before hydrate completes to avoid nuking persisted data */
let hydrated = false;

/** Last snapshot this client knows it persisted. Saves are serialized so a
 * second local mutation never races the first with the same stale expectation. */
let persistedSnapshot: RepositorySnapshot | null = null;
let queuedSnapshot: RepositorySnapshot | null = null;
let saveInFlight = false;
let saveCompletion: Promise<void> = Promise.resolve();
let saveError: unknown;

/** Set by the store so an adoption that arrived mid-save can run once the baseline
 *  is settled. A save writes `persistedSnapshot` when its request resolves, which
 *  would otherwise overwrite a baseline an adoption moved while it was in flight. */
let onSaveSettled: (() => void) | null = null;

function drainRepositorySaveQueue(): void {
	if (saveInFlight || !queuedSnapshot) return;
	const next = queuedSnapshot;
	queuedSnapshot = null;
	const before = persistedSnapshot ?? emptyRepositorySnapshot();
	const mutation = repositoryMutationBatch(before, next);
	if (!hasRepositoryMutations(mutation)) {
		persistedSnapshot = next;
		if (queuedSnapshot) drainRepositorySaveQueue();
		return;
	}

	saveInFlight = true;
	saveError = undefined;
	saveCompletion = persistRepositoryMutation(mutation, next)
		.catch((err: unknown) => {
			saveError = err;
			// An error-level entry increments the visible Errors badge. In particular,
			// same-record conflicts must never remain a debug-only lost mutation.
			appLogger.error("store", "Repository changes were not saved", err);
			// Do not requeue the failed snapshot: a deterministic conflict must not
			// spin. Preserve a newer snapshot queued while this request was in flight;
			// it is a distinct user mutation and still deserves one visible attempt.
		})
		.finally(() => {
			saveInFlight = false;
			onSaveSettled?.();
			if (queuedSnapshot) drainRepositorySaveQueue();
		});
}

/**
 * Persist one mutation batch, recovering from a compare-and-swap rejection.
 *
 * `persistedSnapshot` is the baseline every later diff is computed from. Leaving it
 * stale after a rejection wedged the client permanently: the next save diffed against
 * a document disk had already moved past, so it was rejected too — for every repo, not
 * just the conflicting one, for the life of the session. One backend serves several
 * clients (desktop WebView, browser, PWA) and a successful save broadcasts nothing, so
 * a stale baseline is routine, not exotic.
 */
async function persistRepositoryMutation(mutation: RepositoryMutationBatch, next: RepositorySnapshot): Promise<void> {
	try {
		await invoke("save_repositories", { config: mutation });
	} catch (err) {
		if (!isRepositoryConflict(err)) throw err;
		const fresh = await loadPersistedSnapshot();
		if (!fresh) throw err;
		const rebased = rebaseMutationBatch(mutation, fresh);
		// An empty rebase means disk already holds what we wanted; nothing left to write.
		if (hasRepositoryMutations(rebased)) {
			await invoke("save_repositories", { config: rebased });
		}
	}
	// `next`, not the document just read. The baseline must stay in step with this
	// client's own in-memory state: a record another writer changed is equally stale
	// in both, so it yields no mutation and their write survives. Adopting disk here
	// would instead make the next diff revert it. Convergence between clients is the
	// job of the `repositories-changed` broadcast, which moves the baseline and the
	// store together — see `adoptRemoteRepositories`.
	persistedSnapshot = next;
}

/** Persist repos to Rust backend (fire-and-forget, terminals excluded) */
function saveReposImmediate(
	repositories: Record<string, RepositoryState>,
	repoOrder: string[],
	activeRepoPath: string | null | undefined,
	groups: Record<string, RepoGroup>,
	groupOrder: string[],
): void {
	if (!hydrated) {
		appLogger.warn("store", "Repositories save blocked — hydrate not yet complete");
		return;
	}
	queuedSnapshot = serializableSnapshot(repositories, repoOrder, activeRepoPath, groups, groupOrder);
	drainRepositorySaveQueue();
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** Debounced save — coalesces rapid mutations into a single IPC call */
function saveRepos(
	repositories: Record<string, RepositoryState>,
	repoOrder: string[],
	activeRepoPath: string | null | undefined,
	groups: Record<string, RepoGroup>,
	groupOrder: string[],
): void {
	if (saveTimer) clearTimeout(saveTimer);
	saveTimer = setTimeout(() => {
		saveTimer = null;
		saveReposImmediate(repositories, repoOrder, activeRepoPath, groups, groupOrder);
	}, SAVE_DEBOUNCE_MS);
}

/** Generate a unique group ID */
function generateGroupId(): string {
	return `grp-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
}

/** Create the repositories store */
function createRepositoriesStore() {
	/** Wrapper reuse for `getGroupedLayout`, keyed by group id. Bounded by the
	 *  number of groups, and pruned there when a group disappears. */
	const groupLayoutCache = new Map<string, GroupedLayout["groups"][number]>();
	let lastLayout: GroupedLayout | null = null;

	const [state, setState] = createStore<RepositoriesStoreState>({
		repositories: {},
		repoOrder: [],
		activeRepoPath: null,
		revisions: {},
		gitRevisions: {},
		groups: {},
		groupOrder: [],
		branchSwitching: false,
		staleTempCandidates: [],
	});

	// Inverse index: terminal ID → repo path (O(1) lookup instead of O(repos*workspaces*terminals)).
	// Maps termId→repoPath only (NOT branchName). renameBranch and mergeWorkspaceState don't update
	// this map because they never change the repoPath — terminals stay in the same repo.
	const terminalToRepo = new Map<string, string>();

	/** Debounced save shorthand using current state */
	const save = () =>
		saveRepos(state.repositories, state.repoOrder, state.activeRepoPath, state.groups, state.groupOrder);

	/** Immediate save shorthand using current state (for app exit) */
	const saveNow = () =>
		saveReposImmediate(state.repositories, state.repoOrder, state.activeRepoPath, state.groups, state.groupOrder);

	/** Forget a repository without persisting — the shared half of `remove()` and
	 *  of adopting a removal another client already wrote to disk. */
	const dropRepositoryFromState = (path: string): void => {
		// Clear inverse index entries for all terminals in this repo
		const repo = state.repositories[path];
		if (repo) {
			for (const branch of Object.values(repo.workspaces)) {
				for (const termId of branch.terminals) {
					terminalToRepo.delete(termId);
				}
			}
		}
		setState(
			produce((s) => {
				delete s.repositories[path];
				delete s.revisions[path];
				delete s.gitRevisions[path];
				s.repoOrder = s.repoOrder.filter((p) => p !== path);
				// Clean up group membership
				for (const group of Object.values(s.groups)) {
					group.repoOrder = group.repoOrder.filter((p) => p !== path);
				}
				if (s.activeRepoPath === path) {
					s.activeRepoPath = null;
				}
			}),
		);
	};

	/**
	 * Merge a `repositories.json` another client wrote into this one.
	 *
	 * A key is adopted only when this client has no unsaved intent for it — when
	 * the live value still equals the persisted baseline. Adopted keys move in the
	 * store *and* in the baseline together: those two are diffed against each other
	 * on every save, so refreshing one alone would make the next diff revert what
	 * the other client just wrote. Keys this client did change are left untouched;
	 * they go out on the next save and the compare-and-swap rebase resolves the
	 * collision, exactly as before.
	 *
	 * `activeRepoPath` is never adopted: it is which repo *this* window is looking
	 * at, and a background event must not move the user's focus.
	 */
	const adoptRemoteRepositories = (fresh: RepositorySnapshot): void => {
		const baseline = persistedSnapshot;
		if (!hydrated || !baseline) return;
		// The common event by far is this client's own echo, and the whole body below
		// is `JSON.stringify` on every repository. Leave before paying for it.
		if (jsonEqual(baseline, fresh)) return;
		const current = serializableSnapshot(
			state.repositories,
			state.repoOrder,
			state.activeRepoPath,
			state.groups,
			state.groupOrder,
		);
		const next = cloneJson(baseline);
		let adopted = false;

		for (const id of new Set([...Object.keys(baseline.repos), ...Object.keys(fresh.repos)])) {
			const mine = baseline.repos[id] ?? null;
			const incoming = fresh.repos[id] ?? null;
			if (jsonEqual(mine, incoming)) continue;
			// Unsaved local intent for this repo — leave it to the save path. Derived
			// fields are excluded: a diffstat that moved under us is not an edit.
			if (!jsonEqual(repositoryIntentView(mine), repositoryIntentView(current.repos[id] ?? null))) continue;
			if (incoming) {
				const normalized = cloneJson(incoming);
				normalizeLoadedRepo(normalized);
				const merged = withLiveBranchFields(normalized, state.repositories[id]);
				// Replaced, not merged: `setState(…, id, record)` walks the incoming keys
				// only, so a field the other client cleared (`connectionId`) would survive
				// on the live record forever. Adoption is rare enough to pay for the
				// coarser update.
				setState(
					produce((s) => {
						s.repositories[id] = merged;
					}),
				);
				// The baseline takes the record as this window now holds it, not as disk
				// holds it. Storing disk's copy would leave the live-only fields differing
				// from the baseline on the very next diff, and emit a mutation for them.
				next.repos[id] = serializableRepo(merged);
			} else {
				// A repo holding live terminals is local intent of its own: dropping it
				// would orphan panes the user is looking at. It stays in the store and in
				// the baseline, so no mutation is emitted for it — the other client's
				// removal stands on disk, and this window keeps working until its panes
				// close.
				if (hasLiveTerminals(state.repositories[id])) continue;
				delete next.repos[id];
				dropRepositoryFromState(id);
			}
			adopted = true;
		}

		for (const id of new Set([...Object.keys(baseline.groups), ...Object.keys(fresh.groups)])) {
			const mine = baseline.groups[id] ?? null;
			const incoming = fresh.groups[id] ?? null;
			if (jsonEqual(mine, incoming)) continue;
			if (!jsonEqual(mine, current.groups[id] ?? null)) continue;
			if (incoming) {
				next.groups[id] = cloneJson(incoming);
				setState("groups", id, cloneJson(incoming));
			} else {
				delete next.groups[id];
				next.groupOrder = next.groupOrder.filter((gid) => gid !== id);
				setState(
					produce((s) => {
						delete s.groups[id];
						s.groupOrder = s.groupOrder.filter((gid) => gid !== id);
					}),
				);
			}
			adopted = true;
		}

		if (!jsonEqual(baseline.repoOrder, fresh.repoOrder) && jsonEqual(baseline.repoOrder, current.repoOrder)) {
			next.repoOrder = [...fresh.repoOrder];
			setState("repoOrder", [...fresh.repoOrder]);
			adopted = true;
		}
		if (!jsonEqual(baseline.groupOrder, fresh.groupOrder) && jsonEqual(baseline.groupOrder, current.groupOrder)) {
			next.groupOrder = [...fresh.groupOrder];
			setState("groupOrder", [...fresh.groupOrder]);
			adopted = true;
		}

		// Every repo the store holds has to be reachable from `repoOrder` or from a
		// group, or the sidebar has no row to render it on. Adoption can break that
		// invariant two ways: the loop above refuses to drop a repo holding live
		// terminals while the order arrives from the client that did drop it, and a
		// group deleted elsewhere takes its members' only placement with it. Both end
		// with a repo that is running and invisible; `deleteGroup` resolves the second
		// case the same way, by putting the members back at the end.
		const placed = new Set([...next.repoOrder, ...Object.values(next.groups).flatMap((group) => group.repoOrder)]);
		const unplaced = Object.keys(next.repos).filter((id) => !placed.has(id));
		if (unplaced.length > 0) {
			next.repoOrder = [...next.repoOrder, ...unplaced];
			setState("repoOrder", [...next.repoOrder]);
			adopted = true;
		}

		if (!adopted) return;
		persistedSnapshot = next;
		syncHotRepos(state.repositories);
		invoke("github_update_paths", { paths: actions.getActivePaths() }).catch(() => {});
	};

	let remoteSyncStarted = false;
	let remoteSyncPending = false;

	/**
	 * Re-read disk and merge whatever another client wrote.
	 *
	 * Deferred while a save is in flight. That save ends by assigning
	 * `persistedSnapshot`, computed before it was sent, so an adoption landing inside
	 * that window has its baseline overwritten while its store changes stay — leaving
	 * the two out of step, which is the one thing this whole path exists to prevent.
	 * There is no ordering guarantee between the broadcast and the save's own reply,
	 * so waiting is the only sound answer; `onSaveSettled` runs us again.
	 */
	const syncFromDisk = (): void => {
		if (saveInFlight) {
			remoteSyncPending = true;
			return;
		}
		remoteSyncPending = false;
		loadPersistedSnapshot()
			.then((fresh) => {
				if (!fresh) return;
				// A save can have started while the read was in flight — same window,
				// checked again on the near side of it.
				if (saveInFlight) {
					remoteSyncPending = true;
					return;
				}
				adoptRemoteRepositories(fresh);
			})
			.catch((err) => appLogger.error("store", "Failed to re-read repositories after a remote change", err));
	};

	/** Subscribe once to the backend's `repositories-changed` broadcast. Started
	 *  from `hydrate`, so the first adoption always has a baseline to diff against.
	 *  This client's own saves echo back here too; they adopt nothing, because every
	 *  key they moved still differs from the baseline they were diffed from. */
	const startRemoteSync = (): void => {
		if (remoteSyncStarted) return;
		remoteSyncStarted = true;
		const stopSettings = settingsRefresh.register({
			section: "repositories_registration",
			read: loadPersistedSnapshot,
			apply: (fresh) => {
				if (!fresh) return;
				if (saveInFlight) {
					remoteSyncPending = true;
					return;
				}
				adoptRemoteRepositories(fresh);
			},
		});
		window.addEventListener("beforeunload", stopSettings, { once: true });
		onSaveSettled = () => {
			if (remoteSyncPending) syncFromDisk();
		};
		listen("repositories-changed", syncFromDisk).catch((err) => {
			remoteSyncStarted = false;
			onSaveSettled = null;
			appLogger.error("store", "Failed to register the repositories-changed listener", err);
		});
	};

	const actions = {
		/** Load repos from Rust backend; migrate from localStorage on first run */
		async hydrate(): Promise<void> {
			try {
				// One-time migration from localStorage
				const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
				if (legacy) {
					let parsed: Record<string, RepositoryState> | null = null;
					try {
						const decoded = JSON.parse(legacy) as unknown;
						if (!decoded || typeof decoded !== "object" || Array.isArray(decoded)) {
							throw new Error("legacy repository data must be an object");
						}
						parsed = decoded as Record<string, RepositoryState>;
					} catch {
						// Corrupt browser-local legacy data cannot be recovered.
						localStorage.removeItem(LEGACY_STORAGE_KEY);
					}
					if (parsed) {
						const imported = snapshotFromLoaded({
							repos: parsed,
							repoOrder: Object.keys(parsed),
						});
						try {
							await invoke("save_repositories", {
								config: repositoryMutationBatch(emptyRepositorySnapshot(), imported),
							});
							localStorage.removeItem(LEGACY_STORAGE_KEY);
						} catch (err) {
							// Keep the legacy copy for a later retry; a concurrent conflict must
							// never masquerade as corrupt input and delete the only copy.
							appLogger.error("store", "Failed to migrate legacy repositories", err);
						}
					}
				}

				const loaded = await invoke<{
					repos?: Record<string, RepositoryState>;
					repoOrder?: string[];
					activeRepoPath?: string | null;
					groups?: Record<string, RepoGroup>;
					groupOrder?: string[];
				}>("load_repositories");
				// Check before anything reads `loaded`: hydrating a delta yields an empty
				// repo set, and the first mutation would then persist that emptiness over
				// the user's only copy. `hydrated` stays false, so every save is blocked
				// until a human restores the file with the process stopped — which is the
				// step that failed last time, because the running app clobbered it again.
				if (isMutationDeltaDocument(loaded)) {
					appLogger.error("store", DELTA_DOCUMENT_MESSAGE);
					return;
				}

				persistedSnapshot = snapshotFromLoaded(loaded);
				const repos = loaded?.repos;
				if (repos) {
					// Migration: add collapsed/expanded fields, clear stale terminal IDs
					Object.values(repos).forEach(normalizeLoadedRepo);
					setState("repositories", repos);

					// Hydrate repoOrder: use saved order, falling back to Object.keys for repos not yet in the order
					const repoPaths = Object.keys(repos);
					const savedOrder = loaded.repoOrder ?? [];
					const validOrder = savedOrder.filter((p) => p in repos);
					const missing = repoPaths.filter((p) => !validOrder.includes(p));
					setState("repoOrder", [...validOrder, ...missing]);

					// Hydrate groups — migration: missing groups field initializes empty
					setState("groups", loaded.groups ?? {});
					setState("groupOrder", loaded.groupOrder ?? []);

					// Restore active repo
					if (loaded.activeRepoPath && loaded.activeRepoPath in repos) {
						setState("activeRepoPath", loaded.activeRepoPath);
					}
				}
				hydrated = true;
				syncHotRepos(state.repositories);
				startRemoteSync();
				// Awaited, not fire-and-forget: `hydrate()` is itself awaited before
				// the sidebar renders (`useAppInit.ts`), so a fire-and-forget call
				// here would let a stale-temp ghost row paint for one IPC round trip
				// before quarantine caught up to it — exactly the permanent-ghost-row
				// flash this story exists to remove, just delayed by a frame instead
				// of avoided.
				await this.refreshStaleTempCandidates();
			} catch (err) {
				appLogger.error("store", "Failed to hydrate repositories", err);
				// hydrated stays false — saves are blocked to prevent data loss
			}
		},

		/**
		 * Re-read the backend's stale-temp classification (#763-d219). The
		 * classifier itself lives entirely in Rust (`config.rs`) — this only
		 * fetches its verdict and quarantines the named paths from
		 * `getGroupedLayout`/`getOrderedRepos`, never computes it here.
		 */
		async refreshStaleTempCandidates(): Promise<void> {
			try {
				const candidates = await invoke<StaleTempCandidate[]>("list_stale_temp_repository_candidates");
				// A malformed/unmocked response (undefined, not an array) must never
				// corrupt this field to a non-array — every read of it downstream
				// (`getGroupedLayout`, `getOrderedRepos`) assumes an array.
				setState("staleTempCandidates", Array.isArray(candidates) ? candidates : []);
			} catch (err) {
				appLogger.error("store", "Failed to list stale-temp repository candidates", err);
			}
		},

		/** Current stale-temp candidates, quarantined from the sidebar pending repair. */
		getStaleTempCandidates(): StaleTempCandidate[] {
			return state.staleTempCandidates;
		},

		/**
		 * User-explicit repair (#763-d219): remove exactly `paths` from
		 * `repositories.json`. The backend re-validates every path against the
		 * live on-disk document and refuses the whole request if any no longer
		 * matches — this call does not, and must not, decide that on its own.
		 *
		 * Does not touch `state.repositories` directly: a successful repair
		 * fires the same `repositories-changed` broadcast a remote client's
		 * edit would, and the existing `adoptRemoteRepositories` path removes
		 * the rows from every window (including this one) exactly as it
		 * would for any other client's write — one removal path, not two.
		 * Only the quarantine list updates immediately, for instant feedback
		 * in the confirming window without waiting on that round trip.
		 */
		async repairStaleTemp(paths: string[]): Promise<StaleTempRepairSummary> {
			const summary = await invoke<StaleTempRepairSummary>("repair_stale_temp_repositories", { paths });
			setState("staleTempCandidates", (list) => list.filter((c) => !summary.removed.includes(c.path)));
			return summary;
		},

		/** Add a repository */
		add(repo: {
			path: string;
			displayName: string;
			initials?: string;
			isGitRepo?: boolean;
			connectionId?: string;
		}): void {
			setState("repositories", repo.path, {
				path: repo.path,
				displayName: repo.displayName,
				initials: repo.initials ?? "",
				isGitRepo: repo.isGitRepo ?? true,
				expanded: true,
				collapsed: false,
				parked: false,
				workspaces: {},
				activeWorkspaceId: null,
				connectionId: repo.connectionId,
			});
			if (!state.repoOrder.includes(repo.path)) {
				setState("repoOrder", [...state.repoOrder, repo.path]);
			}
			save();
			invoke("github_update_paths", { paths: this.getActivePaths() }).catch(() => {});
		},

		/** Remove a repository */
		remove(path: string): void {
			dropRepositoryFromState(path);
			save();
			invoke("github_update_paths", { paths: this.getActivePaths() }).catch(() => {});
		},

		/** Set active repository */
		setActive(path: string | null): void {
			// Freeze-investigation breadcrumb: the synchronous reactive cascade off
			// activeRepoPath/revision is the prime repo-switch-hang suspect.
			markPerf("repo.setActive", { path });
			setState("activeRepoPath", path);
			save();
			if (path && !getHotRepoPaths(state.repositories).includes(path)) {
				setState("revisions", path, (n) => (n ?? 0) + 1);
				invoke("github_poll_repo", { path }).catch(() => {});
			}
			// The "and switch" half of the `active_and_switch` index strategy — the
			// boot half lives in Rust (lib.rs pre-warm). Fire-and-forget, and NOT
			// conditional on the hot-repo check above: whether the warm runs is a
			// config policy owned by `content_index::warm_index`, which no-ops for
			// `disabled`/`active_only` and dedupes an already-built repo.
			if (path) {
				invoke("warm_content_index", { repoPath: path }).catch(() => {});
			}
		},

		/** Update the display name of a repository */
		setDisplayName(path: string, displayName: string): void {
			if (!state.repositories[path]) return;
			setState("repositories", path, "displayName", displayName);
			save();
		},

		/** Toggle repository expanded state */
		toggleExpanded(path: string): void {
			setState("repositories", path, "expanded", (e) => !e);
			save();
		},

		/** Toggle repository collapsed state */
		toggleCollapsed(path: string): void {
			setState("repositories", path, "collapsed", (c) => !c);
			save();
		},

		/** Collapse or re-expand the agents under a workspace row (expanded by default) */
		toggleWorkspaceTabsCollapsed(repoPath: string, workspaceId: string): void {
			if (!state.repositories[repoPath]?.workspaces[workspaceId]) return;
			setState("repositories", repoPath, "workspaces", workspaceId, "tabsCollapsed", (c) => !c);
			save();
		},

		setWorktreeDiagnostics(repoPath: string, entries: WorktreeDiagnostic[]): void {
			if (state.repositories[repoPath]) setState("repositories", repoPath, "worktreeDiagnostics", entries);
		},

		setDetachedWorktrees(repoPath: string, entries: DetachedWorktree[]): void {
			if (state.repositories[repoPath]) setState("repositories", repoPath, "detachedWorktrees", entries);
		},

		/** Update git repo status (used when a directory gains or loses .git) */
		setIsGitRepo(path: string, isGitRepo: boolean): void {
			setState("repositories", path, "isGitRepo", isGitRepo);
			save();
		},

		/**
		 * Add or update one workspace, addressed by its id.
		 *
		 * The second argument is the map KEY, never a branch to look up. For
		 * everything that exists today the two are the same string — the identity
		 * migration minted `workspaceId = branchName` so nothing persisted moved —
		 * and that is exactly why the parameter is named for the key: callers
		 * should not couple workspace identity to a display branch name.
		 *
		 * `branchName` in `data` is what is checked out. Absent, it defaults to the
		 * id, which is right for every row created from a branch.
		 */
		setWorkspace(repoPath: string, workspaceId: string, data?: Partial<WorkspaceState>): void {
			const existing = state.repositories[repoPath]?.workspaces[workspaceId];
			const patch = data ? definedFields(data) : {};
			if (existing) {
				setState("repositories", repoPath, "workspaces", workspaceId, (prev) => ({
					...prev,
					...patch,
				}));
			} else {
				const branchName = patch.branchName ?? workspaceId;
				setState("repositories", repoPath, "workspaces", workspaceId, {
					workspaceId,
					branchName,
					kind: isMainBranch(branchName) ? "main" : "worktree",
					parentRepoPath: null,
					isMain: isMainBranch(branchName),
					worktreePath: null,
					terminals: [],
					hadTerminals: false,
					lastActiveTerminal: null,
					additions: 0,
					deletions: 0,
					isMerged: false,
					lastCommitTs: null,
					...patch,
				});
			}
			save();
		},

		/**
		 * Point a repo at the workspace on screen.
		 *
		 * The id must name a row that exists. A dangling pointer does not fail where
		 * it is written — it fails twice, later, in places that name neither this
		 * call nor the repo: the sidebar renders no tab row for the active workspace
		 * (terminals stay alive with nothing to click), and `handleAddTerminalToWorkspace`
		 * reads `workspaces[id]?.worktreePath` off `undefined` and spawns the next
		 * terminal in the user's HOME directory.
		 *
		 * 22 call sites set this, and several pass a branch or a workspace id straight
		 * off a backend payload — the shape those payloads are keyed on changed under
		 * them once already. So the check belongs here, once, and keeps the previous
		 * pointer: it names a row that does exist, which is strictly better than a
		 * pointer to nothing. `migrateActiveWorkspaceId` drops the same dangling id at
		 * load time for exactly this reason; this is the live half of that rule.
		 */
		setActiveWorkspace(repoPath: string, workspaceId: string | null): void {
			const repo = state.repositories[repoPath];
			if (workspaceId !== null && repo && !repo.workspaces[workspaceId]) {
				appLogger.error("store", `setActiveWorkspace: "${workspaceId}" names no workspace — pointer left alone`, {
					repoPath,
					requested: workspaceId,
					current: repo.activeWorkspaceId,
					known: Object.keys(repo.workspaces),
				});
				return;
			}
			setState("repositories", repoPath, "activeWorkspaceId", workspaceId);
		},

		/** Adopt a caller-bound backend declaration before its repository refresh arrives. */
		setDeclaredWorktree(
			repoPath: string,
			peer: string,
			placement: { workspaceId: string; branch: string; worktreePath: string },
		): void {
			const repo = state.repositories[repoPath];
			if (!repo) return;
			setState("repositories", repoPath, "declaredWorktrees", { ...repo.declaredWorktrees, [peer]: placement });
			save();
		},

		/** Add terminal to branch */
		addTerminalToWorkspace(repoPath: string, workspaceId: string, terminalId: string): void {
			const branch = state.repositories[repoPath]?.workspaces[workspaceId];
			if (branch && !branch.terminals.includes(terminalId)) {
				appLogger.info("terminal", `addTerminalToWorkspace ${workspaceId} += ${terminalId}`, {
					before: [...branch.terminals],
				});
				terminalToRepo.set(terminalId, repoPath);
				batch(() => {
					setState("repositories", repoPath, "workspaces", workspaceId, "terminals", (t) => [...t, terminalId]);
					if (!branch.hadTerminals) {
						setState("repositories", repoPath, "workspaces", workspaceId, "hadTerminals", true);
					}
				});
				save();
				syncHotRepos(state.repositories);
			}
		},

		/** Remove terminal from branch */
		removeTerminalFromWorkspace(repoPath: string, workspaceId: string, terminalId: string): void {
			const branch = state.repositories[repoPath]?.workspaces[workspaceId];
			appLogger.info("terminal", `removeTerminalFromWorkspace ${workspaceId} -= ${terminalId}`, {
				before: branch?.terminals ? [...branch.terminals] : [],
			});
			terminalToRepo.delete(terminalId);
			batch(() => {
				setState("repositories", repoPath, "workspaces", workspaceId, "terminals", (t) =>
					t.filter((id) => id !== terminalId),
				);
				// When last terminal is removed, clear stale savedTerminals so the periodic
				// snapshot doesn't resurrect closed tabs on next branch click.
				const updated = state.repositories[repoPath]?.workspaces[workspaceId];
				if (updated && updated.terminals.length === 0 && updated.savedTerminals && updated.savedTerminals.length > 0) {
					setState("repositories", repoPath, "workspaces", workspaceId, "savedTerminals", []);
				}
			});
			save();
			syncHotRepos(state.repositories);
		},

		/** Set run command for a branch */
		setRunCommand(repoPath: string, workspaceId: string, command: string | undefined): void {
			const branch = state.repositories[repoPath]?.workspaces[workspaceId];
			if (branch) {
				setState("repositories", repoPath, "workspaces", workspaceId, "runCommand", command);
				save();
			}
		},

		/** Update CI auto-heal state for a branch */
		setCiAutoHeal(repoPath: string, workspaceId: string, value: WorkspaceState["ciAutoHeal"]): void {
			if (!state.repositories[repoPath]?.workspaces[workspaceId]) return;
			setState("repositories", repoPath, "workspaces", workspaceId, "ciAutoHeal", value);
			save();
		},

		/** Update branch stats (additions/deletions) — only if branch already exists */
		updateWorkspaceStats(repoPath: string, workspaceId: string, additions: number, deletions: number): void {
			if (!state.repositories[repoPath]?.workspaces[workspaceId]) return;
			setState("repositories", repoPath, "workspaces", workspaceId, { additions, deletions });
		},

		/** Remove one workspace, addressed by its id. */
		removeWorkspace(repoPath: string, workspaceId: string): void {
			const repo = state.repositories[repoPath];
			if (!repo) return;

			const branch = repo.workspaces[workspaceId];
			if (branch) {
				appLogger.debug("terminal", `removeWorkspace "${workspaceId}" from ${repoPath}`, {
					terminals: branch.terminals,
					hadTerminals: branch.hadTerminals,
					savedTerminals: branch.savedTerminals?.length ?? 0,
				});
			}

			// Clean up inverse index for all terminals in the removed branch
			if (branch) {
				for (const tid of branch.terminals) {
					terminalToRepo.delete(tid);
				}
			}

			setState(
				produce((s) => {
					const r = s.repositories[repoPath];
					if (!r) return;

					// A removed checkout must not remain a saved peer placement.
					for (const [peer, placement] of Object.entries(r.declaredWorktrees ?? {})) {
						if (placement.workspaceId === workspaceId) delete r.declaredWorktrees![peer];
					}
					// Delete the branch
					delete r.workspaces[workspaceId];

					// Clear active branch if it was removed
					if (r.activeWorkspaceId === workspaceId) {
						const remainingBranches = Object.keys(r.workspaces);
						r.activeWorkspaceId = remainingBranches[0] || null;
					}
				}),
			);
			save();
		},

		/**
		 * A branch was renamed: follow it.
		 *
		 * Both arguments are branch names, and for a branch-derived id they are
		 * also the old and new map key — which is why this moves the record. The
		 * `workspaceId` field has to move WITH the key: the record used to be
		 * spread through unchanged, so after a rename the map key was the new name
		 * while `workspaceId` still held the old one, and every consumer that
		 * reads `ws.workspaceId` then addressed a key that no longer existed.
		 *
		 * Linked worktrees cannot share a branch, so the branch-derived id and map
		 * key move together.
		 */
		renameBranch(repoPath: string, oldName: string, newName: string): void {
			const repo = state.repositories[repoPath];
			if (!repo?.workspaces[oldName]) return;

			setState(
				produce((s) => {
					const r = s.repositories[repoPath];
					if (!r) return;

					// Get the old branch data
					const oldBranch = r.workspaces[oldName];
					if (!oldBranch) return;

					// Create new branch entry with updated name
					r.workspaces[newName] = {
						...oldBranch,
						workspaceId: newName,
						branchName: newName,
						isMain: isMainBranch(newName),
					};

					// Delete the old branch entry
					delete r.workspaces[oldName];

					// Update active branch if it was renamed
					if (r.activeWorkspaceId === oldName) {
						r.activeWorkspaceId = newName;
					}
				}),
			);
			save();
		},

		/** Merge terminal state from one workspace into another, both addressed by id
		 *  (for the main-checkout rename race).
		 *  Moves terminals, savedTerminals, hadTerminals, lastActiveTerminal from source
		 *  to target, keeping the target's worktreePath and other git-derived fields. */
		mergeWorkspaceState(repoPath: string, sourceId: string, targetId: string): void {
			const repo = state.repositories[repoPath];
			if (!repo?.workspaces[sourceId] || !repo.workspaces[targetId]) return;

			setState(
				produce((s) => {
					const r = s.repositories[repoPath];
					if (!r) return;
					const src = r.workspaces[sourceId];
					const tgt = r.workspaces[targetId];
					if (!src || !tgt) return;

					// Transfer terminals
					for (const termId of src.terminals) {
						if (!tgt.terminals.includes(termId)) {
							tgt.terminals.push(termId);
						}
					}
					src.terminals = [];

					// Transfer savedTerminals (only if target has none)
					if (
						src.savedTerminals &&
						src.savedTerminals.length > 0 &&
						(!tgt.savedTerminals || tgt.savedTerminals.length === 0)
					) {
						tgt.savedTerminals = src.savedTerminals;
						src.savedTerminals = [];
					}

					// Carry over flags
					if (src.hadTerminals) tgt.hadTerminals = true;
					if (src.lastActiveTerminal && !tgt.lastActiveTerminal) {
						tgt.lastActiveTerminal = src.lastActiveTerminal;
					}
				}),
			);
			save();
		},

		/** Get the connectionId for a repo (undefined = local) */
		getConnectionId(path: string): string | undefined {
			return state.repositories[path]?.connectionId;
		},

		/** Get repository by path */
		get(path: string): RepositoryState | undefined {
			return state.repositories[path];
		},

		/** One workspace by its id. The only supported way to reach a record: a
		 *  branch cannot name a row once two workspaces share one. */
		getWorkspace(repoPath: string, workspaceId: string): WorkspaceState | undefined {
			return state.repositories[repoPath]?.workspaces[workspaceId];
		},

		/**
		 * The branch a workspace has checked out.
		 *
		 * The legitimate direction: id -> branch is a lookup, branch -> id is a
		 * guess. Callers that hold an id and need a git ref (a merge subject, a PR
		 * lookup, a label) come through here rather than reusing the id as a name.
		 * Falls back to the id, which is what it equals for everything a linked
		 * worktree ever created.
		 */
		branchNameFor(repoPath: string, workspaceId: string): string {
			return state.repositories[repoPath]?.workspaces[workspaceId]?.branchName ?? workspaceId;
		},

		/**
		 * The workspace on `branchName` — the ONE place allowed to go from a branch
		 * to an id, and only for callers whose input genuinely is a branch and
		 * nothing else: a GitHub PR names its head branch, never a directory.
		 *
		 * DEFERRED (2026-09-11) — with two workspaces on one branch this returns the
		 * first and there is no better answer available at these call sites: a PR
		 * merge knows which ref landed, not which of two checkouts the user meant.
		 * Resolving it needs the PR-cleanup flows to carry a workspace from the row
		 * the user clicked, which is a UI change, not a lookup change. Until then
		 * this is a documented single seam instead of the same guess inlined at four
		 * call sites.
		 */
		workspaceIdOnBranch(repoPath: string, branchName: string): string | null {
			const workspaces = state.repositories[repoPath]?.workspaces;
			if (!workspaces) return null;
			for (const [workspaceId, workspace] of Object.entries(workspaces)) {
				if (workspace.branchName === branchName) return workspaceId;
			}
			return null;
		},

		/** True unless the repo is a registered plain directory. Unknown paths
		 *  (worktrees keyed under their parent) default to true. Callers that
		 *  invoke git commands MUST check this — a plain directory makes every
		 *  git call fail with "not a git repository". */
		isGitRepo(path: string): boolean {
			return state.repositories[path]?.isGitRepo !== false;
		},

		/** Get active repository */
		getActive(): RepositoryState | undefined {
			return state.activeRepoPath ? state.repositories[state.activeRepoPath] : undefined;
		},

		/** Get all repository paths (includes parked — use for persistence,
		 *  path-resolution, and snapshot operations). */
		getPaths(): string[] {
			return Object.keys(state.repositories);
		},

		/** Get repository paths that should receive active scan/poll/refresh
		 *  work (excludes parked repos). Use this for any "fan out across all
		 *  repos" loop where parked repos should stay dormant — git stats
		 *  refresh, GitHub PR/issue polling, plugin SDK enumeration, etc.
		 *  (#1358-caf5) */
		getActivePaths(): string[] {
			return Object.keys(state.repositories).filter((p) => !state.repositories[p]?.parked);
		},

		/** Reorder repositories in the sidebar */
		reorderRepo(fromIndex: number, toIndex: number): void {
			setState("repoOrder", (order) => {
				const result = [...order];
				const [moved] = result.splice(fromIndex, 1);
				result.splice(toIndex, 0, moved);
				return result;
			});
			save();
		},

		/** Park or unpark a repository (hide from sidebar, recallable via popover).
		 *  Tears down the Rust file watcher when parked and re-creates it when
		 *  unparked so parked repos stay fully dormant. (#1358-caf5) */
		setPark(path: string, parked: boolean): void {
			if (!state.repositories[path]) return;
			setState("repositories", path, "parked", parked);
			save();
			invoke("github_update_paths", { paths: this.getActivePaths() }).catch(() => {});
			const cmd = parked ? "stop_repo_watcher" : "start_repo_watcher";
			invoke(cmd, { repoPath: path }).catch((err) => {
				appLogger.warn("store", `${cmd} failed for ${path}`, err);
			});
		},

		/** Park or unpark all repositories in a group at once. */
		setParkGroup(groupId: string, parked: boolean): void {
			const group = state.groups[groupId];
			if (!group) return;
			for (const path of group.repoOrder) {
				this.setPark(path, parked);
			}
		},

		/** Check if all repos in a group are parked */
		isGroupFullyParked(groupId: string): boolean {
			const group = state.groups[groupId];
			if (!group || group.repoOrder.length === 0) return false;
			return group.repoOrder.every((path) => state.repositories[path]?.parked);
		},

		/** Get all parked repositories */
		getParkedRepos(): RepositoryState[] {
			return Object.values(state.repositories).filter((r) => r.parked);
		},

		/** Get ordered repo paths (excludes parked repos and quarantined stale-temp ghosts) */
		getOrderedRepos(): RepositoryState[] {
			const quarantined = quarantinedPaths(state.staleTempCandidates);
			return state.repoOrder
				.map((path) => state.repositories[path])
				.filter((r) => r && !r.parked && !quarantined.has(r.path));
		},

		/** Reorder terminals within the active branch */
		reorderTerminals(repoPath: string, workspaceId: string, fromIndex: number, toIndex: number): void {
			setState("repositories", repoPath, "workspaces", workspaceId, "terminals", (terminals) => {
				const result = [...terminals];
				const [moved] = result.splice(fromIndex, 1);
				result.splice(toIndex, 0, moved);
				return result;
			});
			save();
		},

		/** Reverse-lookup: find which repo a terminal belongs to (O(1) via inverse index). */
		getRepoPathForTerminal(termId: string): string | null {
			return terminalToRepo.get(termId) ?? null;
		},

		/** Reverse-lookup: which repo and WORKSPACE own a terminal.
		 *  O(1) repo lookup via the inverse index, then a scan of that repo's
		 *  workspaces (typically 1-5). Returns the map key, so a caller can feed it
		 *  straight back into any id-taking method — the previous name for this
		 *  field said `branchName` while already holding the key, which is how a
		 *  branch reached call sites that needed an id. */
		findOwnerForTerminal(termId: string): { repoPath: string; workspaceId: string } | null {
			const repoPath = terminalToRepo.get(termId);
			if (!repoPath) return null;
			const repo = state.repositories[repoPath];
			if (!repo) return null;
			for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
				if (workspace.terminals.includes(termId)) return { repoPath, workspaceId };
			}
			return null;
		},

		/** Reverse-lookup: returns repo displayName for a terminal (for overlay labels). */
		getRepoForTerminal(termId: string): string | null {
			const path = terminalToRepo.get(termId);
			if (!path) return null;
			return state.repositories[path]?.displayName ?? null;
		},

		/** Get terminals for current active branch */
		getActiveTerminals(): string[] {
			const repo = actions.getActive();
			if (!repo?.activeWorkspaceId) return [];
			return repo.workspaces[repo.activeWorkspaceId]?.terminals || [];
		},

		/** Snapshot terminal metadata into each branch for persistence (called at quit time) */
		snapshotTerminals(snapshots: Map<string, Map<string, SavedTerminal[]>>): void {
			setState(
				produce((s) => {
					for (const [repoPath, workspaces] of snapshots) {
						const repo = s.repositories[repoPath];
						if (!repo) continue;
						for (const [branchName, terminals] of workspaces) {
							const branch = repo.workspaces[branchName];
							if (!branch) continue;
							branch.savedTerminals = terminals;
						}
					}
				}),
			);
			// Flush immediately (not debounced) — app is about to exit
			saveNow();
		},

		/** Wait until the latest queued snapshot reaches the backend. */
		async flushSave(): Promise<void> {
			while (saveInFlight || queuedSnapshot) {
				drainRepositorySaveQueue();
				await saveCompletion;
			}
			if (saveError !== undefined) throw saveError;
		},

		/** Clear savedTerminals from all workspaces (consume-once after restore) */
		clearSavedTerminals(): void {
			setState(
				produce((s) => {
					for (const repo of Object.values(s.repositories)) {
						for (const branch of Object.values(repo.workspaces)) {
							branch.savedTerminals = [];
						}
					}
				}),
			);
			save();
		},

		/** Bump the revision counter for a repo (signals panels to re-fetch) */
		bumpRevision(repoPath: string): void {
			setState("revisions", repoPath, (n) => (n ?? 0) + 1);
		},

		/** Get the current revision counter for a repo (reactive — tracks in effects) */
		getRevision(repoPath: string): number {
			return state.revisions[repoPath] ?? 0;
		},

		/**
		 * Bump both counters for a git-state change. Never bump `gitRevisions`
		 * alone: a commit changes what every panel shows, not just the ones
		 * reading committed history.
		 */
		bumpGitRevision(repoPath: string): void {
			setState(
				produce((s) => {
					s.revisions[repoPath] = (s.revisions[repoPath] ?? 0) + 1;
					s.gitRevisions[repoPath] = (s.gitRevisions[repoPath] ?? 0) + 1;
				}),
			);
		},

		/** Get the current git-state revision counter for a repo (reactive) */
		getGitRevision(repoPath: string): number {
			return state.gitRevisions[repoPath] ?? 0;
		},

		/** Check if empty */
		isEmpty(): boolean {
			return Object.keys(state.repositories).length === 0;
		},

		// ── Group CRUD ──

		/** Create a new group. Returns ID or null if name is duplicate (case-insensitive). */
		createGroup(name: string): string | null {
			const nameLower = name.toLowerCase();
			const exists = Object.values(state.groups).some((g) => g.name.toLowerCase() === nameLower);
			if (exists) return null;

			const id = generateGroupId();
			batch(() => {
				setState("groups", id, { id, name, color: "", collapsed: false, repoOrder: [] });
				setState("groupOrder", [...state.groupOrder, id]);
			});
			save();
			return id;
		},

		/** Delete a group — repos move to ungrouped */
		deleteGroup(id: string): void {
			const group = state.groups[id];
			if (!group) return;
			setState(
				produce((s) => {
					// Move repos to ungrouped order
					const repos = s.groups[id]?.repoOrder ?? [];
					s.repoOrder = [...s.repoOrder, ...repos];
					delete s.groups[id];
					s.groupOrder = s.groupOrder.filter((gid) => gid !== id);
				}),
			);
			save();
		},

		/** Rename a group. Returns false if name is duplicate (case-insensitive). */
		renameGroup(id: string, newName: string): boolean {
			const nameLower = newName.toLowerCase();
			const exists = Object.values(state.groups).some((g) => g.id !== id && g.name.toLowerCase() === nameLower);
			if (exists) return false;
			setState("groups", id, "name", newName);
			save();
			return true;
		},

		/** Set group color */
		setGroupColor(id: string, color: string): void {
			if (!state.groups[id]) return;
			setState("groups", id, "color", color);
			save();
		},

		/** Toggle group collapsed/expanded */
		toggleGroupCollapsed(id: string): void {
			if (!state.groups[id]) return;
			setState("groups", id, "collapsed", (c) => !c);
			save();
		},

		// ── Group assignment ──

		/** Add repo to a group (removes from ungrouped or previous group) */
		addRepoToGroup(repoPath: string, groupId: string): void {
			if (!state.groups[groupId]) return;
			setState(
				produce((s) => {
					// Remove from ungrouped
					s.repoOrder = s.repoOrder.filter((p) => p !== repoPath);
					// Remove from any other group
					for (const group of Object.values(s.groups)) {
						group.repoOrder = group.repoOrder.filter((p) => p !== repoPath);
					}
					// Add to target group
					s.groups[groupId].repoOrder = [...s.groups[groupId].repoOrder, repoPath];
				}),
			);
			save();
		},

		/** Remove repo from its group back to ungrouped */
		removeRepoFromGroup(repoPath: string): void {
			setState(
				produce((s) => {
					for (const group of Object.values(s.groups)) {
						group.repoOrder = group.repoOrder.filter((p) => p !== repoPath);
					}
					if (!s.repoOrder.includes(repoPath)) {
						s.repoOrder = [...s.repoOrder, repoPath];
					}
				}),
			);
			save();
		},

		/** Find which group a repo belongs to (or undefined if ungrouped) */
		getGroupForRepo(repoPath: string): RepoGroup | undefined {
			// Direct lookup via group iteration with early return — O(groups) worst case
			// instead of O(groups * repos_per_group) with Array.includes
			for (const group of Object.values(state.groups)) {
				if (group.repoOrder.indexOf(repoPath) !== -1) return group;
			}
			return undefined;
		},

		// ── Group reordering ──

		/** Reorder a repo within its group */
		reorderRepoInGroup(groupId: string, fromIndex: number, toIndex: number): void {
			if (!state.groups[groupId]) return;
			setState("groups", groupId, "repoOrder", (order) => {
				const result = [...order];
				const [moved] = result.splice(fromIndex, 1);
				result.splice(toIndex, 0, moved);
				return result;
			});
			save();
		},

		/** Move repo from one group to another at a specific index */
		moveRepoBetweenGroups(repoPath: string, fromGroupId: string, toGroupId: string, toIndex: number): void {
			if (!state.groups[fromGroupId] || !state.groups[toGroupId]) return;
			setState(
				produce((s) => {
					s.groups[fromGroupId].repoOrder = s.groups[fromGroupId].repoOrder.filter((p) => p !== repoPath);
					const target = [...s.groups[toGroupId].repoOrder];
					target.splice(toIndex, 0, repoPath);
					s.groups[toGroupId].repoOrder = target;
				}),
			);
			save();
		},

		/** Reorder groups in the display order */
		reorderGroups(fromIndex: number, toIndex: number): void {
			setState("groupOrder", (order) => {
				const result = [...order];
				const [moved] = result.splice(fromIndex, 1);
				result.splice(toIndex, 0, moved);
				return result;
			});
			save();
		},

		/** Get the grouped layout for rendering: ordered groups with their repos, plus ungrouped repos.
		 *
		 *  Every consumer renders this through a reference-keyed `<For>`, so a
		 *  fresh wrapper object means a torn-down and rebuilt DOM subtree. This
		 *  ran on every repo-store change — a branch poll, a file save, an
		 *  unrelated repo's revision bump — and rebuilt every group header and
		 *  every repo row each time. The caches below hand back the previous
		 *  wrapper whenever it still describes the same group and the same repo
		 *  proxies, so an untouched group survives a change to its neighbour. */
		getGroupedLayout(): GroupedLayout {
			const quarantined = quarantinedPaths(state.staleTempCandidates);
			const groups = state.groupOrder
				.map((gid) => state.groups[gid])
				.filter(Boolean)
				.map((group) => {
					const repos = group.repoOrder
						.map((path) => state.repositories[path])
						.filter((r) => r && !r.parked && !quarantined.has(r.path));
					const cached = groupLayoutCache.get(group.id);
					if (cached && cached.group === group && sameRefs(cached.repos, repos)) return cached;
					const entry = { group, repos };
					groupLayoutCache.set(group.id, entry);
					return entry;
				});
			// Drop cache entries for groups that no longer exist.
			if (groupLayoutCache.size > groups.length) {
				const live = new Set(groups.map((g) => g.group.id));
				for (const id of groupLayoutCache.keys()) if (!live.has(id)) groupLayoutCache.delete(id);
			}

			// Collect all repo paths that belong to a group
			const groupedPaths = new Set(groups.flatMap((g) => g.group.repoOrder));

			const ungrouped = state.repoOrder
				.filter((path) => !groupedPaths.has(path))
				.map((path) => state.repositories[path])
				.filter((r) => r && !r.parked && !quarantined.has(r.path));

			if (!lastLayout || !sameRefs(lastLayout.groups, groups) || !sameRefs(lastLayout.ungrouped, ungrouped)) {
				lastLayout = {
					groups: lastLayout && sameRefs(lastLayout.groups, groups) ? lastLayout.groups : groups,
					ungrouped: lastLayout && sameRefs(lastLayout.ungrouped, ungrouped) ? lastLayout.ungrouped : ungrouped,
				};
			}
			return lastLayout;
		},

		/** Every configured repo in display order (ungrouped first, then each
		 *  group's repos in group order). Includes parked repos. Used by the
		 *  Settings nav, which must list every repo regardless of group/park
		 *  state — grouped repos live in group.repoOrder, not state.repoOrder. (#64) */
		getAllReposOrdered(): RepositoryState[] {
			const seen = new Set<string>();
			const result: RepositoryState[] = [];
			const push = (path: string) => {
				if (seen.has(path)) return;
				const repo = state.repositories[path];
				if (!repo) return;
				seen.add(path);
				result.push(repo);
			};
			for (const path of state.repoOrder) push(path);
			for (const gid of state.groupOrder) {
				const group = state.groups[gid];
				if (!group) continue;
				for (const path of group.repoOrder) push(path);
			}
			return result;
		},

		setBranchSwitching(value: boolean): void {
			setState("branchSwitching", value);
		},
	};

	return {
		state,
		...actions,
		/** Test-only: set hydrated flag to enable saves in tests that skip hydrate */
		_testSetHydrated(value: boolean): void {
			hydrated = value;
			if (value && !persistedSnapshot) {
				persistedSnapshot = serializableSnapshot(
					state.repositories,
					state.repoOrder,
					state.activeRepoPath,
					state.groups,
					state.groupOrder,
				);
			}
		},
		_testCancelPendingSave(): void {
			if (saveTimer) {
				clearTimeout(saveTimer);
				saveTimer = null;
			}
			queuedSnapshot = null;
		},
	};
}

export const repositoriesStore = createRepositoriesStore();

// Debug registry — expose repo topology for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("repositories", () => {
	const s = repositoriesStore.state;
	return {
		activeRepoPath: s.activeRepoPath,
		repoOrder: s.repoOrder,
		groupOrder: s.groupOrder,
		repos: Object.fromEntries(
			Object.entries(s.repositories).map(([path, r]) => [
				path,
				{
					displayName: r.displayName,
					activeWorkspaceId: r.activeWorkspaceId,
					expanded: r.expanded,
					collapsed: r.collapsed,
					parked: r.parked,
					isGitRepo: r.isGitRepo,
					workspaces: Object.fromEntries(
						Object.entries(r.workspaces).map(([name, b]) => [
							name,
							{
								isMain: b.isMain,
								terminals: b.terminals.length,
								additions: b.additions,
								deletions: b.deletions,
								isMerged: b.isMerged,
							},
						]),
					),
				},
			]),
		),
	};
});

/** Get the branch key for the currently active repo+branch.
 *
 *  Use this ONLY for something the user is doing to the repo in front of them.
 *  A tab must be scoped with `branchKeyFor(itsOwnRepo)` instead: scoping it to the
 *  focused repo is how a preview from one repo ends up filed under another. */
export function currentBranchKey(): string | undefined {
	return branchKeyFor(repositoriesStore.state.activeRepoPath);
}

/** Branch key for a tab that belongs to `repoPath` — the tab's own repo, never the
 *  focused one. `undefined` (unscoped, visible everywhere) when the repo is unknown
 *  or has no active branch, which is the honest answer for a tab we cannot place. */
export function branchKeyFor(repoPath: string | null | undefined): string | undefined {
	if (!repoPath) return undefined;
	const repo = repositoriesStore.state.repositories[repoPath];
	if (!repo?.activeWorkspaceId) return undefined;
	return makeBranchKey(repoPath, repo.activeWorkspaceId);
}

/** Resolve which registered repo owns `path`. Returns null when none does — callers
 *  must handle that visibly rather than falling back to whatever repo has focus. */
export function resolveRepoOwner(path: string | null | undefined): RepoOwner | null {
	return resolveRepoOwnerIn(path, repositoriesStore.state.repositories);
}

/** The repo that owns `path`, or null. Convenience for callers that do not care
 *  which worktree branch matched. */
export function resolveRepoPathFor(path: string | null | undefined): string | null {
	return resolveRepoOwner(path)?.repoPath ?? null;
}

/** Where a file on disk belongs: its repo, the filesystem root to do I/O against,
 *  and the path relative to that root. Empty repo/root means no registered repo
 *  owns it, and `filePath` stays absolute. */
export interface FileLocation {
	repoPath: string;
	fsRoot: string;
	filePath: string;
}

/**
 * Place an absolute path for tab-opening.
 *
 * Every caller that opens a file used to relativize it against the ACTIVE repo's
 * worktree: a file from another repo then failed the prefix test and opened as an
 * absolute, unscoped tab — visible under every repo. Ask the path who owns it
 * instead, and use that owner's worktree as the root.
 */
export function locateFile(absolutePath: string): FileLocation {
	const owner = resolveRepoOwner(absolutePath);
	if (!owner) return { repoPath: "", fsRoot: "", filePath: absolutePath };

	// A linked worktree is the filesystem root for I/O; the repo root is not.
	const worktreePath = owner.workspaceId
		? repositoriesStore.state.repositories[owner.repoPath]?.workspaces[owner.workspaceId]?.worktreePath
		: null;
	const fsRoot = worktreePath || owner.repoPath;
	const filePath = pathStartsWith(absolutePath, fsRoot)
		? (pathStripPrefix(absolutePath, fsRoot) ?? absolutePath)
		: absolutePath;
	return { repoPath: owner.repoPath, fsRoot, filePath };
}

/**
 * The workspace a terminal owned by `owner` should be filed under.
 *
 * A linked worktree directory names its own workspace and is used as-is. A match
 * at the repo ROOT names none — what is checked out there moves under the user's
 * feet — so it resolves late, here:
 *
 *  1. whichever workspace records the repo root as its worktree;
 *  2. failing that, no workspace until the root checkout is discovered.
 *
 * The root workspace is an exact path match. Falling back to `activeWorkspaceId`
 * files every root session under whichever linked worktree happened to be selected
 * when the frontend rehydrated.
 * It returns that workspace's KEY, not its `branchName`: the caller feeds the
 * answer straight into `addTerminalToWorkspace`.
 */
export function placementWorkspaceFor(owner: RepoOwner): string | null {
	if (owner.workspaceId) return owner.workspaceId;
	const repo = repositoriesStore.state.repositories[owner.repoPath];
	if (!repo) return null;
	const atRoot = Object.entries(repo.workspaces).find(
		([, ws]) => !!ws.worktreePath && sameDir(ws.worktreePath, owner.repoPath),
	);
	return atRoot?.[0] ?? null;
}

/** True while at least one registered repository lives on another machine. */
function anyRepoIsRemote(): boolean {
	const repos = repositoriesStore.state.repositories;
	for (const path in repos) {
		if (repos[path]?.connectionId) return true;
	}
	return false;
}

// Route repo-scoped calls to the machine that holds the repo. The transport asks
// this on every call, so a call reaches the right backend without its call site
// knowing a remote machine exists. Registered here rather than imported there:
// `transportRuntime` must import no store, because the stores import it.
//
// `untrack` because this runs inside whichever effect happened to make the call:
// a transport detail must not subscribe that effect to the whole registry. And
// the early exit matters — on a machine with no remote repo, which is every
// machine until one is added, the prefix walk below is pure cost on every call.
setRepoConnectionLookup((path) =>
	untrack(() => {
		if (!anyRepoIsRemote()) return undefined;
		const repoPath = resolveRepoPathFor(path);
		return repoPath ? repositoriesStore.getConnectionId(repoPath) : undefined;
	}),
);
