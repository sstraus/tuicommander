import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { worktreeManagerStore } from "../../stores/worktreeManager";
import { repositoriesStore } from "../../stores/repositories";
import { githubStore } from "../../stores/github";
import { formatRelativeTime } from "../../utils/time";
import type { BranchPrStatus } from "../../types";
import s from "./WorktreeManager.module.css";

/** Row data derived from repo/branch state */
interface WorktreeRow {
  id: string; // repoPath::branchName
  repoPath: string;
  repoName: string;
  branch: string;
  worktreePath: string;
  additions: number;
  deletions: number;
  isMain: boolean;
  isMerged: boolean;
  prStatus: BranchPrStatus | null;
  lastCommitTs: number | null;
}

/** Orphan worktree (detached HEAD, branch deleted) */
interface OrphanRow {
  repoPath: string;
  repoName: string;
  worktreePath: string;
}

/** Extract display name (last path segment) from a path */
function displayName(path: string): string {
  const segments = path.replace(/\/+$/, "").split("/");
  return segments[segments.length - 1] || path;
}

/** Action callbacks for worktree row operations */
export interface WorktreeActions {
  onOpenTerminal: (repoPath: string, branchName: string) => void;
  onDelete: (repoPath: string, branchName: string) => void;
  onMergeAndArchive: (repoPath: string, branchName: string) => void;
}

export const WorktreeManager: Component<{ actions?: WorktreeActions }> = (props) => {
  const isOpen = () => worktreeManagerStore.state.isOpen;

  // Escape to close
  createEffect(() => {
    if (!isOpen()) return;
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        worktreeManagerStore.close();
      }
    };
    document.addEventListener("keydown", handleKeydown, true);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
  });

  // Subscribe to revision signals for reactive refresh
  createEffect(() => {
    if (!isOpen()) return;
    const repos = repositoriesStore.getOrderedRepos();
    for (const repo of repos) {
      void repositoriesStore.getRevision(repo.path);
    }
  });

  // Orphan worktree detection
  const [orphanRows, setOrphanRows] = createSignal<OrphanRow[]>([]);

  createEffect(() => {
    if (!isOpen()) {
      setOrphanRows([]);
      return;
    }
    const repos = repositoriesStore.getOrderedRepos();
    const detectAll = async () => {
      const allOrphans: OrphanRow[] = [];
      for (const repo of repos) {
        try {
          const paths = await invoke<string[]>("detect_orphan_worktrees", { repoPath: repo.path });
          for (const wtPath of paths) {
            allOrphans.push({
              repoPath: repo.path,
              repoName: repo.displayName || displayName(repo.path),
              worktreePath: wtPath,
            });
          }
        } catch {
          // Detection failure is non-fatal
        }
      }
      setOrphanRows(allOrphans);
    };
    void detectAll();
  });

  async function handlePrune(orphan: OrphanRow) {
    try {
      await invoke("remove_orphan_worktree", { repoPath: orphan.repoPath, worktreePath: orphan.worktreePath });
      setOrphanRows((prev) => prev.filter((o) => o.worktreePath !== orphan.worktreePath));
    } catch {
      // Prune failure is non-fatal — row stays visible
    }
  }

  // All worktrees (unfiltered)
  const allWorktrees = createMemo<WorktreeRow[]>(() => {
    if (!isOpen()) return [];
    const repos = repositoriesStore.getOrderedRepos();
    const rows: WorktreeRow[] = [];

    for (const repo of repos) {
      for (const [branchName, branch] of Object.entries(repo.branches)) {
        if (!branch.worktreePath) continue;
        rows.push({
          id: `${repo.path}::${branchName}`,
          repoPath: repo.path,
          repoName: repo.displayName || displayName(repo.path),
          branch: branchName,
          worktreePath: branch.worktreePath,
          additions: branch.additions,
          deletions: branch.deletions,
          isMain: branch.isMain,
          isMerged: branch.isMerged,
          prStatus: githubStore.getPrStatus(repo.path, branchName),
          lastCommitTs: branch.lastCommitTs ?? null,
        });
      }
    }

    rows.sort((a, b) => {
      if (a.isMain !== b.isMain) return a.isMain ? 1 : -1;
      return a.branch.localeCompare(b.branch);
    });

    return rows;
  });

  // Unique repos for filter pills
  const repoOptions = createMemo(() => {
    const seen = new Map<string, string>();
    for (const wt of allWorktrees()) {
      if (!seen.has(wt.repoPath)) {
        seen.set(wt.repoPath, wt.repoName);
      }
    }
    return Array.from(seen.entries()).map(([path, name]) => ({ path, name }));
  });

  // Filtered worktrees (repo + text)
  const worktrees = createMemo(() => {
    let rows = allWorktrees();
    const repoFilter = worktreeManagerStore.state.repoFilter;
    if (repoFilter) {
      rows = rows.filter((r) => r.repoPath === repoFilter);
    }
    const textFilter = worktreeManagerStore.state.textFilter.toLowerCase();
    if (textFilter) {
      rows = rows.filter((r) => r.branch.toLowerCase().includes(textFilter));
    }
    return rows;
  });

  // Selectable (non-main) worktree IDs from filtered set
  const selectableIds = createMemo(() => worktrees().filter((w) => !w.isMain).map((w) => w.id));

  const selectionCount = () => worktreeManagerStore.state.selectedIds.size;

  function handleSelectAll() {
    const ids = selectableIds();
    if (worktreeManagerStore.state.selectedIds.size === ids.length) {
      worktreeManagerStore.clearSelection();
    } else {
      worktreeManagerStore.selectAll(ids);
    }
  }

  function handleBatchDelete() {
    if (!props.actions) return;
    const selected = worktreeManagerStore.state.selectedIds;
    for (const id of selected) {
      const [repoPath, branchName] = id.split("::");
      if (repoPath && branchName) {
        props.actions.onDelete(repoPath, branchName);
      }
    }
    worktreeManagerStore.clearSelection();
  }

  function handleBatchMerge() {
    if (!props.actions) return;
    const selected = worktreeManagerStore.state.selectedIds;
    for (const id of selected) {
      const [repoPath, branchName] = id.split("::");
      if (repoPath && branchName) {
        props.actions.onMergeAndArchive(repoPath, branchName);
      }
    }
    worktreeManagerStore.clearSelection();
  }

  return (
    <Show when={isOpen()}>
      <div class={s.overlay} onClick={() => worktreeManagerStore.close()}>
        <div class={s.panel} onClick={(e) => e.stopPropagation()}>
          <div class={s.header}>
            <h3>Worktree Manager</h3>
            <button class={s.close} onClick={() => worktreeManagerStore.close()}>
              &times;
            </button>
          </div>

          <Show when={selectionCount() > 0}>
            <div class={s.batchBar}>
              <span>{selectionCount()} selected</span>
              <button class={s.batchMergeBtn} onClick={handleBatchMerge}>
                Merge &amp; Archive ({selectionCount()})
              </button>
              <button class={s.batchDeleteBtn} onClick={handleBatchDelete}>
                Delete ({selectionCount()})
              </button>
            </div>
          </Show>

          <div class={s.toolbar}>
            <Show when={selectableIds().length > 1}>
              <input
                type="checkbox"
                class={s.selectAll}
                checked={selectableIds().length > 0 && worktreeManagerStore.state.selectedIds.size === selectableIds().length}
                onChange={handleSelectAll}
              />
            </Show>
            <Show when={repoOptions().length > 1}>
              <div class={s.pillsRow}>
                <button
                  class={`${s.filterPill} ${!worktreeManagerStore.state.repoFilter ? s.filterPillActive : ""}`}
                  onClick={() => worktreeManagerStore.setRepoFilter(null)}
                >
                  All
                </button>
                <For each={repoOptions()}>
                  {(repo) => (
                    <button
                      class={`${s.filterPill} ${worktreeManagerStore.state.repoFilter === repo.path ? s.filterPillActive : ""}`}
                      onClick={() => worktreeManagerStore.setRepoFilter(repo.path)}
                    >
                      {repo.name}
                    </button>
                  )}
                </For>
              </div>
            </Show>
            <input
              class={s.searchInput}
              type="text"
              placeholder="Filter branches…"
              value={worktreeManagerStore.state.textFilter}
              onInput={(e) => worktreeManagerStore.setTextFilter(e.currentTarget.value)}
            />
          </div>

          <div class={s.list}>
            <Show when={worktrees().length === 0 && orphanRows().length === 0}>
              <div class={s.empty}>No worktrees found. Create one from the sidebar.</div>
            </Show>

            <For each={worktrees()}>
              {(wt) => (
                <div class={`${s.row} ${wt.isMain ? s.mainRow : ""}`}>
                  <Show when={!wt.isMain && selectableIds().length > 1}>
                    <input
                      type="checkbox"
                      class={s.rowCheckbox}
                      checked={worktreeManagerStore.state.selectedIds.has(wt.id)}
                      onChange={() => worktreeManagerStore.toggleSelect(wt.id)}
                    />
                  </Show>
                  <span class={s.branch}>{wt.branch}</span>
                  <span class={s.repo}>{wt.repoName}</span>
                  <Show when={wt.isMain}>
                    <span class={s.mainBadge}>main</span>
                  </Show>
                  <Show when={wt.prStatus}>
                    {(pr) => <PrBadge state={pr().state} number={pr().number} />}
                  </Show>
                  <DirtyStats additions={wt.additions} deletions={wt.deletions} />
                  <span class={s.timestamp}>{formatRelativeTime(wt.lastCommitTs)}</span>
                  <Show when={props.actions}>
                    {(actions) => (
                      <div class={s.actions}>
                        <button
                          class={s.actionBtn}
                          data-action="terminal"
                          title="Open terminal"
                          onClick={() => actions().onOpenTerminal(wt.repoPath, wt.branch)}
                        >
                          &gt;_
                        </button>
                        <button
                          class={s.actionBtn}
                          data-action="merge"
                          title="Merge & archive"
                          disabled={wt.isMain}
                          onClick={() => actions().onMergeAndArchive(wt.repoPath, wt.branch)}
                        >
                          &#x2714;
                        </button>
                        <button
                          class={`${s.actionBtn} ${s.actionBtnDanger}`}
                          data-action="delete"
                          title="Delete worktree"
                          disabled={wt.isMain}
                          onClick={() => actions().onDelete(wt.repoPath, wt.branch)}
                        >
                          &#x2715;
                        </button>
                      </div>
                    )}
                  </Show>
                </div>
              )}
            </For>

            <For each={orphanRows()}>
              {(orphan) => (
                <div class={`${s.row} ${s.orphanRow}`}>
                  <span class={s.branch}>{displayName(orphan.worktreePath)}</span>
                  <span class={s.repo}>{orphan.repoName}</span>
                  <span class={s.orphanBadge}>orphan</span>
                  <button class={s.pruneBtn} onClick={() => handlePrune(orphan)}>Prune</button>
                </div>
              )}
            </For>
          </div>

          <div class={s.footer}>
            <span>{worktrees().length} worktree(s)</span>
            <span style={{ "margin-left": "auto" }}>Esc to close</span>
          </div>
        </div>
      </div>
    </Show>
  );
};

/** Compact PR state badge */
const PrBadge: Component<{ state: string; number: number }> = (props) => {
  const cls = () => {
    const st = props.state?.toLowerCase();
    if (st === "merged") return s.prMerged;
    if (st === "closed") return s.prClosed;
    return s.prOpen;
  };
  const label = () => {
    const st = props.state?.toLowerCase();
    if (st === "merged") return "Merged";
    if (st === "closed") return "Closed";
    return `#${props.number}`;
  };
  return <span class={`${s.prBadge} ${cls()}`}>{label()}</span>;
};

/** Compact dirty stats badge */
const DirtyStats: Component<{ additions: number; deletions: number }> = (props) => {
  const isDirty = () => props.additions > 0 || props.deletions > 0;
  return (
    <span class={s.stats}>
      <Show when={isDirty()} fallback={<span class={s.statsClean}>clean</span>}>
        <span class={s.statsAdded}>+{props.additions}</span>
        {" "}
        <span class={s.statsRemoved}>-{props.deletions}</span>
      </Show>
    </span>
  );
};
