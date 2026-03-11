import { invoke } from "../invoke";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import { appLogger } from "../stores/appLogger";

/** Minimum allowed interval to prevent spamming (5 minutes) */
const MIN_INTERVAL_MS = 5 * 60 * 1000;

/** Tick interval for the master timer (1 minute) */
const TICK_INTERVAL_MS = 60 * 1000;

/** Track last fetch time per repo */
const lastFetchAt = new Map<string, number>();

/** Master tick timer */
let tickTimer: ReturnType<typeof setInterval> | null = null;

/** Perform a single background fetch for a repo */
/** Track consecutive failures per repo for exponential backoff */
const failCount = new Map<string, number>();

async function fetchRepo(repoPath: string): Promise<void> {
  try {
    const result = await invoke<{ success: boolean; stdout: string; stderr: string; exit_code: number }>(
      "run_git_command",
      { path: repoPath, args: ["-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=15", "fetch", "--all"] },
    );
    if (result.success) {
      repositoriesStore.bumpRevision(repoPath);
      failCount.delete(repoPath);
      appLogger.debug("git", `Auto-fetch completed for ${repoPath}`);
    } else {
      const count = (failCount.get(repoPath) ?? 0) + 1;
      failCount.set(repoPath, count);
      appLogger.warn("git", `Auto-fetch failed for ${repoPath}`, { stderr: result.stderr });
    }
  } catch (err) {
    const count = (failCount.get(repoPath) ?? 0) + 1;
    failCount.set(repoPath, count);
    appLogger.warn("git", `Auto-fetch error for ${repoPath}`, err);
  }
}

/** Master tick: check all repos and fetch those whose interval has elapsed */
function tick(): void {
  const now = Date.now();
  const repos = repositoriesStore.getOrderedRepos();

  for (const repo of repos) {
    const effective = repoSettingsStore.getEffective(repo.path);
    const intervalMin = effective?.autoFetchIntervalMinutes ?? 0;
    if (intervalMin <= 0) continue;

    const intervalMs = Math.max(intervalMin * 60 * 1000, MIN_INTERVAL_MS);
    const lastAt = lastFetchAt.get(repo.path) ?? 0;

    // Exponential backoff: double interval per consecutive failure (cap at 8x)
    const fails = failCount.get(repo.path) ?? 0;
    const backoffMultiplier = Math.min(8, Math.pow(2, fails));
    if (now - lastAt >= intervalMs * backoffMultiplier) {
      lastFetchAt.set(repo.path, now);
      fetchRepo(repo.path);
    }
  }
}

/** Start the auto-fetch master timer. Safe to call multiple times (replaces existing). */
export function startAutoFetch(): void {
  stopAutoFetch();
  tickTimer = setInterval(tick, TICK_INTERVAL_MS);
}

/** Stop the auto-fetch master timer and clear all tracking state. */
export function stopAutoFetch(): void {
  if (tickTimer != null) {
    clearInterval(tickTimer);
    tickTimer = null;
  }
  lastFetchAt.clear();
  failCount.clear();
}
