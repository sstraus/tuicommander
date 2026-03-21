import { Component, createEffect, createMemo, createSignal, For, Show, on } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import type { BranchDetail } from "./types";
import s from "./BranchesTab.module.css";

export interface BranchesTabProps {
  repoPath: string | null;
}

const CheckIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z" />
  </svg>
);

/** Convert an ISO date string to a short relative label like "3d ago", "2mo ago" */
function relativeDate(isoDate: string | null): string {
  if (!isoDate) return "";
  const then = new Date(isoDate).getTime();
  if (isNaN(then)) return "";
  const diffMs = Date.now() - then;
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  if (diffDays < 1) return "today";
  if (diffDays < 7) return `${diffDays}d ago`;
  const diffWeeks = Math.floor(diffDays / 7);
  if (diffWeeks < 5) return `${diffWeeks}w ago`;
  const diffMonths = Math.floor(diffDays / 30);
  if (diffMonths < 12) return `${diffMonths}mo ago`;
  const diffYears = Math.floor(diffDays / 365);
  return `${diffYears}y ago`;
}

/** A branch is considered stale if its last commit is older than 90 days */
function isStale(isoDate: string | null): boolean {
  if (!isoDate) return false;
  const then = new Date(isoDate).getTime();
  if (isNaN(then)) return false;
  return Date.now() - then > 90 * 24 * 60 * 60 * 1000;
}

export const BranchesTab: Component<BranchesTabProps> = (props) => {
  const [branches, setBranches] = createSignal<BranchDetail[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [localExpanded, setLocalExpanded] = createSignal(true);
  const [remoteExpanded, setRemoteExpanded] = createSignal(true);

  async function fetchBranches(repoPath: string) {
    setLoading(true);
    try {
      const result = await invoke<BranchDetail[]>("get_branches_detail", { path: repoPath });
      setBranches(result);
    } catch (err) {
      appLogger.error("git", "Failed to load branches", err);
      setBranches([]);
    } finally {
      setLoading(false);
    }
  }

  // Re-fetch when repo changes or revision bumps
  createEffect(
    on(
      () => {
        const repoPath = props.repoPath;
        const rev = repoPath ? repositoriesStore.getRevision(repoPath) : 0;
        return `${repoPath ?? ""}:${rev}`;
      },
      () => {
        const repoPath = props.repoPath;
        if (repoPath) void fetchBranches(repoPath);
        else setBranches([]);
      },
    ),
  );

  const localBranches = createMemo(() =>
    branches().filter((b) => !b.is_remote),
  );

  const remoteBranches = createMemo(() =>
    branches().filter((b) => b.is_remote),
  );

  /** Filter branches by search query, but always keep the current branch visible in local */
  const filteredLocal = createMemo(() => {
    const q = search().trim().toLowerCase();
    if (!q) return localBranches();
    return localBranches().filter(
      (b) => b.is_current || b.name.toLowerCase().includes(q),
    );
  });

  const filteredRemote = createMemo(() => {
    const q = search().trim().toLowerCase();
    if (!q) return remoteBranches();
    return remoteBranches().filter((b) => b.name.toLowerCase().includes(q));
  });

  return (
    <div class={s.container}>
      <div class={s.searchBar}>
        <input
          class={s.searchInput}
          type="text"
          placeholder="Filter branches…"
          value={search()}
          onInput={(e) => setSearch(e.currentTarget.value)}
        />
      </div>

      <Show when={!loading()} fallback={<div class={s.empty}>Loading branches…</div>}>
        <Show when={branches().length > 0} fallback={<div class={s.empty}>No branches</div>}>

          {/* Local section */}
          <div
            class={s.sectionHeader}
            onClick={() => setLocalExpanded((v) => !v)}
          >
            <span class={cx(s.chevron, !localExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
            Local
            <span class={s.sectionCount}>{localBranches().length}</span>
          </div>
          <Show when={localExpanded()}>
            <For each={filteredLocal()}>
              {(branch) => (
                <div class={cx(s.branchRow, isStale(branch.last_commit_date) && s.stale)}>
                  <Show
                    when={branch.is_current}
                    fallback={<span class={s.branchIconPlaceholder} />}
                  >
                    <span class={s.branchCurrentIcon}>
                      <CheckIcon />
                    </span>
                  </Show>
                  <span
                    class={cx(
                      s.branchName,
                      branch.is_current && s.branchNameBold,
                      branch.is_current && s.branchCurrent,
                    )}
                    title={branch.name}
                  >
                    {branch.name}
                  </span>
                  <span class={s.branchMeta}>
                    <Show when={(branch.ahead ?? 0) > 0}>
                      <span class={s.ahead}>↑{branch.ahead}</span>
                    </Show>
                    <Show when={(branch.behind ?? 0) > 0}>
                      <span class={s.behind}>↓{branch.behind}</span>
                    </Show>
                    <Show when={branch.is_merged}>
                      <span class={s.merged}>merged</span>
                    </Show>
                    <Show when={branch.last_commit_date}>
                      <span class={s.metaDate}>{relativeDate(branch.last_commit_date)}</span>
                    </Show>
                  </span>
                </div>
              )}
            </For>
          </Show>

          {/* Remote section — only show when there are remote branches */}
          <Show when={remoteBranches().length > 0}>
            <div
              class={s.sectionHeader}
              onClick={() => setRemoteExpanded((v) => !v)}
            >
              <span class={cx(s.chevron, !remoteExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
              Remote
              <span class={s.sectionCount}>{remoteBranches().length}</span>
            </div>
            <Show when={remoteExpanded()}>
              <For each={filteredRemote()}>
                {(branch) => (
                  <div class={cx(s.branchRow, isStale(branch.last_commit_date) && s.stale)}>
                    <span class={s.branchIconPlaceholder} />
                    <span class={s.branchName} title={branch.name}>
                      {branch.name}
                    </span>
                    <span class={s.branchMeta}>
                      <Show when={branch.is_merged}>
                        <span class={s.merged}>merged</span>
                      </Show>
                      <Show when={branch.last_commit_date}>
                        <span class={s.metaDate}>{relativeDate(branch.last_commit_date)}</span>
                      </Show>
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </Show>

        </Show>
      </Show>
    </div>
  );
};

export default BranchesTab;
