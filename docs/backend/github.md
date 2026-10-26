# GitHub Integration

**Modules:** `src-tauri/crates/tuic-git/src/{github,github_account,github_auth,github_poller,circleci,pr_review,changelog,github_debug}.rs` contains GitHub data types and pure decisions. The matching modules under `src-tauri/src/` keep API clients, credential and config storage, polling, event emission, ego calls, and Tauri commands; they re-export the domain items at their existing paths.

Integrates with GitHub via GraphQL API for PR status, CI checks, and batch queries. Supports OAuth Device Flow login as an alternative to environment tokens, plus **multiple accounts** (additional github.com logins and GitHub Enterprise Server) with per-repo bindings.

Worktree deletion has a fail-closed, account-aware HTTP GraphQL proof through
`RepoApi::resolve` and `graphql_pages`. It uses the repository binding and host,
the shared HTTP client, and account-scoped breaker/rate-limit accounting.
Pagination must finish within 100 pages; errors, malformed page info, and repeated
cursors leave the lookup unknown and uncached. Synchronous Git callbacks bridge
through the Tokio runtime only from blocking tasks. When Git
ancestry does not reach the default or checked-out integration branch, it reads
merged PRs for the worktree branch and accepts one only when its recorded head
contains the local tip. Missing heads are fetched from `refs/pull/N/head` and
checked against the API SHA before ancestry is trusted. An API/auth/fetch
failure leaves the branch unproved; patch-equivalence remains a local fallback.
The MCP local-branch deletion action uses the same merged-PR evidence when a
branch has no worktree.

REST requests use `github::github_rest_request` with the host and token selected by
`resolve_repo_for_rest` (registry and per-repo binding) or by the account identity
flow. The builder pins `User-Agent: TUICommander`, the requested Accept media type,
and `X-GitHub-Api-Version: 2022-11-28`. GraphQL uses the same User-Agent. Each caller
keeps its status/error wording and existing circuit-breaker handling. A production
source guard (`production_code_spawns_no_gh`) fails when production code spawns `gh`; its allowlist is empty.

PR creation, issue creation, and review submission use account-aware `RepoApi::rest`
POST requests to `pulls`, `issues`, and `pulls/{number}/reviews`. The repository
binding selects the host and token, including GitHub Enterprise. These writes no
longer require the `gh` executable or pass tokens to child processes. Review inline
comments retain their side (default `RIGHT`). REST errors retain the HTTP status
and response body as `GitHub API failed ({status}): {body}`. IPC and HTTP request
and response shapes are unchanged.

## Multi-Account Model (`github_account.rs`)

The integration is **account-centric**: the primary key is a stable `GitHubAccountId`, not the host. This keeps github.com behaving exactly as before behind an "ambient default" account while enabling additional accounts.

- **`GitHubHost`** — canonical (lowercased, validated) host. `is_cloud()` → github.com; `graphql_url()` / `rest_base()` return `api.github.com` (+`/graphql`) for cloud and `https://{host}/api/graphql` / `https://{host}/api/v3` for GHE. `is_ambient_default()` routes the global-vs-per-account branch points.
- **Account kinds** — `GithubComOAuth` / `GithubComEnv` / `GithubComGhCli` (the ambient default, existing auth chain), additional named github.com accounts, and `GhePat` (GitHub Enterprise Server via pasted PAT).
- **Credential storage** — github.com keeps `Credential::GithubOauthToken` (`github/oauth-token`) unchanged; per-account PATs use `Credential::GithubToken(account_id)` → `github/account/{id}/token`.
- **Repo bindings** — `{repo_path → account_id, owner, repo, remote_name}` persisted per canonical repo root (worktrees resolve to the main root). `resolve_repo_account(repo_path)` returns `RepoResolution::{Bound | NeedsBind(candidates) | NeedsAccount | Unmonitored}` — binding-first, a single candidate returns `Bound` without persisting a binding, and multiple candidates return `NeedsBind` for every consumer (never a silent `origin` pick). Fork and upstream remotes on the same account still identify different repositories and require a remote choice.
- **Concurrent saves** — Binding maps use `ConfigFile::save_delta`, so changes to different repositories compose. Account additions and removals mutate the latest registry under `ConfigFile::update_with`; the on-disk array is never replaced from a stale account snapshot.
- **Per-account isolation (hybrid)** — github.com keeps the global breaker/viewer/rate/cooldown fields byte-for-byte; GHE accounts get isolated `ghe_state: DashMap<AccountId, GheAccountState>`. The poller groups repos by resolved account and runs one batch per account, so a fault on one never opens another's breaker. Cooldown keys: `owner/repo` (cloud, unchanged) vs `{account_id}:owner/repo` (GHE).
- **CI logs** — github.com and GitHub Enterprise use the repo-resolved account token and REST host. Actions runs, jobs and logs use the account circuit breaker. Log redirects remove bearer credentials when the storage host changes.

### Multi-account commands

Create Enterprise accounts through the core account manager and enter PATs through `account_secret_form`. The legacy PAT add command and `POST /github/accounts` route are removed; `GET /github/accounts` remains available.

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_list_accounts` | `() -> Vec<GitHubAccount>` | Additional accounts beyond the ambient github.com default |
| `github_remove_account` | `(id: String) -> ()` | Cascade-remove token + record + bindings + per-account caches |
| `github_bind_repo` | `(repo_path, account_id, remote_name) -> ()` | Persist a repo→account binding |
| `github_unbind_repo` | `(repo_path: String) -> ()` | Remove a repo binding |
| `github_list_bindings` | `() -> Vec<Binding>` | All persisted repo→account bindings |
| `github_resolve_repo` | `(repo_path: String) -> RepoResolutionDto` | `bound` / `needs-bind` / `needs-account` / `unmonitored` + candidates |

## Token Resolution

Priority order (first non-empty wins) for the ambient github.com account:

1. `GH_TOKEN` environment variable
2. `GITHUB_TOKEN` environment variable
3. OAuth keyring token (`github_auth.rs` — stored in OS keyring via `keyring` crate)

The active token source is tracked in `AppState.github.token_source` as a `TokenSource` enum (`Env`, `OAuth`, `Pat`, `None`), serialized as `env`, `oauth`, `pat`, `none` in both transports. `resolve_token_for_account(&GitHubAccount)` runs this exact chain for github.com and returns the vault PAT (`TokenSource::Pat`) for GHE accounts.

An explicit disconnect is held in runtime state under the token lock. Status refresh, GraphQL lazy resolution, deferred probes and in-flight fallback writes cannot reconnect the ambient account. A successful primary device-flow login re-enables it; named accounts stay independent.

## Tauri Commands — Authentication (`github_auth.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_start_login` | `() -> DeviceCodeResponse` | Start OAuth Device Flow, returns user code |
| `github_poll_login` | `(device_code: String) -> PollResult` | Poll for token, saves to keyring on success |
| `github_logout` | `() -> ()` | Delete OAuth token from keyring, fall back to environment tokens |
| `github_auth_status` | `() -> AuthStatus` | Current auth status with login, avatar, source |
| `github_disconnect` | `() -> ()` | Delete the ambient OAuth token and disable ambient runtime credential resolution until device-flow login or restart; environment variables remain unchanged |
| `github_diagnostics` | `() -> Value` | Diagnostics: token sources, scopes, API connectivity |

## Tauri Commands — GitHub Data (`github.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_github_status` | `(path: String) -> GitHubStatus` | Remote presence, current branch, ahead/behind counts |
| `get_ci_checks` | `(path: String, pr_number: i64) -> Vec<Value>` | Detailed CI check list for one PR |
| `get_pr_review_threads` | `(path: String, pr_number: i64) -> ReviewThreadCounts` | Unresolved review threads of one PR, bot vs human vs ego (GraphQL, 1 point per page). Ego threads start with a comment that holds `review_bots::EGO_MARKER`. The batch poll only carries `BranchPrStatus.unresolved_threads` |
| `get_repo_pr_statuses` | `(path: String, include_merged: Option<bool>) -> Vec<BranchPrStatus>` | PR status for every branch of one repo (TTL-cached unless `include_merged`) |
| `approve_pr` | `(repo_path: String, pr_number: i64) -> ()` | Submit approving review via the REST reviews endpoint |
| `update_pr_branch` | `(repo_path: String, pr_number: i64, expected_head_sha: String) -> ()` | Merge the base branch into the PR branch (REST `PUT /pulls/{n}/update-branch`, same operation as GraphQL `updatePullRequestBranch`), pinned to the head the UI showed; 422 head mismatch → `PR head changed` |
| `close_pr` | `(repo_path: String, pr_number: i64) -> ()` | Close a PR without merging (REST `PATCH /pulls/{n}` `state=closed`) |
| `get_all_pr_statuses` | `(paths: Vec<String>, include_merged: bool) -> HashMap<String, Vec<BranchPrStatus>>` | Batch PR status across many repos in one GraphQL call |
| `get_pr_diff` | `(repo_path: String, pr_number: i64) -> PrDiff {diff, headSha}` | Get PR diff content and the head commit it was read at (head read before and after the diff; a push in between is an error); falls back to a local-clone `git diff` when GitHub rejects oversized diffs |
| `merge_pr_via_github` | `(repo_path: String, pr_number: i64, merge_method: String, expected_head_sha: String) -> String` | Merge PR via GitHub API, pinned to the reviewed head (409 → `PR head changed`) |
| `fetch_ci_failure_logs` | `(repo_path: String, branch: String, check_url?: String, head_sha?: String) -> String` | Fetch failure logs for the local branch head during auto-heal or for a selected check at the PR head SHA |
| `create_issue_from_proposal` | `(repo_path: String, proposal: ImprovementProposal) -> CreatedIssue` | Explicit issue creation from a proposal; scan never creates issues automatically |

### Failing checks for auto-heal

Failing checks are read from the exact commit's `statusCheckRollup.contexts`,
through `RepoApi::resolve` and account-bound GraphQL with breaker accounting.
The lookup does not search open PRs. It combines up to 100 pages before keeping
the newest check per name, then returns failed CheckRuns and external
StatusContexts with their detail links. An incomplete response is an error;
a commit with a null rollup has no checks. CI job log retrieval remains a
separate operation.

### Circuit breaker coverage

**Every** call out to GitHub goes through the account's circuit breaker:
GraphQL via `graphql_with_retry`, PR/issue/review writes via `RepoApi::rest`, and direct
REST via `send_rest_with_breaker` (close/reopen issue, merge PR, approve PR,
`fetch_github_json`, PR diff, PR refs). The availability breaker counts transport
errors and `5xx` responses, not deterministic `4xx` caller outcomes such as a
missing issue, merge conflict, validation failure, or permission denial. Rate
limits use their separate backoff: `429`, primary-limit `403` headers,
`retry-after`, or a secondary/abuse-limit message in an otherwise ambiguous
`403` body. Non-rate-limit bodies remain available to caller-specific error
formatting.

### Cached viewer login

`state.github_viewer_login` backs `author:@me` in the viewer-PR search and the
assignee/creator/mentioned issue filters. It is dropped by
`github::invalidate_viewer_login` on logout, disconnect and a successful device-flow
login — without that, switching accounts kept showing the previous account's PRs
and issues for the rest of the session. Named accounts cache their own login in
`ghe_state` and are deliberately untouched by that invalidation.

## Tauri Commands — Polling (`github_poller.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_start_polling` | `(paths: Vec<String>, issue_filter: String, pr_hide_drafts: bool) -> ()` | Start/reconfigure the background poller |
| `github_stop_polling` | `() -> ()` | Stop the poller task |
| `github_set_visibility` | `(visible: bool) -> ()` | Switch between the visible and hidden poll intervals |
| `github_poll_repo` | `(path: String) -> ()` | Request a debounced one-off poll of a single repo |
| `github_update_paths` | `(paths: Vec<String>) -> ()` | Replace the polled repo set |
| `github_set_issue_filter` | `(filter: String) -> ()` | Change the issue filter mode live |
| `github_set_pr_hide_drafts` | `(hide: bool) -> ()` | Toggle draft-PR filtering live |

The diagnostics `repos_monitored` gauge counts repository paths with successful
PR snapshots in the running poller's transition history, including empty snapshots.
It does not count entries in the TTL query cache. Cache expiry and failed poll ticks
do not remove monitoring identity. Removing a configured path drops its transition
and change-detection history and queued on-demand requests. Late on-demand requests
for unconfigured paths are ignored, so completed asynchronous actions cannot restore
a retired monitor. Stopping the poller resets the gauge to zero. A new
poller starts with no successful snapshots, even if old query payloads remain cached.

For the distinction between configured paths, successful monitoring history, and
rendered sidebar data, see the [2336 investigation](github-monitoring-investigation-2336.md).
It records why the old TTL-cache gauge could report zero without deregistration
and which observations are still required to prove the historical empty sidebar.

## Data Types

### GitHubStatus

```rust
struct GitHubStatus {
    has_remote: bool,
    current_branch: String,
    ahead: i32,
    behind: i32,
}
```

PR and CI data are **not** on this type. They arrive through `BranchPrStatus`, which
the batch endpoints return per branch.

### BranchPrStatus (Batch Endpoint)

Full PR data for a single branch, returned by `get_repo_pr_statuses`:

```rust
struct BranchPrStatus {
    branch: String,
    number: i32,
    title: String,
    state: String,
    url: String,
    additions: i32,
    deletions: i32,
    checks: CheckSummary,        // passed/failed/pending/total
    author: String,
    commits: i32,
    mergeable: String,           // "MERGEABLE", "CONFLICTING", "UNKNOWN"
    merge_state_status: String,  // "CLEAN", "DIRTY", "BEHIND", etc.
    review_decision: String,     // "APPROVED", "CHANGES_REQUESTED", etc.
    viewer_did_approve: bool,    // Viewer's own latest review is APPROVED
    labels: Vec<PrLabel>,        // Labels with pre-computed colors
    is_draft: bool,
    base_ref_name: String,
    head_ref_oid: String,
    created_at: String,
    updated_at: String,
    merge_state_label: Option<StateLabel>,   // Pre-classified display label
    conflict_state: ConflictState,           // The single conflict verdict both surfaces render
    review_state_label: Option<StateLabel>,  // Pre-classified display label
    merge_commit_allowed: bool,  // Repo-level merge settings
    squash_merge_allowed: bool,
    rebase_merge_allowed: bool,
}
```

### PrLabel

```rust
struct PrLabel {
    name: String,
    color: String,            // Hex color from GitHub
    text_color: String,       // Computed: black or white based on luminance
    background_color: String, // Computed: hex_to_rgba with alpha
}
```

### CheckSummary

```rust
struct CheckSummary {
    passed: u32,
    failed: u32,
    pending: u32,
    total: u32,
}
```

### StateLabel

```rust
struct StateLabel {
    label: String,     // Human-readable text (e.g., "Approved", "Behind")
    css_class: String, // CSS class for styling
}
```

## Utility Functions

### `parse_pr_node(v: &serde_json::Value) -> Option<BranchPrStatus>`

Parses one PR node out of the batched GraphQL response and enriches it with computed
fields (merge state classification, conflict verdict, review state classification,
label colors).

### `classify_merge_state(mergeable: Option<&str>, merge_state_status: Option<&str>) -> Option<StateLabel>`

Delegates the conflict question to `classify_conflict_state` first, then maps the
merge state to a display label:

| Condition | Label | CSS Class |
|-----------|-------|-----------|
| `ConflictState::Conflicting` | Conflicts | conflicting |
| `ConflictState::Checking` | *(no chip — GitHub is still recomputing)* | — |
| CLEAN | Ready to merge | clean |
| BEHIND | Behind base | behind |
| BLOCKED | Blocked | blocked |
| UNSTABLE | Unstable | blocked |
| DRAFT | Draft | behind |
| DIRTY | Conflicts | conflicting |
| UNKNOWN, HAS_HOOKS | *(no chip)* | — |

### `classify_review_state(review_decision: Option<&str>) -> Option<StateLabel>`

| review_decision | Label | CSS Class |
|-----------------|-------|-----------|
| APPROVED | Approved | approved |
| CHANGES_REQUESTED | Changes requested | changes-requested |
| REVIEW_REQUIRED | Review required | review-required |

### `hex_to_rgba(hex: &str, alpha: f64) -> String`

Converts a **bare** 6-char hex color (e.g. `"ff0000"`, no leading `#` — that is what
GitHub's label API returns) to an rgba string (e.g. `"rgba(255, 0, 0, 0.5)"`). Invalid
input parses as `(0, 0, 0)`.

### `is_light_color(hex: &str) -> bool`

Computes BT.601 luma (`(r*299 + g*587 + b*114) / 1000 > 128`) on the same bare hex to
decide whether a label needs dark text.

## Tauri Commands — Issues

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_all_issues` | `(paths: Vec<String>, filter_mode: String) -> HashMap<String, Vec<GitHubIssue>>` | Fetch issues for multiple repos in one batched GraphQL call |
| `close_issue` | `(repo_path: String, issue_number: i64) -> ()` | Close an issue via the REST issues endpoint |
| `reopen_issue` | `(repo_path: String, issue_number: i64) -> ()` | Reopen a closed issue via the REST issues endpoint |

### GitHubIssue

```rust
struct GitHubIssue {
    number: i32,
    title: String,
    state: String,           // "OPEN", "CLOSED"
    url: String,
    created_at: String,
    updated_at: String,
    author: String,
    labels: Vec<PrLabel>,    // Reuses PrLabel with computed colors
    assignees: Vec<String>,
    milestone: Option<String>,
    comments_count: i32,
}
```

### Issue Filter Modes

The `filter_mode` parameter controls which issues are fetched. It is applied as a
GraphQL `filterBy` argument (`issues_filter_clause`), **not** a Search API qualifier:

| filter_mode | GraphQL `filterBy` | Description |
|-------------|--------------------|-------------|
| `assigned` | `{ assignee: "{viewer}" }` | Issues assigned to the authenticated user |
| `created` | `{ createdBy: "{viewer}" }` | Issues created by the authenticated user |
| `mentioned` | `{ mentioned: "{viewer}" }` | Issues mentioning the authenticated user |
| `all` | *(none)* | All open issues in the repo |
| `disabled` | *(issues sub-selection omitted)* | Issue fetching disabled — `get_all_batch_impl` drops the whole section |

### Issue Query Construction

`build_multi_repo_issues_query` builds one aliased `repository(owner:, name:)` block
per repo, each embedding an `issues(first: 30, states: [OPEN], orderBy: UPDATED_AT
DESC)` sub-selection. It uses `repository().issues()` rather than `search()` because it
costs fewer GraphQL points. Results are parsed via `parse_issue_node`, which extracts
labels with `hex_to_rgba` color computation (same opacity constant
`LABEL_BG_OPACITY = 0.7` as PRs).

## GraphQL Batching

`get_repo_pr_statuses` reuses the multi-repo query builder (`build_multi_repo_pr_query`)
and sends it through `graphql_with_retry` — one GraphQL call returns every branch with
PR data. `gh pr list` is used only as the parity oracle in tests.

**Polling budget:** `github_poller.rs` batches **all** polled repos into one GraphQL
call per tick, so the cost does not scale with repo count. Intervals: `BASE_INTERVAL`
60 s when visible, `HIDDEN_INTERVAL` 120 s when hidden, backing off to `MAX_INTERVAL`
300 s on failures or a critically low rate budget.

## PR Approval & Merge

### `approve_pr`

`POST {rest_base}/repos/{owner}/{repo}/pulls/{n}/reviews` with `{"event": "APPROVE"}`,
sent through `send_rest_with_breaker`. Used by the remote-only PR popover.

### PR Diff Fetching

PR diff reads use the GitHub REST diff representation first. If GitHub returns the oversized-diff `406 Not Acceptable` response, the backend fetches the PR refs into the local clone and returns a local `git diff base...head` unified diff instead, so AI Review can still run on PRs that exceed GitHub's rendered diff file cap.

### CI Auto-Heal (`fetch_ci_failure_logs`)

Actions logs use REST runs filtered by `head_sha`, then jobs with `filter=latest`.
Each list has a ten-page cap (100 entries per page). A run with unavailable or
incomplete jobs is skipped. Downloads retain only the last 4,000 bytes per job,
with at most 100 job logs retained; output is sanitized before agent use.
One 60-second async deadline covers the sweep and any external CI downloads.
Only local Git head resolution uses a blocking worker; HTTP work is cancellable.


Resolves the local branch head for auto-heal, or uses the supplied PR head SHA for a selected check, then lists matching workflow runs, inspects their jobs, and downloads logs for every completed failed job through the GitHub Actions jobs API. Job-level retrieval works while sibling jobs are still running, before the containing workflow has a final `failure` conclusion. Used by the CI auto-heal hook (`useCiHeal`) to inject failure context into agent terminals for automatic fix cycles (up to 3 delivered attempts per cycle).

GitHub Actions jobs are downloaded through GitHub's jobs API. CircleCI checks are selected from a PR whose head SHA matches the local branch for auto-heal or the supplied PR head SHA for a selected check, including remote-only PRs, restricted to the repository's GitHub owner/repository, and fetched only when a CircleCI token is configured. A selected CircleCI check takes precedence over failed GitHub Actions jobs; an unselected auto-heal request keeps GitHub Actions priority. Every `gh` and `git` process on this path has a 30-second deadline; a timeout returns an error and auto-heal clears its in-flight state. The CircleCI job's `vcs_revision` must match that SHA. CircleCI API and action-log requests never follow redirects; pre-signed action-log downloads accept only Amazon S3 HTTPS hosts and do not receive that token. The JSON action output is parsed as a stream, retaining at most 64 KiB of recent message text before the shared sanitiser and 4,000-character output limit; a 16 MiB download ceiling bounds transient parser memory. Other external providers return a clear error naming the checks instead of the misleading "no jobs found". The auto-heal hook surfaces that message as a warn toast and does **not** consume an attempt (attempts increment only after a fix prompt is delivered).

## Stale PR Filtering

When `include_merged` is true, `get_repo_pr_statuses` includes recently merged PRs. Stale merged PRs are filtered: if a branch has been recreated after a PR was merged (detected via branch creation timestamp vs PR merge timestamp), the old merged PR is excluded to prevent ghost badges.

Browser add-account polling uses `POST /github/accounts/poll` and the same `github_poll_add_account_impl` as desktop IPC. Success persists a named account without replacing the default runtime token. Default login continues to use `POST /github/auth/poll`.

### Line review backend

The additive `github_review_threads` API reads paginated line threads with current/original line, diff side, resolved and outdated state, plus paginated replies. The existing `get_pr_review_threads` badge-count API is unchanged. Resolve/unresolve returns the confirmed GitHub thread; reply returns the posted comment. Draft submission checks the reviewed head against GitHub and posts through the existing review body builder/parser with `commit_id` pinned. A moved head returns `review-head-changed`; refresh before resubmitting. All calls resolve the repository account and use its host, token and circuit breaker.

### PR action backend

`github_pr_actions` exposes auto-merge enable/disable, merge queue enqueue/dequeue/state and failed-job reruns. `merge_or_enqueue_pr` reads the PR's `isMergeQueueEnabled` field and enqueues when the base branch has a merge queue; otherwise it merges with a pinned REST SHA. A missing or non-boolean queue-policy field stops the write. Queue entry and auto-merge enable mutations carry `expectedHeadOid`. Failed reruns use the Actions `rerun-failed-jobs` endpoint, which also reruns dependent jobs. The legacy review post and draft API both reject REQUEST_CHANGES with an empty body before account/network work. These additive commands require frontend consumers to opt in; the existing merge command is unchanged.

## Shared account registry

Core owns provider sites and accounts in `accounts.json`. Tokens are stored only
in the instance credential vault under `account/{id}`. Reads return account
metadata and `secret: {status: "set" | "unset" | "unavailable"}`. Persisted registry, site and account records tolerate unknown fields from newer builds. Vault errors
are unavailable, never an absent credential. No account action reveals tokens.

`account_action` (IPC) and `POST /accounts/action` (HTTP) call the same Rust owner
service. IPC takes `{request: <action>}`; HTTP takes the action directly. HTTP
uses the loopback-or-authenticated route guard on both routers. TCP listeners
additionally require transport authentication, including loopback requests. The future Settings MCP adapter uses this service; this change
does not add an unrestricted plugin `invoke` grant or a standalone MCP tool.

| Action | Input | Reply |
|---|---|---|
| `list` | Optional `provider` | AccountView array |
| `list_sites` | Optional `provider` | Site array |
| `add` | `account: {provider, host, kind, label, username?}`, `token` | Verified AccountView |
| `test` | `id` | Verified AccountView |
| `remove` | `id` | `{removed:true}`; rejects the durable github.com default |
| `add_site` | `provider`, `host`, `label` | Site |
| `remove_site` | Site `id` | `{removed:true}`; removes its accounts, credentials and GitHub bindings; rejects the default github.com site before any removal |
| `set_secret` | `id`, `token` | Verified AccountView; token/basic accounts only |
| `clear_secret` | `id` | `{cleared:true}`; retains account metadata |
| `set_label` | `id`, `label` | `{updated:true}`; renames an account |
| `set_site_label` | `id`, `label` | `{updated:true}`; renames a site |

AccountView fields: `id`, `provider`, `site_id`, `host`, `kind`, `label`, `login`,
`username`, `created_at`, `auth_source`, `secret`. Site fields: `id`, `provider`,
`host`, `label`. IDs are stable; new non-GitHub accounts receive UUIDs. Multiple
accounts may share one provider/site. Site URLs include their HTTP(S) origin and
optional context path; userinfo, query strings, fragments and traversal are
rejected. Verification uses a 30-second deadline, a 64 KiB response cap, no
redirects, and returns only a selected viewer identity. Error bodies are never
returned. Tests use a controlled protocol server, not live Jira/GitLab credentials.

Core registers GitHub and reuses its provider. Cloud login still requires device
flow; PAT input cannot bypass it. Non-core `ProviderDescriptor` registration is
restricted to `bearer` and `basic` token authentication. A descriptor declares
`id`, `label`, `kinds`, and `verify: {path, login_pointer}`. The trusted manifest
adapter calls `accounts::register_provider(owner, descriptor)`; unload calls
`unregister_providers(owner)`. Unload retains persisted accounts and credentials.
Verification of an unloaded provider returns unavailable; metadata remains readable.
Plugin manifest wiring and authenticated plugin HTTP requests are delivered by
the host-API stream, not this registry change.

Native secret-entry integration is separate. Its sink calls
`accounts::add_account(NewAccount, token)` directly. The core Settings control
must not collect or forward token values through plugin JavaScript. This change
provides the write-only service/input transport; it does not claim native
secret-entry privacy before that sink is integrated.

`accounts::catalogue_schema()` returns an inline Rust schema without reading
files, credentials or providers. The logical token leaf has secret sensitivity,
no default or examples, and `get`/`set_secret`/`clear_secret` operations. Get is
presence-only. Catalogue aggregate registration is a serial Settings integration
step after the accounts module lands; the module does not edit the shared catalogue.

### GitHub migration and recovery

The first registry operation imports all `github_accounts.json` entries with
unchanged account IDs, hosts, login and auth provenance. The implicit
`github.com` record still resolves the existing env → OAuth vault chain;
env tokens are not copied into the vault. The legacy metadata file is retained
for recovery and is not re-imported after the migration marker is committed.
`github_bindings.json` stays authoritative, so existing repo bindings remain valid.
All GitHub registry writers now use the shared registry.

On vault load, `github/oauth-token` and `github/account/{id}/token` migrate to
`account/{id}` in one persisted blob. Existing canonical credentials win. Old
copies disappear only after persistence succeeds; a failed migration can retry.
GitHub credential enum variants address the same canonical slots, including OAuth
login/logout and per-account token resolution. OAuth replies and Debug output
omit `access_token`.

Metadata updates use the existing cross-process ConfigFile lock and atomic file
replacement. Corrupt account metadata is rejected and retained for operator
repair. Vault and metadata are separate stores: a vault change followed by a
metadata persistence failure may leave a credential phase committed. An operation
error is not a guarantee of cross-store rollback; inspect presence and retry the
owner action after repairing storage. Site removal can stop after removing some
accounts if a later credential/binding operation fails; retry the site action.
Rust changes load only after Boss manually restarts `make dev` or installs a
`make build`; this work does not launch or restart the app.

### Private account token sink

Enterprise PAT entry in Settings launches `account_secret_form` with account
metadata only. The private native form sends its one-time nonce and token to
Rust; the account sink calls `accounts::add_account` or `accounts::set_secret`.
The token never enters Settings state or the general secret-command environment.
Provider validation precedes persistence. Decline or close before submission
writes no account or credential. After Submit is accepted, verification and storage
finish even if the window closes; a consumed nonce cannot be replayed. Rotation
invalidates the same GitHub Enterprise runtime cache as the account owner API.
Native inspection remains blocked until the private window is destroyed.

GitHub OAuth device flow requests `repo read:project`. The request builder and the Settings permission text read the same value from `src/data/github-oauth-scopes.txt` (Rust `include_str!` and Vite raw import). Existing account tokens retain their previous scopes until the user signs in again. GitHub Projects v2 permission failures retain the API error, including `INSUFFICIENT_SCOPES`; they are not empty boards.

## GitHub settings owner adapter

`settings/accounts.rs` provides an internal typed `GithubSettings` read/perform
boundary. `action_schema()` derives its action DTO schema. Account creation,
credential replacement/clearing, labels and removal use `accounts::execute`;
Enterprise PAT creation keeps the owner's `/user` identity verification. Cloud
PATs cannot replace default or named OAuth credentials. Removal also removes
bindings, per-account caches and cooldowns; corrupt bindings stop removal before
credentials or metadata are deleted.

Snapshots return credential presence (`set`, `unset`, or `unavailable`) and the
ambient runtime token source. Reads do not resolve ambient credentials or refresh
GitHub over the network. They do not invent a stored account for an external
credential source. GitHub display preferences remain in the existing preferences
and repository settings adapters.

Cloud login uses `begin_cloud_login`, `cloud_login_status` and
`complete_cloud_login`. Begin returns an opaque `flowId`, human verification URI
and user code. The backend retains the device credential, enforces expiry and
poll intervals, increases intervals on slow-down, and prevents duplicate exchanges.
Default and additional-account completion call their existing GitHub auth owners.
Token/provider error bodies and token scopes never enter adapter replies.

Binding actions require `expectedRevision` from the snapshot's `bindingsRevision`.
The GitHub owner checks it under the cross-process binding file lock, accepts only
registered repositories, derives owner/repo from the chosen remote, and rejects a
remote whose host differs from the account. Successful mutations include immediate
owner readback; binding no-ops report `persisted:false`.

This step adds no MCP, HTTP or IPC endpoint. Transport publication and settings
change events remain the shared cutover and refresh work. Real provider login and
Enterprise identity checks need an authenticated manual verification after that
wiring; the isolated adapter tests do not access production credentials.

Linked Tasks check failures reuse `ci_failed` PR transitions and the existing native notice service. A separate default-off Tasks UI preference selects a single task-labelled notice, independent of the default-on PR preference. No extra GitHub poller is introduced.

### Account-scoped GitHub review and action ports

`host.githubReviewThreads(account, reference, after)` returns the core B6 review
thread page, including head SHA, bot authors and parsed findings.
`host.githubPrAction(account, reference, request)` calls core B7 policy.
Both require `net:http` and an on-disk manifest declaring `providers: ["github"]`.
The reference must identify a GitHub pull request in the selected account. Core
loads that exact account credential, never a local repository default. Cross-account
references fail before credential lookup. No token is returned to the plugin.

Requests use `action`: `state`, `auto_merge` (`enabled`, optional `mergeMethod`,
`expectedHeadSha`), `queue` (`enqueued`, optional `expectedHeadSha`), `merge`
(`mergeMethod`, `expectedHeadSha`), `rerun` (`runId`), or `review` (`draft` with
`headSha`, `body`, `event`, `comments`). Existing core reviewed-head validation,
merge queue policy and shared request budgets apply unchanged. The desktop command
`plugin_github_action` and `POST /api/plugins/{plugin_id}/github` accept identical
`{account, reference, request}` payloads (IPC also supplies `pluginId`).
The GitHub Work runtime binds both ports when it registers each configured account.
No live write or desktop-load verification is implied by recorded-fixture tests.

## Device-login invalidation (#1787-4778)

Logout and disconnect advance the GitHub login generation. Settings device flows retain their creation generation; an older flow expires instead of completing. Credential activation and logout share an async serialization lock, so a token response racing logout cannot restore credentials after logout completes.

## Settings startup workflows

The Settings facade delegates device flow, Enterprise verification, account cleanup and registered-repository bindings to the GitHub owners. [Startup workflows](settings-startup.md#github) records poll/status semantics, binding revisions, write-only credentials and typed `list_accounts` discovery of account metadata/presence. An adapter-level owner snapshot alone does not prove the same information is reachable through the published Settings schema.

## Persistent PR visibility filters

Global `pr_hide_drafts`, `pr_hide_conflicting` and `pr_hide_ci_failing` values
persist in `config.json` and default to false (show all). Each repository stores
nullable overrides in `repo-settings.json`: null inherits the current global
value, true hides the matching PRs, and false explicitly shows them. Settings
owner transactions preserve unrelated fields, and settings refresh adopts owner
readback without saving it again. The Rust settings catalogue describes both scopes.

### Repository access diagnostics

The unified batch poll preserves `errors[].path`, `type` and the exact message
per repository alias, including nested-field errors. Only a root-level explicit
`NOT_FOUND` enters the account-scoped 24-hour exclusion. `FORBIDDEN` and unexplained
nulls remain eligible for the next normal poll, and a successful poll clears the
failure. A `NOT_FOUND` can mean a repository is not visible to the token; it does
not prove that the repository was deleted.

`github_diagnostics` (IPC and HTTP) retains `repos_not_found` and adds
`repo_access_errors`, grouped by account, host and owner. Each repository carries
`repo`, `reason` and `message`. Reasons are `not_found`, `forbidden` and
`unknown`; message wording does not infer organization policy or SSO.
Every github.com OAuth group offers the app's authorized-app page, including
NOT_FOUND groups because GitHub can hide private repositories until org approval.
Settings renders these backend groups without reclassifying them. Refresh status clears the NOT_FOUND retry cache for all accounts through an
explicit mutation, then reads diagnostics. Opening Settings and ordinary GET
diagnostics remain read-only. Refresh does not log in, grant access, reset rate
limits or bypass the monitoring cadence; the next poll retries the repositories.

### Partial-response regression evidence

BatchPoll does not select Projects fields. Routine PR and issue monitoring does
not require Projects access. The generated-query regression checks this boundary
for every issue filter and PR visibility combination.

`fixtures/github-partial-monitoring-2336.json` records a real GitHub GraphQL
response with an accessible repository and a `NOT_FOUND` sibling. The replay
passes through `graphql_request` and the same response-adoption function used by
`poll_one_account`, and checks both PR and issue output. The fixture records its
request, timestamp and source; it contains no headers or credentials. It is not
a recorded missing-scope or organization-policy response.
