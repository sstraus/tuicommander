import { batch, onCleanup } from "solid-js";
import { invoke, listen } from "../invoke";
import { activityStore } from "../stores/activityStore";
import { appLogger } from "../stores/appLogger";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { sendCommand } from "../utils/sendCommand";
import type { ConfirmOptions } from "./useConfirmDialog";

interface WorktreeSwitchDeps {
	confirm: (options: ConfirmOptions) => Promise<boolean>;
	handleBranchSelect: (repoPath: string, branchName: string) => Promise<void>;
}

interface WorktreeCreatedPayload {
	repo_path: string;
	branch: string;
	worktree_path: string;
}

/**
 * Listens for worktree-created events (from MCP) and offers to switch
 * the active tab + terminal to the new worktree.
 */
export function useWorktreeSwitchPrompt(deps: WorktreeSwitchDeps): void {
	let unlisten: (() => void) | null = null;

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
		handlePrompt(repo_path, branch, worktree_path).catch((err) =>
			appLogger.warn("git", "Worktree switch prompt failed", err),
		);
	})
		.then((fn) => {
			unlisten = fn;
		})
		.catch((err) => appLogger.error("app", "Failed to register worktree-created listener", err));

	onCleanup(() => unlisten?.());

	async function handlePrompt(repoPath: string, branch: string, worktreePath: string): Promise<void> {
		const activeTerm = terminalsStore.getActive();
		const isAgentRunning = activeTerm?.agentType != null;

		const message = isAgentRunning
			? `Worktree "${branch}" was created.\nAn agent is running — it cannot switch directories mid-session.\nMove this terminal to the worktree branch and notify the agent?`
			: `Worktree "${branch}" was created.\nSwitch to it now?`;

		const confirmed = await deps.confirm({
			title: "Switch to new worktree?",
			message,
			okLabel: isAgentRunning ? "Move & Notify" : "Switch",
			cancelLabel: "Stay",
			kind: "info",
			autoCancelMs: 10_000,
		});
		if (!confirmed) return;

		// Switch the sidebar tab/branch
		await deps.handleBranchSelect(repoPath, branch);

		// Move the active terminal to the new branch in the store
		if (activeTerm?.sessionId) {
			const terminalId = activeTerm.id;
			const currentMapping = repositoriesStore.findOwnerForTerminal(terminalId);

			batch(() => {
				if (currentMapping) {
					repositoriesStore.removeTerminalFromBranch(currentMapping.repoPath, currentMapping.branchName, terminalId);
				}
				repositoriesStore.addTerminalToBranch(repoPath, branch, terminalId);
			});

			if (isAgentRunning) {
				const writeFn = async (data: string): Promise<void> => {
					await invoke("write_pty", { sessionId: activeTerm.sessionId, data });
				};
				await sendCommand(
					writeFn,
					`A worktree for branch "${branch}" was created at ${worktreePath}. ` +
						`You cannot switch directories mid-session. ` +
						`Stop what you are doing and instruct the user to start a new session in the worktree. ` +
						`The user should open a new terminal in the "${branch}" branch tab in the sidebar.`,
					activeTerm.agentType,
				);
				appLogger.info("terminal", `[WorktreeSwitch] ${terminalId} → ${branch} (notified agent)`);
			} else {
				await invoke("write_pty", {
					sessionId: activeTerm.sessionId,
					data: `cd ${shellEscape(worktreePath)}\n`,
				});
				appLogger.info("terminal", `[WorktreeSwitch] ${terminalId} → ${branch}`);
			}
		}
	}
}

/** Minimal shell escaping — wrap in single quotes, escape existing quotes */
function shellEscape(path: string): string {
	if (!/[^a-zA-Z0-9_./-]/.test(path)) return path;
	return `'${path.replace(/'/g, "'\\''")}'`;
}
