import { batch, createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { clearForRepo as clearFocusForRepo } from "../stores/focusRegistry";
import { mdTabsStore } from "../stores/mdTabs";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import { reconcileTerminalOwnership } from "../stores/terminalOwnership";
import { terminalsStore } from "../stores/terminals";
import { isTauri, rpc } from "../transport";
import type { RepoInfo } from "../types";
import { openDialog as open } from "../utils/nativeDialog";
import { reportActionError } from "../utils/reportActionError";
import { findOrphanTerminals } from "../utils/terminalOrphans";
import { createBranchSelectionCoordinator } from "./git/createBranchSelectionCoordinator";
import { createRepositoryRefreshCoordinator, type OrphanAssessment } from "./git/createRepositoryRefreshCoordinator";
import { createTerminalWorktreeCoordinator } from "./git/createTerminalWorktreeCoordinator";
import { createWorktreeCreationCoordinator } from "./git/createWorktreeCreationCoordinator";
import { createWorktreeRemovalCoordinator } from "./git/createWorktreeRemovalCoordinator";
import { createWorktreeWorkflowCoordinator } from "./git/createWorktreeWorkflowCoordinator";
import type { RemoveWorktreeResult } from "./useRepository";

/** Dependencies injected into useGitOperations */
export interface GitOperationsDeps {
	repo: {
		getInfo: (path: string) => Promise<{
			path: string;
			name: string;
			initials: string;
			branch: string;
			status: "clean" | "dirty" | "conflict" | "merge" | "not-git" | "unknown";
			is_git_repo: boolean;
		}>;
		getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
		getWorktreePaths: (repoPath: string) => Promise<Record<string, import("./useRepository").WorkspaceWorktree>>;
		getRepoSummary: (repoPath: string) => Promise<{
			worktree_paths: Record<string, import("./useRepository").WorkspaceWorktree>;
			merged_branches: string[];
			diff_stats: Record<string, { additions: number; deletions: number }>;
			last_commit_ts: Record<string, number | null>;
			workspace_statuses: Record<string, unknown>;
		}>;
		getRepoStructure: (repoPath: string) => Promise<{
			worktree_paths: Record<string, import("./useRepository").WorkspaceWorktree>;
			merged_branches: string[];
		}>;
		getRepoDiffStats: (repoPath: string) => Promise<{
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
		}>;
		removeWorktree: (
			repoPath: string,
			workspaceId: string,
			deleteBranch: boolean,
			force?: boolean,
			overrideLock?: boolean,
			expectedFingerprint?: string,
			confirmMissingCheckout?: boolean,
		) => Promise<RemoveWorktreeResult | undefined>;
		createWorktree: (
			baseRepo: string,
			branchName: string,
			createBranch?: boolean,
			baseRef?: string,
		) => Promise<{
			status: "ok";
			name: string;
			path: string;
			workspace_id: string;
			branch: string;
			base_repo: string;
		}>;
		renameBranch: (repoPath: string, oldName: string, newName: string) => Promise<void>;
		createBranch: (repoPath: string, name: string, startPoint: string | null, checkout: boolean) => Promise<void>;
		generateWorktreeName: (existingNames: string[]) => Promise<string>;
		generateCloneBranchName: (sourceBranch: string, existingNames: string[]) => Promise<string>;
		listBaseRefOptions: (repoPath: string) => Promise<import("./useRepository").BaseRefOption[]>;
		mergeAndArchiveWorktree: (
			repoPath: string,
			branchName: string,
			workspaceId: string,
			targetBranch: string,
			afterMerge: string,
			force?: boolean,
		) => Promise<{
			merged: boolean;
			action: string;
			archive_path: string | null;
			commits_ahead?: number;
			worktree_dirty?: boolean;
		}>;
		finalizeMergedWorktree: (
			repoPath: string,
			workspaceId: string,
			action: "archive" | "delete",
			force?: boolean,
		) => Promise<{ merged: boolean; action: string; archive_path: string | null }>;
		listLocalBranches: (repoPath: string) => Promise<string[]>;
		getMergedBranches: (repoPath: string) => Promise<string[]>;
		checkoutRemoteBranch: (repoPath: string, branchName: string) => Promise<void>;
		detectOrphanWorktrees: (repoPath: string) => Promise<string[]>;
		assessOrphanCleanup: (repoPath: string) => Promise<Array<{ path: string; safe: boolean; reason?: string }>>;
		beginOrphanCleanup: (repoPath: string, paths: string[]) => Promise<void>;
		pendingOrphanCleanupAnswer: (repoPath: string) => Promise<boolean | null>;
		clearOrphanCleanup: (repoPath: string, kept: boolean) => Promise<void>;
		removeOrphanWorktree: (
			repoPath: string,
			worktreePath: string,
			safeOnly?: boolean,
			confirmedSessions?: string[],
		) => Promise<void>;
		mergePrViaGithub: (
			repoPath: string,
			prNumber: number,
			mergeMethod: string,
			expectedHeadSha: string,
		) => Promise<string>;
		getWorkspaceLifecycle: (
			repoPath: string,
			workspaceId: string,
		) => Promise<import("../stores/workspaceIdentity").WorkspaceLifecycleStatus>;
		switchBranch: (
			repoPath: string,
			branchName: string,
			opts?: { force?: boolean; stash?: boolean },
		) => Promise<{ success: boolean; stashed: boolean; previous_branch: string; new_branch: string }>;
		runSetupScript: (script: string, cwd: string) => Promise<{ exit_code: number; stdout: string; stderr: string }>;
	};
	pty: {
		canSpawn: () => Promise<boolean>;
		write: (sessionId: string, data: string) => Promise<void>;
		getWorktreesDir: (repoPath?: string) => Promise<string>;
	};
	dialogs: {
		confirmRemoveRepo: (repoName: string) => Promise<boolean>;
		confirmRemoveWorktree: (
			branchName: string,
			status: import("../stores/workspaceIdentity").WorkspaceLifecycleStatus,
			deleteBranch: boolean,
		) => Promise<boolean>;
		confirmRemoveLockedWorktree?: (branchName: string, deleteBranch?: boolean) => Promise<boolean>;
		confirmStashAndSwitch?: (branchName: string) => Promise<boolean>;
		confirmOrphanCleanup?: (
			repoPath: string,
			assessments: OrphanAssessment[],
			countdownSeconds: number,
		) => Promise<boolean>;
		answerOrphanCleanup?: (repoPath: string, remove: boolean) => void;
		/** Archiving or deleting this worktree would destroy uncommitted work — proceed anyway? */
		confirmDirtyWorktreeCleanup?: (
			branchName: string,
			action: string,
			commitsAhead: number,
			lifecycle?: import("../stores/workspaceIdentity").WorkspaceLifecycleStatus,
		) => Promise<boolean>;
		/** Surface a git failure in a dialog with the full output; returns true if the user chose Retry. */
		reportGitError?: (title: string, detail: string, offerRetry?: boolean) => Promise<boolean>;
		/** Browser mode only: browse the serving machine for a repo path */
		promptRepoPath?: () => Promise<string | null>;
		/**
		 * Browse a connected machine's filesystem and return an absolute path on it.
		 *
		 * The local flow uses the OS file dialog, which enumerates the local disk
		 * through the OS and cannot be aimed at another machine. Mounting the remote
		 * host does not rescue it either: the dialog would then hand back the local
		 * mount point, not the path the daemon knows, so the repo would be registered
		 * under a path that does not exist on the machine meant to serve it.
		 */
		pickRemoteRepoPath?: (connectionId: string) => Promise<string | null>;
	};
	closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>;
	createNewTerminal: () => Promise<string | undefined>;
	setStatusInfo: (msg: string) => void;
	getDefaultFontSize: () => number;
	getMaxTabNameLength: () => number;
	/** Returns the effective promptOnCreate setting for the given repo.
	 *  When false, handleAddWorktree skips the dialog and creates instantly.
	 *  Defaults to true (show dialog) when not provided. */
	getPromptOnCreate?: (repoPath: string) => boolean;
}

export type { AgentSeed } from "./git/agentSeed";
export { buildAgentSeed, resolveAutofixAgent } from "./git/agentSeed";
/** Git and repository operations extracted from App.tsx */
export function useGitOperations(deps: GitOperationsDeps) {
	// Derived, never stored: a second copy of the active repo drifts whenever a
	// caller switches repos through repositoriesStore alone (navigateToTerminal).
	const currentRepoPath = () => repositoriesStore.state.activeRepoPath ?? undefined;
	const currentBranch = () => {
		const repo = repositoriesStore.getActive();
		if (!repo?.activeWorkspaceId) return null;
		return repositoriesStore.branchNameFor(repo.path, repo.activeWorkspaceId);
	};
	const [repoStatus, setRepoStatus] = createSignal<"clean" | "dirty" | "conflict" | "merge" | "unknown">("unknown");
	const [branchToRename, setBranchToRename] = createSignal<{ repoPath: string; branchName: string } | null>(null);
	const [branchToCreate, setBranchToCreate] = createSignal<{ repoPath: string; startPoint: string | null } | null>(
		null,
	);
	const [creatingWorktreeRepos, setCreatingWorktreeRepos] = createSignal<Set<string>>(new Set());
	// Key: `${repoPath}::${branchName}` — prevents concurrent remove calls for same branch
	const [removingBranches, setRemovingBranches] = createSignal<Set<string>>(new Set());

	const [worktreeDialogState, setWorktreeDialogState] = createSignal<{
		repoPath: string;
		suggestedName: string;
		existingBranches: string[];
		worktreeBranches: string[];
		worktreesDir: string;
		baseRefs: import("./useRepository").BaseRefOption[];
	} | null>(null);

	/** Pending merge context — set when afterMerge=ask; cleared once the user picks or skips cleanup */
	const [mergePendingCtx, setMergePendingCtx] = createSignal<{
		repoPath: string;
		/** The row being cleaned up. */
		workspaceId: string;
		/** The ref being merged and deleted — a different object from the row. */
		branchName: string;
		baseBranch: string;
		/** Base repo has uncommitted changes — the "Switch to base" step stashes them. */
		hasDirtyFiles: boolean;
		/** The branch's own worktree has uncommitted changes — archive/delete destroys them. */
		worktreeDirty: boolean;
		removalWarnings?: string[];
		worktreeFingerprint?: string;
		submoduleUnpushedCommits?: Array<{ path: string; count: number }>;
	} | null>(null);

	const { markRecentlyCreated, refreshAllBranchStats } = createRepositoryRefreshCoordinator({
		repo: deps.repo,
		dialogs: deps.dialogs,
		closeTerminal: deps.closeTerminal,
		closeTerminalsInWorktree: (worktreePath) => closeTerminalsInWorktree(worktreePath),
		setStatusInfo: deps.setStatusInfo,
	});

	const { handleAddTerminalToWorkspace, handleBranchSelect, handleBranchSelectInner, resumeAgentTabsAfterUpdate } =
		createBranchSelectionCoordinator({
			repo: deps.repo,
			pty: deps.pty,
			setStatusInfo: deps.setStatusInfo,
			getDefaultFontSize: deps.getDefaultFontSize,
		});

	const handleRemoveRepo = async (repoPath: string) => {
		const repoState = repositoriesStore.get(repoPath);
		if (!repoState) return;

		const confirmed = await deps.dialogs.confirmRemoveRepo(repoState.displayName);
		if (!confirmed) return;

		for (const branch of Object.values(repoState.workspaces)) {
			for (const termId of branch.terminals) {
				await deps.closeTerminal(termId, true);
			}
		}

		// The repo's file-backed tabs go with it. Left behind they become
		// unreachable rather than harmless: getVisibleIds filters on a branch key
		// that no longer resolves, so the tab is invisible AND immortal, and every
		// removal leaks another set. closeTerminal is the one path that closes a tab
		// completely — store entry, pane slot, next selection — so reuse it instead
		// of re-deriving that cleanup here. No `skipConfirm`: removing a repo from
		// the sidebar deletes nothing on disk, so a dirty editor still gets its save
		// prompt rather than losing the edit silently.
		for (const tab of [
			...editorTabsStore.getForRepo(repoPath),
			...diffTabsStore.getForRepo(repoPath),
			...mdTabsStore.getForRepo(repoPath),
		]) {
			await deps.closeTerminal(tab.id);
		}
		clearFocusForRepo(repoPath);

		invoke("stop_repo_watcher", { repoPath }).catch((err) =>
			appLogger.warn("app", `RepoWatcher failed to stop for ${repoPath}`, err),
		);

		repositoriesStore.remove(repoPath);
		repoSettingsStore.remove(repoPath);

		deps.setStatusInfo(`Removed ${repoState.displayName}`);

		if (terminalsStore.getCount() === 0) {
			await deps.createNewTerminal();
		}
	};

	const { handleRemoveWorkspace } = createWorktreeRemovalCoordinator({
		repo: deps.repo,
		dialogs: deps.dialogs,
		closeTerminal: deps.closeTerminal,
		setStatusInfo: deps.setStatusInfo,
		removingBranches,
		setRemovingBranches,
	});

	/** Opens the rename dialog; false when the branch is protected (main). The one guard for every entry point. */
	const handleOpenRenameBranchDialog = (repoPath: string, branchName: string): boolean => {
		if (repositoriesStore.get(repoPath)?.workspaces[branchName]?.isMain) return false;
		setBranchToRename({ repoPath, branchName });
		return true;
	};

	const handleOpenCreateBranchDialog = (repoPath: string, startPoint?: string | null) => {
		setBranchToCreate({ repoPath, startPoint: startPoint ?? null });
	};

	const handleCreateBranch = async (name: string, checkout: boolean) => {
		const target = branchToCreate();
		if (!target) return;

		// Throw on failure so the dialog can surface the error inline.
		await deps.repo.createBranch(target.repoPath, name, target.startPoint, checkout);

		repositoriesStore.setWorkspace(target.repoPath, name, {});
		if (checkout) {
			// The checkout moves that repo's branch, not the app's focus: a branch
			// created from a background repo's row leaves the active repo alone.
			repositoriesStore.setActiveWorkspace(target.repoPath, name);
		}
		deps.setStatusInfo(`Created branch ${name}${checkout ? " (checked out)" : ""}`);
		void refreshAllBranchStats();
	};

	const handleRenameBranch = async (oldName: string, newName: string) => {
		const branch = branchToRename();
		if (!branch) return;

		try {
			await deps.repo.renameBranch(branch.repoPath, oldName, newName);
		} catch (err) {
			appLogger.error("git", "Failed to rename branch", err);
			reportActionError(deps.setStatusInfo, `Failed to rename branch: ${err}`);
			return;
		}

		repositoriesStore.renameBranch(branch.repoPath, oldName, newName);
		repoSettingsStore.moveLabel(branch.repoPath, oldName, newName);

		deps.setStatusInfo(`Renamed branch ${oldName} to ${newName}`);
	};

	const activeWorktreePath = () => {
		const activeRepo = repositoriesStore.getActive();
		if (!activeRepo?.activeWorkspaceId) return undefined;
		return activeRepo.workspaces[activeRepo.activeWorkspaceId]?.worktreePath || activeRepo.path;
	};

	const activeRunCommand = () => {
		const activeRepo = repositoriesStore.getActive();
		if (!activeRepo?.activeWorkspaceId) return undefined;
		return activeRepo.workspaces[activeRepo.activeWorkspaceId]?.runCommand;
	};

	/** Add a repo by path and make it active. Shared by the sidebar picker and
	 *  the `tuic <dir>` deep link — both must land on the exact same state. */
	const addRepoByPath = async (path: string) => {
		try {
			const info = await deps.repo.getInfo(path);

			// Close orphan terminals (not associated with any branch)
			const branchTerminalMap: Record<string, string[]> = {};
			for (const repoPath of repositoriesStore.getPaths()) {
				const repoState = repositoriesStore.get(repoPath);
				if (repoState) {
					for (const branch of Object.values(repoState.workspaces)) {
						branchTerminalMap[`${repoPath}:${branch.branchName}`] = branch.terminals;
					}
				}
			}
			const orphanTerminals = findOrphanTerminals(terminalsStore.getIds(), branchTerminalMap);

			for (const id of orphanTerminals) {
				await deps.closeTerminal(id, true);
			}

			repositoriesStore.add({
				path: info.path,
				displayName: info.name,
				initials: info.initials,
				isGitRepo: info.is_git_repo,
			});

			if (info.branch) {
				repositoriesStore.setWorkspace(info.path, info.branch, { worktreePath: info.path });
				repositoriesStore.setActiveWorkspace(info.path, info.branch);
				await handleAddTerminalToWorkspace(info.path, info.branch);
			} else if (!info.is_git_repo) {
				// Non-git directory: create a shell entry so the user can open terminals
				const shellBranch = "shell";
				repositoriesStore.setWorkspace(info.path, shellBranch, {
					worktreePath: info.path,
					isMain: true,
					isShell: true,
				});
				repositoriesStore.setActiveWorkspace(info.path, shellBranch);
				await handleAddTerminalToWorkspace(info.path, shellBranch);
			}

			repositoriesStore.setActive(info.path);
			setRepoStatus(info.status === "not-git" ? "unknown" : info.status);

			// Start unified repo watcher (covers HEAD, git state, working tree).
			// Also started for non-git directories so a later `git init` is detected
			// (the .git-creation event triggers the non-git→git transition probe).
			invoke("start_repo_watcher", { repoPath: info.path }).catch((err) =>
				appLogger.warn("app", `RepoWatcher failed to start for ${info.path}`, err),
			);

			// A terminal already running inside this path was parked in some other repo
			// because nothing claimed its cwd. Now something does.
			reconcileTerminalOwnership();

			await refreshAllBranchStats(info.path, { immediate: true });
		} catch (err) {
			appLogger.error("git", "Failed to add repository", err);
			reportActionError(deps.setStatusInfo, `Failed to add repo: ${err}`);
		}
	};

	const handleAddRepo = async () => {
		let path: string | null = null;

		if (isTauri()) {
			const selected = await open({
				directory: true,
				multiple: false,
				title: "Select Repository Folder",
				defaultPath: repositoriesStore.getActive()?.path ?? "/",
			});
			if (!selected) return;
			path = typeof selected === "string" ? selected : selected[0];
		} else {
			// Browser mode: browse the server through the in-app directory picker
			const input = await deps.dialogs.promptRepoPath?.();
			if (!input?.trim()) return;
			path = input.trim();
		}

		if (!path) return;

		await addRepoByPath(path);
	};

	const handleAddRemoteRepo = async (connectionId: string) => {
		// Browse the machine when a picker is wired; fall back to typing the path,
		// which is all this flow ever had and still the only option under a test
		// double that provides no picker.
		const input = deps.dialogs.pickRemoteRepoPath
			? await deps.dialogs.pickRemoteRepoPath(connectionId)
			: await deps.dialogs.promptRepoPath?.();
		if (!input?.trim()) return;
		const remotePath = input.trim();

		try {
			// Validate by fetching repo info from remote
			const info = await rpc<RepoInfo>("get_repo_info", { path: remotePath }, connectionId);

			repositoriesStore.add({
				path: info.path,
				displayName: info.name,
				initials: info.initials,
				isGitRepo: info.is_git_repo,
				connectionId,
			});

			if (info.branch) {
				repositoriesStore.setWorkspace(info.path, info.branch, { worktreePath: info.path });
				repositoriesStore.setActiveWorkspace(info.path, info.branch);
				await handleAddTerminalToWorkspace(info.path, info.branch);
			} else if (!info.is_git_repo) {
				const shellBranch = "shell";
				repositoriesStore.setWorkspace(info.path, shellBranch, {
					worktreePath: info.path,
					isMain: true,
					isShell: true,
				});
				repositoriesStore.setActiveWorkspace(info.path, shellBranch);
				await handleAddTerminalToWorkspace(info.path, shellBranch);
			}

			reconcileTerminalOwnership();
			repositoriesStore.setActive(info.path);
		} catch (err) {
			appLogger.error("git", `Failed to add remote repo from ${connectionId}`, err);
			reportActionError(deps.setStatusInfo, `Failed to add remote repo: ${err}`);
		}
	};

	const { confirmCreateWorktree, handleAddWorktree, handleCreateWorktreeFromBranch, setupNewWorktree } =
		createWorktreeCreationCoordinator({
			repo: deps.repo,
			pty: deps.pty,
			getPromptOnCreate: deps.getPromptOnCreate,
			setStatusInfo: deps.setStatusInfo,
			creatingWorktreeRepos,
			setCreatingWorktreeRepos,
			worktreeDialogState,
			setWorktreeDialogState,
			markRecentlyCreated,
			handleAddTerminalToWorkspace,
		});

	const {
		closeTerminalsForBranch,
		closeTerminalsInWorktree,
		dismissMergePending,
		handleAutofixIssue,
		handleConflictAssist,
		handleMergeAndArchive,
		handleMergeAndArchiveIntoMain,
	} = createWorktreeWorkflowCoordinator({
		repo: deps.repo,
		closeTerminal: deps.closeTerminal,
		setStatusInfo: deps.setStatusInfo,
		// No dialog wired: refuse rather than silently destroy. The guard only fires
		// when uncommitted work would be swept away by the cleanup.
		confirmDirtyWorktreeCleanup: (branchName, action, commitsAhead, lifecycle) =>
			deps.dialogs.confirmDirtyWorktreeCleanup?.(branchName, action, commitsAhead, lifecycle) ?? Promise.resolve(false),
		creatingWorktreeRepos,
		setCreatingWorktreeRepos,
		setMergePendingCtx,
		setupNewWorktree,
		refreshAllBranchStats: (repoPath) => refreshAllBranchStats(repoPath, { immediate: true }),
	});

	/**
	 * The repo and branch a new tab opens in, or null when no repo is active.
	 *
	 * Prefers the active terminal's branch registration and CWD as source of truth —
	 * the store's activeBranch may be stale if HEAD changed externally and head-changed
	 * hasn't been fully processed yet (race between refreshAllBranchStats and setActiveBranch).
	 */
	const resolveNewTabTarget = (): { repoPath: string; branchName: string } | null => {
		const activeTerminalId = terminalsStore.state.activeId;
		const activeTerminal = activeTerminalId ? terminalsStore.get(activeTerminalId) : null;
		const activeCwd = activeTerminal?.cwd ?? null;

		if (activeCwd) {
			for (const repoPath of repositoriesStore.getPaths()) {
				const repo = repositoriesStore.get(repoPath);
				if (!repo) continue;

				// When multiple branches share the same worktreePath (main checkout after HEAD move),
				// prefer the branch that owns the active terminal — it reflects the partially-processed
				// head-changed state more accurately than insertion-order iteration.
				if (activeTerminalId) {
					const ownerEntry = Object.entries(repo.workspaces).find(
						([, b]) => b.worktreePath === activeCwd && b.terminals.includes(activeTerminalId),
					);
					if (ownerEntry) return { repoPath, branchName: ownerEntry[0] };
				}

				// Linked worktree: unique worktreePath per branch, unambiguous match
				const match = Object.values(repo.workspaces).find((b) => b.worktreePath && b.worktreePath === activeCwd);
				if (match) return { repoPath, branchName: match.branchName };
			}
		}

		// Fall back to store's active branch (no active terminal or no CWD match)
		const activeRepo = repositoriesStore.getActive();
		return activeRepo?.activeWorkspaceId
			? { repoPath: activeRepo.path, branchName: activeRepo.activeWorkspaceId }
			: null;
	};

	const handleNewTab = async () => {
		const target = resolveNewTabTarget();
		if (target) {
			await handleAddTerminalToWorkspace(target.repoPath, target.branchName);
		} else {
			await deps.createNewTerminal();
		}
	};

	const handleRunCommand = (forceDialog: boolean, openDialog: () => void) => {
		const savedCmd = activeRunCommand();
		if (savedCmd && !forceDialog) {
			executeRunCommand(savedCmd);
		} else {
			openDialog();
		}
	};

	const executeRunCommand = async (command: string) => {
		const activeRepo = repositoriesStore.getActive();
		if (!activeRepo?.activeWorkspaceId) return;

		repositoriesStore.setRunCommand(activeRepo.path, activeRepo.activeWorkspaceId, command);

		const canSpawn = await deps.pty.canSpawn();
		if (!canSpawn) {
			deps.setStatusInfo("Max sessions reached (50)");
			return;
		}

		const branch = activeRepo.workspaces[activeRepo.activeWorkspaceId];
		const cwd = branch?.worktreePath || activeRepo.path;
		const maxNameLen = deps.getMaxTabNameLength();
		const tabName = command.length > maxNameLen ? command.slice(0, maxNameLen) + "..." : command;

		const id = terminalsStore.add({
			sessionId: null,
			fontSize: deps.getDefaultFontSize(),
			name: tabName,
			cwd,
			awaitingInput: null,
		});

		terminalsStore.setActive(id);
		repositoriesStore.addTerminalToWorkspace(activeRepo.path, activeRepo.activeWorkspaceId, id);

		let waitAttempts = 0;
		const waitForSession = setInterval(async () => {
			waitAttempts++;
			const terminal = terminalsStore.get(id);
			if (terminal?.sessionId) {
				clearInterval(waitForSession);
				try {
					await deps.pty.write(terminal.sessionId, command + "\n");
				} catch (err) {
					appLogger.error("terminal", "Failed to send run command", err);
				}
			} else if (waitAttempts >= 20) {
				clearInterval(waitForSession);
				appLogger.warn("terminal", "Timed out waiting for session on run command");
			}
		}, 500);
	};

	const generateWorktreeName = async (): Promise<string> => {
		const state = worktreeDialogState();
		const worktreeBranches = state?.worktreeBranches ?? [];
		return deps.repo.generateWorktreeName(worktreeBranches);
	};

	const handleRepoSettings = (
		repoPath: string,
		openSettingsPanel: (context: { kind: "repo"; repoPath: string }) => void,
	) => {
		openSettingsPanel({ kind: "repo", repoPath });
	};

	// --- Branch switching ---

	const [switchBranchLists, setSwitchBranchLists] = createSignal<Record<string, string[]>>({});
	const [currentBranches, setCurrentBranches] = createSignal<Record<string, string>>({});

	/** Refresh the local branch list and current branch for all git repos (for context menu). */
	const refreshBranchLists = async (scopeRepoPath?: string) => {
		const repos = repositoriesStore
			.getOrderedRepos()
			.filter((r) => r.isGitRepo !== false)
			.filter((r) => !scopeRepoPath || r.path === scopeRepoPath);
		const results: Record<string, string[]> = {};
		const heads: Record<string, string> = {};
		await Promise.all(
			repos.map(async (r) => {
				try {
					const [branches, info] = await Promise.all([deps.repo.listLocalBranches(r.path), deps.repo.getInfo(r.path)]);
					results[r.path] = branches;
					heads[r.path] = info.branch;
				} catch (e) {
					appLogger.debug("git", "Skipping repo in branch listing", { path: r.path, error: String(e) });
				}
			}),
		);
		// Scoped refresh merges into the existing maps so the other repos'
		// dropdown lists aren't wiped; a full refresh replaces wholesale.
		if (scopeRepoPath) {
			setSwitchBranchLists((prev) => ({ ...prev, ...results }));
			setCurrentBranches((prev) => ({ ...prev, ...heads }));
		} else {
			setSwitchBranchLists(results);
			setCurrentBranches(heads);
		}
	};

	// Compose branch stats + branch list refresh into a single function
	const refreshAllBranchStatsAndLists = async (scopeRepoPath?: string, options: { immediate?: boolean } = {}) => {
		await refreshAllBranchStats(scopeRepoPath, options);
		await refreshBranchLists(scopeRepoPath);
	};

	/** After a branch switch on the main worktree, migrate all stale branch entries
	 *  (same worktreePath as repoPath) into the new branch and remove them. */
	const migrateMainWorktreeBranches = (repoPath: string, newBranch: string) => {
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return;

		// Ensure the target branch entry exists
		repositoriesStore.setWorkspace(repoPath, newBranch, { worktreePath: repoPath });

		// Find all branches on the main worktree that aren't the new branch
		const stale = Object.values(repo.workspaces).filter(
			(b) => b.worktreePath === repoPath && b.branchName !== newBranch,
		);

		batch(() => {
			for (const branch of stale) {
				repositoriesStore.mergeWorkspaceState(repoPath, branch.branchName, newBranch);
				repositoriesStore.removeWorkspace(repoPath, branch.branchName);
			}
			repositoriesStore.setActiveWorkspace(repoPath, newBranch);
		});
	};

	/** Handle branch switch request from sidebar. Checks terminal safety, then calls Rust. */
	const handleSwitchBranch = async (repoPath: string, branchName: string) => {
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return;

		const mainWorktreeBranches = Object.values(repo.workspaces).filter((b) => b.worktreePath === repoPath);
		for (const branch of mainWorktreeBranches) {
			for (const termId of branch.terminals) {
				const term = terminalsStore.get(termId);
				if (term?.shellState === "busy") {
					reportActionError(
						deps.setStatusInfo,
						`Cannot switch branch: terminal "${term.name || termId}" has a running process`,
					);
					return;
				}
			}
		}

		const isLockError = (msg: string) => /index\.lock|could not write index|another git process/i.test(msg);
		const stashAndSwitch = async () => {
			const result = await deps.repo.switchBranch(repoPath, branchName, { stash: true });
			deps.setStatusInfo(`Switched to ${result.new_branch} (changes stashed)`);
			migrateMainWorktreeBranches(repoPath, result.new_branch);
			await refreshAllBranchStatsAndLists(repoPath, { immediate: true });
		};

		try {
			const result = await deps.repo.switchBranch(repoPath, branchName);
			deps.setStatusInfo(
				result.stashed ? `Switched to ${result.new_branch} (changes stashed)` : `Switched to ${result.new_branch}`,
			);
			migrateMainWorktreeBranches(repoPath, result.new_branch);
			await refreshAllBranchStatsAndLists(repoPath, { immediate: true });
		} catch (err) {
			const errMsg = String(err);
			if (errMsg === "dirty" || errMsg.includes("dirty")) {
				const confirmed = await deps.dialogs.confirmStashAndSwitch?.(branchName);
				if (!confirmed) return;
				try {
					await stashAndSwitch();
				} catch (stashErr) {
					const msg = String(stashErr);
					appLogger.error("git", "Stash & switch failed", { repoPath, branchName, error: msg });
					const lock = isLockError(msg);
					const detail = lock
						? `A stale git lock is blocking the index:\n\n${msg}\n\nStale locks clear automatically after a short while. Retry?`
						: msg;
					const retry = await deps.dialogs.reportGitError?.("Stash & switch failed", detail, lock);
					if (!retry) return;
					try {
						await stashAndSwitch();
					} catch (retryErr) {
						const retryMsg = String(retryErr);
						appLogger.error("git", "Stash & switch retry failed", { repoPath, branchName, error: retryMsg });
						await deps.dialogs.reportGitError?.("Stash & switch failed again", retryMsg, false);
					}
				}
			} else {
				appLogger.error("git", "Branch switch failed", { repoPath, branchName, error: errMsg });
				await deps.dialogs.reportGitError?.("Branch switch failed", errMsg, false);
			}
		}
	};

	const handleCheckoutRemoteBranch = async (repoPath: string, branchName: string) => {
		try {
			await deps.repo.checkoutRemoteBranch(repoPath, branchName);
			deps.setStatusInfo(`Checked out ${branchName}`);
			migrateMainWorktreeBranches(repoPath, branchName);
			await refreshAllBranchStatsAndLists(repoPath, { immediate: true });
			await handleBranchSelectInner(repoPath, branchName);
		} catch (err) {
			appLogger.error("git", "Failed to checkout remote branch", { error: String(err) });
			reportActionError(deps.setStatusInfo, `Checkout failed: ${err}`);
		}
	};

	const { cancelCwdTracking, getWorktreeTargets, handleTerminalCwdChange, moveTerminalToWorktree } =
		createTerminalWorktreeCoordinator({
			refreshBranches: refreshAllBranchStats,
			writePty: deps.pty.write,
		});

	return {
		currentRepoPath,
		currentBranch,
		repoStatus,
		setRepoStatus,
		branchToRename,
		setBranchToRename,
		refreshAllBranchStats: refreshAllBranchStatsAndLists,
		handleBranchSelect,
		resumeAgentTabsAfterUpdate,
		handleAddTerminalToWorkspace,
		handleRemoveRepo,
		handleRemoveWorkspace,
		handleOpenRenameBranchDialog,
		handleRenameBranch,
		branchToCreate,
		setBranchToCreate,
		handleOpenCreateBranchDialog,
		handleCreateBranch,
		activeWorktreePath,
		activeRunCommand,
		handleAddRepo,
		addRepoByPath,
		handleAddRemoteRepo,
		handleAddWorktree,
		confirmCreateWorktree,
		handleCreateWorktreeFromBranch,
		handleAutofixIssue,
		handleConflictAssist,
		handleMergeAndArchive,
		handleMergeAndArchiveIntoMain,
		mergePendingCtx,
		dismissMergePending,
		closeTerminalsForBranch,
		worktreeDialogState,
		setWorktreeDialogState,
		creatingWorktreeRepos,
		removingBranches,
		handleNewTab,
		resolveNewTabTarget,
		handleRunCommand,
		executeRunCommand,
		generateWorktreeName,
		handleRepoSettings,
		handleCheckoutRemoteBranch,
		handleSwitchBranch,
		handleTerminalCwdChange,
		cancelCwdTracking,
		switchBranchLists,
		currentBranches,
		refreshBranchLists,
		getWorktreeTargets,
		moveTerminalToWorktree,
		/** Create a new terminal for the branch and queue the review command */
		handleReviewPr: async (repoPath: string, branchName: string, command: string) => {
			const termId = await handleAddTerminalToWorkspace(repoPath, branchName);
			if (termId) {
				terminalsStore.update(termId, { pendingInitCommand: command });
			}
		},
	};
}
