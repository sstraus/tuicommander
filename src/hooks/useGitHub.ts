import { createSignal, createEffect, onCleanup, onMount } from "solid-js";
import { invoke } from "../invoke";
import type { GitHubStatus } from "../types";

const BASE_INTERVAL = 30000; // 30 seconds
const MAX_INTERVAL = 300000; // 5 minutes (backoff cap)
const HIDDEN_INTERVAL = 120000; // 2 minutes when tab not visible

/** GitHub status hook with optimized polling (Story 062) */
export function useGitHub(getRepoPath: () => string | undefined) {
  const [status, setStatus] = createSignal<GitHubStatus | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  let intervalId: number | null = null;
  let currentInterval = BASE_INTERVAL;
  let consecutiveErrors = 0;

  /** Fetch GitHub status for the current repo */
  async function refresh(): Promise<void> {
    const path = getRepoPath();
    if (!path) {
      setStatus(null);
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const result = await invoke<GitHubStatus>("get_github_status", { path });
      setStatus(result);
      consecutiveErrors = 0;
      currentInterval = document.hidden ? HIDDEN_INTERVAL : BASE_INTERVAL;
    } catch (err) {
      console.error("Failed to get GitHub status:", err);
      setError(String(err));
      setStatus(null);
      // Exponential backoff on errors: 30s -> 60s -> 120s -> 300s (cap)
      consecutiveErrors++;
      currentInterval = Math.min(BASE_INTERVAL * Math.pow(2, consecutiveErrors), MAX_INTERVAL);
    } finally {
      setLoading(false);
    }
  }

  /** Start or restart polling with current interval */
  function scheduleNext(): void {
    if (intervalId) clearInterval(intervalId);
    intervalId = window.setInterval(refresh, currentInterval);
  }

  /** Start automatic polling */
  function startPolling(): void {
    refresh();
    scheduleNext();
  }

  /** Stop automatic polling */
  function stopPolling(): void {
    if (intervalId) {
      clearInterval(intervalId);
      intervalId = null;
    }
  }

  /** Handle visibility changes - slow down when hidden, speed up when visible */
  function handleVisibilityChange(): void {
    if (document.hidden) {
      currentInterval = HIDDEN_INTERVAL;
    } else {
      currentInterval = consecutiveErrors > 0
        ? Math.min(BASE_INTERVAL * Math.pow(2, consecutiveErrors), MAX_INTERVAL)
        : BASE_INTERVAL;
      // Refresh immediately when becoming visible
      refresh();
    }
    scheduleNext();
  }

  // Refresh immediately when the repo path changes (clears stale branch from previous repo)
  createEffect(() => {
    const path = getRepoPath();
    if (path) {
      setStatus(null);
      refresh();
    } else {
      setStatus(null);
    }
  });

  // Auto-start polling on mount
  onMount(() => {
    startPolling();
    document.addEventListener("visibilitychange", handleVisibilityChange);
  });

  // Cleanup on unmount
  onCleanup(() => {
    stopPolling();
    document.removeEventListener("visibilitychange", handleVisibilityChange);
  });

  return {
    status,
    loading,
    error,
    refresh,
    startPolling,
    stopPolling,
  };
}
