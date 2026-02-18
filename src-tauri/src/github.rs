use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tauri::State;

use crate::state::{AppState, GITHUB_CACHE_TTL};

/// Resolve a GitHub API token from environment or gh CLI config.
/// Order: GH_TOKEN env → GITHUB_TOKEN env → gh CLI config (~/.config/gh/hosts.yml).
/// Returns None if no token is found (graceful degradation).
pub(crate) fn resolve_github_token() -> Option<String> {
    if let Ok(token) = std::env::var("GH_TOKEN") {
        if !token.is_empty() {
            return Some(token);
        }
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            return Some(token);
        }
    }
    gh_token::get().ok()
}

/// Parse a git remote URL into (owner, repo) for GitHub repos.
/// Supports HTTPS (github.com/owner/repo.git) and SSH (git@github.com:owner/repo.git).
pub(crate) fn parse_remote_url(url: &str) -> Option<(String, String)> {
    let url = url.trim();

    // SSH: git@github.com:owner/repo.git
    if let Some(path) = url.strip_prefix("git@github.com:") {
        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // HTTPS: https://github.com/owner/repo.git
    if url.contains("github.com") {
        // Strip protocol and host
        let path = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("github.com/");
        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<&str> = path.splitn(3, '/').collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    None
}

/// Execute a GraphQL query against the GitHub API.
/// Returns the parsed JSON response or an error.
pub(crate) fn graphql_request(
    client: &reqwest::blocking::Client,
    token: &str,
    query: &str,
    variables: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({
        "query": query,
        "variables": variables,
    });

    let response = client
        .post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "tui-commander")
        .json(&body)
        .send()
        .map_err(|e| format!("GraphQL request failed: {e}"))?;

    let status = response.status();
    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("Failed to parse GraphQL response: {e}"))?;

    if !status.is_success() {
        let msg = json["message"].as_str().unwrap_or("Unknown error");
        return Err(format!("GitHub API error ({status}): {msg}"));
    }

    if let Some(errors) = json["errors"].as_array() {
        if !errors.is_empty() {
            let msg = errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error");
            return Err(format!("GraphQL error: {msg}"));
        }
    }

    Ok(json)
}

/// Git remote + branch status (no PR/CI — those come from githubStore via batch query)
#[derive(Clone, Serialize)]
pub(crate) struct GitHubStatus {
    has_remote: bool,
    current_branch: String,
    ahead: i32,
    behind: i32,
}

/// Summary of CI check states for a PR
#[derive(Clone, Serialize)]
pub(crate) struct CheckSummary {
    pub(crate) passed: u32,
    pub(crate) failed: u32,
    pub(crate) pending: u32,
    pub(crate) total: u32,
}

/// Individual CI check detail
#[derive(Clone, Serialize)]
pub(crate) struct CheckDetail {
    pub(crate) context: String,
    pub(crate) state: String,
}

/// Pre-computed merge/review state label for the UI
#[derive(Clone, Serialize, Debug, PartialEq)]
pub(crate) struct StateLabel {
    pub(crate) label: String,
    pub(crate) css_class: String,
}

/// Classify merge readiness from mergeable + merge_state_status fields
pub(crate) fn classify_merge_state(
    mergeable: Option<&str>,
    merge_state_status: Option<&str>,
) -> Option<StateLabel> {
    // CONFLICTING takes priority (merge would fail)
    if mergeable == Some("CONFLICTING") {
        return Some(StateLabel {
            label: "Conflicts".to_string(),
            css_class: "conflicting".to_string(),
        });
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
pub(crate) fn classify_review_state(review_decision: Option<&str>) -> Option<StateLabel> {
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

/// Convert a 6-char hex color to an rgba() CSS string with the given alpha
pub(crate) fn hex_to_rgba(hex: &str, alpha: f64) -> String {
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    format!("rgba({r}, {g}, {b}, {alpha})")
}

/// Determine if a hex color is light (needs dark text) using BT.601 luma
pub(crate) fn is_light_color(hex: &str) -> bool {
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as u32;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as u32;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as u32;
    (r * 299 + g * 587 + b * 114) / 1000 > 128
}

/// PR label with name, hex color, and pre-computed display colors
#[derive(Clone, Serialize)]
pub(crate) struct PrLabel {
    name: String,
    color: String,
    text_color: String,
    background_color: String,
}

/// PR status for a branch, returned by batch endpoint
#[derive(Clone, Serialize)]
pub(crate) struct BranchPrStatus {
    pub(crate) branch: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) state: String,
    pub(crate) url: String,
    pub(crate) additions: i32,
    pub(crate) deletions: i32,
    pub(crate) checks: CheckSummary,
    pub(crate) check_details: Vec<CheckDetail>,
    pub(crate) author: String,
    pub(crate) commits: i32,
    pub(crate) mergeable: String,
    pub(crate) merge_state_status: String,
    pub(crate) review_decision: String,
    pub(crate) labels: Vec<PrLabel>,
    pub(crate) is_draft: bool,
    pub(crate) base_ref_name: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) merge_state_label: Option<StateLabel>,
    pub(crate) review_state_label: Option<StateLabel>,
}

/// Parse `gh pr list` JSON output into BranchPrStatus entries.
/// Pure function with no I/O — fully testable.
pub(crate) fn parse_pr_list_json(json_str: &str) -> Vec<BranchPrStatus> {
    let arr: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    arr.into_iter()
        .filter_map(|v| {
            let branch = v["headRefName"].as_str()?.to_string();
            let number = v["number"].as_i64()? as i32;
            let title = v["title"].as_str().unwrap_or("").to_string();
            let state = v["state"].as_str().unwrap_or("").to_string();
            let url = v["url"].as_str().unwrap_or("").to_string();
            let additions = v["additions"].as_i64().unwrap_or(0) as i32;
            let deletions = v["deletions"].as_i64().unwrap_or(0) as i32;
            let author = v["author"]["login"].as_str().unwrap_or("").to_string();
            let commits = v["commits"]["totalCount"].as_i64().unwrap_or(0) as i32;

            // Parse statusCheckRollup into check summary and details
            let rollup = v["statusCheckRollup"].as_array();
            let mut passed: u32 = 0;
            let mut failed: u32 = 0;
            let mut pending: u32 = 0;
            let mut check_details = Vec::new();

            if let Some(checks) = rollup {
                for check in checks {
                    let context = check["context"].as_str()
                        .or_else(|| check["name"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let state_val = check["state"].as_str()
                        .or_else(|| check["conclusion"].as_str())
                        .unwrap_or("")
                        .to_string();

                    match state_val.as_str() {
                        "SUCCESS" | "success" => passed += 1,
                        "FAILURE" | "failure" | "ERROR" | "error" => failed += 1,
                        _ => pending += 1,
                    }

                    check_details.push(CheckDetail {
                        context,
                        state: state_val,
                    });
                }
            }

            let total = passed + failed + pending;

            let mergeable = v["mergeable"].as_str().unwrap_or("UNKNOWN").to_string();
            let merge_state_status = v["mergeStateStatus"].as_str().unwrap_or("UNKNOWN").to_string();
            let review_decision = v["reviewDecision"].as_str().unwrap_or("").to_string();
            let is_draft = v["isDraft"].as_bool().unwrap_or(false);

            let labels = v["labels"].as_array()
                .map(|arr| arr.iter().filter_map(|l| {
                    let color = l["color"].as_str().unwrap_or("").to_string();
                    let (text_color, background_color) = if color.len() == 6 {
                        let text = if is_light_color(&color) { "#1e1e1e" } else { "#e5e5e5" };
                        (text.to_string(), hex_to_rgba(&color, 0.3))
                    } else {
                        (String::new(), String::new())
                    };
                    Some(PrLabel {
                        name: l["name"].as_str()?.to_string(),
                        color,
                        text_color,
                        background_color,
                    })
                }).collect())
                .unwrap_or_default();

            let base_ref_name = v["baseRefName"].as_str().unwrap_or("").to_string();
            let created_at = v["createdAt"].as_str().unwrap_or("").to_string();
            let updated_at = v["updatedAt"].as_str().unwrap_or("").to_string();

            let merge_state_label = classify_merge_state(
                Some(mergeable.as_str()),
                Some(merge_state_status.as_str()),
            );
            let review_state_label = classify_review_state(
                if review_decision.is_empty() { None } else { Some(review_decision.as_str()) },
            );

            Some(BranchPrStatus {
                branch,
                number,
                title,
                state,
                url,
                additions,
                deletions,
                checks: CheckSummary { passed, failed, pending, total },
                check_details,
                author,
                commits,
                mergeable,
                merge_state_status,
                review_decision,
                labels,
                is_draft,
                base_ref_name,
                created_at,
                updated_at,
                merge_state_label,
                review_state_label,
            })
        })
        .collect()
}

/// Parse a GraphQL PR node into a BranchPrStatus.
/// Shared logic for extracting fields from a single PR node.
fn parse_pr_node(v: &serde_json::Value) -> Option<BranchPrStatus> {
    let branch = v["headRefName"].as_str()?.to_string();
    let number = v["number"].as_i64()? as i32;
    let title = v["title"].as_str().unwrap_or("").to_string();
    let state = v["state"].as_str().unwrap_or("").to_string();
    let url = v["url"].as_str().unwrap_or("").to_string();
    let additions = v["additions"].as_i64().unwrap_or(0) as i32;
    let deletions = v["deletions"].as_i64().unwrap_or(0) as i32;
    let author = v["author"]["login"].as_str().unwrap_or("").to_string();
    let commits = v["commits"]["totalCount"].as_i64().unwrap_or(0) as i32;

    // Parse CI check summary from GraphQL statusCheckRollup
    let rollup_contexts = &v["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"];
    let mut passed: u32 = 0;
    let mut failed: u32 = 0;
    let mut pending: u32 = 0;

    // checkRunCountsByState: [{state: "SUCCESS", count: 5}, ...]
    if let Some(counts) = rollup_contexts["checkRunCountsByState"].as_array() {
        for entry in counts {
            let count = entry["count"].as_u64().unwrap_or(0) as u32;
            match entry["state"].as_str().unwrap_or("") {
                "SUCCESS" | "NEUTRAL" | "SKIPPED" => passed += count,
                "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "STARTUP_FAILURE" => failed += count,
                "ACTION_REQUIRED" | "STALE" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "PENDING" => pending += count,
                _ => pending += count,
            }
        }
    }
    // statusContextCountsByState: same shape for commit statuses
    if let Some(counts) = rollup_contexts["statusContextCountsByState"].as_array() {
        for entry in counts {
            let count = entry["count"].as_u64().unwrap_or(0) as u32;
            match entry["state"].as_str().unwrap_or("") {
                "SUCCESS" => passed += count,
                "FAILURE" | "ERROR" => failed += count,
                _ => pending += count,
            }
        }
    }

    let total = passed + failed + pending;

    let mergeable = v["mergeable"].as_str().unwrap_or("UNKNOWN").to_string();
    let merge_state_status = v["mergeStateStatus"].as_str().unwrap_or("UNKNOWN").to_string();
    let review_decision = v["reviewDecision"].as_str().unwrap_or("").to_string();
    let is_draft = v["isDraft"].as_bool().unwrap_or(false);

    let labels = v["labels"]["nodes"].as_array()
        .map(|arr| arr.iter().filter_map(|l| {
            let color = l["color"].as_str().unwrap_or("").to_string();
            let (text_color, background_color) = if color.len() == 6 {
                let text = if is_light_color(&color) { "#1e1e1e" } else { "#e5e5e5" };
                (text.to_string(), hex_to_rgba(&color, 0.3))
            } else {
                (String::new(), String::new())
            };
            Some(PrLabel {
                name: l["name"].as_str()?.to_string(),
                color,
                text_color,
                background_color,
            })
        }).collect())
        .unwrap_or_default();

    let base_ref_name = v["baseRefName"].as_str().unwrap_or("").to_string();
    let created_at = v["createdAt"].as_str().unwrap_or("").to_string();
    let updated_at = v["updatedAt"].as_str().unwrap_or("").to_string();

    let merge_state_label = classify_merge_state(
        Some(mergeable.as_str()),
        Some(merge_state_status.as_str()),
    );
    let review_state_label = classify_review_state(
        if review_decision.is_empty() { None } else { Some(review_decision.as_str()) },
    );

    Some(BranchPrStatus {
        branch,
        number,
        title,
        state,
        url,
        additions,
        deletions,
        checks: CheckSummary { passed, failed, pending, total },
        check_details: vec![], // Populated on-demand via per-PR query
        author,
        commits,
        mergeable,
        merge_state_status,
        review_decision,
        labels,
        is_draft,
        base_ref_name,
        created_at,
        updated_at,
        merge_state_label,
        review_state_label,
    })
}

/// Parse a GraphQL batch PR response into BranchPrStatus entries.
/// Input: full GraphQL response JSON (with data.repository.pullRequests.nodes).
pub(crate) fn parse_graphql_prs(response: &serde_json::Value) -> Vec<BranchPrStatus> {
    let nodes = match response["data"]["repository"]["pullRequests"]["nodes"].as_array() {
        Some(arr) => arr,
        None => return vec![],
    };

    nodes.iter().filter_map(|v| parse_pr_node(v)).collect()
}

/// GraphQL query for batch PR data with CI check summary counts.
/// Uses checkRunCountsByState for efficient aggregation (no per-check iteration).
const BATCH_PR_QUERY: &str = r#"
query RepoPRs($owner: String!, $repo: String!, $first: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: $first, states: [OPEN, CLOSED, MERGED],
                 orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes {
        number title state url headRefName baseRefName isDraft
        additions deletions mergeable mergeStateStatus reviewDecision
        createdAt updatedAt
        author { login }
        labels(first: 10) { nodes { name color } }
        commits(last: 1) {
          totalCount
          nodes {
            commit {
              statusCheckRollup {
                contexts {
                  checkRunCountsByState { state count }
                  statusContextCountsByState { state count }
                }
              }
            }
          }
        }
      }
    }
  }
  rateLimit { cost remaining resetAt }
}
"#;

/// Get the remote URL for a repo, if it has a GitHub origin.
fn get_github_remote_url(repo_path: &PathBuf) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.contains("github.com") {
        Some(url)
    } else {
        None
    }
}

/// Core logic for fetching PR statuses via GitHub GraphQL API (no caching).
pub(crate) fn get_repo_pr_statuses_impl(
    path: &str,
    client: &reqwest::blocking::Client,
    token: Option<&str>,
) -> Vec<BranchPrStatus> {
    let repo_path = PathBuf::from(path);

    let token = match token {
        Some(t) => t,
        None => return vec![], // No token = no GitHub API access
    };

    let remote_url = match get_github_remote_url(&repo_path) {
        Some(url) => url,
        None => return vec![],
    };

    let (owner, repo) = match parse_remote_url(&remote_url) {
        Some(pair) => pair,
        None => return vec![],
    };

    let variables = serde_json::json!({
        "owner": owner,
        "repo": repo,
        "first": 50,
    });

    match graphql_request(client, token, BATCH_PR_QUERY, variables) {
        Ok(response) => parse_graphql_prs(&response),
        Err(e) => {
            eprintln!("[github] GraphQL batch PR query failed: {e}");
            vec![]
        }
    }
}

/// Get all open PR statuses for a repository (cached, 30s TTL)
#[tauri::command]
pub(crate) fn get_repo_pr_statuses(state: State<'_, Arc<AppState>>, path: String) -> Vec<BranchPrStatus> {
    if let Some(cached) = AppState::get_cached(&state.github_status_cache, &path, GITHUB_CACHE_TTL) {
        return cached;
    }

    let statuses = get_repo_pr_statuses_impl(
        &path,
        &state.http_client,
        state.github_token.as_deref(),
    );
    AppState::set_cached(&state.github_status_cache, path, statuses.clone());
    statuses
}

/// Get git remote + branch status for a repository.
/// PR and CI data now comes from the batch githubStore (GraphQL),
/// so this only returns has_remote, current_branch, ahead, and behind.
#[tauri::command]
pub(crate) fn get_github_status(path: String) -> GitHubStatus {
    let repo_path = PathBuf::from(&path);

    let has_remote = get_github_remote_url(&repo_path).is_some();

    // Get current branch
    let current_branch = Command::new("git")
        .current_dir(&repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    if !has_remote {
        return GitHubStatus {
            has_remote: false,
            current_branch,
            ahead: 0,
            behind: 0,
        };
    }

    // Get ahead/behind counts
    let (ahead, behind) = Command::new("git")
        .current_dir(&repo_path)
        .args(["rev-list", "--left-right", "--count", &format!("origin/{current_branch}...HEAD")])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let output = String::from_utf8_lossy(&o.stdout);
                let parts: Vec<&str> = output.split_whitespace().collect();
                if parts.len() == 2 {
                    let behind = parts[0].parse::<i32>().unwrap_or(0);
                    let ahead = parts[1].parse::<i32>().unwrap_or(0);
                    return Some((ahead, behind));
                }
            }
            None
        })
        .unwrap_or((0, 0));

    GitHubStatus {
        has_remote,
        current_branch,
        ahead,
        behind,
    }
}

const PR_CHECKS_QUERY: &str = r#"
query PRChecks($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      commits(last: 1) {
        nodes {
          commit {
            statusCheckRollup {
              contexts(first: 50) {
                nodes {
                  __typename
                  ... on CheckRun { name status conclusion detailsUrl }
                  ... on StatusContext { context state targetUrl }
                }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// Parse GraphQL PR check contexts into frontend-compatible CiCheckDetail objects.
fn parse_pr_check_contexts(data: &serde_json::Value) -> Vec<serde_json::Value> {
    let nodes = &data["data"]["repository"]["pullRequest"]["commits"]["nodes"];
    let contexts = match nodes.as_array().and_then(|a| a.first()) {
        Some(node) => &node["commit"]["statusCheckRollup"]["contexts"]["nodes"],
        None => return vec![],
    };

    let context_nodes = match contexts.as_array() {
        Some(arr) => arr,
        None => return vec![],
    };

    context_nodes.iter().map(|ctx| {
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
    }).collect()
}

/// Core logic for fetching CI check details via GitHub GraphQL API (no caching).
pub(crate) fn get_ci_checks_impl(
    path: &str,
    pr_number: i64,
    client: &reqwest::blocking::Client,
    token: Option<&str>,
) -> Vec<serde_json::Value> {
    let repo_path = PathBuf::from(path);

    let token = match token {
        Some(t) => t,
        None => return vec![],
    };

    let remote_url = match get_github_remote_url(&repo_path) {
        Some(url) => url,
        None => return vec![],
    };

    let (owner, repo) = match parse_remote_url(&remote_url) {
        Some(pair) => pair,
        None => return vec![],
    };

    let variables = serde_json::json!({
        "owner": owner,
        "repo": repo,
        "number": pr_number,
    });

    match graphql_request(client, token, PR_CHECKS_QUERY, variables) {
        Ok(data) => parse_pr_check_contexts(&data),
        Err(e) => {
            eprintln!("[github] GraphQL PR checks query failed: {}", e);
            vec![]
        }
    }
}

/// Get CI check details for a PR via GitHub GraphQL API (Story 060)
#[tauri::command]
pub(crate) fn get_ci_checks(
    path: String,
    pr_number: i64,
    state: State<'_, Arc<AppState>>,
) -> Vec<serde_json::Value> {
    get_ci_checks_impl(&path, pr_number, &state.http_client, state.github_token.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pr_list_json_success() {
        let json = r#"[
            {
                "number": 42,
                "title": "Add feature X",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/42",
                "headRefName": "feature/x",
                "additions": 150,
                "deletions": 30,
                "author": {"login": "alice"},
                "commits": {"totalCount": 5},
                "mergeable": "MERGEABLE",
                "mergeStateStatus": "BLOCKED",
                "reviewDecision": "CHANGES_REQUESTED",
                "statusCheckRollup": [
                    {"context": "build", "state": "SUCCESS"},
                    {"context": "test", "state": "FAILURE"},
                    {"name": "lint", "conclusion": "success"},
                    {"context": "deploy", "state": "PENDING"}
                ]
            },
            {
                "number": 43,
                "title": "Fix bug Y",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/43",
                "headRefName": "fix/y",
                "additions": 10,
                "deletions": 5,
                "author": {"login": "bob"},
                "commits": {"totalCount": 1},
                "mergeable": "MERGEABLE",
                "mergeStateStatus": "CLEAN",
                "reviewDecision": "APPROVED",
                "statusCheckRollup": [
                    {"context": "build", "state": "SUCCESS"},
                    {"context": "test", "state": "SUCCESS"}
                ]
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 2);

        // First PR
        let pr1 = &result[0];
        assert_eq!(pr1.branch, "feature/x");
        assert_eq!(pr1.number, 42);
        assert_eq!(pr1.title, "Add feature X");
        assert_eq!(pr1.state, "OPEN");
        assert_eq!(pr1.url, "https://github.com/org/repo/pull/42");
        assert_eq!(pr1.additions, 150);
        assert_eq!(pr1.deletions, 30);
        assert_eq!(pr1.author, "alice");
        assert_eq!(pr1.commits, 5);
        assert_eq!(pr1.checks.passed, 2); // build SUCCESS + lint success
        assert_eq!(pr1.checks.failed, 1); // test FAILURE
        assert_eq!(pr1.checks.pending, 1); // deploy PENDING
        assert_eq!(pr1.checks.total, 4);
        assert_eq!(pr1.check_details.len(), 4);
        assert_eq!(pr1.mergeable, "MERGEABLE");
        assert_eq!(pr1.merge_state_status, "BLOCKED");
        assert_eq!(pr1.review_decision, "CHANGES_REQUESTED");

        // Second PR
        let pr2 = &result[1];
        assert_eq!(pr2.branch, "fix/y");
        assert_eq!(pr2.number, 43);
        assert_eq!(pr2.author, "bob");
        assert_eq!(pr2.checks.passed, 2);
        assert_eq!(pr2.checks.failed, 0);
        assert_eq!(pr2.checks.pending, 0);
        assert_eq!(pr2.checks.total, 2);
        assert_eq!(pr2.mergeable, "MERGEABLE");
        assert_eq!(pr2.merge_state_status, "CLEAN");
        assert_eq!(pr2.review_decision, "APPROVED");
    }

    #[test]
    fn test_parse_pr_list_json_empty() {
        let result = parse_pr_list_json("[]");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_pr_list_json_missing_mergeable_defaults_to_unknown() {
        let json = r#"[
            {
                "number": 7,
                "title": "No merge info",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/7",
                "headRefName": "no-merge-info",
                "author": {"login": "frank"},
                "commits": {"totalCount": 1},
                "statusCheckRollup": []
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mergeable, "UNKNOWN");
        assert_eq!(result[0].merge_state_status, "UNKNOWN");
    }

    #[test]
    fn test_parse_pr_list_json_no_checks() {
        let json = r#"[
            {
                "number": 10,
                "title": "Draft PR",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/10",
                "headRefName": "draft/feature",
                "additions": 0,
                "deletions": 0,
                "author": {"login": "carol"},
                "commits": {"totalCount": 1},
                "statusCheckRollup": []
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].checks.total, 0);
        assert_eq!(result[0].checks.passed, 0);
        assert_eq!(result[0].checks.failed, 0);
        assert_eq!(result[0].checks.pending, 0);
        assert!(result[0].check_details.is_empty());
    }

    #[test]
    fn test_parse_pr_list_json_malformed() {
        let result = parse_pr_list_json("not json at all");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_pr_list_json_missing_branch_skips_entry() {
        // PR without headRefName should be skipped (filter_map returns None)
        let json = r#"[
            {
                "number": 1,
                "title": "No branch",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/1"
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_pr_list_json_null_rollup() {
        // statusCheckRollup is null (not an array)
        let json = r#"[
            {
                "number": 5,
                "title": "No CI",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/5",
                "headRefName": "no-ci",
                "additions": 1,
                "deletions": 0,
                "author": {"login": "dave"},
                "commits": {"totalCount": 1},
                "statusCheckRollup": null
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].checks.total, 0);
        assert!(result[0].check_details.is_empty());
    }

    #[test]
    fn test_parse_pr_list_json_error_states() {
        let json = r#"[
            {
                "number": 99,
                "title": "Error checks",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/99",
                "headRefName": "error-branch",
                "author": {"login": "eve"},
                "commits": {"totalCount": 1},
                "statusCheckRollup": [
                    {"context": "security", "state": "ERROR"},
                    {"context": "build", "state": "error"}
                ]
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].checks.failed, 2); // ERROR and error both count as failed
        assert_eq!(result[0].checks.passed, 0);
        assert_eq!(result[0].checks.pending, 0);
    }

    #[test]
    fn test_parse_pr_list_json_merged_and_closed_states() {
        let json = r#"[
            {
                "number": 10,
                "title": "Merged feature",
                "state": "MERGED",
                "url": "https://github.com/org/repo/pull/10",
                "headRefName": "feature/merged",
                "author": {"login": "alice"},
                "commits": {"totalCount": 3},
                "mergeable": "UNKNOWN",
                "mergeStateStatus": "UNKNOWN",
                "statusCheckRollup": []
            },
            {
                "number": 11,
                "title": "Closed PR",
                "state": "CLOSED",
                "url": "https://github.com/org/repo/pull/11",
                "headRefName": "feature/closed",
                "author": {"login": "bob"},
                "commits": {"totalCount": 1},
                "statusCheckRollup": []
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].state, "MERGED");
        assert_eq!(result[0].branch, "feature/merged");
        assert_eq!(result[1].state, "CLOSED");
        assert_eq!(result[1].branch, "feature/closed");
    }

    // --- hex_to_rgba tests ---

    #[test]
    fn test_hex_to_rgba_red_label() {
        assert_eq!(hex_to_rgba("d73a4a", 0.3), "rgba(215, 58, 74, 0.3)");
    }

    #[test]
    fn test_hex_to_rgba_light_blue_label() {
        assert_eq!(hex_to_rgba("a2eeef", 0.3), "rgba(162, 238, 239, 0.3)");
    }

    #[test]
    fn test_hex_to_rgba_black() {
        assert_eq!(hex_to_rgba("000000", 0.3), "rgba(0, 0, 0, 0.3)");
    }

    #[test]
    fn test_hex_to_rgba_white() {
        assert_eq!(hex_to_rgba("ffffff", 0.3), "rgba(255, 255, 255, 0.3)");
    }

    #[test]
    fn test_hex_to_rgba_full_opacity() {
        assert_eq!(hex_to_rgba("ff0000", 1.0), "rgba(255, 0, 0, 1)");
    }

    // --- is_light_color tests ---

    #[test]
    fn test_is_light_color_dark_red() {
        // d73a4a: (215*299+58*587+74*114)/1000 = 106.767 < 128
        assert!(!is_light_color("d73a4a"));
    }

    #[test]
    fn test_is_light_color_light_blue() {
        // a2eeef: (162*299+238*587+239*114)/1000 = 215.39 > 128
        assert!(is_light_color("a2eeef"));
    }

    #[test]
    fn test_is_light_color_black() {
        assert!(!is_light_color("000000"));
    }

    #[test]
    fn test_is_light_color_white() {
        assert!(is_light_color("ffffff"));
    }

    #[test]
    fn test_is_light_color_mid_gray() {
        // 808080: (128*299+128*587+128*114)/1000 = 128.0, NOT > 128 => dark
        assert!(!is_light_color("808080"));
    }

    #[test]
    fn test_is_light_color_just_above_threshold() {
        // 818181: (129*299+129*587+129*114)/1000 = 129.0 > 128
        assert!(is_light_color("818181"));
    }

    // --- label color pre-computation in parse_pr_list_json ---

    #[test]
    fn test_parse_pr_list_json_computes_label_colors() {
        let json = r#"[
            {
                "number": 1,
                "title": "Labels PR",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/1",
                "headRefName": "label-branch",
                "author": {"login": "alice"},
                "commits": {"totalCount": 1},
                "statusCheckRollup": [],
                "labels": [
                    {"name": "bug", "color": "d73a4a"},
                    {"name": "enhancement", "color": "a2eeef"}
                ]
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].labels.len(), 2);

        let bug = &result[0].labels[0];
        assert_eq!(bug.name, "bug");
        assert_eq!(bug.color, "d73a4a");
        assert_eq!(bug.background_color, "rgba(215, 58, 74, 0.3)");
        assert_eq!(bug.text_color, "#e5e5e5"); // dark label => light text

        let enhancement = &result[0].labels[1];
        assert_eq!(enhancement.name, "enhancement");
        assert_eq!(enhancement.color, "a2eeef");
        assert_eq!(enhancement.background_color, "rgba(162, 238, 239, 0.3)");
        assert_eq!(enhancement.text_color, "#1e1e1e"); // light label => dark text
    }

    // --- classify_merge_state tests ---

    #[test]
    fn test_classify_merge_state_conflicting_overrides_status() {
        let result = classify_merge_state(Some("CONFLICTING"), Some("CLEAN"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Conflicts".to_string(), css_class: "conflicting".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_clean() {
        let result = classify_merge_state(Some("MERGEABLE"), Some("CLEAN"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Ready to merge".to_string(), css_class: "clean".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_behind() {
        let result = classify_merge_state(Some("MERGEABLE"), Some("BEHIND"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Behind base".to_string(), css_class: "behind".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_blocked() {
        let result = classify_merge_state(Some("MERGEABLE"), Some("BLOCKED"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Blocked".to_string(), css_class: "blocked".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_unstable() {
        let result = classify_merge_state(Some("MERGEABLE"), Some("UNSTABLE"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Unstable".to_string(), css_class: "blocked".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_draft() {
        let result = classify_merge_state(Some("MERGEABLE"), Some("DRAFT"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Draft".to_string(), css_class: "behind".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_dirty() {
        let result = classify_merge_state(Some("MERGEABLE"), Some("DIRTY"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Conflicts".to_string(), css_class: "conflicting".to_string() })
        );
    }

    #[test]
    fn test_classify_merge_state_unknown_returns_none() {
        assert!(classify_merge_state(Some("MERGEABLE"), Some("UNKNOWN")).is_none());
    }

    #[test]
    fn test_classify_merge_state_has_hooks_returns_none() {
        assert!(classify_merge_state(Some("MERGEABLE"), Some("HAS_HOOKS")).is_none());
    }

    #[test]
    fn test_classify_merge_state_none_none_returns_none() {
        assert!(classify_merge_state(None, None).is_none());
    }

    // --- classify_review_state tests ---

    #[test]
    fn test_classify_review_state_approved() {
        let result = classify_review_state(Some("APPROVED"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Approved".to_string(), css_class: "approved".to_string() })
        );
    }

    #[test]
    fn test_classify_review_state_changes_requested() {
        let result = classify_review_state(Some("CHANGES_REQUESTED"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Changes requested".to_string(), css_class: "changes-requested".to_string() })
        );
    }

    #[test]
    fn test_classify_review_state_review_required() {
        let result = classify_review_state(Some("REVIEW_REQUIRED"));
        assert_eq!(
            result,
            Some(StateLabel { label: "Review required".to_string(), css_class: "review-required".to_string() })
        );
    }

    #[test]
    fn test_classify_review_state_none_returns_none() {
        assert!(classify_review_state(None).is_none());
    }

    #[test]
    fn test_classify_review_state_empty_returns_none() {
        assert!(classify_review_state(Some("")).is_none());
    }

    // --- Integration: verify parse_pr_list_json populates computed labels ---

    #[test]
    fn test_parse_pr_list_json_computes_merge_and_review_labels() {
        let json = r#"[
            {
                "number": 1,
                "title": "Clean PR",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/1",
                "headRefName": "clean-branch",
                "author": {"login": "alice"},
                "commits": {"totalCount": 1},
                "mergeable": "MERGEABLE",
                "mergeStateStatus": "CLEAN",
                "reviewDecision": "APPROVED",
                "statusCheckRollup": []
            },
            {
                "number": 2,
                "title": "Conflicting PR",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/2",
                "headRefName": "conflict-branch",
                "author": {"login": "bob"},
                "commits": {"totalCount": 1},
                "mergeable": "CONFLICTING",
                "mergeStateStatus": "DIRTY",
                "reviewDecision": "CHANGES_REQUESTED",
                "statusCheckRollup": []
            },
            {
                "number": 3,
                "title": "Unknown PR",
                "state": "OPEN",
                "url": "https://github.com/org/repo/pull/3",
                "headRefName": "unknown-branch",
                "author": {"login": "carol"},
                "commits": {"totalCount": 1},
                "mergeable": "UNKNOWN",
                "mergeStateStatus": "UNKNOWN",
                "statusCheckRollup": []
            }
        ]"#;

        let result = parse_pr_list_json(json);
        assert_eq!(result.len(), 3);

        // Clean + Approved
        assert_eq!(
            result[0].merge_state_label,
            Some(StateLabel { label: "Ready to merge".to_string(), css_class: "clean".to_string() })
        );
        assert_eq!(
            result[0].review_state_label,
            Some(StateLabel { label: "Approved".to_string(), css_class: "approved".to_string() })
        );

        // Conflicting (mergeable overrides status) + Changes requested
        assert_eq!(
            result[1].merge_state_label,
            Some(StateLabel { label: "Conflicts".to_string(), css_class: "conflicting".to_string() })
        );
        assert_eq!(
            result[1].review_state_label,
            Some(StateLabel { label: "Changes requested".to_string(), css_class: "changes-requested".to_string() })
        );

        // Unknown — both None
        assert!(result[2].merge_state_label.is_none());
        assert!(result[2].review_state_label.is_none());
    }

    // --- parse_graphql_prs tests ---

    /// Helper to build a GraphQL PR node for testing
    fn graphql_pr_node(
        number: i64,
        title: &str,
        state: &str,
        branch: &str,
        additions: i64,
        deletions: i64,
        author: &str,
        commits_count: i64,
        check_run_counts: &[(&str, u64)],
        status_context_counts: &[(&str, u64)],
        mergeable: &str,
        merge_state_status: &str,
        review_decision: Option<&str>,
        is_draft: bool,
        labels: &[(&str, &str)],
        base_ref_name: &str,
    ) -> serde_json::Value {
        let check_run_counts_json: Vec<serde_json::Value> = check_run_counts.iter()
            .map(|(s, c)| serde_json::json!({"state": s, "count": c}))
            .collect();
        let status_context_counts_json: Vec<serde_json::Value> = status_context_counts.iter()
            .map(|(s, c)| serde_json::json!({"state": s, "count": c}))
            .collect();
        let labels_json: Vec<serde_json::Value> = labels.iter()
            .map(|(name, color)| serde_json::json!({"name": name, "color": color}))
            .collect();

        serde_json::json!({
            "number": number,
            "title": title,
            "state": state,
            "url": format!("https://github.com/org/repo/pull/{number}"),
            "headRefName": branch,
            "baseRefName": base_ref_name,
            "isDraft": is_draft,
            "additions": additions,
            "deletions": deletions,
            "mergeable": mergeable,
            "mergeStateStatus": merge_state_status,
            "reviewDecision": review_decision,
            "createdAt": "2025-01-01T00:00:00Z",
            "updatedAt": "2025-01-02T00:00:00Z",
            "author": {"login": author},
            "labels": {"nodes": labels_json},
            "commits": {
                "totalCount": commits_count,
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": {
                            "contexts": {
                                "checkRunCountsByState": check_run_counts_json,
                                "statusContextCountsByState": status_context_counts_json,
                            }
                        }
                    }
                }]
            }
        })
    }

    /// Wrap PR nodes into a full GraphQL response
    fn graphql_response(nodes: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": nodes
                    }
                }
            },
            "rateLimit": {"cost": 1, "remaining": 4999, "resetAt": "2025-01-01T01:00:00Z"}
        })
    }

    #[test]
    fn test_parse_graphql_prs_basic() {
        let response = graphql_response(vec![
            graphql_pr_node(42, "Add feature X", "OPEN", "feature/x",
                150, 30, "alice", 5,
                &[("SUCCESS", 2), ("FAILURE", 1)],
                &[("PENDING", 1)],
                "MERGEABLE", "BLOCKED", Some("CHANGES_REQUESTED"), false,
                &[], "main"),
            graphql_pr_node(43, "Fix bug Y", "OPEN", "fix/y",
                10, 5, "bob", 1,
                &[("SUCCESS", 2)],
                &[],
                "MERGEABLE", "CLEAN", Some("APPROVED"), false,
                &[], "main"),
        ]);

        let result = parse_graphql_prs(&response);
        assert_eq!(result.len(), 2);

        let pr1 = &result[0];
        assert_eq!(pr1.branch, "feature/x");
        assert_eq!(pr1.number, 42);
        assert_eq!(pr1.title, "Add feature X");
        assert_eq!(pr1.state, "OPEN");
        assert_eq!(pr1.additions, 150);
        assert_eq!(pr1.deletions, 30);
        assert_eq!(pr1.author, "alice");
        assert_eq!(pr1.commits, 5);
        assert_eq!(pr1.checks.passed, 2);
        assert_eq!(pr1.checks.failed, 1);
        assert_eq!(pr1.checks.pending, 1);
        assert_eq!(pr1.checks.total, 4);
        assert!(pr1.check_details.is_empty()); // Empty for batch query

        let pr2 = &result[1];
        assert_eq!(pr2.branch, "fix/y");
        assert_eq!(pr2.number, 43);
        assert_eq!(pr2.checks.passed, 2);
        assert_eq!(pr2.checks.failed, 0);
        assert_eq!(pr2.checks.pending, 0);
        assert_eq!(pr2.checks.total, 2);
    }

    #[test]
    fn test_parse_graphql_prs_empty_nodes() {
        let response = graphql_response(vec![]);
        let result = parse_graphql_prs(&response);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_graphql_prs_no_data() {
        let response = serde_json::json!({"errors": [{"message": "something went wrong"}]});
        let result = parse_graphql_prs(&response);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_graphql_prs_missing_branch_skips() {
        let mut node = graphql_pr_node(1, "No branch", "OPEN", "test", 0, 0, "alice", 1,
            &[], &[], "UNKNOWN", "UNKNOWN", None, false, &[], "main");
        // Remove headRefName
        node.as_object_mut().unwrap().remove("headRefName");
        let response = graphql_response(vec![node]);
        let result = parse_graphql_prs(&response);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_graphql_prs_no_checks() {
        let response = graphql_response(vec![
            graphql_pr_node(10, "Draft PR", "OPEN", "draft/feature",
                0, 0, "carol", 1,
                &[], &[],
                "UNKNOWN", "DRAFT", None, true, &[], "main"),
        ]);

        let result = parse_graphql_prs(&response);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].checks.total, 0);
        assert!(result[0].is_draft);
    }

    #[test]
    fn test_parse_graphql_prs_labels_with_colors() {
        let response = graphql_response(vec![
            graphql_pr_node(1, "Labels PR", "OPEN", "label-branch",
                0, 0, "alice", 1,
                &[], &[],
                "UNKNOWN", "UNKNOWN", None, false,
                &[("bug", "d73a4a"), ("enhancement", "a2eeef")], "main"),
        ]);

        let result = parse_graphql_prs(&response);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].labels.len(), 2);

        let bug = &result[0].labels[0];
        assert_eq!(bug.name, "bug");
        assert_eq!(bug.color, "d73a4a");
        assert_eq!(bug.background_color, "rgba(215, 58, 74, 0.3)");
        assert_eq!(bug.text_color, "#e5e5e5"); // dark label => light text

        let enh = &result[0].labels[1];
        assert_eq!(enh.name, "enhancement");
        assert_eq!(enh.text_color, "#1e1e1e"); // light label => dark text
    }

    #[test]
    fn test_parse_graphql_prs_merge_and_review_labels() {
        let response = graphql_response(vec![
            graphql_pr_node(1, "Clean PR", "OPEN", "clean-branch",
                0, 0, "alice", 1,
                &[], &[],
                "MERGEABLE", "CLEAN", Some("APPROVED"), false, &[], "main"),
            graphql_pr_node(2, "Conflicting PR", "OPEN", "conflict-branch",
                0, 0, "bob", 1,
                &[], &[],
                "CONFLICTING", "DIRTY", Some("CHANGES_REQUESTED"), false, &[], "main"),
        ]);

        let result = parse_graphql_prs(&response);
        assert_eq!(result.len(), 2);

        assert_eq!(
            result[0].merge_state_label,
            Some(StateLabel { label: "Ready to merge".to_string(), css_class: "clean".to_string() })
        );
        assert_eq!(
            result[0].review_state_label,
            Some(StateLabel { label: "Approved".to_string(), css_class: "approved".to_string() })
        );

        assert_eq!(
            result[1].merge_state_label,
            Some(StateLabel { label: "Conflicts".to_string(), css_class: "conflicting".to_string() })
        );
    }

    #[test]
    fn test_parse_graphql_prs_error_check_states() {
        let response = graphql_response(vec![
            graphql_pr_node(99, "Error checks", "OPEN", "error-branch",
                0, 0, "eve", 1,
                &[("ERROR", 1), ("TIMED_OUT", 1), ("CANCELLED", 1)],
                &[("ERROR", 1)],
                "UNKNOWN", "UNKNOWN", None, false, &[], "main"),
        ]);

        let result = parse_graphql_prs(&response);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].checks.failed, 4); // ERROR + TIMED_OUT + CANCELLED + status ERROR
    }

    #[test]
    fn test_parse_graphql_prs_merged_and_closed() {
        let response = graphql_response(vec![
            graphql_pr_node(10, "Merged feature", "MERGED", "feature/merged",
                0, 0, "alice", 3,
                &[], &[],
                "UNKNOWN", "UNKNOWN", None, false, &[], "main"),
            graphql_pr_node(11, "Closed PR", "CLOSED", "feature/closed",
                0, 0, "bob", 1,
                &[], &[],
                "UNKNOWN", "UNKNOWN", None, false, &[], "main"),
        ]);

        let result = parse_graphql_prs(&response);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].state, "MERGED");
        assert_eq!(result[1].state, "CLOSED");
    }

    // --- parse_remote_url tests ---

    #[test]
    fn test_parse_remote_url_https() {
        let result = parse_remote_url("https://github.com/owner/repo.git");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn test_parse_remote_url_https_no_git_suffix() {
        let result = parse_remote_url("https://github.com/owner/repo");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn test_parse_remote_url_ssh() {
        let result = parse_remote_url("git@github.com:owner/repo.git");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn test_parse_remote_url_ssh_no_git_suffix() {
        let result = parse_remote_url("git@github.com:owner/repo");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn test_parse_remote_url_with_trailing_newline() {
        let result = parse_remote_url("https://github.com/owner/repo.git\n");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn test_parse_remote_url_not_github() {
        let result = parse_remote_url("https://gitlab.com/owner/repo.git");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_remote_url_empty() {
        assert_eq!(parse_remote_url(""), None);
    }

    #[test]
    fn test_parse_remote_url_malformed() {
        assert_eq!(parse_remote_url("not-a-url"), None);
    }

    // --- resolve_github_token tests ---
    // All env var scenarios in a single test to avoid parallel race conditions
    // (env vars are process-global state).

    #[test]
    fn test_resolve_github_token_env_priority() {
        // Scenario 1: GH_TOKEN takes priority
        unsafe {
            std::env::set_var("GH_TOKEN", "gh-wins");
            std::env::set_var("GITHUB_TOKEN", "github-loses");
        }
        assert_eq!(resolve_github_token(), Some("gh-wins".to_string()));

        // Scenario 2: Falls back to GITHUB_TOKEN when GH_TOKEN absent
        unsafe {
            std::env::remove_var("GH_TOKEN");
            std::env::set_var("GITHUB_TOKEN", "github-token-456");
        }
        assert_eq!(resolve_github_token(), Some("github-token-456".to_string()));

        // Scenario 3: Empty GH_TOKEN is skipped, falls back to GITHUB_TOKEN
        unsafe {
            std::env::set_var("GH_TOKEN", "");
            std::env::set_var("GITHUB_TOKEN", "fallback");
        }
        assert_eq!(resolve_github_token(), Some("fallback".to_string()));

        // Cleanup
        unsafe {
            std::env::remove_var("GH_TOKEN");
            std::env::remove_var("GITHUB_TOKEN");
        }
    }

    // --- parse_pr_check_contexts tests ---

    #[test]
    fn test_parse_pr_check_contexts_check_runs() {
        let data = serde_json::json!({
            "data": {
                "repository": {
                    "pullRequest": {
                        "commits": {
                            "nodes": [{
                                "commit": {
                                    "statusCheckRollup": {
                                        "contexts": {
                                            "nodes": [
                                                {
                                                    "__typename": "CheckRun",
                                                    "name": "build",
                                                    "status": "COMPLETED",
                                                    "conclusion": "SUCCESS",
                                                    "detailsUrl": "https://github.com/runs/1"
                                                },
                                                {
                                                    "__typename": "CheckRun",
                                                    "name": "test",
                                                    "status": "COMPLETED",
                                                    "conclusion": "FAILURE",
                                                    "detailsUrl": "https://github.com/runs/2"
                                                }
                                            ]
                                        }
                                    }
                                }
                            }]
                        }
                    }
                }
            }
        });

        let result = parse_pr_check_contexts(&data);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["name"], "build");
        assert_eq!(result[0]["conclusion"], "success");
        assert_eq!(result[0]["html_url"], "https://github.com/runs/1");
        assert_eq!(result[1]["name"], "test");
        assert_eq!(result[1]["conclusion"], "failure");
    }

    #[test]
    fn test_parse_pr_check_contexts_status_contexts() {
        let data = serde_json::json!({
            "data": {
                "repository": {
                    "pullRequest": {
                        "commits": {
                            "nodes": [{
                                "commit": {
                                    "statusCheckRollup": {
                                        "contexts": {
                                            "nodes": [
                                                {
                                                    "__typename": "StatusContext",
                                                    "context": "ci/circleci",
                                                    "state": "SUCCESS",
                                                    "targetUrl": "https://circleci.com/build/1"
                                                },
                                                {
                                                    "__typename": "StatusContext",
                                                    "context": "ci/jenkins",
                                                    "state": "PENDING",
                                                    "targetUrl": "https://jenkins.io/build/2"
                                                }
                                            ]
                                        }
                                    }
                                }
                            }]
                        }
                    }
                }
            }
        });

        let result = parse_pr_check_contexts(&data);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["name"], "ci/circleci");
        assert_eq!(result[0]["conclusion"], "success");
        assert_eq!(result[0]["status"], "completed");
        assert_eq!(result[0]["html_url"], "https://circleci.com/build/1");
        assert_eq!(result[1]["name"], "ci/jenkins");
        assert_eq!(result[1]["conclusion"], "");
        assert_eq!(result[1]["status"], "in_progress");
    }

    #[test]
    fn test_parse_pr_check_contexts_empty() {
        let data = serde_json::json!({
            "data": {
                "repository": {
                    "pullRequest": {
                        "commits": { "nodes": [] }
                    }
                }
            }
        });
        assert_eq!(parse_pr_check_contexts(&data).len(), 0);
    }

    #[test]
    fn test_parse_pr_check_contexts_no_data() {
        let data = serde_json::json!({});
        assert_eq!(parse_pr_check_contexts(&data).len(), 0);
    }
}
