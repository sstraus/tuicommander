use serde::{Deserialize, Serialize};

/// Git remote + branch status (no PR/CI — those come from githubStore via batch query)
#[derive(Clone, Serialize)]
pub struct GitHubStatus {
    pub has_remote: bool,
    pub current_branch: String,
    pub ahead: i32,
    pub behind: i32,
}

/// Summary of CI check states for a PR
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct CheckSummary {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub total: u32,
}

/// Pre-computed merge/review state label for the UI
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct StateLabel {
    pub label: String,
    pub css_class: String,
}

/// Whether a PR's branch conflicts with its base — as far as GitHub has
/// actually worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictState {
    Conflicting,
    /// GitHub is still recomputing. It keeps serving the LAST KNOWN `mergeable`
    /// meanwhile, so that field says nothing about the current head.
    Checking,
    Clear,
}

/// The single rule for "does this PR conflict", shared by the sidebar badge and
/// the PR popover.
///
/// It exists because the two used to decide separately: the badge tested
/// `mergeable == CONFLICTING` on its own and kept painting a red Conflicts badge
/// through the window where GitHub had already invalidated that value, and the
/// popover's own CONFLICTING short-circuit did the same. A push therefore
/// accused a PR of conflicting on stale data, with no second surface to
/// contradict it (#8537).
///
/// `mergeStateStatus == UNKNOWN` is the recompute marker and wins over
/// `mergeable`; `DIRTY` is GitHub's computed conflict verdict and is trusted on
/// its own.
pub fn classify_conflict_state(
    mergeable: Option<&str>,
    merge_state_status: Option<&str>,
) -> ConflictState {
    match merge_state_status {
        Some("DIRTY") => ConflictState::Conflicting,
        // Absent is treated as recomputing: an answer we never received is not
        // evidence of a clean merge.
        None | Some("UNKNOWN") => ConflictState::Checking,
        _ if mergeable == Some("CONFLICTING") => ConflictState::Conflicting,
        _ => ConflictState::Clear,
    }
}

/// Classify merge readiness from mergeable + merge_state_status fields
pub fn classify_merge_state(
    mergeable: Option<&str>,
    merge_state_status: Option<&str>,
) -> Option<StateLabel> {
    match classify_conflict_state(mergeable, merge_state_status) {
        ConflictState::Conflicting => {
            return Some(StateLabel {
                label: "Conflicts".to_string(),
                css_class: "conflicting".to_string(),
            });
        }
        // No chip at all while GitHub recomputes — same as before, and still the
        // honest answer: the popover has nothing to report yet. The sidebar
        // badge, which must render *something* in that slot, shows its neutral
        // checking state instead.
        ConflictState::Checking => return None,
        ConflictState::Clear => {}
    }

    match merge_state_status {
        Some("CLEAN") => Some(StateLabel {
            label: "Ready to merge".to_string(),
            css_class: "clean".to_string(),
        }),
        Some("BEHIND") => Some(StateLabel {
            label: "Behind base".to_string(),
            css_class: "behind".to_string(),
        }),
        Some("BLOCKED") => Some(StateLabel {
            label: "Blocked".to_string(),
            css_class: "blocked".to_string(),
        }),
        Some("UNSTABLE") => Some(StateLabel {
            label: "Unstable".to_string(),
            css_class: "blocked".to_string(),
        }),
        Some("DRAFT") => Some(StateLabel {
            label: "Draft".to_string(),
            css_class: "behind".to_string(),
        }),
        Some("DIRTY") => Some(StateLabel {
            label: "Conflicts".to_string(),
            css_class: "conflicting".to_string(),
        }),
        _ => None, // UNKNOWN, HAS_HOOKS — don't show
    }
}

/// Classify review decision into display label
pub fn classify_review_state(review_decision: Option<&str>) -> Option<StateLabel> {
    match review_decision {
        Some("APPROVED") => Some(StateLabel {
            label: "Approved".to_string(),
            css_class: "approved".to_string(),
        }),
        Some("CHANGES_REQUESTED") => Some(StateLabel {
            label: "Changes requested".to_string(),
            css_class: "changes-requested".to_string(),
        }),
        Some("REVIEW_REQUIRED") => Some(StateLabel {
            label: "Review required".to_string(),
            css_class: "review-required".to_string(),
        }),
        _ => None,
    }
}

/// Parse r/g/b from a 6-char hex color string, returning (0,0,0) for invalid input
fn parse_hex_rgb(hex: &str) -> (u8, u8, u8) {
    if hex.len() < 6 {
        return (0, 0, 0);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    (r, g, b)
}

/// Opacity used for GitHub label backgrounds in PR and issue display
pub const LABEL_BG_OPACITY: f64 = 0.7;

/// Convert a 6-char hex color to an rgba() CSS string with the given alpha
pub fn hex_to_rgba(hex: &str, alpha: f64) -> String {
    let (r, g, b) = parse_hex_rgb(hex);
    format!("rgba({r}, {g}, {b}, {alpha})")
}

/// Determine if a hex color is light (needs dark text) using BT.601 luma
pub fn is_light_color(hex: &str) -> bool {
    let (r, g, b) = parse_hex_rgb(hex);
    let (r, g, b) = (r as u32, g as u32, b as u32);
    (r * 299 + g * 587 + b * 114) / 1000 > 128
}

/// PR label with name, hex color, and pre-computed display colors
#[derive(Clone, Debug, Serialize)]
pub struct PrLabel {
    pub name: String,
    pub color: String,
    pub text_color: String,
    pub background_color: String,
}

/// PR status for a branch, returned by batch endpoint
#[derive(Clone, Debug, Serialize)]
pub struct BranchPrStatus {
    pub branch: String,
    pub number: i32,
    pub title: String,
    pub state: String,
    pub url: String,
    pub additions: i32,
    pub deletions: i32,
    pub checks: CheckSummary,
    pub author: String,
    pub commits: i32,
    pub mergeable: String,
    pub merge_state_status: String,
    pub review_decision: String,
    /// Whether the authenticated viewer's latest review on this PR is APPROVED.
    /// Used to hide the Approve button once the current user has already approved,
    /// even when the overall `review_decision` is still REVIEW_REQUIRED.
    pub viewer_did_approve: bool,
    pub labels: Vec<PrLabel>,
    pub is_draft: bool,
    pub base_ref_name: String,
    pub head_ref_oid: String,
    pub created_at: String,
    pub updated_at: String,
    pub merge_state_label: Option<StateLabel>,
    /// The conflict verdict both surfaces render. Sent pre-computed so the
    /// sidebar badge cannot re-derive it from `mergeable` alone and disagree
    /// with the popover (#8537).
    pub conflict_state: ConflictState,
    pub review_state_label: Option<StateLabel>,
    /// Repo-level: merge commits allowed
    pub merge_commit_allowed: bool,
    /// Repo-level: squash merge allowed
    pub squash_merge_allowed: bool,
    /// Repo-level: rebase merge allowed
    pub rebase_merge_allowed: bool,
    /// Unresolved review threads among the first [`REVIEW_THREADS_LIMIT`]. A plain
    /// `isResolved` selection costs no extra GraphQL points; the bot/human split
    /// needs a nested connection per thread and is fetched on demand
    /// (`get_pr_review_threads`).
    pub unresolved_threads: u32,
    /// More than [`REVIEW_THREADS_LIMIT`] threads exist, so `unresolved_threads` is a lower
    /// bound and 0 does not prove "none open" (GitHub cannot filter threads by resolution).
    /// The poll fetch walks the remaining pages and clears this ([`Self::settle_review_threads`]);
    /// it stays set only when that walk failed or ran out of pages.
    pub unresolved_threads_truncated: bool,
}

impl BranchPrStatus {
    /// Replace the first-page thread count with the total of a full walk. `complete` is false
    /// when the walk hit its page bound, so the count stays a lower bound ("N+").
    pub fn settle_review_threads(&mut self, unresolved: u32, complete: bool) {
        self.unresolved_threads = unresolved;
        self.unresolved_threads_truncated = !complete;
    }
}

/// Threads inspected per PR in the batch poll.
pub const REVIEW_THREADS_LIMIT: u32 = 50;

/// GraphQL selection for [`BranchPrStatus::unresolved_threads`], shared by every PR query.
pub fn review_threads_selection() -> String {
    format!(
        "reviewThreads(first: {REVIEW_THREADS_LIMIT}) {{ pageInfo {{ hasNextPage }} nodes {{ isResolved }} }}"
    )
}

/// Unresolved review threads split by who opened them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReviewThreadCounts {
    pub bot: u32,
    pub human: u32,
    /// Threads opened by a finding TUIC posted for ego ([`crate::review_bots::EGO_MARKER`]).
    pub ego: u32,
}

impl ReviewThreadCounts {
    pub fn total(&self) -> u32 {
        self.bot + self.human + self.ego
    }
}

/// Count unresolved threads from `reviewThreads.nodes`, attributing each to the
/// author of its first comment: GraphQL actor `__typename` "Bot" is a bot (a
/// `[bot]` login covers apps surfaced as users), everything else is human.
pub fn count_review_threads(nodes: &serde_json::Value) -> ReviewThreadCounts {
    let mut counts = ReviewThreadCounts::default();
    for node in nodes.as_array().into_iter().flatten() {
        if node["isResolved"].as_bool().unwrap_or(true) {
            continue;
        }
        let first = &node["comments"]["nodes"][0];
        if crate::review_bots::is_ego(first) {
            counts.ego += 1;
        } else if crate::review_bots::is_bot(&first["author"]) {
            counts.bot += 1;
        } else {
            counts.human += 1;
        }
    }
    counts
}

/// Classification of a single check node for summary counting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckCategory {
    Passed,
    Failed,
    Pending,
}

/// Map a deduped statusCheckRollup node (CheckRun or StatusContext) to a summary category.
pub fn classify_check_node(node: &serde_json::Value) -> CheckCategory {
    if node["__typename"].as_str() == Some("CheckRun") {
        // `conclusion` is only meaningful once `status` is COMPLETED.
        if node["status"].as_str().unwrap_or("").to_uppercase() != "COMPLETED" {
            return CheckCategory::Pending;
        }
        match node["conclusion"]
            .as_str()
            .unwrap_or("")
            .to_uppercase()
            .as_str()
        {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => CheckCategory::Passed,
            "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "STARTUP_FAILURE"
            | "ACTION_REQUIRED" => CheckCategory::Failed,
            _ => CheckCategory::Pending,
        }
    } else {
        // StatusContext
        match node["state"].as_str().unwrap_or("").to_uppercase().as_str() {
            "SUCCESS" => CheckCategory::Passed,
            "FAILURE" | "ERROR" => CheckCategory::Failed,
            _ => CheckCategory::Pending,
        }
    }
}

/// Deduplicate statusCheckRollup context nodes by check name.
///
/// GitHub attaches every check suite to the head commit, so when a workflow runs
/// more than once on the same commit (e.g. a stale run cancelled by a `concurrency`
/// group, or a re-run after the base branch advanced) the rollup lists each check
/// name multiple times. We keep only the most recently started entry per name —
/// matching what `gh pr checks` displays. Insertion order is preserved for a stable
/// list. Expects the `contexts` object (reads its `nodes` array).
pub fn dedup_rollup_nodes(contexts: &serde_json::Value) -> Vec<serde_json::Value> {
    let nodes = match contexts["nodes"].as_array() {
        Some(arr) => arr,
        None => return vec![],
    };

    // name -> (timestamp, node). Insertion order tracked separately for stable output.
    let mut latest: std::collections::HashMap<String, (String, serde_json::Value)> =
        std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for node in nodes {
        let name = node["name"]
            .as_str()
            .or_else(|| node["context"].as_str())
            .unwrap_or("")
            .to_string();
        let ts = node["startedAt"]
            .as_str()
            .or_else(|| node["createdAt"].as_str())
            .unwrap_or("")
            .to_string();
        match latest.get(&name) {
            // Keep the newest entry; ISO-8601 timestamps sort lexicographically.
            Some((existing_ts, _)) if existing_ts.as_str() >= ts.as_str() => {}
            _ => {
                if !latest.contains_key(&name) {
                    order.push(name.clone());
                }
                latest.insert(name, (ts, node.clone()));
            }
        }
    }

    order
        .into_iter()
        .filter_map(|name| latest.remove(&name).map(|(_, node)| node))
        .collect()
}

/// Shared logic for extracting fields from a single PR node.
pub fn parse_pr_node(v: &serde_json::Value) -> Option<BranchPrStatus> {
    let branch = v["headRefName"].as_str()?.to_string();
    let number = v["number"].as_i64()? as i32;
    let title = v["title"].as_str().unwrap_or("").to_string();
    let state = v["state"].as_str().unwrap_or("").to_string();
    let url = v["url"].as_str().unwrap_or("").to_string();
    let additions = v["additions"].as_i64().unwrap_or(0) as i32;
    let deletions = v["deletions"].as_i64().unwrap_or(0) as i32;
    let author = v["author"]["login"].as_str().unwrap_or("").to_string();
    let commits = v["commits"]["totalCount"].as_i64().unwrap_or(0) as i32;

    // Parse CI check summary from GraphQL statusCheckRollup. GitHub attaches every
    // check suite to the head commit, so a re-run (or a stale run cancelled by a
    // `concurrency` group) duplicates a check name in the rollup. Dedup to the
    // newest entry per name — matching `gh pr checks` — before tallying, otherwise
    // passed/failed/pending double-count the stale duplicates.
    let rollup_contexts = &v["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"];
    let mut passed: u32 = 0;
    let mut failed: u32 = 0;
    let mut pending: u32 = 0;
    for node in dedup_rollup_nodes(rollup_contexts) {
        match classify_check_node(&node) {
            CheckCategory::Passed => passed += 1,
            CheckCategory::Failed => failed += 1,
            CheckCategory::Pending => pending += 1,
        }
    }

    let total = passed + failed + pending;

    let mergeable = v["mergeable"].as_str().unwrap_or("UNKNOWN").to_string();
    let merge_state_status = v["mergeStateStatus"]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_string();
    let review_decision = v["reviewDecision"].as_str().unwrap_or("").to_string();
    let viewer_did_approve = v["viewerLatestReview"]["state"].as_str() == Some("APPROVED");
    let is_draft = v["isDraft"].as_bool().unwrap_or(false);
    let unresolved_threads_truncated = v["reviewThreads"]["pageInfo"]["hasNextPage"]
        .as_bool()
        .unwrap_or(false);
    let unresolved_threads = v["reviewThreads"]["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter(|n| n["isResolved"].as_bool() == Some(false))
                .count() as u32
        })
        .unwrap_or(0);

    let labels = v["labels"]["nodes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|l| {
                    let color = l["color"].as_str().unwrap_or("").to_string();
                    let (text_color, background_color) = if color.len() == 6 {
                        let text = if is_light_color(&color) {
                            "#1e1e1e"
                        } else {
                            "#e5e5e5"
                        };
                        (text.to_string(), hex_to_rgba(&color, LABEL_BG_OPACITY))
                    } else {
                        (String::new(), String::new())
                    };
                    Some(PrLabel {
                        name: l["name"].as_str()?.to_string(),
                        color,
                        text_color,
                        background_color,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let base_ref_name = v["baseRefName"].as_str().unwrap_or("").to_string();
    let head_ref_oid = v["headRefOid"].as_str().unwrap_or("").to_string();
    let created_at = v["createdAt"].as_str().unwrap_or("").to_string();
    let updated_at = v["updatedAt"].as_str().unwrap_or("").to_string();

    let merge_state_label =
        classify_merge_state(Some(mergeable.as_str()), Some(merge_state_status.as_str()));
    let conflict_state =
        classify_conflict_state(Some(mergeable.as_str()), Some(merge_state_status.as_str()));
    let review_state_label = classify_review_state(if review_decision.is_empty() {
        None
    } else {
        Some(review_decision.as_str())
    });

    Some(BranchPrStatus {
        branch,
        number,
        title,
        state,
        url,
        additions,
        deletions,
        checks: CheckSummary {
            passed,
            failed,
            pending,
            total,
        },
        author,
        commits,
        mergeable,
        merge_state_status,
        review_decision,
        viewer_did_approve,
        labels,
        is_draft,
        base_ref_name,
        head_ref_oid,
        created_at,
        updated_at,
        merge_state_label,
        conflict_state,
        review_state_label,
        // Defaults — stamped with real values from repo-level response after parsing
        merge_commit_allowed: true,
        squash_merge_allowed: true,
        rebase_merge_allowed: true,
        unresolved_threads,
        unresolved_threads_truncated,
    })
}

/// Stamp merge policy from a GraphQL repository object onto parsed PR nodes.
pub fn stamp_merge_policy(nodes: &mut [BranchPrStatus], repo_json: &serde_json::Value) {
    let merge = repo_json["mergeCommitAllowed"].as_bool().unwrap_or(true);
    let squash = repo_json["squashMergeAllowed"].as_bool().unwrap_or(true);
    let rebase = repo_json["rebaseMergeAllowed"].as_bool().unwrap_or(true);
    for pr in nodes.iter_mut() {
        pr.merge_commit_allowed = merge;
        pr.squash_merge_allowed = squash;
        pr.rebase_merge_allowed = rebase;
    }
}

/// A merged pull request, as consumed by the AI changelog generator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergedPr {
    pub number: i64,
    pub title: String,
    pub url: String,
    pub author: String,
    /// ISO-8601 merge timestamp (`mergedAt`), or empty if absent.
    pub merged_at: String,
    pub labels: Vec<String>,
}

pub const MERGED_PRS_QUERY: &str = r#"
query MergedPRs($owner: String!, $repo: String!, $first: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: $first, states: [MERGED],
                 orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes {
        number title url mergedAt
        author { login }
        labels(first: 10) { nodes { name } }
      }
    }
  }
}
"#;

/// Parse the `MergedPRs` GraphQL response into a list of merged PRs, newest
/// first. Pure — unit-tested against a fixture. Nodes missing a number are
/// dropped; other fields default to empty.
pub fn parse_merged_prs(response: &serde_json::Value) -> Vec<MergedPr> {
    let nodes = match response["data"]["repository"]["pullRequests"]["nodes"].as_array() {
        Some(arr) => arr,
        None => return vec![],
    };
    nodes
        .iter()
        .filter_map(|n| {
            let number = n["number"].as_i64()?;
            let labels = n["labels"]["nodes"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Some(MergedPr {
                number,
                title: n["title"].as_str().unwrap_or("").to_string(),
                url: n["url"].as_str().unwrap_or("").to_string(),
                author: n["author"]["login"].as_str().unwrap_or("").to_string(),
                merged_at: n["mergedAt"].as_str().unwrap_or("").to_string(),
                labels,
            })
        })
        .collect()
}

/// GitHub Issue status, analogous to BranchPrStatus for PRs.
#[derive(Clone, Debug, Serialize)]
pub struct GitHubIssue {
    pub number: i32,
    pub title: String,
    pub state: String, // OPEN, CLOSED
    pub url: String,
    pub author: String,
    pub labels: Vec<PrLabel>, // Reuse PrLabel — same GitHub schema
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub comments_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Parse a single issue node from GraphQL JSON.
pub fn parse_issue_node(v: &serde_json::Value) -> Option<GitHubIssue> {
    let number = v["number"].as_i64()? as i32;
    let title = v["title"].as_str().unwrap_or("").to_string();
    let state = v["state"].as_str().unwrap_or("").to_string();
    let url = v["url"].as_str().unwrap_or("").to_string();
    let author = v["author"]["login"].as_str().unwrap_or("").to_string();

    let labels = v["labels"]["nodes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|l| {
                    let color = l["color"].as_str().unwrap_or("").to_string();
                    let (text_color, background_color) = if color.len() == 6 {
                        let text = if is_light_color(&color) {
                            "#1e1e1e"
                        } else {
                            "#e5e5e5"
                        };
                        (text.to_string(), hex_to_rgba(&color, LABEL_BG_OPACITY))
                    } else {
                        (String::new(), String::new())
                    };
                    Some(PrLabel {
                        name: l["name"].as_str()?.to_string(),
                        color,
                        text_color,
                        background_color,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let assignees = v["assignees"]["nodes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a["login"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let milestone = v["milestone"]["title"].as_str().map(String::from);
    let comments_count = v["comments"]["totalCount"].as_i64().unwrap_or(0) as i32;
    let created_at = v["createdAt"].as_str().unwrap_or("").to_string();
    let updated_at = v["updatedAt"].as_str().unwrap_or("").to_string();

    Some(GitHubIssue {
        number,
        title,
        state,
        url,
        author,
        labels,
        assignees,
        milestone,
        comments_count,
        created_at,
        updated_at,
    })
}

/// A viewer-scoped issue filter (`assigned`/`created`/`mentioned`) is only
/// meaningful once the viewer's login is known. When it can't be resolved the
/// filter would collapse to a match-nobody clause, so callers omit the issues
/// section instead. `all` (and the no-issues sentinels) need no viewer.
pub fn filter_requires_viewer(filter_mode: &str) -> bool {
    matches!(filter_mode, "assigned" | "created" | "mentioned")
}

/// Build `filterBy` clause for `repository().issues()` based on filter mode.
fn issues_filter_clause(filter_mode: &str, viewer: &str) -> String {
    match filter_mode {
        "assigned" => format!(", filterBy: {{ assignee: \"{viewer}\" }}"),
        "created" => format!(", filterBy: {{ createdBy: \"{viewer}\" }}"),
        "mentioned" => format!(", filterBy: {{ mentioned: \"{viewer}\" }}"),
        _ => String::new(), // "all" — no user filter
    }
}

/// The issues sub-selection for embedding inside a repository alias.
fn issues_repo_section(filter_mode: &str, viewer: &str) -> String {
    let filter = issues_filter_clause(filter_mode, viewer);
    let node_fields = r#"number title state url createdAt updatedAt
        author { login }
        labels(first: 10) { nodes { name color } }
        assignees(first: 5) { nodes { login } }
        milestone { title }
        comments { totalCount }"#;
    format!(
        "    issues(first: 30, states: [OPEN]{filter}, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{\n      nodes {{ {node_fields} }}\n    }}"
    )
}

/// Build a batched GraphQL query for issues only (on-demand Tauri command path).
/// Uses `repository().issues()` — cheaper than `search()` in GraphQL points.
pub fn build_multi_repo_issues_query(
    repos: &[(String, String, String)],
    viewer: &str,
    filter_mode: &str,
) -> (String, Vec<(String, String)>) {
    let mut aliases: Vec<(String, String)> = Vec::new();
    let mut parts = vec!["query BatchRepoIssues {".to_string()];

    for (i, (path, owner, name)) in repos.iter().enumerate() {
        let alias = format!("r{i}");
        parts.push(format!(
            "  {alias}: repository(owner: \"{owner}\", name: \"{name}\") {{\n{}\n  }}",
            issues_repo_section(filter_mode, viewer),
        ));
        aliases.push((alias, path.clone()));
    }
    parts.push("  rateLimit { cost remaining resetAt }".to_string());
    parts.push("}".to_string());

    (parts.join("\n"), aliases)
}

/// Build a batched GraphQL query fetching PRs and (optionally) Issues for all
/// repos in a single HTTP request.  When `filter_mode` is "disabled" the issues
/// section is omitted entirely, saving GraphQL points.
///
/// DEFERRED (2026-10-01) — no chunking: each repo requests about 6.5k nodes at 40 PRs (4.5k before
/// `reviewThreads`), so more than ~75 repos in one query would pass GitHub's 500,000 node limit.
/// Split the repo list into chunks when a user reaches that many repositories.
pub fn build_unified_batch_query(
    repos: &[(String, String, String)],
    include_merged: bool,
    filter_mode: &str,
    viewer: &str,
    hide_drafts: bool,
) -> (String, Vec<(String, String)>) {
    let states = if include_merged {
        "[OPEN, MERGED]"
    } else {
        "[OPEN]"
    };
    // Fetch more items when drafts are hidden so filtering leaves enough valid PRs.
    let pr_first = if hide_drafts { 40 } else { 20 };
    let pr_node_fields = format!(
        r#"number title state url headRefName headRefOid baseRefName isDraft
        additions deletions mergeable mergeStateStatus reviewDecision
        viewerLatestReview {{ state }}
        createdAt updatedAt
        author {{ login }}
        labels(first: 10) {{ nodes {{ name color }} }}
        {threads}
        commits(last: 1) {{
          totalCount
          nodes {{
            commit {{
              statusCheckRollup {{
                contexts(first: 100) {{
                  nodes {{
                    __typename
                    ... on CheckRun {{ name status conclusion startedAt }}
                    ... on StatusContext {{ context state createdAt }}
                  }}
                }}
              }}
            }}
          }}
        }}"#,
        threads = review_threads_selection()
    );

    let include_issues = !matches!(filter_mode, "" | "disabled");

    let mut aliases: Vec<(String, String)> = Vec::new();
    let mut parts = vec!["query BatchPoll {".to_string()];

    for (i, (path, owner, name)) in repos.iter().enumerate() {
        let alias = format!("r{i}");
        let issues_section = if include_issues {
            format!("\n{}", issues_repo_section(filter_mode, viewer))
        } else {
            String::new()
        };
        parts.push(format!(
            "  {alias}: repository(owner: \"{owner}\", name: \"{name}\") {{\n    mergeCommitAllowed\n    squashMergeAllowed\n    rebaseMergeAllowed\n    pullRequests(first: {pr_first}, states: {states}, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{\n      nodes {{ {pr_node_fields} }}\n    }}{issues_section}\n  }}"
        ));
        aliases.push((alias, path.clone()));
    }

    // Supplemental search for viewer's own open PRs across all queried repos.
    // Guarantees the current user's PRs appear even if outside the top-20 by activity.
    if !viewer.is_empty() {
        let repo_filters: String = repos
            .iter()
            .map(|(_, owner, name)| format!("repo:{owner}/{name}"))
            .collect::<Vec<_>>()
            .join(" ");
        let draft_filter = if hide_drafts { " -is:draft" } else { "" };
        let search_query = format!("is:pr is:open author:{viewer}{draft_filter} {repo_filters}");
        parts.push(format!(
            "  viewerPrs: search(query: \"{search_query}\", type: ISSUE, first: 30) {{\n    nodes {{\n      ... on PullRequest {{\n        repository {{ nameWithOwner }}\n        {pr_node_fields}\n      }}\n    }}\n  }}"
        ));
    }

    parts.push("  rateLimit { cost remaining resetAt }".to_string());
    parts.push("}".to_string());

    (parts.join("\n"), aliases)
}

/// Result of a unified batch poll.
pub struct BatchPollResult {
    pub prs: std::collections::HashMap<String, Vec<BranchPrStatus>>,
    /// Non-empty only when filter_mode != "disabled".
    pub issues: std::collections::HashMap<String, Vec<GitHubIssue>>,
}

/// Parse GraphQL PR check contexts into frontend-compatible CiCheckDetail objects.
pub fn parse_pr_check_contexts(data: &serde_json::Value) -> Vec<serde_json::Value> {
    let nodes = &data["data"]["repository"]["pullRequest"]["commits"]["nodes"];
    let contexts = match nodes.as_array().and_then(|a| a.first()) {
        Some(node) => &node["commit"]["statusCheckRollup"]["contexts"],
        None => return vec![],
    };

    // GitHub lists a check name multiple times on the head commit when a workflow
    // re-runs (a stale run cancelled by a `concurrency` group, or a re-run after the
    // base advanced). Dedup to the newest entry per name — the same strategy the
    // summary tally uses in `parse_pr_node` — so the detail list shows no duplicates
    // and agrees with the passed/failed counts.
    dedup_rollup_nodes(contexts)
        .iter()
        .map(|ctx| {
            let typename = ctx["__typename"].as_str().unwrap_or("");
            if typename == "CheckRun" {
                serde_json::json!({
                    "name": ctx["name"].as_str().unwrap_or(""),
                    "status": ctx["status"].as_str().unwrap_or("").to_lowercase(),
                    "conclusion": ctx["conclusion"].as_str().unwrap_or("").to_lowercase(),
                    "html_url": ctx["detailsUrl"].as_str().unwrap_or(""),
                })
            } else {
                // StatusContext
                let state = ctx["state"].as_str().unwrap_or("").to_lowercase();
                let conclusion = match state.as_str() {
                    "success" => "success",
                    "failure" | "error" => "failure",
                    "pending" | "expected" => "",
                    _ => "",
                };
                serde_json::json!({
                    "name": ctx["context"].as_str().unwrap_or(""),
                    "status": if conclusion.is_empty() { "in_progress" } else { "completed" },
                    "conclusion": conclusion,
                    "html_url": ctx["targetUrl"].as_str().unwrap_or(""),
                })
            }
        })
        .collect()
}

/// Head/base branch refs for a PR, used by conflict-assist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrRefs {
    pub head_ref: String,
    pub base_ref: String,
    pub head_sha: Option<String>,
    pub base_sha: Option<String>,
    /// True when the PR head is on a fork (`head.repo != base.repo`) — such a
    /// head branch isn't a plain `origin/<ref>`, so conflict-assist can't rebase
    /// it locally without extra remote setup.
    pub head_from_fork: bool,
}

/// Parse `head.ref` / `base.ref` (and fork detection) from a GitHub PR JSON.
/// Pure — unit-tested. Fork detection compares `head.repo.full_name` against
/// `base.repo.full_name`; when either is absent it conservatively reports the
/// same-repo case (not a fork).
pub fn parse_pr_refs(pr_json: &serde_json::Value) -> Option<PrRefs> {
    let head_ref = pr_json["head"]["ref"].as_str()?.to_string();
    let base_ref = pr_json["base"]["ref"].as_str()?.to_string();
    let head_sha = pr_json["head"]["sha"].as_str().map(str::to_string);
    let base_sha = pr_json["base"]["sha"].as_str().map(str::to_string);
    let head_repo = pr_json["head"]["repo"]["full_name"].as_str();
    let base_repo = pr_json["base"]["repo"]["full_name"].as_str();
    let head_from_fork = match (head_repo, base_repo) {
        (Some(h), Some(b)) => h != b,
        _ => false,
    };
    Some(PrRefs {
        head_ref,
        base_ref,
        head_sha,
        base_sha,
        head_from_fork,
    })
}

/// Maximum characters returned from CI failure logs to avoid overwhelming
/// the agent's context window.
pub const CI_LOG_MAX_CHARS: usize = 4000;

/// Truncate log text to the last [`CI_LOG_MAX_CHARS`] characters, splitting
/// at a newline boundary to avoid cutting mid-line.
pub fn truncate_ci_logs(logs: &str) -> String {
    let logs = logs.trim();
    if logs.len() <= CI_LOG_MAX_CHARS {
        return logs.to_string();
    }
    // Keep the tail — the most relevant failures are usually at the end.
    // The byte cut can land inside a multibyte character; move it forward.
    let mut cut = logs.len() - CI_LOG_MAX_CHARS;
    while !logs.is_char_boundary(cut) {
        cut += 1;
    }
    let truncated = &logs[cut..];
    let start = truncated.find('\n').map(|i| i + 1).unwrap_or(0);
    format!(
        "[… truncated to last ~{CI_LOG_MAX_CHARS} chars …]\n{}",
        &truncated[start..]
    )
}

pub fn sanitize_ci_logs(logs: &str) -> String {
    String::from_utf8_lossy(&tuic_ansi::strip(logs.as_bytes()))
        .chars()
        .filter(|character| {
            (*character == '\n' || *character == '\t' || !character.is_control())
                && !matches!(*character, '\u{00ad}' | '\u{0600}'..='\u{0605}' | '\u{061c}' | '\u{06dd}' | '\u{070f}' | '\u{0890}'..='\u{0891}' | '\u{08e2}' | '\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}' | '\u{110bd}' | '\u{110cd}' | '\u{13430}'..='\u{1343f}' | '\u{1bca0}'..='\u{1bca3}' | '\u{1d173}'..='\u{1d17a}' | '\u{e0000}'..='\u{e007f}')
        })
        .collect()
}

pub fn sanitize_ci_label(label: &str) -> String {
    sanitize_ci_logs(label)
        .chars()
        .filter(|character| !character.is_control())
        .take(200)
        .collect()
}

pub fn format_ci_logs(logs: &str) -> String {
    truncate_ci_logs(&sanitize_ci_logs(logs))
}

/// Return failed job IDs and names from the Actions REST jobs response.
pub fn failed_jobs_from_run_json(value: &serde_json::Value) -> Vec<(u64, String)> {
    value
        .get("jobs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|job| job.get("conclusion").and_then(serde_json::Value::as_str) == Some("failure"))
        .filter_map(|job| {
            let id = job.get("id")?.as_u64()?;
            let name = job
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("failed job")
                .to_string();
            Some((id, sanitize_ci_label(&name)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- review threads ---

    fn thread(resolved: bool, typename: &str, login: &str) -> serde_json::Value {
        serde_json::json!({
            "isResolved": resolved,
            "comments": {"nodes": [{"author": {"__typename": typename, "login": login}}]}
        })
    }

    /// Catches: a blocked PR with open threads reporting no reason, or bot and human
    /// threads collapsed into one number.
    #[test]
    fn review_threads_split_unresolved_by_bot_and_human() {
        let nodes = serde_json::json!([
            thread(false, "User", "alice"),
            thread(false, "User", "bob"),
            thread(false, "Bot", "quill-reviewer"),
            thread(false, "User", "copilot-pull-request-reviewer[bot]"),
            thread(true, "User", "alice"),
        ]);
        assert_eq!(
            count_review_threads(&nodes),
            ReviewThreadCounts {
                bot: 2,
                human: 2,
                ego: 0
            }
        );
        assert_eq!(
            count_review_threads(&serde_json::Value::Null),
            ReviewThreadCounts::default()
        );
    }

    /// Catches: ego's own posted findings counted as human review threads.
    #[test]
    fn ego_marked_threads_count_as_ego_not_human() {
        let first = |body: &str| serde_json::json!({"author": {"__typename": "User", "login": "boss"}, "body": body});
        let nodes = serde_json::json!([
            {"isResolved": false, "comments": {"nodes": [first("Null check\n\n<!-- tuic:ego-review -->")]}},
            {"isResolved": false, "comments": {"nodes": [first("Null check")]}},
            {"isResolved": true, "comments": {"nodes": [first("<!-- tuic:ego-review -->")]}},
        ]);
        assert_eq!(
            count_review_threads(&nodes),
            ReviewThreadCounts {
                bot: 0,
                human: 1,
                ego: 1
            }
        );
    }

    #[test]
    fn pr_node_counts_unresolved_threads_and_defaults_to_zero() {
        let mut node = serde_json::json!({
            "number": 7, "headRefName": "feat/x", "state": "OPEN",
            "reviewThreads": {"nodes": [{"isResolved": false}, {"isResolved": true}, {"isResolved": false}]}
        });
        assert_eq!(parse_pr_node(&node).unwrap().unresolved_threads, 2);
        node.as_object_mut().unwrap().remove("reviewThreads");
        assert_eq!(parse_pr_node(&node).unwrap().unresolved_threads, 0);
    }

    /// Catches: a PR whose first 50 threads are resolved and later ones open reporting a
    /// clean 0 — the flag says the count is only a lower bound.
    #[test]
    fn pr_node_flags_truncated_thread_pages() {
        let node = serde_json::json!({
            "number": 7, "headRefName": "feat/x", "state": "OPEN",
            "reviewThreads": {"pageInfo": {"hasNextPage": true}, "nodes": [{"isResolved": true}]}
        });
        let pr = parse_pr_node(&node).unwrap();
        assert_eq!(pr.unresolved_threads, 0);
        assert!(pr.unresolved_threads_truncated);
    }

    // --- statusCheckRollup dedup + classification tests ---

    #[test]
    fn classify_check_node_maps_each_category() {
        let cr = |status: &str, conclusion: serde_json::Value| serde_json::json!({"__typename": "CheckRun", "status": status, "conclusion": conclusion});
        assert_eq!(
            classify_check_node(&cr("COMPLETED", "SUCCESS".into())),
            CheckCategory::Passed
        );
        assert_eq!(
            classify_check_node(&cr("COMPLETED", "SKIPPED".into())),
            CheckCategory::Passed
        );
        assert_eq!(
            classify_check_node(&cr("COMPLETED", "FAILURE".into())),
            CheckCategory::Failed
        );
        assert_eq!(
            classify_check_node(&cr("COMPLETED", "TIMED_OUT".into())),
            CheckCategory::Failed
        );
        // ACTION_REQUIRED is a blocking conclusion (e.g. security gate) — must
        // count as Failed, NOT Pending, or a blocked PR renders as clean.
        assert_eq!(
            classify_check_node(&cr("COMPLETED", "ACTION_REQUIRED".into())),
            CheckCategory::Failed
        );
        // Not yet COMPLETED → pending regardless of (absent) conclusion.
        assert_eq!(
            classify_check_node(&cr("IN_PROGRESS", serde_json::Value::Null)),
            CheckCategory::Pending
        );
        // StatusContext is classified by its `state`.
        let sc = |state: &str| serde_json::json!({"__typename": "StatusContext", "state": state});
        assert_eq!(classify_check_node(&sc("SUCCESS")), CheckCategory::Passed);
        assert_eq!(classify_check_node(&sc("FAILURE")), CheckCategory::Failed);
        assert_eq!(classify_check_node(&sc("PENDING")), CheckCategory::Pending);
    }

    #[test]
    fn dedup_rollup_keeps_newest_entry_per_check_name() {
        // Same check name run twice (stale FAILURE, then newer SUCCESS) plus one
        // distinct check. GitHub lists all three; we keep the newest per name.
        let contexts = serde_json::json!({
            "nodes": [
                {"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "FAILURE", "startedAt": "2025-01-01T00:00:00Z"},
                {"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "SUCCESS", "startedAt": "2025-01-01T01:00:00Z"},
                {"__typename": "CheckRun", "name": "test", "status": "IN_PROGRESS", "conclusion": serde_json::Value::Null, "startedAt": "2025-01-01T00:00:00Z"},
            ]
        });
        let nodes = dedup_rollup_nodes(&contexts);
        assert_eq!(nodes.len(), 2, "duplicate 'build' must collapse to one");
        let build = nodes.iter().find(|n| n["name"] == "build").unwrap();
        assert_eq!(
            build["conclusion"], "SUCCESS",
            "newest 'build' entry (by startedAt) must win"
        );
        // The deduped set tallies as 1 passed (build) + 1 pending (test), not 3.
        let mut passed = 0;
        let mut pending = 0;
        for n in &nodes {
            match classify_check_node(n) {
                CheckCategory::Passed => passed += 1,
                CheckCategory::Pending => pending += 1,
                CheckCategory::Failed => unreachable!(),
            }
        }
        assert_eq!((passed, pending), (1, 1));
    }

    #[test]
    fn dedup_rollup_handles_missing_nodes() {
        assert_eq!(
            dedup_rollup_nodes(&serde_json::json!({})),
            [] as [serde_json::Value; 0]
        );
        assert_eq!(
            dedup_rollup_nodes(&serde_json::json!({"nodes": []})),
            [] as [serde_json::Value; 0]
        );
    }

    // --- truncate_ci_logs tests ---

    #[test]
    fn test_ci_log_short_output_unchanged() {
        let short = "Error: test failed\nassert_eq failed";
        let result = truncate_ci_logs(short);
        assert_eq!(result, short);
    }

    #[test]
    fn test_ci_log_truncation_keeps_tail() {
        let mut logs = String::new();
        for i in 0..500 {
            logs.push_str(&format!("line {i}: some log output here\n"));
        }
        let result = truncate_ci_logs(&logs);

        assert!(result.starts_with("[… truncated"));
        assert!(result.contains("line 499"));
        assert!(!result.contains("line 0:"));
        // Result length should be manageable
        assert!(result.len() <= CI_LOG_MAX_CHARS + 100); // header adds a bit
    }

    #[test]
    fn test_ci_log_truncation_cut_inside_a_multibyte_char_does_not_panic() {
        // Test runners print checkmarks, box drawing and emoji. A byte cut that
        // lands inside one of them must move to a boundary, not panic the task.
        for glyph in ["é", "✓", "🦀"] {
            for pad in 1..glyph.len() {
                let logs = format!("{}{}", "a".repeat(pad), glyph.repeat(CI_LOG_MAX_CHARS));
                let result = truncate_ci_logs(&logs);

                let tail = result
                    .split_once('\n')
                    .map_or(result.as_str(), |(_, tail)| tail);
                assert!(tail.len() <= CI_LOG_MAX_CHARS, "{glyph} pad {pad}");
                assert!(tail.ends_with(glyph), "{glyph} pad {pad}");
                assert!(
                    tail.chars().all(|c| c.to_string() == glyph),
                    "{glyph} pad {pad}"
                );
            }
        }
    }

    #[test]
    fn test_ci_log_truncation_cut_before_a_newline_keeps_the_next_line() {
        let logs = format!("{}é\nlast failure\n", "x".repeat(CI_LOG_MAX_CHARS));
        let result = truncate_ci_logs(&logs);

        assert!(result.starts_with("[… truncated"), "{result}");
        assert!(result.ends_with("last failure"), "{result}");
    }

    #[test]
    fn test_ci_log_empty_input() {
        assert_eq!(truncate_ci_logs(""), "");
        assert_eq!(truncate_ci_logs("  \n  "), "");
    }
}

#[cfg(test)]
mod critic2_tests {
    use super::*;
    use serde_json::json;

    /// Catches: a panic or a miscount when GitHub returns a thread with no comments,
    /// a deleted (null) author, or a non-user actor type.
    #[test]
    fn thread_attribution_survives_missing_and_odd_authors() {
        let nodes = json!([
            {"isResolved": false, "comments": {"nodes": []}},
            {"isResolved": false, "comments": null},
            {"isResolved": false, "comments": {"nodes": [{"author": null}]}},
            {"isResolved": false, "comments": {"nodes": [{"author": {"__typename": "Mannequin", "login": "m"}}]}},
            {"isResolved": false, "comments": {"nodes": [{"author": {"__typename": "Bot", "login": "dependabot"}}]}},
            {"isResolved": null, "comments": {"nodes": [{"author": {"__typename": "Bot", "login": "x"}}]}},
        ]);
        assert_eq!(
            count_review_threads(&nodes),
            ReviewThreadCounts {
                bot: 1,
                human: 4,
                ego: 0
            }
        );
    }

    /// Catches: batch badge count and on-demand split disagreeing on which nodes are
    /// unresolved (null/missing `isResolved` counted by one and not the other).
    #[test]
    fn badge_count_and_split_agree_on_the_same_nodes() {
        let nodes = json!([
            {"isResolved": false}, {"isResolved": true}, {"isResolved": null}, {},
            {"isResolved": false, "comments": {"nodes": [{"author": {"__typename": "Bot", "login": "b"}}]}},
        ]);
        let counts = count_review_threads(&nodes);
        let pr = parse_pr_node(&json!({
            "number": 1, "headRefName": "f", "state": "OPEN",
            "reviewThreads": {"nodes": nodes}
        }))
        .unwrap();
        assert_eq!(pr.unresolved_threads, counts.total());
    }

    /// Catches: `reviewThreads: null` / `nodes: null` (permission-limited or partial
    /// GraphQL response) failing the whole PR parse instead of counting zero.
    #[test]
    fn null_review_threads_parse_as_zero() {
        for rt in [json!(null), json!({"nodes": null}), json!({"nodes": []})] {
            let pr = parse_pr_node(&json!({
                "number": 1, "headRefName": "f", "state": "OPEN", "reviewThreads": rt
            }))
            .expect("PR must still parse");
            assert_eq!(pr.unresolved_threads, 0);
        }
    }

    /// Catches: threads selected with a nested connection (5 -> 51 points for 6 repos, measured
    /// with rateLimit dryRun) or the page flag dropped, in the worst-case query: 6 repos,
    /// viewer search, drafts hidden. The selection must appear once per repo plus viewerPrs.
    #[test]
    fn batch_query_thread_selection_is_flat_and_flags_truncation() {
        let repos: Vec<(String, String, String)> = (0..6)
            .map(|i| (format!("/r{i}"), "o".into(), format!("r{i}")))
            .collect();
        let (q, _) = build_unified_batch_query(&repos, true, "assigned", "me", true);
        let selection = review_threads_selection();
        assert_eq!(q.matches(&selection).count(), 7); // 6 repos + viewerPrs
        assert!(selection.contains("pageInfo") && selection.contains("isResolved"));
        assert!(
            !q.contains("comments("),
            "no nested comments connection: {q}"
        );
    }

    /// Catches: a PR with more than 50 threads, all resolved, staying "Comments" forever: once
    /// the full walk settles the count, the Ready notice fires.
    #[test]
    fn settled_truncated_threads_let_ready_fire() {
        let open_node = |threads: usize, more: bool| {
            let nodes: Vec<_> = (0..threads)
                .map(|_| serde_json::json!({"isResolved": false}))
                .collect();
            parse_pr_node(&serde_json::json!({
                "number": 42, "headRefName": "feat/test", "state": "OPEN",
                "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
                "reviewDecision": "APPROVED",
                "reviewThreads": {"pageInfo": {"hasNextPage": more}, "nodes": nodes}
            }))
            .unwrap()
        };
        let old = open_node(1, false);
        let mut pr = open_node(0, true);
        assert!(pr.unresolved_threads_truncated);
        pr.settle_review_threads(0, true);
        assert!(!pr.unresolved_threads_truncated);
        assert!(
            crate::github_poller::detect_transitions("/repo", &old, &pr)
                .iter()
                .any(|x| matches!(x, crate::github_poller::PrTransition::Ready { .. }))
        );
        // Walk bound exhausted: still a lower bound, never Ready.
        let mut capped = open_node(0, true);
        capped.settle_review_threads(0, false);
        assert!(capped.unresolved_threads_truncated);
    }
}
