# GitHub Integration

**Module:** `src-tauri/src/github.rs`

Integrates with GitHub via the `gh` CLI for PR status, CI checks, and batch queries.

## Tauri Commands

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_github_status` | `(path: String) -> GitHubStatus` | PR + CI status for current branch |
| `get_ci_checks` | `(path: String) -> Vec<Value>` | Detailed CI check list |
| `get_repo_pr_statuses` | `(path: String) -> Vec<BranchPrStatus>` | Batch PR status for all branches |

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
