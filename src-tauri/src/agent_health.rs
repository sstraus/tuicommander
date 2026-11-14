//! Silence alarms and bounded recovery share the existing maintenance sweep.
// DEFERRED (2026-11-02) — automatic model retries require provider-owned HTTP
// status and replay-safety facts. PTY recovery snapshots expose only phase,
// attempt and deadline; resubmitting terminal input could duplicate side effects.
// The one retry of a refused managed-child turn (story 3016) is not a replay:
// it types `continue`, a new turn in which the agent decides what to redo.
use crate::{AppState, agent_stage::AgentReportView};
use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};

/// What moves when a session progresses on its terminal. Discrete events
/// without terminal output (a hook post, an accepted report) restart the
/// silence window through [`Tracker::restart`] instead.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Activity {
    epoch: u64,
    input: u64,
    output: u64,
}

struct Silence {
    activity: Activity,
    since: u64,
    flagged: bool,
}

#[derive(Default)]
pub(crate) struct Tracker {
    seen: HashMap<String, Silence>,
    stops: HashMap<String, (StopKey, u64)>,
    /// Parent mails per child whose turn check is still open (story 3015).
    /// The stop report waits for that verdict.
    delivery_checks: HashMap<String, usize>,
}

/// The report a managed child owes its parent (story 3014). Each parent mail or
/// submission opens one; a RESULT- or BLOCKED-class mail, or TUIC's own
/// `child_report`, closes it. So a parent gets at most one report per task.
/// Before that report, TUIC takes at most one automatic recovery step per
/// obligation (story 3016); a new obligation gets a new step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Obligation {
    pub(crate) opened_by: String,
    pub(crate) closed: bool,
    pub(crate) recovered: bool,
}

impl Obligation {
    pub(crate) fn open(opened_by: &str) -> Self {
        Self {
            opened_by: opened_by.into(),
            closed: false,
            recovered: false,
        }
    }
}

/// A child without a recorded opening still owes a report for its spawn task.
pub(crate) const SPAWN_OBLIGATION: &str = "spawn";
/// Quiet time before an idle or refused turn counts as a real stop: long
/// enough for consecutive turns and queued wakes to start, short enough that
/// the parent learns of the stop in seconds instead of the 30-minute stall.
const IDLE_STOP_MS: u64 = 15_000;
/// A dialog is a blocked state, not silence; it only has to persist one sweep.
const DIALOG_STOP_MS: u64 = 5_000;
/// Wait before the one automatic retry of a refused turn: a provider outage
/// or rate limit seldom clears within the 15 s idle window.
const RETRY_STOP_MS: u64 = 60_000;
/// What a refused turn is retried with: the same new turn a user would type.
const RETRY_INPUT: &str = "continue";

#[derive(Clone, Debug, PartialEq, Eq)]
struct StopKey {
    stop: &'static str,
    epoch: u64,
    input: u64,
}

impl Tracker {
    pub(crate) fn stalled(&self, pty: &str, activity: &Activity) -> bool {
        self.seen
            .get(pty)
            .is_some_and(|s| s.flagged && s.activity == *activity)
    }

    /// A hook post or an accepted report is activity with no terminal output:
    /// the silence of `pty` starts again at the next sweep. True when this
    /// ended a flagged stall, which the caller then republishes.
    pub(crate) fn restart(&mut self, pty: &str) -> bool {
        self.seen.remove(pty).is_some_and(|s| s.flagged)
    }

    fn observe(&mut self, pty: &str, activity: Activity, now: u64, delay: u64) -> bool {
        let silence = self.seen.entry(pty.into()).or_insert_with(|| Silence {
            activity: activity.clone(),
            since: now,
            flagged: false,
        });
        if silence.activity != activity {
            *silence = Silence {
                activity,
                since: now,
                flagged: false,
            };
        }
        if !silence.flagged && now.saturating_sub(silence.since) >= delay {
            silence.flagged = true;
            return true;
        }
        false
    }
}

pub(crate) fn configured_delay_ms() -> u64 {
    std::env::var("TUIC_AGENT_STALL_MINUTES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30)
        .saturating_mul(60_000)
}

/// One silence rule for every agent session (story 3050): only an agent that
/// claims to work can stall. An idle agent waits for its user, and the stop of
/// a managed child is reported by [`report_stops`]. `queued` counts: commands
/// queue behind a busy agent, the stall nudge included.
pub(crate) fn eligible(view: &AgentReportView) -> bool {
    matches!(
        view.stage.as_deref(),
        Some("working" | "queued" | "stalled")
    )
}

/// A worker that holds a BLOCKED report for its parent is waiting for it,
/// not silent, until the parent answers (story 2748). Background work is not
/// exempt: a hung command must still reach the parent, which decides the stop.
fn waiting(state: &AppState, peer: &str) -> bool {
    state.blocked_children.contains(peer)
}

pub(crate) fn activity(state: &AppState, pty: &str, epoch: u64) -> Activity {
    Activity {
        epoch,
        input: state
            .session_maps
            .last_input_ms
            .get(pty)
            .map(|t| t.load(Ordering::Acquire))
            .unwrap_or(0),
        output: state
            .session_maps
            .last_output_ms
            .get(pty)
            .map(|t| t.load(Ordering::Acquire))
            .unwrap_or(0),
    }
}

/// Every live session with the identity its rows report, managed or opened by
/// hand. A session without a registered identity is addressed by its PTY id,
/// as `live_pty_for_peer` resolves it.
fn live_sessions(state: &AppState) -> Vec<(String, String)> {
    let by_pty = crate::mcp_http::session::live_tuic_sessions_by_pty(state);
    let ptys: Vec<String> = state
        .session_maps
        .sessions
        .iter()
        .map(|s| s.key().clone())
        .collect();
    ptys.into_iter()
        .map(|pty| {
            (
                by_pty.get(&pty).cloned().unwrap_or_else(|| pty.clone()),
                pty,
            )
        })
        .collect()
}

fn managed_children(state: &AppState) -> Vec<(String, String)> {
    state
        .peer_agents
        .iter()
        .filter_map(|p| {
            let peer = p.key().clone();
            let pty = state.live_pty_for_peer(&peer)?;
            state
                .session_maps
                .session_parent
                .contains_key(&pty)
                .then_some((peer, pty))
        })
        .collect()
}

pub(crate) fn sweep(state: &Arc<AppState>, now: u64, delay: u64) {
    let peers = live_sessions(state);
    let live: std::collections::HashSet<_> = peers.iter().map(|(_, pty)| pty.as_str()).collect();
    state
        .agent_reports
        .health
        .lock()
        .seen
        .retain(|pty, _| delay > 0 && live.contains(pty.as_str()));
    if delay == 0 {
        return;
    }
    for (peer, pty) in peers {
        if state.session_maps.exit_codes.contains_key(&pty) {
            continue;
        }
        let snapshot = state.session_state_with_shell(&pty);
        let view = state
            .agent_reports
            .view(state, &pty, Some(&peer), snapshot.as_ref());
        if !eligible(&view) || waiting(state, &peer) {
            state.agent_reports.health.lock().seen.remove(&pty);
            continue;
        }
        // Never block the maintenance sweep behind an in-flight submission.
        let transaction = state.input_transaction(&pty);
        let Some(_input) = transaction.try_lock() else {
            continue;
        };
        let epoch = |s: Option<&crate::state::SessionState>| s.map_or(0, |s| s.turn_epoch);
        let key = activity(state, &pty, epoch(snapshot.as_ref()));
        let newly_stalled =
            state
                .agent_reports
                .health
                .lock()
                .observe(&pty, key.clone(), now, delay);
        if newly_stalled {
            let current = state.session_state_with_shell(&pty);
            let current_view = state
                .agent_reports
                .view(state, &pty, Some(&peer), current.as_ref());
            if state.live_pty_for_peer(&peer).as_deref() != Some(pty.as_str())
                || !eligible(&current_view)
                || waiting(state, &peer)
                || activity(state, &pty, epoch(current.as_ref())) != key
            {
                continue;
            }
            // Every client renders the stall from the published session state.
            state.republish_session_state(&pty);
            // Parent mail and the nudge belong to managed children only.
            if !state.session_maps.session_parent.contains_key(&pty) {
                continue;
            }
            let reported = state.reported_children.contains(&peer);
            // Recovery never stops the PTY: silence is not proof of a hang, and
            // a stop loses work in progress. Nudge the worker, tell the parent,
            // and leave the stop to an explicit parent decision.
            crate::pty::push_state_change_to_parent(
                state,
                &pty,
                serde_json::json!({
                    "type":"state_change", "state":"stalled", "reason":"no_report_or_output",
                    "session_id":pty, "tuic_session":peer, "threshold_ms":delay,
                    "recovery":if reported { "parent_decides" } else { "nudged_worker" },
                    "background_work":current.as_ref().is_some_and(|s| s.background_work),
                    "stop":format!("session action=kill session_id={pty}"),
                }),
            );
            // A worker that already reported RESULT would only resend it.
            if !reported {
                crate::pty::nudge_stalled_pty(state, &pty, &peer, delay);
            }
        }
    }
}

/// The real-stop state of one managed child, or `None` while it works, waits on
/// background work (`agent_state` is then `working`) or has queued input.
fn stop_key(state: &AppState, pty: &str) -> Option<StopKey> {
    let snapshot = state.session_state_with_shell(pty)?;
    let stop = match snapshot.agent_state.as_deref()? {
        "awaiting_input" => "blocked_dialog",
        "error" => "error",
        "idle" | "completed" => "ended_without_result",
        _ => return None,
    };
    if crate::pty::queued_command_count(state, pty) > 0 {
        return None;
    }
    // Output is not activity here: a user status line or a waiting dialog
    // repaints by itself. A new turn moves the agent state, epoch or input.
    Some(StopKey {
        stop,
        epoch: snapshot.turn_epoch,
        input: state
            .session_maps
            .last_input_ms
            .get(pty)
            .map(|t| t.load(Ordering::Acquire))
            .unwrap_or(0),
    })
}

/// Send one typed `child_report` for each managed child that stopped while it
/// still owes its parent a report (story 3014). Runs on the maintenance sweep;
/// `processes` is the machine process list, read only for a due idle stop.
pub(crate) fn report_stops(
    state: &Arc<AppState>,
    now: u64,
    processes: &mut dyn FnMut() -> Option<Vec<crate::pty::ProcessTreeEntry>>,
) {
    let children = managed_children(state);
    state
        .agent_reports
        .health
        .lock()
        .stops
        .retain(|pty, _| children.iter().any(|(_, live)| live == pty));
    for (peer, pty) in children {
        let key = (!state.session_maps.exit_codes.contains_key(&pty))
            .then(|| stop_key(state, &pty))
            .flatten();
        let since = {
            let mut tracker = state.agent_reports.health.lock();
            let Some(key) = key.clone() else {
                tracker.stops.remove(&pty);
                continue;
            };
            let entry = tracker
                .stops
                .entry(pty.clone())
                .or_insert((key.clone(), now));
            if entry.0 != key {
                *entry = (key, now);
            }
            entry.1
        };
        let Some(key) = key else { continue };
        let step = match key.stop {
            "ended_without_result" => Some("reprompt"),
            "error" => Some("retry"),
            _ => None,
        }
        .filter(|_| recovery_available(state, &peer));
        let delay = match (key.stop, step) {
            ("blocked_dialog", _) => DIALOG_STOP_MS,
            ("error", Some(_)) => RETRY_STOP_MS,
            _ => IDLE_STOP_MS,
        };
        if now.saturating_sub(since) < delay
            || !obligation_open(state, &peer)
            || state
                .agent_reports
                .health
                .lock()
                .delivery_checks
                .contains_key(&peer)
        {
            continue;
        }
        // A child that ended its turn to wait on its own `tuic bg` job or on a
        // background command of the agent (a hooked agent reads idle then) has
        // not stopped. An unreadable process list cannot prove that none runs.
        // DEFERRED (2026-11-13) — the shared helper rule ignores a descendant
        // that starts within 60 s of the agent, so a background command started
        // in the first minute still reads as a stop (seen live: Claude
        // run_in_background 8 s after launch). The same rule keeps MCP helpers
        // from holding every child busy; a turn-relative rule needs evidence.
        if key.stop == "ended_without_result"
            && (crate::idle_close::bg_wake_blocks_close(&peer)
                || processes().is_none_or(|processes| {
                    let agent = state
                        .session_maps
                        .session_states
                        .get(&pty)
                        .and_then(|s| s.agent_type.clone())
                        .unwrap_or_default();
                    crate::pty::sleep_work_in_snapshot(state, &pty, &peer, &agent, &processes)
                }))
        {
            continue;
        }
        if stop_key(state, &pty).as_ref() != Some(&key) {
            continue;
        }
        let snapshot = state
            .session_maps
            .session_states
            .get(&pty)
            .map(|s| s.clone());
        let detail = match key.stop {
            "blocked_dialog" => serde_json::json!({
                "on": snapshot.as_ref().and_then(|s| s.question_text.clone()),
            }),
            "error" => serde_json::json!({
                "reason": snapshot.as_ref().and_then(|s| s.last_error.clone()),
            }),
            _ => serde_json::json!({}),
        };
        let Some(step) = step else {
            send_child_report(state, &peer, &pty, key.stop, detail, None);
            continue;
        };
        // The step restarts the stop window: a step that never reached the
        // composer is a second failure one window later, not a loop.
        state
            .agent_reports
            .health
            .lock()
            .stops
            .insert(pty.clone(), (key.clone(), now));
        send_child_report(state, &peer, &pty, key.stop, detail, Some(step));
        if step == "reprompt" {
            crate::pty::nudge_pty(
                state,
                &pty,
                &peer,
                serde_json::json!({ "type": "report_reprompt", "session_id": pty }),
                "[TUIC] Your turn ended without a RESULT or BLOCKED report to your parent. Continue the task, or send that report now.",
            );
        } else {
            retry_refused_turn(state, &pty);
        }
    }
}

fn obligation_open(state: &AppState, peer: &str) -> bool {
    state
        .child_obligations
        .get(peer)
        .is_none_or(|obligation| !obligation.closed)
}

fn recovery_available(state: &AppState, peer: &str) -> bool {
    state
        .child_obligations
        .get(peer)
        .is_none_or(|obligation| !obligation.recovered)
}

/// Type one `continue` into a child whose turn was refused, through the agent
/// writer that the provider-recovery retry uses. Only while the refusal still
/// holds an idle, empty composer: never over a new turn or a user's draft.
fn retry_refused_turn(state: &Arc<AppState>, pty: &str) {
    let state = Arc::clone(state);
    let pty = pty.to_owned();
    crate::pty::spawn_injection_job(move || {
        let transaction = state.input_transaction(&pty);
        let _input = transaction.lock();
        let refused = state
            .session_maps
            .session_states
            .get(&pty)
            .is_some_and(|s| s.agent_error);
        let idle = state
            .session_maps
            .shell_states
            .get(&pty)
            .is_some_and(|s| s.load(Ordering::Acquire) == crate::pty::SHELL_IDLE);
        let drafting = state
            .session_maps
            .input_buffers
            .get(&pty)
            .is_some_and(|buffer| !buffer.lock().content().is_empty());
        if !refused || !idle || drafting {
            return;
        }
        match crate::pty::write_agent_command_to_pty(&state, &pty, RETRY_INPUT) {
            // Recording the line clears the refusal mark and starts the turn.
            Ok(()) => crate::pty::record_submitted_line(&state, &pty, RETRY_INPUT.into(), -1),
            Err(error) => tracing::warn!(session_id = %pty, %error, "child retry not typed"),
        }
    });
}

/// Report a managed agent child that exited by itself while it owed a report.
/// An exit that TUIC caused (kill, close, idle close, suspend) has a recorded
/// reason, and the parent already knows of it.
pub(crate) fn report_exit(state: &Arc<AppState>, pty: &str, exit_code: Option<i32>) {
    let agent = state
        .session_maps
        .session_states
        .get(pty)
        .is_some_and(|s| s.agent_type.is_some());
    if agent
        && state.session_maps.session_parent.contains_key(pty)
        && !state.session_maps.exit_reasons.contains_key(pty)
    {
        send_child_report(
            state,
            pty,
            pty,
            "dead",
            serde_json::json!({ "exit_code": exit_code }),
            None,
        );
    }
    state.child_obligations.remove(pty);
}

/// The one producer of `child_report` mail. Closing the obligation first makes
/// the report at most one per task, whichever stop reaches it. With a recovery
/// `step`, the mail is a notice of that step instead: the obligation stays open
/// and records the step, so the next stop is the report.
fn send_child_report(
    state: &Arc<AppState>,
    peer: &str,
    pty: &str,
    stop: &str,
    detail: serde_json::Value,
    step: Option<&str>,
) {
    let (opened_by, recovery) = {
        let mut obligation = state
            .child_obligations
            .entry(peer.to_owned())
            .or_insert_with(|| Obligation::open(SPAWN_OBLIGATION));
        if obligation.closed {
            return;
        }
        let recovery = match step {
            Some(step) => {
                obligation.recovered = true;
                step
            }
            None => {
                obligation.closed = true;
                if obligation.recovered {
                    "exhausted"
                } else {
                    "none"
                }
            }
        };
        (obligation.opened_by.clone(), recovery)
    };
    let agent = state
        .session_maps
        .session_states
        .get(pty)
        .and_then(|s| s.agent_type.clone());
    let resume_id = state
        .suspend_metadata
        .get(pty)
        .and_then(|m| m["agentSessionId"].as_str().map(str::to_owned));
    crate::pty::push_state_change_to_parent(
        state,
        pty,
        serde_json::json!({
            "type": "child_report", "state": stop,
            "session_id": pty, "tuic_session": peer,
            "name": state.peer_agents.get(peer).map(|p| p.name.clone()),
            "obligation_open": true, "obligation": opened_by,
            "recovery": recovery,
            "detail": detail,
            "last_text": last_text(state, pty),
            "resume": { "agent": agent, "id": resume_id, "tuic_session": peer },
        }),
    );
}

const LAST_TEXT_BYTES: usize = 2048;

/// The agent's last screen text above its input box, at most 2 KB. The status
/// line below the input box is user configuration, not agent output.
fn last_text(state: &AppState, pty: &str) -> String {
    let Some(rows) = state
        .grid
        .vt_log_buffers
        .get(pty)
        .map(|vt| vt.lock().screen_rows())
    else {
        return String::new();
    };
    let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
    let end = crate::chrome::find_chrome_cutoff(&refs).unwrap_or(rows.len());
    let text = rows[..end].join("\n");
    let text = text.trim_end();
    let mut start = text.len().saturating_sub(LAST_TEXT_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].trim_start_matches('\n').to_owned()
}

/// How long a parent mail to an idle managed child may wait for the child's
/// turn before the parent learns that it did not start one (story 3015).
pub(crate) const MAIL_TURN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

/// One mail from a parent to its idle managed child, waiting for its verdict.
pub(crate) struct MailDelivery {
    pub(crate) peer: String,
    pub(crate) pty: String,
    pub(crate) message_id: String,
    pub(crate) message_timestamp: u64,
    /// Output-ring position before the wake, the busy-edge boundary.
    pub(crate) offset: u64,
}

/// Arm one timer for this mail. A child that starts no turn and reads no mail
/// before `deadline` makes its parent receive one `delivery_failed` notice. The
/// stop report for the child waits for this verdict, so the parent does not
/// read a delivery failure as a finished task.
pub(crate) fn watch_mail_delivery(
    state: &Arc<AppState>,
    delivery: MailDelivery,
    deadline: std::time::Duration,
) -> bool {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return false;
    };
    *state
        .agent_reports
        .health
        .lock()
        .delivery_checks
        .entry(delivery.peer.clone())
        .or_default() += 1;
    let state = Arc::clone(state);
    runtime.spawn(async move {
        tokio::time::sleep(deadline).await;
        settle_mail_delivery(&state, &delivery);
    });
    true
}

fn settle_mail_delivery(state: &Arc<AppState>, delivery: &MailDelivery) {
    {
        let mut tracker = state.agent_reports.health.lock();
        if let Some(count) = tracker.delivery_checks.get_mut(&delivery.peer) {
            *count -= 1;
            if *count == 0 {
                tracker.delivery_checks.remove(&delivery.peer);
            }
        }
    }
    let cursor = state
        .agent_read_cursor
        .get(&delivery.peer)
        .map(|cursor| *cursor)
        .unwrap_or(0);
    if cursor >= delivery.message_timestamp
        || crate::pty::agent_turn_started_after(state, &delivery.pty, delivery.offset)
    {
        return;
    }
    let reason = crate::pty::mail_wake_failure_reason(state, &delivery.pty);
    let pending = state.agent_inbox.get(&delivery.peer).map_or(0, |inbox| {
        inbox.iter().filter(|m| m.timestamp > cursor).count()
    });
    // The child never took this task, so it owes no report for it: the
    // delivery failure replaces the stop report.
    if let Some(mut obligation) = state.child_obligations.get_mut(&delivery.peer)
        && obligation.opened_by == delivery.message_id
    {
        obligation.closed = true;
    }
    crate::pty::push_state_change_to_parent(
        state,
        &delivery.pty,
        serde_json::json!({
            "type": "delivery_failed", "reason": reason,
            "message_id": delivery.message_id, "pending": pending,
            "session_id": delivery.pty, "tuic_session": delivery.peer,
            "name": state.peer_agents.get(&delivery.peer).map(|p| p.name.clone()),
        }),
    );
}
