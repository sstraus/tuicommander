# Git Operations

**Modules:** `src-tauri/src/git.rs`, `src-tauri/src/git_cli.rs`, `src-tauri/src/git_reads.rs`

Git **writes** are performed by shelling out to the `git` CLI via the unified `git_cli` module. Git **reads** go through the reversible `GitReads` port (see below), which serves some ops from in-process gix and the rest from the same CLI. The `git_cli::git_cmd(path)` builder provides consistent error handling, binary resolution, and credential prompt suppression across all callsites.

## Async Execution & Caching

All Tauri git commands are `async` and run git subprocesses inside `tokio::task::spawn_blocking`. This prevents blocking Tokio worker threads during I/O-heavy operations like `git diff`, `git log`, or `git fetch`.

Git data is cached with a 60s TTL in `GitCacheState` (`state.rs`), one `moka::sync::Cache<String, Arc<T>>` per result type keyed by repo path. `moka`'s `get_with`/`try_get_with` **coalesce concurrent identical loads to a single computation** — replacing the previous hand-rolled `DashMap<String,(T,Instant)>` whose check-then-compute-then-set pattern had a TOCTOU race that let a `repo-changed` burst fan out N duplicate computes. `sync::Cache` is used (not `future::Cache`) because every loader is blocking git work run on the blocking pool; the sync `*_cached` helpers keep working without async (`git.rs::cached_get`/`cached_try` wrap the pattern). `github_repo_cooldown` stays a plain `DashMap` — it is a cooldown set, not a TTL value cache.

The unified `repo_watcher` (FSEvents on macOS, inotify on Linux) monitors the entire working tree recursively with per-category debounce (Git/WorkTree/Config) and calls `invalidate_repo_caches()` on file system changes (which also clears the prompt `var_cache` for the repo), so git data refreshes immediately instead of waiting for TTL expiry. The watcher respects `.gitignore` rules and hot-reloads them when `.gitignore` is modified. The 60s TTL serves as a safety net for missed watcher events. Most IPC calls for git data hit the cache (~0.2ms) instead of spawning a git subprocess (~20-30ms).

**Watcher-miss observability:** each cache's `moka` eviction listener increments a shared `ttl_fallbacks` counter only on `RemovalCause::Expired` (TTL aged out without the watcher invalidating first) — explicit invalidations do not count. A rising counter means the watcher likely missed events; it is surfaced in the `cpu_watchdog` HEALTH/CPU-SPIKE snapshots as `git_cache_ttl_fallbacks`.

Internal callers that need synchronous access use `_impl` suffixes (e.g. `get_diff_stats_impl`) to avoid double `spawn_blocking` nesting.

## GitReads Port (gix migration)

Read operations go through a reversible `GitReads` port (`src-tauri/src/git_reads.rs`) so individual ops can be served by in-process **gix** (gitoxide 0.84) instead of shelling out, removing the process spawn + FD + stdout-parse cost on hot paths. `CliGitReads` delegates to the existing `git_cmd`-based functions; `GixGitReads` implements the same trait with a `moka` handle cache (`ThreadSafeRepository` per path → thread-local `Repository` per call). `GitReadsRouter` (the global `git_reads()`) dispatches each op to its backend via a per-op `PerOpBackend`.

**An op is flipped to gix only behind a byte-for-byte parity ("shootout") test** comparing gix output to the CLI on a fixture repo. Where gix 0.84 cannot match git's exact output, the op stays on the CLI.

| Op | Backend | Notes |
|----|---------|-------|
| `branches_detail` | **gix** | `references()` → shorten / peel / committer ISO8601 / author / summary / upstream. ahead/behind via the `ahead_behind` backend. |
| `ahead_behind` | **gix** | `rev_parse_single` + two `with_hidden` revwalks (counts are order-independent; handles no-common-ancestor). |
| `worktree_paths` | **gix** | `worktrees()` + main worktree; paths canonicalized to match `git worktree list` real paths. |
| `blame` | **gix** | `blame_file()`; **renamed-history files fall back to CLI** (gix blame lacks `-C`/`-M` rename following). |
| `commit_log`, `graph_commits` | CLI | gix 0.84 `rev_walk` has no topological sort → cannot match `git log --topo-order` on merge histories. |
| `status_counts` | CLI | gix status model (Rewrite renames, untracked-dir collapsing, conflict stages) does not match `--porcelain=v2` counts; also the mandated sparse/submodule fallback. |
| `diff_stats` | CLI | hot mode is worktree-vs-index `--shortstat`, not matchable without per-blob worktree diffing + binary/rename handling; fan-out already capped by the semaphore. |

The displayed unified diff/patch (`get_git_diff`), stash, reflog, and **all writes/auth stay on the CLI permanently** — they are not part of the port. The `gix` dependency uses `default-features = false` with only `["sha1","revision","status","blame","blob-diff","dirwalk","parallel"]` (pure Rust, no C toolchain).

## Monitoring Git Concurrency

Background repo-monitoring refreshes — `get_repo_summary_impl`, `get_repo_structure_impl`, and `get_repo_diff_stats_impl` — each fan out git subprocesses (worktree-list, `branch --merged`, per-worktree diffs). On a `repo-changed` burst across many registered repos this is unbounded and can spike concurrent git pipes past the OS file-descriptor limit (EMFILE) while flooding the main thread with IPC.

Each of these entry points acquires one permit from `AppState.monitoring_git_sem` (`MONITORING_GIT_CONCURRENCY = 8`) for the whole refresh, capping concurrent background refreshes to 8. Gating is per-function (not per-spawn) and deadlock-free because these entry points never call each other. **Operational git** (commit/push/stage/checkout/diff-on-click) is never gated — only monitoring work is throttled.

## Subprocess Helper (`git_cli.rs`)

Every git subprocess invocation goes through `git_cmd(cwd: &Path) -> GitCmd`. The builder provides three execution modes:

| Method | Use Case |
|--------|----------|
| `run()` | Strict — returns `Err(GitError)` on non-zero exit |
| `run_silent()` | Optional — returns `None` on any error |
| `run_raw()` | Full control — returns raw `Output` regardless of exit code |

`GitError` implements `Into<String>` for seamless use in Tauri command returns.

## Tauri Commands

### Repository Info

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_repo_info` | `(path: String) -> RepoInfo` | Get repo name, branch, status, initials |
| `get_git_branches` | `(path: String) -> Vec<Value>` | List all branches (sorted by rules below) |
| `check_is_main_branch` | `(branch: String) -> bool` | Check if branch is main/master/develop/trunk |
| `get_initials` | `(name: String) -> String` | Generate 2-char initials from repo name |

### Diff Operations

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_git_diff` | `(path: String) -> String` | Full git diff (staged + unstaged) |
| `get_diff_stats` | `(path: String) -> DiffStats` | Addition/deletion counts |
| `get_changed_files` | `(path: String) -> Vec<ChangedFile>` | List changed files with per-file stats (single subprocess call) |
| `get_file_diff` | `(path: String, file: String) -> String` | Diff for a single file |

### Repository Summary

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_repo_summary` | `(repo_path: String) -> RepoSummary` | Aggregate snapshot: worktree paths, merged branches, diff stats, timestamps |
| `get_repo_structure` | `(repo_path: String) -> RepoStructure` | Fast: worktree paths + merged branches only |
| `get_repo_diff_stats` | `(repo_path: String) -> RepoDiffStats` | Slow: per-worktree diff stats + last commit timestamps |

The frontend uses `get_repo_structure` (Phase 1) and `get_repo_diff_stats` (Phase 2) for progressive loading — UI rows appear immediately, stats fill in later. `get_repo_summary` remains for backward compatibility.

### Branch Operations

| Command | Signature | Description |
|---------|-----------|-------------|
| `rename_branch` | `(path, old_name, new_name) -> ()` | Rename a branch |
| `update_from_base` | `(path, branch) -> String` | Fetch base ref (if remote) and rebase branch onto it |
| `get_branch_base` | `(path, branch) -> Option<String>` | Read stored base ref from `git config branch.<name>.tuicommander-base` |
| `git_apply_reverse_patch` | `(path, patch) -> ()` | Apply a reverse patch for hunk/line-level restore |

## Data Types

### RepoInfo

```rust
struct RepoInfo {
    path: String,        // Repository path
    name: String,        // Repository name (from directory)
    initials: String,    // 2-char initials (e.g., "TC" for tuicommander)
    branch: String,      // Current branch name
    status: String,      // "clean", "dirty", or "conflict"
    is_git_repo: bool,   // Whether path is a git repository
}
```

### DiffStats

```rust
struct DiffStats {
    additions: i32,
    deletions: i32,
}
```

### ChangedFile

```rust
struct ChangedFile {
    path: String,       // Relative file path
    status: String,     // "M" (modified), "A" (added), "D" (deleted), etc.
    additions: u32,     // Lines added
    deletions: u32,     // Lines deleted
}
```

### RepoStructure

```rust
struct RepoStructure {
    worktree_paths: HashMap<String, String>,  // branch → worktree path
    merged_branches: Vec<String>,             // branches merged into default
}
```

### RepoDiffStats

```rust
struct RepoDiffStats {
    diff_stats: HashMap<String, DiffStats>,       // worktree_path → additions/deletions
    last_commit_ts: HashMap<String, Option<i64>>,  // branch → unix timestamp (seconds)
}
```

## Utility Functions

### `get_repo_initials(name: &str) -> String`

Generates 2-character initials from a repository name:
- Split on hyphens, underscores, dots, spaces
- If multiple words: first letter of first two words (e.g., "tuicommander" → "TC")
- If single word: first two letters (e.g., "react" → "RE")
- Always uppercase

### `is_main_branch(branch_name: &str) -> bool`

Returns `true` for: `main`, `master`, `develop`, `trunk`, `dev`.

### `sort_branches(branches: &mut [Value])`

Sorts branches by priority:
1. Currently active branch (always first)
2. Main branches (main, master, develop)
3. Open PR branches (alphabetical)
4. Feature branches without PRs (alphabetical)
5. Merged/closed PR branches (alphabetical, always last)
