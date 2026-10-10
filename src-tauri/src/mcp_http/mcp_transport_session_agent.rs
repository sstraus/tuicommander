use crate::pty::{resolve_shell, spawn_reader_thread};
use crate::{AppState, MAX_CONCURRENT_SESSIONS, PtySession};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "desktop")]
use tauri::Emitter;
use uuid::Uuid;

use super::mcp_transport::emit_close_html_tabs;
use super::mcp_transport::insert_optional_value;
use super::mcp_transport::to_json_or_error;
use super::mcp_transport_ancillary::journal_hand_off;
use super::mcp_transport_ancillary::resolve_mcp_origin_pty;
use super::mcp_transport_ancillary::resolve_mcp_origin_session;
use super::mcp_transport_catalogue::AGENT_ACTIONS;
use super::mcp_transport_catalogue::SESSION_ACTIONS;
use super::mcp_transport_catalogue::TASK_ACTIONS;
use super::mcp_transport_peer::INITIAL_PROMPT_DELIVERY_TIMEOUT;
use super::mcp_transport_peer::TASK_POLL_INTERVAL_MS;
use super::mcp_transport_peer::caller_task_identities;
use super::mcp_transport_peer::handle_messaging;
use super::mcp_transport_peer::is_pending_parent;
use super::mcp_transport_peer::now_unix_ms;
use super::mcp_transport_peer::pending_parent_id;
use super::mcp_transport_peer::task_owner_identity;

/// Translate special key names to terminal escape sequences
pub(super) fn translate_special_key(key: &str) -> Option<&'static str> {
    match key {
        "enter" | "return" => Some("\r"),
        "tab" => Some("\t"),
        "escape" | "esc" => Some("\x1b"),
        "backspace" => Some("\x7f"),
        "delete" => Some("\x1b[3~"),
        "up" => Some("\x1b[A"),
        "down" => Some("\x1b[B"),
        "right" => Some("\x1b[C"),
        "left" => Some("\x1b[D"),
        "home" => Some("\x1b[H"),
        "end" => Some("\x1b[F"),
        "ctrl+c" => Some("\x03"),
        "ctrl+d" => Some("\x04"),
        "ctrl+z" => Some("\x1a"),
        "ctrl+l" => Some("\x0c"),
        "ctrl+a" => Some("\x01"),
        "ctrl+e" => Some("\x05"),
        "ctrl+k" => Some("\x0b"),
        "ctrl+u" => Some("\x15"),
        "ctrl+w" => Some("\x17"),
        "ctrl+r" => Some("\x12"),
        "ctrl+p" => Some("\x10"),
        "ctrl+n" => Some("\x0e"),
        _ => None,
    }
}

pub(super) fn uses_agent_command_injection(
    agent_type: Option<&str>,
    key_seq: Option<&str>,
) -> bool {
    agent_type.is_some_and(crate::agent::prompt_prefill_only) && key_seq == Some("\r")
}

/// Extract action from args, returning a guidance error if missing
pub(super) fn require_action<'a>(
    args: &'a serde_json::Value,
    tool: &str,
    available: &str,
) -> Result<&'a str, serde_json::Value> {
    args["action"]
        .as_str()
        .ok_or_else(|| serde_json::json!({"error": format!("Missing 'action'. Available actions for '{}': {}", tool, available)}))
}

/// Extract the session reference from args and resolve it to the live PTY key.
///
/// Callers hold whichever name they were given: the PTY key, the `$TUIC_SESSION`
/// a desktop tab persists, or the short repo alias the tab menu shows. All three
/// address one terminal — see [`AppState::resolve_session_ref`].
///
/// A reference that resolves to nothing is handed back unchanged rather than
/// rejected here. An exited session keeps its output buffers as a tombstone while
/// its `sessions` entry is gone, so `action=output` on a dead child must still
/// reach the handler that can answer it — and that handler owns the
/// "Session not found" wording.
fn require_session_id(
    state: &AppState,
    args: &serde_json::Value,
    action: &str,
) -> Result<String, serde_json::Value> {
    let reference = args["session_id"].as_str().ok_or_else(|| {
        serde_json::json!({"error": format!(
            "Action '{action}' requires 'session_id' — a PTY id, a tuic_session, or an alias such as 'tu-1'. Get valid values with session action='list'"
        )})
    })?;
    state
        .resolve_session_ref_checked(reference)
        .map_err(|error| serde_json::json!({"error": error}))
        .map(|resolved| resolved.unwrap_or_else(|| reference.to_string()))
}

pub(super) fn require_string<'a>(
    args: &'a serde_json::Value,
    field: &str,
) -> Result<&'a str, serde_json::Value> {
    args[field].as_str().ok_or_else(
        || serde_json::json!({"error": format!("Missing required parameter '{field}'")}),
    )
}

#[derive(Debug, PartialEq)]
pub(super) enum PtyDescriptionUpdate {
    Unchanged,
    Set(Option<String>),
}

pub(super) const INFERRED_PTY_DESCRIPTION_MAX_CHARS: usize = 160;

/// Parse the optional PTY description field. Omitted means unchanged; null or
/// an empty/whitespace-only string clears the current description.
pub(super) fn parse_pty_description(
    args: &serde_json::Value,
) -> Result<PtyDescriptionUpdate, serde_json::Value> {
    let Some(value) = args.get("pty_description") else {
        return Ok(PtyDescriptionUpdate::Unchanged);
    };
    match value {
        serde_json::Value::Null => Ok(PtyDescriptionUpdate::Set(None)),
        serde_json::Value::String(text) => {
            let text = text.trim();
            Ok(PtyDescriptionUpdate::Set(
                (!text.is_empty()).then(|| text.to_string()),
            ))
        }
        _ => Err(serde_json::json!({
            "error": "'pty_description' must be a string or null"
        })),
    }
}

/// Resolve the description for a newly orchestrated PTY. Callers with a rich
/// orchestration surface can set or clear it explicitly; callers whose spawn
/// schema only carries a task prompt still get a compact, display-only summary.
/// This never changes the prompt delivered to the child.
pub(super) fn resolve_spawn_pty_description(
    update: PtyDescriptionUpdate,
    prompt: &str,
) -> Option<String> {
    match update {
        PtyDescriptionUpdate::Set(description) => description,
        PtyDescriptionUpdate::Unchanged => {
            let collapsed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
            if collapsed.is_empty() {
                return None;
            }

            let mut chars = collapsed.chars();
            let prefix: String = chars
                .by_ref()
                .take(INFERRED_PTY_DESCRIPTION_MAX_CHARS)
                .collect();
            if chars.next().is_none() {
                Some(prefix)
            } else {
                let mut truncated: String = prefix
                    .chars()
                    .take(INFERRED_PTY_DESCRIPTION_MAX_CHARS - 1)
                    .collect();
                truncated.push('…');
                Some(truncated)
            }
        }
    }
}

fn apply_pty_description(state: &AppState, session_id: &str, description: PtyDescriptionUpdate) {
    if let PtyDescriptionUpdate::Set(description) = description {
        state.set_pty_description(session_id, description);
    }
}

/// Extract path from args with guidance error
pub(super) fn require_path(
    args: &serde_json::Value,
    action: &str,
) -> Result<String, serde_json::Value> {
    args["path"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| serde_json::json!({"error": format!("Action '{}' requires 'path' (absolute path to git repository)", action)}))
}

/// Default `wait` timeout when the caller omits `timeout_ms`.
pub(super) const WAIT_DEFAULT_MS: u64 = 60_000;
/// Advertised cap on a single server-side wait — the number in the tool schema.
pub(super) const WAIT_MAX_MS: u64 = 300_000;
/// Headroom between the advertised cap and the wait we actually run.
///
/// A client aborts its own `tools/call` on a deadline of its own, and at least
/// one shipping client (Codex) uses exactly 300s — the same round number as our
/// cap. A wait that runs the full `WAIT_MAX_MS` therefore answers right on that
/// deadline and loses the race every time, turning the advertised maximum into a
/// guaranteed error instead of `{timed_out:true}`. The bridge already reserves
/// the same margin on its own response deadline. Kept client-agnostic on
/// purpose: a per-client table would have to be maintained against every client
/// release, and a wait that ends 5s early is indistinguishable to the caller.
const WAIT_CLIENT_DEADLINE_MARGIN_MS: u64 = 5_000;
/// The wait actually run for a caller asking for the cap or more.
pub(super) const WAIT_EFFECTIVE_MAX_MS: u64 = WAIT_MAX_MS - WAIT_CLIENT_DEADLINE_MARGIN_MS;

/// Resolve the effective wait timeout: default when absent/zero, capped so the
/// reply beats the caller's own tool-call deadline.
pub(super) fn clamp_wait_timeout(requested: Option<u64>) -> u64 {
    match requested {
        Some(ms) if ms > 0 => ms.min(WAIT_EFFECTIVE_MAX_MS),
        _ => WAIT_DEFAULT_MS,
    }
}

/// Whether a session's blocking-wait condition is currently satisfied.
/// `until` is "idle" (shell idle) or "exited" (process gone / exit code recorded).
pub(super) fn session_wait_met(state: &AppState, session_id: &str, until: &str) -> bool {
    match until {
        // `exit_codes` is recorded by `mark_session_exited` and kept for the
        // tombstone TTL. Using only this signal avoids a false "exited" for a
        // never-created (typo'd) session id, which would otherwise return met
        // immediately because it isn't in `sessions`.
        "exited" => state.session_maps.exit_codes.contains_key(session_id),
        // Default and "idle": shell state reached IDLE.
        _ => state
            .session_maps
            .shell_states
            .get(session_id)
            .map(|a| a.load(std::sync::atomic::Ordering::Relaxed) == crate::pty::SHELL_IDLE)
            .unwrap_or(false),
    }
}

fn session_wait_response(
    state: &AppState,
    session_id: &str,
    until: &str,
    met: bool,
) -> serde_json::Value {
    if !met {
        return serde_json::json!({
            "met": false,
            "timed_out": true,
            "until": until,
        });
    }
    let shell_state = state
        .session_maps
        .shell_states
        .get(session_id)
        .and_then(|value| {
            crate::pty::shell_state_wire(value.load(std::sync::atomic::Ordering::Relaxed))
        });
    let mut response = serde_json::json!({
        "met": true,
        "timed_out": false,
        "until": until,
    });
    let object = response
        .as_object_mut()
        .expect("session wait response is an object");
    insert_optional_value(
        object,
        "shell_state",
        shell_state.map(|value| serde_json::Value::String(value.to_string())),
    );
    insert_optional_value(
        object,
        "exit_code",
        state
            .session_maps
            .exit_codes
            .get(session_id)
            .map(|entry| serde_json::Value::from(*entry.value())),
    );
    response
}

/// How long the tab has to answer a suspend request: it verifies the agent's resume
/// command and closes the PTY before it can say yes.
const SUSPEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// A UI that can own the tab is attached: the desktop window, or an open HTTP event stream.
fn ui_attached(state: &AppState) -> bool {
    #[cfg(feature = "desktop")]
    if state.app_handle.read().is_some() {
        return true;
    }
    state
        .sse_client_count
        .load(std::sync::atomic::Ordering::Relaxed)
        > 0
}

/// `session action=suspend`. The tab lives in the frontend, which ends the PTY and keeps
/// the tab; this returns the tab's verdict, so a refusal there is never reported as success.
pub(super) async fn handle_session_suspend(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let resolved = match require_session_id(state, args, "suspend") {
        Ok(id) => id,
        Err(e) => return e,
    };
    let session_id = resolved.as_str();
    // Self-suspend guard: mirror close — an agent must not end its own session.
    if let Some(sid) = mcp_session_id
        && let Some(own_pty) = state.mcp.to_session.get(sid)
        && own_pty.value() == session_id
    {
        return serde_json::json!({"error": "Cannot suspend own session."});
    }
    // An exited session keeps its `session_states` entry (and the agent state it died in)
    // until SessionClosed, so that entry alone would let the busy rule pass for a dead PTY.
    if state.session_maps.exit_codes.contains_key(session_id) {
        return serde_json::json!({"error": "Cannot suspend: session has exited"});
    }
    let Some(ss) = state.session_state_with_shell(session_id) else {
        return serde_json::json!({"error": "Session not found"});
    };
    if let Some(reason) = suspend_refusal(&ss) {
        return serde_json::json!({"error": format!("Cannot suspend: {reason}")});
    }
    if !ui_attached(state) {
        return serde_json::json!({"error": "Cannot suspend: no UI is attached to own the tab (headless)"});
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.suspend_responses.insert(request_id.clone(), tx);
    state.request_session_suspend(session_id, &request_id);
    match tokio::time::timeout(SUSPEND_TIMEOUT, rx).await {
        Ok(Ok(Ok(()))) => serde_json::json!({"ok": true}),
        Ok(Ok(Err(reason))) => serde_json::json!({"error": format!("Cannot suspend: {reason}")}),
        // Sender dropped, or no tab answered: no UI owns this session's tab.
        _ => {
            state.suspend_responses.remove(&request_id);
            serde_json::json!({"error": "Cannot suspend: no tab answered the request"})
        }
    }
}

/// Deliver a tab's suspend verdict to the waiting MCP call. Shared by the Tauri command
/// and the HTTP route. Unknown or already-answered ids are ignored: every attached client
/// that owns the tab may answer, and only the first can win.
pub(crate) fn resolve_session_suspend(
    state: &AppState,
    request_id: &str,
    ok: bool,
    reason: Option<String>,
) {
    if let Some((_, tx)) = state.suspend_responses.remove(request_id) {
        let _ = tx.send(if ok {
            Ok(())
        } else {
            Err(reason.unwrap_or_else(|| "refused".to_string()))
        });
    }
}

/// `session action=wait` — block (server-side) until the session is idle or has
/// exited, or the timeout elapses. Replaces an LLM polling loop (each poll is a
/// full model turn) with one cheap blocking call.
pub(super) async fn handle_session_wait(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let session_id = match require_session_id(state, args, "wait") {
        Ok(id) => id,
        Err(e) => return e,
    };
    let until = args["until"].as_str().unwrap_or("idle");
    if !matches!(until, "idle" | "exited") {
        return serde_json::json!({"error": "wait 'until' must be 'idle' or 'exited'"});
    }
    let timeout_ms = clamp_wait_timeout(args["timeout_ms"].as_u64());
    // Answer an already-satisfied wait first, before the liveness guard below.
    // `mark_session_exited` records the exit code and THEN drops the `sessions`
    // entry, so a just-reaped child satisfies `until=exited` from its tombstone
    // while failing that guard — checking liveness first turned the one outcome
    // `until=exited` exists to report into `Unknown session`. This path
    // subscribes to nothing, so it cannot leak a channel either.
    if session_wait_met(state, &session_id, until) {
        return session_wait_response(state, &session_id, until, true);
    }
    // Reject an unknown session BEFORE subscribing. `subscribe_pty_events` uses
    // the DashMap entry API, so it CREATES a 256-slot broadcast channel for
    // whatever id it is handed. Session teardown reaps that entry — but only for
    // ids that were ever real sessions, so a caller passing made-up ids grew
    // `pty_event_channels` with entries nothing will ever remove. It also spared
    // the caller a pointless full-timeout block on a session that cannot exist.
    if !state.session_maps.sessions.contains_key(&session_id) {
        return serde_json::json!({
            "error": format!("Unknown session \"{session_id}\"")
        });
    }
    // Subscribe, then re-read state. This closes the lost-wake window without
    // polling: a transition landing between the fast path above and this
    // subscription is still visible in state, while any later one is retained by
    // the per-session event receiver.
    let mut events = state.subscribe_pty_events(&session_id);
    if session_wait_met(state, &session_id, until) {
        return session_wait_response(state, &session_id, until, true);
    }
    let woke = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
        loop {
            match events.recv().await {
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if session_wait_met(state, &session_id, until) {
                        return true;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    // Re-check at the timeout boundary so a simultaneous transition wins over
    // a spurious timed_out response.
    let met = woke || session_wait_met(state, &session_id, until);
    session_wait_response(state, &session_id, until, met)
}

const SUBMIT_ACK_DEFAULT_MS: u64 = 3_000;
pub(super) const SUBMIT_ACK_MIN_MS: u64 = 250;
const SUBMIT_ACK_MAX_MS: u64 = 10_000;
const SUBMIT_ACK_CHECK_MS: u64 = 10;

struct StartedSubmission {
    pub(super) submission_id: String,
    pub(super) session_id: String,
    pub(super) turn_epoch: u64,
    acknowledgement_offset: u64,
    pub(super) timeout_ms: u64,
    pub(super) text: String,
}

enum BeginSubmission {
    Response(serde_json::Value),
    Started(StartedSubmission),
}

fn submission_turn_epoch(state: &AppState, session_id: &str) -> u64 {
    state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch)
        .unwrap_or(0)
}

fn submission_output_offset(state: &AppState, session_id: &str) -> Option<u64> {
    state
        .session_maps
        .output_buffers
        .get(session_id)
        .map(|buffer| buffer.lock().total_written)
}

fn begin_session_submit(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    human_reply: bool,
) -> BeginSubmission {
    let session_id = match require_session_id(state, args, "submit") {
        Ok(id) => id,
        Err(error) => return BeginSubmission::Response(error),
    };
    let text = match args["input"].as_str() {
        Some(text) if !text.is_empty() => text,
        _ => {
            return BeginSubmission::Response(serde_json::json!({
                "error": "Action 'submit' requires non-empty 'input'"
            }));
        }
    };
    let submission_id = Uuid::new_v4().to_string();
    let turn_epoch = submission_turn_epoch(state, &session_id);
    if !state.session_maps.sessions.contains_key(&session_id) {
        return BeginSubmission::Response(serde_json::json!({
            "status": "rejected",
            "submission_id": submission_id,
            "submitted": false,
            "write_state": "not_started",
            "acknowledged": false,
            "retry_safe": true,
            "reason": "session_not_found",
            "detail": crate::pty::agent_submission_rejection_detail(state, &session_id, "session_not_found"),
            "turn_epoch": turn_epoch,
            "composer_state": "unknown",
        }));
    }
    if submission_output_offset(state, &session_id).is_none() {
        return BeginSubmission::Response(serde_json::json!({
            "status": "rejected",
            "submission_id": submission_id,
            "submitted": false,
            "write_state": "not_started",
            "acknowledged": false,
            "retry_safe": true,
            "reason": "observation_unavailable",
            "detail": crate::pty::agent_submission_rejection_detail(state, &session_id, "observation_unavailable"),
            "turn_epoch": turn_epoch,
            "composer_state": "unknown",
        }));
    }

    let write = if human_reply {
        crate::pty::write_human_reply_to_pty(state, &session_id, text)
    } else {
        crate::pty::write_agent_submission_to_pty(state, &session_id, text)
    };
    match write {
        crate::pty::AgentSubmissionWrite::Rejected {
            reason,
            composer_state,
            pending,
        } => BeginSubmission::Response(serde_json::json!({
            "status": "rejected",
            "submission_id": submission_id,
            "submitted": false,
            "write_state": "not_started",
            "acknowledged": false,
            "retry_safe": true,
            "reason": reason,
            "detail": crate::pty::agent_submission_rejection_detail(state, &session_id, reason),
            "turn_epoch": submission_turn_epoch(state, &session_id),
            "composer_state": composer_state,
            // What is actually parked ahead of this submission, by id and kind.
            // Delete an entry with the queued-commands endpoint to unblock the
            // composer; a bare `queued_commands_pending` against an empty
            // Compose list left a caller retrying with nothing to act on.
            "pending": pending,
        })),
        crate::pty::AgentSubmissionWrite::Failed(detail) => {
            BeginSubmission::Response(serde_json::json!({
                "status": "write_failed",
                "submission_id": submission_id,
                "submitted": false,
                "write_state": "not_started",
                "acknowledged": false,
                "retry_safe": true,
                "reason": "pty_write_failed",
                "detail": detail,
                "turn_epoch": submission_turn_epoch(state, &session_id),
                "composer_state": "empty",
            }))
        }
        crate::pty::AgentSubmissionWrite::Uncertain(detail) => {
            BeginSubmission::Response(serde_json::json!({
                "status": "write_uncertain",
                "submission_id": submission_id,
                "submitted": false,
                "write_state": "uncertain",
                "acknowledged": false,
                "retry_safe": false,
                "reason": "pty_write_uncertain",
                "detail": detail,
                "turn_epoch": submission_turn_epoch(state, &session_id),
                "composer_state": "unknown",
            }))
        }
        crate::pty::AgentSubmissionWrite::Complete {
            acknowledgement_offset,
        } => {
            // Reuse the same FSM path as raw MCP/HTTP input. The text and CR are
            // bookkeeping boundaries only; the framed PTY bytes were written once
            // above. This advances the authoritative turn epoch exactly once.
            crate::pty_capture::record_input(&session_id, text.as_bytes());
            crate::pty_capture::record_input(&session_id, b"\r");
            super::session::apply_input_bookkeeping(state, &session_id, text);
            super::session::apply_input_bookkeeping(state, &session_id, "\r");
            let timeout_ms = args["timeout_ms"]
                .as_u64()
                .unwrap_or(SUBMIT_ACK_DEFAULT_MS)
                .clamp(SUBMIT_ACK_MIN_MS, SUBMIT_ACK_MAX_MS);
            BeginSubmission::Started(StartedSubmission {
                submission_id,
                session_id: session_id.clone(),
                turn_epoch: submission_turn_epoch(state, &session_id),
                acknowledgement_offset,
                timeout_ms,
                text: text.to_string(),
            })
        }
    }
}

pub(super) async fn handle_session_submit(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    human_reply: bool,
) -> serde_json::Value {
    let pty_description = match parse_pty_description(args) {
        Ok(description) => description,
        Err(error) => return error,
    };
    let blocking_state = Arc::clone(state);
    let blocking_args = args.clone();
    let begin = match tokio::task::spawn_blocking(move || {
        begin_session_submit(&blocking_state, &blocking_args, human_reply)
    })
    .await
    {
        Ok(begin) => begin,
        Err(error) => {
            return serde_json::json!({
                "error": format!("tool handler failed to complete: {error}")
            });
        }
    };
    let started = match begin {
        BeginSubmission::Response(response) => return response,
        BeginSubmission::Started(started) => started,
    };
    // The coordinator answers a BLOCKED child through the terminal.
    state.blocked_children.remove(&started.session_id);
    apply_pty_description(state, &started.session_id, pty_description);

    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_millis(started.timeout_ms);
    loop {
        let current_epoch = submission_turn_epoch(state, &started.session_id);
        if current_epoch != started.turn_epoch {
            return serde_json::json!({
                "status": "superseded",
                "submission_id": started.submission_id,
                "submitted": true,
                "write_state": "complete",
                "acknowledged": false,
                "retry_safe": false,
                "reason": "turn_epoch_changed",
                "turn_epoch": started.turn_epoch,
                "current_turn_epoch": current_epoch,
                "composer_state": "cleared",
            });
        }
        if let Some(output_offset) = submission_output_offset(state, &started.session_id)
            && output_offset > started.acknowledgement_offset
            // A repaint that still shows the text in the composer is not a turn.
            && !crate::pty::composer_retains_text(state, &started.session_id, &started.text)
        {
            return serde_json::json!({
                "status": "acknowledged",
                "submission_id": started.submission_id,
                "submitted": true,
                "write_state": "complete",
                "acknowledged": true,
                "retry_safe": false,
                "turn_epoch": started.turn_epoch,
                "composer_state": "cleared",
                "acknowledgement": {
                    "kind": "terminal_movement",
                    "screen_state": crate::pty::agent_submission_ack_kind(state, &started.session_id),
                    "output_offset": output_offset,
                },
            });
        }
        if !state
            .session_maps
            .sessions
            .contains_key(&started.session_id)
        {
            let mut response = serde_json::json!({
                "status": "session_ended",
                "submission_id": started.submission_id,
                "submitted": true,
                "write_state": "complete",
                "acknowledged": false,
                "retry_safe": false,
                "reason": "session_ended_before_ack",
                "turn_epoch": started.turn_epoch,
                "composer_state": "unknown",
            });
            if let Some(exit_code) = state.session_maps.exit_codes.get(&started.session_id) {
                response["exit_code"] = serde_json::json!(*exit_code.value());
            }
            return response;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return serde_json::json!({
                "status": "ack_timeout",
                "submission_id": started.submission_id,
                "submitted": true,
                "write_state": "complete",
                "acknowledged": false,
                "retry_safe": false,
                "reason": "no_terminal_movement_after_enter",
                "turn_epoch": started.turn_epoch,
                "composer_state": "cleared",
                "timeout_ms": started.timeout_ms,
                "output_offset": submission_output_offset(state, &started.session_id),
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            SUBMIT_ACK_CHECK_MS.min((deadline - now).as_millis().max(1) as u64),
        ))
        .await;
    }
}

/// Secrets the terminal knows whole, so that a read showing only a fragment of
/// one can still scrub it: the retained log (cached until it changes), and the
/// screen with the history rows that wrap into it.
fn terminal_secrets(buf: &mut crate::state::VtLogBuffer) -> Vec<String> {
    let mut secrets = buf.cached_log_secrets(|buf| {
        let (log_lines, _) = buf.lines_since_owned(buf.oldest_offset(), usize::MAX);
        crate::redaction::secrets_in(&crate::redaction::join_wrapped_rows(
            log_lines.iter().map(|ll| (ll.text(), ll.wrapped)),
        ))
    });
    let screen = crate::redaction::join_wrapped_rows(
        buf.screen_rows().into_iter().zip(buf.screen_row_wraps()),
    );
    let screen_context = format!("{}{screen}", buf.screen_head_context());
    secrets.extend(crate::redaction::secrets_in(&screen_context));
    // A PEM footer can still be on screen while its header/body are in history.
    // Join both only for that boundary; ordinary polling keeps the log cache.
    if screen_context.contains("-----END ") && screen_context.contains("PRIVATE KEY-----") {
        let (log_lines, _) = buf.lines_since_owned(buf.oldest_offset(), usize::MAX);
        secrets.extend(crate::redaction::secrets_in(
            &crate::redaction::join_wrapped_rows(
                log_lines
                    .iter()
                    .map(|ll| (ll.text(), ll.wrapped))
                    .chain(buf.screen_rows().into_iter().zip(buf.screen_row_wraps())),
            ),
        ));
    }
    secrets
}

/// Redact the raw byte stream of a session. Patterns alone miss a token that
/// the line editor redrew across a wrap (the pieces sit between cursor moves),
/// so every secret found in the whole ring or on the terminal grid is also
/// scrubbed wherever a fragment of it survives in `window`. (#1281-10e6)
fn redact_raw_output(
    state: &Arc<AppState>,
    session_id: &str,
    bytes: &[u8],
    window: std::ops::Range<usize>,
    mut known: Vec<String>,
    paged: bool,
) -> String {
    if let Some(vt) = state.grid.vt_log_buffers.get(session_id) {
        known.extend(terminal_secrets(&mut vt.lock()));
    }
    if !paged {
        return state.secrets.mask(&crate::redaction::redact_secrets(
            &crate::redaction::scrub_fragments(&String::from_utf8_lossy(&bytes[window]), &known),
        ));
    }
    let masked = state.secrets.mask_preserving_offsets(bytes);
    let context = String::from_utf8_lossy(&masked);
    let redacted = crate::redaction::mask_raw_context(&context, &known);
    // Lossy decoding can expand invalid source bytes. Translate source-byte
    // boundaries after registry masking, before the length-preserving redaction.
    let start = String::from_utf8_lossy(&masked[..window.start]).len();
    let end = String::from_utf8_lossy(&masked[..window.end]).len();
    redacted[start..end].to_owned()
}

/// Describe a page of retained output and the existing action that continues it.
fn add_output_page_metadata(
    response: &mut serde_json::Value,
    args: &serde_json::Value,
    start: u64,
    next: u64,
    total: u64,
    oldest: u64,
) {
    let raw = args["format"] == "raw";
    let position_key = if raw { "from_byte" } else { "from_line" };
    let requested = args["since_cursor"]
        .as_u64()
        .or_else(|| args[position_key].as_u64());
    let missed = requested.map_or(0, |offset| oldest.saturating_sub(offset));
    let has_more = next < total;
    let omitted_tail_history = requested.is_none() && start > oldest;
    response["start_offset"] = start.into();
    response["oldest_offset"] = oldest.into();
    response["has_more"] = has_more.into();
    response["next_cursor"] = if has_more {
        next.into()
    } else {
        serde_json::Value::Null
    };
    response["truncated"] = (has_more || omitted_tail_history || missed > 0).into();
    if missed > 0 {
        response["missed_count"] = missed.into();
    }
    if has_more || omitted_tail_history {
        let mut request = serde_json::json!({
            "action": "output", "session_id": args["session_id"],
            "limit": args["limit"].as_u64().unwrap_or(50).max(1),
        });
        if raw {
            request["format"] = "raw".into();
        }
        request[position_key] = if has_more { next.into() } else { oldest.into() };
        response["continuation"] = format!(
            "Output truncated to a retained window. Fetch {} with session {}; do not rerun the command. Positions may expire when the buffer evicts output.",
            if has_more { "the next page" } else { "older output" }, request
        ).into();
    } else if missed > 0 {
        response["continuation"] = "Requested output was evicted; no further retained page is available. The missing output cannot be recovered from this buffer.".into();
    }
}

pub(super) fn session_output(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    handle_session(state, args, None)
}

pub(super) fn handle_session(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "session", SESSION_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "declare_worktree" => {
            let Some(peer) = resolve_mcp_origin_session(state, mcp_session_id) else {
                return serde_json::json!({"error": "declare_worktree requires an authenticated managed caller"});
            };
            let Some(pty) = resolve_mcp_origin_pty(state, mcp_session_id) else {
                return serde_json::json!({"error": "declare_worktree requires the caller's live terminal"});
            };
            if args.get("session_id").is_some() && args["session_id"].as_str() != Some(pty.as_str())
            {
                return serde_json::json!({"error": "declare_worktree cannot target another session"});
            }
            let Some(path) = args["worktree_path"].as_str() else {
                return serde_json::json!({"error": "declare_worktree requires worktree_path"});
            };
            match super::session_placement::declare_worktree(state, &peer, &pty, path) {
                Ok(payload) => to_json_or_error(payload),
                Err(error) => serde_json::json!({"error": error}),
            }
        }
        "list" => {
            let caller_tuic = mcp_session_id
                .and_then(|sid| state.mcp.to_session.get(sid))
                .map(|entry| entry.value().clone());
            // Resolve, do not compare. A desktop tab persists its own
            // `$TUIC_SESSION`, which is never the key its PTY was filed under, so
            // `caller_tuic == pty_id` answered false for every tab — the one case
            // the flag exists for, since an orchestrator that cannot recognise its
            // own terminal can close itself.
            let caller_pty = caller_tuic
                .as_deref()
                .and_then(|tuic| state.live_pty_for_peer(tuic));
            // One pass instead of one scan per row: a per-session reverse lookup
            // would be quadratic in the session cap for a list every orientation
            // call reads.
            //
            // A session opened with no caller identity is bound under its own PTY
            // key as a fallback, so a real `$TUIC_SESSION` outranks that entry
            // whichever order the map is walked — otherwise the field would depend
            // on hash order.
            let tuic_by_pty = super::session::live_tuic_sessions_by_pty(state);
            let mut sessions: Vec<serde_json::Value> = state
                .session_maps
                .sessions
                .iter()
                .map(|entry| {
                    let id = entry.key().clone();
                    let s = entry.value().lock();
                    #[cfg(not(windows))]
                    let pgid = s.master.process_group_leader();
                    #[cfg(windows)]
                    let pgid = s._child.process_id();
                    #[cfg(not(windows))]
                    let process_name =
                        pgid.and_then(|p| crate::pty::process_name_from_pid(p as u32));
                    #[cfg(windows)]
                    let process_name = pgid.and_then(crate::pty::process_name_from_pid);
                    let session_state = state.session_state_with_shell(&id);
                    let shell_state = session_state
                        .as_ref()
                        .and_then(|snapshot| snapshot.shell_state.clone());
                    let agent_state = session_state
                        .as_ref()
                        .and_then(|snapshot| snapshot.agent_state.clone());
                    let background_work = session_state
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.background_work);
                    let alias = state
                        .session_maps
                        .term_aliases
                        .get(&id)
                        .map(|e| e.value().clone());
                    #[cfg(unix)]
                    let standby = state
                        .session_maps
                        .standby_sessions
                        .contains_key(id.as_str());
                    #[cfg(not(unix))]
                    let standby = false;
                    let mut session = serde_json::json!({
                        "session_id": id,
                        "is_caller": caller_pty.as_deref() == Some(id.as_str()),
                    });
                    let object = session
                        .as_object_mut()
                        .expect("session list entry is an object");
                    // Both flags are false for nearly every session, and an omitted
                    // field already reads as "not the case" everywhere else in this
                    // payload.
                    insert_optional_value(
                        object,
                        "background_work",
                        background_work.then_some(serde_json::Value::Bool(true)),
                    );
                    insert_optional_value(
                        object,
                        "standby",
                        standby.then_some(serde_json::Value::Bool(true)),
                    );
                    insert_optional_value(object, "alias", alias.map(serde_json::Value::String));
                    insert_optional_value(
                        object,
                        "tuic_session",
                        tuic_by_pty.get(&id).cloned().map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "display_name",
                        s.display_name.clone().map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "pty_description",
                        state
                            .session_maps
                            .pty_descriptions
                            .get(&id)
                            .map(|value| serde_json::Value::String(value.value().clone())),
                    );
                    insert_optional_value(
                        object,
                        "cwd",
                        s.cwd.clone().map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "worktree_path",
                        s.worktree.as_ref().map(|worktree| {
                            serde_json::Value::String(worktree.path.to_string_lossy().to_string())
                        }),
                    );
                    insert_optional_value(
                        object,
                        "worktree_branch",
                        s.worktree
                            .as_ref()
                            .and_then(|worktree| worktree.branch.clone())
                            .map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "foreground_process",
                        process_name.map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "shell_state",
                        shell_state.map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "agent_state",
                        agent_state.map(serde_json::Value::String),
                    );
                    session
                })
                .collect();
            for row in crate::remote_mirror::mirrored_rows(state) {
                let mut value = to_json_or_error(&row);
                value["is_caller"] = serde_json::json!(false);
                if let Some(remote_state) = row.state.as_ref() {
                    if let Some(shell_state) = remote_state.shell_state.as_ref() {
                        value["shell_state"] = serde_json::json!(shell_state);
                    }
                    if let Some(agent_state) = remote_state.agent_state.as_ref() {
                        value["agent_state"] = serde_json::json!(agent_state);
                    }
                }
                if let Some(host) = row.connection_id.as_deref() {
                    value["address"] = serde_json::json!(format!("{host}/{}", row.session_id));
                }
                sessions.push(value);
            }
            if let Some(host) = args["connection_id"].as_str() {
                if host != "local" && state.remote.base_url(host).is_none() {
                    return serde_json::json!({"error":format!("Remote connection '{host}' is unavailable"),"connection_id":host});
                }
                sessions.retain(|row| {
                    if host == "local" {
                        row.get("connection_id").is_none()
                    } else {
                        row["connection_id"] == host
                    }
                });
            }
            serde_json::json!(sessions)
        }
        "create" => {
            if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
                return serde_json::json!({"error": "Max concurrent sessions reached"});
            }
            let rows = args["rows"].as_u64().unwrap_or(24) as u16;
            let cols = args["cols"].as_u64().unwrap_or(80) as u16;
            if let Err(msg) = super::validate_terminal_size(rows, cols) {
                return serde_json::json!({"error": msg});
            }
            let shell = resolve_shell(args["shell"].as_str().map(|s| s.to_string()));
            let cwd = args["cwd"].as_str().map(|s| s.to_string());

            match super::session::spawn_pty_session(
                state.clone(),
                shell,
                cwd,
                rows,
                cols,
                None,
                super::session::RequestedIdentity::default(),
            ) {
                Ok(session_id) => serde_json::json!({"session_id": session_id}),
                Err((_, body)) => {
                    serde_json::json!({"error": body.0.get("error").and_then(|v| v.as_str()).unwrap_or("spawn failed")})
                }
            }
        }
        "input" => {
            let resolved = match require_session_id(state, args, "input") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            let text = args["input"].as_str().unwrap_or("");
            let key_seq: Option<&str> = if let Some(key) = args["special_key"].as_str() {
                match translate_special_key(key) {
                    Some(seq) => Some(seq),
                    None => {
                        return serde_json::json!({"error": format!("Unknown special key: {}", key)});
                    }
                }
            } else {
                None
            };
            let pty_description = match parse_pty_description(args) {
                Ok(description) => description,
                Err(error) => return error,
            };
            if text.is_empty()
                && key_seq.is_none()
                && matches!(&pty_description, PtyDescriptionUpdate::Unchanged)
            {
                return serde_json::json!({"error": "Action 'input' requires 'input' (text), 'special_key', or 'pty_description'"});
            }
            if text.is_empty() && key_seq.is_none() {
                if !state.session_maps.sessions.contains_key(session_id) {
                    return serde_json::json!({"error": "Session not found"});
                }
                apply_pty_description(state, session_id, pty_description);
                return serde_json::json!({"ok": true});
            }
            let agent_type = state
                .session_maps
                .session_states
                .get(session_id)
                .and_then(|s| s.agent_type.clone());

            // Submitting text to a prefill-only agent is not a generic text+key
            // pair. Codex/OpenCode require Ctrl-U framing, bracketed paste for
            // multiline prompts, and a real scheduling gap before CR. Reuse the
            // peer-injection recipe without changing Claude's working input path.
            if !text.is_empty() && uses_agent_command_injection(agent_type.as_deref(), key_seq) {
                if let Err(e) = crate::pty::write_agent_command_to_pty(state, session_id, text) {
                    return serde_json::json!({"error": e});
                }
                crate::pty_capture::record_input(session_id, text.as_bytes());
                crate::pty_capture::record_input(session_id, b"\r");
                super::session::apply_input_bookkeeping(state, session_id, text);
                super::session::apply_input_bookkeeping(state, session_id, "\r");
                apply_pty_description(state, session_id, pty_description);
                return serde_json::json!({"ok": true});
            }

            // Non-agent sessions and non-Enter special keys retain raw pair
            // semantics under one lock so concurrent writers cannot interleave.
            match (text.is_empty(), key_seq) {
                (false, Some(seq)) => {
                    if let Err(e) = super::session::write_pty_input_pair(
                        state,
                        session_id,
                        text,
                        seq,
                        agent_type.as_deref(),
                    ) {
                        return serde_json::json!({"error": e});
                    }
                }
                (false, None) => {
                    if let Err(e) = super::session::write_pty_input(state, session_id, text) {
                        return serde_json::json!({"error": e});
                    }
                }
                (true, Some(seq)) => {
                    if let Err(e) = super::session::write_pty_input(state, session_id, seq) {
                        return serde_json::json!({"error": e});
                    }
                }
                (true, None) => unreachable!("checked above: text.is_empty() && key_seq.is_none()"),
            }
            apply_pty_description(state, session_id, pty_description);
            serde_json::json!({"ok": true})
        }
        "submit" => serde_json::json!({
            "error": "Action 'submit' requires the asynchronous MCP dispatch path"
        }),
        "output" => {
            let resolved = match require_session_id(state, args, "output") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            let limit = (args["limit"].as_u64().unwrap_or(50) as usize).max(1);

            // Resolve the session's lifecycle state.
            //
            // A session can be in four observable states here:
            //   1. Live       — present in `state.session_maps.sessions`, child still running
            //   2. Draining   — present in `state.session_maps.sessions`, child already exited
            //   3. Tombstoned — absent from `state.session_maps.sessions` but buffers still present
            //                   (reader thread called `mark_session_exited` on EOF;
            //                   reaped by `spawn_tombstone_sweeper` after TTL)
            //   4. Unknown    — no trace at all; either never existed or already reaped
            //
            // `exited` is only true for (2) and (3) — cases where we have evidence
            // the process actually terminated. (4) returns a structured error.
            let session_entry = state.session_maps.sessions.get(session_id);
            let buffers_present = state.grid.vt_log_buffers.contains_key(session_id)
                || state.session_maps.output_buffers.contains_key(session_id);

            let (exited, exit_code): (bool, Option<i64>) = if let Some(entry) = &session_entry {
                match entry.lock()._child.try_wait() {
                    Ok(Some(status)) => {
                        let code = if let Some(sig) = status.signal() {
                            128 + crate::pty::parse_signal_number(sig) as i64
                        } else {
                            status.exit_code() as i64
                        };
                        (true, Some(code))
                    }
                    _ => (false, None),
                }
            } else if buffers_present {
                // Tombstoned — the reader thread captured the exit code if it could.
                (
                    true,
                    state
                        .session_maps
                        .exit_codes
                        .get(session_id)
                        .map(|e| *e.value() as i64),
                )
            } else {
                // Unknown — no session entry, no buffers, no tombstone.
                (false, None)
            };
            drop(session_entry);
            // Default: serve clean rows from VtLogBuffer (no strip_ansi needed).
            // Pass format="raw" to get the raw ring buffer content with ANSI.
            if args["format"].as_str() != Some("raw") {
                let vt_log = match state.grid.vt_log_buffers.get(session_id) {
                    Some(b) => b,
                    None => {
                        return serde_json::json!({
                            "error": "Session not found",
                            "reason": "session_not_found_or_reaped"
                        });
                    }
                };
                let mut buf = vt_log.lock();
                let total = buf.total_lines();
                let oldest = buf.oldest_offset();
                let scrollback_lines = total - oldest;

                // Delta read: if since_cursor provided, return only new scrollback lines.
                if let Some(since) = args["since_cursor"].as_u64().map(|v| v as usize) {
                    let (log_lines, start, new_cursor) = buf.lines_since_logical(since, limit);
                    // Redaction applies to all three reads below — delta, absolute
                    // and raw ring. `format=raw` keeps ANSI; it is not an opt-out
                    // of redaction, and `data_length` reports what was returned.
                    let known = terminal_secrets(&mut buf);
                    let data = crate::redaction::redact_wrapped_rows(
                        log_lines.iter().map(|ll| (ll.text(), ll.wrapped)),
                        &known,
                    );
                    let mut response = serde_json::json!({"data": data, "data_length": data.len(), "cursor": new_cursor, "scrollback_lines": scrollback_lines, "oldest_offset": oldest, "exited": exited});
                    add_output_page_metadata(
                        &mut response,
                        args,
                        start as u64,
                        new_cursor as u64,
                        total as u64,
                        oldest as u64,
                    );
                    insert_optional_value(
                        response
                            .as_object_mut()
                            .expect("output response is an object"),
                        "exit_code",
                        exit_code.map(serde_json::Value::from),
                    );
                    return response;
                }

                // Absolute positioning: from_line overrides the default tail window.
                let offset = if let Some(from) = args["from_line"].as_u64().map(|v| v as usize) {
                    from.max(oldest)
                } else {
                    total.saturating_sub(limit)
                };
                let (log_lines, start, page_end) = buf.lines_since_logical(offset, limit);
                let mut all_lines: Vec<(String, bool)> =
                    log_lines.iter().map(|ll| (ll.text(), ll.wrapped)).collect();
                // Only append screen rows when reading the tail (no from_line).
                if args["from_line"].is_null() {
                    let mut screen: Vec<(String, bool)> = buf
                        .screen_rows()
                        .into_iter()
                        .zip(buf.screen_row_wraps())
                        .collect();
                    let cutoff = {
                        let refs: Vec<&str> = screen.iter().map(|(row, _)| row.as_str()).collect();
                        crate::chrome::find_empty_input_box_cutoff(&refs)
                    };
                    if let Some(cutoff) = cutoff {
                        screen.truncate(cutoff);
                    }
                    all_lines.extend(screen.into_iter().filter(|(row, _)| !row.is_empty()));
                }
                let known = terminal_secrets(&mut buf);
                let data = crate::redaction::redact_wrapped_rows(all_lines, &known);
                let mut response = serde_json::json!({"data": data, "data_length": data.len(), "cursor": total, "total_written": total, "scrollback_lines": scrollback_lines, "oldest_offset": oldest, "exited": exited});
                add_output_page_metadata(
                    &mut response,
                    args,
                    start as u64,
                    page_end as u64,
                    total as u64,
                    oldest as u64,
                );
                insert_optional_value(
                    response
                        .as_object_mut()
                        .expect("output response is an object"),
                    "exit_code",
                    exit_code.map(serde_json::Value::from),
                );
                return response;
            }
            let ring = match state.session_maps.output_buffers.get(session_id) {
                Some(r) => r,
                None => {
                    return serde_json::json!({
                        "error": "Session not found",
                        "reason": "session_not_found_or_reaped"
                    });
                }
            };
            // Read the whole ring: the `limit` window may cut a secret in two,
            // and the half left in the window can only be scrubbed if the
            // redaction has seen the other half.
            let (all_bytes, total_written, ring_secrets) = {
                let mut ring = ring.lock();
                let (all_bytes, total_written) = ring.read_last(usize::MAX);
                let secrets = ring.cached_secrets(|| {
                    crate::redaction::secrets_in(&String::from_utf8_lossy(&all_bytes))
                });
                (all_bytes, total_written, secrets)
            };
            let oldest = total_written.saturating_sub(all_bytes.len() as u64);
            let requested = args["since_cursor"]
                .as_u64()
                .or_else(|| args["from_byte"].as_u64());
            let offset = requested
                .unwrap_or_else(|| total_written.saturating_sub(limit as u64))
                .clamp(oldest, total_written);
            let mut start = (offset - oldest) as usize;
            let mut end = start.saturating_add(limit).min(all_bytes.len());
            // Source-byte cursors must not split a UTF-8 codepoint across pages.
            // A caller's mid-codepoint start rounds down; our next cursor always
            // points past the whole codepoint. Raw invalid bytes remain lossy text.
            while start > 0 && start < all_bytes.len() && all_bytes[start] & 0xc0 == 0x80 {
                start -= 1;
            }
            while end < all_bytes.len() && all_bytes[end] & 0xc0 == 0x80 {
                end += 1;
            }
            let data = redact_raw_output(
                state,
                session_id,
                &all_bytes,
                start..end,
                ring_secrets,
                requested.is_some(),
            );
            let mut response = serde_json::json!({"data": data, "data_length": data.len(), "cursor": oldest + end as u64, "total_written": total_written, "exited": exited});
            add_output_page_metadata(
                &mut response,
                args,
                oldest + start as u64,
                oldest + end as u64,
                total_written,
                oldest,
            );
            insert_optional_value(
                response
                    .as_object_mut()
                    .expect("output response is an object"),
                "exit_code",
                exit_code.map(serde_json::Value::from),
            );
            response
        }
        "resize" => {
            let resolved = match require_session_id(state, args, "resize") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            let rows = args["rows"].as_u64().unwrap_or(24) as u16;
            let cols = args["cols"].as_u64().unwrap_or(80) as u16;
            if let Err(msg) = super::validate_terminal_size(rows, cols) {
                return serde_json::json!({"error": msg});
            }
            // Same core as HTTP resize_session: grid before SIGWINCH, same-dims no-op.
            match crate::pty::resize_session_core(state, session_id, rows, cols) {
                Ok(Some(frame)) => crate::pty::send_grid_frame(state, session_id, frame),
                Ok(None) => {}
                Err(e) if e.starts_with("Session not found") => {
                    return serde_json::json!({"error": "Session not found"});
                }
                Err(e) => return serde_json::json!({"error": format!("Resize failed: {}", e)}),
            }
            serde_json::json!({"ok": true})
        }
        "rename" => {
            let resolved = match require_session_id(state, args, "rename") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            let name = match args["name"].as_str().map(str::trim) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => {
                    return serde_json::json!({"error": "name (non-empty string) is required for action=rename"});
                }
            };
            if name.chars().count() > 256 || name.chars().any(char::is_control) {
                return serde_json::json!({"error": "name must be one line of at most 256 characters without control characters"});
            }
            let is_custom = args["is_custom"].as_bool().unwrap_or(true);
            if !state.rename_session_from_backend(session_id, name, is_custom) {
                return serde_json::json!({"error": "Session not found"});
            }
            serde_json::json!({"ok": true})
        }
        "close" => {
            let resolved = match require_session_id(state, args, "close") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            // Self-close guard: prevent an agent from closing its own session.
            if let Some(sid) = mcp_session_id
                && let Some(own_pty) = state.mcp.to_session.get(sid)
                && own_pty.value() == session_id
            {
                return serde_json::json!({"error": "Cannot close own session. Use exit to terminate yourself."});
            }
            // Uses the same tombstone path as the Tauri close_pty command so
            // post-mortem MCP reads keep returning final output + exit code.
            // Idempotent: returns ok even if session was already tombstoned.
            let reason = if args.get("reason").and_then(|v| v.as_str()) == Some(IDLE_CLOSE_REASON) {
                IDLE_CLOSE_REASON
            } else {
                "close_requested"
            };
            let existed = crate::pty::close_pty_core_with_reason(state, session_id, false, reason)
                .is_some()
                || state.grid.vt_log_buffers.contains_key(session_id);
            if existed {
                // Notify frontend and SSE consumers so the tab is removed from
                // the UI. Without this the reader thread's EOF-driven
                // session-closed event may never fire (the cloned reader fd
                // keeps the pty master alive after close_pty_core drops it).
                state.emit_pty_event(crate::state::AppEvent::SessionClosed {
                    session_id: session_id.to_string(),
                    reason: "closed".to_string(),
                });
                #[cfg(feature = "desktop")]
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit(
                        "session-closed",
                        serde_json::json!({
                            "session_id": session_id,
                            "reason": "closed",
                        }),
                    );
                }
            }
            // SIMP-1: drain HTML tabs registered by this session and emit close.
            emit_close_html_tabs(state.as_ref(), session_id);
            serde_json::json!({"ok": true})
        }
        "keep_open" => {
            let resolved = match require_session_id(state, args, "keep_open") {
                Ok(id) => id,
                Err(error) => return error,
            };
            let Some(enabled) = args.get("enabled").and_then(serde_json::Value::as_bool) else {
                return serde_json::json!({"error": "enabled (boolean) is required for action=keep_open"});
            };
            if !state.session_maps.session_parent.contains_key(&resolved) {
                return serde_json::json!({"error": "keep_open applies only to managed child sessions"});
            }
            if enabled {
                state.keep_open_sessions.insert(resolved);
            } else {
                state.keep_open_sessions.remove(&resolved);
            }
            serde_json::json!({"ok": true, "keep_open": enabled})
        }
        "kill" => {
            let resolved = match require_session_id(state, args, "kill") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            // Self-kill guard: mirror the close branch — an agent must not SIGKILL itself.
            if let Some(sid) = mcp_session_id
                && let Some(own_pty) = state.mcp.to_session.get(sid)
                && own_pty.value() == session_id
            {
                return serde_json::json!({"error": "Cannot kill own session. Use exit to terminate yourself."});
            }
            if crate::pty::kill_pty_core(state, session_id) {
                state.emit_pty_event(crate::state::AppEvent::SessionClosed {
                    session_id: session_id.to_string(),
                    reason: "killed".to_string(),
                });
                #[cfg(feature = "desktop")]
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit(
                        "session-closed",
                        serde_json::json!({
                            "session_id": session_id,
                            "reason": "killed",
                        }),
                    );
                }
                // SIMP-1: drain HTML tabs registered by this session and emit close.
                emit_close_html_tabs(state, session_id);
                serde_json::json!({"ok": true})
            } else {
                serde_json::json!({"error": "Session not found"})
            }
        }
        "pause" => {
            let resolved = match require_session_id(state, args, "pause") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            let entry = match state.session_maps.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            entry.lock().paused.store(true, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        "resume" => {
            let resolved = match require_session_id(state, args, "resume") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            let entry = match state.session_maps.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            entry.lock().paused.store(false, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        "status" => {
            let resolved = match require_session_id(state, args, "status") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let session_id = resolved.as_str();
            match state.session_state_with_shell(session_id) {
                Some(ss) => {
                    let exit_code = state
                        .session_maps
                        .exit_codes
                        .get(session_id)
                        .map(|e| *e.value());
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let since_ms = state
                        .session_maps
                        .shell_state_since_ms
                        .get(session_id)
                        .map(|a| a.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(0);
                    let elapsed = if since_ms > 0 {
                        now_ms.saturating_sub(since_ms)
                    } else {
                        0
                    };
                    let is_idle = ss.shell_state.as_deref() == Some("idle");
                    let is_busy = ss.shell_state.as_deref() == Some("busy");
                    let delivery_uncertain = state
                        .session_maps
                        .silence_states
                        .get(session_id)
                        .map(|silence| silence.lock().injection_delivery_uncertain)
                        .unwrap_or(false);
                    #[cfg(unix)]
                    let standby = state.session_maps.standby_sessions.contains_key(session_id);
                    #[cfg(not(unix))]
                    let standby = false;
                    let mut response = serde_json::json!({
                        "session_id": session_id,
                        "background_work": ss.background_work,
                        "awaiting_input": ss.awaiting_input,
                        "rate_limited": ss.rate_limited,
                        "delivery_uncertain": delivery_uncertain,
                        "last_activity_ms": ss.last_activity_ms,
                        "standby": standby,
                    });
                    let object = response
                        .as_object_mut()
                        .expect("session status response is an object");
                    insert_optional_value(
                        object,
                        "shell_state",
                        ss.shell_state.map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "agent_state",
                        ss.agent_state.map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "agent_type",
                        ss.agent_type.map(serde_json::Value::String),
                    );
                    insert_optional_value(
                        object,
                        "exit_code",
                        exit_code.map(serde_json::Value::from),
                    );
                    insert_optional_value(
                        object,
                        "idle_since_ms",
                        (is_idle && elapsed > 0).then(|| serde_json::json!(elapsed)),
                    );
                    insert_optional_value(
                        object,
                        "busy_duration_ms",
                        (is_busy && elapsed > 0).then(|| serde_json::json!(elapsed)),
                    );
                    response
                }
                None => serde_json::json!({"error": format!("Session '{}' not found", session_id)}),
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'session'. Available: {}", other, SESSION_ACTIONS
        )}),
    }
}

/// Why a session must not be suspended now, or None. Ending the PTY mid-turn would
/// cut the agent's work or an unanswered question. The UI applies the same rule to
/// the tab (`suspendRefusal` in src/utils/suspendTerminal.ts).
pub(super) fn suspend_refusal(ss: &crate::state::SessionState) -> Option<&'static str> {
    if ss.awaiting_input {
        return Some("waiting for input");
    }
    if ss.queued_commands > 0 {
        return Some("queued commands pending");
    }
    if ss.agent_type.is_some() {
        let working = matches!(ss.agent_state.as_deref(), Some("working" | "starting"));
        if ss.background_work || working {
            return Some("agent working");
        }
    } else if ss.shell_state.as_deref() == Some("busy") {
        return Some("command running");
    }
    None
}

/// Use the same close path as `session action=close`, including frontend events.
/// Logged as the close cause when the idle sweep, not a client, closes a session.
const IDLE_CLOSE_REASON: &str = "idle_close";

pub(crate) fn close_idle_managed_session(state: &Arc<AppState>, session_id: &str) {
    let result = handle_session(
        state,
        &serde_json::json!({"action": "close", "session_id": session_id, "reason": IDLE_CLOSE_REASON}),
        None,
    );
    if result.get("error").is_some() {
        tracing::warn!(session_id, %result, "idle-close session close failed");
    }
}

/// Build the full prompt for a spawned agent.
/// Prepends multi-agent context when the caller is a registered peer so the child
/// knows its identity and how to communicate back. Returns the original prompt
/// unchanged when called outside a multi-agent context (`parent_tuic` is `None`).
pub(super) fn build_spawn_prompt(
    prompt: &str,
    parent_tuic: Option<&str>,
    session_id: &str,
    peer_name: &str,
) -> String {
    let Some(parent) = parent_tuic else {
        return prompt.to_string();
    };
    format!(
        "## TUICommander Multi-Agent Context\n\
         You are operating as a managed peer. There is no separate swarm action; use the agent and session primitives.\n\
         - You are pre-registered as peer `{peer_name}`.\n\
         - Your session ID (`$TUIC_SESSION`): `{session_id}`\n\
         - Your parent agent session: `{parent}`\n\n\
         TUICommander already created your peer identity and inbox. If an MCP\n\
         reconnect reports that you are unregistered, repair the binding with:\n\
         `agent action=register tuic_session=\"{session_id}\" name=\"{peer_name}\"`\n\n\
         You can communicate with your parent at any time, and must report task\n\
         completion or a real blocker with:\n\
         `agent action=send to=\"{parent}\" message=\"<done summary>\"`\n\n\
         ## Your Task\n\n\
         {prompt}"
    )
}

pub(super) fn resolve_effective_spawn_cwd(
    state: &AppState,
    explicit_cwd: Option<&str>,
    managed_parent_cwd: Option<&str>,
    caller_tuic: Option<&str>,
    mcp_session_id: Option<&str>,
) -> Option<String> {
    explicit_cwd
        .map(str::to_string)
        .or_else(|| {
            caller_tuic.and_then(|parent| {
                state
                    .session_maps
                    .sessions
                    .get(parent)
                    .and_then(|session| session.lock().cwd.clone())
            })
        })
        // The bridge header identifies the actual managed PTY when no verified
        // caller binding exists. A conflicting bound caller rejects this hint
        // before resolution reaches this function.
        .or_else(|| managed_parent_cwd.map(str::to_string))
        // Non-managed clients have no PTY header; initialize roots remain the
        // final repo-scoped fallback.
        .or_else(|| {
            mcp_session_id.and_then(|sid| {
                state
                    .mcp
                    .sessions
                    .get(sid)
                    .and_then(|meta| meta.repo_path.clone())
            })
        })
}

/// Build the `agent action=spawn` response.
///
/// Split out of the spawn handler so its size can be measured without launching
/// a process — see `mcp_instruction_surface_bytes_stay_within_budget`. Every
/// field here is a function of the ids passed in: the response carries no static
/// prose block, because the operational workflow it would otherwise repeat lives
/// in `agent(register).workflow`, which the caller has already read.
pub(super) fn spawn_response(
    session_id: &str,
    task_id: &str,
    peer_name: &str,
    spawn_ts: u64,
    caller_tuic: Option<&str>,
    codex_wrapper_warning: Option<&str>,
    prompt_deferred: bool,
) -> serde_json::Value {
    let mut response = serde_json::json!({
        "session_id": session_id,
        "task_id": task_id,
        "poll_interval_ms": TASK_POLL_INTERVAL_MS,
        "name": peer_name,
        // No `peer_registered` / `communication_ready` / `send_to`: the
        // first two are constant for every successful spawn, and the third
        // repeated `session_id` verbatim. `parent_session_id` (or the
        // `communication_warning` in its place) already reports whether
        // child-to-parent messaging is available.
        "server_ts": spawn_ts,
        // No `*_with` call templates: they only restated `session_id` and
        // `server_ts` inside prose the tool description already carries, and
        // were ~70% of every spawn response in recorded orchestrator traces.
    });
    // Only the exceptional case is reported, so the common response stays the
    // size it was. A prompt passed on argv is delivered by definition; a prompt
    // the server must type into a prefill-only TUI is not delivered yet, and a
    // bare success read as "the child has the work" in both cases — which is how
    // a spawn whose prompt was swallowed by a startup dialog looked identical to
    // one that had started working.
    if prompt_deferred && let Some(obj) = response.as_object_mut() {
        obj.insert(
            "prompt_delivery".to_string(),
            serde_json::json!("queued — the child must reach its ready prompt first; a prompt_delivery_failed notice follows if it does not, and carries the prompt for re-delivery"),
        );
    }
    if let Some(warning) = codex_wrapper_warning
        && let Some(obj) = response.as_object_mut()
    {
        obj.insert("launch_warning".to_string(), serde_json::json!(warning));
    }
    if let Some(parent) = caller_tuic
        && let Some(obj) = response.as_object_mut()
    {
        obj.insert("parent_session_id".to_string(), serde_json::json!(parent));
    } else if let Some(obj) = response.as_object_mut() {
        obj.insert(
            "communication_warning".to_string(),
            serde_json::json!(
                "Child has no parent: child-to-parent messaging is unavailable for this spawn."
            ),
        );
    }
    response
}

#[cfg(test)]
pub(super) fn handle_agent(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    handle_agent_with_parent_cwd(state, addr, args, mcp_session_id, None)
}

pub(super) fn handle_agent_with_parent_cwd(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "agent", AGENT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "spawn" => {
            // Agent spawning is restricted to localhost — matches the HTTP route guard in agent_routes.rs
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "Agent spawning is restricted to localhost connections"});
            }
            let keep_open = match args.get("keep_open") {
                None => false,
                Some(value) => match value.as_bool() {
                    Some(value) => value,
                    None => return serde_json::json!({"error": "keep_open must be a boolean"}),
                },
            };
            for removed in ["allow_alt_screen", "allowAltScreen"] {
                if args.get(removed).is_some() {
                    return serde_json::json!({"error": format!("Removed agent spawn parameter: {removed}; configure prevent_alt_screen for the agent instead")});
                }
            }
            let prompt = match args["prompt"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "Action 'spawn' requires 'prompt'"}),
            };
            let caller_env: std::collections::HashMap<String, String> = match args.get("env") {
                Some(value) => match serde_json::from_value(value.clone()) {
                    Ok(env) => env,
                    Err(_) => {
                        return serde_json::json!({"error": "Action 'spawn' requires 'env' to be a map of string values"});
                    }
                },
                None => Default::default(),
            };
            let pty_description = match parse_pty_description(args) {
                Ok(update) => resolve_spawn_pty_description(update, &prompt),
                Err(error) => return error,
            };
            if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
                return serde_json::json!({"error": "Max concurrent sessions reached"});
            }

            // Resolve agent binary — run config name takes priority, then literal agent type
            let agents_cfg = crate::config::load_agents_config();
            let (binary_path, resolved) = if let Some(path) = args["binary_path"].as_str() {
                let expanded = crate::cli::expand_tilde(path);
                let p = std::path::Path::new(&expanded);
                if !p.is_absolute() {
                    return serde_json::json!({"error": "binary_path must be an absolute path"});
                }
                if !p.is_file() {
                    return serde_json::json!({"error": "binary_path does not point to an existing file"});
                }
                (expanded, None)
            } else {
                let agent_type_raw = args["agent_type"].as_str().unwrap_or("claude");
                let rc = resolve_run_config(agent_type_raw, &agents_cfg);
                let bin_raw = rc.command.as_deref().unwrap_or(&rc.agent_type);
                let bin = crate::cli::expand_tilde(bin_raw);
                let detection = crate::agent::detect_agent_binary_sync(bin.clone());
                match detection.path {
                    Some(p) => (p, Some(rc)),
                    None => {
                        return serde_json::json!({"error": format!("Agent binary '{}' not found", bin)});
                    }
                }
            };

            let rows = args["rows"].as_u64().unwrap_or(24) as u16;
            let cols = args["cols"].as_u64().unwrap_or(80) as u16;
            if let Err(msg) = super::validate_terminal_size(rows, cols) {
                return serde_json::json!({"error": msg});
            }

            // Canonical agent type for this spawn: a resolved direct Codex
            // executable wins over the configured bucket; otherwise use the run
            // config key or raw agent_type. Pre-set below so argv finalization,
            // parser gates, hooks, and session events share one CLI identity.
            let configured_agent_type: Option<String> = resolved
                .as_ref()
                .map(|rc| rc.agent_type.clone())
                .or_else(|| args["agent_type"].as_str().map(|s| s.to_string()));
            let effective_agent_type =
                resolve_spawn_agent_type(&binary_path, configured_agent_type.as_deref());
            let effective_model = args["model"]
                .as_str()
                .or_else(|| resolved.as_ref().and_then(|rc| rc.model.as_deref()));
            let codex_wrapper_warning =
                codex_wrapper_launch_warning(effective_agent_type.as_deref(), &binary_path);

            let requested_name = match args.get("name") {
                Some(value) => match value.as_str().map(str::trim) {
                    Some("") | None => {
                        return serde_json::json!({"error": "Action 'spawn' requires 'name' to be a non-empty string when provided"});
                    }
                    Some(name) => Some(name.to_string()),
                },
                None => None,
            };
            let peer_name = requested_name
                .clone()
                .unwrap_or_else(|| "agent".to_string());

            let session_id = Uuid::new_v4().to_string();

            // Resolve caller's tuic_session from their MCP session via the O(1) reverse map.
            // Only set when caller is a registered peer — drives multi-agent context + TUIC_PARENT.
            let caller_tuic: Option<String> = mcp_session_id
                .and_then(|sid| state.mcp.to_session.get(sid).map(|e| e.value().clone()));

            // Effective prompt: context prepended for managed-peer spawns, unchanged otherwise.
            let effective_prompt =
                build_spawn_prompt(&prompt, caller_tuic.as_deref(), &session_id, &peer_name);

            // Effective cwd: an explicit `cwd` arg wins; otherwise inherit the SPAWNING
            // agent's working dir (its PTY session's cwd). Without this the child runs in
            // the TUIC process's own cwd AND its session carries no cwd, so the frontend
            // `session-created` handler can't match it to the parent's repo and drops the
            // tab into whatever repo the desktop user has focused (the active-repo
            // fallback). Inheriting the parent cwd lands both the process and the tab in
            // the parent agent's repo.
            let effective_cwd = resolve_effective_spawn_cwd(
                state,
                args["cwd"].as_str(),
                managed_parent_cwd,
                caller_tuic.as_deref(),
                mcp_session_id,
            );

            let mut cmd = CommandBuilder::new(&binary_path);
            crate::pty::sanitize_pty_parent_env(&mut cmd);

            let mut screen_env = resolved
                .as_ref()
                .map(|rc| rc.env.clone())
                .unwrap_or_default();
            // Run-config values are defaults; caller values take precedence.
            if let Some(ref rc) = resolved {
                for (k, v) in &rc.env {
                    cmd.env(k, v);
                }
            }
            for (k, v) in &caller_env {
                if k == "TUIC_SESSION" || k == "TUIC_PARENT" {
                    continue;
                }
                cmd.env(k, v);
                screen_env.insert(k.clone(), v.clone());
            }
            if let Err(error) =
                apply_managed_claude_tmpdir(&mut cmd, effective_agent_type.as_deref())
            {
                return serde_json::json!({"error": error});
            }
            let mut env_keys: Vec<_> = screen_env.keys().collect();
            env_keys.sort();
            tracing::debug!(
                ?env_keys,
                "MCP agent spawn environment applied (values redacted)"
            );
            // Peer identity always wins over config and caller environment.
            if let Some(ref parent) = caller_tuic {
                cmd.env("TUIC_PARENT", parent);
            } else {
                cmd.env_remove("TUIC_PARENT");
            }
            crate::pty::bind_pty_identity(state, &mut cmd, &session_id, None);
            crate::pty::apply_agent_screen_env(&mut cmd, &screen_env);

            // Initial prompt withheld from argv for prefill-only TUIs (codex):
            // queued into pending_injections after session registration below.
            let mut deferred_initial_prompt: Option<String> = None;
            let mut launch_args: Vec<String> = Vec::new();

            if let Some(raw_args) = args.get("args").and_then(|a| a.as_array()) {
                // Explicit args remain authoritative when they contain
                // `{prompt}` (for example `codex exec {prompt}`). When they are
                // flags only, the required spawn prompt must still be delivered:
                // append it for normal CLIs, or defer it through PTY injection
                // for prefill-only TUIs such as interactive Codex.
                let explicit_args: Vec<String> = raw_args
                    .iter()
                    .filter_map(|arg| arg.as_str().map(ToOwned::to_owned))
                    .collect();
                let agent_type = effective_agent_type.as_deref().unwrap_or_default();
                let (final_args, deferred) = match compose_mcp_spawn_args(McpSpawnArgs {
                    agent_type,
                    args: &explicit_args,
                    prompt: &effective_prompt,
                    model: effective_model,
                    print_mode: args["print_mode"].as_bool().unwrap_or(false),
                    output_format: args["output_format"].as_str(),
                    default_template: false,
                }) {
                    Ok(m) => m,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                deferred_initial_prompt = deferred;
                launch_args.extend(final_args);
            } else if let Some(ref rc) = resolved {
                if let Some(ref rc_args) = rc.args {
                    // Run config matched: user-authored argv remains authoritative.
                    // Merge structured MCP params, then preserve prompt substitution
                    // or positional append semantics. In particular, wrapper and
                    // subcommand configs must not be rewritten into PTY delivery.
                    let agent_type = effective_agent_type.as_deref().unwrap_or_default();
                    if rc.default_config && is_direct_codex_executable(&binary_path) {
                        let (final_args, deferred) = match compose_mcp_spawn_args(McpSpawnArgs {
                            agent_type,
                            args: rc_args,
                            prompt: &effective_prompt,
                            model: effective_model,
                            print_mode: args["print_mode"].as_bool().unwrap_or(false),
                            output_format: args["output_format"].as_str(),
                            default_template: false,
                        }) {
                            Ok(args) => args,
                            Err(error) => return serde_json::json!({"error": error}),
                        };
                        deferred_initial_prompt = deferred;
                        launch_args.extend(final_args);
                    } else {
                        let final_args = match compose_mcp_run_config_args(
                            agent_type,
                            rc_args,
                            &effective_prompt,
                            effective_model,
                            args["print_mode"].as_bool().unwrap_or(false),
                            args["output_format"].as_str(),
                        ) {
                            Ok(m) => m,
                            Err(e) => return serde_json::json!({"error": e}),
                        };
                        launch_args.extend(final_args);
                    }
                } else {
                    // No run config args: use the built-in per-agent template
                    // (mirrors the shipped frontend spawnArgs) so cross-agent
                    // spawns work out of the box; only truly unknown agents fail,
                    // with a copy-pasteable example. Claude rides the same table
                    // (story 092) — merge's claude flags-first rule keeps its
                    // argv byte-identical to the retired dedicated branch.
                    let agent_type = effective_agent_type.as_deref().unwrap_or_default();
                    match crate::agent::default_prompt_args(agent_type) {
                        Some(template) => {
                            let (final_args, deferred) =
                                match compose_mcp_spawn_args(McpSpawnArgs {
                                    agent_type,
                                    args: &template,
                                    prompt: &effective_prompt,
                                    model: effective_model,
                                    print_mode: args["print_mode"].as_bool().unwrap_or(false),
                                    output_format: args["output_format"].as_str(),
                                    default_template: true,
                                }) {
                                    Ok(m) => m,
                                    Err(e) => return serde_json::json!({"error": e}),
                                };
                            deferred_initial_prompt = deferred;
                            launch_args.extend(final_args);
                        }
                        None => {
                            return serde_json::json!({"error": format!(
                                "Don't know how to spawn agent '{name}' with a prompt. Pass explicit args with a {{prompt}} placeholder, e.g. args=[\"--message\", \"{{prompt}}\"], or configure a run config named '{name}' in Settings -> Agents.",
                                name = rc.agent_type
                            )});
                        }
                    }
                }
            } else {
                // No run config, no explicit args — default MCP param logic
                if is_direct_codex_executable(&binary_path) {
                    let template = crate::agent::default_prompt_args("codex").unwrap_or_default();
                    let (final_args, deferred) = match compose_mcp_spawn_args(McpSpawnArgs {
                        agent_type: "codex",
                        args: &template,
                        prompt: &effective_prompt,
                        model: effective_model,
                        print_mode: args["print_mode"].as_bool().unwrap_or(false),
                        output_format: args["output_format"].as_str(),
                        default_template: true,
                    }) {
                        Ok(m) => m,
                        Err(e) => return serde_json::json!({"error": e}),
                    };
                    deferred_initial_prompt = deferred;
                    launch_args.extend(final_args);
                } else {
                    if args["print_mode"].as_bool().unwrap_or(false) {
                        launch_args.push("--print".to_string());
                    }
                    if let Some(format) = args["output_format"].as_str() {
                        launch_args.push("--output-format".to_string());
                        launch_args.push(format.to_string());
                    }
                    if let Some(model) = effective_model {
                        launch_args.push("--model".to_string());
                        launch_args.push(model.to_string());
                    }
                    launch_args.push(effective_prompt.clone());
                }
            }
            if let Some(agent_type) = effective_agent_type.as_deref() {
                launch_args = crate::agent_hook_launch::augment_args(
                    agent_type,
                    &binary_path,
                    &launch_args,
                    &crate::config::config_dir(),
                );
            }
            let skip_trust_dialog = effective_agent_type
                .as_deref()
                .and_then(|agent_type| agents_cfg.agents.get(agent_type))
                .and_then(|settings| settings.skip_trust_dialog)
                .unwrap_or(true);
            if effective_agent_type.as_deref() == Some("codex") {
                // A pending Codex update shows an Update now / Skip prompt before
                // the composer, and a managed child waits on it forever (1373).
                launch_args.insert(0, "-c".to_string());
                launch_args.insert(1, "check_for_update_on_startup=false".to_string());
            }
            if skip_trust_dialog && effective_agent_type.as_deref() == Some("codex") {
                let cwd = effective_cwd
                    .as_deref()
                    .map(crate::cli::expand_tilde)
                    .map(std::path::PathBuf::from)
                    .or_else(|| std::env::current_dir().ok());
                let Some(cwd) = cwd else {
                    return serde_json::json!({"error": "Cannot resolve Codex spawn cwd for trust override"});
                };
                let cwd = match std::fs::canonicalize(&cwd) {
                    Ok(path) => path,
                    Err(error) => {
                        return serde_json::json!({"error": format!("Cannot resolve Codex spawn cwd for trust override: {error}")});
                    }
                };
                launch_args.insert(0, "-c".to_string());
                launch_args.insert(
                    1,
                    // Inline table, not a dotted key: Codex splits a `-c` key on every
                    // `.`, so `projects."<cwd>".trust_level` never matches the path
                    // (every real path holds a dot) and the trust prompt stays.
                    format!(
                        "projects={{{}={{trust_level=\"trusted\"}}}}",
                        serde_json::to_string(&crate::fs::portable_spelling(
                            &cwd.to_string_lossy()
                        ))
                        .expect("path serializes")
                    ),
                );
            }
            let launch_receipt = crate::prompt_receipt::PromptReceipt::capture(
                &effective_prompt,
                "TUIC managed spawn / build_spawn_prompt",
                &launch_args,
                effective_cwd.as_deref(),
                deferred_initial_prompt.is_some(),
            );
            for arg in launch_args {
                cmd.arg(arg);
            }
            if let Some(ref cwd) = effective_cwd {
                cmd.cwd(crate::cli::expand_tilde(cwd));
            }

            // The argv assembly above can bail out with an error response, so it
            // cannot live inside the retry closure; the built command is cloned
            // per attempt instead (`spawn_command` consumes it).
            let (pair, child) = match crate::pty::spawn_pty_pair_with_retry(
                PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                || cmd.clone(),
            ) {
                Ok(pair_and_child) => pair_and_child,
                Err(e) => {
                    state.unbind_live_pty(&session_id);
                    return serde_json::json!({"error": e});
                }
            };
            let writer = match pair.master.take_writer() {
                Ok(w) => w,
                Err(e) => {
                    state.unbind_live_pty(&session_id);
                    return serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)});
                }
            };
            let reader = match pair.master.try_clone_reader() {
                Ok(r) => r,
                Err(e) => {
                    state.unbind_live_pty(&session_id);
                    return serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)});
                }
            };

            let paused = Arc::new(AtomicBool::new(false));
            // Pre-set the session's agent type so agent_active_for_parse is true
            // from the first output chunk and intent/suggest tokens are parsed
            // without waiting on foreground polling. Seeded before registration
            // because that is what publishes `session-created`.
            let mut session_state = crate::state::SessionState {
                spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
                ..Default::default()
            };
            if effective_agent_type.is_some() {
                session_state.hook_instrumented =
                    crate::pty::hook_instrumented_for(&agents_cfg, effective_agent_type.as_deref());
                session_state.seed_configured_agent(effective_agent_type.clone());
            }
            state
                .session_maps
                .session_states
                .insert(session_id.clone(), session_state);
            // Prefill-only TUIs (codex): the task was withheld from argv — queue it
            // now so the BUSY→IDLE flush types it (text + CR) the moment the child's
            // TUI reaches its ready prompt. Queued AFTER session_states is inserted:
            // flush_pending_injections requires agent_type to treat this session as
            // an injectable agent. Same delivery path as peer messages (story 091).
            let prompt_deferred = deferred_initial_prompt.is_some();
            if let Some(initial_prompt) = deferred_initial_prompt {
                state.pending_initial_prompts.insert(
                    session_id.clone(),
                    crate::state::PendingInitialPrompt::new(initial_prompt.clone()),
                );
                state
                    .pending_injections
                    .entry(session_id.clone())
                    .or_default()
                    .push_back(crate::state::PendingInjection::initial_prompt(
                        initial_prompt,
                    ));
            }
            // Resolved before the session-created broadcast so the event names the
            // parent; the parent map entry below uses the same value.
            let spawn_parent = caller_tuic
                .clone()
                .or_else(|| mcp_session_id.map(pending_parent_id));
            // What the UI is told. A placeholder matches no tab, and nothing
            // corrects the tab once `register` resolves it; the session row does,
            // on the next reload.
            // DEFERRED (2026-09-23) — push the resolved parent live when
            // `link_pending_children_to_parent` swaps the placeholder. Needs a new
            // bus event, SSE arm and desktop emit; until then the tag shows only
            // after a reload for a caller that spawns before it registers.
            let published_parent = spawn_parent.clone().filter(|p| !is_pending_parent(p));
            // Buffers, alias, metrics, grid watch and the session-created
            // broadcast, sharing one helper with session::spawn_pty_session so the
            // VT screen can only ever be built at the geometry the PTY was opened
            // with.
            super::session::register_pty_session(
                state,
                &session_id,
                PtySession {
                    launch_receipt: Some(launch_receipt),
                    writer: Arc::new(Mutex::new(writer)),
                    master: pair.master,
                    _child: child,
                    paused: paused.clone(),
                    worktree: None,
                    initial_cwd: effective_cwd.clone(),
                    cwd: effective_cwd.clone(),
                    display_name: requested_name.clone(),
                    display_name_is_custom: false,
                    display_name_from_spawn: requested_name.is_some(),
                    is_remote: true,
                    shell: binary_path.clone(),
                },
                rows,
                cols,
                effective_agent_type.clone(),
                None,
                published_parent.clone(),
            );
            crate::prompt_receipt::adopt_mcp_instructions(state, &session_id);
            let cwd_str = effective_cwd.clone();

            #[cfg(feature = "desktop")]
            {
                let print_mode = args["print_mode"].as_bool().unwrap_or(false);
                let app_handle = state.app_handle.read().clone();
                if !print_mode && let Some(ref app) = app_handle {
                    let agent_type_val = effective_agent_type.as_deref();
                    let _ = app.emit(
                        "session-created",
                        serde_json::json!({
                            "session_id": session_id,
                            "cwd": cwd_str,
                            "agent_type": agent_type_val,
                            "display_name": requested_name,
                            "parent_session": published_parent,
                        }),
                    );
                }
            }
            state.set_pty_description(&session_id, pty_description);
            if skip_trust_dialog && effective_agent_type.as_deref() == Some("claude") {
                state.managed_trust_dialogs.insert(session_id.clone());
            }
            spawn_reader_thread(reader, paused, session_id.clone(), state.clone(), None);

            // Every managed child is a peer immediately, independent of whether
            // its initial prompt runs or its own MCP bridge has connected yet.
            state.peer_agents.insert(
                session_id.clone(),
                crate::state::PeerAgent {
                    tuic_session: session_id.clone(),
                    mcp_session_id: String::new(), // filled when child connects via MCP
                    name: peer_name.clone(),
                    project: effective_cwd.clone(),
                    registered_at: now_unix_ms(),
                },
            );
            state.agent_inbox.entry(session_id.clone()).or_default();

            // Bidirectional communication additionally needs an identified
            // parent. The child receives TUIC_PARENT + the spawn preamble; the
            // parent receives the child target in the response below.
            if let Some(parent_id) = spawn_parent {
                journal_hand_off(
                    state,
                    crate::progress::ProgressKind::Delegated,
                    &parent_id,
                    &session_id,
                    Some(&peer_name),
                    &prompt,
                );
                state
                    .session_maps
                    .session_parent
                    .insert(session_id.clone(), parent_id);
                if keep_open {
                    state.keep_open_sessions.insert(session_id.clone());
                }
                if state.pending_initial_prompts.contains_key(&session_id) {
                    let watchdog_state = Arc::clone(state);
                    let watchdog_session = session_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(INITIAL_PROMPT_DELIVERY_TIMEOUT).await;
                        crate::pty::notify_initial_prompt_timeout_if_pending(
                            &watchdog_state,
                            &watchdog_session,
                        );
                    });
                }
            }

            let spawn_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            // Durable handle for this spawn. Created only after the PTY is live, so
            // every early return above (loopback guard, bad binary, spawn failure)
            // leaves no task behind. Purely additive in the response: classic MCP
            // clients that ignore `task_id` keep working exactly as before.
            let task_id = state.tasks.create(
                crate::tasks::TaskKind::AgentSpawn,
                &task_owner_identity(caller_tuic.as_deref(), mcp_session_id),
                Some(&session_id),
            );

            spawn_response(
                &session_id,
                &task_id,
                &peer_name,
                spawn_ts,
                caller_tuic.as_deref(),
                codex_wrapper_warning.as_deref(),
                prompt_deferred,
            )
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'agent'. Available: {}", other, AGENT_ACTIONS
        )}),
    }
}

/// Poll or cancel a task handle. Non-blocking by design: this is what lifts the
/// 300s ceiling on `agent wait`, so it must never wait on anything itself.
pub(super) fn handle_task(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "task", TASK_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let task_id = match require_string(args, "task_id") {
        Ok(id) => id,
        Err(e) => return e,
    };
    if !matches!(action, "get" | "cancel") {
        return serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'task'. Available: {}", action, TASK_ACTIONS
        )});
    }

    let caller_tuic: Option<String> =
        mcp_session_id.and_then(|sid| state.mcp.to_session.get(sid).map(|e| e.value().clone()));
    let identities = caller_task_identities(caller_tuic.as_deref(), mcp_session_id);

    // A task handle is a capability over a spawned agent, so ownership is checked
    // before any state is returned or mutated — one agent must not be able to
    // inspect or cancel another's children.
    let rec = match state.tasks.get(task_id) {
        Some(rec) if identities.contains(&rec.owner) => rec,
        Some(_) => {
            return serde_json::json!({"error": "task_id is not owned by the calling identity"});
        }
        None => return serde_json::json!({"error": "unknown or expired task_id"}),
    };

    if action == "cancel" {
        // Same loopback restriction as `agent spawn`: cancelling mutates another
        // agent's orchestration state, so a remote MCP client must not reach it.
        if !addr.ip().is_loopback() {
            return serde_json::json!({
                "error": "Task cancellation is restricted to localhost connections"
            });
        }
        return match state.tasks.cancel(&rec.task_id) {
            Ok(()) => serde_json::json!({
                "task_id": rec.task_id,
                "status": crate::tasks::TaskStatus::Cancelled.as_str(),
                "cancelled": true,
                "note": "The agent process is untouched — use session(action=kill) to stop it.",
            }),
            // Already finished: report the state that stands rather than an error,
            // so a cancel racing the agent's exit is not a failure for the caller.
            Err(crate::tasks::TaskError::Terminal(status)) => serde_json::json!({
                "task_id": rec.task_id,
                "status": status.as_str(),
                "cancelled": false,
                "note": "Task already finished; terminal states are immutable.",
            }),
            Err(crate::tasks::TaskError::NotFound) => {
                serde_json::json!({"error": "unknown or expired task_id"})
            }
        };
    }

    // Absent optional fields are omitted, not null — same convention as session
    // and agent responses.
    let mut response = serde_json::json!({
        "task_id": rec.task_id,
        "status": rec.status.as_str(),
        "poll_interval_ms": TASK_POLL_INTERVAL_MS,
    });
    let obj = response
        .as_object_mut()
        .expect("literal above is an object");
    if let Some(message) = rec.status_message {
        obj.insert("status_message".to_string(), serde_json::json!(message));
    }
    if let Some(result) = rec.result {
        obj.insert("result".to_string(), result);
    }
    if let Some(error) = rec.error {
        obj.insert("error_detail".to_string(), serde_json::json!(error));
    }
    response
}

/// Merged agent tool: original agent actions + messaging actions.
#[cfg(test)]
pub(super) fn handle_agent_unified(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    handle_agent_unified_with_parent_cwd(state, addr, args, mcp_session_id, None)
}

pub(super) fn handle_agent_unified_with_parent_cwd(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "agent", AGENT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "spawn" => {
            handle_agent_with_parent_cwd(state, addr, args, mcp_session_id, managed_parent_cwd)
        }
        "register" | "list_peers" | "send" | "inbox" => {
            // Inter-agent messaging is same-machine coordination only, so it carries
            // the same loopback restriction as `spawn`. Without this, a non-loopback
            // MCP client — whether Basic-Auth'd remotely or admitted via lan_auth_bypass —
            // could register a peer identity, enumerate peers, or inject a message that
            // lands verbatim in another agent's context. Loopback (incl. the local
            // Unix socket, injected as 127.0.0.1 upstream) is the trust boundary here.
            if !addr.ip().is_loopback() {
                return serde_json::json!({
                    "error": "Inter-agent messaging is restricted to localhost connections"
                });
            }
            handle_messaging(state, args, mcp_session_id)
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'agent'. Available: {}", other, AGENT_ACTIONS
        )}),
    }
}

// ---------------------------------------------------------------------------
// Run config resolution
// ---------------------------------------------------------------------------

/// Result of resolving an `agent_type` string against the agents config.
/// When a run config matches, command/args/env override the agent binary defaults.
#[derive(Debug, Clone)]
pub(super) struct ResolvedRunConfig {
    /// The canonical agent type key (e.g. "claude", "codex").
    pub(super) agent_type: String,
    /// Override command from the matched run config, if any.
    pub(super) command: Option<String>,
    /// Override args from the matched run config, if any.
    pub(super) args: Option<Vec<String>>,
    pub(super) model: Option<String>,
    /// Env vars from the matched run config, if any.
    pub(super) env: std::collections::HashMap<String, String>,
    /// Literal Codex selects its default, preserving interactive task delivery.
    default_config: bool,
}

/// Resolve an `agent_type` parameter as either:
/// 1. A run config name (case-insensitive match across all enabled agents), or
/// 2. A literal agent type / binary name.
///
/// Returns `ResolvedRunConfig` with overrides when a run config matches,
/// or just the agent_type passthrough when it doesn't.
pub(super) fn resolve_run_config(
    agent_type: &str,
    agents_cfg: &crate::config::AgentsConfig,
) -> ResolvedRunConfig {
    let needle = agent_type.to_ascii_lowercase();

    // Pass 1: try to match as a run config name across all agents
    for (agent_key, settings) in &agents_cfg.agents {
        for cfg in &settings.run_configs {
            if cfg.name.to_ascii_lowercase() == needle {
                return ResolvedRunConfig {
                    agent_type: agent_key.clone(),
                    command: Some(cfg.command.clone()),
                    args: Some(cfg.args.clone()),
                    model: cfg.model.clone(),
                    env: cfg.env.clone(),
                    default_config: false,
                };
            }
        }
    }

    // Literal Codex consumes the same visible default used by terminal menus.
    if needle == "codex"
        && let Some(settings) = agents_cfg.agents.get("codex")
        && let Some(cfg) = settings
            .run_configs
            .iter()
            .find(|cfg| cfg.is_default)
            .or_else(|| settings.run_configs.first())
    {
        return ResolvedRunConfig {
            agent_type: "codex".into(),
            command: Some(cfg.command.clone()),
            args: Some(cfg.args.clone()),
            model: cfg.model.clone(),
            env: cfg.env.clone(),
            default_config: true,
        };
    }

    // Pass 2: treat other literal agent types as before (no run config overrides)
    ResolvedRunConfig {
        agent_type: agent_type.to_string(),
        command: None,
        args: None,
        model: None,
        env: Default::default(),
        default_config: false,
    }
}

/// Substitute `{prompt}` placeholders in args, or append prompt as last arg.
pub(super) fn substitute_prompt_in_args(args: &[String], prompt: &str) -> Vec<String> {
    let has_placeholder = args.iter().any(|a| a.contains("{prompt}"));
    if has_placeholder {
        args.iter().map(|a| a.replace("{prompt}", prompt)).collect()
    } else {
        let mut result: Vec<String> = args.to_vec();
        result.push(prompt.to_string());
        result
    }
}

/// Finalize caller-supplied argv without silently dropping the required task.
/// A `{prompt}` placeholder is an explicit delivery decision and is preserved
/// verbatim after substitution. Flags-only argv inherits the built-in behavior:
/// prefill-only agents receive the task through deferred PTY injection; other
/// agents receive it as the final positional argument.
pub(super) fn finalize_explicit_spawn_args(
    agent_type: &str,
    explicit: &[String],
    prompt: &str,
) -> (Vec<String>, Option<String>) {
    if explicit.iter().any(|arg| arg.contains("{prompt}")) {
        return (substitute_prompt_in_args(explicit, prompt), None);
    }
    // Value-taking root options from installed `codex --help` (2026-10-04).
    // Attached values (`--profile=review`, `-preview`) stay in the option token.
    const CODEX_VALUE_OPTIONS: &[&str] = &[
        "-c",
        "--config",
        "--enable",
        "--disable",
        "--remote",
        "--remote-auth-token-env",
        "-i",
        "--image",
        "-m",
        "--model",
        "--local-provider",
        "-p",
        "--profile",
        "-s",
        "--sandbox",
        "-C",
        "--cd",
        "--add-dir",
        "-a",
        "--ask-for-approval",
    ];
    let mut codex_subcommand = false;
    if agent_type == "codex" {
        let mut args = explicit.iter();
        while let Some(arg) = args.next() {
            if arg == "--" {
                break;
            }
            if CODEX_VALUE_OPTIONS.contains(&arg.as_str()) {
                args.next();
            } else if !arg.starts_with('-') {
                codex_subcommand = matches!(arg.as_str(), "exec" | "e" | "review");
                break;
            }
        }
    }
    if crate::agent::prompt_prefill_only(agent_type)
        && !(agent_type == "codex"
            && (explicit.first().is_some_and(|arg| !arg.starts_with('-')) || codex_subcommand))
    {
        return (explicit.to_vec(), Some(prompt.to_string()));
    }
    (substitute_prompt_in_args(explicit, prompt), None)
}

/// Final argv + optional deferred initial prompt for an orchestrated agent spawn.
///
/// For prefill-only TUIs (`crate::agent::prompt_prefill_only`, e.g. codex) the
/// task must NOT ride in argv — it prefills the interactive input without
/// submitting, parking the child forever (story 091). Every argv element
/// carrying `{prompt}` is dropped and the prompt is returned separately for the
/// caller to queue as a pending injection, delivered (text + CR) on the child's
/// first idle. All other agents keep the normal placeholder substitution.
///
/// Applied ONLY to the built-in `default_prompt_args` template path — a
/// user-authored run config (e.g. codex `["exec", "{prompt}"]`) is authoritative
/// and must never be rewritten behind the user's back.
pub(super) fn finalize_spawn_args(
    agent_type: &str,
    merged: &[String],
    prompt: &str,
) -> (Vec<String>, Option<String>) {
    if crate::agent::prompt_prefill_only(agent_type) {
        let argv = merged
            .iter()
            .filter(|a| !a.contains("{prompt}"))
            .cloned()
            .collect();
        (argv, Some(prompt.to_string()))
    } else {
        (substitute_prompt_in_args(merged, prompt), None)
    }
}

/// Merge MCP params (model, print_mode, output_format) into run config args.
/// Returns Ok(merged args) or Err(conflict description).
///
/// `agent_type` gates the Claude-only flags: `--print` and `--output-format` are
/// understood only by the `claude` CLI. For any other agent (codex, gemini,
/// goose, …) they are DROPPED with a `warn` — injecting them makes the child
/// clap-exit 2 and the spawn silently fails (todo.md O5). `--model` is generic
/// and passed through for every agent.
///
/// Placement: when `default_template` is true (args came from
/// `default_prompt_args`, not a user run config) claude's flags go FIRST, in
/// `--print`, `--output-format`, `--model` order — byte-identical to the retired
/// dedicated claude spawn branch, whose argv put every flag before the
/// positional prompt (story 092). Everything else — every other agent AND every
/// user-authored run config (whose args may start with a wrapper subcommand
/// flags must not precede) — keeps flags appended, as before.
use crate::config::CODEX_BYPASS_ARG;

pub(super) fn is_direct_codex_executable(binary_path: &str) -> bool {
    let file_name = binary_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(binary_path);
    std::path::Path::new(file_name)
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("codex"))
}

pub(super) fn resolve_spawn_agent_type(
    binary_path: &str,
    configured: Option<&str>,
) -> Option<String> {
    if is_direct_codex_executable(binary_path) {
        Some("codex".to_string())
    } else {
        configured.map(str::to_string)
    }
}

/// Keep Claude's harness-owned background output under Gits unless the user
/// supplied a location through inherited, run-config or caller environment.
pub(super) fn apply_managed_claude_tmpdir(
    cmd: &mut CommandBuilder,
    agent_type: Option<&str>,
) -> Result<(), String> {
    if agent_type != Some("claude") || cmd.get_env("CLAUDE_CODE_TMPDIR").is_some() {
        return Ok(());
    }
    let home = cmd
        .get_env("HOME")
        .map(std::path::PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or("Cannot determine home directory for Claude background output")?;
    let path = home.join("Gits/.tmp/claude");
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("Cannot create Claude background output directory: {error}"))?;
    cmd.env("CLAUDE_CODE_TMPDIR", path.as_os_str());
    Ok(())
}

fn codex_wrapper_launch_warning(
    effective_agent_type: Option<&str>,
    binary_path: &str,
) -> Option<String> {
    (effective_agent_type == Some("codex") && !is_direct_codex_executable(binary_path)).then(|| {
        format!(
            "Codex run config command '{}' is a wrapper; TUIC did not inject {CODEX_BYPASS_ARG} and cannot validate whether the wrapper enables it internally.",
            binary_path
        )
    })
}

pub(super) struct McpSpawnArgs<'a> {
    pub(super) agent_type: &'a str,
    pub(super) args: &'a [String],
    pub(super) prompt: &'a str,
    pub(super) model: Option<&'a str>,
    pub(super) print_mode: bool,
    pub(super) output_format: Option<&'a str>,
    pub(super) default_template: bool,
}

pub(super) fn compose_mcp_spawn_args(
    spawn: McpSpawnArgs<'_>,
) -> Result<(Vec<String>, Option<String>), String> {
    let merged = merge_mcp_params_into_args(
        spawn.agent_type,
        spawn.args,
        spawn.model,
        spawn.print_mode,
        spawn.output_format,
        spawn.default_template,
    )?;
    if spawn.default_template {
        Ok(finalize_spawn_args(spawn.agent_type, &merged, spawn.prompt))
    } else {
        Ok(finalize_explicit_spawn_args(
            spawn.agent_type,
            &merged,
            spawn.prompt,
        ))
    }
}

fn compose_mcp_run_config_args(
    agent_type: &str,
    args: &[String],
    prompt: &str,
    model: Option<&str>,
    print_mode: bool,
    output_format: Option<&str>,
) -> Result<Vec<String>, String> {
    let merged =
        merge_mcp_params_into_args(agent_type, args, model, print_mode, output_format, false)?;
    Ok(substitute_prompt_in_args(&merged, prompt))
}

pub(super) fn merge_mcp_params_into_args(
    agent_type: &str,
    args: &[String],
    model: Option<&str>,
    print_mode: bool,
    output_format: Option<&str>,
    default_template: bool,
) -> Result<Vec<String>, String> {
    let is_claude = agent_type == "claude";
    let base_args = args.to_vec();
    let mut flags: Vec<String> = Vec::new();

    if print_mode {
        if !is_claude {
            tracing::warn!(
                agent_type,
                "Dropping Claude-only MCP param print_mode (--print) for non-claude agent"
            );
        } else if !base_args.iter().any(|a| a.starts_with("--print")) {
            flags.push("--print".to_string());
        }
    }

    if let Some(fmt) = output_format {
        if !is_claude {
            tracing::warn!(
                agent_type,
                output_format = fmt,
                "Dropping Claude-only MCP param output_format (--output-format) for non-claude agent"
            );
        } else if base_args.iter().any(|a| a.starts_with("--output-format")) {
            return Err(format!(
                "Conflict: run config already contains --output-format but MCP param output_format=\"{}\" was also passed",
                fmt
            ));
        } else {
            flags.push("--output-format".to_string());
            flags.push(fmt.to_string());
        }
    }

    if let Some(model_val) = model {
        if base_args.iter().any(|a| a.starts_with("--model")) {
            return Err(format!(
                "Conflict: run config already contains --model but MCP param model=\"{}\" was also passed",
                model_val
            ));
        }
        flags.push("--model".to_string());
        flags.push(model_val.to_string());
    }

    let merged = if is_claude && default_template {
        let mut m = flags;
        m.extend(base_args);
        m
    } else {
        let mut m = base_args;
        m.extend(flags);
        m
    };
    Ok(merged)
}
