# 2098: checkout-scoped monitoring status cache (implementation contract)

**Contract:** sidebar diff totals and dirty/removal indicators stay current after external edits and Git operations; a change in one checkout does not scan unchanged siblings. Destructive removal preflights always read fresh state. Coordinator/advisor approved Step A then Step B with the safeguards below.

## Evidence and consumers

`src-tauri/src/git.rs::get_repo_diff_stats_impl` calls `GitReads::diff_stats(path, None)` and `inspect_workspace_monitoring_with_key` for every workspace. The former calls `GixGitReads::gix_diff_stats_worktree`; the latter calls `GixGitReads::dirty_files` (CLI fallback: `dirty_files_at`). Both run a full status iterator. The diff counts unstaged tracked lines; dirty state counts unique staged/modified paths plus untracked records. They are not interchangeable. Sparse/submodule/error fallbacks retain current semantics.

Equal-refresh traces at baseline `201667ab81` and after `e3195408d3` show all 4,096 linked tracked paths stat'ed twice per refresh, twelve refreshes: **98,304 tracked-file lstats in each run**. Changing-repo pathname events fall only 14,814 → 11,431.75 per refresh. Narrow config/ref open savings do not meet aggregate 10x. See retained `isolated-main-comparison.md` under `~/Gits/.tmp/2098-evidence`.

## Smallest change and ownership

**Step A first:** merge the monitoring diff/dirty traversals into one gix status pass with `UntrackedFiles::Files`. Untracked records affect only dirty count. An `any_error` flag makes dirty state use its existing CLI fallback while diff retains its existing partial-count behavior. Protect parity and one traversal per refresh with a real Git regression and actual traversal counter, then targeted RED/GREEN.

**Step B:** add one bounded, single-flight **monitoring-only** snapshot per checkout: `(DiffStats, Result<dirty_count>, generation)`. `get_repo_diff_stats_impl` obtains the pair and passes the dirty result into a monitoring-specific lifecycle helper; merge proof still uses the current ref snapshot. Compute misses through the Step A shared status pass; preserve the two consumers' different error semantics. Do not add a per-file index. Cache successful results only. Explicit file panels, click-time scoped diffs, listing safety checks and removal preflights do not use this cache. Keep existing repository aggregate cache/coordinator; broad aggregate invalidation must not erase unrelated checkout snapshots.

The existing repo watcher owns checkout/admin-path mapping and emits **invalidation in the raw callback, before classification, ignore filtering and debounce**. Match the main root or a `worktree_snapshot` root; tracked files under ALWAYS_EXCLUDED or gitignored directories still affect status. Map `.git/worktrees/<name>/index` via its `gitdir` file, not `monitoring_paths_changed`, which filters it out. Record a generation under the same snapshot lock; an event during computation prevents publication of that generation. Cap entries; eviction recomputes. Every snapshot expires after five minutes as a missed-event backstop. Unsupported gix repositories (sparse/submodules) never reuse snapshots: CLI status can write the index, and `.git/modules` is not completely watched on Linux. Arm watches before the initial scan; cache reuse requires successful, complete watching for that checkout. Watch-stop/restart changes the generation and disables reuse until re-armed.

## Invalidation contract

| Input | Invalidated monitoring entries |
|---|---|
| Working-tree create/write/delete/rename, directory move, attributes/ignore source in checkout | Owning checkout; both rename paths, before ignore filtering (an ignored path can become visible) |
| Checkout index, HEAD, operation sentinels, own local branch target | Owning checkout; map linked admin paths to their checkout |
| Common refs to another branch / remote / packed-refs | Refresh integration proofs, not dirty snapshots; invalidate checkouts whose own tip changed |
| Shared config, include/config.worktree, attributes, global excludes/ignore sources | Affected checkout, or all owned checkouts when scope cannot be proved; retain cheap source-signature validation for sources outside watched roots |
| Unknown working-tree path outside mapped roots, pathless `need_rescan`, watcher error, partial registration, root/day-equivalent directory replacement | Invalidate affected group (all if unknown); disable reuse and use existing fresh path until complete watches are re-armed |

Read-only access and unrelated object/log writes do not invalidate dirty state. Git commands that alter status explicitly invalidate their checkout as well as relying on filesystem events. An empty commit can invalidate its own checkout, never every sibling.

## Correctness and verification

External editor/Git writes are authoritative watcher inputs. Overflow/rescan/error must be processed **before** access/path filtering. No event stream can prove a silently omitted event; do not claim otherwise. A healthy complete watcher is the same freshness assumption as current panel updates. Without that authority use fresh reads; the five-minute TTL bounds undetected loss only while watching is trusted. Signature checks cover out-of-tree config sources, not every tracked file.

RED names: sibling commit rescans unchanged checkout status; external edit/rename or linked-index change returns stale dirty/diff values; event during a scan publishes stale values; lost/partial watch reuses stale state. Use real Git/watcher fixtures and observable line totals/dirty state, plus actual scan-boundary counters. Check staged+unstaged overlap, untracked files, nested worktrees, sparse/submodules, checkout removal/replacement, config/ignore changes and fresh destructive preflight. Re-run the approved equal-refresh workload with twelve verified current timestamps; require measured aggregate 10x before checking GREEN. First scan remains O(all files); one changed checkout remains O(its files), not O(changed paths). Add `--no-optional-locks` to `repo_git_fingerprint`'s main `git status --porcelain` and include that scan in measurements. The fixture has 256 tracked files in its main checkout plus 4,096 across sixteen linked checkouts, not 4,096 in main alone. Main commits legitimately miss main status and change integration keys. If the aggregate cannot reach 10x after eliminating sibling scans, report the measured floor as a QUESTION; only Boss can change acceptance.

## Platform coverage decision

Snapshot reuse is disabled in Linux production. Its pruned directory watches cannot prove coverage of tracked paths inside ignored/excluded directories. This change does not widen those watches. macOS/Windows require a canonical main checkout, a successful recursive main watch and successful recursive registration of each linked root. Watch loss revokes authority until restart; refreshing the inventory does not re-arm existing untrusted roots. Tests may use fully recursive watches over their small Linux fixtures without enabling the production cache there.

Normal new linked-admin inventory paths do not revoke existing watches. A new checkout cannot affect existing siblings' dirty snapshots, and its first read is fresh until inventory sync arms its own watch. Do not infer watch loss merely because the bounded cache evicted its main entry: while main is present, its admin prefix masks that over-conservative predicate, so the regression must also exercise main-entry eviction.

Windows reports generic metadata modifications for existing Git and working-tree directories. The shared `fs::is_directory_metadata_event` classifier treats `Modify(Any)` on an existing mapped directory as metadata, not a status change or directory replacement. This includes directories under the common `.git` and below registered checkout roots, without name-based ignore rules. Child file, index, HEAD, ref and config events still invalidate their existing scopes, including tracked files under ignored-looking directories. Checkout roots and unmapped paths retain conservative watch-identity handling. Missing/non-directory paths, create/remove/rename and rescan handling are unchanged. Native Windows run `40787712783` records a linked `target` directory metadata event during the initial status load; the event must not prevent publication of that unchanged snapshot. The trace does not identify the writer.
