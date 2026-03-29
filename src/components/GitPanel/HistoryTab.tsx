import { Component, createEffect, createSignal, For, Show, on, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { relativeTime } from "../../utils/time";
import type { CommitLogEntry } from "./types";
import { appLogger } from "../../stores/appLogger";
import s from "./HistoryTab.module.css";

const PAGE_SIZE = 50;

export interface HistoryTabProps {
  repoPath: string | null;
  filePath: string | null;
}

export const HistoryTab: Component<HistoryTabProps> = (props) => {
  const [commits, setCommits] = createSignal<CommitLogEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [loadingMore, setLoadingMore] = createSignal(false);
  const [hasMore, setHasMore] = createSignal(true);

  /** Fetch the initial commit page for the file */
  async function fetchHistory(repoPath: string, filePath: string, isCancelled?: () => boolean) {
    setLoading(true);
    setCommits([]);
    setHasMore(true);
    try {
      const result = await invoke<CommitLogEntry[]>("get_file_history", {
        path: repoPath,
        file: filePath,
        count: PAGE_SIZE,
      });
      if (isCancelled?.()) return;
      setCommits(result);
      setHasMore(result.length >= PAGE_SIZE);
    } catch (err) {
      if (isCancelled?.()) return;
      appLogger.debug("git", "Failed to load file history", err);
      setCommits([]);
      setHasMore(false);
    } finally {
      if (!isCancelled?.()) setLoading(false);
    }
  }

  /** Load the next page of commits */
  async function loadMore() {
    const repoPath = props.repoPath;
    const filePath = props.filePath;
    const current = commits();
    if (!repoPath || !filePath || current.length === 0 || loadingMore()) return;

    const lastHash = current[current.length - 1].hash;
    setLoadingMore(true);
    try {
      const result = await invoke<CommitLogEntry[]>("get_file_history", {
        path: repoPath,
        file: filePath,
        count: PAGE_SIZE,
        after: lastHash,
      });
      // The `after` param includes the starting commit, so skip the first (duplicate)
      const newCommits = result.length > 0 && result[0].hash === lastHash
        ? result.slice(1)
        : result;
      if (newCommits.length === 0) {
        setHasMore(false);
      } else {
        setCommits((prev) => [...prev, ...newCommits]);
        setHasMore(newCommits.length >= PAGE_SIZE - 1);
      }
    } catch (err) {
      appLogger.debug("git", "Failed to load more file history", err);
      setHasMore(false);
    } finally {
      setLoadingMore(false);
    }
  }

  /** Open a diff tab for the file at a specific commit */
  function openCommitDiff(hash: string) {
    const repoPath = props.repoPath;
    const filePath = props.filePath;
    if (!repoPath || !filePath) return;
    diffTabsStore.add(repoPath, filePath, "M", hash);
  }

  // Re-fetch when repo, file, or revision changes
  createEffect(
    on(
      () => {
        const repoPath = props.repoPath;
        const filePath = props.filePath;
        // Track revision to re-fetch on repo changes (value consumed for reactivity)
        const rev = repoPath ? repositoriesStore.getRevision(repoPath) : 0;
        return `${repoPath ?? ""}:${filePath ?? ""}:${rev}`;
      },
      () => {
        let cancelled = false;
        onCleanup(() => { cancelled = true; });

        const repoPath = props.repoPath;
        const filePath = props.filePath;
        if (repoPath && filePath) {
          void fetchHistory(repoPath, filePath, () => cancelled);
        } else {
          setCommits([]);
        }
      },
    ),
  );

  return (
    <div class={s.container} onContextMenu={(e) => e.preventDefault()}>
      {/* File header */}
      <Show when={props.filePath}>
        <div class={s.fileHeader}>
          <svg class={s.fileIcon} width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M13 4H8.4L7 2H3a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V5a1 1 0 0 0-1-1z"/>
          </svg>
          <span class={s.filePath} title={props.filePath!}>{props.filePath}</span>
        </div>
      </Show>

      <Show when={!props.filePath}>
        <div class={s.empty}>Select a file to view its history</div>
      </Show>

      <Show when={props.filePath}>
        <Show when={!loading()} fallback={<div class={s.empty}>Loading history...</div>}>
          <Show when={commits().length > 0} fallback={<div class={s.empty}>No history found for this file</div>}>
            <div class={s.scrollContainer}>
              <For each={commits()}>
                {(commit) => (
                  <div
                    class={s.commitRow}
                    onClick={() => openCommitDiff(commit.hash)}
                  >
                    {/* Line 1: dot + hash + subject */}
                    <div class={s.commitLine1} title={commit.subject}>
                      <span class={s.commitDot} />
                      <span class={s.commitHash}>{commit.hash.slice(0, 7)}</span>
                      <span class={s.commitSubject}>{commit.subject}</span>
                    </div>
                    {/* Line 2: author + time */}
                    <div class={s.commitLine2}>
                      <span class={s.commitMeta}>
                        {commit.author_name} · {relativeTime(commit.author_date)}
                      </span>
                    </div>
                  </div>
                )}
              </For>
              {/* Load more button */}
              <Show when={hasMore() && commits().length > 0}>
                <div class={s.loadMore}>
                  <button
                    class={s.loadMoreBtn}
                    onClick={(e) => { e.stopPropagation(); loadMore(); }}
                    disabled={loadingMore()}
                  >
                    {loadingMore() ? "Loading..." : "Load more"}
                  </button>
                </div>
              </Show>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
};

export default HistoryTab;
