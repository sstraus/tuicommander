import { createMemo } from "solid-js";
import { githubStore } from "../stores/github";
import type { GitHubStatus } from "../types";

/**
 * GitHub status hook — reactive wrapper around the centralized githubStore.
 *
 * Reads remote tracking data (ahead/behind) from the store's unified polling
 * instead of maintaining its own independent timer.
 */
export function useGitHub(getRepoPath: () => string | undefined) {
  const status = createMemo<GitHubStatus | null>(() => {
    const path = getRepoPath();
    if (!path) return null;
    return githubStore.getRemoteStatus(path);
  });

  const loading = () => false;
  const error = () => null;

  function refresh(): void {
    const path = getRepoPath();
    if (path) githubStore.pollRepo(path);
  }

  return {
    status,
    loading,
    error,
    refresh,
    startPolling: () => {},
    stopPolling: () => {},
  };
}
