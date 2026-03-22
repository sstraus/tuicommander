# GitHub Integration

**Modules:** `src-tauri/src/github.rs`, `src-tauri/src/github_auth.rs`

Integrates with GitHub via GraphQL API for PR status, CI checks, and batch queries. Supports OAuth Device Flow login as an alternative to gh CLI tokens.

## Token Resolution

Priority order (first non-empty wins):

1. `GH_TOKEN` environment variable
2. `GITHUB_TOKEN` environment variable
3. OAuth keyring token (`github_auth.rs` — stored in OS keyring via `keyring` crate)
4. `gh_token` crate (reads `~/.config/gh/hosts.yml`)
5. `gh auth token` CLI subprocess

The active token source is tracked in `AppState.github_token_source` as a `TokenSource` enum (`Env`, `OAuth`, `GhCli`, `None`).

## Tauri Commands — Authentication (`github_auth.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_start_login` | `() -> DeviceCodeResponse` | Start OAuth Device Flow, returns user code |
| `github_poll_login` | `(device_code: String) -> PollResult` | Poll for token, saves to keyring on success |
| `github_logout` | `() -> ()` | Delete OAuth token from keyring, fall back to env/CLI |
| `github_auth_status` | `() -> AuthStatus` | Current auth status with login, avatar, source |
| `github_disconnect` | `() -> ()` | Disconnect GitHub — clear all tokens from keyring and env cache |
| `github_diagnostics` | `() -> Value` | Diagnostics: token sources, scopes, API connectivity |

## Tauri Commands — GitHub Data (`github.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_github_status` | `(path: String) -> GitHubStatus` | PR + CI status for current branch |
| `get_ci_checks` | `(path: String) -> Vec<Value>` | Detailed CI check list |
| `get_repo_pr_statuses` | `(path: String, include_merged: bool) -> Vec<BranchPrStatus>` | Batch PR status for all branches |
| `approve_pr` | `(repo_path: String, pr_number: i32) -> String` | Submit approving review via GitHub API |
| `get_all_pr_statuses` | `(path: String) -> Vec<BranchPrStatus>` | Batch PR status for all branches (includes merged) |
| `get_pr_diff` | `(repo_path: String, pr_number: i32) -> String` | Get PR diff content |
| `merge_pr_via_github` | `(repo_path: String, pr_number: i32, merge_method: String) -> String` | Merge PR via GitHub API |
| `fetch_ci_failure_logs` | `(repo_path: String, run_id: i64) -> String` | Fetch failure logs from a GitHub Actions run for CI auto-heal |
| `check_github_circuit` | `(path: String) -> CircuitState` | Check GitHub API circuit breaker state |

## Data Types

### GitHubStatus

```rust
struct GitHubStatus {
    has_remote: bool,
    current_branch: String,
    pr_status: Option<PrStatus>,
    ci_status: Option<CiStatus>,
    ahead: i32,
    behind: i32,
}
```

### PrStatus

```rust
struct PrStatus {
    number: i32,
    title: String,
    state: String,    // "OPEN", "CLOSED", "MERGED"
    url: String,
}
```

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
    check_details: Vec<CheckDetail>,
    author: String,
    commits: i32,
    mergeable: String,           // "MERGEABLE", "CONFLICTING", "UNKNOWN"
    merge_state_status: String,  // "CLEAN", "DIRTY", "BEHIND", etc.
    review_decision: String,     // "APPROVED", "CHANGES_REQUESTED", etc.
    labels: Vec<PrLabel>,        // Labels with pre-computed colors
    is_draft: bool,
    base_ref_name: String,
    created_at: String,
    updated_at: String,
    merge_state_label: Option<StateLabel>,   // Pre-classified display label
    review_state_label: Option<StateLabel>,  // Pre-classified display label
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

### `parse_pr_list_json(json_str: &str) -> Vec<BranchPrStatus>`

Parses the JSON output from `gh pr list --json ...` and enriches with computed fields (merge state classification, review state classification, label colors).

### `classify_merge_state(mergeable, merge_state_status) -> Option<StateLabel>`

Maps GitHub merge state to display labels:

| mergeable | merge_state_status | Label | CSS Class |
|-----------|-------------------|-------|-----------|
| MERGEABLE | CLEAN | Ready to merge | merge-ready |
| MERGEABLE | UNSTABLE | Checks failing | merge-unstable |
| CONFLICTING | * | Has conflicts | merge-conflict |
| * | BEHIND | Behind base | merge-behind |
| * | BLOCKED | Blocked | merge-blocked |
| * | DRAFT | Draft | merge-draft |

### `classify_review_state(review_decision) -> Option<StateLabel>`

| review_decision | Label | CSS Class |
|-----------------|-------|-----------|
| APPROVED | Approved | review-approved |
| CHANGES_REQUESTED | Changes requested | review-changes |
| REVIEW_REQUIRED | Review required | review-required |

### `hex_to_rgba(hex: &str, alpha: f64) -> String`

Converts hex color (e.g., "#ff0000") to rgba string (e.g., "rgba(255, 0, 0, 0.5)").

### `is_light_color(hex: &str) -> bool`

Calculates relative luminance using the sRGB formula to determine if a color is light (for choosing black vs white text).

## GraphQL Batching

`get_repo_pr_statuses` uses `gh pr list` with extensive `--json` fields to fetch all open PRs in a single call. This is efficient: 1 API call returns all branches with PR data.

**Polling budget:** ~2 calls/min/repo = 1,200/hr for 10 repos, well within GitHub's 5,000/hr rate limit.

## PR Approval & Merge

### `approve_pr`

Submits an approving review on a pull request via `gh api`. Used by the remote-only PR popover.

### CI Auto-Heal (`fetch_ci_failure_logs`)

Fetches the latest failure logs from a GitHub Actions run. Used by the CI auto-heal hook (`useCiHeal`) to inject failure context into agent terminals for automatic fix cycles (up to 3 attempts per cycle).

## Stale PR Filtering

When `include_merged` is true, `get_repo_pr_statuses` includes recently merged PRs. Stale merged PRs are filtered: if a branch has been recreated after a PR was merged (detected via branch creation timestamp vs PR merge timestamp), the old merged PR is excluded to prevent ghost badges.
