use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
#[cfg(feature = "desktop")]
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
    Merged {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Closed {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Blocked {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    CiFailed {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    CiRecovered {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    ChangesRequested {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Ready {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
    },
    Pushed {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
        head_ref_oid: String,
        /// PR author login — used by the watcher's authored_by_others filter.
        author: String,
    },
    /// A brand-new PR appeared on an open branch. Detected in `process_repo_update`
    /// (no prior state to diff), not in `detect_transitions`. Carries `author` for
    /// the watcher's authored_by_others filter and `head_ref_oid` for worktree review.
    Opened {
        repo_path: String,
        branch: String,
        pr_number: i32,
        title: String,
        head_ref_oid: String,
        author: String,
    },
}

fn is_ready(pr: &BranchPrStatus) -> bool {
    pr.mergeable == "MERGEABLE" && pr.review_decision == "APPROVED" && pr.checks.failed == 0
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
        out.push(PrTransition::Merged {
            repo_path: rp.clone(),
            branch: branch.clone(),
            pr_number,
            title: title.clone(),
        });
    } else if old_state != "CLOSED" && new_state == "CLOSED" {
        primary_type = Some("closed");
        out.push(PrTransition::Closed {
            repo_path: rp.clone(),
            branch: branch.clone(),
            pr_number,
            title: title.clone(),
        });
    }
    // Actionable transitions (only for OPEN PRs)
    else if new_state == "OPEN" {
        if old.mergeable != "CONFLICTING" && new.mergeable == "CONFLICTING" {
            primary_type = Some("blocked");
            out.push(PrTransition::Blocked {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        } else if old.checks.failed == 0 && new.checks.failed > 0 {
            primary_type = Some("ci_failed");
            out.push(PrTransition::CiFailed {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        } else if old.review_decision != "CHANGES_REQUESTED"
            && new.review_decision == "CHANGES_REQUESTED"
        {
            primary_type = Some("changes_requested");
            out.push(PrTransition::ChangesRequested {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        } else if !is_ready(old) && is_ready(new) {
            primary_type = Some("ready");
            out.push(PrTransition::Ready {
                repo_path: rp.clone(),
                branch: branch.clone(),
                pr_number,
                title: title.clone(),
            });
        }
    }

    // New commit pushed to an open PR: head_ref_oid changed. Independent signal
    // (a push can coincide with ci_failed etc.), carries the new oid for dedup.
    if new_state == "OPEN" && old.head_ref_oid != new.head_ref_oid && !new.head_ref_oid.is_empty() {
        out.push(PrTransition::Pushed {
            repo_path: rp.clone(),
            branch: branch.clone(),
            pr_number,
            title: title.clone(),
            head_ref_oid: new.head_ref_oid.clone(),
            author: new.author.clone(),
        });
    }

    // CI recovery: failed → all passing, suppressed when "ready" already fired
    if primary_type != Some("ready") && new_state == "OPEN" {
        let old_failed = old.checks.failed;
        let new_failed = new.checks.failed;
        let new_pending = new.checks.pending;
        if old_failed > 0 && new_failed == 0 && new_pending == 0 {
            out.push(PrTransition::CiRecovered {
                repo_path: rp,
                branch,
                pr_number,
                title,
            });
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Poller
// ---------------------------------------------------------------------------

const BASE_INTERVAL: Duration = Duration::from_secs(60);
const HIDDEN_INTERVAL: Duration = Duration::from_secs(120);
const MAX_INTERVAL: Duration = Duration::from_secs(300);
/// Debounce window for coalescing event-driven poll requests into the periodic batch.
const DEBOUNCE_WINDOW: Duration = Duration::from_secs(2);
/// Proactive throttle: slow down when fewer than this many GraphQL points remain.
const RATE_BUDGET_LOW: u32 = 500;
/// Proactive throttle: pause at MAX_INTERVAL when critically low on budget.
const RATE_BUDGET_CRITICAL: u32 = 100;
/// Repos with PR changes within this window are polled every tick.
/// Repos idle longer are polled every IDLE_POLL_DIVISOR ticks.
const ACTIVE_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Idle repos are included every Nth poll cycle.
const IDLE_POLL_DIVISOR: u32 = 5;
/// Cold repos (no active terminals) are included every Nth poll cycle (~10min at 60s base).
const DORMANT_POLL_DIVISOR: u32 = 10;

pub(crate) enum PollerCmd {
    SetVisibility(bool),
    PollRepo(String),
    UpdatePaths(Vec<String>),
    SetIssueFilter(String),
    SetPrHideDrafts(bool),
    /// Re-emit current PR + issue state for all repos on the next poll, even when
    /// unchanged. Sent when the frontend (re)subscribes (e.g. webview reload after
    /// standby): the frontend store reset to empty, but the poller's change-detection
    /// would otherwise suppress re-sending unchanged data, leaving the UI blank.
    ForceResync,
    Stop,
}

pub(crate) struct GitHubPoller {
    pub(crate) cmd_tx: mpsc::Sender<PollerCmd>,
}

impl GitHubPoller {
    #[cfg(feature = "desktop")]
    pub(crate) fn start(state: Arc<AppState>, handle: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(poll_loop(state, handle, rx));
        Self { cmd_tx: tx }
    }
}

#[cfg(feature = "desktop")]
/// Per-repo previous PR state for transition comparison.
type PrevState = HashMap<String, HashMap<String, BranchPrStatus>>;

#[cfg(feature = "desktop")]
struct PollMutableState {
    prev: PrevState,
    fail_count: u32,
    last_changed: HashMap<String, Instant>,
    /// Per-repo max PR updated_at — `None` means known-empty PR set.
    last_pr_updated_at: HashMap<String, Option<String>>,
    /// Per-repo max issue updated_at — `None` means known-empty issue set.
    last_issue_updated_at: HashMap<String, Option<String>>,
    /// When set, the next poll re-emits PR + issue state regardless of change
    /// detection, then clears. Set by `PollerCmd::ForceResync`.
    force_resync: bool,
}

#[cfg(feature = "desktop")]
async fn poll_loop(state: Arc<AppState>, handle: AppHandle, mut rx: mpsc::Receiver<PollerCmd>) {
    let mut visible = true;
    let mut paths: Vec<String> = Vec::new();
    let mut issue_filter = String::new();
    let mut pr_hide_drafts = false;
    let mut ps = PollMutableState {
        prev: HashMap::new(),
        fail_count: 0,
        last_changed: HashMap::new(),
        last_pr_updated_at: HashMap::new(),
        last_issue_updated_at: HashMap::new(),
        force_resync: false,
    };
    let mut startup = true;
    let mut poll_cycle: u32 = 0;
    // Pending on-demand poll: set by PollRepo/SetIssueFilter to fire the batch
    // early rather than spawning a separate single-repo API call.
    let mut pending_poll_at: Option<tokio::time::Instant> = None;
    // Scoped paths for on-demand polls (PollRepo). Empty = use all paths.
    let mut pending_poll_paths: Vec<String> = Vec::new();

    let mut interval = tokio::time::interval(current_interval(visible, ps.fail_count, u32::MAX));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        // Resolves immediately when pending_poll_at has elapsed; stays pending otherwise.
        let pending_sleep = async {
            match pending_poll_at {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            _ = pending_sleep => {
                pending_poll_at = None;
                let rate_budget = state.github_rate_limit_remaining.load(std::sync::atomic::Ordering::Relaxed);
                let batch = if pending_poll_paths.is_empty() { &paths } else { &pending_poll_paths };
                poll_batch(&state, &handle, batch, false, &issue_filter, pr_hide_drafts, &mut ps).await;
                pending_poll_paths.clear();
                let dur = current_interval(visible, ps.fail_count, rate_budget);
                interval = tokio::time::interval_at(tokio::time::Instant::now() + dur, dur);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            }
            _ = interval.tick() => {
                let rate_budget = state.github_rate_limit_remaining.load(std::sync::atomic::Ordering::Relaxed);
                let batch_paths = if startup {
                    paths.clone()
                } else {
                    let hot = state.hot_repo_paths.read();
                    tiered_paths(&paths, &ps.last_changed, poll_cycle, &hot)
                };
                poll_batch(&state, &handle, &batch_paths, startup, &issue_filter, pr_hide_drafts, &mut ps).await;
                startup = false;
                poll_cycle = poll_cycle.wrapping_add(1);
                pending_poll_at = None;
                pending_poll_paths.clear();
                let dur = current_interval(visible, ps.fail_count, rate_budget);
                interval = tokio::time::interval_at(tokio::time::Instant::now() + dur, dur);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            }
            cmd = rx.recv() => {
                match cmd {
                    Some(PollerCmd::SetVisibility(v)) => {
                        let was_hidden = !visible;
                        visible = v;
                        if was_hidden && visible {
                            // Became visible: fire immediately via pending_poll_at
                            pending_poll_at = Some(tokio::time::Instant::now());
                        }
                    }
                    Some(PollerCmd::PollRepo(path)) => {
                        if !pending_poll_paths.contains(&path) {
                            pending_poll_paths.push(path);
                        }
                        let at = tokio::time::Instant::now() + DEBOUNCE_WINDOW;
                        if pending_poll_at.is_none_or(|existing| at < existing) {
                            pending_poll_at = Some(at);
                        }
                    }
                    Some(PollerCmd::UpdatePaths(new_paths)) => {
                        paths = new_paths;
                    }
                    Some(PollerCmd::SetIssueFilter(filter)) => {
                        if filter != issue_filter {
                            issue_filter = filter;
                            pending_poll_at = Some(tokio::time::Instant::now());
                        }
                    }
                    Some(PollerCmd::SetPrHideDrafts(hide)) => {
                        if hide != pr_hide_drafts {
                            pr_hide_drafts = hide;
                            pending_poll_at = Some(tokio::time::Instant::now());
                        }
                    }
                    Some(PollerCmd::ForceResync) => {
                        // Frontend re-subscribed: re-emit full state on an immediate
                        // poll over all paths, bypassing change detection.
                        ps.force_resync = true;
                        pending_poll_paths.clear();
                        pending_poll_at = Some(tokio::time::Instant::now());
                    }
                    Some(PollerCmd::Stop) | None => break,
                }
            }
        }
    }
}

/// Deterministic hash of a path to a u32 — used for jitter offset.
fn path_hash(path: &str) -> u32 {
    let mut hasher = std::hash::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish() as u32
}

/// Select which repos to include in this poll cycle.
/// Active repos (PR data changed within ACTIVE_WINDOW) are polled every tick.
/// Idle repos are polled every IDLE_POLL_DIVISOR ticks.
/// Dormant repos (cold — no active terminals) are polled every DORMANT_POLL_DIVISOR
/// ticks with per-path jitter so they don't all fire on the same cycle.
/// Repos never seen yet are always included (ensures first fetch).
fn tiered_paths(
    all_paths: &[String],
    last_changed: &HashMap<String, Instant>,
    cycle: u32,
    hot_paths: &HashSet<String>,
) -> Vec<String> {
    let now = Instant::now();
    all_paths
        .iter()
        .filter(|p| {
            let is_hot = hot_paths.contains(p.as_str());
            match last_changed.get(p.as_str()) {
                None => true,
                Some(t) => {
                    if now.duration_since(*t) < ACTIVE_WINDOW {
                        true
                    } else if is_hot {
                        cycle.is_multiple_of(IDLE_POLL_DIVISOR)
                    } else {
                        let offset = path_hash(p) % DORMANT_POLL_DIVISOR;
                        cycle % DORMANT_POLL_DIVISOR == offset
                    }
                }
            }
        })
        .cloned()
        .collect()
}

/// Compute the next poll interval based on visibility, failure count, and rate-limit budget.
fn current_interval(visible: bool, fail_count: u32, rate_budget: u32) -> Duration {
    if !visible {
        return HIDDEN_INTERVAL;
    }
    if rate_budget < RATE_BUDGET_CRITICAL {
        return MAX_INTERVAL;
    }
    let base = if rate_budget < RATE_BUDGET_LOW {
        // Low budget — double the base interval to conserve points
        BASE_INTERVAL * 2
    } else {
        BASE_INTERVAL
    };
    if fail_count == 0 {
        return base;
    }
    let backoff = base.as_millis() as f64 * 2_f64.powi(fail_count as i32 - 1);
    Duration::from_millis(backoff.min(MAX_INTERVAL.as_millis() as f64) as u64)
}

/// Decide whether to (re-)emit a repo's PR or issue snapshot to the frontend.
///
/// Normally we only emit when the data changed since the previous poll (cheap
/// change detection on the max `updated_at`). `force` overrides that for a full
/// resync: when the frontend re-subscribes (e.g. webview reload after standby)
/// its store reset to empty, so unchanged data must be re-sent or the UI stays
/// blank until the next real change. `prev_ts == None` means this repo was never
/// polled before, which always counts as changed.
#[cfg(any(feature = "desktop", test))]
fn should_emit(prev_ts: Option<&Option<String>>, cur_ts: &Option<String>, force: bool) -> bool {
    force || prev_ts.is_none_or(|p| p != cur_ts)
}

#[cfg(feature = "desktop")]
async fn poll_batch(
    state: &AppState,
    handle: &AppHandle,
    paths: &[String],
    include_merged: bool,
    issue_filter: &str,
    pr_hide_drafts: bool,
    ps: &mut PollMutableState,
) {
    if paths.is_empty() {
        return;
    }
    if state.github_circuit_breaker.check().is_err() {
        return;
    }

    match crate::github::get_all_batch_impl(
        paths,
        include_merged,
        issue_filter,
        pr_hide_drafts,
        state,
    )
    .await
    {
        Ok(result) => {
            ps.fail_count = 0;
            let now = Instant::now();
            // One-shot full re-emit requested by a frontend re-subscribe. Consumed
            // here so a single successful poll re-hydrates the (reset) frontend store.
            let force = ps.force_resync;
            ps.force_resync = false;

            for (repo_path, statuses) in result.prs {
                let changed =
                    process_repo_update(state, handle, &repo_path, &statuses, &mut ps.prev);
                if changed {
                    ps.last_changed.insert(repo_path.clone(), now);
                } else {
                    ps.last_changed.entry(repo_path.clone()).or_insert(now);
                }

                let cur_ts = statuses
                    .iter()
                    .map(|s| s.updated_at.as_str())
                    .filter(|s| !s.is_empty())
                    .max()
                    .map(|s| s.to_string());
                let emit = should_emit(ps.last_pr_updated_at.get(&repo_path), &cur_ts, force);
                ps.last_pr_updated_at.insert(repo_path.clone(), cur_ts);

                if emit {
                    let _ = handle.emit(
                        "github-pr-update",
                        PrUpdatePayload {
                            repo_path: repo_path.clone(),
                            statuses: statuses.clone(),
                        },
                    );
                    let _ = state.event_bus.send(AppEvent::GitHubPrUpdate {
                        repo_path,
                        statuses,
                    });
                }
            }

            for (repo_path, issues) in result.issues {
                let cur_ts = issues
                    .iter()
                    .map(|i| i.updated_at.as_str())
                    .filter(|s| !s.is_empty())
                    .max()
                    .map(|s| s.to_string());
                let emit = should_emit(ps.last_issue_updated_at.get(&repo_path), &cur_ts, force);
                ps.last_issue_updated_at.insert(repo_path.clone(), cur_ts);

                if emit {
                    let _ = handle.emit(
                        "github-issues-update",
                        IssuesUpdatePayload {
                            repo_path: repo_path.clone(),
                            issues: issues.clone(),
                        },
                    );
                    let _ = state
                        .event_bus
                        .send(AppEvent::GitHubIssuesUpdate { repo_path, issues });
                }
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if !msg.starts_with("rate-limit:") {
                ps.fail_count = ps.fail_count.saturating_add(1);
            }
            tracing::warn!(source = "github_poller", "poll_all failed: {msg}");
        }
    }
}

#[cfg(feature = "desktop")]
/// Process PR updates for a single repo. Returns `true` if any PR data changed.
fn process_repo_update(
    state: &AppState,
    handle: &AppHandle,
    repo_path: &str,
    statuses: &[BranchPrStatus],
    prev: &mut PrevState,
) -> bool {
    // First poll for this repo seeds `old_map` with pre-existing PRs; those must
    // not fire `Opened`. Only PRs that appear on a *later* poll are genuinely new.
    let first_poll_for_repo = !prev.contains_key(repo_path);
    let old_map = prev.entry(repo_path.to_string()).or_default();
    let mut changed = false;
    for new_pr in statuses {
        let is_new = if let Some(old_pr) = old_map.get(&new_pr.branch) {
            let transitions = detect_transitions(repo_path, old_pr, new_pr);
            if !transitions.is_empty() {
                changed = true;
            }
            for t in transitions {
                let _ = handle.emit("github-transition", &t);
                let _ = state
                    .event_bus
                    .send(AppEvent::GitHubTransition { transition: t });
            }
            old_pr.updated_at != new_pr.updated_at
                || old_pr.checks != new_pr.checks
                || old_pr.state != new_pr.state
        } else {
            // Brand-new branch: emit Opened for open PRs (skipping the first poll's
            // pre-existing set). The poller inserts it into old_map below, so a PR
            // fires Opened at most once per appearance.
            if !first_poll_for_repo && new_pr.state.to_uppercase() == "OPEN" {
                let t = PrTransition::Opened {
                    repo_path: repo_path.to_string(),
                    branch: new_pr.branch.clone(),
                    pr_number: new_pr.number,
                    title: new_pr.title.clone(),
                    head_ref_oid: new_pr.head_ref_oid.clone(),
                    author: new_pr.author.clone(),
                };
                let _ = handle.emit("github-transition", &t);
                let _ = state
                    .event_bus
                    .send(AppEvent::GitHubTransition { transition: t });
            }
            true
        };
        if is_new {
            changed = true;
        }
    }
    let old_len = old_map.len();
    let new_branches: std::collections::HashSet<&str> =
        statuses.iter().map(|s| s.branch.as_str()).collect();
    old_map.retain(|branch, _| new_branches.contains(branch.as_str()));
    for new_pr in statuses {
        old_map.insert(new_pr.branch.clone(), new_pr.clone());
    }
    if old_len != statuses.len() {
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_start_polling(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
    paths: Vec<String>,
    issue_filter: String,
    pr_hide_drafts: bool,
) -> Result<(), String> {
    let mut guard = state.github_poller.lock();
    if guard.is_some() {
        // Already running — just update paths and filter
        if let Some(poller) = guard.as_ref() {
            if let Err(e) = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(paths)) {
                tracing::warn!(
                    source = "github",
                    "Failed to send UpdatePaths to poller: {e}"
                );
            }
            if let Err(e) = poller
                .cmd_tx
                .try_send(PollerCmd::SetIssueFilter(issue_filter))
            {
                tracing::warn!(
                    source = "github",
                    "Failed to send SetIssueFilter to poller: {e}"
                );
            }
            if let Err(e) = poller
                .cmd_tx
                .try_send(PollerCmd::SetPrHideDrafts(pr_hide_drafts))
            {
                tracing::warn!(
                    source = "github",
                    "Failed to send SetPrHideDrafts to poller: {e}"
                );
            }
            // Frontend just (re)subscribed — its store reset to empty (e.g. webview
            // reload after standby). Force a full re-emit so unchanged PRs/issues
            // re-hydrate instead of staying blank until the next data change.
            if let Err(e) = poller.cmd_tx.try_send(PollerCmd::ForceResync) {
                tracing::warn!(
                    source = "github",
                    "Failed to send ForceResync to poller: {e}"
                );
            }
        }
        return Ok(());
    }
    let poller = GitHubPoller::start(Arc::clone(&state), app);
    if let Err(e) = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(paths)) {
        tracing::warn!(source = "github", "Failed to send initial UpdatePaths: {e}");
    }
    if let Err(e) = poller
        .cmd_tx
        .try_send(PollerCmd::SetIssueFilter(issue_filter))
    {
        tracing::warn!(
            source = "github",
            "Failed to send initial SetIssueFilter: {e}"
        );
    }
    if let Err(e) = poller
        .cmd_tx
        .try_send(PollerCmd::SetPrHideDrafts(pr_hide_drafts))
    {
        tracing::warn!(
            source = "github",
            "Failed to send initial SetPrHideDrafts: {e}"
        );
    }
    *guard = Some(poller);
    Ok(())
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_set_visibility(
    state: tauri::State<'_, Arc<AppState>>,
    visible: bool,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref()
        && let Err(e) = poller.cmd_tx.try_send(PollerCmd::SetVisibility(visible))
    {
        tracing::warn!(source = "github", "Failed to send SetVisibility: {e}");
    }
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_poll_repo(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref()
        && let Err(e) = poller.cmd_tx.try_send(PollerCmd::PollRepo(path))
    {
        tracing::warn!(source = "github", "Failed to send PollRepo: {e}");
    }
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_update_paths(
    state: tauri::State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref()
        && let Err(e) = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(paths))
    {
        tracing::warn!(source = "github", "Failed to send UpdatePaths: {e}");
    }
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_set_issue_filter(
    state: tauri::State<'_, Arc<AppState>>,
    filter: String,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref()
        && let Err(e) = poller.cmd_tx.try_send(PollerCmd::SetIssueFilter(filter))
    {
        tracing::warn!(source = "github", "Failed to send SetIssueFilter: {e}");
    }
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_set_pr_hide_drafts(
    state: tauri::State<'_, Arc<AppState>>,
    hide: bool,
) -> Result<(), String> {
    if let Some(poller) = state.github_poller.lock().as_ref()
        && let Err(e) = poller.cmd_tx.try_send(PollerCmd::SetPrHideDrafts(hide))
    {
        tracing::warn!(source = "github", "Failed to send SetPrHideDrafts: {e}");
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

    fn make_pr(
        state: &str,
        mergeable: &str,
        review: &str,
        failed: u32,
        pending: u32,
    ) -> BranchPrStatus {
        BranchPrStatus {
            branch: "feat/test".to_string(),
            number: 42,
            title: "Test PR".to_string(),
            state: state.to_string(),
            url: String::new(),
            additions: 0,
            deletions: 0,
            checks: CheckSummary {
                passed: 0,
                failed,
                pending,
                total: failed + pending,
            },
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
    fn transition_pushed() {
        // New commit on an open PR: head_ref_oid changed → exactly one Pushed.
        let mut old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        old.head_ref_oid = "aaa111".to_string();
        let mut new = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        new.head_ref_oid = "bbb222".to_string();
        new.author = "octocat".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert_eq!(t.len(), 1);
        assert!(matches!(
            &t[0],
            PrTransition::Pushed { head_ref_oid, author, .. }
                if head_ref_oid == "bbb222" && author == "octocat"
        ));
    }

    #[test]
    fn pushed_same_oid_none() {
        // Unchanged head_ref_oid → no Pushed.
        let mut old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        old.head_ref_oid = "aaa111".to_string();
        let mut new = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        new.head_ref_oid = "aaa111".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert!(!t.iter().any(|x| matches!(x, PrTransition::Pushed { .. })));
    }

    #[test]
    fn pushed_non_open_none() {
        // oid changed but PR is no longer OPEN → no Pushed (Closed fires instead).
        let mut old = make_pr("OPEN", "MERGEABLE", "", 0, 0);
        old.head_ref_oid = "aaa111".to_string();
        let mut new = make_pr("CLOSED", "MERGEABLE", "", 0, 0);
        new.head_ref_oid = "bbb222".to_string();
        let t = detect_transitions("/repo", &old, &new);
        assert!(!t.iter().any(|x| matches!(x, PrTransition::Pushed { .. })));
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

    #[test]
    fn dormant_repo_appears_every_10th_cycle() {
        let paths = vec!["/cold/repo".to_string()];
        let mut last_changed = HashMap::new();
        last_changed.insert(
            "/cold/repo".to_string(),
            Instant::now() - Duration::from_secs(3600),
        );
        let hot_paths = HashSet::new();

        let offset = path_hash("/cold/repo") % DORMANT_POLL_DIVISOR;
        let mut included_cycles = Vec::new();
        for cycle in 0..20 {
            let batch = tiered_paths(&paths, &last_changed, cycle, &hot_paths);
            if !batch.is_empty() {
                included_cycles.push(cycle);
            }
        }
        assert_eq!(
            included_cycles.len(),
            2,
            "dormant repo should appear twice in 20 cycles"
        );
        assert_eq!(included_cycles[0], offset);
        assert_eq!(included_cycles[1], offset + DORMANT_POLL_DIVISOR);
    }

    #[test]
    fn hot_repo_uses_idle_divisor_not_dormant() {
        let paths = vec!["/hot/repo".to_string()];
        let mut last_changed = HashMap::new();
        last_changed.insert(
            "/hot/repo".to_string(),
            Instant::now() - Duration::from_secs(3600),
        );
        let mut hot_paths = HashSet::new();
        hot_paths.insert("/hot/repo".to_string());

        let mut count = 0;
        for cycle in 0..10 {
            let batch = tiered_paths(&paths, &last_changed, cycle, &hot_paths);
            if !batch.is_empty() {
                count += 1;
            }
        }
        assert_eq!(
            count, 2,
            "hot idle repo should appear every 5th cycle = 2 times in 10"
        );
    }

    #[test]
    fn path_hash_distributes_across_cycles() {
        let offsets: HashSet<u32> = ["/repo/a", "/repo/b", "/repo/c", "/repo/d", "/repo/e"]
            .iter()
            .map(|p| path_hash(p) % DORMANT_POLL_DIVISOR)
            .collect();
        assert!(
            offsets.len() >= 2,
            "hash should produce at least 2 distinct offsets for 5 paths"
        );
    }

    // --- should_emit: change detection vs. forced resync ------------------
    // The bug these guard: after standby the webview reloads, resetting the
    // frontend GitHub store to empty. The Rust poller survives and keeps its
    // change-detection state, so unchanged PRs/issues were never re-sent and the
    // sidebar badges (incl. issue counts) stayed blank. `force` fixes that by
    // re-emitting current state on a frontend re-subscribe.

    #[test]
    fn emit_on_first_poll() {
        // Never-polled repo (no prior timestamp) is always new data → emit.
        let cur = Some("2026-06-03T00:00:00Z".to_string());
        assert!(should_emit(None, &cur, false));
    }

    #[test]
    fn no_emit_when_unchanged() {
        // The optimization: identical max updated_at and no resync → skip emit.
        let ts = Some("2026-06-03T00:00:00Z".to_string());
        assert!(!should_emit(Some(&ts), &ts, false));
    }

    #[test]
    fn emit_when_changed() {
        let prev = Some("2026-06-03T00:00:00Z".to_string());
        let cur = Some("2026-06-03T09:00:00Z".to_string());
        assert!(should_emit(Some(&prev), &cur, false));
    }

    #[test]
    fn resync_re_emits_unchanged_data() {
        // THE FIX: data is identical, but the frontend re-subscribed (force=true)
        // so we must re-send it — otherwise the reset store stays empty.
        let ts = Some("2026-06-03T00:00:00Z".to_string());
        assert!(should_emit(Some(&ts), &ts, true));
    }

    #[test]
    fn resync_re_emits_known_empty_repo() {
        // A repo with zero open issues (cur = None) that was already known-empty
        // (prev = Some(None)): no change, but a resync must still re-confirm the
        // empty set so the frontend doesn't show stale counts.
        let prev: Option<String> = None;
        let cur: Option<String> = None;
        assert!(!should_emit(Some(&prev), &cur, false));
        assert!(should_emit(Some(&prev), &cur, true));
    }
}
