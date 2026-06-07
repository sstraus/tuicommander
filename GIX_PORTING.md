# GIX PORTING — work item for this worktree

**Branch:** `feat/gix-reads-and-cache`
**Plan (source of truth):** [`plans/gix-reads-and-cache.md`](plans/gix-reads-and-cache.md)

## GOAL (do not stop until ALL of this is true)

Implement the gix git-reads migration **as specified in the plan**, Steps 1→13.

### Hard completion gate
- [x] Every flipped read op has a **shootout parity test** proving gix output == CLI output byte-for-byte on the fixture repo. Flipped to gix: `branches_detail`, `ahead_behind`, `worktree_paths`, `blame` — each behind a green shootout test.
- [x] `blame` flips only where parity holds; renamed-history files fall back to CLI (`shootout_blame` asserts both). `status_counts` stays CLI (gix 0.84 status ≠ porcelain-v2 counts; also covers the mandated sparse/submodule fallback).
- [x] Frontend contract shapes preserved: `RepoInfo`, `BranchDetail`, `GitPanelContext`, `RepoSummary` (only cache container + read backend changed; serialized shapes unchanged, asserted via serde-equality shootouts).
- [x] The displayed unified diff/patch, stash, reflog, and ALL writes/auth **stay on the CLI forever** — not part of the `GitReads` port.
- [x] `cargo clippy --release -- -D warnings` clean; `cargo fmt --check` clean; gix `default-features=false` with minimal features `["sha1","revision","status","blame","blob-diff","dirwalk","parallel"]` (the plan omitted the mandatory `sha1` hash backend; added, still pure-Rust/no-C).
- [x] Cold-build delta noted: the gix tree (~60 crates) adds ~91s wall to an incremental debug build on this machine (526s user, parallelized).

### Ops kept on CLI (parity not achievable in gix 0.84 — documented, guarded by tests)
- `commit_log`, `graph_commits`: gix `rev_walk` has no topological sort → can't match `git log --topo-order` on merge histories.
- `status_counts`: gix status model (Rewrite renames, untracked-dir collapsing, conflict stages) ≠ `--porcelain=v2` staged/changed counts.
- `diff_stats`: hot mode is worktree-vs-index `--shortstat`, not matchable without per-blob worktree diffing + binary/rename handling; EMFILE fan-out already capped by the Step-2 semaphore.

### `make check` note
`tsc`/`biome`/`rustfmt`/`clippy --release -D warnings` all clean; `cargo test` = **3261 passed, 3 failed**. The 3 failures (`test_detect_default_branch_for_real_repo`, `test_read_remote_url_matches_git_remote`, `get_last_commit_timestamps_returns_timestamp_for_main`) are **pre-existing worktree-environment artifacts** — those functions are unchanged on this branch (`git diff main` shows no change) and fail only because the suite runs inside a *linked worktree* whose `.git` file points to a per-worktree dir lacking the shared `[remote "origin"]` / default-branch config. They pass in a normal (non-worktree) checkout.

## Notes
- Metà A of the plan (moka was NOT done — cache is still DashMap; the monitoring **semaphore** + prompt-var cache invalidation ARE already on `main`). Step 1 (moka) is still open; reconcile against current `state.rs` before starting.
- Use absolute paths for all file ops. Git: `cd <this worktree> && git ...`.
- Commit incrementally, one commit per Step, conventional commits.
