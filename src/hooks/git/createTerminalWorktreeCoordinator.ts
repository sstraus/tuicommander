import { batch } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { placementWorkspaceFor, repositoriesStore, resolveRepoOwner } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { pathStartsWith } from "../../utils/pathUtils";

interface TerminalWorktreeCoordinatorDeps {
	refreshBranches: () => Promise<void>;
	writePty: (sessionId: string, data: string) => Promise<void>;
}

/** Owns OSC 7 CWD reassignment and explicit terminal-to-worktree moves. */
export function createTerminalWorktreeCoordinator(deps: TerminalWorktreeCoordinatorDeps) {
	const cwdDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();

	/** Find the repo and WORKSPACE that own a CWD.
	 *
	 *  A linked worktree directory names its workspace. A match at the repo root
	 *  does not — what is checked out there changes under the user's feet — so the
	 *  root resolves at the moment we need it, through `placementWorkspaceFor`. */
	const findWorkspaceForCwd = (cwd: string): { repoPath: string; workspaceId: string } | null => {
		const owner = resolveRepoOwner(cwd);
		if (!owner) return null;
		const workspaceId = placementWorkspaceFor(owner);
		return workspaceId ? { repoPath: owner.repoPath, workspaceId } : null;
	};

	const performCwdReassignment = async (terminalId: string, newCwd: string) => {
		if (!terminalsStore.get(terminalId)) return;

		const currentRepoPath = repositoriesStore.getRepoPathForTerminal(terminalId);
		const currentWorkspaceId = repositoriesStore.findOwnerForTerminal(terminalId)?.workspaceId ?? null;

		// Explicit backend placement remains authoritative even when git -C leaves
		// the shell cwd unchanged or an OSC 7 notification arrives later.
		const placementCwd = terminalsStore.get(terminalId)?.placementPath ?? newCwd;
		let target = findWorkspaceForCwd(placementCwd);
		if (!target && currentRepoPath) {
			const insideKnownRepo = repositoriesStore.getPaths().some((repoPath) => pathStartsWith(placementCwd, repoPath));
			if (insideKnownRepo) {
				await deps.refreshBranches();
				target = findWorkspaceForCwd(placementCwd);
			}
		}

		if (!target) return;
		if (target.repoPath === currentRepoPath && target.workspaceId === currentWorkspaceId) return;

		// A cd across repos is navigation, not a placement. The tab belongs to the repo
		// it was opened in — `repoPath` records that owner — and this path may only move
		// it *within* that repo, which is what a worktree switch is. Re-homing an owned
		// tab across repos is the regression Boss reported as "the app changes repo on
		// its own": the tab left the strip, and the branch below even switched the
		// sidebar to the other repo. A tab still parked (`repoPath === null`) has no
		// owner to respect, so a cd may still settle it.
		const owner = terminalsStore.get(terminalId)?.repoPath ?? null;
		if (owner !== null && target.repoPath !== owner) return;

		appLogger.info("terminal", `[CwdChange] ${terminalId} → ${target.repoPath}:${target.workspaceId} (cwd=${newCwd})`);
		batch(() => {
			if (currentRepoPath && currentWorkspaceId) {
				repositoriesStore.removeTerminalFromWorkspace(currentRepoPath, currentWorkspaceId, terminalId);
			}
			// The workspace arrays are the display index; the terminal's own repoPath
			// is the record. Moving one without the other is what left ids stranded.
			terminalsStore.setRepoPath(terminalId, target.repoPath);
			repositoriesStore.addTerminalToWorkspace(target.repoPath, target.workspaceId, terminalId);

			if (terminalsStore.state.activeId === terminalId) {
				repositoriesStore.setActiveWorkspace(target.repoPath, target.workspaceId);
				if (target.repoPath !== currentRepoPath) {
					repositoriesStore.setActive(target.repoPath);
				}
			}
		});
	};

	const handleTerminalCwdChange = (terminalId: string, newCwd: string) => {
		clearTimeout(cwdDebounceTimers.get(terminalId));
		cwdDebounceTimers.set(
			terminalId,
			setTimeout(() => {
				cwdDebounceTimers.delete(terminalId);
				void performCwdReassignment(terminalId, newCwd).catch((error) =>
					appLogger.warn("terminal", `[CwdChange] reassignment error for ${terminalId}`, error),
				);
			}, 300),
		);
	};

	const cancelCwdTracking = (terminalId: string) => {
		const timer = cwdDebounceTimers.get(terminalId);
		if (timer !== undefined) {
			clearTimeout(timer);
			cwdDebounceTimers.delete(terminalId);
		}
	};

	/** Where this terminal could move to: one entry per OTHER workspace of its repo
	 *  that has a directory. `workspaceId` identifies it, `branchName` labels it —
	 *  two same-branch workspaces are two targets, distinguishable only by id. */
	const getWorktreeTargets = (terminalId: string): Array<{ workspaceId: string; branchName: string; path: string }> => {
		const repoPath = repositoriesStore.getRepoPathForTerminal(terminalId);
		if (!repoPath) return [];
		const repo = repositoriesStore.get(repoPath);
		if (!repo) return [];

		const currentWorkspaceId = repositoriesStore.findOwnerForTerminal(terminalId)?.workspaceId ?? null;

		const targets: Array<{ workspaceId: string; branchName: string; path: string }> = [];
		for (const [workspaceId, workspace] of Object.entries(repo.workspaces)) {
			if (workspaceId === currentWorkspaceId) continue;
			const worktreePath = workspace.worktreePath ?? (workspace.isMain ? repoPath : null);
			if (worktreePath) targets.push({ workspaceId, branchName: workspace.branchName, path: worktreePath });
		}
		return targets;
	};

	const moveTerminalToWorktree = async (terminalId: string, worktreePath: string): Promise<void> => {
		const terminal = terminalsStore.get(terminalId);
		if (!terminal?.sessionId) return;
		const escapedPath = `'${worktreePath.replaceAll("'", "'\\''")}'`;
		await deps.writePty(terminal.sessionId, `cd ${escapedPath}\n`);
		appLogger.info("terminal", `[MoveToWorktree] ${terminalId} → cd ${worktreePath}`);
	};

	return {
		cancelCwdTracking,
		findWorkspaceForCwd,
		getWorktreeTargets,
		handleTerminalCwdChange,
		moveTerminalToWorktree,
	};
}
