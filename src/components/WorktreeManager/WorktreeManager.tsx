import { Component, For, Show, createEffect, createMemo, onCleanup } from "solid-js";
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

/** Extract display name (last path segment) from a path */
function displayName(path: string): string {
  const segments = path.replace(/\/+$/, "").split("/");
  return segments[segments.length - 1] || path;
}

export const WorktreeManager: Component = () => {
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

  const worktrees = createMemo<WorktreeRow[]>(() => {
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

    // Sort: non-main first (actionable), then main; alphabetical within each group
    rows.sort((a, b) => {
      if (a.isMain !== b.isMain) return a.isMain ? 1 : -1;
      return a.branch.localeCompare(b.branch);
    });

    return rows;
  });

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

          <div class={s.list}>
            <Show when={worktrees().length === 0}>
              <div class={s.empty}>No worktrees found. Create one from the sidebar.</div>
            </Show>

            <For each={worktrees()}>
              {(wt) => (
                <div class={`${s.row} ${wt.isMain ? s.mainRow : ""}`}>
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
