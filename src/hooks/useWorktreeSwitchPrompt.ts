import { onCleanup } from "solid-js";
import { invoke, listen } from "../invoke";
import { activityStore } from "../stores/activityStore";
import { appLogger } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";
import { reconcileTerminalOwnership, removeWorktreeWorkspace } from "../stores/terminalOwnership";
import { terminalsStore } from "../stores/terminals";
import { workItemLinksStore } from "../stores/workItemLinks";
import { pathBasename, pathDirname } from "../utils/pathUtils";

interface WorktreeSwitchDeps {
	handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
	closeTerminalsForBranch: (repoPath: string, branchName: string) => Promise<void>;
}

interface WorktreeCreatedPayload {
	__tuic_origin?: unknown;
	creator_session?: string | null;
	spawn_session?: boolean;
	repo_path: string;
	/** Which workspace was born — the store key. */
	workspace_id: string;
	/** Display only. */
	branch: string;
	worktree_path: string;
	kind: "worktree";
}

interface WorktreeRemovedPayload {
	__tuic_origin?: unknown;
	repo_path: string;
	/** Which workspace died. Not a branch: two workspaces may share one, so a
	 *  branch cannot name the row to drop (#726-5ac7). */
	workspace_id: string;
	/** Display only — captured before the checkout went away, because nothing can
	 *  resolve the id afterwards. */
	branch: string;
}

/**
 * Move to a worktree the backend just created, without pretending a running
 * agent can move with its terminal. Branch selection opens or focuses a terminal
 * rooted in the worktree; the agent terminal remains attached to its original
 * branch and CWD.
 *
 * Called from the bell item's action, so whether the active terminal can
 * follow is decided HERE, at click time — by then the user may have moved to a
 * different tab than the one that was active when the worktree appeared.
 */
export async function switchToCreatedWorktree(
	deps: WorktreeSwitchDeps,
	repoPath: string,
	_workspaceId: string,
	branch: string,
	worktreePath: string,
): Promise<void> {
	const activeTerm = terminalsStore.getActive();
	const isAgentRunning = activeTerm?.agentType != null;

	// This selects an existing worktree terminal (for example one spawned by
	// MCP) or creates the first terminal for the branch when none exists.
	await deps.handleBranchSelect(repoPath, branch);

	if (isAgentRunning) {
		appLogger.info("terminal", `[WorktreeSwitch] ${activeTerm.id} stayed on its current branch; opened ${branch}`);
		return;
	}

	// Request the shell move, but keep its current placement until OSC 7 confirms cwd.
	// A failed cd must not file a repo-root tab under a removable linked checkout.
	if (activeTerm?.sessionId) {
		const terminalId = activeTerm.id;
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
	workspaceId: string,
	closeTerminalsForBranch: (repoPath: string, workspaceId: string) => Promise<void>,
): Promise<void> {
	const workspace = repositoriesStore.get(repoPath)?.workspaces[workspaceId];
	if (!workspace) return;
	if (workspace.worktreePath === repoPath) return;

	if (workspace.terminals.length > 0) {
		await closeTerminalsForBranch(repoPath, workspaceId);
	}
	removeWorktreeWorkspace(repoPath, workspaceId);
	appLogger.info("git", `Worktree removed — pruned sidebar row "${workspaceId}"`, { repoPath });
}

/** `repo__wt/feature` — the last two segments, enough to tell two worktrees of
 *  different repos apart without spending a toast line on the whole path. */
function worktreeLabel(worktreePath: string): string {
	const parent = pathBasename(pathDirname(worktreePath));
	const leaf = pathBasename(worktreePath);
	return parent ? `${parent}/${leaf}` : leaf;
}

/** Render backend-authored workspace membership without sending shell input. */
function applyWorktreePlacement(payload: WorktreeCreatedPayload, declared = false): void {
	const { repo_path, workspace_id, branch, worktree_path, kind } = payload;
	// Register the branch in the store immediately so the sidebar shows the new
	// worktree right away — independent of whether the user accepts the switch
	// offer below. Mirrors the in-app create path (setupNewWorktree → setWorkspace).
	// Guarded on repo existence so we don't create a half-formed repo entry for a
	// worktree on a repo that isn't open in the sidebar.
	if (repositoriesStore.get(repo_path)) {
		// Same record `setupNewWorktree` writes for an in-app creation.
		repositoriesStore.setWorkspace(repo_path, workspace_id, {
			branchName: branch,
			worktreePath: worktree_path,
			kind,
			parentRepoPath: null,
		});
	}
	// Creating a checkout does not change the caller's cwd (agents often use git -C).
	// Creation stays cwd-based; an explicit caller-bound declaration owns display placement.
	const creatorId = payload.creator_session ? terminalsStore.findBySessionId(payload.creator_session) : undefined;
	if (
		creatorId &&
		repositoriesStore.findOwnerForTerminal(creatorId)?.repoPath === repo_path &&
		!payload.spawn_session
	) {
		const peer = terminalsStore.get(creatorId)?.tuicSession;
		if (declared && peer) {
			repositoriesStore.setDeclaredWorktree(repo_path, peer, {
				workspaceId: workspace_id,
				branch,
				worktreePath: worktree_path,
			});
		}
		reconcileTerminalOwnership(creatorId);
		if (declared && peer && terminalsStore.state.activeId === creatorId) {
			repositoriesStore.setActiveWorkspace(repo_path, workspace_id);
		}
	}
}

/**
 * Listens for backend worktree lifecycle events: offers to switch the active tab
 * + terminal to a newly created worktree, and prunes the row of a removed one.
 *
 * Only backend-initiated creations reach here — `worktree-created` is emitted by
 * the MCP `repo worktree_create` tool and the HTTP worktree route, never by the
 * in-app "+" button, which switches directly. The offer used to be a blocking
 * modal with a ten-second auto-cancel, which is exactly backwards for the only
 * case it fires in: an orchestrator creating a worktree every few minutes asked a
 * question nobody was at the keyboard to answer, and stole the screen until each
 * one timed out. The bell keeps the offer reachable without covering the input.
 */
export function useWorktreeSwitchPrompt(deps: WorktreeSwitchDeps): void {
	let unlisten: (() => void) | null = null;
	let unlistenRemoved: (() => void) | null = null;
	let unlistenDeclared: (() => void) | null = null;

	listen<WorktreeCreatedPayload>("worktree-created", (event) => {
		if (event.payload.__tuic_origin !== undefined) return;
		const { repo_path, workspace_id, branch, worktree_path } = event.payload;
		const switchToWorktree = () => {
			switchToCreatedWorktree(deps, repo_path, workspace_id, branch, worktree_path).catch((err) =>
				appLogger.warn("git", `Failed to switch to worktree "${branch}"`, err),
			);
		};
		applyWorktreePlacement(event.payload);
		const label = worktreeLabel(worktree_path);
		activityStore.addItem({
			id: `wt-${workspace_id}-${Date.now()}`,
			pluginId: "core",
			sectionId: "worktrees",
			title: `Worktree: ${branch}`,
			subtitle: label,
			icon: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M5 3.25a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0zm0 2.122a2.25 2.25 0 1 0-1.5 0v.878A2.25 2.25 0 0 0 5.75 8.5h1.5v2.128a2.251 2.251 0 1 0 1.5 0V8.5h1.5a2.25 2.25 0 0 0 2.25-2.25v-.878a2.25 2.25 0 1 0-1.5 0v.878a.75.75 0 0 1-.75.75h-5a.75.75 0 0 1-.75-.75v-.878zM8 12.25a.75.75 0 1 1 0 1.5.75.75 0 0 1 0-1.5zm3.25-9a.75.75 0 1 1 0 1.5.75.75 0 0 1 0-1.5z"/></svg>',
			repoPath: repo_path,
			dismissible: true,
			// The bell keeps the offer reachable after an
			// unattended run finishes and the user comes back to the machine.
			onClick: switchToWorktree,
		});
	})
		.then((fn) => {
			unlisten = fn;
		})
		.catch((err) => appLogger.error("app", "Failed to register worktree-created listener", err));

	listen<WorktreeCreatedPayload>("session-worktree-declared", (event) => {
		if (event.payload.__tuic_origin !== undefined) return;
		applyWorktreePlacement(event.payload, true);
	})
		.then((fn) => {
			unlistenDeclared = fn;
		})
		.catch((err) => appLogger.error("app", "Failed to register session-worktree-declared listener", err));

	listen<WorktreeRemovedPayload>("worktree-removed", (event) => {
		if (event.payload.__tuic_origin !== undefined) return;
		const { repo_path, workspace_id, branch } = event.payload;
		// Remove live and persisted offers: their action captured the removed checkout.
		for (const item of [...activityStore.state.items]) {
			if (
				item.pluginId === "core" &&
				item.sectionId === "worktrees" &&
				item.repoPath === repo_path &&
				item.id.startsWith(`wt-${workspace_id}-`) &&
				/^\d+$/.test(item.id.slice(`wt-${workspace_id}-`.length))
			) {
				activityStore.removeItem(item.id);
			}
		}
		workItemLinksStore.forget(repo_path, branch);
		pruneRemovedWorktree(repo_path, workspace_id, deps.closeTerminalsForBranch).catch((err) =>
			appLogger.warn("git", `Failed to prune removed worktree "${workspace_id}"`, err),
		);
	})
		.then((fn) => {
			unlistenRemoved = fn;
		})
		.catch((err) => appLogger.error("app", "Failed to register worktree-removed listener", err));

	onCleanup(() => {
		unlisten?.();
		unlistenDeclared?.();
		unlistenRemoved?.();
	});
}

/** Minimal shell escaping — wrap in single quotes, escape existing quotes */
function shellEscape(path: string): string {
	if (!/[^a-zA-Z0-9_./-]/.test(path)) return path;
	return `'${path.replaceAll("'", "'\\''")}'`;
}
