import { batch, onCleanup } from "solid-js";
import { invoke, listen } from "../invoke";
import { activityStore } from "../stores/activityStore";
import { appLogger } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import type { ConfirmOptions } from "./useConfirmDialog";

interface WorktreeSwitchDeps {
	confirm: (options: ConfirmOptions) => Promise<boolean>;
	handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
	closeTerminalsForBranch: (repoPath: string, branchName: string) => Promise<void>;
	/** Returns the effective promptOnWorktreeSwitch setting for the given repo.
	 *  When false, a created worktree is registered in the sidebar without asking.
	 *  Defaults to true (ask) when not provided. */
	getPromptOnWorktreeSwitch?: (repoPath: string) => boolean;
}

interface WorktreeCreatedPayload {
	repo_path: string;
	branch: string;
	worktree_path: string;
}

interface WorktreeRemovedPayload {
	repo_path: string;
	branch: string;
}

/**
 * Offer the newly created worktree without pretending a running agent can move
 * with its terminal. Branch selection opens or focuses a terminal rooted in the
 * worktree; the agent terminal remains attached to its original branch and CWD.
 *
 * Only backend-initiated creations reach here (MCP, HTTP), so an orchestrating
 * agent creating a worktree every few minutes asks a question nobody is at the
 * keyboard to answer. `promptOnWorktreeSwitch` turns the question off; the
 * sidebar row and activity entry are added by the caller either way.
 */
export async function promptForCreatedWorktree(
	deps: WorktreeSwitchDeps,
	repoPath: string,
	branch: string,
	worktreePath: string,
): Promise<void> {
	if (deps.getPromptOnWorktreeSwitch?.(repoPath) === false) return;

	const activeTerm = terminalsStore.getActive();
	const isAgentRunning = activeTerm?.agentType != null;

	const message = isAgentRunning
		? `Worktree "${branch}" was created.\nThe running agent cannot switch directories mid-session.\nOpen the worktree in its own terminal and keep the agent in its current branch?`
		: `Worktree "${branch}" was created.\nSwitch to it now?`;

	const confirmed = await deps.confirm({
		title: "Switch to new worktree?",
		message,
		okLabel: isAgentRunning ? "Open Worktree" : "Switch",
		cancelLabel: "Stay",
		kind: "info",
		autoCancelMs: 10_000,
	});
	if (!confirmed) return;

	// This selects an existing worktree terminal (for example one spawned by
	// MCP) or creates the first terminal for the branch when none exists.
	await deps.handleBranchSelect(repoPath, branch);

	if (isAgentRunning) {
		appLogger.info("terminal", `[WorktreeSwitch] ${activeTerm.id} stayed on its current branch; opened ${branch}`);
		return;
	}

	// Plain shells can safely move: reassign the tab and then change its CWD.
	if (activeTerm?.sessionId) {
		const terminalId = activeTerm.id;
		const currentMapping = repositoriesStore.findOwnerForTerminal(terminalId);

		batch(() => {
			if (currentMapping) {
				repositoriesStore.removeTerminalFromBranch(currentMapping.repoPath, currentMapping.branchName, terminalId);
			}
			repositoriesStore.addTerminalToBranch(repoPath, branch, terminalId);
		});

		await invoke("write_pty", {
			sessionId: activeTerm.sessionId,
			data: `cd ${shellEscape(worktreePath)}\n`,
		});
		appLogger.info("terminal", `[WorktreeSwitch] ${terminalId} → ${branch}`);
	}
}

/**
 * Drop the sidebar row for a worktree the backend just removed.
 *
 * Backend-initiated removals (MCP `repo worktree_remove`, the HTTP route,
 * merge&archive) have no other way to reach the store: the repo-watcher used to
 * swallow worktree-only changes, and a row with a live terminal is deliberately
 * kept by the refresh prune — which is how deleted worktrees stayed in the
 * sidebar and even acquired fresh terminals in their missing directory.
 *
 * Idempotent, so the UI removal path (which prunes the row itself) can also emit.
 * The main checkout is never removed: its `worktreePath` IS the repo root, and a
 * branch delete there only means HEAD moved.
 */
export async function pruneRemovedWorktree(
	repoPath: string,
	branchName: string,
	closeTerminalsForBranch: (repoPath: string, branchName: string) => Promise<void>,
): Promise<void> {
	const branch = repositoriesStore.get(repoPath)?.branches[branchName];
	if (!branch) return;
	if (branch.worktreePath === repoPath) return;

	if (branch.terminals.length > 0) {
		await closeTerminalsForBranch(repoPath, branchName);
	}
	repositoriesStore.removeBranch(repoPath, branchName);
	appLogger.info("git", `Worktree removed — pruned sidebar row "${branchName}"`, { repoPath });
}

/**
 * Listens for backend worktree lifecycle events: offers to switch the active tab
 * + terminal to a newly created worktree, and prunes the row of a removed one.
 */
export function useWorktreeSwitchPrompt(deps: WorktreeSwitchDeps): void {
	let unlisten: (() => void) | null = null;
	let unlistenRemoved: (() => void) | null = null;

	listen<WorktreeCreatedPayload>("worktree-created", (event) => {
		const { repo_path, branch, worktree_path } = event.payload;
		// Register the branch in the store immediately so the sidebar shows the new
		// worktree right away — independent of whether the user accepts the switch
		// prompt below. Mirrors the in-app create path (setupNewWorktree → setBranch).
		// Guarded on repo existence so we don't create a half-formed repo entry for a
		// worktree on a repo that isn't open in the sidebar.
		if (repositoriesStore.get(repo_path)) {
			repositoriesStore.setBranch(repo_path, branch, { worktreePath: worktree_path });
		}
		activityStore.addItem({
			id: `wt-${branch}-${Date.now()}`,
			pluginId: "core",
			sectionId: "worktrees",
			title: `Worktree: ${branch}`,
			subtitle: worktree_path.split("/").slice(-2).join("/"),
			icon: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M5 3.25a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0zm0 2.122a2.25 2.25 0 1 0-1.5 0v.878A2.25 2.25 0 0 0 5.75 8.5h1.5v2.128a2.251 2.251 0 1 0 1.5 0V8.5h1.5a2.25 2.25 0 0 0 2.25-2.25v-.878a2.25 2.25 0 1 0-1.5 0v.878a.75.75 0 0 1-.75.75h-5a.75.75 0 0 1-.75-.75v-.878zM8 12.25a.75.75 0 1 1 0 1.5.75.75 0 0 1 0-1.5zm3.25-9a.75.75 0 1 1 0 1.5.75.75 0 0 1 0-1.5z"/></svg>',
			repoPath: repo_path,
			dismissible: true,
		});
		promptForCreatedWorktree(deps, repo_path, branch, worktree_path).catch((err) =>
			appLogger.warn("git", "Worktree switch prompt failed", err),
		);
	})
		.then((fn) => {
			unlisten = fn;
		})
		.catch((err) => appLogger.error("app", "Failed to register worktree-created listener", err));

	listen<WorktreeRemovedPayload>("worktree-removed", (event) => {
		const { repo_path, branch } = event.payload;
		pruneRemovedWorktree(repo_path, branch, deps.closeTerminalsForBranch).catch((err) =>
			appLogger.warn("git", `Failed to prune removed worktree "${branch}"`, err),
		);
	})
		.then((fn) => {
			unlistenRemoved = fn;
		})
		.catch((err) => appLogger.error("app", "Failed to register worktree-removed listener", err));

	onCleanup(() => {
		unlisten?.();
		unlistenRemoved?.();
	});
}

/** Minimal shell escaping — wrap in single quotes, escape existing quotes */
function shellEscape(path: string): string {
	if (!/[^a-zA-Z0-9_./-]/.test(path)) return path;
	return `'${path.replace(/'/g, "'\\''")}'`;
}
