import { onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { rpc } from "../transport";
import { githubStore } from "../stores/github";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";

const MAX_ATTEMPTS = 3;

/**
 * Auto-heal CI failures by injecting failure logs into an agent terminal.
 *
 * When CI checks fail on a branch that has auto-heal enabled and an active
 * agent terminal, this hook:
 * 1. Fetches the failure logs via `gh run view --log-failed`
 * 2. Waits for the agent to be idle/awaiting input
 * 3. Writes the logs + fix prompt into the terminal
 * 4. Repeats up to MAX_ATTEMPTS times, then stops and notifies
 */
export function useCiHeal(): void {
  /** Track in-flight heals to prevent re-entry */
  const healing = new Set<string>();

  function handleCiFailed(repoPath: string, branch: string, _prNumber: number): void {
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
      return;
    }

    healing.add(key);
    triggerHeal(repoPath, branch, agentTerminal).finally(() => healing.delete(key));
  }

  async function triggerHeal(repoPath: string, branch: string, terminalId: string): Promise<void> {
    const branchState = repositoriesStore.state.repositories[repoPath]?.branches[branch];
    if (!branchState?.ciAutoHeal) return;

    const attempt = (branchState.ciAutoHeal.attempts ?? 0) + 1;
    appLogger.info("ci-heal", `Auto-heal attempt ${attempt}/${MAX_ATTEMPTS} for ${branch}`);

    // Mark as healing and increment attempts
    repositoriesStore.setCiAutoHeal(repoPath, branch, {
      ...branchState.ciAutoHeal,
      attempts: attempt,
      healing: true,
    });

    try {
      // Fetch failure logs
      const logs = await invoke<string>("fetch_ci_failure_logs", {
        repoPath,
        branch,
      });

      // Wait for agent to be ready for input
      const terminal = terminalsStore.get(terminalId);
      if (!terminal?.sessionId) {
        appLogger.warn("ci-heal", `Terminal ${terminalId} has no session, aborting heal`);
        return;
      }

      await waitForAgentIdle(terminalId, 30_000);

      // Inject the failure logs and fix prompt into the terminal
      const prompt = [
        "",
        "",
        "CI checks failed. Here are the failure logs:",
        "",
        logs,
        "",
        "Please fix the issues and push again.",
        "",
      ].join("\n");

      await rpc("write_pty", { sessionId: terminal.sessionId, data: prompt });
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
  onCleanup(() => {
    githubStore.setOnCiFailed(null);
    githubStore.setOnCiRecovered(null);
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
