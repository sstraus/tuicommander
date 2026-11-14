//! Delayed cleanup for managed agent terminals.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::AppState;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    state: String,
    turn_epoch: u64,
    last_input_ms: u64,
    last_output_ms: u64,
    last_mail_id: Option<String>,
    read_cursor: u64,
}

#[derive(Default)]
pub(crate) struct IdleCloseTracker {
    seen: HashMap<String, (Observation, u64)>,
    hibernate_seen: HashMap<String, (Observation, u64)>,
    awake_ms: u64,
}

impl IdleCloseTracker {
    fn observe(
        &mut self,
        session_id: &str,
        now_ms: u64,
        delay_ms: u64,
        observation: Option<Observation>,
    ) -> bool {
        let Some(observation) = observation.filter(|_| delay_ms > 0) else {
            self.seen.remove(session_id);
            return false;
        };
        match self.seen.get_mut(session_id) {
            Some((old, since)) if *old == observation => now_ms.saturating_sub(*since) >= delay_ms,
            Some(entry) => {
                *entry = (observation, now_ms);
                false
            }
            None => {
                self.seen
                    .insert(session_id.to_owned(), (observation, now_ms));
                false
            }
        }
    }
}

pub(crate) fn live_bg_runner_for_session(commands: &[String], session_id: &str) -> bool {
    commands.iter().any(|command| {
        let mut words = command.split_whitespace();
        words.any(|word| word == "__bg-runner")
            && command
                .split_whitespace()
                .take_while(|word| *word != "--")
                .any(|word| word == session_id)
    })
}

pub(crate) fn wake_marker_path(session_id: &str) -> std::path::PathBuf {
    crate::config::config_dir()
        .join("bg-wakes")
        .join(format!("{session_id}.json"))
}

pub(crate) fn bg_wake_blocks_close(session_id: &str) -> bool {
    let path = wake_marker_path(session_id);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => {
            tracing::warn!(%session_id, %error, "idle-close cannot read bg wake marker");
            return true;
        }
    };
    let Ok(marker) = serde_json::from_str::<serde_json::Value>(&content) else {
        tracing::warn!(%session_id, "idle-close found malformed bg wake marker");
        return true;
    };
    if marker["session_id"].as_str() != Some(session_id) {
        tracing::warn!(%session_id, "idle-close found mismatched bg wake marker");
        return true;
    }
    matches!(
        marker["status"].as_str(),
        Some("failed" | "retrying" | "uncertain")
    )
}

/// Nobody can answer a child whose parent has no PTY and no MCP session. The
/// hold is kept meanwhile: a parent that reconnects still owes its answer.
fn parent_alive(state: &AppState, parent: &str) -> bool {
    state.session_maps.sessions.contains_key(parent)
        || state
            .mcp
            .session_to_mcp
            .get(parent)
            .is_some_and(|sessions| !sessions.is_empty())
}

fn observation(state: &AppState, session_id: &str) -> Option<(Observation, u64)> {
    let parent = state.session_maps.session_parent.get(session_id)?.clone();
    if !state.session_maps.sessions.contains_key(session_id)
        || crate::mcp_http::mcp_transport::is_pending_parent(&parent)
        || state.keep_open_sessions.contains(session_id)
        || (state.blocked_children.contains(session_id) && parent_alive(state, &parent))
    {
        return None;
    }
    let snapshot = state.session_state_with_shell(session_id)?;
    if !matches!(snapshot.agent_state.as_deref(), Some("idle" | "completed"))
        || snapshot.background_work
    {
        return None;
    }
    let cursor = state
        .agent_read_cursor
        .get(session_id)
        .map(|v| *v)
        .unwrap_or(0);
    let inbox = state.agent_inbox.get(session_id);
    if inbox
        .as_ref()
        .is_some_and(|mail| mail.iter().any(|message| message.timestamp > cursor))
        || bg_wake_blocks_close(session_id)
    {
        return None;
    }
    let delay_minutes = crate::config::load_agents_config()
        .agents
        .get(snapshot.agent_type.as_deref()?)
        .map(|settings| settings.idle_close_minutes)
        .unwrap_or(crate::config::DEFAULT_IDLE_CLOSE_MINUTES);
    Some((
        Observation {
            state: snapshot.agent_state?,
            turn_epoch: snapshot.turn_epoch,
            last_input_ms: state
                .session_maps
                .last_input_ms
                .get(session_id)
                .map(|v| v.load(Ordering::Relaxed))
                .unwrap_or(0),
            last_output_ms: state
                .session_maps
                .last_output_ms
                .get(session_id)
                .map(|v| v.load(Ordering::Relaxed))
                .unwrap_or(0),
            last_mail_id: inbox
                .as_ref()
                .and_then(|mail| mail.back().map(|message| message.id.clone())),
            read_cursor: cursor,
        },
        u64::from(delay_minutes).saturating_mul(60_000),
    ))
}

fn sweep_with_snapshot(
    state: &Arc<AppState>,
    tracker: &mut IdleCloseTracker,
    now_ms: u64,
    mut snapshot: impl FnMut() -> Option<Vec<String>>,
) {
    crate::agent_health::sweep(state, now_ms, crate::agent_health::configured_delay_ms());
    let mut processes = None;
    crate::agent_health::report_stops(state, now_ms, &mut || {
        processes
            .get_or_insert_with(crate::pty::process_tree_snapshot)
            .clone()
    });
    sweep_abandoned_shells(state);
    sweep_hibernation(state, tracker, now_ms, &mut snapshot);
    let children: Vec<String> = state
        .session_maps
        .session_parent
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    tracker
        .seen
        .retain(|session_id, _| children.contains(session_id));
    state
        .blocked_children
        .retain(|session_id| children.contains(session_id));
    state
        .reported_children
        .retain(|session_id| children.contains(session_id));
    state
        .child_obligations
        .retain(|session_id, _| children.contains(session_id));
    let mut runner_commands: Option<Option<Vec<String>>> = None;
    for session_id in children {
        let candidate = observation(state, &session_id);
        let (observed, delay) = match candidate {
            Some(value) => value,
            None => {
                tracker.observe(&session_id, now_ms, 0, None);
                continue;
            }
        };
        if !tracker.observe(&session_id, now_ms, delay, Some(observed.clone())) {
            continue;
        }
        // Cheap state may have changed while the timer matured; avoid a process
        // scan unless this child still qualifies for closure.
        if observation(state, &session_id)
            .as_ref()
            .is_none_or(|(current, _)| current != &observed)
        {
            continue;
        }
        let Some(commands) = runner_commands.get_or_insert_with(&mut snapshot).as_ref() else {
            // A failed inventory cannot establish that no detached runner exists.
            continue;
        };
        if live_bg_runner_for_session(commands, &session_id) {
            tracker.seen.remove(&session_id);
            continue;
        }
        // Recheck mail, output, marker and state after the process inventory.
        if observation(state, &session_id)
            .as_ref()
            .is_none_or(|(current, _)| current != &observed)
        {
            continue;
        }
        let name = state
            .peer_agents
            .get(&session_id)
            .map(|peer| peer.name.clone())
            .unwrap_or_else(|| session_id.clone());
        crate::pty::push_state_change_to_parent(
            state,
            &session_id,
            serde_json::json!({
                "type": "state_change", "state": "closed", "reason": "idle_timeout",
                "session_id": session_id, "name": name,
            }),
        );
        crate::mcp_http::mcp_transport::close_idle_managed_session(state, &session_id);
        tracker.seen.remove(&session_id);
    }
}

pub(crate) fn shell_last_io_ms(state: &AppState, id: &str) -> Option<u64> {
    let output = state
        .session_maps
        .last_output_ms
        .get(id)?
        .load(Ordering::Acquire);
    let input = state
        .session_maps
        .last_input_ms
        .get(id)
        .map(|v| v.load(Ordering::Acquire))
        .unwrap_or(0);
    let activity = state.session_maps.session_states.get(id)?.last_activity_ms;
    let last = output.max(input).max(activity);
    (last > 0).then_some(last)
}

fn shell_candidate(state: &AppState, id: &str) -> Option<(u32, u64)> {
    if state.session_maps.session_parent.contains_key(id)
        || state.peer_agents.contains_key(id)
        || state.keep_open_sessions.contains(id)
        || state
            .session_maps
            .session_parent
            .iter()
            .any(|p| p.value() == id)
        || crate::pty::queued_command_count(state, id) > 0
        || bg_wake_blocks_close(id)
        || state
            .session_maps
            .input_buffers
            .get(id)
            .is_some_and(|b| !b.lock().is_empty())
    {
        return None;
    }
    // Managed identities can differ from the PTY UUID. Both ownership and
    // children must be checked against the live binding, not just the tab key.
    if state
        .session_maps
        .live_pty_by_tuic_session
        .iter()
        .any(|binding| {
            binding.value() == id
                && (state.peer_agents.contains_key(binding.key())
                    || state
                        .session_maps
                        .session_parent
                        .iter()
                        .any(|p| p.value() == binding.key()))
        })
    {
        return None;
    }
    let ss = state.session_state_with_shell(id)?;
    if ss.agent_type.is_some()
        || ss.background_work
        || ss.awaiting_input
        || ss.has_pending_background_probe()
        || ss.shell_state.as_deref() != Some("idle")
    {
        return None;
    }
    let pid = state
        .session_maps
        .sessions
        .get(id)?
        .lock()
        ._child
        .process_id()?;
    Some((pid, shell_last_io_ms(state, id)?))
}

fn sweep_abandoned_shells(state: &Arc<AppState>) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    sweep_abandoned_shells_at(
        state,
        now_ms,
        crate::config::load_agents_config().abandoned_shell_hours,
        crate::pty::process_tree_snapshot,
    );
}

fn sweep_abandoned_shells_at(
    state: &Arc<AppState>,
    now_ms: u64,
    hours: u16,
    mut inventory: impl FnMut() -> Option<Vec<crate::pty::ProcessTreeEntry>>,
) {
    let ids: Vec<String> = state
        .session_maps
        .sessions
        .iter()
        .map(|s| s.key().clone())
        .collect();
    let mut processes = None;
    for id in ids {
        let candidate = shell_candidate(state, &id)
            .filter(|(_, last)| now_ms.saturating_sub(*last) >= 3_600_000);
        let eligible = candidate
            .filter(|(pid, _)| {
                processes
                    .get_or_insert_with(&mut inventory)
                    .as_ref()
                    .is_some_and(|ps| {
                        crate::pty::abandoned_shell_process(*pid, ps)
                            && !live_bg_runner_for_session(
                                &ps.iter().map(|p| p.command.clone()).collect::<Vec<_>>(),
                                &id,
                            )
                    })
            })
            .filter(|observed| shell_candidate(state, &id).as_ref() == Some(observed));
        let since_ms = eligible.map(|(_, last)| last);
        let previous = state
            .session_maps
            .session_states
            .get(&id)
            .and_then(|s| s.abandoned_since_ms);
        if previous != since_ms {
            state.emit_pty_event(crate::state::AppEvent::ShellAbandoned {
                session_id: id.clone(),
                since_ms,
            });
        }
        if let Some((pid, last)) = eligible
            && hours > 0 && now_ms.saturating_sub(last) >= u64::from(hours) * 3_600_000
            // A second fresh observation at the destructive boundary. A failed
            // OS inventory, new child, queued input or activity cancels closure.
            && inventory().is_some_and(|ps| crate::pty::abandoned_shell_process(pid, &ps)
                && !live_bg_runner_for_session(&ps.iter().map(|p| p.command.clone()).collect::<Vec<_>>(), &id))
            && shell_candidate(state, &id) == Some((pid, last))
        {
            crate::mcp_http::mcp_transport::close_idle_managed_session(state, &id);
        }
    }
}

/// Share the idle-close observation and ready-state refusal contract, but never
/// apply child teardown to a top-level tab. A scan/storage error fails closed.
fn hibernate_observation(state: &AppState, id: &str) -> Option<Observation> {
    let identity = crate::mcp_http::session::live_tuic_sessions_by_pty(state)
        .remove(id)
        .unwrap_or_else(|| id.into());
    if state.session_maps.session_parent.contains_key(id)
        || state.keep_open_sessions.contains(id)
        || state.keep_open_sessions.contains(&identity)
        || state.background_visibility.lock().watched(state, id)
    {
        return None;
    }
    let ss = state.session_state_with_shell(id)?;
    if !matches!(
        ss.agent_type.as_deref(),
        Some(
            "claude"
                | "codex"
                | "gemini"
                | "grok"
                | "ego"
                | "goose"
                | "amp"
                | "cursor"
                | "opencode"
                | "pi"
        )
    ) || !matches!(ss.agent_state.as_deref(), Some("idle" | "completed"))
        || ss.has_pending_background_probe()
        || crate::mcp_http::mcp_transport_session_agent::suspend_refusal(&ss).is_some()
    {
        return None;
    }
    let cursor = state
        .agent_read_cursor
        .get(&identity)
        .map(|v| *v)
        .unwrap_or(0);
    let inbox = state.agent_inbox.get(&identity);
    if inbox
        .as_ref()
        .is_some_and(|mail| mail.iter().any(|m| m.timestamp > cursor))
        || bg_wake_blocks_close(&identity)
    {
        return None;
    }
    Some(Observation {
        state: ss.agent_state?,
        turn_epoch: ss.turn_epoch,
        last_input_ms: state
            .session_maps
            .last_input_ms
            .get(id)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0),
        last_output_ms: state
            .session_maps
            .last_output_ms
            .get(id)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0),
        last_mail_id: inbox
            .as_ref()
            .and_then(|mail| mail.back().map(|m| m.id.clone())),
        read_cursor: cursor,
    })
}

pub(crate) fn hibernate_allowed(state: &AppState, id: &str) -> bool {
    hibernate_observation(state, id).is_some()
        && crate::automations::store::RunStore::open()
            .and_then(|store| store.open_runs())
            .is_ok_and(|runs| !runs.iter().any(|run| run.session_id.as_deref() == Some(id)))
}

fn add_awake_time(current: u64, gap: std::time::Duration) -> u64 {
    if gap <= crate::cpu_watchdog::SLEEP_WAKE_GAP {
        current.saturating_add(gap.as_millis() as u64)
    } else {
        current
    }
}

fn sweep_hibernation(
    state: &Arc<AppState>,
    tracker: &mut IdleCloseTracker,
    now_ms: u64,
    snapshot: &mut impl FnMut() -> Option<Vec<String>>,
) {
    let config = crate::config::load_agents_config();
    if config.hibernate_idle_minutes == 0
        || (config.hibernate_scope == crate::config::HibernateScope::BackgroundOnly
            && !crate::background_mode::hidden::is_hidden(state))
    {
        tracker.hibernate_seen.clear();
        return;
    }
    let Ok(runs) = crate::automations::store::RunStore::open().and_then(|store| store.open_runs())
    else {
        return;
    };
    let bound: std::collections::HashSet<_> = runs
        .iter()
        .filter_map(|run| run.session_id.as_deref())
        .collect();
    let ids: Vec<_> = state
        .session_maps
        .sessions
        .iter()
        .map(|s| s.key().clone())
        .collect();
    tracker.hibernate_seen.retain(|id, _| ids.contains(id));
    let delay = u64::from(config.hibernate_idle_minutes).saturating_mul(60_000);
    let mut commands = None;
    for id in ids {
        let observation = (!bound.contains(id.as_str()))
            .then(|| hibernate_observation(state, &id))
            .flatten();
        let Some(observed) = observation else {
            tracker.hibernate_seen.remove(&id);
            continue;
        };
        let due = match tracker.hibernate_seen.get_mut(&id) {
            Some((old, since)) if *old == observed => now_ms.saturating_sub(*since) >= delay,
            Some(entry) => {
                *entry = (observed.clone(), now_ms);
                false
            }
            None => {
                tracker
                    .hibernate_seen
                    .insert(id.clone(), (observed.clone(), now_ms));
                false
            }
        };
        if !due {
            continue;
        }
        let Some(commands) = commands.get_or_insert_with(&mut *snapshot).as_ref() else {
            continue;
        };
        let identity = crate::mcp_http::session::live_tuic_sessions_by_pty(state)
            .remove(&id)
            .unwrap_or_else(|| id.clone());
        if live_bg_runner_for_session(commands, &identity)
            || hibernate_observation(state, &id).as_ref() != Some(&observed)
        {
            tracker.hibernate_seen.remove(&id);
            continue;
        }
        match crate::suspended_sessions::suspend_with_reason(
            state,
            &id,
            &serde_json::Value::Null,
            "idle_hibernate",
        ) {
            Ok(_) => tracing::info!(session_id = %id, "Idle agent hibernated"),
            Err(error) => tracing::debug!(session_id = %id, %error, "Idle agent cannot hibernate"),
        }
        tracker.hibernate_seen.remove(&id);
    }
}

#[cfg(all(test, unix))]
fn sweep_with_commands(
    state: &Arc<AppState>,
    tracker: &mut IdleCloseTracker,
    now_ms: u64,
    runner_commands: &[String],
) {
    sweep_with_snapshot(state, tracker, now_ms, || Some(runner_commands.to_vec()));
}

async fn sweep_step(
    state: Arc<AppState>,
    mut tracker: IdleCloseTracker,
    now_ms: u64,
    snapshot: impl FnMut() -> Option<Vec<String>> + Send + 'static,
) -> IdleCloseTracker {
    match tokio::task::spawn_blocking(move || {
        sweep_with_snapshot(&state, &mut tracker, now_ms, snapshot);
        tracker
    })
    .await
    {
        Ok(tracker) => tracker,
        Err(error) => {
            tracing::error!("idle-close sweep failed: {error}");
            IdleCloseTracker::default()
        }
    }
}

pub(crate) fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut tracker = IdleCloseTracker::default();
        let mut previous = std::time::Instant::now();
        loop {
            interval.tick().await;
            let now = std::time::Instant::now();
            let gap = now.duration_since(previous);
            previous = now;
            tracker.awake_ms = add_awake_time(tracker.awake_ms, gap);
            let now_ms = tracker.awake_ms;
            tracker = sweep_step(
                Arc::clone(&state),
                tracker,
                now_ms,
                crate::pty::live_bg_runner_commands,
            )
            .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a silent live worker is never flagged, or each sweep repeats its mail.
    #[cfg(unix)]
    #[tokio::test]
    async fn working_agent_past_threshold_is_flagged_once() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().to_path_buf());
        let fixture = stalled_recovery_fixture();
        make_working(&fixture.state, &fixture.id);
        let state = &fixture.state;
        let id = &fixture.id;
        state.keep_open_sessions.insert(id.clone());
        // This test covers the stall alarm only: the child owes no report, so
        // no `child_report` joins the parent mail counted below.
        state.child_obligations.insert(
            id.clone(),
            crate::agent_health::Obligation {
                opened_by: "spawn".into(),
                closed: true,
                recovered: false,
            },
        );
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(state, &mut tracker, 0, &[]);
        sweep_with_commands(state, &mut tracker, 1_799_999, &[]);
        assert!(!state.agent_inbox.contains_key("coordinator"));
        // Fresh output starts a new silence window before recovery is due.
        state
            .session_maps
            .last_output_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(17));
        sweep_with_commands(state, &mut tracker, 1_800_000, &[]);
        sweep_with_commands(state, &mut tracker, 3_599_999, &[]);
        assert!(state.session_maps.sessions.contains_key(id));
        let accepted = crate::agent_stage::Reports::prepare(
            state,
            id,
            crate::agent_report::AgentReport {
                state: crate::agent_report::ReportState::Waiting,
                wait_for: Some(crate::agent_report::WaitFor::Dependency),
                question: None,
            },
        );
        state.agent_reports.commit(id, "declared-wait", accepted);
        sweep_with_commands(state, &mut tracker, 7_200_000, &[]);
        assert!(
            !state.agent_inbox.contains_key("coordinator"),
            "declared dependency wait is not silence"
        );
        assert!(state.session_maps.sessions.contains_key(id));
        // A new submitted turn invalidates the old wait report.
        state
            .session_maps
            .session_states
            .get_mut(id)
            .unwrap()
            .turn_epoch += 1;
        sweep_with_commands(state, &mut tracker, 7_200_001, &[]);
        sweep_with_commands(state, &mut tracker, 9_000_001, &[]);
        sweep_with_commands(state, &mut tracker, 10_800_001, &[]);
        let mail: Vec<serde_json::Value> = state
            .agent_inbox
            .get("coordinator")
            .unwrap()
            .iter()
            .filter_map(|m| serde_json::from_str(&m.content).ok())
            .collect();
        assert_eq!(mail.iter().filter(|m| m["state"] == "stalled").count(), 1);
        assert!(
            state.session_maps.sessions.contains_key(id),
            "only the parent may stop a stalled worker"
        );
        let subscribed = state.mail_subscriptions.action(&crate::config::config_dir(), "coordinator",
            &serde_json::json!({"action":"subscribe", "subscription_id":"stall-alarm", "filter":"actionable"}));
        assert!(subscribed.get("error").is_none(), "{subscribed}");
        let actionable = state.mail_subscriptions.action(
            &crate::config::config_dir(),
            "coordinator",
            &serde_json::json!({"action":"inbox", "subscription_id":"stall-alarm"}),
        );
        assert_eq!(
            actionable["count"], 1,
            "the stall notice reaches subscribers: {actionable}"
        );
        let failure = state.mail_subscriptions.action(&crate::config::config_dir(), "coordinator",
            &serde_json::json!({"action":"subscribe", "subscription_id":"stall-failure", "filter":"failure"}));
        assert!(failure.get("error").is_none(), "{failure}");
        let failures = state.mail_subscriptions.action(
            &crate::config::config_dir(),
            "coordinator",
            &serde_json::json!({"action":"inbox", "subscription_id":"stall-failure"}),
        );
        assert_eq!(
            failures["count"], 1,
            "the stall notice must reach failure subscribers: {failures}"
        );
    }

    // Catches: recovery stops a silent worker as its first action instead of
    // nudging it and leaving the stop to its parent (2748: five agents lost).
    #[cfg(unix)]
    #[tokio::test]
    async fn stalled_worker_is_nudged_and_parent_decides_the_stop() {
        let fixture = stalled_recovery_fixture();
        make_working(&fixture.state, &fixture.id);
        let state = &fixture.state;
        crate::agent_health::sweep(state, 0, 100);
        crate::agent_health::sweep(state, 99, 100);
        assert_eq!(stall_notices(state), 0);
        crate::agent_health::sweep(state, 100, 100);
        crate::agent_health::sweep(state, 150, 100);
        {
            let entry = state.session_maps.sessions.get(&fixture.id).unwrap();
            let mut session = entry.lock();
            assert!(
                session._child.try_wait().unwrap().is_none(),
                "a stalled worker must keep running until its parent stops it"
            );
        }
        let inbox = state.agent_inbox.get("coordinator").unwrap();
        let notices: Vec<serde_json::Value> = inbox
            .iter()
            .filter_map(|m| serde_json::from_str(&m.content).ok())
            .filter(|m: &serde_json::Value| m["state"] == "stalled")
            .collect();
        assert_eq!(notices.len(), 1, "one parent notice per silence episode");
        assert_eq!(notices[0]["recovery"], "nudged_worker");
        assert!(
            notices[0]["stop"]
                .as_str()
                .is_some_and(|stop| stop.contains("action=kill") && stop.contains(&fixture.id)),
            "the parent notice names the explicit stop: {}",
            notices[0]
        );
        assert!(
            inbox.iter().all(|m| m.report.is_none()),
            "a silent worker has not failed; no typed failure report"
        );
        drop(inbox);
        let nudges = state.agent_inbox.get(&fixture.id).map_or(0, |mail| {
            mail.iter()
                .filter(|m| m.content.contains("stall_nudge"))
                .count()
        });
        assert_eq!(nudges, 1, "the worker receives one nudge");
    }

    #[cfg(unix)]
    fn stall_notices(state: &AppState) -> usize {
        state.agent_inbox.get("coordinator").map_or(0, |mail| {
            mail.iter()
                .filter(|m| m.content.contains(r#""state":"stalled""#))
                .count()
        })
    }

    // Catches: the worker PTY is stopped while it waits for its parent (2748: tuic-2743).
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_worker_waiting_for_parent_is_not_stalled() {
        let fixture = stalled_recovery_fixture();
        make_working(&fixture.state, &fixture.id);
        let state = &fixture.state;
        state.push_agent_inbox(
            "coordinator",
            crate::state::AgentMessage {
                id: "blocked-1".into(),
                from_tuic_session: fixture.id.clone(),
                from_name: "worker".into(),
                content: "BLOCKED contract decision needed".into(),
                timestamp: 1,
                report: None,
                delivered_via_channel: false,
            },
        );
        for now in [0, 100, 200, 300] {
            crate::agent_health::sweep(state, now, 100);
        }
        assert!(
            state.session_maps.sessions.contains_key(&fixture.id),
            "a BLOCKED worker must stay alive"
        );
        assert_eq!(stall_notices(state), 0, "a BLOCKED hold is not silence");
    }

    // Catches: a finished worker is nudged every episode and resends its RESULT
    // (2748 critic F2: ego-792, tuic-2705).
    #[cfg(unix)]
    #[tokio::test]
    async fn finished_worker_gets_no_nudge() {
        let fixture = stalled_recovery_fixture();
        make_working(&fixture.state, &fixture.id);
        let state = &fixture.state;
        state.push_agent_inbox(
            "coordinator",
            crate::state::AgentMessage {
                id: "result-1".into(),
                from_tuic_session: fixture.id.clone(),
                from_name: "worker".into(),
                content: "RESULT story done, branch ready".into(),
                timestamp: 1,
                report: None,
                delivered_via_channel: false,
            },
        );
        for now in [0, 100, 150] {
            crate::agent_health::sweep(state, now, 100);
        }
        assert!(state.session_maps.sessions.contains_key(&fixture.id));
        assert_eq!(stall_notices(state), 1, "the parent still decides the stop");
        let nudges = state.agent_inbox.get(&fixture.id).map_or(0, |mail| {
            mail.iter()
                .filter(|m| m.content.contains("stall_nudge"))
                .count()
        });
        assert_eq!(nudges, 0, "a worker that reported RESULT is not nudged");
    }

    // Catches: a long background build kills its owner (2748: int-tuic, orch-174).
    // The parent still hears about the silence: it decides whether the build hangs.
    #[cfg(unix)]
    #[tokio::test]
    async fn worker_with_running_background_command_is_not_stalled() {
        let fixture = stalled_recovery_fixture();
        let state = &fixture.state;
        state
            .session_maps
            .session_states
            .get_mut(&fixture.id)
            .unwrap()
            .background_work = true;
        for now in [0, 100, 150] {
            crate::agent_health::sweep(state, now, 100);
        }
        assert!(
            state.session_maps.sessions.contains_key(&fixture.id),
            "a worker with a running background command must stay alive"
        );
        assert_eq!(stall_notices(state), 1, "one notice per silence episode");
        let notice = state
            .agent_inbox
            .get("coordinator")
            .unwrap()
            .iter()
            .find(|m| m.content.contains(r#""state":"stalled""#))
            .map(|m| m.content.clone())
            .unwrap();
        assert!(
            notice.contains(r#""background_work":true"#),
            "the parent sees the running background work: {notice}"
        );
    }

    /// The agent of a fixture session claims a turn: busy shell, no settled
    /// completion, so `agent_state` reads `working`.
    #[cfg(unix)]
    fn make_working(state: &AppState, id: &str) {
        state
            .session_maps
            .session_states
            .get_mut(id)
            .unwrap()
            .suggested_actions = None;
        state.session_maps.shell_states.insert(
            id.into(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY),
        );
    }

    /// The (stage, stalled) pair every client reads for a session.
    #[cfg(unix)]
    fn stage_of(state: &AppState, id: &str) -> (Option<String>, bool) {
        let snapshot = state.session_state_with_shell(id);
        let view = state
            .agent_reports
            .view(state, id, Some(id), snapshot.as_ref());
        (view.stage, view.stalled)
    }

    /// A session a user opened by hand: no parent records it.
    #[cfg(unix)]
    fn hand_opened_fixture() -> ShellFixture {
        let fixture = stalled_recovery_fixture();
        fixture
            .state
            .session_maps
            .session_parent
            .remove(&fixture.id);
        fixture
    }

    // Catches: stall detection limited to managed children, so a hand-opened
    // session silent for hours still reads working (story 3050, wa-2: 6.3 h).
    #[cfg(unix)]
    #[tokio::test]
    async fn hand_opened_working_session_silent_past_threshold_is_stalled() {
        let fixture = hand_opened_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        make_working(state, id);
        crate::agent_health::sweep(state, 0, 100);
        crate::agent_health::sweep(state, 99, 100);
        assert_eq!(stage_of(state, id), (Some("working".into()), false));
        crate::agent_health::sweep(state, 100, 100);
        assert_eq!(stage_of(state, id), (Some("stalled".into()), true));
        let nudges = state.agent_inbox.get(id.as_str()).map_or(0, |mail| {
            mail.iter()
                .filter(|m| m.content.contains("stall_nudge"))
                .count()
        });
        assert_eq!(nudges, 0, "only a managed child is nudged");
        assert_eq!(stall_notices(state), 0, "no parent, no parent mail");
        // Output ends the silence at once, before the next sweep.
        state
            .session_maps
            .last_output_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(150));
        assert_eq!(stage_of(state, id), (Some("working".into()), false));
    }

    // Catches: two code paths for one state, so a managed child and a
    // hand-opened session with the same silence get different stages.
    #[cfg(unix)]
    #[tokio::test]
    async fn managed_and_hand_opened_sessions_with_same_silence_share_the_stage() {
        let managed = stalled_recovery_fixture();
        let hand = hand_opened_fixture();
        for fixture in [&managed, &hand] {
            make_working(&fixture.state, &fixture.id);
            crate::agent_health::sweep(&fixture.state, 0, 100);
            crate::agent_health::sweep(&fixture.state, 100, 100);
        }
        let expected = (Some("stalled".to_string()), true);
        assert_eq!(stage_of(&managed.state, &managed.id), expected);
        assert_eq!(stage_of(&hand.state, &hand.id), expected);
        assert_eq!(stall_notices(&managed.state), 1, "the parent still hears");
    }

    // Catches: the silence rule applied to idle agents, so every tab that
    // waits for its user overnight reads stalled.
    #[cfg(unix)]
    #[tokio::test]
    async fn idle_hand_opened_session_is_never_stalled() {
        let fixture = hand_opened_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        for now in [0, 100, 200] {
            crate::agent_health::sweep(state, now, 100);
        }
        assert_eq!(stage_of(state, id), (Some("idle".into()), false));
    }

    // Catches: a stall visible to rows but never pushed, so the tab keeps its
    // green busy indicator (story 3050 criterion 5).
    #[cfg(unix)]
    #[tokio::test]
    async fn stall_is_published_in_the_session_state_clients_render() {
        let fixture = hand_opened_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        make_working(state, id);
        crate::agent_health::sweep(state, 0, 100);
        let queued = state.session_maps.session_state_events.depth();
        crate::agent_health::sweep(state, 100, 100);
        assert_eq!(
            state.session_maps.session_state_events.depth(),
            queued + 1,
            "the flagged stall is republished once"
        );
        crate::agent_health::sweep(state, 200, 100);
        assert_eq!(state.session_maps.session_state_events.depth(), queued + 1);
        let snapshot = state.session_state_with_shell(id).unwrap();
        assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
        assert_eq!(serde_json::to_value(&snapshot).unwrap()["stalled"], true);
    }

    #[cfg(unix)]
    fn child_reports(state: &AppState) -> Vec<serde_json::Value> {
        state.agent_inbox.get("coordinator").map_or(vec![], |mail| {
            mail.iter()
                .filter_map(|m| serde_json::from_str::<serde_json::Value>(&m.content).ok())
                .filter(|m| m["type"] == "child_report")
                .collect()
        })
    }

    #[cfg(unix)]
    fn report_stops_at(
        state: &Arc<AppState>,
        now: u64,
        processes: &[crate::pty::ProcessTreeEntry],
    ) {
        crate::agent_health::report_stops(state, now, &mut || Some(processes.to_vec()));
    }

    /// The child's foreground process, alive for an hour, with no descendant.
    #[cfg(unix)]
    fn quiet_agent_tree(state: &AppState, id: &str) -> Vec<crate::pty::ProcessTreeEntry> {
        let root = crate::pty::session_foreground_pid(state, id).unwrap();
        vec![crate::pty::test_process_entry(
            root, 1, "sh", "/bin/sh", 3600,
        )]
    }

    // Catches: a report on the bare idle edge (a consecutive turn), a report
    // held back for ever by a status line that repaints below the input box,
    // a report repeated on every sweep, a new parent task that never re-arms
    // it, or a report that a `report` subscription cannot select (story 3014).
    #[cfg(unix)]
    #[tokio::test]
    async fn silent_child_reports_once_per_task_after_fifteen_quiet_seconds() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().to_path_buf());
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        let quiet = quiet_agent_tree(state, id);
        report_stops_at(state, 0, &quiet);
        state
            .session_maps
            .last_input_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(5));
        report_stops_at(state, 10_000, &quiet);
        state
            .session_maps
            .last_output_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(20_000));
        report_stops_at(state, 24_999, &quiet);
        assert_eq!(
            child_reports(state),
            Vec::<serde_json::Value>::new(),
            "input restarts the window"
        );
        // The first stop is the one automatic reprompt (story 3016). A
        // reprompt that starts no turn makes the next window the report.
        report_stops_at(state, 25_000, &quiet);
        crate::pty::wait_for_injection_queue();
        state.pending_injections.remove(id.as_str());
        report_stops_at(state, 40_000, &quiet);
        report_stops_at(state, 60_000, &quiet);
        let reports: Vec<_> = child_reports(state)
            .into_iter()
            .filter(|r| r["recovery"] != "reprompt")
            .collect();
        assert_eq!(reports.len(), 1, "one report per task: {reports:?}");
        assert_eq!(reports[0]["state"], "ended_without_result");
        assert_eq!(reports[0]["obligation"], "spawn");
        assert_eq!(reports[0]["resume"]["agent"], "claude");
        assert_eq!(reports[0]["resume"]["tuic_session"], id.as_str());
        state.push_agent_inbox(
            id,
            crate::state::AgentMessage {
                id: "task-2".into(),
                from_tuic_session: "coordinator".into(),
                from_name: "coordinator".into(),
                content: "next task".into(),
                timestamp: 2,
                report: None,
                delivered_via_channel: false,
            },
        );
        report_stops_at(state, 70_000, &quiet);
        crate::pty::wait_for_injection_queue();
        state.pending_injections.remove(id.as_str());
        report_stops_at(state, 85_000, &quiet);
        let reports: Vec<_> = child_reports(state)
            .into_iter()
            .filter(|r| r["recovery"] != "reprompt")
            .collect();
        assert_eq!(reports.len(), 2, "a new task opens a new report");
        assert_eq!(reports[1]["obligation"], "task-2");
        let dir = crate::config::config_dir();
        let subscribe = state.mail_subscriptions.action(&dir, "coordinator",
            &serde_json::json!({"action":"subscribe", "subscription_id":"reports", "filter":"report"}));
        assert!(subscribe.get("error").is_none(), "{subscribe}");
        let batch = state.mail_subscriptions.action(
            &dir,
            "coordinator",
            &serde_json::json!({"action":"inbox", "subscription_id":"reports"}),
        );
        assert_eq!(
            batch["count"], 4,
            "two reprompt notices, two reports: {batch}"
        );
    }

    // Catches: a report for a child that already sent RESULT for its task.
    #[cfg(unix)]
    #[tokio::test]
    async fn child_that_sent_result_gets_no_child_report() {
        let fixture = stalled_recovery_fixture();
        let state = &fixture.state;
        state.push_agent_inbox(
            "coordinator",
            crate::state::AgentMessage {
                id: "result-1".into(),
                from_tuic_session: fixture.id.clone(),
                from_name: "worker".into(),
                content: "RESULT done".into(),
                timestamp: 1,
                report: None,
                delivered_via_channel: false,
            },
        );
        let quiet = quiet_agent_tree(state, &fixture.id);
        report_stops_at(state, 0, &quiet);
        report_stops_at(state, 60_000, &quiet);
        assert_eq!(child_reports(state), Vec::<serde_json::Value>::new());
    }

    // Catches: a child that ended its turn to wait on its own `tuic bg` job,
    // or on a background command of the agent (Claude run_in_background: a
    // hooked agent then reads idle), reported as stopped (critic-3014 F1).
    #[cfg(unix)]
    #[tokio::test]
    async fn child_waiting_on_background_work_gets_no_child_report() {
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        let quiet = quiet_agent_tree(state, id);
        let root = quiet[0].pid;
        let mut runner = quiet.clone();
        runner.push(crate::pty::test_process_entry(
            root + 100_000,
            1,
            "tuic",
            &format!("tuic __bg-runner /log {id} -- cargo test"),
            30,
        ));
        let mut build = quiet.clone();
        build.push(crate::pty::test_process_entry(
            root + 100_001,
            root,
            "cargo",
            "cargo nextest run",
            30,
        ));
        report_stops_at(state, 0, &runner);
        report_stops_at(state, 60_000, &runner);
        report_stops_at(state, 120_000, &build);
        assert_eq!(child_reports(state), Vec::<serde_json::Value>::new());
        report_stops_at(state, 180_000, &quiet);
        assert_eq!(child_reports(state).len(), 1, "the work ended: a stop");
    }

    // Catches: a dialog held back for the idle delay, or held back for ever by
    // the repaints some agents make while they wait.
    #[cfg(unix)]
    #[tokio::test]
    async fn child_on_a_dialog_reports_blocked_dialog_after_five_seconds() {
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        {
            let mut session = state.session_maps.session_states.get_mut(id).unwrap();
            session.awaiting_input = true;
            session.question_text = Some("Allow external imports?".into());
        }
        report_stops_at(state, 0, &[]);
        state
            .session_maps
            .last_output_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(3_000));
        report_stops_at(state, 5_000, &[]);
        let reports = child_reports(state);
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0]["state"], "blocked_dialog");
        assert_eq!(reports[0]["detail"]["on"], "Allow external imports?");
    }

    // Catches: an agent child that exits by itself goes unreported, or an exit
    // that TUIC caused (kill, close, suspend) is reported as a dead child.
    #[cfg(unix)]
    #[tokio::test]
    async fn only_a_self_exit_reports_a_dead_child() {
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        state
            .session_maps
            .exit_reasons
            .insert(id.clone(), "killed".into());
        crate::agent_health::report_exit(state, id, Some(143));
        assert_eq!(child_reports(state), Vec::<serde_json::Value>::new());
        state.session_maps.exit_reasons.remove(id);
        crate::agent_health::report_exit(state, id, Some(1));
        let reports = child_reports(state);
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0]["state"], "dead");
        assert_eq!(reports[0]["detail"]["exit_code"], 1);
    }

    #[cfg(unix)]
    fn reprompts(state: &AppState, id: &str) -> usize {
        state.agent_inbox.get(id).map_or(0, |mail| {
            mail.iter()
                .filter(|m| m.content.contains("report_reprompt"))
                .count()
        })
    }

    /// A refused turn on an idle composer whose writes are recorded.
    #[cfg(unix)]
    fn refused_turn(state: &AppState, id: &str) -> Arc<std::sync::Mutex<Vec<u8>>> {
        let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
        state.session_maps.sessions.get(id).unwrap().lock().writer = Arc::new(
            parking_lot::Mutex::new(Box::new(crate::test_support::RecordingWriter {
                bytes: Arc::clone(&bytes),
            })),
        );
        refuse_turn(state, id);
        bytes
    }

    #[cfg(unix)]
    fn refuse_turn(state: &AppState, id: &str) {
        state.session_maps.shell_states.insert(
            id.into(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
        );
        let mut session = state.session_maps.session_states.get_mut(id).unwrap();
        session.agent_error = true;
        session.last_error = Some("API Error: 529 overloaded".into());
    }

    #[cfg(unix)]
    fn typed(bytes: &Arc<std::sync::Mutex<Vec<u8>>>) -> String {
        crate::pty::wait_for_injection_queue();
        String::from_utf8_lossy(&bytes.lock().unwrap()).into_owned()
    }

    // Catches: a child that ended its turn without a report goes to its parent
    // with no automatic reprompt, a reprompt the parent is not told of, or a
    // reprompt repeated on each silent turn (a loop) (story 3016).
    #[cfg(unix)]
    #[tokio::test]
    async fn silent_child_gets_one_reprompt_then_a_report() {
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        let quiet = quiet_agent_tree(state, id);
        report_stops_at(state, 0, &quiet);
        report_stops_at(state, 15_000, &quiet);
        let reports = child_reports(state);
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0]["state"], "ended_without_result");
        assert_eq!(reports[0]["recovery"], "reprompt");
        assert_eq!(reprompts(state, id), 1, "the child is reprompted once");
        // The reprompt is typed and starts a second turn, which also ends
        // without a report.
        crate::pty::wait_for_injection_queue();
        state.pending_injections.remove(id.as_str());
        state
            .session_maps
            .last_input_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(16_000));
        report_stops_at(state, 16_000, &quiet);
        report_stops_at(state, 31_000, &quiet);
        report_stops_at(state, 120_000, &quiet);
        let reports = child_reports(state);
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert_eq!(reports[1]["state"], "ended_without_result");
        assert_eq!(reports[1]["recovery"], "exhausted");
        assert_eq!(reprompts(state, id), 1, "never a second reprompt");
    }

    // Catches: a provider error goes to the parent with no automatic retry, a
    // retry sooner than 60 s, or a retry the parent is not told of (story 3016).
    #[cfg(unix)]
    #[tokio::test]
    async fn refused_turn_gets_one_retry_after_sixty_seconds() {
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        let written = refused_turn(state, id);
        report_stops_at(state, 0, &[]);
        report_stops_at(state, 59_999, &[]);
        assert_eq!(child_reports(state), Vec::<serde_json::Value>::new());
        assert_eq!(typed(&written), "", "no retry before 60 s");
        report_stops_at(state, 60_000, &[]);
        let reports = child_reports(state);
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0]["state"], "error");
        assert_eq!(reports[0]["recovery"], "retry");
        assert_eq!(reports[0]["detail"]["reason"], "API Error: 529 overloaded");
        assert!(typed(&written).contains("continue"), "one retry is typed");
    }

    // Catches: a second provider error after the retry retried again (a loop),
    // or never reported to the parent (story 3016).
    #[cfg(unix)]
    #[tokio::test]
    async fn second_refused_turn_reports_without_another_retry() {
        let fixture = stalled_recovery_fixture();
        let (state, id) = (&fixture.state, &fixture.id);
        let written = refused_turn(state, id);
        report_stops_at(state, 0, &[]);
        report_stops_at(state, 60_000, &[]);
        assert_eq!(typed(&written).matches("continue").count(), 1);
        // The retried turn is refused again.
        refuse_turn(state, id);
        report_stops_at(state, 61_000, &[]);
        report_stops_at(state, 76_000, &[]);
        report_stops_at(state, 300_000, &[]);
        let reports = child_reports(state);
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert_eq!(reports[1]["state"], "error");
        assert_eq!(reports[1]["recovery"], "exhausted");
        assert_eq!(
            typed(&written).matches("continue").count(),
            1,
            "never a second retry"
        );
    }

    #[cfg(unix)]
    fn stalled_recovery_fixture() -> ShellFixture {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let id = uuid::Uuid::new_v4().to_string();
        live_child_command(
            &state,
            &id,
            crate::test_support::test_temp_root(),
            portable_pty::CommandBuilder::new("/bin/sh"),
        );
        state
            .session_maps
            .session_parent
            .insert(id.clone(), "coordinator".into());
        state.peer_agents.insert(
            id.clone(),
            crate::state::PeerAgent {
                tuic_session: id.clone(),
                mcp_session_id: "test".into(),
                name: "worker".into(),
                project: None,
                registered_at: 1,
            },
        );
        ShellFixture { state, id }
    }

    #[cfg(unix)]
    fn live_child(state: &Arc<AppState>, session_id: &str, worktree: std::path::PathBuf) {
        live_child_command(
            state,
            session_id,
            worktree,
            portable_pty::CommandBuilder::new("true"),
        );
    }

    #[cfg(unix)]
    fn live_child_command(
        state: &Arc<AppState>,
        session_id: &str,
        worktree: std::path::PathBuf,
        command: portable_pty::CommandBuilder,
    ) {
        use portable_pty::{PtySize, native_pty_system};
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open PTY");
        let child = pair.slave.spawn_command(command).expect("start child");
        let writer = pair.master.take_writer().expect("PTY writer");
        state.session_maps.sessions.insert(
            session_id.into(),
            parking_lot::Mutex::new(crate::state::PtySession {
                launch_receipt: None,
                writer: Arc::new(parking_lot::Mutex::new(writer)),
                master: pair.master,
                _child: child,
                paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                worktree: Some(crate::state::WorktreeInfo {
                    name: "child".into(),
                    path: worktree.clone(),
                    branch: Some("child".into()),
                    base_repo: worktree,
                }),
                initial_cwd: None,
                cwd: None,
                display_name: Some("worker".into()),
                display_name_is_custom: false,
                display_name_from_spawn: true,
                is_remote: false,
                shell: "true".into(),
            }),
        );
        state.session_maps.session_states.insert(
            session_id.into(),
            crate::state::SessionState {
                agent_type: Some("claude".into()),
                suggested_actions: Some(vec![]),
                ..Default::default()
            },
        );
    }

    #[cfg(unix)]
    struct ShellFixture {
        state: Arc<AppState>,
        id: String,
    }
    #[cfg(unix)]
    impl Drop for ShellFixture {
        fn drop(&mut self) {
            crate::pty::close_pty_core(&self.state, &self.id, false);
        }
    }
    #[cfg(unix)]
    fn abandoned_fixture(command: portable_pty::CommandBuilder) -> ShellFixture {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let id = uuid::Uuid::new_v4().to_string();
        live_child_command(&state, &id, crate::test_support::test_temp_root(), command);
        state.session_maps.session_states.insert(
            id.clone(),
            crate::state::SessionState {
                last_activity_ms: 1000,
                ..Default::default()
            },
        );
        state.session_maps.shell_states.insert(
            id.clone(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
        );
        state
            .session_maps
            .last_output_ms
            .insert(id.clone(), std::sync::atomic::AtomicU64::new(1000));
        ShellFixture { state, id }
    }

    // Catches: closure at the wrong threshold, default-on cleanup, or a marker
    // that never reaches the same snapshot read by desktop and mobile.
    #[cfg(unix)]
    #[test]
    fn abandoned_shell_marks_at_one_hour_and_closes_only_at_configured_threshold() {
        let fixture = abandoned_fixture(portable_pty::CommandBuilder::new("/bin/sh"));
        let (state, id) = (&fixture.state, &fixture.id);
        let mut events = state.event_bus.subscribe();
        for (now, hours, abandoned, live) in [
            (3_600_999, 1, false, true),
            (3_601_000, 0, true, true),
            (7_200_999, 2, true, true),
            (7_201_000, 2, true, false),
        ] {
            sweep_abandoned_shells_at(state, now, hours, crate::pty::process_tree_snapshot);
            while let Ok(event) = events.try_recv() {
                if matches!(event, crate::state::AppEvent::ShellAbandoned { .. }) {
                    crate::state::tests_support::apply_replay_event(state, &event);
                }
            }
            assert_eq!(
                state.session_maps.sessions.contains_key(id),
                live,
                "now={now}"
            );
            if live {
                assert_eq!(
                    state
                        .session_state_with_shell(id)
                        .unwrap()
                        .abandoned_since_ms,
                    abandoned.then_some(1000),
                    "now={now}"
                );
            }
        }
    }

    // Catches: a long-silent foreground command being killed just because its
    // agent label is absent. OS argv and descendants come from real processes.
    #[cfg(unix)]
    #[test]
    fn abandoned_shell_never_closes_foreground_or_background_commands() {
        for script in [
            "sleep 600",
            "sleep 600 & wait",
            ":; while read line; do :; done",
        ] {
            let mut cmd = portable_pty::CommandBuilder::new("/bin/sh");
            cmd.args(["-c", script]);
            let fixture = abandoned_fixture(cmd);
            sweep_abandoned_shells_at(
                &fixture.state,
                100_000_000,
                1,
                crate::pty::process_tree_snapshot,
            );
            assert!(
                fixture
                    .state
                    .session_maps
                    .sessions
                    .contains_key(&fixture.id),
                "{script}"
            );
        }
    }

    // Catches: failed inventory authorizing deletion, activity racing the probe,
    // overflow at u16::MAX, or an identity-bound managed peer losing its shell.
    #[cfg(unix)]
    #[test]
    fn abandoned_shell_fails_closed_on_unknown_activity_and_live_ownership() {
        let fixture = abandoned_fixture(portable_pty::CommandBuilder::new("/bin/sh"));
        let (state, id) = (&fixture.state, &fixture.id);
        sweep_abandoned_shells_at(state, 100_000_000, 1, || None);
        assert!(state.session_maps.sessions.contains_key(id));
        sweep_abandoned_shells_at(
            state,
            100_000_000,
            u16::MAX,
            crate::pty::process_tree_snapshot,
        );
        assert!(state.session_maps.sessions.contains_key(id));
        sweep_abandoned_shells_at(state, 100_000_000, 1, || {
            let ps = crate::pty::process_tree_snapshot();
            state
                .session_maps
                .last_input_ms
                .insert(id.clone(), std::sync::atomic::AtomicU64::new(100_000_000));
            ps
        });
        assert!(state.session_maps.sessions.contains_key(id));
        state.session_maps.last_input_ms.remove(id);
        state.bind_live_pty("owner-identity", id);
        state
            .session_maps
            .session_parent
            .insert("live-child".into(), "owner-identity".into());
        sweep_abandoned_shells_at(state, 100_000_000, 1, crate::pty::process_tree_snapshot);
        assert!(state.session_maps.sessions.contains_key(id));
    }

    /// The parent is an MCP peer without a PTY, as an orchestrator usually is.
    #[cfg(unix)]
    fn live_parent(state: &Arc<AppState>) {
        state
            .mcp
            .session_to_mcp
            .insert("parent".into(), vec!["parent-mcp".into()]);
    }

    #[cfg(unix)]
    fn verified_hibernation_agent(state: &Arc<AppState>, sid: &str, root: &std::path::Path) {
        // Own the PTY and idle state without a reader that can overwrite the fixture.
        live_child(state, sid, root.to_path_buf());
        state.session_maps.sessions.get(sid).unwrap().lock().cwd =
            Some(root.to_string_lossy().into_owned());
        let profile = root.join("claude");
        let cwd = root.to_string_lossy().into_owned();
        let slug: String = cwd
            .trim_end_matches('/')
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let project = profile.join("projects").join(slug);
        std::fs::create_dir_all(&project).unwrap();
        let agent_id = "11111111-1111-4111-8111-111111111111";
        // The owned verifier's contract is existence of the exact profile/session file.
        std::fs::write(project.join(format!("{agent_id}.jsonl")), "").unwrap();
        state.session_maps.session_states.insert(
            sid.into(),
            crate::state::SessionState {
                agent_type: Some("claude".into()),
                agent_state: Some("idle".into()),
                suggested_actions: Some(vec![]),
                ..Default::default()
            },
        );
        state.suspend_metadata.insert(sid.into(), serde_json::json!({
            "agentSessionId": agent_id,
            "agentLaunchCommand": format!("CLAUDE_CONFIG_DIR={} claude", crate::prompt::shell_quote(&profile.to_string_lossy()))
        }));
    }

    #[cfg(unix)]
    fn always_hibernate() {
        let base = crate::config::load_agents_config();
        let mut desired = base.clone();
        desired.hibernate_scope = crate::config::HibernateScope::Always;
        desired.hibernate_idle_minutes = 1;
        crate::config::save_agents_config(base, desired).unwrap();
    }

    #[cfg(unix)]
    fn assert_due_sweep_preserves(state: &Arc<AppState>, sid: &str) {
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(state, &mut tracker, 0, &[]);
        sweep_with_commands(state, &mut tracker, 60_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key(sid),
            "excluded PTY must stay live"
        );
        assert!(
            crate::suspended_sessions::list().unwrap().is_empty(),
            "excluded PTY must not publish a retained record"
        );
    }

    // Catches: idle top-level agents never entering the durable hibernation path.
    #[cfg(unix)]
    #[tokio::test]
    async fn hibernates_resumable_idle_agent_after_threshold_in_background_only() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "hibernate-agent".to_string();
        verified_hibernation_agent(&state, &sid, root.path());
        let mut tracker = IdleCloseTracker::default();
        // Existing visibility controller's authoritative state; no desktop is launched.
        struct HiddenWindows;
        impl crate::background_mode::hidden::Windows for HiddenWindows {
            fn visible_labels(&self) -> Vec<String> {
                vec!["main".into()]
            }
            fn hide(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn show(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn unminimize(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn focus(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn background_resources(&self, _: bool) {}
        }
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key(&sid),
            "background-only scope must preserve foreground agents"
        );
        crate::background_mode::hidden::enter_background(&state, &HiddenWindows).unwrap();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        assert!(
            hibernate_observation(&state, &sid).is_some(),
            "fixture must remain idle"
        );
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        let records = crate::suspended_sessions::list().unwrap();
        let live = state.session_maps.sessions.contains_key(&sid);
        crate::pty::close_pty_core_with_reason(&state, &sid, false, "test_cleanup");
        assert!(!live, "due top-level agent must hibernate");
        assert_eq!(records[&sid].reason, "idle_hibernate");
    }

    // Catches: keep-open addressed by durable peer identity being ignored after PTY rebinding.
    #[cfg(unix)]
    #[tokio::test]
    async fn hibernation_preserves_keep_open_peer_after_pty_identity_rebinding() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "rebound-pty";
        let identity = "durable-peer";
        verified_hibernation_agent(&state, sid, root.path());
        state
            .session_maps
            .live_pty_by_tuic_session
            .insert(identity.into(), sid.into());
        assert!(
            hibernate_allowed(&state, sid),
            "fixture must otherwise qualify"
        );
        // agent send keep_open stores the resolved peer identity, not its current PTY key.
        state.keep_open_sessions.insert(identity.into());
        let result = crate::suspended_sessions::suspend_with_reason(
            &state,
            sid,
            &serde_json::Value::Null,
            "idle_hibernate",
        );
        let still_live = state.session_maps.sessions.contains_key(sid);
        let retained = crate::suspended_sessions::list().unwrap();
        crate::pty::close_pty_core_with_reason(&state, sid, false, "test_cleanup");
        assert!(
            result.is_err(),
            "keep-open peer must refuse automatic hibernation"
        );
        assert!(still_live, "keep-open peer must retain its live PTY");
        assert!(
            !retained.contains_key(identity),
            "refused hibernation must not publish a retained tab"
        );
    }

    // Catches: a night asleep expiring the actual hibernation deadline.
    #[cfg(unix)]
    #[tokio::test]
    async fn sleep_time_does_not_count_as_idle() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "sleep-agent";
        verified_hibernation_agent(&state, sid, root.path());
        always_hibernate();
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        for gap in [30, 29, 8 * 3600] {
            tracker.awake_ms =
                add_awake_time(tracker.awake_ms, std::time::Duration::from_secs(gap));
            let now_ms = tracker.awake_ms;
            sweep_with_commands(&state, &mut tracker, now_ms, &[]);
            assert!(
                state.session_maps.sessions.contains_key(sid),
                "sleep must not expire the deadline"
            );
            assert!(crate::suspended_sessions::list().unwrap().is_empty());
        }
        tracker.awake_ms = add_awake_time(tracker.awake_ms, std::time::Duration::from_secs(1));
        let now_ms = tracker.awake_ms;
        sweep_with_commands(&state, &mut tracker, now_ms, &[]);
        let live = state.session_maps.sessions.contains_key(sid);
        let records = crate::suspended_sessions::list().unwrap();
        crate::pty::close_pty_core_with_reason(&state, sid, false, "test_cleanup");
        assert!(!live, "one final awake second must expire the deadline");
        assert_eq!(records[sid].reason, "idle_hibernate");
    }

    // Catches: unattended classification overlooking mail, work or an active reader.
    #[cfg(unix)]
    #[tokio::test]
    async fn never_hibernates_with_unread_mail_background_work_awaiting_input_or_watching_client() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "excluded-agent";
        verified_hibernation_agent(&state, sid, root.path());
        always_hibernate();
        let idle = state.session_maps.session_states.get(sid).unwrap().clone();
        assert!(
            hibernate_allowed(&state, sid),
            "verified baseline must qualify"
        );
        for candidate in [
            crate::state::SessionState {
                background_work: true,
                ..idle.clone()
            },
            crate::state::SessionState {
                awaiting_input: true,
                ..idle.clone()
            },
        ] {
            state
                .session_maps
                .session_states
                .insert(sid.into(), candidate);
            assert_due_sweep_preserves(&state, sid);
        }
        state.session_maps.session_states.insert(sid.into(), idle);
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        state.ws_clients.insert(sid.into(), vec![tx]);
        assert_due_sweep_preserves(&state, sid);
        state.ws_clients.remove(sid);
        state.keep_open_sessions.insert(sid.into());
        assert_due_sweep_preserves(&state, sid);
        state.keep_open_sessions.remove(sid);
        state.push_agent_inbox(
            sid,
            crate::state::AgentMessage {
                id: "mail".into(),
                from_tuic_session: "sender".into(),
                from_name: "sender".into(),
                content: "unread".into(),
                timestamp: 1,
                report: None,
                delivered_via_channel: false,
            },
        );
        assert_due_sweep_preserves(&state, sid);
        crate::pty::close_pty_core_with_reason(&state, sid, false, "test_cleanup");
    }

    // Catches: plain shells, unsupported and fallback-only agents losing their live PTY.
    #[cfg(unix)]
    #[tokio::test]
    async fn never_hibernates_non_resumable_agent_or_plain_shell() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "non-resumable-agent";
        verified_hibernation_agent(&state, sid, root.path());
        always_hibernate();
        let idle = state.session_maps.session_states.get(sid).unwrap().clone();
        assert!(
            hibernate_allowed(&state, sid),
            "verified baseline must qualify"
        );
        for agent in [None, Some("droid"), Some("unknown"), Some("amp")] {
            state.session_maps.session_states.insert(
                sid.into(),
                crate::state::SessionState {
                    agent_type: agent.map(str::to_owned),
                    ..idle.clone()
                },
            );
            assert_due_sweep_preserves(&state, sid);
        }
        crate::pty::close_pty_core_with_reason(&state, sid, false, "test_cleanup");
    }

    // Catches: an idle agent being ended while its Automation run still owns it.
    #[cfg(unix)]
    #[tokio::test]
    async fn never_hibernates_a_session_bound_to_an_open_automation_run() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "automation-agent".to_string();
        verified_hibernation_agent(&state, &sid, root.path());
        always_hibernate();
        assert!(
            hibernate_allowed(&state, &sid),
            "verified baseline must qualify"
        );
        let owner = crate::automations::store::RunOwner::acquire(0).unwrap();
        let definition = serde_json::from_value(serde_json::json!({
            "id":"owned", "name":"Owned", "prompt":"work", "run_config":"amp", "repository":"/project",
            "workspace":{"mode":"existing"}, "cron":"0 9 * * *", "timezone":"UTC", "enabled":true,
            "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600, "precheck":null
        })).unwrap();
        let run = owner
            .store()
            .reserve(&definition, crate::automations::run::RunTrigger::Manual, 0)
            .unwrap()
            .unwrap();
        owner
            .store()
            .transition(
                &run.id,
                crate::automations::run::RunStatus::Prechecking,
                crate::automations::run::RunDetails {
                    session_id: Some(sid.clone()),
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        assert_due_sweep_preserves(&state, &sid);
        crate::pty::close_pty_core_with_reason(&state, &sid, false, "test_cleanup");
    }

    fn idle() -> Observation {
        Observation {
            state: "idle".into(),
            turn_epoch: 1,
            last_input_ms: 100,
            last_output_ms: 200,
            last_mail_id: None,
            read_cursor: 0,
        }
    }

    #[test]
    fn idle_managed_child_closes_at_fifteen_minutes_and_activity_restarts_window() {
        let mut tracker = IdleCloseTracker::default();
        assert!(!tracker.observe("child", 0, 900_000, Some(idle())));
        assert!(!tracker.observe("child", 899_999, 900_000, Some(idle())));
        let mut after_output = idle();
        after_output.last_output_ms = 300;
        assert!(!tracker.observe("child", 899_999, 900_000, Some(after_output.clone())));
        assert!(!tracker.observe("child", 1_799_998, 900_000, Some(after_output.clone())));
        assert!(tracker.observe("child", 1_799_999, 900_000, Some(after_output)));
    }

    #[test]
    fn active_work_or_mail_or_keep_open_never_ages_into_close() {
        let mut tracker = IdleCloseTracker::default();
        assert!(!tracker.observe("child", 0, 900_000, Some(idle())));
        assert!(!tracker.observe("child", 1_000_000, 900_000, None));
        assert!(!tracker.observe("child", 1_000_001, 900_000, Some(idle())));
        assert!(!tracker.observe("child", 1_000_002, 0, Some(idle())));
    }

    #[test]
    fn detached_bg_job_owned_by_child_blocks_idle_close_until_runner_exits() {
        let child = "9023a0e6-229b-4322-ba92-93507eb00d15";
        let other = "917ec723-27e5-47fb-8a6a-801f65af6cf6";
        let commands = vec![format!(
            "/usr/local/bin/tuic __bg-runner /some/log {child} -- cargo test"
        )];
        assert!(live_bg_runner_for_session(&commands, child));
        assert!(!live_bg_runner_for_session(&commands, other));
        assert!(!live_bg_runner_for_session(&[], child));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_sweep_skips_process_snapshot_until_managed_child_reaches_deadline() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-snapshot-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let mut tracker = IdleCloseTracker::default();
        let snapshots = std::cell::Cell::new(0);
        let mut count_snapshot = || {
            snapshots.set(snapshots.get() + 1);
            Some(vec![])
        };
        sweep_with_snapshot(&state, &mut tracker, 0, &mut count_snapshot);
        assert_eq!(snapshots.get(), 0, "no managed child needs a process scan");

        live_child(&state, "managed-child", temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert("managed-child".into(), "parent".into());
        state
            .session_maps
            .session_states
            .get_mut("managed-child")
            .unwrap()
            .background_work = true;
        sweep_with_snapshot(&state, &mut tracker, 900_000, &mut count_snapshot);
        assert_eq!(
            snapshots.get(),
            0,
            "background work is not an idle-close candidate"
        );
        state
            .session_maps
            .session_states
            .get_mut("managed-child")
            .unwrap()
            .background_work = false;
        sweep_with_snapshot(&state, &mut tracker, 900_000, &mut count_snapshot);
        sweep_with_snapshot(&state, &mut tracker, 1_799_999, &mut count_snapshot);
        assert_eq!(
            snapshots.get(),
            0,
            "an idle child is not due before its delay"
        );
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "managed-child");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_sweep_rechecks_live_runner_at_deadline_before_closing() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-runner-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "managed-child";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        let mut tracker = IdleCloseTracker::default();
        let snapshots = std::cell::Cell::new(0);
        sweep_with_snapshot(&state, &mut tracker, 0, || {
            snapshots.set(snapshots.get() + 1);
            Some(vec![])
        });
        assert_eq!(
            snapshots.get(),
            0,
            "runner inventory waits until the close deadline"
        );
        sweep_with_snapshot(&state, &mut tracker, 900_000, || {
            snapshots.set(snapshots.get() + 1);
            Some(vec![format!("tuic __bg-runner /log {child} -- cargo test")])
        });
        assert_eq!(
            snapshots.get(),
            1,
            "the destructive step checks live runners"
        );
        assert!(state.session_maps.sessions.contains_key(child));
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, child);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_sweep_resumes_after_a_panicking_process_snapshot() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-panic-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "managed-child";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        let tracker = sweep_step(Arc::clone(&state), IdleCloseTracker::default(), 0, || {
            Some(vec![])
        })
        .await;
        let tracker = sweep_step(Arc::clone(&state), tracker, 900_000, || {
            panic!("injected process inventory failure")
        })
        .await;
        assert!(state.session_maps.sessions.contains_key(child));
        let tracker = sweep_step(Arc::clone(&state), tracker, 900_001, || Some(vec![])).await;
        sweep_step(Arc::clone(&state), tracker, 1_800_001, || Some(vec![])).await;
        assert!(!state.session_maps.sessions.contains_key(child));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn only_managed_child_closes_after_delay_and_parent_gets_reason_while_worktree_stays() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let root = crate::test_support::test_temp_root();
        let temp = tempfile::Builder::new()
            .prefix("idle-close-")
            .tempdir_in(root)
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let worktree = temp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let marker = worktree.join("preserve.txt");
        std::fs::write(&marker, "committed work").unwrap();
        live_child(&state, "managed-child", worktree.clone());
        live_child(&state, "manual-session", worktree);
        live_child(&state, "pending-parent", temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert("managed-child".into(), "parent".into());
        state
            .session_maps
            .session_parent
            .insert("pending-parent".into(), "pending-mcp:unbound".into());
        state.peer_agents.insert(
            "managed-child".into(),
            crate::state::PeerAgent {
                tuic_session: "managed-child".into(),
                mcp_session_id: String::new(),
                name: "worker".into(),
                project: None,
                registered_at: 0,
            },
        );
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 899_999, &[]);
        assert!(state.session_maps.sessions.contains_key("managed-child"));
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(!state.session_maps.sessions.contains_key("managed-child"));
        assert!(state.session_maps.sessions.contains_key("manual-session"));
        assert!(state.session_maps.sessions.contains_key("pending-parent"));
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "committed work");
        let notices = state.agent_inbox.get("parent").unwrap();
        let notice: serde_json::Value =
            serde_json::from_str(&notices.back().unwrap().content).unwrap();
        assert_eq!(notice["reason"], "idle_timeout");
        assert_eq!(notice["name"], "worker");
        drop(notices);
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "manual-session");
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "pending-parent");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mail_background_work_keep_open_and_detached_runner_each_prevent_close() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let root = crate::test_support::test_temp_root();
        let temp = tempfile::Builder::new()
            .prefix("idle-close-")
            .tempdir_in(root)
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        live_child(&state, "child-with-work", temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert("child-with-work".into(), "parent".into());
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        state.keep_open_sessions.insert("child-with-work".into());
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        state.keep_open_sessions.remove("child-with-work");
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        state
            .session_maps
            .session_states
            .get_mut("child-with-work")
            .unwrap()
            .background_work = true;
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        state
            .session_maps
            .session_states
            .get_mut("child-with-work")
            .unwrap()
            .background_work = false;
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        let runner = vec!["tuic __bg-runner /log child-with-work -- make check".into()];
        sweep_with_commands(&state, &mut tracker, 2_700_000, &runner);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        state.push_agent_inbox(
            "child-with-work",
            crate::state::AgentMessage {
                id: "mail".into(),
                from_tuic_session: "parent".into(),
                from_name: "parent".into(),
                content: "follow up".into(),
                timestamp: 2_700_001,
                report: None,
                delivered_via_channel: false,
            },
        );
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "child-with-work");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_whose_last_mail_to_parent_is_blocked_stays_open_until_a_newer_mail() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-blocked-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "managed-child";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        live_parent(&state);
        let mail = |from: &str, content: &str, timestamp| crate::state::AgentMessage {
            id: format!("mail-{timestamp}"),
            from_tuic_session: from.into(),
            from_name: from.into(),
            content: content.into(),
            timestamp,
            report: None,
            delivered_via_channel: false,
        };
        state.push_agent_inbox("parent", mail(child, "BLOCKED: rb box unreachable", 10));
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key(child),
            "a BLOCKED child waiting for its parent must not be idle-closed"
        );
        state.push_agent_inbox(child, mail("parent", "box is back", 20));
        state.agent_read_cursor.insert(child.into(), 20);
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        assert!(!state.session_maps.sessions.contains_key(child));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_or_retrying_bg_wake_keeps_child_open_until_a_successful_wake() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-wake-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "9023a0e6-229b-4322-ba92-93507eb00d15";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        let wakes = crate::config::config_dir().join("bg-wakes");
        std::fs::create_dir_all(&wakes).unwrap();
        let marker = wakes.join(format!("{child}.json"));
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"failed"}}"#),
        )
        .unwrap();
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(state.session_maps.sessions.contains_key(child));
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"retrying"}}"#),
        )
        .unwrap();
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        assert!(state.session_maps.sessions.contains_key(child));
        // Catches: exhausted ambiguous queue receipts close a child still owed a wake.
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"uncertain"}}"#),
        )
        .unwrap();
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        assert!(state.session_maps.sessions.contains_key(child));
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"queued"}}"#),
        )
        .unwrap();
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        assert!(!state.session_maps.sessions.contains_key(child));
    }

    // ---- critic-1319: adversarial cases for the BLOCKED hold and the close reason ----

    #[cfg(unix)]
    fn critic_mail(
        id: &str,
        from: &str,
        content: &str,
        timestamp: u64,
    ) -> crate::state::AgentMessage {
        crate::state::AgentMessage {
            id: id.into(),
            from_tuic_session: from.into(),
            from_name: from.into(),
            content: content.into(),
            timestamp,
            report: None,
            delivered_via_channel: false,
        }
    }

    #[cfg(unix)]
    fn critic_child(state: &Arc<AppState>, child: &str, dir: &std::path::Path) {
        live_parent(state);
        live_child(state, child, dir.to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
    }

    #[cfg(unix)]
    fn critic_temp() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("idle-close-critic-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap()
    }

    /// Two sweeps 15 minutes apart: the first starts the window, the second matures it.
    #[cfg(unix)]
    fn critic_sweep_to_maturity(state: &Arc<AppState>) {
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(state, &mut tracker, 0, &[]);
        sweep_with_commands(state, &mut tracker, 900_000, &[]);
    }

    #[cfg(unix)]
    #[derive(Clone)]
    struct CriticLogSink(Arc<std::sync::Mutex<Vec<u8>>>);

    #[cfg(unix)]
    impl std::io::Write for CriticLogSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[cfg(unix)]
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CriticLogSink {
        type Writer = CriticLogSink;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Catches: idle close still logging `close_requested` (or the reason being dropped),
    /// so the cause of an idle kill is invisible in the log (criterion 3).
    #[cfg(unix)]
    #[test]
    fn idle_close_of_an_unblocked_child_logs_reason_idle_close() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "quiet-child", temp.path());
        let output = Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(CriticLogSink(output.clone()))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || critic_sweep_to_maturity(&state));
        assert!(!state.session_maps.sessions.contains_key("quiet-child"));
        let log = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(log.contains("Closing session"), "{log}");
        assert!(log.contains("reason=\"idle_close\""), "{log}");
        assert!(!log.contains("close_requested"), "{log}");
    }

    /// Catches: the hold keying on any earlier BLOCKED instead of the LAST mail, so a
    /// child that reported BLOCKED and later finished with RESULT is kept open for ever.
    #[cfg(unix)]
    #[tokio::test]
    async fn child_that_mailed_blocked_then_result_is_closed() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.push_agent_inbox("parent", critic_mail("m2", "c", "RESULT: done", 20));
        critic_sweep_to_maturity(&state);
        assert!(!state.session_maps.sessions.contains_key("c"));
    }

    /// Catches: a lifecycle notice (`tuic-auto-*`) that TUIC itself files for the child
    /// after its BLOCKED being treated as the child's last mail, so the hold is lost and
    /// the blocked child is closed.
    #[cfg(unix)]
    #[tokio::test]
    async fn lifecycle_notice_after_blocked_does_not_release_the_hold() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.push_agent_inbox(
            "parent",
            critic_mail("tuic-auto-state-1", "c", "state idle", 20),
        );
        critic_sweep_to_maturity(&state);
        assert!(state.session_maps.sessions.contains_key("c"));
    }

    /// Catches: the hold matching on the parent's inbox without filtering by sender, so
    /// one child's BLOCKED keeps every sibling open.
    #[cfg(unix)]
    #[tokio::test]
    async fn sibling_blocked_does_not_hold_a_child_with_no_mail() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "quiet", temp.path());
        state.push_agent_inbox(
            "parent",
            critic_mail("m1", "other", "BLOCKED: box down", 10),
        );
        critic_sweep_to_maturity(&state);
        assert!(!state.session_maps.sessions.contains_key("quiet"));
    }

    /// Catches: the hold living only in the parent's bounded inbox (capacity 100): once
    /// the BLOCKED mail is evicted by later traffic from other children, the blocked
    /// child is closed while it still waits for an answer.
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_hold_survives_parent_inbox_overflow_from_other_senders() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        for index in 0..crate::state::AGENT_INBOX_CAPACITY {
            state.push_agent_inbox(
                "parent",
                critic_mail(
                    &format!("noise-{index}"),
                    "other",
                    "RESULT: x",
                    11 + index as u64,
                ),
            );
        }
        critic_sweep_to_maturity(&state);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "BLOCKED mail was evicted from the parent inbox and the child was closed"
        );
    }

    /// Catches: comparing the parent inbox's logical clock with the child inbox's clock.
    /// A burst of same-millisecond mail makes the parent clock run ahead of wall time, so
    /// a reply stamped with the real time is judged older than BLOCKED and never releases.
    #[cfg(unix)]
    #[tokio::test]
    async fn reply_releases_the_hold_even_when_parent_clock_ran_ahead() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        for index in 0..20 {
            state.push_agent_inbox(
                "parent",
                critic_mail(&format!("burst-{index}"), "other", "RESULT: x", 1_000),
            );
        }
        // Stored as 1_020 by the per-recipient logical clock.
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 1_000));
        // The parent answers 5 ms later in wall time.
        state.push_agent_inbox("c", critic_mail("reply", "parent", "box is back", 1_005));
        state.agent_read_cursor.insert("c".into(), 1_005);
        critic_sweep_to_maturity(&state);
        assert!(
            !state.session_maps.sessions.contains_key("c"),
            "the parent replied, the hold must be released"
        );
    }

    // ---- critic-1319 round 2: lifecycle of the blocked_children set ----

    /// Catches: the hold surviving the child's PTY. The set is only swept by the idle
    /// sweep, so a session re-created under the same durable id before the next sweep
    /// inherits a stale BLOCKED hold and is never closed.
    #[cfg(unix)]
    #[tokio::test]
    async fn closing_a_blocked_child_drops_its_hold_before_the_id_is_reused() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        assert!(state.blocked_children.contains("c"));
        crate::pty::close_pty_core(&state, "c", false);
        critic_child(&state, "c", temp.path());
        critic_sweep_to_maturity(&state);
        assert!(
            !state.session_maps.sessions.contains_key("c"),
            "a new session under a reused id inherited the dead session's BLOCKED hold"
        );
    }

    /// Catches: any non-lifecycle mail to the child releasing the hold. A sibling's
    /// chatter is not the parent answering; once the child has read it, the child is
    /// still blocked on its parent and must stay open.
    #[cfg(unix)]
    #[tokio::test]
    async fn mail_from_a_sibling_does_not_release_the_blocked_hold() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.push_agent_inbox("c", critic_mail("s1", "sibling", "fyi", 20));
        state.agent_read_cursor.insert("c".into(), 20);
        critic_sweep_to_maturity(&state);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "a sibling's mail released a hold that only the parent can end"
        );
    }

    /// Catches: a dead parent leaving its blocked child held for ever, with nobody
    /// left to answer it.
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_child_of_a_dead_parent_is_closed_like_any_idle_child() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.mcp.session_to_mcp.remove("parent");
        critic_sweep_to_maturity(&state);
        assert!(!state.session_maps.sessions.contains_key("c"));
    }

    // ---- critic-1319 round 3: release paths ----

    /// Catches: a transient parent absence (MCP session reaped, then reconnect) dropping
    /// the hold for good. The sweep prunes the hold while the parent is away; when the
    /// parent returns still waiting to answer, the blocked child is closed.
    #[cfg(unix)]
    #[tokio::test]
    async fn parent_reconnecting_after_a_sweep_still_finds_its_child_held() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        let mut tracker = IdleCloseTracker::default();
        state.mcp.session_to_mcp.remove("parent");
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        live_parent(&state);
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "the parent was only away for one sweep; its BLOCKED child was closed"
        );
    }

    /// Catches: a late BLOCKED mail from a child that is already closed re-creating a
    /// hold that outlives it (and is inherited by a reused id).
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_mail_from_a_closed_child_sets_no_hold() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        crate::pty::close_pty_core(&state, "c", false);
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: late", 10));
        assert!(!state.blocked_children.contains("c"));
    }

    /// Catches: the hold and its release staying bound to the old parent id after the
    /// parent re-registers under a new one (session_parent rewritten). The hold must
    /// survive the rebind, the old id's mail must not release it, the new parent's must.
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_hold_follows_the_parent_to_its_new_identity() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.mcp.session_to_mcp.remove("parent");
        state
            .mcp
            .session_to_mcp
            .insert("parent2".into(), vec!["parent2-mcp".into()]);
        state
            .session_maps
            .session_parent
            .insert("c".into(), "parent2".into());
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "hold lost on rebind"
        );
        state.push_agent_inbox("c", critic_mail("old", "parent", "stale id", 20));
        state.agent_read_cursor.insert("c".into(), 20);
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "mail from the retired parent id released the hold"
        );
        state.push_agent_inbox("c", critic_mail("new", "parent2", "box is back", 30));
        state.agent_read_cursor.insert("c".into(), 30);
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        assert!(
            !state.session_maps.sessions.contains_key("c"),
            "the new parent's mail did not release the hold"
        );
    }
}
