import { Component, createEffect, createSignal, For, Show, on, onCleanup } from "solid-js";
import { createVirtualizer } from "@tanstack/solid-virtual";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { cx } from "../../utils";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore, isDiffStatus } from "../../stores/diffTabs";
import { CommitGraph, graphWidth } from "./CommitGraph";
import type { GraphNode } from "./CommitGraph";
import type { CommitLogEntry, ChangedFile } from "./types";
import { relativeTime } from "../../utils/time";
import s from "./LogTab.module.css";

/** Collapsed row height (2 lines: subject + meta) */
const ROW_HEIGHT = 48;
/** Extra height per changed file when expanded */
const FILE_LINE_HEIGHT = 22;
/** Loading indicator + padding for expanded section */
const EXPANDED_OVERHEAD = 12;

export interface LogTabProps {
  repoPath: string | null;
}

/** Classify a ref string into branch, tag, or HEAD. */
function classifyRef(ref: string): "head" | "tag" | "branch" {
  if (ref === "HEAD") return "head";
  if (ref.startsWith("tag: ")) return "tag";
  return "branch";
}

/** Strip common ref prefixes for display. */
function displayRef(ref: string): string {
  if (ref.startsWith("tag: ")) return ref.slice(5);
  if (ref.startsWith("HEAD -> ")) return ref.slice(8);
  return ref;
}

const FILE_STATUS_CLASS: Record<string, string> = {
  M: s.statusM,
  A: s.statusA,
  D: s.statusD,
  R: s.statusR,
};

const REF_CLASS: Record<string, string> = {
  head: s.refHead,
  tag: s.refTag,
  branch: s.refBranch,
};

export const LogTab: Component<LogTabProps> = (props) => {
  const [commits, setCommits] = createSignal<CommitLogEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [loadingMore, setLoadingMore] = createSignal(false);
  const [hasMore, setHasMore] = createSignal(true);
  const [expandedHash, setExpandedHash] = createSignal<string | null>(null);
  const [changedFiles, setChangedFiles] = createSignal<Record<string, ChangedFile[]>>({});
  const [filesLoading, setFilesLoading] = createSignal<Record<string, boolean>>({});
  const [graphNodes, setGraphNodes] = createSignal<GraphNode[]>([]);
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewportHeight, setViewportHeight] = createSignal(0);
  const [focusedIndex, setFocusedIndex] = createSignal(-1);

  let scrollRef!: HTMLDivElement;

  const PAGE_SIZE = 50;

  /** Fetch the initial commit page and graph data */
  async function fetchCommits(repoPath: string, isCancelled?: () => boolean) {
    setLoading(true);
    setCommits([]);
    setGraphNodes([]);
    setExpandedHash(null);
    setChangedFiles({});
    setHasMore(true);
    try {
      const [logResult, graphResult] = await Promise.all([
        invoke<CommitLogEntry[]>("get_commit_log", {
          path: repoPath,
          count: PAGE_SIZE,
        }),
        invoke<GraphNode[]>("get_commit_graph", {
          path: repoPath,
          count: 200,
        }).catch((err) => {
          appLogger.debug("git", "Failed to load commit graph", err);
          return [] as GraphNode[];
        }),
      ]);
      if (isCancelled?.()) return;
      setCommits(logResult);
      setGraphNodes(graphResult);
      setHasMore(logResult.length >= PAGE_SIZE);
    } catch (err) {
      if (isCancelled?.()) return;
      appLogger.debug("git", "Failed to load commit log", err);
      setCommits([]);
      setGraphNodes([]);
      setHasMore(false);
    } finally {
      if (!isCancelled?.()) setLoading(false);
    }
  }

  /** Load the next page of commits */
  async function loadMore() {
    const repoPath = props.repoPath;
    const current = commits();
    if (!repoPath || current.length === 0 || loadingMore()) return;

    const lastHash = current[current.length - 1].hash;
    setLoadingMore(true);
    try {
      const result = await invoke<CommitLogEntry[]>("get_commit_log", {
        path: repoPath,
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
      appLogger.debug("git", "Failed to load more commits", err);
      setHasMore(false);
    } finally {
      setLoadingMore(false);
    }
  }

  /** Toggle expansion and fetch changed files on demand */
  async function toggleExpand(hash: string) {
    if (expandedHash() === hash) {
      setExpandedHash(null);
      return;
    }
    setExpandedHash(hash);

    const repoPath = props.repoPath;
    if (!repoPath) return;

    // Fetch files if not cached
    if (!changedFiles()[hash]) {
      setFilesLoading((prev) => ({ ...prev, [hash]: true }));
      try {
        const files = await invoke<ChangedFile[]>("get_changed_files", {
          path: repoPath,
          scope: hash,
        });
        setChangedFiles((prev) => ({ ...prev, [hash]: files }));
      } catch (err) {
        appLogger.debug("git", "Failed to load changed files for commit", err);
        setChangedFiles((prev) => ({ ...prev, [hash]: [] }));
      } finally {
        setFilesLoading((prev) => ({ ...prev, [hash]: false }));
      }
    }
  }

  /** Compute dynamic row height: collapsed = ROW_HEIGHT, expanded = ROW_HEIGHT + files */
  function estimateSize(index: number): number {
    const commit = commits()[index];
    if (!commit || expandedHash() !== commit.hash) return ROW_HEIGHT;
    const files = changedFiles()[commit.hash];
    if (!files) return ROW_HEIGHT + EXPANDED_OVERHEAD + FILE_LINE_HEIGHT; // loading state
    return ROW_HEIGHT + EXPANDED_OVERHEAD + files.length * FILE_LINE_HEIGHT;
  }

  // Re-fetch when repo changes or revision bumps
  createEffect(
    on(
      () => {
        const repoPath = props.repoPath;
        // Track revision to re-fetch on repo changes (value consumed for reactivity)
        const rev = repoPath ? repositoriesStore.getRevision(repoPath) : 0;
        return `${repoPath ?? ""}:${rev}`;
      },
      () => {
        let cancelled = false;
        onCleanup(() => { cancelled = true; });
        if (props.repoPath) void fetchCommits(props.repoPath, () => cancelled);
      },
    ),
  );

  const virtualizer = createVirtualizer({
    get count() { return commits().length; },
    getScrollElement: () => scrollRef,
    estimateSize,
    overscan: 5,
  });

  // Sync scroll position and viewport size for the graph canvas.
  createEffect(() => {
    const el = scrollRef;
    if (!el) return;

    setViewportHeight(el.clientHeight);

    let rafId = 0;
    const onScroll = () => {
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        setScrollTop(el.scrollTop);
      });
    };

    const onResize = () => setViewportHeight(el.clientHeight);

    el.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(onResize);
    ro.observe(el);

    onCleanup(() => {
      el.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(rafId);
      ro.disconnect();
    });
  });

  /** Left padding for commit rows to leave room for the graph */
  const graphPad = () => graphWidth(graphNodes());

  function handleListKeyDown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

    const total = commits().length;
    if (total === 0) return;

    const idx = focusedIndex();

    if (e.key === "ArrowDown") {
      e.preventDefault();
      const next = Math.min(idx + 1, total - 1);
      setFocusedIndex(next);
      virtualizer.scrollToIndex(next, { align: "auto" });
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(idx - 1, 0);
      setFocusedIndex(next);
      virtualizer.scrollToIndex(next, { align: "auto" });
      return;
    }
    if (e.key === "Enter" && idx >= 0 && idx < total) {
      e.preventDefault();
      toggleExpand(commits()[idx].hash);
      return;
    }
  }

  return (
    <div class={s.container} onKeyDown={handleListKeyDown} onContextMenu={(e) => e.preventDefault()} tabIndex={-1}>
      {/* Graph canvas overlays the scroll container from outside the scroll flow */}
      <Show when={!loading() && graphNodes().length > 0}>
        <CommitGraph
          nodes={graphNodes()}
          scrollTop={scrollTop()}
          viewportHeight={viewportHeight()}
          totalHeight={virtualizer.getTotalSize()}
        />
      </Show>
      {/* Always-mounted scroll container so virtualizer has a valid ref */}
      <div ref={scrollRef!} class={s.scrollContainer}>
        <Show when={loading()}>
          <div class={s.empty}>Loading commits...</div>
        </Show>
        <Show when={!loading() && commits().length === 0}>
          <div class={s.empty}>No commits</div>
        </Show>
        <Show when={!loading() && commits().length > 0}>
          <div class={s.virtualList} style={{ height: `${virtualizer.getTotalSize()}px` }}>
            <For each={virtualizer.getVirtualItems()}>
              {(virtualItem) => {
                const commit = () => commits()[virtualItem.index];
                const isExpanded = () => expandedHash() === commit()?.hash;
                const files = () => commit() ? changedFiles()[commit()!.hash] : undefined;
                const isFilesLoading = () => commit() ? filesLoading()[commit()!.hash] : false;

                const isFocused = () => focusedIndex() === virtualItem.index;

                return (
                  <div
                    class={cx(s.commitRow, isExpanded() && s.commitRowExpanded, isFocused() && s.commitRowFocused)}
                    style={{
                      position: "absolute",
                      top: `${virtualItem.start}px`,
                      height: `${virtualItem.size}px`,
                      width: "100%",
                      "padding-left": graphPad() > 0 ? `${graphPad() + 4}px` : undefined,
                    }}
                    onClick={() => {
                      setFocusedIndex(virtualItem.index);
                      commit() && toggleExpand(commit()!.hash);
                    }}
                  >
                    {/* Line 1: subject (full width) */}
                    <div class={s.commitLine1} title={commit()?.subject}>
                      <Show when={graphNodes().length === 0}>
                        <span class={s.commitDot} />
                      </Show>
                      <span class={s.commitSubject}>{commit()?.subject}</span>
                      <For each={commit()?.refs ?? []}>
                        {(ref) => {
                          const kind = classifyRef(ref);
                          return (
                            <span class={cx(s.refBadge, REF_CLASS[kind])}>
                              {displayRef(ref)}
                            </span>
                          );
                        }}
                      </For>
                    </div>
                    {/* Line 2: hash + author + time */}
                    <div class={s.commitLine2}>
                      <span class={s.commitHash}>{commit()?.hash.slice(0, 7)}</span>
                      <span class={s.commitMeta}>
                        {commit()?.author_name} · {commit() ? relativeTime(commit()!.author_date) : ""}
                      </span>
                    </div>
                    {/* Expanded: changed files list */}
                    <Show when={isExpanded()}>
                      <div class={s.changedFiles}>
                        <Show when={!isFilesLoading()} fallback={<div class={s.filesLoading}>Loading files...</div>}>
                          <For each={files() ?? []}>
                            {(file) => (
                              <div
                                class={cx(s.changedFile, isDiffStatus(file.status) && s.changedFileClickable)}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  if (props.repoPath && isDiffStatus(file.status)) {
                                    diffTabsStore.add(props.repoPath, file.path, file.status, commit()!.hash);
                                  }
                                }}
                              >
                                <span class={cx(s.fileStatus, FILE_STATUS_CLASS[file.status] ?? s.statusOther)}>
                                  {file.status}
                                </span>
                                <span class={s.filePath}>{file.path}</span>
                              </div>
                            )}
                          </For>
                          <Show when={files()?.length === 0}>
                            <div class={s.filesLoading}>No changed files</div>
                          </Show>
                        </Show>
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
          {/* Load more button */}
          <Show when={hasMore()}>
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
        </Show>
      </div>
    </div>
  );
};

export default LogTab;
