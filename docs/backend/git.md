# Git Operations

**Modules:** `src-tauri/crates/tuic-git/src/{git,git_cli,git_graph,git_locks,git_reads,worktree,cow}.rs` (domain), `src-tauri/src/{git,git_graph,worktree}.rs` (app adapters)

The `tuic-git` crate owns Git subprocesses, reads, branch operations, worktree operations, and artifact warming. It depends on `tuic-core` for path spelling. The root app retains Tokio scheduling, `AppState` caches, configuration lookup, Tauri commands, and event emission; its adapters re-export the moved domain items at their former crate paths. The domain crate has no normal Tokio or Tauri dependency. The root `worktree.rs` adapter also fetches merged GitHub PR evidence through `gh` and passes the response to `tuic-git` for SHA and ancestry verification. Workspace removal and MCP `branch_delete` both use that boundary.

Worktree removal verifies checkout and submodule safety before asking Git to unregister it. Git can return `Directory not empty` after removing the registration and some files. In that state, the removal call finishes deleting the known checkout directory and only then compares and deletes the branch ref. A failed cleanup reports the remaining path; an unregistered directory without TUIC ownership evidence is never recursively deleted.

Git **writes** are performed by shelling out to the `git` CLI via the unified `git_cli` module. Git **reads** go through the reversible `GitReads` port (see below), which serves some ops from in-process gix and the rest from the same CLI. The `git_cli::git_cmd(path)` builder provides consistent error handling, binary resolution, and credential prompt suppression across all callsites.

Windows checkout comparisons use `tuic-core::path_spelling::portable_spelling`: native and verbatim paths match Git's forward-slash paths. Unix backslashes remain literal filename bytes. Git subprocesses enable `core.longpaths` on Windows, including deep submodule preservation refs. Worktree scripts use the same enriched PATH as Git subprocesses; the Windows search includes the standard Git installation and its `.exe` filename. Hook PATH entries use native Windows separators, put the resolved Git directory first, and remove case-insensitive duplicates. The hook keeps whole directories in precedence order up to cmd.exe's 8191 UTF-16-unit environment-value limit; later directories beyond that limit are omitted.

## Async Execution & Caching

The root Git adapters run blocking domain operations inside `tokio::task::spawn_blocking`; the domain crate does not own a runtime. All Tauri git commands are `async` and run git subprocesses inside `tokio::task::spawn_blocking`. This prevents blocking Tokio worker threads during I/O-heavy operations like `git diff`, `git log`, or `git fetch`.

Git data is cached with a 60s TTL in `GitCacheState` (`state.rs`), one `moka::sync::Cache<String, Arc<T>>` per result type keyed by repo path. `moka`'s `get_with`/`try_get_with` **coalesce concurrent identical loads to a single computation** — replacing the previous hand-rolled `DashMap<String,(T,Instant)>` whose check-then-compute-then-set pattern had a TOCTOU race that let a `repo-changed` burst fan out N duplicate computes. `sync::Cache` is used (not `future::Cache`) because every loader is blocking git work run on the blocking pool; the sync `*_cached` helpers keep working without async (`git.rs::cached_get`/`cached_try` wrap the pattern). `github_repo_cooldown` stays a plain `DashMap` — it is a cooldown set, not a TTL value cache.

The `repo_watcher` (FSEvents on macOS, inotify on Linux) monitors the working tree with per-category debounce (Git/WorkTree/Config) and calls `invalidate_repo_caches()` on file system changes (which also clears the prompt `var_cache` for the repo), so git data refreshes immediately instead of waiting for TTL expiry. On macOS/Windows it registers a single recursive watch (near-zero cost at the OS level); on Linux it splits into pruned non-recursive watches over the working tree (skipping `ALWAYS_EXCLUDED_DIRS` and gitignored paths, adding watches for newly created dirs from the event callback) plus targeted `.git` watches (root non-recursive for HEAD/index/sentinels, `refs` and `worktrees` recursive — never `objects`/`logs`), because a recursive inotify watch would walk and watch every subtree (`node_modules`, `target`, `.git/objects`) and flood the callback (issue #82). Each **linked worktree gets its own watch**: its working tree usually lives outside the repo root (the `Sibling`/`AppDir` storage strategies), so the root's watch never sees it, and the git-state fingerprint is computed from the main checkout's index + porcelain status, so a worktree-local edit leaves it identical and the emit is suppressed. Without those watches an agent editing a worktree produced no event at all and the branch's sidebar diff badge stayed stale until the user selected the branch. The roots come from `.git/worktrees/*/gitdir` (`linked_worktree_roots`) and are re-synced by `sync_worktree_watches` on every git-state change — worktree add/remove is part of the fingerprint, so it always rides an emit and needs no watcher restart. `classify_path` matches worktree roots *before* the repo root, so a worktree stored inside the repo (`.worktrees/`, `.claude/worktrees/` — usually gitignored) is not dropped as noise. The watcher respects `.gitignore` rules and hot-reloads them when `.gitignore` is modified. The 60s TTL serves as a safety net for missed watcher events. Most IPC calls for git data hit the cache (~0.2ms) instead of spawning a git subprocess (~20-30ms).

**Watcher-miss observability:** each cache's `moka` eviction listener increments a shared `ttl_fallbacks` counter only on `RemovalCause::Expired` (TTL aged out without the watcher invalidating first) — explicit invalidations do not count. A rising counter means the watcher likely missed events; it is surfaced in the `cpu_watchdog` HEALTH/CPU-SPIKE snapshots as `git_cache_ttl_fallbacks`.

Internal callers that need synchronous access use `_impl` suffixes (e.g. `get_diff_stats_impl`) to avoid double `spawn_blocking` nesting.

## GitReads Port (gix migration)

Read operations go through a reversible `GitReads` port (`src-tauri/crates/tuic-git/src/git_reads.rs`) so individual ops can be served by in-process **gix** (gitoxide 0.84) instead of shelling out, removing the process spawn + FD + stdout-parse cost on hot paths. `CliGitReads` delegates to the existing `git_cmd`-based functions; `GixGitReads` implements the same trait with a `moka` handle cache (`ThreadSafeRepository` per path → thread-local `Repository` per call). `GitReadsRouter` (the global `git_reads()`) dispatches each op to its backend via a per-op `PerOpBackend`.

**An op is flipped to gix only behind a byte-for-byte parity ("shootout") test** comparing gix output to the CLI on a fixture repo. Where gix 0.84 cannot match git's exact output, the op stays on the CLI.

| Op | Backend | Notes |
|----|---------|-------|
| `branches_detail` | **gix** | `references()` → shorten / peel / committer ISO8601 / author / summary / upstream. ahead/behind via the `ahead_behind` backend. |
| `ahead_behind` | **gix** | `rev_parse_single`, then **identical tips short-circuit to `(0, 0)` with no walk**; otherwise two `with_hidden` revwalks (counts are order-independent; handles no-common-ancestor). `branches_detail` asks this once per branch, and a branch level with its upstream is the common case, so the short-circuit is what keeps that fan-out off `O(branches x history)`. |
| `worktree_paths` | **gix** | `worktrees()` + main worktree; paths canonicalized to match `git worktree list` real paths. Warm status is attached after backend selection so CLI and gix return the same checkout data. |
| `blame` | **gix** | `blame_file()`; **renamed-history files fall back to CLI** (gix blame lacks `-C`/`-M` rename following). |
| `commit_log`, `graph_commits` | **gix** | gix has no built-in topo sort, so `gix_topo_order` reproduces `git log --topo-order` (Kahn seeded by commit-date) and `gix_decorations` reproduces `%D` byte-for-byte (reverse-refname order, `tag:` prefix, `HEAD -> branch`). `author_date` UTC is normalized to git's `Z`. |
| `status_counts` | **gix** | `repo.status()` items mapped to staged/changed counts (TreeIndex = staged; IndexWorktree Change/IntentToAdd/untracked/conflict = changed; `NeedsUpdate` skipped). **sparse-checkout / submodule → CLI fallback.** |
| `diff_stats` | **gix** (worktree) | unstaged worktree-vs-index `--shortstat` via per-blob `imara` (Myers + slider), binary excluded. Staged (`--cached`) and commit (`hash^..hash`) modes → CLI; sparse/submodule/error → CLI. |

**All 8 read ops are served by gix**, each gated by a byte-for-byte shootout test; the gix adapters fall back to the CLI internally for their unsupported edge cases (sparse/submodule, renamed-history blame, staged/commit diff). `Backend::Cli` is retained in `PerOpBackend` as a per-op rollback lever. The lever is temporary: after **2026-12-05**, if the shootout tests have stayed green and no op has been rolled back, `Backend::Cli` and its router arms are deleted (`CliGitReads` itself stays — the gix fallbacks and the shootout tests both use it).

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

### Deadlines

`.timeout(Duration)` kills the invocation when it outlives the deadline and returns
`GitError::TimedOut` (`run_silent()` turns that into `None`, with a warning). Without
it a git call waits forever, which is right for the local reads that dominate this
module and wrong for anything that can block on something outside this machine —
a network `fetch`, or a user-supplied setup script.

The deadline path pipes stdout/stderr, nulls stdin (matching `Command::output()`, so a
child cannot park on a read of the app's own stdin) and drains the pipes on two reader
threads, so a child that fills a pipe buffer cannot deadlock against our own wait. On
timeout the child is killed **and reaped** (no zombie), but the reader threads are
deliberately not joined: a killed git can leave a grandchild (credential helper,
`core.askpass`) holding the write end open, and joining would reintroduce the unbounded
wait the deadline exists to prevent.

Two deadlines are in force:

| Constant | Value | Applies to |
|----------|-------|------------|
| `git_cli::FETCH_TIMEOUT` | 180s | Every `git fetch`: `conflict_assist.rs` (base refspec + PR head), `github.rs` `local_pr_diff` (base, `refs/pull/N/head`, head fallback), `worktree.rs` `fetch_if_remote` |
| `worktree::SCRIPT_TIMEOUT` | 900s | The user's worktree setup and archive/delete scripts, via `run_shell_script` |

Both are deliberately generous. A fetch is the only git call that waits on something
off this machine (`GIT_TERMINAL_PROMPT=0` stops git's own prompt but not a blocking
`credential.helper`, a half-open TCP connection or a wedged mount); 180s clears a
dual-stack connect timeout, so an unreachable host still reports git's own error rather
than ours, and leaves room for a large branch delta on a slow link. The script deadline
is not there to bound how long a build may take — 900s sits above any plausible
cold-cache install-and-build — but to end the script that will never finish. Killing
work that would have succeeded is a worse outcome than waiting for it.

### Stale `index.lock` reclaim

`git_cmd()` reclaims a `.git/index.lock` left by a crashed process, gated by **two**
independent checks — age **and** ownership:

1. **Age** (`is_index_lock_stale`): a 0-byte lock is stale after 5s (git crashed
   before writing the new index); a non-empty one after 30s (index written, rename
   never happened).
2. **Ownership** (`probe_index_lock_owner`): `lsof -w -t -- <lock>`. If any live
   process still holds the lock open, it is kept whatever its age. Age alone cannot
   tell a crashed git from a merely slow one, and on a large monorepo an `add`/`stash`
   index write can outrun the threshold — deleting the lock under it corrupts the
   index.

The lock is located through `resolve_git_dir()`, not by joining `.git` to the cwd. In
a linked worktree `.git` is a **file** holding `gitdir: <path>`, so the naive join
traverses a file and the sweep returned before looking at anything — inert in every
worktree, which is where most of TUIC's work happens.

The `lsof` fork happens only for a lock the age rule has already condemned, never on
the hot path of an ordinary git call, and it carries a 2s deadline of its own — it
runs inside `git_cmd`, so an `lsof` stuck on a wedged network mount would otherwise
wedge every git call in the app.

The probe answers with a `LockOwnership`: `HeldBy(pids)` keeps the lock, `Unowned` is
the only outcome that licenses a delete, and `Unknown` carries **why** it has no
answer — `Unavailable` (no `lsof`, exec refused, an `lsof` that ran and failed,
non-unix) or `DeadlineExceeded` (installed, working, slower than the deadline).

`classify_owner_probe` reads that answer from **stdout and stderr, never the exit
code**. Measured on macOS lsof 4.91, "nobody has this file open" and "lsof could not
stat the path" both exit 1 with empty stdout; only stderr separates them, and `-w`
does not hide it (it silences warnings, not status errors). So: PIDs win outright — a
complaint never outranks an answer, and `HeldBy` is the safe direction; otherwise a
non-empty stderr is a probe that ran and *failed*, which is `Unknown`; silence with no
PIDs is the real "no match". Letting a failed probe land in `Unowned` would be a
fail-open with no log and no name, which is the failure this whole enum exists to
prevent.

Both `Unknown` arms **fail closed** (Boss's decision, 2026-09-07, #694-4fcc): with no
answer the lock is **kept**, logged at `warn`. The two costs are not symmetric — a
stranded repo is recoverable by hand, a corrupted index is not — and the age rule
alone cannot tell a crashed git from a merely slow one. Measured `lsof` latency on
this hardware ranges 0.32s–3.7s against a 2s deadline, so `DeadlineExceeded` is a
routine outcome, not an exotic one; treating it as permission to delete was the
sharpest edge of the old policy.

"Kept" is not "kept forever". The escape hatch is a second, much stronger rule for a
lock **no probe can adjudicate**: age at `UNADJUDICATED_LOCK_STALE_SECS` (1 hour),
far above the 30s ordinary threshold. The hazard fail-closed guards against is a git
that is merely *slow*, and the slowest legitimate index write is seconds — a process
holding `index.lock` for an hour is dead, not slow. This matters most exactly where
the probe can never work: on a host with no `lsof`, `Unavailable` is permanent, and
without the hatch a crash-orphaned lock would outlive the process that left it and
nothing could ever clear it. Evidence still outranks age at every age — `HeldBy`
returns before the hatch is considered.

On Windows there is no probe at all (`Unavailable` always), so an `index.lock` is now
reclaimed only via the hatch. That is safe rather than merely tolerable: Windows
refuses to unlink a file another process holds open, so the OS enforces the same rule
the probe does on unix.

## Linked-worktree warming (`cow.rs`)

Every managed workspace is a linked Git worktree. After Git creates the clean
checkout, `cow.rs` asks Git for ignored directories and copy-on-write copies
those directories from the parent. It excludes every `.tmp` or `.mdkb` path component,
tracked paths, ignored files, nested repositories, and any directory containing
the destination. Tauri `bundle.externalBin` entries additionally select their
target-triple sidecar files by configuration, rather than by hard-coded names.
Each sidecar copy lands only in a new worktree path; clonefile creates a separate inode, and warming skips an existing destination. The source executable is never opened for writing by this path.
Submodules initialise from the parent checkout first (with the configured remote
as fallback), so unpublished pinned objects remain usable; failures are returned
as workspace warnings.

HTTP, MCP, and desktop IPC creation return while copy-on-write warming is pending. Their
instructions say to wait before running a build; `GET /worktrees/paths?path=<repo>`
and IPC `get_worktree_paths` report each workspace's `warm_artifacts.status`
(`pending`, `done`, or `failed`). HTTP and MCP run a configured setup script
before warming. Desktop IPC currently starts warming before its frontend setup
script, so those two operations can overlap. Pending is recorded before
the setup script starts. If creation is cancelled during setup, the status
becomes `failed` instead of remaining `pending`. Removing or archiving a
worktree waits for an active copy, clears its warm state, and prevents a queued
copy from recreating the old path. Before Git removes a worktree, the removal
path restores owner write permission only inside that checkout so a sealed
ignored build directory cannot leave a half-deleted, unregistered worktree.
Symlinks are not followed. A leftover path without Git registration is
reported with its path instead of being treated as a successful removal.

If Git has already unregistered a checkout but its directory remains, removal
still clears a pending warm token before attempting directory cleanup. A path
without a Git registration or a TUIC warm token is left untouched.

Archiving refuses a locked or missing checkout before moving it. It renames the
checkout into `__archived`, runs `git worktree repair`, and repairs initialized
submodule gitfiles and `core.worktree` paths. The Git administration directory,
HEAD, and reflogs remain registered; the workspace mapper hides archived paths
from active UI listings. A repair failure rolls the checkout back to its old
path. The archive path is never unlocked and never pruned.

Orphan pruning accepts only a registered detached linked checkout with no Git
operation in progress. Immediately before removal it checks detached HEAD
reachability against all durable refs with `git for-each-ref --contains`; a
commit reachable only from the worktree HEAD/reflog is kept for recovery.

The Ask-mode cleanup dialog uses a separate automatic-removal assessment:
tracked and untracked changes, and commits reachable only from tags, make the
checkout unsafe for the countdown. Ignored files are allowed. The same check
runs again for an agent's remove answer and immediately before an automatic
removal; the dialog lists each unsafe reason.

Non-force removal first requires a clean checkout and submodules, no Git
operation in progress, and a HEAD matching the captured branch tip. Git needs
one `--force` to remove a populated submodule even when it is clean; TUICommander
uses it only after checking submodule status and rechecking dirtiness immediately
before removal. A separate, confirmed lock override bypasses the lock during
removal; dirty-file `force` alone does not bypass a lock. Before removal, every
initialized submodule's HEAD and refs are copied into preserved refs in the
main checkout's module repository under a unique namespace. Before fetching,
the destination must resolve as that initialized module's Git root inside the
main checkout, with a Git directory distinct from the superproject's. An
uninitialized module or a misplaced `.git` file stops removal without deleting
the source checkout. Every stash and
reflog tip also gets a durable preserved ref, including older stash entries and
commits that have fallen off a branch. A missing checkout's module bundle
includes reflog objects before Git removes its registration. If preservation
fails, removal stops. For a deinitialized module, the leftover Git directory is
located by its configured `.gitmodules` name, including nested names that
differ from checkout paths; any retained admin state stops removal. The checkout is checked again after preservation,
immediately before Git removes it. Removing one checkout does not prune unrelated missing worktree registrations. A missing registered checkout requires force confirmation before preserving module refs and pruning its registration; a lock still needs a separate override. The normal workspace list hides missing checkouts. An
uninitialized submodule without Git state is safe to remove. The lifecycle preflight identifies a missing registered checkout explicitly; desktop and HTTP removal require a separate missing-checkout confirmation and recheck that the directory has not reappeared. Force confirmation
can carry a fingerprint of checkout status per path, HEAD, and submodule refs, rechecked
under the removal lock. The status portion records Git's per-path porcelain
entries, not the contents of a file that was already dirty. Branch deletion in
either mode uses the captured OID in a compare-and-delete operation. If proof
fails or the branch moves, removal reports a warning and keeps the ref. For
branch deletion, it checks the default branch's ancestry. A clean branch
whose commits were squash- or rebase-merged can also pass when `git cherry`
reports no unique patches. `git cherry` and context-free twin comparisons do not cover merge resolution
changes; a virtual no-op merge can cover the final tree instead. The removal
result names the matching rule.

The checked-out main worktree branch is another integration target: if it
contains the candidate tip, removal records `integration_ancestry` even when
the remote default branch is behind. Otherwise, a merged GitHub PR can prove
a squash merge when its fetched head contains the local tip. Open or closed
unmerged PRs, mismatched heads, and unavailable API data never prove deletion.
MCP `branch_integrations` and `branch_integration`, the branch panel's merged
set, and workspace lifecycle share `classify_branch_merge` in `tuic-git`.
The classifier also recognises clean virtual merges whose tree equals the
default target, and exact same-subject twin patches with unchanged added and
removed lines. A squash message is corroborating evidence only. The
`content_superset` heuristic requires an exact-tip archive before deletion;
worktree removal rechecks it after the archive hook. Safe branch-panel
deletion uses this classifier and compare-and-delete instead of Git's
upstream-dependent `branch -d`. Explicit force deletion retains `branch -D`.
See [the fixture provenance](branch-integration-fixtures.md) for audit cases,
thresholds and known limits.

Keeping the branch skips branch-deletion proof while retaining dirty-work,
submodule, operation, and lock checks.

MCP `repo action=branch_delete` applies the same merge classification to a
local branch with no checkout. It refuses the current and default branches,
checks every worktree record, and accepts ancestry in the checked-out
integration branch or patch equivalence against that branch when GitHub PR
proof is unavailable. The ref is removed with its captured OID as the expected
old value, so an advanced branch remains intact. Remote refs are untouched.

`probe_cow_support` performs a real copy against the source/destination pair so
an unsupported filesystem produces one warning instead of one failure per
ignored directory. The probe and copy primitive share `COW_COPY_FLAGS` (macOS
`cp -c`, then GNU `cp --reflink=always`); neither may silently fall back to a
byte-for-byte recursive copy. Warming is best-effort: failure leaves a complete,
valid, cold linked worktree.
After each copy, warming adds owner write permission to cloned files and
directories. It does not follow symlinks or change the source checkout, so
sealed ignored evidence remains available and the new worktree stays removable.

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
| `get_repo_summary` | `(repo_path: String) -> RepoSummary` | Aggregate snapshot: worktree paths, merged branches, diff stats, timestamps, workspace lifecycle |
| `get_repo_structure` | `(repo_path: String) -> RepoStructure` | Fast: worktree paths + merged branches only |
| `get_repo_diff_stats` | `(repo_path: String) -> RepoDiffStats` | Slow: per-worktree diff stats, last commit timestamps, and workspace-id lifecycle verdicts |

Lifecycle is backend-authored from the exact checkout `HEAD`. Linked worktrees
report dirty state, default-branch ancestry, and removal safety. Any failed check
serializes as `unknown`, never as clean or safe.
When branch deletion is requested, worktree removal checks this verdict before
touching the checkout. Without force, an unmerged branch or unknown verdict
stops the combined operation and leaves the worktree in place. With force,
removal may discard dirty checkout files, but an unmerged branch is retained
with a warning.

The frontend uses `get_repo_structure` (Phase 1) and `get_repo_diff_stats` (Phase 2) for progressive loading — UI rows appear immediately, stats fill in later. Refresh is single-flight per repository: concurrent requests join the active run and coalesce into one trailing rerun. This guarantees that sustained filesystem events cannot repeatedly cancel Phase 1 and leave deleted worktrees in the persisted sidebar cache. `get_repo_summary` remains for backward compatibility.

### Branch Operations

| Command | Signature | Description |
|---------|-----------|-------------|
| `rename_branch` | `(path, old_name, new_name) -> ()` | Rename a branch |
| `update_from_base` | `(path, branch, strategy?) -> String` | Fetch base ref (if remote) and rebase or merge the branch onto it. On conflict, reports `(aborted)` only after `git rebase/merge --abort` succeeds; if abort fails, the error says the repo may still be conflicted and includes the manual abort command. |
| `start_conflict_assist` | `(repo_path, pr_number) -> ConflictAssistResult` | Creates a PR-head worktree and rebases it. A conflict-free result is `clean` only after a successful origin refresh; stale tracking or local fallback results are `clean_unverified` with `base_source` and `base_warning`. |
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

`status` is the **same verdict the git panel shows**: `get_repo_info_impl` reads it
from `git_reads().status_counts()`, so the repo badge no longer forks a
`git status --porcelain` of its own beside the panel's `--porcelain=v2` read. One
status source per repo, one set of semantics (`status_counts_from_porcelain_v2` in
`git.rs`): `conflict` when a porcelain-v2 `u ` (unmerged) record is present, `dirty`
when anything is staged or changed, `clean` otherwise. Codes are read positionally,
so a file named `UUID.md` does not mark the repository as conflicted.

A repo git cannot read reports `unknown`, never `clean` — a green tick on an
unreadable repo is a silent failure.

**Still separate:** the repo watcher's git-state fingerprint (`repo_watcher.rs`)
runs its own `status --porcelain` v1, and `get_working_tree_status` runs its own
`--porcelain=v2 --branch --show-stash` because it needs the per-file entries that
`StatusCounts` discards.

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

## Workflow integration receipts

The workflow service checks both merge parents and compares the canonical merge tree with `git merge-tree --write-tree` for those exact parents before running pinned post-integration checks. A merge that adds unrelated content or drops checked source changes cannot receive an integration receipt. This verification requires Git >= 2.38; older installations receive an explicit version requirement. A conflicted merge is rejected with an explicit request for human review or a separately verified artifact; the current receipt path does not grant either exception.

The `run_git_command` IPC command and `/repo/run-git` HTTP route share a subcommand and flag allowlist. Each argument is checked before Git starts; caller-controlled configuration, executable helpers, and output paths are rejected. Network commands share the 180 s `FETCH_TIMEOUT`. Supported UI flags are fetch `--all`, pull `--ff-only`, push `-u`/`--delete`, diff `--name-status`, and status `--porcelain`.

Force branch deletion first preserves the exact tip at `refs/archive/<branch>` and uses an expected-old-tip ref deletion. If `refs/archive/<branch>` already holds a different tip, the tip is archived at `refs/archive/<branch>-<sha7>` instead; an archive already at the same tip is reused. A checkout of the branch refuses deletion.

Stale creation recovery deletes only unregistered directories without a Git checkout. A sanitized-name collision with a registered worktree fails, preserving clean, dirty, and detached checkouts. Plain orphan directories can be recreated as worktrees.
