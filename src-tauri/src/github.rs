use serde::Serialize;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::State;

use crate::error_classification::calculate_backoff_delay;
use crate::state::{AppState, GIT_CACHE_TTL, GITHUB_CACHE_TTL};

/// Run `gh auth token` CLI to get the current token from gh's secure storage.
/// This works even when env vars are empty/unset, because gh reads from the
/// system keychain on macOS or credential store on other platforms.
fn token_from_gh_cli() -> Option<String> {
    let output = Command::new(crate::agent::resolve_cli("gh"))
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?;
    let token = token.trim().to_string();
    if token.is_empty() { None } else { Some(token) }
}

/// Resolve a GitHub API token from environment or gh CLI.
/// Order: GH_TOKEN env → GITHUB_TOKEN env → gh_token crate → `gh auth token` CLI.
/// The gh_token crate has a bug where it returns empty strings for env vars set
/// to "" (e.g., in Tauri GUI processes that don't inherit shell env vars).
/// We filter empty values and fall back to the CLI as a last resort.
/// Returns None if no token is found (graceful degradation).
pub(crate) fn resolve_github_token() -> Option<String> {
    if let Ok(token) = std::env::var("GH_TOKEN")
        && !token.is_empty()
    {
        return Some(token);
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        return Some(token);
    }
    // gh_token crate doesn't filter empty env var values (env::var_os returns
    // Some("") for vars set to empty string), so we must filter here.
    if let Some(token) = gh_token::get().ok().filter(|t| !t.is_empty()) {
        return Some(token);
    }
    // Direct CLI fallback: gh_token's internal CLI call may be skipped when it
    // short-circuits on an empty env var. Call `gh auth token` explicitly.
    token_from_gh_cli()
}

/// Collect all non-empty GitHub token candidates in priority order.
/// Used for fallback when the primary token gets a 401.
pub(crate) fn resolve_github_token_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(token) = std::env::var("GH_TOKEN")
        && !token.is_empty()
    {
        candidates.push(token);
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        candidates.push(token);
    }
    if let Ok(token) = gh_token::get()
        && !token.is_empty() && !candidates.contains(&token)
    {
        candidates.push(token);
    }
    // Explicit CLI fallback for when gh_token short-circuits on empty env vars
    if let Some(token) = token_from_gh_cli()
        && !candidates.contains(&token)
    {
        candidates.push(token);
    }
    candidates
}

/// Error type for GraphQL requests, distinguishing auth failures and rate limits from other errors.
#[derive(Debug)]
pub(crate) enum GqlError {
    /// 401 Unauthorized — token is invalid or expired
    Auth(String),
    /// Rate limited by GitHub (429, 403 with exhausted limits, or GraphQL RATE_LIMITED)
    RateLimit {
        /// Unix epoch from `x-ratelimit-reset` header (primary limit reset time)
        reset_at: Option<u64>,
        /// Seconds from `retry-after` header (secondary/abuse limits)
        retry_after: Option<u64>,
        message: String,
    },
    /// Any other error (network, parse, non-401 HTTP status, GraphQL errors)
    Other(String),
}

impl fmt::Display for GqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GqlError::Auth(msg) => write!(f, "Auth error: {msg}"),
            GqlError::RateLimit { message, .. } => write!(f, "Rate limited: {message}"),
            GqlError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

/// Circuit breaker for GitHub API calls.
/// Tracks consecutive failures and stops making requests after a threshold.
/// Rate limits are tracked separately so they don't inflate the failure count.
pub(crate) struct GitHubCircuitBreaker {
    failure_count: AtomicU32,
    open_until: parking_lot::RwLock<Option<Instant>>,
    /// Separate backoff for rate limits — does not affect failure_count
    rate_limit_until: parking_lot::RwLock<Option<Instant>>,
}

/// Consecutive failures before the circuit opens (tolerates occasional transient errors).
const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
/// Initial backoff when the circuit opens (5 seconds).
const CIRCUIT_BREAKER_BASE_MS: f64 = 5_000.0;
/// Maximum backoff cap so the circuit eventually retries (5 minutes).
const CIRCUIT_BREAKER_MAX_MS: f64 = 300_000.0;
/// Exponential backoff multiplier (doubles each failure beyond threshold).
const CIRCUIT_BREAKER_MULTIPLIER: f64 = 2.0;

impl GitHubCircuitBreaker {
    pub(crate) fn new() -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            open_until: parking_lot::RwLock::new(None),
            rate_limit_until: parking_lot::RwLock::new(None),
        }
    }

    /// Check if the circuit is open (failure-based or rate-limited).
    /// Returns Ok(()) if closed (requests allowed), or Err with a message.
    pub(crate) fn check(&self) -> Result<(), String> {
        // Check rate limit backoff first (more specific message)
        let rl_guard = self.rate_limit_until.read();
        if let Some(until) = *rl_guard
            && Instant::now() < until
        {
            let remaining = until.duration_since(Instant::now());
            return Err(format!(
                "rate-limit: backing off for {:.0}s",
                remaining.as_secs_f64()
            ));
        }
        drop(rl_guard);

        // Check failure-based circuit breaker
        let guard = self.open_until.read();
        if let Some(until) = *guard
            && Instant::now() < until
        {
            let remaining = until.duration_since(Instant::now());
            return Err(format!(
                "GitHub API circuit breaker open — retrying in {:.0}s",
                remaining.as_secs_f64()
            ));
        }
        Ok(())
    }

    /// Record a successful API call. Resets failure count and closes the circuit.
    pub(crate) fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        *self.open_until.write() = None;
    }

    /// Record a rate limit response. Sets a dedicated backoff timer
    /// without inflating the failure count.
    pub(crate) fn record_rate_limit(&self, wait_secs: u64) {
        let delay = std::time::Duration::from_secs(wait_secs);
        *self.rate_limit_until.write() = Some(Instant::now() + delay);
        eprintln!(
            "[github] Rate limited — backing off for {wait_secs}s",
        );
    }

    /// Record a failed API call. Opens the circuit after threshold failures.
    pub(crate) fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= CIRCUIT_BREAKER_THRESHOLD {
            let delay_ms = calculate_backoff_delay(
                count - CIRCUIT_BREAKER_THRESHOLD,
                CIRCUIT_BREAKER_BASE_MS,
                CIRCUIT_BREAKER_MAX_MS,
                CIRCUIT_BREAKER_MULTIPLIER,
            );
            let delay = std::time::Duration::from_millis(delay_ms as u64);
            *self.open_until.write() = Some(Instant::now() + delay);
            eprintln!(
                "[github] Circuit breaker open after {count} failures, backing off for {:.1}s",
                delay.as_secs_f64()
            );
        }
    }
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

/// Parse a header value as a u64, returning None if missing or unparseable.
fn header_as_u64(response: &reqwest::blocking::Response, name: &str) -> Option<u64> {
    response.headers().get(name)?.to_str().ok()?.parse().ok()
}

/// Execute a GraphQL query against the GitHub API.
/// Returns the parsed JSON response or a typed error.
/// Detects rate limits from HTTP status codes, headers, and GraphQL error types.
pub(crate) fn graphql_request(
    client: &reqwest::blocking::Client,
    token: &str,
    query: &str,
    variables: &serde_json::Value,
) -> Result<serde_json::Value, GqlError> {
    let body = serde_json::json!({
        "query": query,
        "variables": variables,
    });

    let response = client
        .post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "tuicommander")
        .json(&body)
        .send()
        .map_err(|e| GqlError::Other(format!("GraphQL request failed: {e}")))?;

    let status = response.status();

    // Extract rate limit headers before consuming the response body
    let ratelimit_remaining = header_as_u64(&response, "x-ratelimit-remaining");
    let ratelimit_reset = header_as_u64(&response, "x-ratelimit-reset");
    let retry_after = header_as_u64(&response, "retry-after");

    // 1. HTTP 429 → always a rate limit
    if status.as_u16() == 429 {
        return Err(GqlError::RateLimit {
            reset_at: ratelimit_reset,
            retry_after,
            message: "HTTP 429 Too Many Requests".to_string(),
        });
    }

    let json: serde_json::Value = response
        .json()
        .map_err(|e| GqlError::Other(format!("Failed to parse GraphQL response: {e}")))?;

    if !status.is_success() {
        let msg = json["message"].as_str().unwrap_or("Unknown error");
        let err_msg = format!("GitHub API error ({status}): {msg}");

        if status.as_u16() == 401 {
            return Err(GqlError::Auth(err_msg));
        }

        // 2. HTTP 403 + x-ratelimit-remaining: 0 → primary rate limit exhausted
        if status.as_u16() == 403 && ratelimit_remaining == Some(0) {
            return Err(GqlError::RateLimit {
                reset_at: ratelimit_reset,
                retry_after,
                message: format!("Primary rate limit exhausted: {msg}"),
            });
        }

        // 3. HTTP 403 + body mentions "secondary rate" → secondary/abuse rate limit
        if status.as_u16() == 403 {
            let msg_lower = msg.to_lowercase();
            if msg_lower.contains("secondary rate") || msg_lower.contains("abuse") {
                return Err(GqlError::RateLimit {
                    reset_at: ratelimit_reset,
                    retry_after,
                    message: format!("Secondary rate limit: {msg}"),
                });
            }
        }

        return Err(GqlError::Other(err_msg));
    }

    // 4. HTTP 200 + GraphQL errors with type "RATE_LIMITED"
    if let Some(errors) = json["errors"].as_array()
        && !errors.is_empty()
    {
        let has_rate_limit_error = errors.iter().any(|e| {
            e["type"].as_str() == Some("RATE_LIMITED")
        });

        if has_rate_limit_error {
            let msg = errors[0]["message"].as_str().unwrap_or("GraphQL rate limit");
            return Err(GqlError::RateLimit {
                reset_at: ratelimit_reset,
                retry_after,
                message: msg.to_string(),
            });
        }

        let msg = errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error");
        return Err(GqlError::Other(format!("GraphQL error: {msg}")));
    }

    Ok(json)
}

/// Calculate how long to wait for a rate limit, in seconds.
/// Prefers `retry-after` (secondary limits), falls back to `reset_at - now + 1`,
/// defaults to 60s if neither header is available.
fn rate_limit_wait_secs(reset_at: Option<u64>, retry_after: Option<u64>) -> u64 {
    if let Some(secs) = retry_after {
        return secs;
    }
    if let Some(reset) = reset_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if reset > now {
            return reset - now + 1;
        }
    }
    60 // Default: wait 60 seconds
}

/// Execute a GraphQL query with token fallback and circuit breaker protection.
/// On 401, tries remaining token candidates and updates the stored token on success.
/// Rate limits are handled separately from failures — they don't inflate the failure count.
pub(crate) fn graphql_with_retry(
    state: &AppState,
    query: &str,
    variables: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Check circuit breaker first
    state.github_circuit_breaker.check()?;

    let current_token = state.github_token.read().clone();
    let token = match current_token.as_deref() {
        Some(t) => t.to_string(),
        None => return Err("No GitHub token available".to_string()),
    };

    match graphql_request(&state.http_client, &token, query, &variables) {
        Ok(response) => {
            state.github_circuit_breaker.record_success();
            Ok(response)
        }
        Err(GqlError::RateLimit { reset_at, retry_after, message }) => {
            let wait = rate_limit_wait_secs(reset_at, retry_after);
            state.github_circuit_breaker.record_rate_limit(wait);
            Err(format!("rate-limit: {message}"))
        }
        Err(GqlError::Auth(msg)) => {
            eprintln!("[github] 401 with current token, trying fallback candidates");
            // Try other candidates
            let candidates = resolve_github_token_candidates();
            for candidate in &candidates {
                if candidate == &token {
                    continue; // Skip the one that already failed
                }
                match graphql_request(&state.http_client, candidate, query, &variables) {
                    Ok(response) => {
                        eprintln!("[github] Token fallback succeeded");
                        *state.github_token.write() = Some(candidate.clone());
                        state.github_circuit_breaker.record_success();
                        return Ok(response);
                    }
                    Err(GqlError::Auth(_)) => continue, // Try next candidate
                    Err(GqlError::RateLimit { reset_at, retry_after, message }) => {
                        let wait = rate_limit_wait_secs(reset_at, retry_after);
                        state.github_circuit_breaker.record_rate_limit(wait);
                        return Err(format!("rate-limit: {message}"));
                    }
                    Err(GqlError::Other(e)) => {
                        state.github_circuit_breaker.record_failure();
                        return Err(e);
                    }
                }
            }
            // All candidates failed with 401
            state.github_circuit_breaker.record_failure();
            Err(msg)
        }
        Err(GqlError::Other(msg)) => {
            state.github_circuit_breaker.record_failure();
            Err(msg)
        }
    }
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

/// Convert a 6-char hex color to an rgba() CSS string with the given alpha
pub(crate) fn hex_to_rgba(hex: &str, alpha: f64) -> String {
    let (r, g, b) = parse_hex_rgb(hex);
    format!("rgba({r}, {g}, {b}, {alpha})")
}

/// Determine if a hex color is light (needs dark text) using BT.601 luma
pub(crate) fn is_light_color(hex: &str) -> bool {
    let (r, g, b) = parse_hex_rgb(hex);
    let (r, g, b) = (r as u32, g as u32, b as u32);
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

    nodes.iter().filter_map(parse_pr_node).collect()
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
/// Reads directly from .git/config (no subprocess).
fn get_github_remote_url(repo_path: &Path) -> Option<String> {
    let url = crate::git::read_remote_url(repo_path)?;
    if url.contains("github.com") {
        Some(url)
    } else {
        None
    }
}

/// Core logic for fetching PR statuses via GitHub GraphQL API (no caching).
/// Returns Err for rate limits (prefixed with "rate-limit:") so callers can handle them.
pub(crate) fn get_repo_pr_statuses_impl(
    path: &str,
    state: &AppState,
) -> Result<Vec<BranchPrStatus>, String> {
    let repo_path = PathBuf::from(path);

    if state.github_token.read().is_none() {
        return Ok(vec![]); // No token = no GitHub API access
    }

    let remote_url = match get_github_remote_url(&repo_path) {
        Some(url) => url,
        None => return Ok(vec![]),
    };

    let (owner, repo) = match parse_remote_url(&remote_url) {
        Some(pair) => pair,
        None => return Ok(vec![]),
    };

    let variables = serde_json::json!({
        "owner": owner,
        "repo": repo,
        "first": 20,
    });

    match graphql_with_retry(state, BATCH_PR_QUERY, variables) {
        Ok(response) => Ok(parse_graphql_prs(&response)),
        Err(e) if e.starts_with("rate-limit:") => Err(e),
        Err(e) => {
            eprintln!("[github] GraphQL batch PR query failed: {e}");
            Ok(vec![])
        }
    }
}

/// Get all open PR statuses for a repository (cached, 30s TTL).
/// Runs on a blocking thread to avoid freezing the UI on focus.
#[tauri::command]
pub(crate) async fn get_repo_pr_statuses(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<Vec<BranchPrStatus>, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        if let Some(cached) = AppState::get_cached(&state.github_status_cache, &path, GITHUB_CACHE_TTL) {
            return Ok(cached);
        }

        let statuses = get_repo_pr_statuses_impl(&path, &state)?;
        AppState::set_cached(&state.github_status_cache, path.clone(), statuses.clone());
        Ok(statuses)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

/// Build a single aliased GraphQL query that fetches PRs for multiple repos in one HTTP call.
/// Each repo gets an alias `r{i}` to avoid field name collisions.
/// Returns (query_string, Vec<(alias, repo_path)>) for result extraction.
fn build_multi_repo_pr_query(
    repos: &[(String, String, String)], // Vec<(path, owner, name)>
    include_merged: bool,
) -> (String, Vec<(String, String)>) {
    let states = if include_merged { "[OPEN, MERGED]" } else { "[OPEN]" };
    let node_fields = r#"number title state url headRefName baseRefName isDraft
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
        }"#;

    let mut aliases: Vec<(String, String)> = Vec::new();
    let mut parts = vec!["query BatchRepoPRs {".to_string()];

    for (i, (path, owner, name)) in repos.iter().enumerate() {
        let alias = format!("r{i}");
        parts.push(format!(
            "  {alias}: repository(owner: \"{owner}\", name: \"{name}\") {{\n    pullRequests(first: 20, states: {states}, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{\n      nodes {{ {node_fields} }}\n    }}\n  }}"
        ));
        aliases.push((alias, path.clone()));
    }
    parts.push("  rateLimit { cost remaining resetAt }".to_string());
    parts.push("}".to_string());

    (parts.join("\n"), aliases)
}

/// Fetch PR statuses for all repos in a single batched GraphQL call.
/// On failure (network, auth, complexity), returns Err so the caller can fall back to per-repo calls.
fn get_all_pr_statuses_impl(
    paths: &[String],
    include_merged: bool,
    state: &AppState,
) -> Result<std::collections::HashMap<String, Vec<BranchPrStatus>>, String> {
    if state.github_token.read().is_none() {
        return Ok(std::collections::HashMap::new());
    }

    // Resolve (path, owner, repo) for each path that has a GitHub remote
    let repos: Vec<(String, String, String)> = paths
        .iter()
        .filter_map(|path| {
            let repo_path = PathBuf::from(path);
            let url = get_github_remote_url(&repo_path)?;
            let (owner, name) = parse_remote_url(&url)?;
            Some((path.clone(), owner, name))
        })
        .collect();

    if repos.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let (query, aliases) = build_multi_repo_pr_query(&repos, include_merged);

    let response = graphql_with_retry(state, &query, serde_json::Value::Null)?;

    let mut results = std::collections::HashMap::new();
    for (alias, path) in &aliases {
        let nodes = match response["data"][alias]["pullRequests"]["nodes"].as_array() {
            Some(arr) => arr,
            None => continue,
        };
        let statuses: Vec<BranchPrStatus> = nodes.iter().filter_map(parse_pr_node).collect();
        // Update the per-repo cache so get_repo_pr_statuses hits cache on next individual call
        AppState::set_cached(&state.github_status_cache, path.clone(), statuses.clone());
        results.insert(path.clone(), statuses);
    }
    Ok(results)
}

/// Fetch PR statuses for all repos in a single batched GraphQL call.
/// On failure, the frontend should retry with per-repo individual calls.
/// `include_merged` is true for the startup poll to detect offline transitions.
#[tauri::command]
pub(crate) async fn get_all_pr_statuses(
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
    include_merged: bool,
) -> Result<std::collections::HashMap<String, Vec<BranchPrStatus>>, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        get_all_pr_statuses_impl(&paths, include_merged, &state)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

/// Get git remote + branch status for a repository (implementation).
/// PR and CI data now comes from the batch githubStore (GraphQL),
/// so this only returns has_remote, current_branch, ahead, and behind.
pub(crate) fn get_github_status_impl(path: &str) -> GitHubStatus {
    let repo_path = PathBuf::from(path);

    let has_remote = get_github_remote_url(&repo_path).is_some();

    // Read current branch from .git/HEAD (no subprocess)
    let current_branch = crate::git::read_branch_from_head(&repo_path)
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
    let (ahead, behind) = Command::new(crate::agent::resolve_cli("git"))
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

/// Tauri command wrapper — cached with GIT_CACHE_TTL to avoid spawning git subprocesses every poll.
#[tauri::command]
pub(crate) async fn get_github_status(
    path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<GitHubStatus, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        if let Some(cached) = AppState::get_cached(&state.git_status_cache, &path, GIT_CACHE_TTL) {
            return cached;
        }
        let status = get_github_status_impl(&path);
        AppState::set_cached(&state.git_status_cache, path, status.clone());
        status
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))
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
    state: &AppState,
) -> Vec<serde_json::Value> {
    let repo_path = PathBuf::from(path);

    if state.github_token.read().is_none() {
        return vec![];
    }

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

    match graphql_with_retry(state, PR_CHECKS_QUERY, variables) {
        Ok(data) => parse_pr_check_contexts(&data),
        Err(e) => {
            eprintln!("[github] GraphQL PR checks query failed: {}", e);
            vec![]
        }
    }
}

/// Merge a PR via GitHub REST API using the specified merge method.
///
/// Calls PUT /repos/{owner}/{repo}/pulls/{pull_number}/merge.
/// Returns the SHA of the merge commit on success.
pub(crate) fn merge_pr_github_impl(
    repo_path: &str,
    pr_number: i64,
    merge_method: &str,
    state: &AppState,
) -> Result<String, String> {
    let token = state
        .github_token
        .read()
        .clone()
        .ok_or_else(|| "No GitHub token available".to_string())?;

    let remote_url = get_github_remote_url(std::path::Path::new(repo_path))
        .ok_or_else(|| "No GitHub remote URL found for this repository".to_string())?;

    let (owner, repo) = parse_remote_url(&remote_url)
        .ok_or_else(|| format!("Failed to parse GitHub remote URL: {remote_url}"))?;

    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}/merge");
    let body = serde_json::json!({ "merge_method": merge_method });

    let response = state
        .http_client
        .put(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "tuicommander")
        .header("Accept", "application/vnd.github+json")
        .json(&body)
        .send()
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

    let status = response.status();
    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("Failed to parse GitHub API response: {e}"))?;

    if status.is_success() {
        let sha = json["sha"].as_str().unwrap_or("").to_string();
        Ok(sha)
    } else {
        let msg = json["message"].as_str().unwrap_or("Unknown error");
        Err(format!("GitHub merge failed ({status}): {msg}"))
    }
}

/// Merge a PR via GitHub REST API (Tauri command).
/// Supports merge_method: "merge", "squash", "rebase".
#[tauri::command]
pub(crate) async fn merge_pr_via_github(
    repo_path: String,
    pr_number: i64,
    merge_method: String,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        merge_pr_github_impl(&repo_path, pr_number, &merge_method, &state)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

/// Get CI check details for a PR via GitHub GraphQL API (Story 060).
/// Runs on a blocking thread to avoid freezing the UI on focus.
#[tauri::command]
pub(crate) async fn get_ci_checks(
    path: String,
    pr_number: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<serde_json::Value>, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        get_ci_checks_impl(&path, pr_number, &state)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // --- Integration tests: GraphQL API vs gh CLI (requires network + token) ---
    // Run with: cargo test --lib -- --ignored --test-threads=1

    /// Test that our GraphQL batch PR query returns the same data as `gh pr list`.
    /// Compares owner/repo extraction, token resolution, API call, and parsed results
    /// against the gh CLI output on this repository.
    #[test]
    #[ignore] // Requires network + GitHub token
    fn test_graphql_pr_query_matches_gh_cli() {
        // 1. Resolve token (same path our production code uses)
        let token = resolve_github_token()
            .expect("No GitHub token found — set GH_TOKEN or run `gh auth login`");

        // 2. Get repo info from local .git (same as production code)
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
        let remote_url = crate::git::read_remote_url(&repo_root)
            .expect("No origin remote found");
        let (owner, repo) = parse_remote_url(&remote_url)
            .expect("Failed to parse remote URL into owner/repo");

        println!("Testing against {owner}/{repo}");

        // 3. Call GraphQL API
        let client = reqwest::blocking::Client::new();
        let variables = serde_json::json!({
            "owner": owner,
            "repo": repo,
            "first": 50,
        });
        let graphql_result = graphql_request(&client, &token, BATCH_PR_QUERY, &variables);
        assert!(graphql_result.is_ok(), "GraphQL request failed: {:?}", graphql_result.err());

        let data = graphql_result.unwrap();

        // 4. Verify response structure
        assert!(data["data"]["repository"].is_object(),
            "Response should have data.repository: {}", serde_json::to_string_pretty(&data).unwrap());
        assert!(data["data"]["repository"]["pullRequests"]["nodes"].is_array(),
            "Response should have pullRequests.nodes array");
        assert!(data["data"]["rateLimit"]["remaining"].is_number(),
            "Response should include rateLimit info");

        let remaining = data["data"]["rateLimit"]["remaining"].as_i64().unwrap();
        println!("GraphQL rate limit remaining: {remaining}");

        // 5. Parse into BranchPrStatus (same as production code)
        let graphql_prs = parse_graphql_prs(&data);
        println!("GraphQL returned {} PRs", graphql_prs.len());

        // 6. Compare with gh CLI (if available)
        let gh_output = Command::new(crate::agent::resolve_cli("gh"))
            .current_dir(&repo_root)
            .args([
                "pr", "list", "--state", "all", "--limit", "50",
                "--json", "number,title,state,headRefName,additions,deletions,isDraft",
            ])
            .output()
            .ok();

        if let Some(output) = gh_output {
            if output.status.success() {
                let gh_json: Vec<serde_json::Value> =
                    serde_json::from_slice(&output.stdout).unwrap_or_default();
                println!("gh CLI returned {} PRs", gh_json.len());

                // PR counts should match
                assert_eq!(graphql_prs.len(), gh_json.len(),
                    "GraphQL and gh CLI should return the same number of PRs");

                // For each PR, verify key fields match
                for gh_pr in &gh_json {
                    let number = gh_pr["number"].as_i64().unwrap() as i32;
                    let branch = gh_pr["headRefName"].as_str().unwrap();

                    let gql_pr = graphql_prs.iter().find(|p| p.number == number);
                    assert!(gql_pr.is_some(),
                        "PR #{number} ({branch}) found in gh CLI but not in GraphQL");

                    let gql_pr = gql_pr.unwrap();
                    assert_eq!(gql_pr.branch, branch,
                        "PR #{number}: branch mismatch");
                    assert_eq!(gql_pr.title, gh_pr["title"].as_str().unwrap(),
                        "PR #{number}: title mismatch");
                    assert_eq!(gql_pr.state, gh_pr["state"].as_str().unwrap(),
                        "PR #{number}: state mismatch");
                    assert_eq!(gql_pr.is_draft, gh_pr["isDraft"].as_bool().unwrap_or(false),
                        "PR #{number}: isDraft mismatch");
                    assert_eq!(gql_pr.additions, gh_pr["additions"].as_i64().unwrap_or(0) as i32,
                        "PR #{number}: additions mismatch");
                    assert_eq!(gql_pr.deletions, gh_pr["deletions"].as_i64().unwrap_or(0) as i32,
                        "PR #{number}: deletions mismatch");
                }

                println!("All {} PRs match between GraphQL and gh CLI", gh_json.len());
            } else {
                println!("gh CLI not available — skipping comparison, GraphQL-only validation passed");
            }
        } else {
            println!("gh CLI not installed — skipping comparison, GraphQL-only validation passed");
        }
    }

    /// Test that GraphQL token resolution works and can authenticate.
    #[test]
    #[ignore] // Requires network + GitHub token
    fn test_graphql_auth_and_rate_limit() {
        let token = resolve_github_token()
            .expect("No GitHub token found");

        let client = reqwest::blocking::Client::new();
        // Minimal query just to verify auth works
        let result = graphql_request(
            &client, &token,
            "query { viewer { login } rateLimit { remaining resetAt } }",
            &serde_json::json!({}),
        );

        assert!(result.is_ok(), "Auth failed: {:?}", result.err());
        let data = result.unwrap();

        let login = data["data"]["viewer"]["login"].as_str();
        assert!(login.is_some(), "Should return authenticated user login");
        println!("Authenticated as: {}", login.unwrap());

        let remaining = data["data"]["rateLimit"]["remaining"].as_i64().unwrap();
        println!("Rate limit remaining: {remaining}/5000");
        assert!(remaining > 0, "Should have rate limit remaining");
    }

    // --- resolve_github_token_candidates tests ---

    #[test]
    fn test_resolve_github_token_candidates() {
        // Set both env vars to known values
        unsafe {
            std::env::set_var("GH_TOKEN", "gh-token-1");
            std::env::set_var("GITHUB_TOKEN", "github-token-2");
        }

        let candidates = resolve_github_token_candidates();
        assert!(candidates.len() >= 2, "Should have at least 2 candidates");
        assert_eq!(candidates[0], "gh-token-1");
        assert_eq!(candidates[1], "github-token-2");

        // Cleanup
        unsafe {
            std::env::remove_var("GH_TOKEN");
            std::env::remove_var("GITHUB_TOKEN");
        }
    }

    // --- Empty token filtering tests ---
    // Env vars are process-global state, so all env-var scenarios run in a single
    // test to avoid parallel race conditions.

    #[test]
    fn test_resolve_github_token_filters_empty_from_gh_token_crate() {
        // Simulate Tauri GUI process: GITHUB_TOKEN="" (set but empty).
        // gh_token crate's get() uses env::var_os() which returns Some("") for
        // empty env vars without checking emptiness. Our resolve_github_token()
        // must filter this and not return Some("").
        unsafe {
            std::env::set_var("GITHUB_TOKEN", "");
            std::env::remove_var("GH_TOKEN");
        }

        let result = resolve_github_token();
        // Result should be either None (no gh CLI) or a non-empty token from CLI
        if let Some(ref token) = result {
            assert!(!token.is_empty(),
                "resolve_github_token must never return an empty string");
        }

        // Cleanup
        unsafe {
            std::env::remove_var("GITHUB_TOKEN");
        }
    }

    #[test]
    fn test_resolve_github_token_candidates_filters_empty() {
        unsafe {
            std::env::set_var("GH_TOKEN", "");
            std::env::set_var("GITHUB_TOKEN", "");
        }

        let candidates = resolve_github_token_candidates();
        for candidate in &candidates {
            assert!(!candidate.is_empty(),
                "Candidates must never contain empty strings");
        }

        // Cleanup
        unsafe {
            std::env::remove_var("GH_TOKEN");
            std::env::remove_var("GITHUB_TOKEN");
        }
    }

    // --- Integration test: token resolution with gh CLI fallback ---
    // Run with: cargo test --lib -- --ignored --test-threads=1

    /// Verify that resolve_github_token works even when env vars are empty,
    /// by falling through to `gh auth token` CLI.
    /// This catches the exact bug where GITHUB_TOKEN="" in Tauri GUI processes
    /// caused gh_token crate to return an empty string → 401 Bad credentials.
    #[test]
    #[ignore] // Requires gh CLI authenticated
    fn test_resolve_token_with_empty_env_falls_through_to_cli() {
        // Save and clear env vars to simulate GUI context
        let saved_gh = std::env::var("GH_TOKEN").ok();
        let saved_github = std::env::var("GITHUB_TOKEN").ok();
        unsafe {
            std::env::set_var("GITHUB_TOKEN", "");
            std::env::set_var("GH_TOKEN", "");
        }

        let token = resolve_github_token();
        assert!(token.is_some(),
            "Should resolve token via gh CLI when env vars are empty");
        let token = token.unwrap();
        assert!(!token.is_empty(), "Token from CLI should not be empty");

        // Verify the token actually works against GitHub API
        let client = reqwest::blocking::Client::new();
        let result = graphql_request(
            &client, &token,
            "query { viewer { login } }",
            &serde_json::json!({}),
        );
        assert!(result.is_ok(),
            "Token from gh CLI should authenticate successfully: {:?}", result.err());

        let data = result.unwrap();
        let login = data["data"]["viewer"]["login"].as_str();
        assert!(login.is_some(), "Should return authenticated user login");
        println!("Authenticated via CLI fallback as: {}", login.unwrap());

        // Restore env vars
        unsafe {
            match saved_gh {
                Some(v) => std::env::set_var("GH_TOKEN", v),
                None => std::env::remove_var("GH_TOKEN"),
            }
            match saved_github {
                Some(v) => std::env::set_var("GITHUB_TOKEN", v),
                None => std::env::remove_var("GITHUB_TOKEN"),
            }
        }
    }

    // --- GqlError display tests ---

    #[test]
    fn test_gql_error_display_auth() {
        let err = GqlError::Auth("401 Unauthorized".to_string());
        assert_eq!(format!("{err}"), "Auth error: 401 Unauthorized");
    }

    #[test]
    fn test_gql_error_display_other() {
        let err = GqlError::Other("network timeout".to_string());
        assert_eq!(format!("{err}"), "network timeout");
    }

    // --- GitHubCircuitBreaker tests ---

    #[test]
    fn test_circuit_breaker_stays_closed_on_success() {
        let cb = GitHubCircuitBreaker::new();
        cb.record_success();
        cb.record_success();
        assert!(cb.check().is_ok());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let cb = GitHubCircuitBreaker::new();
        cb.record_failure();
        cb.record_failure();
        assert!(cb.check().is_ok(), "Should still be closed after 2 failures");
        cb.record_failure();
        assert!(cb.check().is_err(), "Should be open after 3 failures");
    }

    #[test]
    fn test_circuit_breaker_resets_on_success() {
        let cb = GitHubCircuitBreaker::new();
        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // Reset before threshold
        cb.record_failure();
        cb.record_failure();
        assert!(cb.check().is_ok(), "Should be closed — success reset the count");
    }

    #[test]
    fn test_circuit_breaker_respects_open_until() {
        let cb = GitHubCircuitBreaker::new();
        // Force circuit open with a future instant
        *cb.open_until.write() = Some(Instant::now() + std::time::Duration::from_secs(60));
        let result = cb.check();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("circuit breaker open"));
    }

    // --- Rate limit circuit breaker tests ---

    #[test]
    fn test_circuit_breaker_rate_limit_blocks_requests() {
        let cb = GitHubCircuitBreaker::new();
        cb.record_rate_limit(60);
        let result = cb.check();
        assert!(result.is_err());
        assert!(result.unwrap_err().starts_with("rate-limit:"),
            "Rate limit check should return error starting with 'rate-limit:'");
    }

    #[test]
    fn test_circuit_breaker_rate_limit_does_not_increment_failure_count() {
        let cb = GitHubCircuitBreaker::new();
        // Record 2 failures (below threshold)
        cb.record_failure();
        cb.record_failure();
        // Record a rate limit — this should NOT push us over the failure threshold
        cb.record_rate_limit(1);
        // Wait for rate limit to expire
        std::thread::sleep(std::time::Duration::from_millis(1100));
        // Should still be open for requests (only 2 failures, threshold is 3)
        assert!(cb.check().is_ok(),
            "Rate limit should not inflate failure count");
    }

    #[test]
    fn test_circuit_breaker_rate_limit_expires() {
        let cb = GitHubCircuitBreaker::new();
        // Set rate limit that expires in 1 second
        *cb.rate_limit_until.write() = Some(Instant::now() + std::time::Duration::from_millis(50));
        assert!(cb.check().is_err(), "Should be rate limited initially");
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(cb.check().is_ok(), "Should be open after rate limit expires");
    }

    #[test]
    fn test_circuit_breaker_rate_limit_takes_priority_over_open() {
        let cb = GitHubCircuitBreaker::new();
        // Set both circuit breaker open and rate limited
        *cb.open_until.write() = Some(Instant::now() + std::time::Duration::from_secs(60));
        *cb.rate_limit_until.write() = Some(Instant::now() + std::time::Duration::from_secs(60));
        let result = cb.check();
        assert!(result.is_err());
        // Rate limit message should take priority
        assert!(result.unwrap_err().starts_with("rate-limit:"),
            "Rate limit should take priority in error message");
    }

    // --- GqlError display tests for RateLimit ---

    #[test]
    fn test_gql_error_display_rate_limit() {
        let err = GqlError::RateLimit {
            reset_at: Some(1700000000),
            retry_after: Some(60),
            message: "API rate limit exceeded".to_string(),
        };
        assert_eq!(format!("{err}"), "Rate limited: API rate limit exceeded");
    }

    // --- rate_limit_wait_secs tests ---

    #[test]
    fn test_rate_limit_wait_secs_prefers_retry_after() {
        // retry-after takes priority over reset_at
        assert_eq!(rate_limit_wait_secs(Some(9999999999), Some(42)), 42);
    }

    #[test]
    fn test_rate_limit_wait_secs_falls_back_to_reset_at() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let reset = now + 30;
        let wait = rate_limit_wait_secs(Some(reset), None);
        // Should be approximately 31 (30 + 1 safety margin)
        assert!(wait >= 30 && wait <= 32, "Expected ~31, got {wait}");
    }

    #[test]
    fn test_rate_limit_wait_secs_defaults_to_60() {
        assert_eq!(rate_limit_wait_secs(None, None), 60);
    }

    #[test]
    fn test_rate_limit_wait_secs_reset_in_past() {
        // If reset_at is in the past and no retry-after, default to 60
        assert_eq!(rate_limit_wait_secs(Some(1000), None), 60);
    }
}
