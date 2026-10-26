import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import type { WorkspaceLifecycleStatus } from "../stores/workspaceIdentity";
import type { RepoInfo } from "../types";
import type { OrphanAssessment } from "./git/createRepositoryRefreshCoordinator";

// ---------------------------------------------------------------------------
// TCC (macOS permission) error detection — global, shown once per session
// ---------------------------------------------------------------------------

const [tccDeniedPaths, setTccDeniedPaths] = createSignal<string[]>([]);
let tccAlertShown = false;

/** Paths that triggered an "Operation not permitted" TCC error. */
export { tccDeniedPaths };

/** Mark the TCC alert as shown so it doesn't repeat. */
export function markTccAlertShown(): void {
	tccAlertShown = true;
}

/** Check if an error is a macOS TCC permission denial and track it. */
function checkTccError(err: unknown, repoPath: string): void {
	const msg = err instanceof Error ? err.message : String(err);
	if (msg.includes("Operation not permitted") && !tccAlertShown) {
		setTccDeniedPaths((prev) => (prev.includes(repoPath) ? prev : [...prev, repoPath]));
	}
}

/** Changed file information for diff browser */
export interface ChangedFile {
	path: string;
	status: string; // "M" | "A" | "D" | "R" | "?"
	additions: number;
	deletions: number;
}

/** A base ref option with metadata for grouped dropdown display */
export interface BaseRefOption {
	name: string;
	/** "local" or "remote" */
	kind: string;
	/** Whether this is the default branch (e.g. main/master) */
	is_default: boolean;
}

export interface RemoveWorktreeResult {
	branch_delete_warning?: string | null;
	/** Branch the removed workspace was on, read off the record before removal. */
	branch: string;
	removal_rule: string;
}

/** One workspace's checkout. Keyed by workspace id; the branch is a field on the
 *  value because two workspaces may share one. */
export interface WorkspaceWorktree {
	branch: string;
	path: string;
	kind: "worktree";
}

/** Detached checkouts are inventory, not selectable branch workspaces. */
export interface DetachedWorktree {
	path: string;
	head: string;
	short_head: string;
	directory_name: string;
	state: "detached";
}

/** Read-only inventory entries that cannot safely use a branch workspace identity. */
export interface WorktreeDiagnostic {
	path: string;
	branch: string | null;
	state: "missing" | "duplicate_branch";
	guidance: string;
}

/** Repository hook for git operations */
export function useRepository() {
	/** Get repository info */
	async function getInfo(path: string): Promise<RepoInfo> {
		return await invoke<RepoInfo>("get_repo_info", { path });
	}

	/** Get git diff for a repository */
	async function getDiff(path: string, scope?: string): Promise<string> {
		return await invoke<string>("get_git_diff", { path, scope });
	}

	/** Open a path in an application, optionally at a specific line/col */
	async function openInApp(path: string, app: string, line?: number, col?: number): Promise<void> {
		await invoke("open_in_app", { path, app, line, col });
	}

	/** Rename a git branch */
	async function renameBranch(repoPath: string, oldName: string, newName: string): Promise<void> {
		await invoke("rename_branch", { path: repoPath, oldName, newName });
	}

	/** Create a new git branch (optionally checking it out). */
	async function createBranch(
		repoPath: string,
		name: string,
		startPoint: string | null,
		checkout: boolean,
	): Promise<void> {
		await invoke("create_branch", { path: repoPath, name, startPoint, checkout });
	}

	/** Get diff stats (additions/deletions) for a repository */
	async function getDiffStats(path: string, scope?: string): Promise<{ additions: number; deletions: number }> {
		try {
			return await invoke<{ additions: number; deletions: number }>("get_diff_stats", { path, scope });
		} catch (err) {
			appLogger.debug("git", "Failed to get diff stats", { path, err });
			return { additions: 0, deletions: 0 };
		}
	}

	/** Remove one workspace's checkout, addressed by workspace id */
	async function removeWorktree(
		repoPath: string,
		workspaceId: string,
		deleteBranch: boolean,
		force?: boolean,
		overrideLock?: boolean,
		expectedFingerprint?: string,
		confirmMissingCheckout?: boolean,
	): Promise<RemoveWorktreeResult> {
		return await invoke<RemoveWorktreeResult>("remove_worktree", {
			repoPath,
			workspaceId,
			deleteBranch,
			force: force ?? false,
			...(overrideLock ? { overrideLock: true } : {}),
			...(expectedFingerprint ? { expectedFingerprint } : {}),
			...(confirmMissingCheckout ? { confirmMissingCheckout: true } : {}),
		});
	}

	/** Create a linked worktree and warm its ignored build directories. */
	async function createWorktree(
		baseRepo: string,
		branchName: string,
		createBranch?: boolean,
		baseRef?: string,
	): Promise<{
		status: "ok";
		name: string;
		path: string;
		kind?: "worktree";
		workspace_id: string;
		branch: string;
		base_repo: string;
	}> {
		return await invoke("create_worktree", { baseRepo, branchName, createBranch, baseRef });
	}

	/** Fresh backend preflight used immediately before removal. */
	async function getWorkspaceLifecycle(repoPath: string, workspaceId: string): Promise<WorkspaceLifecycleStatus> {
		const status = await invoke<{
			dirty_files: number | null;
			untracked_files?: number | null;
			live_sessions?: Array<{ session_id: string; name: string }>;
			warnings?: string[];
			missing_checkout: boolean;
			dirty_fingerprint?: string;
			submodule_unpushed_commits?: Array<{ path: string; count: number }>;
			commit_status: WorkspaceLifecycleStatus["commitStatus"];
			removal_safety: WorkspaceLifecycleStatus["removalSafety"];
			error?: string;
		}>("get_workspace_lifecycle", { repoPath, workspaceId });
		return {
			dirtyFiles: status.dirty_files,
			untrackedFiles: status.untracked_files,
			liveSessions: status.live_sessions?.map((session) => ({ sessionId: session.session_id, name: session.name })),
			warnings: status.warnings,
			missingCheckout: status.missing_checkout,
			dirtyFingerprint: status.dirty_fingerprint,
			submoduleUnpushedCommits: status.submodule_unpushed_commits,
			commitStatus: status.commit_status,
			removalSafety: status.removal_safety,
			error: status.error,
		};
	}

	/** Get workspaces: workspace id → its checkout */
	async function getWorktreePaths(repoPath: string): Promise<Record<string, WorkspaceWorktree>> {
		try {
			return await invoke<Record<string, WorkspaceWorktree>>("get_worktree_paths", { repoPath });
		} catch (err) {
			appLogger.warn("git", `Failed to get worktree paths for ${repoPath}`, err);
			return {};
		}
	}

	/** Get list of changed files with status and stats */
	async function getChangedFiles(path: string, scope?: string): Promise<ChangedFile[]> {
		try {
			return await invoke<ChangedFile[]>("get_changed_files", { path, scope });
		} catch (err) {
			appLogger.error("git", "Failed to get changed files", err);
			return [];
		}
	}

	/** Get diff for a single file */
	async function getFileDiff(path: string, file: string, scope?: string, untracked?: boolean): Promise<string> {
		try {
			return await invoke<string>("get_file_diff", { path, file, scope, untracked: untracked || undefined });
		} catch (err) {
			appLogger.error("git", "Failed to get file diff", err);
			return "";
		}
	}

	/** Markdown file entry with git status */
	interface MarkdownFileEntry {
		path: string;
		git_status: string; // "modified" | "staged" | "untracked" | ""
		is_ignored: boolean;
		modified_at: number; // Unix epoch seconds (0 if unavailable)
	}

	/** List all markdown files in repository with git status */
	async function listMarkdownFiles(path: string): Promise<MarkdownFileEntry[]> {
		try {
			return await invoke<MarkdownFileEntry[]>("list_markdown_files", { path });
		} catch (err) {
			appLogger.error("git", "Failed to list markdown files", err);
			return [];
		}
	}

	/** Read file content */
	async function readFile(path: string, file: string): Promise<string> {
		try {
			return await invoke<string>("read_file", { path, file });
		} catch (err) {
			const msg = String(err);
			// ENOENT is legitimate (file deleted/renamed while a tab still points at it)
			// and not actionable; log at debug so the app log isn't spammed on every
			// repo revision bump (a stale tab re-reads the missing file each bump).
			if (msg.includes("No such file or directory") || msg.includes("os error 2")) {
				appLogger.debug("git", "File not found", { path, file });
			} else {
				appLogger.error("git", "Failed to read file", { path, file, error: msg });
			}
			return "";
		}
	}

	/** Generate a unique worktree branch name, avoiding collisions with existing names */
	async function generateWorktreeName(existingNames: string[]): Promise<string> {
		return await invoke<string>("generate_worktree_name_cmd", { existingNames });
	}

	/** Generate a hybrid clone branch name: `{sanitized_source}--{random_name}` */
	async function generateCloneBranchName(sourceBranch: string, existingNames: string[]): Promise<string> {
		return await invoke<string>("generate_clone_branch_name_cmd", { sourceBranch, existingNames });
	}

	/** List base ref options for branch/worktree creation (local + remote, grouped) */
	async function listBaseRefOptions(repoPath: string): Promise<BaseRefOption[]> {
		try {
			return await invoke<BaseRefOption[]>("list_base_ref_options", { repoPath });
		} catch (err) {
			appLogger.error("git", `Failed to list base ref options for ${repoPath}`, err);
			return [];
		}
	}

	/** Result of merge-and-archive operation */
	interface MergeArchiveResult {
		merged: boolean;
		/** archived | deleted | pending | needs_confirmation */
		action: string;
		archive_path: string | null;
		branch_delete_warning?: string | null;
		/** Commits the branch had that the target did not, measured before the merge.
		 *  0 means the merge was an "Already up to date" no-op. */
		commits_ahead: number;
		/** Whether the worktree had uncommitted changes at pre-flight time. */
		worktree_dirty: boolean;
	}

	/** Merge a worktree branch into target, then archive or delete.
	 *  `force` skips the dirty-worktree guard after the user confirms the loss. */
	async function mergeAndArchiveWorktree(
		repoPath: string,
		branchName: string,
		workspaceId: string,
		targetBranch: string,
		afterMerge: string,
		force = false,
		expectedFingerprint?: string,
	): Promise<MergeArchiveResult> {
		return await invoke<MergeArchiveResult>("merge_and_archive_worktree", {
			repoPath,
			branchName,
			workspaceId,
			targetBranch,
			afterMerge,
			force,
			...(expectedFingerprint ? { expectedFingerprint } : {}),
		});
	}

	/** Finalize a pending merge by archiving or deleting the worktree.
	 *  Cleanup is destructive, so it passes the same dirty-worktree guard as
	 *  `mergeAndArchiveWorktree`: without `force` a worktree that is not known to
	 *  be clean comes back as `action: "needs_confirmation"` instead of being wiped. */
	async function finalizeMergedWorktree(
		repoPath: string,
		workspaceId: string,
		action: "archive" | "delete",
		force = false,
		expectedFingerprint?: string,
	): Promise<MergeArchiveResult> {
		return await invoke<MergeArchiveResult>("finalize_merged_worktree", {
			repoPath,
			workspaceId,
			action,
			force,
			...(expectedFingerprint ? { expectedFingerprint } : {}),
		});
	}

	/** Recent commit entry */
	interface RecentCommit {
		hash: string;
		short_hash: string;
		subject: string;
	}

	/** Get branches fully merged into the repo's main branch */
	async function getMergedBranches(repoPath: string): Promise<string[]> {
		try {
			return await invoke<string[]>("get_merged_branches", { path: repoPath });
		} catch (err) {
			appLogger.warn("git", `Failed to get merged branches for ${repoPath}`, err);
			return [];
		}
	}

	/** Aggregate repo snapshot: worktree paths + merged branches + per-path diff stats.
	 *  Replaces 3 separate IPC calls in refreshAllBranchStats with one round-trip. */
	async function getRepoSummary(repoPath: string): Promise<{
		worktree_paths: Record<string, WorkspaceWorktree>;
		detached_worktrees: DetachedWorktree[];
		worktree_diagnostics?: WorktreeDiagnostic[];
		merged_branches: string[];
		diff_stats: Record<string, { additions: number; deletions: number }>;
		last_commit_ts: Record<string, number | null>;
		workspace_statuses: Record<string, unknown>;
	}> {
		try {
			return await invoke("get_repo_summary", { repoPath });
		} catch (err) {
			appLogger.warn("git", `Failed to get repo summary for ${repoPath}`, err);
			return {
				worktree_paths: {},
				detached_worktrees: [],
				merged_branches: [],
				diff_stats: {},
				last_commit_ts: {},
				workspace_statuses: {},
			};
		}
	}

	/** Fast structural snapshot: worktree paths + merged branches only.
	 *  Used by progressive loading Phase 1 — returns before expensive diff stats. */
	async function getRepoStructure(repoPath: string): Promise<{
		worktree_paths: Record<string, WorkspaceWorktree>;
		detached_worktrees: DetachedWorktree[];
		worktree_diagnostics?: WorktreeDiagnostic[];
		merged_branches: string[];
	}> {
		try {
			return await invoke("get_repo_structure", { repoPath });
		} catch (err) {
			checkTccError(err, repoPath);
			appLogger.warn("git", `Failed to get repo structure for ${repoPath}`, err);
			return { worktree_paths: {}, detached_worktrees: [], merged_branches: [] };
		}
	}

	/** Per-worktree diff stats + last-commit timestamps.
	 *  Used by progressive loading Phase 2 — runs after structure is already displayed. */
	async function getRepoDiffStats(repoPath: string): Promise<{
		diff_stats: Record<string, { additions: number; deletions: number }>;
		last_commit_ts: Record<string, number | null>;
		workspace_statuses: Record<
			string,
			{
				dirty_files: number | null;
				commit_status: import("../stores/workspaceIdentity").WorkspaceCommitStatus;
				removal_safety: import("../stores/workspaceIdentity").WorkspaceRemovalSafety;
				error?: string;
			}
		>;
	}> {
		try {
			return await invoke("get_repo_diff_stats", { repoPath });
		} catch (err) {
			checkTccError(err, repoPath);
			appLogger.warn("git", `Failed to get repo diff stats for ${repoPath}`, err);
			return { diff_stats: {}, last_commit_ts: {}, workspace_statuses: {} };
		}
	}

	/** Result of branch switch operation */
	interface SwitchBranchResult {
		success: boolean;
		stashed: boolean;
		previous_branch: string;
		new_branch: string;
	}

	/** Switch the main worktree to a different branch via git checkout.
	 *  Runs in Rust (no PTY involvement) — safe even with editors open.
	 *  Returns "dirty" error string when working tree has uncommitted changes. */
	async function switchBranch(
		repoPath: string,
		branchName: string,
		opts?: { force?: boolean; stash?: boolean },
	): Promise<SwitchBranchResult> {
		return await invoke<SwitchBranchResult>("switch_branch", {
			repoPath,
			branchName,
			force: opts?.force ?? false,
			stash: opts?.stash ?? false,
		});
	}

	/** Check out a remote-only branch as a new local branch tracking origin. */
	async function checkoutRemoteBranch(repoPath: string, branchName: string): Promise<void> {
		await invoke("checkout_remote_branch", { repoPath, branchName });
	}

	/** Detect linked worktrees in detached HEAD state (branch was deleted). */
	async function detectOrphanWorktrees(repoPath: string): Promise<string[]> {
		try {
			return await invoke<string[]>("detect_orphan_worktrees", { repoPath });
		} catch (err) {
			appLogger.error("git", "Failed to detect orphan worktrees", err);
			return [];
		}
	}

	async function assessOrphanCleanup(repoPath: string): Promise<OrphanAssessment[]> {
		return await invoke("assess_orphan_cleanup", { repoPath });
	}

	async function beginOrphanCleanup(repoPath: string, paths: string[]): Promise<void> {
		await invoke("begin_orphan_cleanup", { repoPath, paths });
	}

	async function pendingOrphanCleanupAnswer(repoPath: string): Promise<boolean | null> {
		return await invoke<boolean | null>("pending_orphan_cleanup_answer", { repoPath });
	}

	async function clearOrphanCleanup(repoPath: string, kept: boolean): Promise<void> {
		await invoke("clear_orphan_cleanup", { repoPath, kept });
	}

	/** Remove a detached-HEAD worktree by path (no branch to look up). */
	async function removeOrphanWorktree(
		repoPath: string,
		worktreePath: string,
		safeOnly = false,
		confirmedSessions: string[] = [],
	): Promise<void> {
		await invoke("remove_orphan_worktree", { repoPath, worktreePath, safeOnly, confirmedSessions });
	}

	/** Merge a PR via GitHub REST API. merge_method: "merge" | "squash" | "rebase" */
	async function mergePrViaGithub(
		repoPath: string,
		prNumber: number,
		mergeMethod: string,
		expectedHeadSha: string,
	): Promise<string> {
		return await invoke<string>("merge_pr_via_github", { repoPath, prNumber, mergeMethod, expectedHeadSha });
	}

	/** List local branch names for a repository */
	async function listLocalBranches(repoPath: string): Promise<string[]> {
		try {
			return await invoke<string[]>("list_local_branches", { repoPath });
		} catch (err) {
			checkTccError(err, repoPath);
			appLogger.error("git", "Failed to list local branches", err);
			return [];
		}
	}

	/** Run a setup/run script in a given directory */
	async function runSetupScript(
		script: string,
		cwd: string,
	): Promise<{ exit_code: number; stdout: string; stderr: string }> {
		return await invoke<{ exit_code: number; stdout: string; stderr: string }>("run_setup_script", { script, cwd });
	}

	/** Get recent commits for a repository */
	async function getRecentCommits(path: string, count?: number): Promise<RecentCommit[]> {
		try {
			return await invoke<RecentCommit[]>("get_recent_commits", { path, count });
		} catch (err) {
			appLogger.error("git", "Failed to get recent commits", err);
			return [];
		}
	}

	return {
		getInfo,
		getDiff,
		getDiffStats,
		openInApp,
		renameBranch,
		createBranch,
		removeWorktree,
		createWorktree,
		getWorkspaceLifecycle,
		getWorktreePaths,
		getChangedFiles,
		getFileDiff,
		listMarkdownFiles,
		readFile,
		generateWorktreeName,
		generateCloneBranchName,
		listBaseRefOptions,
		mergeAndArchiveWorktree,
		finalizeMergedWorktree,
		getMergedBranches,
		getRepoSummary,
		getRepoStructure,
		getRepoDiffStats,
		checkoutRemoteBranch,
		detectOrphanWorktrees,
		assessOrphanCleanup,
		beginOrphanCleanup,
		pendingOrphanCleanupAnswer,
		clearOrphanCleanup,
		removeOrphanWorktree,
		mergePrViaGithub,
		listLocalBranches,
		switchBranch,
		runSetupScript,
		getRecentCommits,
	};
}
