use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
#[cfg(feature = "desktop")]
use tauri::Emitter;
use tokio::sync::{Notify, mpsc};

use crate::github::BranchPrStatus;
use crate::state::{AppEvent, AppState};

#[cfg(test)]
#[path = "github_volume_measurement_1995.rs"]
mod volume_measurement_1995;

#[cfg(test)]
#[path = "github_live_measurement_1995.rs"]
mod live_measurement_1995;

/// Desktop delivery is optional; the event bus is authoritative on headless daemons.
fn emit_window<T: Serialize + Clone>(state: &AppState, event: &str, payload: T) {
    #[cfg(feature = "desktop")]
    if let Some(handle) = state.app_handle.read().as_ref()
        && let Err(error) = handle.emit(event, payload)
    {
        tracing::warn!(source = "github", event, %error, "GitHub window emission failed");
    }
    #[cfg(not(feature = "desktop"))]
    let _ = (state, event, payload);
}

// ---------------------------------------------------------------------------
pub(crate) use tuic_git::github_poller::{PrTransition, detect_transitions};

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
}

pub(crate) struct GitHubPoller {
    pub(crate) cmd_tx: mpsc::Sender<PollerCmd>,
    /// Successful PR snapshots also identify monitored repositories. Unlike the
    /// query cache, this state lives until a path is removed or this poller stops.
    pub(crate) previous_prs: Arc<parking_lot::RwLock<PrevState>>,
    /// Stop signal, raised by [`stop_poller`].
    ///
    /// Stop does not travel on `cmd_tx`: the loop only drains that channel
    /// between polls, so a Stop sent while a request is in flight waits for the
    /// request — and a request against a half-open socket may never return.
    /// The notify is awaited *alongside* the poll instead, so it lands whatever
    /// the loop is doing.
    pub(crate) stop: Arc<Notify>,
}

impl GitHubPoller {
    pub(crate) fn start(state: Arc<AppState>) -> Self {
        let (tx, rx) = mpsc::channel(32);
        let stop = Arc::new(Notify::new());
        let previous_prs = Arc::new(parking_lot::RwLock::new(HashMap::new()));
        tokio::spawn(poll_loop(
            state,
            rx,
            Arc::clone(&stop),
            Arc::clone(&previous_prs),
        ));
        Self {
            cmd_tx: tx,
            stop,
            previous_prs,
        }
    }

    pub(crate) fn monitored_repo_count(&self) -> usize {
        self.previous_prs.read().len()
    }
}

/// Per-repo previous PR state for transition comparison.
type PrevState = HashMap<String, HashMap<String, BranchPrStatus>>;

#[derive(Default)]
struct PollMutableState {
    prev: Arc<parking_lot::RwLock<PrevState>>,
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

impl PollMutableState {
    fn retain_paths(&mut self, paths: &[String]) {
        let retained: HashSet<&str> = paths.iter().map(String::as_str).collect();
        self.prev
            .write()
            .retain(|path, _| retained.contains(path.as_str()));
        self.last_changed
            .retain(|path, _| retained.contains(path.as_str()));
        self.last_pr_updated_at
            .retain(|path, _| retained.contains(path.as_str()));
        self.last_issue_updated_at
            .retain(|path, _| retained.contains(path.as_str()));
    }
}

/// Await a poll batch, giving up as soon as `stop` is raised.
///
/// Returns `false` when Stop cut the poll short. `biased` makes a Stop that is
/// already pending win over a poll that happens to be ready in the same tick —
/// a shutdown must not be delayed by one more round of event emission.
async fn poll_batch_or_stop(stop: &Notify, batch: impl std::future::Future<Output = ()>) -> bool {
    tokio::select! {
        biased;
        _ = stop.notified() => false,
        _ = batch => true,
    }
}

async fn poll_loop(
    state: Arc<AppState>,
    mut rx: mpsc::Receiver<PollerCmd>,
    stop: Arc<Notify>,
    previous_prs: Arc<parking_lot::RwLock<PrevState>>,
) {
    let mut visible = true;
    let mut paths: Vec<String> = Vec::new();
    let mut issue_filter = String::new();
    let mut pr_hide_drafts = false;
    let mut ps = PollMutableState {
        prev: previous_prs,
        ..Default::default()
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
                let rate_budget = crate::github::min_rate_budget(&state);
                let batch = if pending_poll_paths.is_empty() { &paths } else { &pending_poll_paths };
                let finished = poll_batch_or_stop(
                    &stop,
                    poll_batch(&state, batch, false, &issue_filter, pr_hide_drafts, &mut ps),
                )
                .await;
                if !finished {
                    break;
                }
                pending_poll_paths.clear();
                let dur = current_interval(visible, ps.fail_count, rate_budget);
                interval = tokio::time::interval_at(tokio::time::Instant::now() + dur, dur);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            }
            _ = interval.tick() => {
                let rate_budget = crate::github::min_rate_budget(&state);
                let batch_paths = if startup {
                    paths.clone()
                } else {
                    let hot = state.hot_repo_paths.read();
                    tiered_paths(&paths, &ps.last_changed, poll_cycle, &hot)
                };
                let finished = poll_batch_or_stop(
                    &stop,
                    poll_batch(&state, &batch_paths, startup, &issue_filter, pr_hide_drafts, &mut ps),
                )
                .await;
                if !finished {
                    break;
                }
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
                        // An asynchronous action may finish after its repo was removed.
                        if !paths.contains(&path) {
                            continue;
                        }
                        if !pending_poll_paths.contains(&path) {
                            pending_poll_paths.push(path);
                        }
                        let at = tokio::time::Instant::now() + DEBOUNCE_WINDOW;
                        if pending_poll_at.is_none_or(|existing| at < existing) {
                            pending_poll_at = Some(at);
                        }
                    }
                    Some(PollerCmd::UpdatePaths(new_paths)) => {
                        ps.retain_paths(&new_paths);
                        pending_poll_paths.retain(|path| new_paths.contains(path));
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
                    // The sender lives in the `GitHubPoller` that `stop_poller`
                    // takes out of state, so a closed channel also means stop.
                    None => break,
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
fn should_emit(prev_ts: Option<&Option<String>>, cur_ts: &Option<String>, force: bool) -> bool {
    force || prev_ts.is_none_or(|p| p != cur_ts)
}

/// Change-detection key for a PR/issue snapshot: `<count>|<max updated_at>`.
/// The count is essential — an item that drops out of the open set (merged/closed
/// PR, closed issue) shrinks the list without necessarily moving the max
/// timestamp, so a max-only key would miss the removal and leave the badge stale.
fn snapshot_key(count: usize, max_updated_at: &str) -> String {
    format!("{count}|{max_updated_at}")
}

async fn poll_batch(
    state: &AppState,
    paths: &[String],
    include_merged: bool,
    issue_filter: &str,
    pr_hide_drafts: bool,
    ps: &mut PollMutableState,
) {
    if paths.is_empty() {
        return;
    }
    // Per-account circuit breakers are checked inside get_all_batch_impl so a
    // single failing account no longer blocks polling of the others.

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
                    process_repo_update(state, &repo_path, &statuses, &mut ps.prev.write());
                if changed {
                    ps.last_changed.insert(repo_path.clone(), now);
                } else {
                    ps.last_changed.entry(repo_path.clone()).or_insert(now);
                }

                // Key on count + max(updated_at): a PR that drops out of the open
                // set (merged/closed) shrinks the list without necessarily changing
                // the max timestamp, so max alone would miss the removal and leave
                // the badge stale. The count flips the signal in that case.
                let max_ts = statuses
                    .iter()
                    .map(|s| s.updated_at.as_str())
                    .filter(|s| !s.is_empty())
                    .max()
                    .unwrap_or_default();
                let cur_ts = Some(snapshot_key(statuses.len(), max_ts));
                let emit = should_emit(ps.last_pr_updated_at.get(&repo_path), &cur_ts, force);
                ps.last_pr_updated_at.insert(repo_path.clone(), cur_ts);

                if emit {
                    emit_window(
                        state,
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
                // Key on count + max(updated_at): a closed issue drops out of the
                // OPEN set, shrinking the list without changing the max timestamp
                // when it was not the latest — max alone misses the removal and the
                // badge stays stale (observed: stuck at 7 after closing issues via
                // the API). The count flips the signal in that case.
                let max_ts = issues
                    .iter()
                    .map(|i| i.updated_at.as_str())
                    .filter(|s| !s.is_empty())
                    .max()
                    .unwrap_or_default();
                let cur_ts = Some(snapshot_key(issues.len(), max_ts));
                let emit = should_emit(ps.last_issue_updated_at.get(&repo_path), &cur_ts, force);
                ps.last_issue_updated_at.insert(repo_path.clone(), cur_ts);

                if emit {
                    emit_window(
                        state,
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

/// Process PR updates for a single repo. Returns `true` if any PR data changed.
fn process_repo_update(
    state: &AppState,
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
                emit_window(state, "github-transition", &t);
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
                emit_window(state, "github-transition", &t);
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
// Shared start/config helpers (IPC/HTTP parity)
// ---------------------------------------------------------------------------

/// Push the poll config (paths, issue filter, hide-drafts) to a running poller.
///
/// `resync` additionally requests a full re-emit (`ForceResync`) — sent when a
/// client (re)subscribes to an *already-running* poller whose frontend store may
/// have reset (e.g. webview reload after standby), so unchanged PRs/issues
/// re-hydrate instead of staying blank until the next data change. A freshly
/// started poller has no prior state to resync, so it passes `resync = false`.
///
/// Shared by the Tauri command and the HTTP route so both transports send the
/// identical command sequence.
pub(crate) fn send_poller_config(
    poller: &GitHubPoller,
    paths: Vec<String>,
    issue_filter: String,
    pr_hide_drafts: bool,
    resync: bool,
) {
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
    if resync && let Err(e) = poller.cmd_tx.try_send(PollerCmd::ForceResync) {
        tracing::warn!(
            source = "github",
            "Failed to send ForceResync to poller: {e}"
        );
    }
}

/// Start the GitHub poller if not already running, then push the poll config.
///
/// Cold start: spawns the poller and seeds it (no resync — nothing to re-emit).
/// Already running: forwards the new config and forces a resync for the
/// (re)subscribing client. This is the single implementation behind both the
/// Tauri `github_start_polling` command and the HTTP `poller_start` route.
pub(crate) fn ensure_polling(
    state: &Arc<AppState>,
    paths: Vec<String>,
    issue_filter: String,
    pr_hide_drafts: bool,
) {
    let mut guard = state.github.poller.lock();
    if let Some(poller) = guard.as_ref() {
        send_poller_config(poller, paths, issue_filter, pr_hide_drafts, true);
        return;
    }
    let poller = GitHubPoller::start(Arc::clone(state));
    send_poller_config(&poller, paths, issue_filter, pr_hide_drafts, false);
    *guard = Some(poller);
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_start_polling(
    state: tauri::State<'_, Arc<AppState>>,
    paths: Vec<String>,
    issue_filter: String,
    pr_hide_drafts: bool,
) -> Result<(), String> {
    ensure_polling(state.inner(), paths, issue_filter, pr_hide_drafts);
    Ok(())
}

/// Deliver a control command to the running GitHub poller.
///
/// The five control entry points used to swallow the outcome — `let _ = try_send`
/// on the HTTP side, a `warn!` that still returned `Ok(())` on the IPC side — and
/// reported success even when NO poller was running. A client that changed the
/// issue filter or the visibility had no way to learn the change went nowhere.
///
/// Both transports go through this one function so their shapes cannot drift
/// (see AGENTS.md "IPC / HTTP Parity").
///
/// The sender is cloned out of the lock before `try_send` so the poller mutex is
/// never held across the send.
pub(crate) fn send_poller_cmd(state: &AppState, cmd: PollerCmd) -> Result<(), String> {
    let tx = state
        .github
        .poller
        .lock()
        .as_ref()
        .map(|poller| poller.cmd_tx.clone())
        .ok_or_else(|| "GitHub poller is not running".to_string())?;
    tx.try_send(cmd)
        .map_err(|e| format!("GitHub poller command not delivered: {e}"))
}

/// Stop the poller and clear it from state.
///
/// Separate from [`send_poller_cmd`] because Stop does not travel on the command
/// channel at all. It used to, and that made stopping only as reliable as the
/// loop's willingness to read the channel: a poll awaited inline in the loop's
/// `select!` blocks the read, and against a half-open socket that poll never
/// returns, so the Stop was stranded for good. Raising the notify instead lands
/// on the poll itself — see [`GitHubPoller::stop`].
pub(crate) async fn stop_poller(state: &AppState) -> Result<(), String> {
    let poller = state
        .github
        .poller
        .lock()
        .take()
        .ok_or_else(|| "GitHub poller is not running".to_string())?;
    poller.stop.notify_one();
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_stop_polling(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    stop_poller(&state).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_set_visibility(
    state: tauri::State<'_, Arc<AppState>>,
    visible: bool,
) -> Result<(), String> {
    send_poller_cmd(&state, PollerCmd::SetVisibility(visible))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_poll_repo(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    send_poller_cmd(&state, PollerCmd::PollRepo(path))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_update_paths(
    state: tauri::State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<(), String> {
    send_poller_cmd(&state, PollerCmd::UpdatePaths(paths))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_set_issue_filter(
    state: tauri::State<'_, Arc<AppState>>,
    filter: String,
) -> Result<(), String> {
    send_poller_cmd(&state, PollerCmd::SetIssueFilter(filter))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_set_pr_hide_drafts(
    state: tauri::State<'_, Arc<AppState>>,
    hide: bool,
) -> Result<(), String> {
    github_set_pr_hide_drafts_impl(state.inner(), hide)
}

pub(crate) fn github_set_pr_hide_drafts_impl(
    state: &Arc<AppState>,
    hide: bool,
) -> Result<(), String> {
    if let Some(poller) = state.github.poller.lock().as_ref()
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

    // Catches: expiring query payloads falsely reporting that a successfully polled repo is unmonitored.
    #[tokio::test]
    async fn monitored_repository_survives_query_cache_eviction() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        ensure_polling(&state, vec![], String::new(), false);
        let previous = Arc::clone(&state.github.poller.lock().as_ref().unwrap().previous_prs);
        state
            .git_cache
            .github_status
            .insert("/monitored".into(), Arc::new(vec![]));
        process_repo_update(&state, "/monitored", &[], &mut previous.write());
        state.git_cache.github_status.run_pending_tasks();
        assert_eq!(
            crate::github_auth::compute_diagnostics(&state).repos_monitored,
            1
        );

        state.git_cache.github_status.invalidate_all();
        state.git_cache.github_status.run_pending_tasks();
        assert!(state.git_cache.github_status.get("/monitored").is_none());
        assert_eq!(
            crate::github_auth::compute_diagnostics(&state).repos_monitored,
            1,
            "query payload eviction must not erase successful monitoring identity"
        );
        stop_poller(&state).await.unwrap();
    }

    // Catches: duplicate snapshots inflating the gauge, or retired paths/pollers keeping ghost monitors.
    #[tokio::test]
    async fn monitoring_count_follows_paths_and_poller_lifetime() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        ensure_polling(
            &state,
            vec!["/kept".into(), "/removed".into()],
            String::new(),
            false,
        );
        let previous = Arc::clone(&state.github.poller.lock().as_ref().unwrap().previous_prs);
        for path in ["/kept", "/removed", "/kept"] {
            process_repo_update(&state, path, &[], &mut previous.write());
        }
        assert_eq!(
            crate::github_auth::compute_diagnostics(&state).repos_monitored,
            2
        );
        send_poller_cmd(&state, PollerCmd::UpdatePaths(vec!["/kept".into()])).unwrap();
        // No setup-speed bound: nextest owns the outer hang limit.
        while crate::github_auth::compute_diagnostics(&state).repos_monitored != 1 {
            tokio::task::yield_now().await;
        }
        stop_poller(&state).await.unwrap();
        assert_eq!(
            crate::github_auth::compute_diagnostics(&state).repos_monitored,
            0
        );
    }

    // Catches: queued or late PollRepo resurrecting a retired monitor after UpdatePaths.
    #[tokio::test]
    async fn removed_paths_cannot_return_through_on_demand_polls() {
        let mut fixture = crate::github::partial_tests::PollHttpFixture::start(true).await;
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
        let mut paths = Vec::new();
        for name in ["removed", "kept"] {
            let repo = scratch.path().join(name);
            std::fs::create_dir(&repo).unwrap();
            for args in [
                vec!["init", "--quiet"],
                vec![
                    "remote",
                    "add",
                    "origin",
                    "https://github.com/sstraus/tuicommander.git",
                ],
            ] {
                let output = std::process::Command::new("git")
                    .args(args)
                    .current_dir(&repo)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            paths.push(repo.to_string_lossy().into_owned());
        }
        let mut events = fixture.state.event_bus.subscribe();
        ensure_polling(&fixture.state, paths.clone(), "all".into(), false);
        // Wait for a real successful snapshot, not a hand-populated history.
        loop {
            if matches!(
                events.recv().await.unwrap(),
                AppEvent::GitHubPrUpdate { .. }
            ) && crate::github_auth::compute_diagnostics(&fixture.state).repos_monitored == 2
            {
                break;
            }
        }
        fixture.requests.recv().await.unwrap();
        while events.try_recv().is_ok() {}
        for late in [false, true] {
            if !late {
                send_poller_cmd(&fixture.state, PollerCmd::PollRepo(paths[0].clone())).unwrap();
                send_poller_cmd(
                    &fixture.state,
                    PollerCmd::UpdatePaths(vec![paths[1].clone()]),
                )
                .unwrap();
            } else {
                send_poller_cmd(&fixture.state, PollerCmd::PollRepo(paths[0].clone())).unwrap();
            }
            // A configured-path request gives an observable completion barrier
            // for the same debounce batch, without sleeping or racing the network.
            send_poller_cmd(&fixture.state, PollerCmd::PollRepo(paths[1].clone())).unwrap();
            send_poller_cmd(
                &fixture.state,
                PollerCmd::SetIssueFilter(if late { "assigned" } else { "created" }.into()),
            )
            .unwrap();
            loop {
                if let AppEvent::GitHubIssuesUpdate { repo_path, .. } = events.recv().await.unwrap()
                    && repo_path == paths[1]
                {
                    break;
                }
            }
            let request = fixture.requests.recv().await.unwrap();
            assert_eq!(
                crate::github_auth::compute_diagnostics(&fixture.state).repos_monitored,
                1,
                "retired monitor returned (late={late}); request={request}"
            );
        }
        stop_poller(&fixture.state).await.unwrap();
        fixture.close().await;
    }

    #[tokio::test]
    async fn headless_poller_cold_starts_without_an_app_handle() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        ensure_polling(&state, vec![], String::new(), false);
        assert!(state.github.poller.lock().is_some());
        stop_poller(&state).await.expect("empty-path poller stops");
    }

    #[test]
    fn headless_transitions_reach_the_bus_once_without_a_window() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();
        let mut prev = HashMap::new();
        let mut pr = BranchPrStatus {
            branch: "feature".into(),
            number: 42,
            title: "Feature".into(),
            state: "OPEN".into(),
            url: "https://github.com/example/repo/pull/42".into(),
            additions: 0,
            deletions: 0,
            checks: crate::github::CheckSummary {
                passed: 0,
                failed: 0,
                pending: 0,
                total: 0,
            },
            author: "author".into(),
            commits: 1,
            mergeable: "MERGEABLE".into(),
            merge_state_status: "CLEAN".into(),
            review_decision: String::new(),
            viewer_did_approve: false,
            labels: vec![],
            is_draft: false,
            base_ref_name: "main".into(),
            head_ref_oid: "head".into(),
            created_at: String::new(),
            updated_at: String::new(),
            merge_state_label: None,
            conflict_state: crate::github::ConflictState::Clear,
            review_state_label: None,
            merge_commit_allowed: true,
            squash_merge_allowed: true,
            rebase_merge_allowed: true,
            unresolved_threads: 0,
            unresolved_threads_truncated: false,
        };
        process_repo_update(&state, "/repo", &[pr.clone()], &mut prev);
        assert!(
            events.try_recv().is_err(),
            "initial snapshot must not report opened"
        );
        pr.state = "MERGED".into();
        process_repo_update(&state, "/repo", &[pr.clone()], &mut prev);
        assert!(matches!(
            events.try_recv().expect("headless transition"),
            AppEvent::GitHubTransition {
                transition: PrTransition::Merged { .. }
            }
        ));
        process_repo_update(&state, "/repo", &[pr], &mut prev);
        assert!(
            events.try_recv().is_err(),
            "unchanged snapshot must not repeat a notice"
        );
    }

    /// The five control routes used to answer `{"ok": true}` no matter what,
    /// including when no poller was running at all — so a client that changed the
    /// issue filter or hid the window had no way to learn the command went
    /// nowhere. Both transports now share this one function.
    #[test]
    fn a_command_for_a_stopped_poller_is_an_error() {
        let state = crate::state::tests_support::make_test_app_state();
        assert!(state.github.poller.lock().is_none(), "fixture: no poller");

        let err = send_poller_cmd(&state, PollerCmd::SetVisibility(false))
            .expect_err("no poller must not report success");
        assert!(err.contains("not running"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn stopping_a_poller_that_is_not_running_is_an_error() {
        let state = crate::state::tests_support::make_test_app_state();
        let err = stop_poller(&state)
            .await
            .expect_err("no poller must not report success");
        assert!(err.contains("not running"), "unexpected error: {err}");
    }

    /// A poll request that never resolves is exactly what a half-open socket
    /// produces after a VPN drop. `poll_batch` is awaited inline in the poller's
    /// `select!`, so without an abort path the loop never reaches `rx.recv()`
    /// again and the Stop is stranded behind a request that will never land.
    #[tokio::test]
    async fn stop_aborts_a_poll_that_never_resolves() {
        let stop = Arc::new(tokio::sync::Notify::new());
        let signal = Arc::clone(&stop);
        let task =
            tokio::spawn(
                async move { poll_batch_or_stop(&signal, std::future::pending::<()>()).await },
            );

        // Give the task a chance to park on the never-resolving poll.
        tokio::task::yield_now().await;
        stop.notify_one();

        let completed = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("stop must abort the in-flight poll")
            .expect("poll task panicked");
        assert!(
            !completed,
            "a poll cancelled by Stop must report that it did not finish"
        );
    }

    /// The end-to-end stop path: `stop_poller` must reach a loop that is parked
    /// inside a request, not just one sitting idle on its command channel.
    #[tokio::test]
    async fn stop_poller_reaches_a_loop_parked_inside_a_request() {
        let state = crate::state::tests_support::make_test_app_state();
        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(8);
        let stop = Arc::new(tokio::sync::Notify::new());
        *state.github.poller.lock() = Some(GitHubPoller {
            cmd_tx,
            previous_prs: Default::default(),
            stop: Arc::clone(&stop),
        });

        let task =
            tokio::spawn(
                async move { poll_batch_or_stop(&stop, std::future::pending::<()>()).await },
            );
        tokio::task::yield_now().await;

        stop_poller(&state).await.expect("stop must be delivered");

        let completed = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("stop_poller must unpark the in-flight poll")
            .expect("poll task panicked");
        assert!(!completed, "the poll must report that Stop cut it short");
        assert!(
            state.github.poller.lock().is_none(),
            "stop_poller must clear the poller from state"
        );
    }

    /// A running poller receives the command and the send is reported as
    /// delivered — the filter must narrow the failure case, not break the
    /// success case.
    #[tokio::test]
    async fn a_command_for_a_running_poller_is_delivered() {
        let state = crate::state::tests_support::make_test_app_state();
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel(8);
        *state.github.poller.lock() = Some(GitHubPoller {
            cmd_tx,
            previous_prs: Default::default(),
            stop: Arc::new(Notify::new()),
        });

        send_poller_cmd(&state, PollerCmd::PollRepo("/repo".to_string())).expect("delivered");

        match cmd_rx.recv().await {
            Some(PollerCmd::PollRepo(path)) => assert_eq!(path, "/repo"),
            Some(_) => panic!("a different command was delivered"),
            None => panic!("nothing was delivered"),
        }
    }

    #[test]
    fn snapshot_key_detects_count_shrink_with_unchanged_max_ts() {
        // A closed issue / merged PR drops out of the open set, shrinking the
        // list. If it was not the most-recently-updated item, max(updated_at) is
        // unchanged — only the count moves. The snapshot key must still differ so
        // should_emit re-emits the smaller list (otherwise the badge stays stale,
        // the bug that left the sidebar stuck at 7 after closing issues via API).
        let ts = "2026-06-01T00:00:00Z";
        let prev = Some(snapshot_key(7, ts));
        let cur = Some(snapshot_key(4, ts));
        assert_ne!(prev, cur, "a count change must change the snapshot key");
        assert!(
            should_emit(Some(&prev), &cur, false),
            "a count shrink with unchanged max ts must trigger a re-emit"
        );
    }

    #[test]
    fn snapshot_key_stable_when_unchanged() {
        // No spurious emit when neither count nor max ts moved.
        let key = Some(snapshot_key(4, "2026-06-01T00:00:00Z"));
        assert!(
            !should_emit(Some(&key), &key, false),
            "an unchanged snapshot must not re-emit"
        );
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

    // --- IPC/HTTP parity: shared poller start/config path (story 127-89ec) ----
    // `send_poller_config` is the single command sequence behind both the Tauri
    // `github_start_polling` command and the HTTP `poller_start` route. The HTTP
    // route previously dropped SetPrHideDrafts + ForceResync; these guard that the
    // shared helper carries the full config on the (re)subscribe path and omits
    // only ForceResync on a fresh cold start.

    #[test]
    fn send_poller_config_resync_sends_full_sequence() {
        let (tx, mut rx) = mpsc::channel(8);
        let poller = GitHubPoller {
            cmd_tx: tx,
            previous_prs: Default::default(),
            stop: Arc::new(Notify::new()),
        };
        send_poller_config(
            &poller,
            vec!["/repo".to_string()],
            "assigned".to_string(),
            true,
            true,
        );
        assert!(
            matches!(rx.try_recv(), Ok(PollerCmd::UpdatePaths(p)) if p == vec!["/repo".to_string()])
        );
        assert!(matches!(rx.try_recv(), Ok(PollerCmd::SetIssueFilter(f)) if f == "assigned"));
        assert!(matches!(
            rx.try_recv(),
            Ok(PollerCmd::SetPrHideDrafts(true))
        ));
        assert!(matches!(rx.try_recv(), Ok(PollerCmd::ForceResync)));
        assert!(rx.try_recv().is_err(), "no extra commands expected");
    }

    #[test]
    fn send_poller_config_cold_start_omits_resync() {
        // A freshly started poller has no prior state to re-emit, so cold start
        // passes resync = false: hide-drafts is still forwarded, ForceResync is not.
        let (tx, mut rx) = mpsc::channel(8);
        let poller = GitHubPoller {
            cmd_tx: tx,
            previous_prs: Default::default(),
            stop: Arc::new(Notify::new()),
        };
        send_poller_config(&poller, vec![], String::new(), false, false);
        assert!(matches!(rx.try_recv(), Ok(PollerCmd::UpdatePaths(_))));
        assert!(matches!(rx.try_recv(), Ok(PollerCmd::SetIssueFilter(_))));
        assert!(matches!(
            rx.try_recv(),
            Ok(PollerCmd::SetPrHideDrafts(false))
        ));
        assert!(
            rx.try_recv().is_err(),
            "cold start must not send ForceResync"
        );
    }
}
