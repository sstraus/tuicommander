use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::github::BranchPrStatus;
use crate::state::{AppEvent, AppState};

// ---------------------------------------------------------------------------
// Transition detection
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum PrTransition {
    Merged { repo_path: String, branch: String, pr_number: i32, title: String },
    Closed { repo_path: String, branch: String, pr_number: i32, title: String },
    Blocked { repo_path: String, branch: String, pr_number: i32, title: String },
    CiFailed { repo_path: String, branch: String, pr_number: i32, title: String },
    CiRecovered { repo_path: String, branch: String, pr_number: i32, title: String },
    ChangesRequested { repo_path: String, branch: String, pr_number: i32, title: String },
    Ready { repo_path: String, branch: String, pr_number: i32, title: String },
}

fn is_ready(pr: &BranchPrStatus) -> bool {
    pr.mergeable == "MERGEABLE"
        && pr.review_decision == "APPROVED"
        && pr.checks.failed == 0
}

pub(crate) fn detect_transitions(
    repo_path: &str,
    old: &BranchPrStatus,
    new: &BranchPrStatus,
) -> Vec<PrTransition> {
    let mut out = Vec::new();
    let old_state = old.state.to_uppercase();
    let new_state = new.state.to_uppercase();

    let rp = repo_path.to_string();
    let branch = new.branch.clone();
    let pr_number = new.number;
    let title = new.title.clone();

    let mut primary_type: Option<&str> = None;

    // Terminal transitions
    if old_state != "MERGED" && new_state == "MERGED" {
        primary_type = Some("merged");
        out.push(PrTransition::Merged { repo_path: rp.clone(), branch: branch.clone(), pr_number, title: title.clone() });
    } else if old_state != "CLOSED" && new_state == "CLOSED" {
        primary_type = Some("closed");
        out.push(PrTransition::Closed { repo_path: rp.clone(), branch: branch.clone(), pr_number, title: title.clone() });
    }
    // Actionable transitions (only for OPEN PRs)
    else if new_state == "OPEN" {
        if old.mergeable != "CONFLICTING" && new.mergeable == "CONFLICTING" {
            primary_type = Some("blocked");
            out.push(PrTransition::Blocked { repo_path: rp.clone(), branch: branch.clone(), pr_number, title: title.clone() });
        } else if old.checks.failed == 0 && new.checks.failed > 0 {
            primary_type = Some("ci_failed");
            out.push(PrTransition::CiFailed { repo_path: rp.clone(), branch: branch.clone(), pr_number, title: title.clone() });
        } else if old.review_decision != "CHANGES_REQUESTED" && new.review_decision == "CHANGES_REQUESTED" {
            primary_type = Some("changes_requested");
            out.push(PrTransition::ChangesRequested { repo_path: rp.clone(), branch: branch.clone(), pr_number, title: title.clone() });
        } else if !is_ready(old) && is_ready(new) {
            primary_type = Some("ready");
            out.push(PrTransition::Ready { repo_path: rp.clone(), branch: branch.clone(), pr_number, title: title.clone() });
        }
    }

    // CI recovery: failed → all passing, suppressed when "ready" already fired
    if primary_type != Some("ready") && new_state == "OPEN" {
        let old_failed = old.checks.failed;
        let new_failed = new.checks.failed;
        let new_pending = new.checks.pending;
        if old_failed > 0 && new_failed == 0 && new_pending == 0 {
            out.push(PrTransition::CiRecovered { repo_path: rp, branch, pr_number, title });
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Poller
// ---------------------------------------------------------------------------

const BASE_INTERVAL: Duration = Duration::from_secs(30);
const HIDDEN_INTERVAL: Duration = Duration::from_secs(120);
const MAX_INTERVAL: Duration = Duration::from_secs(300);
const ISSUES_INTERVAL: Duration = Duration::from_secs(120);
const DEBOUNCE_WINDOW: Duration = Duration::from_secs(2);

pub(crate) enum PollerCmd {
    SetVisibility(bool),
    PollRepo(String),
    UpdatePaths(Vec<String>),
    SetIssueFilter(String),
    Stop,
}

pub(crate) struct GitHubPoller {
    pub(crate) cmd_tx: mpsc::Sender<PollerCmd>,
}

impl GitHubPoller {
    pub(crate) fn start(state: Arc<AppState>, handle: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(poll_loop(state, handle, rx));
        Self { cmd_tx: tx }
    }
}

/// Per-repo previous PR state for transition comparison.
type PrevState = HashMap<String, HashMap<String, BranchPrStatus>>;

async fn poll_loop(
    state: Arc<AppState>,
    handle: AppHandle,
    mut rx: mpsc::Receiver<PollerCmd>,
) {
    let mut visible = true;
    let mut paths: Vec<String> = Vec::new();
    let mut issue_filter = String::new();
    let mut prev: PrevState = HashMap::new();
    let mut fail_count: u32 = 0;
    let mut startup = true;
    let mut debounce_timers: HashMap<String, Instant> = HashMap::new();
    let mut last_issues_poll = Instant::now() - ISSUES_INTERVAL;

    let mut interval = tokio::time::interval(current_interval(visible, fail_count));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                poll_all_pr(
                    &state, &handle, &paths, startup,
                    &mut prev, &mut fail_count,
                ).await;
                startup = false;
                interval = tokio::time::interval(current_interval(visible, fail_count));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                // Issues polling piggybacks on the PR interval tick
                if last_issues_poll.elapsed() >= ISSUES_INTERVAL {
                    poll_issues(&state, &handle, &paths, &issue_filter).await;
                    last_issues_poll = Instant::now();
                }
            }
            cmd = rx.recv() => {
                match cmd {
                    Some(PollerCmd::SetVisibility(v)) => {
                        let was_hidden = !visible;
                        visible = v;
                        if was_hidden && visible {
                            // Became visible: poll immediately + reset interval
                            poll_all_pr(
                                &state, &handle, &paths, false,
                                &mut prev, &mut fail_count,
                            ).await;
                            interval = tokio::time::interval(current_interval(visible, fail_count));
                            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        }
                    }
                    Some(PollerCmd::PollRepo(path)) => {
                        let now = Instant::now();
                        if debounce_timers.get(&path).is_some_and(|last| now.duration_since(*last) < DEBOUNCE_WINDOW) {
                            continue;
                        }
                        debounce_timers.insert(path.clone(), now);
                        poll_single_repo(&state, &handle, &path, &mut prev).await;
                    }
                    Some(PollerCmd::UpdatePaths(new_paths)) => {
                        paths = new_paths;
                    }
                    Some(PollerCmd::SetIssueFilter(filter)) => {
                        issue_filter = filter;
                        poll_issues(&state, &handle, &paths, &issue_filter).await;
                        last_issues_poll = Instant::now();
                    }
                    Some(PollerCmd::Stop) | None => break,
                }
            }
        }
    }
}

fn current_interval(visible: bool, fail_count: u32) -> Duration {
    if !visible {
        return HIDDEN_INTERVAL;
    }
    if fail_count == 0 {
        return BASE_INTERVAL;
    }
    let backoff = BASE_INTERVAL.as_millis() as f64 * 2_f64.powi(fail_count as i32 - 1);
    Duration::from_millis(backoff.min(MAX_INTERVAL.as_millis() as f64) as u64)
}

async fn poll_all_pr(
    state: &AppState,
    handle: &AppHandle,
    paths: &[String],
    include_merged: bool,
    prev: &mut PrevState,
    fail_count: &mut u32,
) {
    if paths.is_empty() { return; }

    // Check circuit breaker
    if state.github_circuit_breaker.check().is_err() { return; }

    let path_strs: Vec<String> = paths.to_vec();
    match crate::github::get_all_pr_statuses_impl(&path_strs, include_merged, state).await {
        Ok(results) => {
            *fail_count = 0;
            for (repo_path, statuses) in &results {
                process_repo_update(state, handle, repo_path, statuses, prev);
            }
            for (repo_path, statuses) in results {
                let _ = handle.emit("github-pr-update", PrUpdatePayload {
                    repo_path: repo_path.clone(),
                    statuses: statuses.clone(),
                });
                let _ = state.event_bus.send(AppEvent::GitHubPrUpdate {
                    repo_path,
                    statuses,
                });
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("rate-limit:") {
                // Don't increment fail_count for rate limits
            } else {
                *fail_count = fail_count.saturating_add(1);
            }
            tracing::warn!(source = "github_poller", "poll_all failed: {msg}");
        }
    }
}

async fn poll_single_repo(
    state: &AppState,
    handle: &AppHandle,
    path: &str,
    prev: &mut PrevState,
) {
    if state.github_circuit_breaker.check().is_err() { return; }

    match crate::github::get_repo_pr_statuses_impl(path, false, state).await {
        Ok(statuses) => {
            process_repo_update(state, handle, path, &statuses, prev);
            let _ = handle.emit("github-pr-update", PrUpdatePayload {
                repo_path: path.to_string(),
                statuses: statuses.clone(),
            });
            let _ = state.event_bus.send(AppEvent::GitHubPrUpdate {
                repo_path: path.to_string(),
                statuses,
            });
        }
        Err(e) => {
            tracing::warn!(source = "github_poller", path, "poll_repo failed: {}", e);
        }
    }
}

fn process_repo_update(
    state: &AppState,
    handle: &AppHandle,
    repo_path: &str,
    statuses: &[BranchPrStatus],
    prev: &mut PrevState,
) {
    let old_map = prev.entry(repo_path.to_string()).or_default();
    for new_pr in statuses {
        if let Some(old_pr) = old_map.get(&new_pr.branch) {
            let transitions = detect_transitions(repo_path, old_pr, new_pr);
            for t in transitions {
                let _ = handle.emit("github-transition", &t);
                let _ = state.event_bus.send(AppEvent::GitHubTransition {
                    transition: t,
                });
            }
        }
        old_map.insert(new_pr.branch.clone(), new_pr.clone());
    }
}

async fn poll_issues(
    state: &AppState,
    handle: &AppHandle,
    paths: &[String],
    filter: &str,
) {
    if filter.is_empty() || filter == "disabled" || paths.is_empty() { return; }
    if state.github_circuit_breaker.check().is_err() { return; }

    match crate::github::get_all_issues_impl(paths, filter, state).await {
        Ok(results) => {
            for (repo_path, issues) in results {
                let _ = handle.emit("github-issues-update", IssuesUpdatePayload {
                    repo_path: repo_path.clone(),
                    issues: issues.clone(),
                });
                let _ = state.event_bus.send(AppEvent::GitHubIssuesUpdate {
                    repo_path,
                    issues,
                });
            }
        }
        Err(e) => {
            tracing::warn!(source = "github_poller", "poll_issues failed: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) async fn github_start_polling(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
    paths: Vec<String>,
    issue_filter: String,
) -> Result<(), String> {
    let mut guard = state.github_poller.lock();
    if guard.is_some() {
        // Already running — just update paths and filter
        if let Some(poller) = guard.as_ref() {
            let _ = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(paths));
            let _ = poller.cmd_tx.try_send(PollerCmd::SetIssueFilter(issue_filter));
        }
        return Ok(());
    }
    let poller = GitHubPoller::start(Arc::clone(&state), app);
    let _ = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(paths));
    let _ = poller.cmd_tx.try_send(PollerCmd::SetIssueFilter(issue_filter));
    *guard = Some(poller);
    Ok(())
}

#[tauri::command]
pub(crate) async fn github_stop_polling(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let poller = state.github_poller.lock().take();
    if let Some(p) = poller {
        let _ = p.cmd_tx.send(PollerCmd::Stop).await;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn github_set_visibility(
    state: tauri::State<'_, Arc<AppState>>,
    visible: bool,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::SetVisibility(visible));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn github_poll_repo(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::PollRepo(path));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn github_update_paths(
    state: tauri::State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(paths));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn github_set_issue_filter(
    state: tauri::State<'_, Arc<AppState>>,
    filter: String,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::SetIssueFilter(filter));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Event payloads — bare types per Tauri emit convention
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
struct PrUpdatePayload {
    repo_path: String,
    statuses: Vec<BranchPrStatus>,
}

#[derive(Clone, Serialize)]
struct IssuesUpdatePayload {
    repo_path: String,
    issues: Vec<crate::github::GitHubIssue>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::CheckSummary;

    fn make_pr(state: &str, mergeable: &str, review: &str, failed: u32, pending: u32) -> BranchPrStatus {
        BranchPrStatus {
            branch: "feat/test".to_string(),
            number: 42,
            title: "Test PR".to_string(),
            state: state.to_string(),
            url: String::new(),
            additions: 0,
            deletions: 0,
            checks: CheckSummary { passed: 0, failed, pending, total: failed + pending },
            check_details: vec![],
            author: String::new(),
            commits: 1,
            mergeable: mergeable.to_string(),
            merge_state_status: String::new(),
            review_decision: review.to_string(),
            labels: vec![],
            is_draft: false,
            base_ref_name: "main".to_string(),
            head_ref_oid: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            merge_state_label: None,
            review_state_label: None,
            merge_commit_allowed: true,
            squash_merge_allowed: true,
            rebase_merge_allowed: true,
        }
    }

    #[test]
    fn transition_merged() {
        let old = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let new = make_pr("MERGED", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Merged { .. }));
    }

    #[test]
    fn transition_closed() {
        let old = make_pr("OPEN", "UNKNOWN", "", 0, 0);
        let new = make_pr("CLOSED", "UNKNOWN", "", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Closed { .. }));
    }

    #[test]
    fn transition_blocked() {
        let old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        let new = make_pr("OPEN", "CONFLICTING", "", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Blocked { .. }));
    }

    #[test]
    fn transition_ci_failed() {
        let old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        let new = make_pr("OPEN", "MERGEABLE", "", 2, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::CiFailed { .. }));
    }

    #[test]
    fn transition_changes_requested() {
        let old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        let new = make_pr("OPEN", "MERGEABLE", "CHANGES_REQUESTED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::ChangesRequested { .. }));
    }

    #[test]
    fn transition_ready() {
        let old = make_pr("OPEN", "UNKNOWN", "", 1, 0);
        let new = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Ready { .. }));
    }

    #[test]
    fn transition_ci_recovered() {
        let old = make_pr("OPEN", "UNKNOWN", "", 3, 0);
        let new = make_pr("OPEN", "UNKNOWN", "", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::CiRecovered { .. }));
    }

    #[test]
    fn ci_recovered_suppressed_when_ready() {
        // Old: failing + not ready. New: ready (all green). Only "ready" fires, not ci_recovered.
        let old = make_pr("OPEN", "UNKNOWN", "", 2, 0);
        let new = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], PrTransition::Ready { .. }));
    }

    #[test]
    fn no_transition_on_unchanged() {
        let pr = make_pr("OPEN", "MERGEABLE", "APPROVED", 0, 0);
        let t = detect_transitions("/repo", &pr, &pr);
        assert!(t.is_empty());
    }

    #[test]
    fn no_transition_pending_ci() {
        // CI recovered from failed, but still has pending — no ci_recovered yet
        let old = make_pr("OPEN", "UNKNOWN", "", 2, 0);
        let new = make_pr("OPEN", "UNKNOWN", "", 0, 3);
        let t = detect_transitions("/repo", &old, &new);
        assert!(t.is_empty());
    }
}
