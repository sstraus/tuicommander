# Empty GitHub sidebar investigation (2336-da0a)

## Scope and conclusion

Source audit: branch `fix/2336b-empty-sidebar-cause`, based on `5852fbcb7a`,
after the fixes merged in `a98d863a3`. This audit did not access the daily app,
credentials, or an authenticated test instance.

The historical `repos_monitored=0` does **not** prove that repository
registration was empty. Before `a98d863a3`, `compute_diagnostics` counted the
GitHub query cache. The historical empty sidebar remains unproven. Do not close
criterion 1 on the strength of the diagnostic fix alone.

## Three independent sets

1. **Configured paths:** `repositoriesStore.getActivePaths()` returns repository
   keys that are not parked (`src/stores/repositories.ts`). Linked worktree rows
   do not create extra keys unless the checkout is separately registered.
2. **Poll paths:** `poll_loop` owns `paths`, replaced by `UpdatePaths`
   (`src-tauri/src/github_poller.rs`). It starts empty until the queued config
   arrives. This is not the diagnostics gauge.
3. **Successful snapshots:** each successful PR result calls
   `process_repo_update`, which inserts the path into `previous_prs`, including
   an empty PR list. Current diagnostics count this history in the running
   poller. A configured path with no successful result does not count yet.

## Entry and exit trace

| Stage | Source and effect |
| --- | --- |
| Startup hydration | `hydrateStores` in `src/hooks/useAppBootstrap.ts` uses `Promise.allSettled`. `initApp` in `src/hooks/useAppInit.ts` catches hydration failure and later calls `deps.stores.startPolling()`. Repository-local config loads are fire-and-forget; they do not supply the polling path list. |
| Startup ownership | `startPolling` in `src/stores/github.ts` groups active paths by connection. It always starts a local group, even if empty. An unavailable remote group is skipped independently. A connected remote receives its own RPC. |
| Backend registration | Desktop `github_start_polling` and HTTP `poller_start` call `ensure_polling`. It starts one poller or reconfigures the existing one through `send_poller_config`. Reconfiguration also requests `ForceResync`. |
| Config delivery | `send_poller_config` queues `UpdatePaths`, filter, and draft settings through a bounded channel. Failed delivery logs a warning. `UpdatePaths` replaces paths and retires history and pending on-demand paths that are no longer configured. |
| Add, remove, park, external adoption | `add`, `remove`, `setPark`, and repository adoption in `src/stores/repositories.ts` send `github_update_paths` with the current active keys. This is separate from `.tuic.json` settings refresh. |
| Account selection | `get_all_batch_impl` in `src-tauri/src/github.rs` loads account metadata and bindings, then calls `resolve_repo_account`. An explicit valid binding wins; otherwise one remote/account candidate binds automatically. Ambiguous, no-account, and non-GitHub results are skipped. |
| Worktree binding | `RepoBindingStore::key` in `src-tauri/src/github_account.rs` uses `canonical_repo_root`, so linked checkouts share the main repository binding. Sidebar branch PR lookup uses the registered root path, not the worktree's ahead/behind path. |
| Poll eligibility | A missing credential, open account breaker, or repository cooldown skips the affected account/repository. The ambient account's request error returns an error for the batch; named account errors are isolated. An error does not erase existing poller history or frontend PR/issue rows. |
| Result adoption | `adopt_batch_response` in `src-tauri/src/github.rs` skips null aliases and inserts accessible PR arrays into both the payload cache and result map. `poll_batch` adopts the result into history and emits PR/issue events. |
| Frontend adoption | `ownsNotice` in `src/stores/github.ts` checks connection ownership before updating rows. `RepoSection.tsx` reads PRs and issues from `githubStore`; it does not consult `repos_monitored` to decide whether to show them. |
| Stop | `stop_poller` removes the running poller from state and signals its stop notification. Current diagnostics then report zero. |

Repository mutation updates currently send the full active path list to the local
poller, unlike owner-grouped startup and remote reconnect. This is a separate
ownership inconsistency, not evidence that the historical local set became empty:
with an active local repository, that list still contains its path. No transport
change is justified as the fix for this incident without a reproduction.

## Settings and plugin account binding are separate

The `repo_local` reader in `src/stores/repoSettings.ts` adopts `.tuic.json`
configuration. Before its isolation fix, a rejected remote read aborted that
refresh. It did not call `UpdatePaths([])`, stop the poller, or clear GitHub rows.
The startup polling list comes from `repositoriesStore`, not this reader.

The `github-work` plugin registers Tasks providers from its `work-items.json`
account entries (`plugins/github-work/main.js`). `accountFetch` in its
`runtime.js` forwards the provider account ID through `host.httpFetch`.
`accountCore` forwards that same ID for core PR actions and review threads.
These registrations do not call `github_start_polling` or modify the core poller
path set. A plugin without configured providers is not proof of an empty core
GitHub monitor set. Core repository bindings and plugin provider account entries
must be inspected separately.

## Why the old gauge could reach zero

At `a98d863a3^1`, `compute_diagnostics` used
`state.git_cache.github_status.entry_count()`. `state.rs` configures that cache
with `GITHUB_CACHE_TTL = 60s` through `build_git_cache(...).time_to_live(...)`.
`adopt_batch_response` writes it after successful PR-array adoption.

The poller's base visible interval is 60 seconds; its hidden interval is 120
seconds. After the 15-minute active window, hot idle repositories are selected
every fifth cycle and cold repositories every tenth cycle, with per-path jitter.
Backoff can also extend the interval to 300 seconds. Therefore the cache is not a
valid registration set: payloads can expire before the next successful write,
even with a healthy hidden poller. Failed requests can extend that empty period.
No path-removal command is required.

The existing RED log
`~/Gits/.tmp/results/tuic-2336-gauge-red.log` records
`monitored_repository_survives_query_cache_eviction` failing with 0 instead of 1
on the pre-fix revision recorded in the story, `af3be3658`. This test explicitly
invalidates the payload cache and calls the production successful-update seam;
it is **not** a replay of the historical registration sequence or a real network
failure. The recorded runtime worklog separately reports a network-down poll
failure and later reports 19 PRs, 5 issues, and a visible sidebar badge while the
old diagnostics remained zero. Those observations contradict treating zero as
proof of an empty frontend or an empty poll path list.

`8c07925d91`, included in `a98d863a3`, changes the gauge to poller history, so
cache expiration no longer removes successful monitoring identity.
`5d817958f7`, also included in that merge, prunes queued removed paths and ignores
late unconfigured `PollRepo` requests. Its RED observed **2 instead of 1** in
`tuic-2336-fix-red3.log`. It prevents ghost monitors, not an empty set.

## What is still missing

To prove the historical sidebar cause, capture these observations together while
the sidebar is actually empty, using only an authorized isolated instance:

- The loaded frontend module's active repository keys, parked flags, connection
  owners, effective PR/issue filters, and per-root PR/issue rows. A separately
  imported duplicate store is not evidence about the rendered store.
- The delivered start/update path list and the poller's current path list, plus
  per-path account resolution **without credentials**. Current diagnostics expose
  successful-history count, not the full configured/delivered path list.
- The next actual batch's completion or error and whether accessible aliases
  contain PR/issue arrays, followed by event emission and frontend adoption.
- A screenshot of the same empty sidebar state and its subsequent recovery.

These observations distinguish empty registration, unresolved account, request
failure, successful empty response, event loss/ownership rejection, and rendering
filters. A login status or a zero gauge alone cannot distinguish them. A
successful controlled outage/recovery test would prove recovery behavior, but
would not establish the historical cause unless it reproduces the same failure.

The 2026-10-26 authenticated check reused an existing login on the isolated
`agent-test-ghlive` instance. Its retained evidence showed an unreachable remote
beside a local repository with 2 PRs, 5 issues, and an active monitor count of 1.
That proves coexistence, not a fresh successful GraphQL poll or controlled
outage recovery. Keep criterion 1 and the full authenticated human check open.
Do not infer the missing state from the removed-path fix or from the disconnected
remote settings warning.

## Closure reconciliation (2026-10-27)

### Proven causes and their limits

- **Settings refresh:** before `af3be3658`, the `repo_local` reader used one
  rejecting `Promise.all` across repository reads. One rejection prevented
  adoption of every healthy sibling's new `.tuic.json` value. Direct failed
  reloads also replaced the last successful value with `null`. The recorded
  pre-fix failures are `local config refresh isolates failed repositories and
  later recovers` and `direct local config reload preserves the last successful
  value on failure`. The existing coordinator GREEN ran the settings store and
  coordinator files: 14 tests passed in 1.12 seconds. Neither test asserts
  GitHub poller registration, diagnostics, or sidebar rows.
- **Misleading status count:** the old payload-cache gauge could expire while a
  repository remained configured. The recorded
  `monitored_repository_survives_query_cache_eviction` RED failed with 0 rather
  than 1. This proves the diagnostic defect, not the historical empty sidebar.
- **Historical empty sidebar:** no retained reproduction establishes its cause.
  The independent defects above must not be relabelled as that cause.

### Remote settings failure versus local GitHub behavior

The additional store-boundary test in `src/__tests__/stores/github.test.ts`,
`local GitHub monitoring and sidebar data survive a remote settings read failure`,
uses the real settings refresh coordinator and the real GitHub store. It seeds
local and remote config entries, makes the remote config transport reject,
and lets the refresh finish. It then checks the local poller start command and
local PR/issue event adoption while the disconnected remote is not started.
Its PR/issue inputs are internal event-contract fixtures, not a captured GitHub
response or a network/authentication test.

The test **passes before the settings fix**: the three relevant production files
(`repoSettings.ts`, `settingsRefresh.ts`, and `github.ts`) were temporarily
replaced with their versions at `5eec9fc47^`
(`2d29b47127088429b6cca9e2afda04ddfcf26b28`), then restored byte-for-byte. The
new test ran alone through build-slot and `scripts/with-test-tmp.sh`: 1 passed,
39 filtered out, duration 963 ms. Retained log:
`~/Gits/.tmp/results/tuic-2336-close-prefixed.log`.

This is a counterexample to the proposed causal link at this boundary, not RED
evidence. Forcing that test to fail by awaiting a rejected refresh outside its
production coordinator would test a different pipeline. No new production fix
is justified. The literal RED criterion remains open; the local-isolation
behavior is supported by this boundary test and the retained native evidence.
The historical missing state and the human outage/recovery check remain open.

### Mac-mint HTTP 403 Untrusted Host

`RequestBoundary::allowed_host` in
`src-tauri/src/mcp_http/request_boundary.rs` parses Host (or HTTP/2 authority).
It accepts localhost, loopback/private IPs, detected local IPs, the machine's
current hostname/`.local` form, and the current Running Tailscale FQDN or its
short name. `check` returns HTTP 403 with `Untrusted Host` when that predicate
fails, before authentication. Login cannot fix this rejection.

Thus the reported FQDN was outside the remote server's accepted authority set
at rejection time. The record does not establish that server's deployed
revision, detected hostname, or Tailscale state. In particular, it does not prove
whether Tailscale was stopped, detection was stale, or the address differed from
the detected FQDN. Existing tests cover own versus foreign hostname, Running
Tailscale short names, and late Tailscale startup recovery. No new defect is
proven and no speculative story is filed.
