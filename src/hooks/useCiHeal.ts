import { onCleanup } from "solid-js";
import { t } from "../i18n";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { githubStore } from "../stores/github";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { toastsStore } from "../stores/toasts";
import { rpc } from "../transport";
import { getShellFamily, sendCommand } from "../utils/sendCommand";

const MAX_ATTEMPTS = 3;

/** What blocked the PR — drives which fix prompt the agent receives. */
type HealKind = "ci" | "conflict";

/**
 * Auto-heal a blocked PR by handing the problem to an agent terminal.
 *
 * Triggers on two transitions when auto-heal is enabled on the branch:
 * - CI failure (`ci`): fetches `gh run view --log-failed` and asks the agent to fix.
 * - Merge conflict (`conflict`): asks the agent to resolve conflicts with the base branch.
 *
 * In both cases it waits for the agent to be idle/awaiting input, writes the fix
 * prompt into the terminal, and repeats up to MAX_ATTEMPTS times before stopping.
 */
export function useCiHeal(): void {
	/** Track in-flight heals to prevent re-entry */
	const healing = new Set<string>();

	function handleCiFailed(repoPath: string, branch: string, _prNumber: number): void {
		startHeal(repoPath, branch, "ci");
	}

	function handleConflict(repoPath: string, branch: string, _prNumber: number): void {
		startHeal(repoPath, branch, "conflict");
	}

	/** Shared gate: enabled check, attempt budget, agent-terminal lookup, re-entry guard. */
	function startHeal(repoPath: string, branch: string, kind: HealKind): void {
		const key = `${repoPath}:${branch}`;
		if (healing.has(key)) return;

		// Check if auto-heal is enabled for this branch
		const repo = repositoriesStore.state.repositories[repoPath];
		if (!repo) return;
		const branchState = repo.branches[branch];
		if (!branchState?.ciAutoHeal?.enabled) return;

		// Check attempt count
		if ((branchState.ciAutoHeal.attempts ?? 0) >= MAX_ATTEMPTS) {
			appLogger.warn("ci-heal", `Auto-heal exhausted after ${MAX_ATTEMPTS} attempts for ${branch}`);
			repositoriesStore.setCiAutoHeal(repoPath, branch, {
				...branchState.ciAutoHeal,
				healing: false,
			});
			return;
		}

		// Find an agent terminal on this branch
		const agentTerminal = findAgentTerminal(repoPath, branch);
		if (!agentTerminal) {
			appLogger.debug("ci-heal", `No agent terminal found for ${branch}, skipping auto-heal`);
			// Surface why nothing happened — auto-heal injects the fix request into an
			// agent terminal, which doesn't exist on this branch.
			toastsStore.add(
				t("ciHeal.noAgentTitle", "Auto-heal: no agent"),
				`Open an AI agent on "${branch}" so auto-heal can hand it the ${
					kind === "conflict" ? "merge conflicts" : "CI failures"
				}.`,
				"warn",
			);
			return;
		}

		healing.add(key);
		triggerHeal(repoPath, branch, agentTerminal, kind).finally(() => healing.delete(key));
	}

	/** Build the fix prompt for a heal kind. CI fetches failure logs; conflict is self-describing. */
	async function buildHealPrompt(repoPath: string, branch: string, kind: HealKind): Promise<string> {
		const body =
			kind === "conflict"
				? "This PR is blocked by merge conflicts with its base branch.\n\nPlease resolve the merge conflicts: integrate the latest base branch, resolve each conflicting file preserving the intent from both sides, then commit and push."
				: `CI checks failed. Here are the failure logs:\n\n${await invoke<string>("fetch_ci_failure_logs", {
						repoPath,
						branch,
					})}\n\nPlease fix the issues and push again.`;
		return `\n\n${body}\n\n`;
	}

	async function triggerHeal(repoPath: string, branch: string, terminalId: string, kind: HealKind): Promise<void> {
		const branchState = repositoriesStore.state.repositories[repoPath]?.branches[branch];
		if (!branchState?.ciAutoHeal) return;

		const attempt = (branchState.ciAutoHeal.attempts ?? 0) + 1;
		appLogger.info("ci-heal", `Auto-heal (${kind}) attempt ${attempt}/${MAX_ATTEMPTS} for ${branch}`);

		// Mark as healing and increment attempts
		repositoriesStore.setCiAutoHeal(repoPath, branch, {
			...branchState.ciAutoHeal,
			attempts: attempt,
			healing: true,
		});

		try {
			const prompt = await buildHealPrompt(repoPath, branch, kind);

			// Wait for agent to be ready for input
			const terminal = terminalsStore.get(terminalId);
			if (!terminal?.sessionId) {
				appLogger.warn("ci-heal", `Terminal ${terminalId} has no session, aborting heal`);
				return;
			}

			await waitForAgentIdle(terminalId, 30_000);

			// PTY injection rule: route through sendCommand so the prompt actually
			// submits (Ink raw-mode agents ignore a trailing \n; only the split
			// Ctrl-U/\r writes submit). A raw write_pty here silently failed to
			// submit for Claude/Gemini.
			const shellFamily = await getShellFamily(terminal.sessionId);
			await sendCommand(
				(data) => rpc("write_pty", { sessionId: terminal.sessionId, data }),
				prompt.trimEnd(),
				terminal.agentType,
				shellFamily,
			);
		} catch (err) {
			appLogger.error("ci-heal", `Auto-heal failed for ${branch}`, err);
		} finally {
			// Clear healing flag (keep attempts)
			const current = repositoriesStore.state.repositories[repoPath]?.branches[branch]?.ciAutoHeal;
			if (current) {
				repositoriesStore.setCiAutoHeal(repoPath, branch, {
					...current,
					healing: false,
				});
			}
		}
	}

	function handleCiRecovered(repoPath: string, branch: string, _prNumber: number): void {
		const branchState = repositoriesStore.state.repositories[repoPath]?.branches[branch];
		if (!branchState?.ciAutoHeal?.enabled) return;
		if ((branchState.ciAutoHeal.attempts ?? 0) === 0) return;

		const attempts = branchState.ciAutoHeal.attempts ?? 0;
		appLogger.info("ci-heal", `CI healed after ${attempts} attempt(s) for ${branch}`);

		// Reset attempts but keep enabled
		repositoriesStore.setCiAutoHeal(repoPath, branch, {
			enabled: true,
			attempts: 0,
			healing: false,
		});
	}

	githubStore.setOnCiFailed(handleCiFailed);
	githubStore.setOnCiRecovered(handleCiRecovered);
	githubStore.setOnConflict(handleConflict);
	onCleanup(() => {
		githubStore.setOnCiFailed(null);
		githubStore.setOnCiRecovered(null);
		githubStore.setOnConflict(null);
	});
}

/** Find an agent terminal assigned to the given branch */
function findAgentTerminal(repoPath: string, branch: string): string | null {
	const repo = repositoriesStore.state.repositories[repoPath];
	if (!repo) return null;
	const branchState = repo.branches[branch];
	if (!branchState) return null;

	for (const termId of branchState.terminals) {
		const terminal = terminalsStore.get(termId);
		if (terminal?.agentType) {
			return termId;
		}
	}
	return null;
}

/** Wait for a terminal's agent to be idle or awaiting input, with timeout */
function waitForAgentIdle(terminalId: string, timeoutMs: number): Promise<void> {
	return new Promise((resolve, reject) => {
		const terminal = terminalsStore.get(terminalId);
		// If already idle or awaiting, resolve immediately
		if (terminal?.shellState === "idle" || terminal?.awaitingInput) {
			resolve();
			return;
		}

		const deadline = Date.now() + timeoutMs;
		const interval = setInterval(() => {
			const t = terminalsStore.get(terminalId);
			if (!t) {
				clearInterval(interval);
				reject(new Error("Terminal no longer exists"));
				return;
			}
			if (t.shellState === "idle" || t.awaitingInput) {
				clearInterval(interval);
				resolve();
				return;
			}
			if (Date.now() > deadline) {
				clearInterval(interval);
				reject(new Error("Timeout waiting for agent idle"));
			}
		}, 500);
	});
}
