use crate::pty::{build_shell_command, resolve_shell, spawn_reader_thread};
use crate::state::{OUTPUT_RING_BUFFER_CAPACITY, VT_LOG_BUFFER_CAPACITY};
use crate::{AppState, MAX_CONCURRENT_SESSIONS, OutputRingBuffer, PtySession};
use axum::Json;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use portable_pty::PtySize;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "desktop")]
use tauri::Emitter;
use uuid::Uuid;

use super::types::*;
use super::ws_compression::{DEFLATE_SUBPROTOCOL, WsCompression, WsFrameSender};

pub(super) async fn get_session_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    Json(crate::session_usage::get_session_usage_impl(&state, &id).await).into_response()
}

pub(super) async fn get_prompt_receipt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match crate::prompt_receipt::read_receipt(&state, &id) {
        Ok(receipt) => Json(receipt).into_response(),
        Err(_) => session_not_found().into_response(),
    }
}

/// Standard 404 response for missing sessions.
fn session_not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "Session not found"})),
    )
}

pub(super) async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let uptime = state.server_start_time.elapsed().as_secs();
    let session_count = state.session_maps.sessions.len();
    #[cfg(unix)]
    let socket_path = {
        let p = state.bound_socket_path.read();
        if p.as_os_str().is_empty() {
            None
        } else {
            Some(p.display().to_string())
        }
    };
    #[cfg(not(unix))]
    let socket_path = None;
    Json(HealthResponse {
        ok: true,
        uptime_secs: uptime,
        session_count,
        protocol_version: crate::remote_runtime::REMOTE_PROTOCOL_VERSION as u32,
        build: crate::remote_deploy::assets::running_build_identity().ok(),
        survive_secs: state.remote_survive_secs,
        socket_path,
        instance_id: crate::app_instance::instance_identity(),
    })
}

pub(super) async fn app_version() -> Json<super::types::VersionResponse> {
    Json(super::types::VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
        git_hash: env!("BUILD_GIT_HASH"),
    })
}

/// Resolve live agent identities in one pass for all session-list consumers.
pub(crate) fn live_tuic_sessions_by_pty(
    state: &AppState,
) -> std::collections::HashMap<String, String> {
    // A session opened without a caller identity is bound under its own PTY
    // key. A later registered identity must win regardless of map iteration.
    let mut by_pty = std::collections::HashMap::new();
    for entry in state.session_maps.live_pty_by_tuic_session.iter() {
        let (identity, pty) = (entry.key(), entry.value());
        let bound = by_pty
            .entry(pty.clone())
            .or_insert_with(|| identity.clone());
        if bound == pty && identity != pty {
            *bound = identity.clone();
        }
    }
    by_pty
}

/// Publish PTY creation to the native frontend at the same boundary for
/// generic HTTP sessions and HTTP/MCP agent spawns.
#[cfg(feature = "desktop")]
pub(super) fn emit_native_agent_session_created(
    emit: impl FnOnce(&str, serde_json::Value),
    state: &AppState,
    session_id: &str,
    cwd: Option<&str>,
    agent_type: Option<&str>,
    display_name: Option<&str>,
    parent_session: Option<&str>,
) {
    emit(
        "session-created",
        serde_json::json!({
            "session_id": session_id,
            "tuic_session": live_tuic_sessions_by_pty(state).remove(session_id),
            "cwd": cwd,
            "agent_type": agent_type,
            "display_name": display_name,
            "parent_session": parent_session,
        }),
    );
}

/// Every PTY session this machine runs, as session-list rows.
///
/// One builder for both transports: `GET /sessions` and the `list_active_sessions`
/// Tauri command return the same rows, so a mirrored remote row (#791-055e) lands
/// in both lists the same way.
pub(crate) fn local_session_rows(state: &AppState) -> Vec<SessionInfo> {
    let tuic_by_pty = live_tuic_sessions_by_pty(state);
    let repositories = crate::config::load_repositories();
    state
        .session_maps
        .sessions
        .iter()
        .map(|entry| entry.key().clone())
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|session_id| {
            let entry = state.session_maps.sessions.get(&session_id)?;
            let session = entry.value().lock();
            let declared = tuic_by_pty
                .get(&session_id)
                .and_then(|peer| super::session_placement::declared_worktree(&repositories, peer));
            let mut row = SessionInfo {
                child_pid: if state.session_maps.exit_codes.contains_key(&session_id) {
                    None
                } else {
                    session._child.process_id()
                },
                cwd: session.cwd.clone(),
                worktree_path: declared.as_ref().map(|(path, _)| path.clone()).or_else(|| {
                    session
                        .worktree
                        .as_ref()
                        .map(|w| w.path.to_string_lossy().to_string())
                }),
                worktree_branch: declared
                    .map(|(_, branch)| branch)
                    .or_else(|| session.worktree.as_ref().and_then(|w| w.branch.clone())),
                display_name: session.display_name.clone(),
                display_name_is_custom: session.display_name_is_custom,
                display_name_from_spawn: session.display_name_from_spawn,
                is_remote: session.is_remote,
                pty_description: state
                    .session_maps
                    .pty_descriptions
                    .get(&session_id)
                    .map(|value| value.value().clone()),
                alias: state
                    .session_maps
                    .term_aliases
                    .get(&session_id)
                    .map(|value| value.value().clone()),
                tuic_session: tuic_by_pty.get(&session_id).cloned(),
                parent_session: state
                    .session_maps
                    .session_parent
                    .get(&session_id)
                    .map(|value| value.value().clone())
                    .filter(|parent| !super::mcp_transport::is_pending_parent(parent)),
                state: None,
                connection_id: None,
                session_id,
            };
            // Native Codex validation probes the live PTY again. Release both
            // the session mutex and map guard before taking that snapshot.
            drop(session);
            drop(entry);
            row.state = state.session_state_with_shell(&row.session_id);
            Some(row)
        })
        .collect()
}

/// Local rows plus one row per session a connected remote machine runs.
pub(crate) fn session_rows_including_remote(state: &AppState) -> Vec<SessionInfo> {
    let mut rows = local_session_rows(state);
    rows.extend(crate::remote_mirror::mirrored_rows(state));
    rows
}

pub(super) async fn list_sessions(State(state): State<Arc<AppState>>) -> Json<Vec<SessionInfo>> {
    Json(session_rows_including_remote(&state))
}

pub(super) async fn write_to_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<WriteRequest>,
) -> impl IntoResponse {
    if let Err(e) =
        crate::pty::run_bounded_pty_input(move || write_pty_input(&state, &session_id, &body.data))
            .await
    {
        if e == "Session not found" {
            return session_not_found();
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        );
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn write_parts_to_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<WritePartsRequest>,
) -> impl IntoResponse {
    if let Err(e) = crate::pty::run_bounded_pty_input(move || {
        let parts: Vec<&str> = body.parts.iter().map(String::as_str).collect();
        write_pty_input_parts(&state, &session_id, &parts)
    })
    .await
    {
        if e == "Session not found" {
            return session_not_found();
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        );
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

/// Browser/PWA counterpart of the `enqueue_agent_command` Tauri command.
pub(super) async fn enqueue_command(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<EnqueueCommandRequest>,
) -> impl IntoResponse {
    let outcome = tokio::task::spawn_blocking(move || {
        if let Some(draft) = body.expected_draft.as_deref() {
            return crate::pty::enqueue_owned_draft(&state, &session_id, &body.text, draft);
        }
        let session_id = crate::session_wake::ensure_live(&state, &session_id)?;
        crate::pty::enqueue_user_command(
            &state,
            &session_id,
            &body.text,
            body.idempotency_key.as_deref(),
        )
    })
    .await;
    match outcome {
        Ok(Ok(outcome)) => (StatusCode::OK, Json(serde_json::json!(outcome))),
        Ok(Err(e)) if e == "Session not found" => session_not_found(),
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Queue worker failed: {e}")})),
        ),
    }
}

/// Browser/PWA counterpart of the `clear_queued_agent_commands` Tauri command.
pub(super) async fn clear_queued_commands(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let cleared = crate::pty::clear_queued_commands(&state, &session_id);
    (StatusCode::OK, Json(serde_json::json!(cleared)))
}

/// Browser/PWA counterpart of the `list_queued_agent_commands` Tauri command.
pub(super) async fn list_queued_commands(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let queued = crate::pty::list_queued_commands(&state, &session_id);
    (StatusCode::OK, Json(serde_json::json!(queued)))
}

/// Browser/PWA counterpart of the `remove_queued_agent_command` Tauri command.
pub(super) async fn remove_queued_command(
    State(state): State<Arc<AppState>>,
    Path((session_id, command_id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let removed = crate::pty::remove_queued_command(&state, &session_id, command_id);
    (StatusCode::OK, Json(serde_json::json!(removed)))
}

pub(crate) fn write_pty_input(
    state: &Arc<AppState>,
    session_id: &str,
    data: &str,
) -> Result<(), String> {
    write_pty_input_parts(state, session_id, &[data])
}

/// Write all input parts to the PTY under one writer lock, then apply capture
/// and input bookkeeping once per original request, in order.
pub(crate) fn write_pty_input_parts(
    state: &Arc<AppState>,
    session_id: &str,
    parts: &[&str],
) -> Result<(), String> {
    crate::background_mode::hidden::record_remote_activity(state, session_id);
    let transaction = state.input_transaction(session_id);
    let _input = transaction.lock();
    let byte_parts: Vec<&[u8]> = parts.iter().map(|part| part.as_bytes()).collect();
    crate::pty::stamp_input_ms(state, session_id);
    state.write_pty_parts(session_id, &byte_parts)?;

    for part in parts {
        crate::pty_capture::record_input(session_id, part.as_bytes());
        apply_input_bookkeeping(state, session_id, part);
    }

    Ok(())
}

/// Write two input parts (e.g. text + a special-key sequence) to a session's
/// PTY under a SINGLE lock acquisition, then run the same post-write
/// bookkeeping (input-time stamp + InputLineBuffer FSM feed) that two
/// sequential `write_pty_input` calls would have run — once per part, in
/// order. Closes the interleave window a concurrent writer (peer injection,
/// desktop `write_pty`) could otherwise land in between the text write and
/// the Enter keystroke when the two writes took the PTY mutex separately.
/// Agent Enter flushes the text and shares the injection scheduling gap;
/// shell input and other keys remain raw atomic pairs.
pub(crate) fn write_pty_input_pair(
    state: &Arc<AppState>,
    session_id: &str,
    text: &str,
    key: &str,
    agent_type: Option<&str>,
) -> Result<(), String> {
    if key != "\r" || agent_type.is_none() {
        return write_pty_input_parts(state, session_id, &[text, key]);
    }
    crate::background_mode::hidden::record_remote_activity(state, session_id);
    let transaction = state.input_transaction(session_id);
    let _input = transaction.lock();

    let writer = state
        .pty_writer(session_id)
        .ok_or_else(|| "Session not found".to_string())?;
    let mut writer = writer.lock();
    state.retire_turn_interrupt(session_id);
    crate::pty::stamp_input_ms(state, session_id);
    writer
        .write_all(text.as_bytes())
        .map_err(|error| format!("Write failed: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Flush failed: {error}"))?;
    crate::pty::sleep_agent_enter_gap(agent_type);
    crate::pty::stamp_input_ms(state, session_id);
    writer
        .write_all(key.as_bytes())
        .map_err(|error| format!("Write failed: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Flush failed: {error}"))?;
    drop(writer);
    crate::pty_capture::record_input(session_id, text.as_bytes());
    crate::pty_capture::record_input(session_id, key.as_bytes());
    apply_input_bookkeeping(state, session_id, text);
    apply_input_bookkeeping(state, session_id, key);
    Ok(())
}

fn write_pty_input_bytes(
    state: &Arc<AppState>,
    session_id: &str,
    data: &[u8],
) -> Result<(), String> {
    crate::background_mode::hidden::record_remote_activity(state, session_id);
    let transaction = state.input_transaction(session_id);
    let _input = transaction.lock();
    crate::pty::stamp_input_ms(state, session_id);
    state.write_pty_parts(session_id, &[data])?;
    crate::pty_capture::record_input(session_id, data);
    if let Ok(text) = std::str::from_utf8(data) {
        apply_input_bookkeeping(state, session_id, text);
    }
    Ok(())
}

/// Post-write bookkeeping shared by all UTF-8 PTY input helpers: stamps
/// last-input time and feeds the
/// InputLineBuffer FSM to track slash_mode accurately. Runs once per input
/// part, after the single PTY lock for the complete write has been released.
pub(crate) fn apply_input_bookkeeping(state: &Arc<AppState>, session_id: &str, data: &str) {
    // Stamp last-input time (same as desktop write_pty) so the grid ticker
    // throttles frames for remote/PWA typing under CPU saturation too.
    crate::pty::record_input_ms(state, session_id);
    crate::state::resolve_choice_prompt_input(state, session_id, data);

    // Feed input through InputLineBuffer FSM to track slash_mode accurately.
    // The old substring heuristic false-positived on pastes starting with '/'.
    // Copy the FSM result and composer content, then release BOTH the inner
    // mutex and DashMap entry guard before any transition/delivery callback.
    // `flush_pending_injections` re-reads input_buffers; retaining the entry
    // guard across that call self-deadlocks its DashMap shard.
    let (actions, buffer_content) = {
        let input_entry = state
            .session_maps
            .input_buffers
            .entry(session_id.to_string())
            .or_insert_with(|| {
                parking_lot::Mutex::new(crate::input_line_buffer::InputLineBuffer::new())
            });
        let mut buf = input_entry.lock();
        let actions = buf.feed(data);
        let buffer_content = buf.content();
        (actions, buffer_content)
    };
    let interrupted = actions
        .iter()
        .any(|a| matches!(a, crate::input_line_buffer::InputAction::Interrupt));
    let line_submitted = actions.iter().any(|a| {
        matches!(
            a,
            crate::input_line_buffer::InputAction::Line(_)
                | crate::input_line_buffer::InputAction::Interrupt
        )
    });
    if interrupted || data == "\x1b" {
        if let Some(sl) = state.session_maps.silence_states.get(session_id) {
            sl.lock().note_interrupt_requested();
        }
    } else {
        for action in &actions {
            if let crate::input_line_buffer::InputAction::Line(content) = action {
                crate::pty::record_submitted_line(state, session_id, content.clone(), -1);
            }
        }
    }
    // Determine slash mode. The InputLineBuffer may accumulate junk from
    // terminal responses (e.g. DA reply "1;2c"), so buf.content() alone is
    // unreliable. Use multiple signals:
    let in_slash = if line_submitted {
        false
    } else if buffer_content.starts_with('/') {
        true
    } else if data == "/" {
        // Fresh slash keystroke from PWA/MCP — always enters slash mode
        true
    } else {
        // Maintain current slash_mode for subsequent chars (delta sync sends
        // one char at a time after the initial "/"), unless dismissed
        let is_bare_esc = data == "\x1b" || (data.contains('\x1b') && !data.contains("\x1b["));
        let dismissed = is_bare_esc || data.contains('\x03');
        !dismissed
            && state
                .session_maps
                .slash_mode
                .get(session_id)
                .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed))
    };
    tracing::trace!(
        "write_pty slash_mode: in_slash={in_slash} buf='{}' data='{}'",
        buffer_content,
        data
    );
    state
        .session_maps
        .slash_mode
        .entry(session_id.to_string())
        .or_insert_with(|| std::sync::atomic::AtomicBool::new(false))
        .store(in_slash, std::sync::atomic::Ordering::Relaxed);
    if buffer_content.is_empty() {
        crate::pty::flush_pending_injections(state, session_id);
    }
}

pub(super) async fn set_session_name(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SetNameRequest>,
) -> impl IntoResponse {
    let entry = match state.session_maps.sessions.get(&session_id) {
        Some(e) => e,
        None => return session_not_found(),
    };
    entry
        .lock()
        .set_display_name(body.name, body.is_custom.unwrap_or(true));
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn resize_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<ResizeRequest>,
) -> impl IntoResponse {
    if let Err(msg) = super::validate_terminal_size(body.rows, body.cols) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        );
    }
    // Shared core: grid-before-SIGWINCH ordering + same-dims no-op (056-7545),
    // on the blocking pool — a whole-ring rewrap must not sit on a tokio worker.
    match crate::pty::resize_session_off_thread(
        &state,
        session_id.clone(),
        body.rows,
        body.cols,
        "http",
    )
    .await
    {
        Ok(Some(frame)) => {
            crate::pty::send_grid_frame(&state, &session_id, frame);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Ok(None) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) if e.starts_with("Session not found") => session_not_found(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Resize failed: {}", e)})),
        ),
    }
}

/// Dump the raw PTY byte flight recorder for a session (story 056-7545).
/// Returns the most recent raw output bytes (pre-transform, up to 2 MiB) as
/// binary, for offline replay via `replay_capture_from_env`.
pub(super) async fn get_raw_ring(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.grid.pty_raw_rings.get(&session_id) {
        Some(ring) => {
            let bytes: Vec<u8> = {
                let ring = ring.lock();
                ring.iter().copied().collect()
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        None => session_not_found().into_response(),
    }
}

pub(super) async fn get_output(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<OutputQuery>,
) -> impl IntoResponse {
    crate::background_mode::hidden::record_remote_activity(&state, &session_id);
    let format = query.format.as_deref().unwrap_or("raw");

    // Both local MCP and the remote proxy use the same serializer, including
    // cursor windows, secret redaction and retained output after PTY exit.
    if matches!(format, "mcp" | "mcp_raw") {
        let mut args = serde_json::json!({
            "action": "output", "session_id": session_id,
            "format": if format == "mcp_raw" { "raw" } else { "text" },
            "limit": query.limit, "from_line": query.from_line,
            "since_cursor": query.since_cursor, "from_byte": query.from_byte,
            "byte_budget": query.byte_budget, "continuation": query.continuation,
        });
        args.as_object_mut()
            .expect("output arguments")
            .retain(|_, value| !value.is_null());
        let result = super::mcp_transport::session_output(&state, &args);
        return (
            if result.get("error").is_some() {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::OK
            },
            Json(result),
        );
    }

    // format=log: return VT100-extracted clean log lines (best for mobile/REST consumers)
    if format == "log" {
        // Snapshot identity before taking the grid lock, as in the log stream.
        // Retained agent rows are history after confirmed shell return.
        let agent_active = state
            .session_state_with_shell(&session_id)
            .is_some_and(|session| session.agent_type.is_some());
        let vt_log = match state.grid.vt_log_buffers.get(&session_id) {
            Some(b) => b,
            None => return session_not_found(),
        };
        let buf = vt_log.lock();
        let limit = query.limit.unwrap_or(usize::MAX);
        let total = buf.total_lines();
        let offset = match query.offset {
            Some(o) => o.min(total),
            None => total.saturating_sub(limit),
        };
        let (lines, _) = buf.lines_since_owned(offset, limit);
        // Absolute offset of the first line actually returned. `lines.len()` cannot
        // stand in for it: chrome lines occupy offset slots without being returned,
        // so a client subtracting the length would land inside the window it already
        // holds and replay those lines when scrolling up.
        let window_start = offset.max(buf.oldest_offset()).min(total);
        let projected = poll_screen(&buf, 0, agent_active);
        let screen = projected.screen.unwrap_or_default();
        let input_line = projected.input_line;
        let mut resp = serde_json::json!({
            "lines": lines,
            "total_lines": total,
            "offset": window_start,
            "screen": screen,
        });
        if let Some(il) = &input_line {
            resp["input_line"] = serde_json::json!(il);
        }
        return (StatusCode::OK, Json(resp));
    }

    // format=text: serve one canonical terminal snapshot. Do not concatenate
    // VtLogBuffer's finalized-log cursor with its current screen: after a row
    // resize grows the viewport, rows can move from history back onto the
    // screen while still being retained in the cursor log, producing duplicate
    // text even though the canonical terminal grid is correct.
    if format == "text" {
        let vt_log = match state.grid.vt_log_buffers.get(&session_id) {
            Some(b) => b,
            None => return session_not_found(),
        };
        let buf = vt_log.lock();
        let total = buf.grid_total_lines();
        let limit = query.limit.unwrap_or(usize::MAX);
        let start = query
            .offset
            .unwrap_or_else(|| total.saturating_sub(limit))
            .min(total);
        let end = start.saturating_add(limit).min(total);
        let data = buf.grid_get_lines(start, end).join("\n");
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "data": data,
                "data_length": data.len(),
                "total_written": total,
            })),
        );
    }

    let ring = match state.session_maps.output_buffers.get(&session_id) {
        Some(r) => r,
        None => return session_not_found(),
    };
    let limit = query.limit.unwrap_or(8192);
    let (bytes, total_written) = ring.lock().read_last(limit);
    let raw = String::from_utf8_lossy(&bytes).to_string();
    let data = raw;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "data": data,
            "data_length": data.len(),
            "total_written": total_written
        })),
    )
}

pub(super) async fn close_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if state.session_maps.sessions.contains_key(&session_id)
        || state.grid.vt_log_buffers.contains_key(&session_id)
    {
        let cleanup_state = Arc::clone(&state);
        if let Err(error) = tokio::task::spawn_blocking(move || {
            super::mcp_transport_session_agent::handle_session(
                &cleanup_state,
                &serde_json::json!({"action":"close", "session_id":session_id}),
                None,
            )
        })
        .await
        {
            tracing::warn!(%error, "session cleanup task failed");
        }

        (StatusCode::OK, Json(serde_json::json!({"ok": true})))
    } else {
        session_not_found()
    }
}

/// Wire a freshly spawned PTY into `AppState`: the session handle, its terminal
/// alias, the spawn metrics, the output ring, the VT screen **at the geometry the
/// PTY was actually opened with**, the idle clock, the grid-watch channel, and
/// the `SessionCreated` broadcast.
///
/// Three spawn paths need exactly this block — `spawn_pty_session`, the MCP
/// `agent spawn` handler, and `POST /agents` — and open-coding it three times let
/// them drift: two built the VT screen at a hardcoded 24x220 while handing the
/// child the caller's rows/cols, so every screen scrape (agent-state detection,
/// choice prompts, the chrome cutoff) parsed a grid the child had never drawn
/// into. Taking `rows`/`cols` here makes that class of mismatch unrepresentable.
///
/// A caller that pre-seeds `session_states` or queues injections must do so
/// **before** calling: this emits `SessionCreated`, and the reader thread the
/// caller starts afterwards is what consumes them.
// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "PTY registration boundary preserves explicit session metadata"
)]
pub(super) fn register_pty_session(
    state: &AppState,
    session_id: &str,
    session: PtySession,
    rows: u16,
    cols: u16,
    agent_type: Option<String>,
    requested_alias: Option<&str>,
    parent_session: Option<String>,
) {
    let cwd = session.cwd.clone();
    let display_name = session.display_name.clone();

    state
        .session_maps
        .sessions
        .insert(session_id.to_string(), Mutex::new(session));
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);

    state.session_maps.output_buffers.insert(
        session_id.to_string(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(state.new_vt_log_buffer(rows, cols, VT_LOG_BUFFER_CAPACITY)),
    );
    state
        .session_maps
        .last_output_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    // Without this `GET /sessions/{id}/stream?format=grid` finds no entry and
    // silently closes the socket.
    state
        .grid
        .watch
        .insert(session_id.to_string(), crate::grid_watch::new_grid_watch());

    // Broadcast to SSE/WebSocket consumers before the reader thread starts.
    state.emit_pty_event(crate::state::AppEvent::SessionCreated {
        session_id: session_id.to_string(),
        tuic_session: live_tuic_sessions_by_pty(state).remove(session_id),
        cwd,
        agent_type,
        display_name,
        parent_session,
    });
    // After `SessionCreated`: on the bus a consumer learns that a session exists
    // before an event about it. That ordering holds on the bus only — the desktop
    // `term-alias-assigned` window emit fires here, before either caller's desktop
    // `session-created`, and the UI relies on `terminalsStore.applyAlias`
    // retaining an alias for a session it has not bound yet.
    state.assign_term_alias(session_id, requested_alias);
}

/// Shared PTY setup: opens a PTY, spawns the shell, registers buffers and reader thread.
///
/// Returns `(session_id, cwd_string)` on success. Both `create_session` and
/// `create_session_with_worktree` delegate here after deriving the cwd and worktree.
/// What a client asks to keep when it is *restoring* a tab rather than opening a
/// new one: its stable TUIC identity, requested PTY key, and terminal alias.
/// Resume preserves the TUIC identity while minting a fresh PTY key. A requested
/// PTY key or alias is honoured only when nothing live holds it.
#[derive(Default)]
pub(crate) struct RequestedIdentity {
    pub session_id: Option<String>,
    pub tuic_session: Option<String>,
    pub alias: Option<String>,
    pub startup_command: Option<String>,
    pub agent_type: Option<String>,
}

struct PtyLaunch {
    shell: String,
    cwd: Option<String>,
    rows: u16,
    cols: u16,
    worktree: Option<crate::state::WorktreeInfo>,
    requested: RequestedIdentity,
    direct: Option<portable_pty::CommandBuilder>,
}

pub(crate) fn spawn_pty_session(
    state: Arc<AppState>,
    shell: String,
    cwd: Option<String>,
    rows: u16,
    cols: u16,
    worktree: Option<crate::state::WorktreeInfo>,
    requested: RequestedIdentity,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    spawn_pty_launch(
        state,
        PtyLaunch {
            shell,
            cwd,
            rows,
            cols,
            worktree,
            requested,
            direct: None,
        },
    )
}

/// Configured owner processes use argv directly, never shell command text.
pub(crate) fn spawn_pty_command(
    state: Arc<AppState>,
    program: &std::path::Path,
    cwd: &std::path::Path,
    args: &[String],
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error":"Max concurrent sessions reached"})),
        ));
    }
    let mut command = portable_pty::CommandBuilder::new(program.as_os_str());
    crate::pty::sanitize_pty_parent_env(&mut command);
    command.args(args);
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    spawn_pty_launch(
        state,
        PtyLaunch {
            shell: program.to_string_lossy().into_owned(),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            rows: 24,
            cols: 80,
            worktree: None,
            requested: RequestedIdentity::default(),
            direct: Some(command),
        },
    )
}

fn spawn_pty_launch(
    state: Arc<AppState>,
    launch: PtyLaunch,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    #[cfg(feature = "desktop")]
    let app_handle = state.app_handle.read().clone();
    spawn_pty_launch_with_emitter(
        state,
        launch,
        #[cfg(feature = "desktop")]
        |event, payload| {
            if let Some(app) = app_handle {
                let _ = app.emit(event, payload);
            }
        },
    )
}

fn spawn_pty_launch_with_emitter(
    state: Arc<AppState>,
    launch: PtyLaunch,
    #[cfg(feature = "desktop")] emit: impl FnOnce(&str, serde_json::Value),
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let PtyLaunch {
        shell,
        cwd,
        rows,
        cols,
        worktree,
        requested,
        direct,
    } = launch;
    // portable-pty can report a successful spawn even when the child fails to
    // enter its requested directory. Reject that request before registering a
    // session that would otherwise render as an empty terminal.
    if let Some(ref dir) = cwd {
        let expanded = crate::cli::expand_tilde(dir);
        let metadata = std::fs::metadata(&expanded).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Working directory {expanded:?} is unavailable: {error}")})),
            )
        })?;
        if !metadata.is_dir() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": format!("Working directory {expanded:?} is not a directory")}),
                ),
            ));
        }
    }
    // Honor a client-provided id when it is non-empty and not already taken
    // (browser duplicate-tab fix); otherwise mint a fresh one.
    let session_id = match requested.session_id {
        Some(id) if !id.is_empty() && !state.session_maps.sessions.contains_key(&id) => id,
        _ => Uuid::new_v4().to_string(),
    };
    let (pair, child) = crate::pty::spawn_pty_pair_with_retry(
        PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        || {
            if let Some(mut command) = direct {
                crate::pty::bind_pty_identity(&state, &mut command, &session_id, None);
                crate::pty::apply_agent_screen_env(&mut command, &std::collections::HashMap::new());
                return command;
            }
            let mut cmd = build_shell_command(&shell);
            if let Some(ref dir) = cwd {
                let dir = crate::cli::expand_tilde(dir);
                cmd.cwd(dir);
            }
            // This path used to inject neither shell integration nor an identity, so
            // every browser/remote/MCP-created session ran without OSC 133 markers and
            // without a `$TUIC_SESSION` to announce. Bring it in line with the desktop
            // path: absent a durable caller identity, the PTY key serves as both.
            crate::shell_integration::inject(&state.data_dir, &shell, &mut cmd);
            crate::pty::bind_pty_identity(
                &state,
                &mut cmd,
                &session_id,
                requested.tuic_session.as_deref(),
            );
            crate::pty::apply_agent_screen_env(&mut cmd, &std::collections::HashMap::new());
            if let Some(command) = &requested.startup_command {
                let (env, command) = crate::session_resume::shell_launch(command);
                for (key, value) in env {
                    cmd.env(key, value);
                }
                #[cfg(not(windows))]
                {
                    cmd.arg("-i");
                    cmd.arg("-c");
                    cmd.arg(&command);
                }
                #[cfg(windows)]
                {
                    let name = shell.to_ascii_lowercase();
                    if name.contains("powershell") || name.contains("pwsh") {
                        cmd.arg("-NoExit");
                        cmd.arg("-Command");
                    } else if name.contains("cmd.exe") {
                        cmd.arg("/K");
                    } else {
                        cmd.arg("-c");
                    }
                    cmd.arg(&command);
                }
            }
            cmd
        },
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
    })?;

    let writer = pair.master.take_writer().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)})),
        )
    })?;

    let reader = pair.master.try_clone_reader().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)})),
        )
    })?;

    state
        .session_maps
        .session_states
        .entry(session_id.clone())
        .or_default()
        .spawn_root_role = crate::state::SpawnRootRole::Shell;
    if let Some(mut snapshot) = state.session_maps.session_states.get_mut(&session_id) {
        snapshot.agent_type = requested.agent_type.clone();
        if requested.agent_type.is_some() {
            snapshot.agent_state = Some("starting".into());
        }
    }
    let paused = Arc::new(AtomicBool::new(false));
    register_pty_session(
        &state,
        &session_id,
        PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree,
            initial_cwd: cwd.clone(),
            cwd: cwd.clone(),
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: true,
            shell: shell.clone(),
        },
        rows,
        cols,
        None,
        requested.alias.as_deref(),
        None,
    );

    #[cfg(feature = "desktop")]
    let state_ref = state.clone();
    spawn_reader_thread(reader, paused, session_id.clone(), state, None);

    #[cfg(feature = "desktop")]
    emit_native_agent_session_created(
        emit,
        &state_ref,
        &session_id,
        cwd.as_deref(),
        None,
        None,
        None,
    );

    Ok(session_id)
}

pub(super) async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        );
    }

    let rows = body.rows.unwrap_or(24);
    let cols = body.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        );
    }
    let shell = resolve_shell(body.shell);

    let spawn = tokio::task::spawn_blocking(move || {
        spawn_pty_session(
            state,
            shell,
            body.cwd,
            rows,
            cols,
            None,
            RequestedIdentity {
                session_id: body.session_id,
                alias: body.alias,
                ..Default::default()
            },
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("PTY spawn task panicked: {error}")})),
        ))
    });
    match spawn {
        Ok(session_id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({"session_id": session_id})),
        ),
        Err(err) => err,
    }
}

pub(super) async fn pause_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let entry = match state.session_maps.sessions.get(&session_id) {
        Some(e) => e,
        None => return session_not_found(),
    };
    entry.lock().paused.store(true, Ordering::Relaxed);
    state
        .metrics
        .pauses_triggered
        .fetch_add(1, Ordering::Relaxed);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn resume_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let entry = match state.session_maps.sessions.get(&session_id) {
        Some(e) => e,
        None => return session_not_found(),
    };
    entry.lock().paused.store(false, Ordering::Relaxed);
    drop(entry);
    if let Err(error) = crate::session_wake::ensure_live(&state, &session_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn get_kitty_flags(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let flags = state
        .session_maps
        .kitty_states
        .get(&session_id)
        .map(|entry| entry.lock().current_flags())
        .unwrap_or(0);
    (StatusCode::OK, Json(serde_json::json!(flags)))
}

pub(super) async fn get_foreground_process(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let agent = crate::pty::refresh_session_agent(&state, &session_id);

    match agent {
        Some(name) => (StatusCode::OK, Json(serde_json::json!({"agent": name}))),
        None => (StatusCode::OK, Json(serde_json::json!({"agent": null}))),
    }
}

// --- PTY/terminal read-state queries (browser/remote parity, story 062). ---
// These mirror desktop commands through the same AppState. Foreground identity
// discovery uses the shared pty::refresh_session_agent path on both transports.

/// Shell state atom ("busy"/"idle") for a session, or null if never produced output.
pub(super) async fn get_shell_state(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let value = state
        .session_maps
        .shell_states
        .get(&session_id)
        .and_then(|atom| {
            crate::pty::shell_state_wire(atom.load(Ordering::Relaxed)).map(str::to_string)
        });
    Json(serde_json::json!({ "state": value }))
}

/// Last relevant user prompt (>= 10 words) for a session, or null.
pub(super) async fn get_last_prompt(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let value = state
        .session_maps
        .last_prompts
        .get(&session_id)
        .map(|v| v.clone());
    Json(serde_json::json!({ "prompt": value }))
}

#[derive(serde::Deserialize)]
pub(super) struct GuardedAgentInput {
    text: String,
}

pub(super) async fn send_agent_input_if_empty(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<GuardedAgentInput>,
) -> impl IntoResponse {
    super::json_result(
        tokio::task::spawn_blocking(move || {
            crate::pty::send_agent_input_if_empty_blocking(&state, &session_id, &body.text)
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result),
    )
}

/// Current input-line buffer content for a session (empty string if not typing).
pub(super) async fn get_input_buffer_content(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let content = state
        .session_maps
        .input_buffers
        .get(&session_id)
        .map(|entry| entry.lock().content())
        .unwrap_or_default();
    Json(serde_json::json!({ "content": content }))
}

/// PID of the deepest foreground process (PGID on Unix), or null.
pub(super) async fn get_session_leaf_pid(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let pid = crate::pty::session_leaf_pid(&state, &session_id);
    Json(serde_json::json!({ "pid": pid }))
}

/// Non-shell foreground process name (e.g. "htop", "node"), or null if the
/// foreground is the shell itself. Used to warn before closing a tab.
pub(super) async fn has_foreground_process(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    const SHELLS: &[&str] = &[
        "zsh",
        "bash",
        "fish",
        "sh",
        "dash",
        "ksh",
        "csh",
        "tcsh",
        "nushell",
        "nu",
        "powershell",
        "pwsh",
        "cmd",
    ];
    let process = (|| -> Option<String> {
        let entry = state.session_maps.sessions.get(&session_id)?;
        #[cfg(not(windows))]
        let pid = {
            let session = entry.value().lock();
            let pgid = session.master.process_group_leader()?;
            u32::try_from(pgid).ok()?
        };
        #[cfg(windows)]
        let pid = {
            let session = entry.value().lock();
            let child_pid = session._child.process_id()?;
            crate::pty::deepest_descendant_pid(child_pid)?
        };
        let name = crate::pty::process_name_from_pid(pid)?;
        if SHELLS.contains(&name.as_str()) {
            None
        } else {
            Some(name)
        }
    })();
    Json(serde_json::json!({ "process": process }))
}

/// Set a session's tab-visibility flag (focus/blur tracking; wakes on Unix).
pub(super) async fn set_session_visible(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SessionVisibleRequest>,
) -> impl IntoResponse {
    crate::background_mode::hidden::set_visible(&state, &session_id, body.visible, false);
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

pub(super) async fn get_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.orchestrator_stats())
}

pub(super) async fn get_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.session_metrics_json())
}

pub(super) async fn get_process_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(crate::pty::collect_process_stats(&state))
}

pub(super) async fn process_monitor_panel() -> impl IntoResponse {
    axum::response::Html(include_str!("process_monitor.html"))
}

pub(super) async fn create_session_with_worktree(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionWithWorktreeRequest>,
) -> impl IntoResponse {
    if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        );
    }

    // Create the worktree first
    let wt_config = crate::worktree::WorktreeConfig {
        task_name: body.branch_name.clone(),
        base_repo: body.base_repo,
        branch: Some(body.branch_name),
        create_branch: true,
    };
    let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
        std::path::Path::new(&wt_config.base_repo),
        &state.worktrees_dir,
    );
    let wt_config_bg = wt_config.clone();
    let worktree = match tokio::task::spawn_blocking(move || {
        crate::worktree::create_worktree_with_stale_recovery(&worktrees_dir, &wt_config_bg, None)
    })
    .await
    {
        Ok(Ok(wt)) => wt,
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("task panic: {e}")})),
            );
        }
    };

    let base_repo = wt_config.base_repo.clone();
    let worktree_path_str = worktree.path.to_string_lossy().to_string();
    let worktree_branch = worktree.branch.clone();
    let branch_name = worktree_branch.clone().unwrap_or_default();
    state.notify_worktree_created(crate::state::WorktreeCreatedPayload {
        creator_session: None,
        spawn_session: true,
        repo_path: base_repo.clone(),
        workspace_id: crate::worktree::workspace_id_of_worktree(&branch_name),
        branch: branch_name.clone(),
        worktree_path: worktree_path_str.clone(),
        // `create_worktree_with_stale_recovery` only ever links a worktree; this
        // route has no `mode` and cannot produce a clone.
        kind: crate::worktree::WorkspaceKind::Worktree,
    });

    let rows = body.config.rows.unwrap_or(24);
    let cols = body.config.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        );
    }
    let shell = resolve_shell(body.config.shell);

    let spawn_cwd = worktree_path_str.clone();
    let spawn = tokio::task::spawn_blocking(move || {
        spawn_pty_session(
            state,
            shell,
            Some(spawn_cwd),
            rows,
            cols,
            Some(worktree),
            RequestedIdentity {
                session_id: body.config.session_id,
                alias: body.config.alias,
                ..Default::default()
            },
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("PTY spawn task panicked: {error}")})),
        ))
    });
    match spawn {
        Ok(session_id) => {
            let mut response = serde_json::json!({
                "session_id": session_id,
                "worktree_path": worktree_path_str.clone(),
                "branch": worktree_branch,
            });
            let repo_for_script = base_repo.clone();
            let cwd_for_script = worktree_path_str;
            if let Some(script) = tokio::task::spawn_blocking(move || {
                crate::config::resolve_effective_setup_script(&repo_for_script)
            })
            .await
            .ok()
            .flatten()
            {
                match tokio::task::spawn_blocking(move || {
                    crate::worktree::run_setup_script(script, cwd_for_script)
                })
                .await
                {
                    Ok(Ok(result)) => {
                        response["setup_script"] = result;
                    }
                    Ok(Err(e)) => {
                        response["setup_script_error"] = serde_json::json!(e);
                    }
                    Err(e) => {
                        response["setup_script_error"] =
                            serde_json::json!(format!("task panic: {e}"));
                    }
                }
            }
            (StatusCode::CREATED, Json(response))
        }
        Err(err) => err,
    }
}

/// WebSocket upgrade handler for streaming PTY output.
/// Bidirectional: server sends PTY output, client sends PTY input.
/// Supports `?format=text` to strip ANSI, `?format=log` for VT100 log lines,
/// and `?compress=deflate` to compress the frames (`mcp_http::ws_compression`),
/// which is acknowledged by selecting the `tuic.deflate` subprotocol.
pub(super) async fn ws_stream(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    Query(query): Query<OutputQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if !state.session_maps.sessions.contains_key(&id) {
        let path = format!("/sessions/{id}/stream");
        let mut params = Vec::new();
        if let Some(format) = query.format.as_deref() {
            params.push(("format", format.to_string()));
        }
        if let Some(offset) = query.offset {
            params.push(("offset", offset.to_string()));
        }
        let query_string = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params)
            .finish();
        let Some(owner) =
            super::remote_session_proxy::owner_url(&state, &id, &path, Some(&query_string))
        else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let mut url = match owner {
            Ok(url) => url,
            Err(status) => return status.into_response(),
        };
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        if url.set_scheme(scheme).is_err() {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        let upstream = match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            tokio_tungstenite::connect_async(url.as_str()),
        )
        .await
        {
            Ok(Ok((stream, _))) => stream,
            _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
        return ws.on_upgrade(move |socket| relay_remote_ws(socket, upstream));
    }
    let format = query.format.as_deref().unwrap_or("raw");
    // A loopback peer is either genuinely on this machine — no link to save —
    // or arriving through the SSH tunnel, whose own `Compression=yes` already
    // deflated the channel. Either way, deflating here would burn CPU twice for
    // nothing, so the decision is the peer's address and not the request alone.
    let compression = WsCompression::negotiate(query.compress.as_deref(), &addr);
    // Say so on the handshake. `protocols` selects the subprotocol only when the
    // client offered it, so this header appears exactly when both halves agree —
    // which is what lets a client tell this server from an older one that
    // ignored `?compress=deflate` and is still sending untagged frames.
    let ws = if compression.is_tagged() {
        ws.protocols([DEFLATE_SUBPROTOCOL])
    } else {
        ws
    };

    if format == "grid" {
        return ws
            .write_buffer_size(64 * 1024)
            .max_write_buffer_size(256 * 1024)
            .on_upgrade(move |socket| async move {
                state
                    .remote_client_generation
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                handle_ws_grid_session(socket, id, state, compression).await;
            });
    }

    // format=text and format=log both serve clean VtLogBuffer rows (no strip_ansi).
    let log_mode = format == "log" || format == "text";
    let initial_offset = query.offset;
    ws.on_upgrade(move |socket| async move {
        state
            .remote_client_generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        handle_ws_session(socket, id, state, log_mode, initial_offset, compression).await;
    })
}

async fn relay_remote_ws(
    mut phone: WebSocket,
    mut owner: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    use tokio_tungstenite::tungstenite::Message as OwnerMessage;
    loop {
        tokio::select! {
            frame = phone.next() => {
                let Some(Ok(frame)) = frame else { break };
                let outgoing = match frame {
                    Message::Text(value) => OwnerMessage::Text(value.to_string().into()),
                    Message::Binary(value) => OwnerMessage::Binary(value),
                    Message::Ping(value) => OwnerMessage::Ping(value),
                    Message::Pong(value) => OwnerMessage::Pong(value),
                    Message::Close(_) => break,
                };
                if owner.send(outgoing).await.is_err() { break }
            }
            frame = owner.next() => {
                let Some(Ok(frame)) = frame else { break };
                let outgoing = match frame {
                    OwnerMessage::Text(value) => Message::Text(value.to_string().into()),
                    OwnerMessage::Binary(value) => Message::Binary(value),
                    OwnerMessage::Ping(value) => Message::Ping(value),
                    OwnerMessage::Pong(value) => Message::Pong(value),
                    OwnerMessage::Close(_) | OwnerMessage::Frame(_) => break,
                };
                if phone.send(outgoing).await.is_err() { break }
            }
        }
    }
}

/// Handle a WebSocket connection for a PTY session.
///
/// Multiplexes two streams to the client:
/// 1. Raw PTY output via mpsc channel → `{"type":"output","data":"..."}`
/// 2. Parsed events via broadcast channel → `{"type":"parsed","event":{...}}`
///
/// When `log_mode` is true (`?format=log` or `?format=text`), instead of raw PTY
/// output the client receives VT100-extracted log lines:
/// `{"type":"log","lines":[...],"offset":N}`
///
/// Client → server messages are written to the PTY as input.
async fn handle_ws_session(
    socket: WebSocket,
    session_id: String,
    state: Arc<AppState>,
    log_mode: bool,
    initial_offset: Option<usize>,
    compression: WsCompression,
) {
    let _watch =
        crate::background_mode::hidden::RemoteWatch::new(state.clone(), Some(session_id.clone()));
    let (ws_sender, mut ws_receiver) = socket.split();
    let mut ws_sender = WsFrameSender::new(ws_sender, compression);

    if log_mode {
        // Log/text mode: stream clean VtLogBuffer rows, no raw PTY chunks
        handle_ws_log_session(
            ws_sender,
            ws_receiver,
            session_id,
            state,
            initial_offset.unwrap_or(0),
        )
        .await;
        return;
    }

    // Subscribe to this session's per-session PTY event channel (no global-bus fan-out).
    let mut event_rx = state.subscribe_pty_events(&session_id);

    // Snapshot the ring buffer and register the live mpsc subscription
    // atomically while holding ring.lock(). The PTY writer takes the same
    // lock when appending + broadcasting to ws_clients, so serializing the
    // two sides guarantees every byte is delivered either via catch-up or
    // via the live channel — never both (duplicate) nor neither (gap).
    let (tx, mut rx) = crate::state::new_ws_client_channel();
    let snapshot = state
        .session_maps
        .output_buffers
        .get(&session_id)
        .map(|ring| {
            let r = ring.lock();
            let snap = if let Some(off) = initial_offset {
                r.read_since(off as u64)
            } else {
                r.read_last(OUTPUT_RING_BUFFER_CAPACITY)
            };
            state
                .ws_clients
                .entry(session_id.clone())
                .or_default()
                .push(tx);
            drop(r);
            snap
        });

    // Send catch-up data in chunks (64 KB) so the client can render progressively.
    const CATCHUP_CHUNK_SIZE: usize = 64 * 1024;
    if let Some((data, total)) = snapshot
        && !data.is_empty()
    {
        for chunk in data.chunks(CATCHUP_CHUNK_SIZE) {
            let text = String::from_utf8_lossy(chunk);
            if !text.is_empty() {
                let frame =
                    serde_json::json!({"type": "output", "data": text, "total_written": total});
                if ws_sender.text(&frame.to_string()).await.is_err() {
                    // Client disconnected during catch-up. It was already
                    // registered above, so reap it here — this path never
                    // reaches the purge at the end of the read loop.
                    crate::state::purge_dead_ws_clients(&state.ws_clients, &session_id);
                    return;
                }
            }
        }
    }

    // Spawn a task to forward PTY output + parsed events to the WebSocket
    let sid_for_events = session_id.clone();
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Raw PTY output from mpsc channel
                data = rx.recv() => {
                    let Some(data) = data else { break };
                    let frame = serde_json::json!({"type": "output", "data": data});
                    if ws_sender.text(&frame.to_string()).await.is_err() {
                        break;
                    }
                }
                // Parsed events from broadcast channel
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Per-session channel — every event belongs to this session.
                            // Extract the inner payload (without serde tag wrapping).
                            let payload = match &event {
                                crate::state::AppEvent::PtyParsed { parsed, .. } => {
                                    serde_json::json!({"type": "parsed", "event": parsed})
                                }
                                crate::state::AppEvent::PtyBell { .. } => serde_json::json!({"type": "bell"}),
                                crate::state::AppEvent::PtyTitle { title, .. } => {
                                    serde_json::json!({"type": "title", "title": title})
                                }
                                crate::state::AppEvent::PtyExit { session_id: sid } => {
                                    serde_json::json!({"type": "exit", "session_id": sid})
                                }
                                // The browser counterpart of the desktop
                                // `pty-activity-{id}` Tauri event. Both ride the
                                // subscription `subscribePty` owns, so the two
                                // transports carry the same signal by construction
                                // rather than by coincidence. The grid WS below
                                // deliberately does NOT forward this — CanvasTerminal
                                // has no activity consumer, and a pulse nobody reads
                                // is a wake-up nobody needs.
                                crate::state::AppEvent::PtyActivity { session_id: sid } => {
                                    serde_json::json!({"type": "activity", "session_id": sid})
                                }
                                crate::state::AppEvent::PluginWatcherLines { session_id: sid, lines } => {
                                    serde_json::json!({"type": "watcher-lines", "session_id": sid, "lines": lines})
                                }
                                crate::state::AppEvent::SessionClosed { session_id: sid, reason } => {
                                    serde_json::json!({"type": "closed", "session_id": sid, "reason": reason})
                                }
                                crate::state::AppEvent::PtyDescriptionChanged { session_id: sid, description } => {
                                    serde_json::json!({"type": "pty-description", "session_id": sid, "description": description})
                                }
                                _ => continue,
                            };
                            if ws_sender.text(&payload.to_string()).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(session_id = %sid_for_events, lagged = n, "WebSocket broadcast lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // Read messages from the client and write to PTY
    let state_clone = state.clone();
    let sid = session_id.clone();
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let input_state = state_clone.clone();
                let input_sid = sid.clone();
                if let Err(error) = crate::pty::run_bounded_pty_input(move || {
                    write_pty_input(&input_state, &input_sid, &text)
                })
                .await
                {
                    tracing::error!(session_id = %sid, %error, "PTY write failed");
                    break;
                }
            }
            Message::Binary(data) => {
                let input_state = state_clone.clone();
                let input_sid = sid.clone();
                if let Err(error) = crate::pty::run_bounded_pty_input(move || {
                    write_pty_input_bytes(&input_state, &input_sid, &data)
                })
                .await
                {
                    tracing::error!(session_id = %sid, %error, "PTY write failed");
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Drop the receiver before reaping senders and releasing the remote watch.
    send_task.abort();
    let _ = send_task.await;
    crate::state::purge_dead_ws_clients(&state.ws_clients, &session_id);
}

/// Handle a WebSocket connection in log mode (`?format=log`).
///
/// Sends VT100-extracted log lines: catch-up on connect, then polls for new
/// lines every 200 ms and batches them as `{"type":"log","lines":[...],"offset":N}`.
/// The client can still send PTY input (written as-is to the PTY).
async fn handle_ws_log_session(
    mut ws_sender: WsFrameSender,
    mut ws_receiver: futures_util::stream::SplitStream<WebSocket>,
    session_id: String,
    state: Arc<AppState>,
    skip_offset: usize,
) {
    // Send catch-up: only lines accumulated AFTER skip_offset.
    // When the client already fetched lines via HTTP, skip_offset = total_lines
    // from that response, so the catch-up only sends the delta.
    let initial_offset = {
        if let Some(vt_log) = state.grid.vt_log_buffers.get(&session_id) {
            let (total, catchup_frame) = {
                let buf = vt_log.lock();
                let total = buf.total_lines();
                let frame = if total > skip_offset {
                    let (lines, _) = buf.lines_since_owned(skip_offset, usize::MAX);
                    if !lines.is_empty() {
                        // total_lines is the post-read cursor (monotonic): the client
                        // stores it and passes it back as ?offset= on reconnect, so the
                        // next catch-up resumes from here instead of replaying from mount.
                        Some(serde_json::json!({"type": "log", "lines": lines, "offset": skip_offset, "total_lines": total}).to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                (total, frame)
            }; // lock released here
            if let Some(frame_str) = catchup_frame {
                let _ = ws_sender.text(&frame_str).await;
            }
            total
        } else {
            0
        }
    };

    // Spawn polling task: check for new lines every 200ms AND forward state changes.
    let sid_poll = session_id.clone();
    let state_poll = state.clone();
    let send_task = tokio::spawn(async move {
        let mut offset = initial_offset;
        let mut event_rx = state_poll.subscribe_pty_events(&sid_poll);
        let mut prev_screen_hash: u64 = 0;
        // Dedup: only send state frames when SessionState actually changed
        let mut prev_state: Option<crate::state::SessionState> = None;

        // Send initial state snapshot so the client has the correct status immediately
        if let Some(current) = state_poll.session_state_with_shell(&sid_poll) {
            let frame = serde_json::json!({"type": "state", "state": &current});
            prev_state = Some(current);
            let _ = ws_sender.text(&frame.to_string()).await;
        }

        // Event traffic must not restart the batching deadline: animated titles
        // can arrive faster than 200 ms indefinitely.
        let mut poll_tick = tokio::time::interval(std::time::Duration::from_millis(200));
        poll_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        poll_tick.tick().await;
        loop {
            // Track whether we need to check state and/or send log frames
            enum LoopAction {
                Poll,        // sleep arm: check state + send log/screen
                Event,       // event arm: check state only (relevant event)
                Skip,        // event arm: irrelevant event, skip state check
                SessionGone, // vt_log_buffer missing, exit loop
            }

            let action = tokio::select! {
                _ = poll_tick.tick() => {
                    if state_poll.grid.vt_log_buffers.contains_key(&sid_poll) {
                        LoopAction::Poll
                    } else {
                        LoopAction::SessionGone
                    }
                }
                event = event_rx.recv() => {
                    // Per-session channel — every delivered event belongs to this
                    // session, so any Ok triggers a state re-check. On Closed the
                    // channel was reaped (session gone): exit instead of spinning on
                    // the immediately-ready error arm. Lagged is a transient skip.
                    match event {
                        Ok(crate::state::AppEvent::PtyBell { .. }) => {
                            if ws_sender.text(&serde_json::json!({"type": "bell"}).to_string()).await.is_err() {
                                break;
                            }
                            LoopAction::Skip
                        }
                        Ok(crate::state::AppEvent::PtyTitle { title, .. }) => {
                            if ws_sender.text(&serde_json::json!({"type": "title", "title": title}).to_string()).await.is_err() {
                                break;
                            }
                            LoopAction::Skip
                        }
                        Ok(_) => LoopAction::Event,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => LoopAction::SessionGone,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => LoopAction::Skip,
                    }
                }
            };

            if matches!(action, LoopAction::SessionGone) {
                break;
            }
            if matches!(action, LoopAction::Skip) {
                continue;
            }

            // On Event: yield to let the session_state_accumulator task process
            // the same broadcast event before we read the state.  Without this,
            // we may read stale state (the accumulator hasn't applied the event
            // yet) → dedup sees no change → client misses the update.
            if matches!(action, LoopAction::Event) {
                tokio::task::yield_now().await;
            }

            // Single session_state_with_shell call per iteration, used by both arms
            if let Some(current) = state_poll.session_state_with_shell(&sid_poll)
                && prev_state.as_ref() != Some(&current)
            {
                let frame = serde_json::json!({"type": "state", "state": &current});
                prev_state = Some(current);
                if ws_sender.text(&frame.to_string()).await.is_err() {
                    break;
                }
            }

            // Poll arm: also send log lines and screen content
            if matches!(action, LoopAction::Poll) {
                let Some(vt_log) = state_poll.grid.vt_log_buffers.get(&sid_poll) else {
                    break;
                };
                let (lines, new_offset, polled) = {
                    let buf = vt_log.lock();
                    let (l, o) = buf.lines_since_owned(offset, usize::MAX);
                    (
                        l,
                        o,
                        poll_screen(
                            &buf,
                            prev_screen_hash,
                            prev_state
                                .as_ref()
                                .is_some_and(|session| session.agent_type.is_some()),
                        ),
                    )
                }; // lock released
                let input_line = polled.input_line;
                let screen_lines = polled.screen;
                let screen_changed = screen_lines.is_some();
                // Store the signature for every poll, not only the ones that
                // produce a frame: a screen of nothing but blanks styles to
                // nothing, and leaving the old hash in place made the next tick
                // rebuild it to reach the same conclusion.
                prev_screen_hash = polled.hash;
                // Send frame if there are new log lines OR screen content changed
                if !lines.is_empty() || screen_changed {
                    // total_lines = post-read monotonic cursor (== offset when no new
                    // lines). The client tracks it for reconnect resume — see catch-up above.
                    let mut frame = serde_json::json!({"type": "log", "offset": offset, "total_lines": new_offset});
                    if !lines.is_empty() {
                        frame["lines"] = serde_json::json!(lines);
                    }
                    if let Some(ref screen) = screen_lines {
                        frame["screen"] = serde_json::json!(screen);
                        if let Some(ref il) = input_line {
                            frame["input_line"] = serde_json::json!(il);
                        }
                    }
                    if ws_sender.text(&frame.to_string()).await.is_err() {
                        break;
                    }
                    if !lines.is_empty() {
                        offset = new_offset;
                    }
                }
            }
        }
    });

    // Read messages from the client and write to PTY (input passthrough)
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let input_state = state.clone();
                let input_sid = session_id.clone();
                if let Err(error) = crate::pty::run_bounded_pty_input(move || {
                    write_pty_input(&input_state, &input_sid, &text)
                })
                .await
                {
                    tracing::error!(session_id = %session_id, %error, "PTY write failed");
                    break;
                }
            }
            Message::Binary(data) => {
                let input_state = state.clone();
                let input_sid = session_id.clone();
                if let Err(error) = crate::pty::run_bounded_pty_input(move || {
                    write_pty_input_bytes(&input_state, &input_sid, &data)
                })
                .await
                {
                    tracing::error!(session_id = %session_id, %error, "PTY write failed");
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}

/// Build the JSON text frame the grid WebSocket sends for one bus event, or
/// `None` for events this socket does not carry.
///
/// A function rather than an inline `match` so a test can assert the shape that
/// actually goes on the wire. A test that rebuilds the frame by hand proves only
/// that the test agrees with itself: it stays green while the wire carries
/// something the client cannot read.
///
/// SHAPE CONTRACT: `WsTransport` destructures each frame as
/// `const { type, ...payload } = event` and hands `payload` to the same handler
/// the desktop `listen()` feeds. So every frame here must be its Tauri event
/// payload plus a `type` discriminator — no renamed fields, no extra nesting —
/// or `CanvasTerminal` needs a per-transport branch.
fn grid_ws_frame(event: &crate::state::AppEvent) -> Option<serde_json::Value> {
    // Per-session channel — every event belongs to this session.
    Some(match event {
        crate::state::AppEvent::PtyBell { .. } => serde_json::json!({"type":"bell"}),
        crate::state::AppEvent::PtyParsed { parsed, .. } => {
            serde_json::json!({"type": "parsed", "event": parsed})
        }
        crate::state::AppEvent::PtyExit { session_id: sid } => {
            serde_json::json!({"type": "exit", "session_id": sid})
        }
        crate::state::AppEvent::PluginWatcherLines {
            session_id: sid,
            lines,
        } => {
            serde_json::json!({"type": "watcher-lines", "session_id": sid, "lines": lines})
        }
        crate::state::AppEvent::SessionClosed {
            session_id: sid,
            reason,
        } => {
            serde_json::json!({"type": "closed", "session_id": sid, "reason": reason})
        }
        crate::state::AppEvent::PtyDescriptionChanged {
            session_id: sid,
            description,
        } => {
            serde_json::json!({"type": "pty-description", "session_id": sid, "description": description})
        }
        // Mirrors the desktop `Osc133Event` field for field — see the shape
        // contract above. Without this a browser/PWA client had no command
        // blocks, no gutter marks and no Cmd+Up/Down navigation.
        crate::state::AppEvent::PtyOsc133 {
            marker,
            line,
            exit_code,
            ..
        } => {
            serde_json::json!({"type": "osc133", "marker": marker, "line": line, "exit_code": exit_code})
        }
        crate::state::AppEvent::PtyCwd { cwd, .. } => {
            serde_json::json!({"type": "cwd", "cwd": cwd})
        }
        _ => return None,
    })
}

/// Handle a WebSocket connection in grid mode (`?format=grid`).
///
/// Streams binary grid frames (same format as Tauri Channel) using the
/// `grid_watch` channel. The channel keeps only the newest frame, so a client
/// that cannot keep up skips intermediate ones — and those frames are DELTAS, so
/// a skip strands the rows it carried. Each frame therefore rides with a sequence
/// number (Rust-side only, the wire format is untouched) and a gap is repaired
/// with a fresh full frame instead of being rendered as a hole.
///
/// On connect, sends a full frame (all rows marked dirty). Subsequent frames
/// are delta-based (only changed rows). Client sends text messages for
/// commands (e.g. `{"type":"ack"}`) and binary messages for PTY input.
async fn handle_ws_grid_session(
    socket: WebSocket,
    session_id: String,
    state: Arc<AppState>,
    compression: WsCompression,
) {
    let _watch =
        crate::background_mode::hidden::RemoteWatch::new(state.clone(), Some(session_id.clone()));
    let (ws_sender, mut ws_receiver) = socket.split();
    let mut ws_sender = WsFrameSender::new(ws_sender, compression);

    // Subscribe to the grid watch channel (newest-frame-wins for slow clients).
    let mut frame_rx = match state.grid.watch.get(&session_id) {
        Some(tx) => tx.subscribe(),
        None => {
            let _ = ws_sender.close().await;
            return;
        }
    };
    // Whatever has been published so far is superseded by the full frame below,
    // so start from the current sequence rather than zero — otherwise the first
    // delta of a long-running session would always look like a gap.
    let mut last_seq = frame_rx.borrow_and_update().seq;

    // Send initial full frame so the client can render immediately. The helper
    // scopes the MutexGuard (dropped before the .await) and gives the damage back
    // so this connect does not cost the desktop channel its next frame.
    let initial_frame = full_frame_for_single_client(&state, &session_id);
    let sent = match initial_frame {
        Some(frame) => ws_sender.binary(frame).await,
        // A live watch channel can precede its grid buffer. Silence would look
        // like a lost replay to the client, even for a healthy idle PTY.
        None => ws_sender.text(r#"{"type":"grid-replay-empty"}"#).await,
    };
    if sent.is_err() {
        return;
    }

    // Subscribe to this session's per-session PTY event channel (exit, closed, parsed).
    let mut event_rx = state.subscribe_pty_events(&session_id);
    let sid_for_events = session_id.clone();
    let resync_state = state.clone();
    let resync_sid = session_id.clone();

    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = frame_rx.changed() => {
                    if result.is_err() { break; } // sender dropped
                    let (seq, frame) = {
                        let slot = frame_rx.borrow_and_update();
                        (slot.seq, slot.frame.clone())
                    };
                    // Frames the channel dropped carried dirty rows that exist
                    // nowhere else. Sending this delta on top of a row map missing
                    // them would leave stale content on screen with no error, so
                    // re-serialize the whole grid instead — through the helper that
                    // hands the damage back to the other transports.
                    //
                    // DEFERRED (2026-08-18) — a bell flag carried by a skipped frame
                    // is lost: the resync reports the grid's current state, and the
                    // bell is an event, not state. Fixing it means latching bells per
                    // subscriber, which is a second piece of per-client state on a
                    // path that only skips frames when the client is already too slow
                    // to keep up. Revisit if a missed bell is ever reported.
                    let frame = if crate::grid_watch::watch_dropped_frames(last_seq, seq) {
                        tracing::debug!(
                            session_id = %resync_sid,
                            last_seq,
                            seq,
                            "grid watch dropped frames, resyncing with a full frame"
                        );
                        full_frame_for_single_client(&resync_state, &resync_sid).unwrap_or(frame)
                    } else {
                        frame
                    };
                    last_seq = seq;
                    if !frame.is_empty() && ws_sender.binary(frame).await.is_err() {
                        break;
                    }
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            let Some(payload) = grid_ws_frame(&event) else { continue };
                            if ws_sender.text(&payload.to_string()).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(session_id = %sid_for_events, lagged = n, "grid WS broadcast lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // Read messages from the client
    let state_clone = state.clone();
    let sid = session_id.clone();
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(_text) => {
                // Reserved for client commands (e.g. resize, scroll).
                // No ACK needed — watch channel handles backpressure naturally.
            }
            Message::Binary(data) => {
                let input_state = state_clone.clone();
                let input_sid = sid.clone();
                if let Err(error) = crate::pty::run_bounded_pty_input(move || {
                    write_pty_input_bytes(&input_state, &input_sid, &data)
                })
                .await
                {
                    tracing::error!(session_id = %sid, %error, "PTY write failed");
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();

    // The send task held the only other receiver, and aborting it drops it. If
    // that was the last one, nobody will ever read the frame still sitting in
    // the watch slot — free it instead of pinning it for the session's life.
    if let Some(watch_tx) = state.grid.watch.get(&session_id)
        && watch_tx.receiver_count() == 0
    {
        crate::grid_watch::release_grid_frame(&watch_tx);
    }
}

/// Remove agent TUI chrome from screen rows (status bars, prompt lines,
/// separators) and trim trailing empty rows.
///
/// Scans from the bottom for two anchor patterns:
/// 1. Separator line (all box-drawing chars like `────`) — cuts from there
/// 2. Prompt line (`❯`, `>`) — cuts from there, extending up past separators
///
/// The scan window is 15 rows to accommodate Claude Code's full footer
/// (prompt + input area + separator + status bar = ~12 rows).
/// Result of trimming screen chrome: cleaned rows.
struct TrimResult {
    /// How many rows were kept (cutoff index). Allows applying the same trim to parallel data.
    cutoff: usize,
}

use crate::chrome::find_chrome_cutoff;

/// Borrows: it reports a cutoff and reads nothing else, so taking the rows by
/// value only forced every caller to clone a screen it already had in hand.
fn trim_screen_chrome(rows: &[String]) -> TrimResult {
    // A modal's selection marker and rules are content, not composer chrome.
    // Its active interaction footer owns the bottom of the screen regardless
    // of the dialog title. Only presentation uses this rule: parser cutoffs
    // must still exclude the input zone. A later composer supersedes the footer.
    let interaction_footer = rows
        .iter()
        .rev()
        .find(|row| !row.trim().is_empty())
        .map(|row| row.trim())
        .is_some_and(|footer| {
            (footer.contains("Esc to cancel") || footer.contains("Esc to clear"))
                && (footer.starts_with("Enter to ")
                    || footer.starts_with("Esc to ")
                    || footer.contains(" · "))
        });
    if interaction_footer {
        return TrimResult { cutoff: rows.len() };
    }
    let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
    let cutoff = find_chrome_cutoff(&refs).unwrap_or(rows.len());
    TrimResult { cutoff }
}

/// Chrome cutoff for the buffer's current screen, borrowing the grid's cached
/// rows. The owned fallback is only for a buffer whose `process()` has never
/// run, which has no snapshot to lend.
fn screen_chrome_cutoff(buf: &crate::state::VtLogBuffer) -> TrimResult {
    match buf.screen_rows_ref() {
        Some(rows) => trim_screen_chrome(rows),
        None => trim_screen_chrome(&buf.screen_rows()),
    }
}

/// What one log-WS poll found on the screen.
struct ScreenPoll {
    /// The styled rows, present ONLY when the screen changed since `prev_hash`.
    screen: Option<Vec<crate::state::LogLine>>,
    input_line: Option<String>,
    /// Signature to pass back as `prev_hash` on the next poll.
    hash: u64,
}

/// Decide whether the screen changed, and build the styled rows only if it did.
///
/// The signature is the plain text of the visible rows plus the input line —
/// exactly what the old check reduced to, since it hashed `span.text` and
/// nothing else. Building the styled `Vec<LogLine>` first and hashing it
/// afterwards made an idle session materialize a full screen five times a
/// second, under the buffer's mutex, only to discard it.
///
/// The styled build stays inside the caller's lock because it reads the grid;
/// what changed is how often it runs, not where.
fn poll_screen(buf: &crate::state::VtLogBuffer, prev_hash: u64, agent_active: bool) -> ScreenPoll {
    use std::hash::{Hash, Hasher};

    // Only a live agent owns composer chrome. After identity release, a
    // retained prompt/selection cannot truncate later shell output or become
    // the mobile input line. REST and WS share this projection.
    let cutoff = if agent_active {
        screen_chrome_cutoff(buf).cutoff
    } else {
        buf.grid_screen_lines()
    };
    let input_line = agent_active.then(|| buf.prompt_input_text()).flatten();

    let owned;
    let rows: &[String] = match buf.screen_rows_ref() {
        Some(rows) => rows,
        None => {
            owned = buf.screen_rows();
            &owned
        }
    };
    let visible = &rows[..cutoff.min(rows.len())];

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for row in visible {
        row.hash(&mut hasher);
    }
    input_line.hash(&mut hasher);
    let hash = hasher.finish();

    if hash == prev_hash {
        return ScreenPoll {
            screen: None,
            input_line,
            hash,
        };
    }

    let styled: Vec<_> = buf.screen_log_lines().into_iter().take(cutoff).collect();
    // screen_log_lines drops trailing blank rows, so a screen of nothing but
    // blanks styles to nothing. The old path sent no frame for it either.
    let screen = if styled.is_empty() {
        None
    } else {
        Some(styled)
    };
    ScreenPoll {
        screen,
        input_line,
        hash,
    }
}

// --- Terminal grid HTTP endpoints ---
//
// The grid reads take the vt mutex, which the PTY reader holds through a whole
// `serialize_dirty_rows`, so they go to the blocking pool through the same
// `pty::vt_try_read` the desktop commands use. Sharing the helper rather than
// the command is forced: the commands are `#[cfg(feature = "desktop")]` and
// these routes also compile into the headless `tuic-remote` binary. See
// `docs/backend/command-threading.md`.

/// Publish the resolved terminal theme. Not session-scoped: one window, one
/// palette, and the emulator answers colour queries from a process-wide value.
pub(super) async fn terminal_theme_colors(
    Json(body): Json<super::types::TerminalThemeColorsRequest>,
) -> impl IntoResponse {
    crate::terminal_grid::set_terminal_palette(crate::terminal_grid::TerminalPalette {
        foreground: (body.foreground[0], body.foreground[1], body.foreground[2]),
        background: (body.background[0], body.background[1], body.background[2]),
        cursor: (body.cursor[0], body.cursor[1], body.cursor[2]),
    });
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn terminal_scroll(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<super::types::TerminalScrollRequest>,
) -> impl IntoResponse {
    let Some(vt) = state.grid.vt_log_buffers.get(&session_id) else {
        return session_not_found();
    };
    let frame = {
        let mut vt = vt.lock();
        vt.grid_scroll(body.delta);
        vt.serialize_dirty_rows()
    };
    crate::pty::send_grid_frame(&state, &session_id, frame);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn terminal_scroll_to(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<super::types::TerminalScrollToRequest>,
) -> impl IntoResponse {
    let Some(vt) = state.grid.vt_log_buffers.get(&session_id) else {
        return session_not_found();
    };
    let frame = {
        let mut vt = vt.lock();
        vt.grid_scroll_to_line(body.line);
        vt.serialize_dirty_rows()
    };
    crate::pty::send_grid_frame(&state, &session_id, frame);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

/// Coalesced scroll to an absolute display offset. Mirrors the desktop
/// `terminal_scroll_to_offset` Tauri command: records the target and marks the
/// grid dirty so the frame ticker applies it under its own lock — taking NO vt
/// lock here, so scrolling never contends with the PTY output processor. The
/// ticker emits the resulting frame over the same bus SSE/WS feeds in browser
/// mode. This is the wheel + scrollbar-drag scroll path.
pub(super) async fn terminal_scroll_to_offset(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<super::types::TerminalScrollToOffsetRequest>,
) -> impl IntoResponse {
    if let Some(p) = state.grid.pending_scroll.get(&session_id) {
        p.store(body.offset as i64, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(d) = state.grid.frame_dirty.get(&session_id) {
        d.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

// Same coalesced history coordinate and tail sentinel as desktop IPC.
pub(super) async fn terminal_scroll_to_anchor(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<super::types::TerminalScrollToAnchorRequest>,
) -> impl IntoResponse {
    crate::pty::queue_scroll_anchor(&state, &session_id, body.anchor);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn terminal_scroll_info(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, |vt| {
        serde_json::json!({
            "display_offset": vt.grid_display_offset(),
            "total_lines": vt.grid_total_lines(),
            "screen_lines": vt.grid_screen_lines(),
        })
    })
    .await
    {
        Ok(Some(info)) => Json(info).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

/// A grid read whose session went away between the request and the pool hop.
fn not_found_response() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "Session not found"})),
    )
        .into_response()
}

/// The blocking-pool task itself failed — a panic in the read, or a runtime
/// shutting down. Distinct from a missing session, which is routine.
fn read_failed_response(error: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": error})),
    )
        .into_response()
}

pub(super) async fn terminal_search(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<super::types::TerminalSearchRequest>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, move |vt| vt.grid_search(&body.query)).await {
        Ok(Some(matches)) => Json(serde_json::json!({"matches": matches})).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

pub(super) async fn terminal_search_buffer(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<super::types::TerminalSearchRequest>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, move |vt| {
        vt.grid_search_buffer(&body.query)
    })
    .await
    {
        Ok(Some(matches)) => Json(serde_json::json!({"matches": matches})).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

pub(super) async fn terminal_get_row_text(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<super::types::TerminalRowQuery>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, move |vt| {
        vt.grid_get_row_text(query.row)
    })
    .await
    {
        Ok(Some(text)) => Json(serde_json::json!({"text": text})).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

/// Extract the text of a selection span (start/end row/col) from the grid.
pub(super) async fn terminal_get_selection_text(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(q): Query<super::types::TerminalSelectionQuery>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, move |vt| {
        vt.grid_get_selection_text(
            q.start_row,
            q.start_col,
            q.end_row,
            q.end_col,
            q.history_base,
        )
    })
    .await
    {
        Ok(Some(Ok(text))) => Json(serde_json::json!({"text": text})).into_response(),
        Ok(Some(Err(error))) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

/// Unwrap a soft-wrapped logical line at `row` → `[logicalStartRow, text]`.
pub(super) async fn terminal_get_logical_line(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(q): Query<super::types::TerminalRowQuery>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, move |vt| {
        vt.grid_get_logical_line(q.row)
    })
    .await
    {
        Ok(Some((idx, text))) => Json(serde_json::json!([idx, text])).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

/// Hyperlink span at a cell → `[startCol, endCol, url]` or null (OSC 8).
pub(super) async fn terminal_hyperlink_span(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(q): Query<super::types::TerminalCellQuery>,
) -> impl IntoResponse {
    // Answers null for a gone session rather than 404: a hover can outlive the
    // tab it started on, and the frontend reads "no link here" either way.
    let span = crate::pty::vt_read(&state, session_id, move |vt| {
        vt.grid_hyperlink_span(q.row, q.col)
    })
    .await;
    match span {
        Ok(span) => Json(serde_json::json!(span)).into_response(),
        Err(e) => read_failed_response(&e),
    }
}

pub(super) async fn terminal_get_lines(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<super::types::TerminalLinesQuery>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, move |vt| {
        vt.grid_get_lines(query.start, query.end)
    })
    .await
    {
        Ok(Some(lines)) => Json(serde_json::json!({"lines": lines})).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

/// The chat view of a Claude terminal from `from_seq`. A terminal with no bound
/// Claude agent answers `not_bound: <reason>`.
pub(super) async fn chat_view(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<ChatViewQuery>,
) -> Response {
    super::json_result(
        crate::chat_view::chat_view_snapshot_blocking(
            state,
            session_id,
            query.epoch,
            query.from_seq.unwrap_or(0),
        )
        .await,
    )
}

/// Serialize the whole grid for ONE client, without touching what the other
/// clients are about to receive.
///
/// Damage is tracked per session and `serialize_dirty_rows` CONSUMES it, so this
/// used to force full damage, serialize, and force it again — handing the damage
/// back at the cost of pinning the session into full frames for everyone and
/// re-arming the ticker. One browser that fell behind therefore made the desktop
/// decode 108 KB frames it had not asked for. `serialize_full_frame` reads the
/// same rows without consuming damage, without moving the `last_frame_*`
/// viewport state and without draining the bell, so a resync costs the other
/// transports nothing at all.
fn full_frame_for_single_client(state: &Arc<AppState>, session_id: &str) -> Option<Vec<u8>> {
    let vt = state.grid.vt_log_buffers.get(session_id)?;
    let frame = vt.lock().serialize_full_frame();
    if frame.is_empty() { None } else { Some(frame) }
}

/// Wrap packed row bytes as a binary body.
///
/// Deliberately not `Json(bytes)`: that spells a 141 KB chunk as ~350 KB of
/// decimal numbers for the client to parse back into the bytes it started as.
/// The desktop command hands the same payload over raw, and `rpcImpl` decides
/// between `arrayBuffer()` and `json()` on this header alone.
fn styled_rows_response(bytes: Vec<u8>) -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        bytes,
    )
}

/// Styled row range as packed bytes (same encoding as the desktop
/// `terminal_styled_rows` command). Fills the CanvasTerminal client-side row
/// cache so scrolled-back history renders during smooth scroll in browser mode
/// instead of showing blank rows. Returns an empty body when the session or
/// range is gone — the frontend treats that as "nothing to cache".
pub(super) async fn terminal_styled_rows(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<super::types::TerminalStyledRowsQuery>,
) -> axum::response::Response {
    match crate::pty::vt_read(&state, session_id, move |vt| {
        vt.grid_serialize_styled_range(query.start, query.count)
    })
    .await
    {
        Ok(bytes) => styled_rows_response(bytes).into_response(),
        Err(e) => read_failed_response(&e),
    }
}

pub(super) async fn terminal_get_cursor_line(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match crate::pty::vt_try_read(&state, session_id, |vt| vt.grid_get_cursor_line()).await {
        Ok(Some(text)) => Json(serde_json::json!({"text": text})).into_response(),
        Ok(None) => not_found_response(),
        Err(e) => read_failed_response(&e),
    }
}

pub(super) async fn terminal_request_frame(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let Some(vt) = state.grid.vt_log_buffers.get(&session_id) else {
        return session_not_found();
    };
    let frame = {
        let mut vt = vt.lock();
        vt.grid_force_full_damage();
        vt.serialize_dirty_rows()
    };
    crate::pty::send_grid_frame(&state, &session_id, frame);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

/// `GET /sessions/{id}/shell-family` — mirror of the `get_session_shell_family`
/// command. Answers the bare `ShellFamily` (or `null` for an unknown session),
/// which is what `src/utils/sendCommand.ts` reads over both transports.
pub(super) async fn get_session_shell_family(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let family = state
        .session_maps
        .sessions
        .get(&session_id)
        .map(|entry| crate::pty::classify_shell(&entry.lock().shell));
    Json(family)
}

#[cfg(test)]
mod tests {
    // Catches: POST /sessions binds the PTY key but publishes null to native
    // tabs, so later worktree declarations cannot find the caller.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn generic_post_sessions_created_event_retains_bound_identity() {
        let state = super::super::tests::test_state();
        let mut emitted = None;
        let result = super::spawn_pty_launch_with_emitter(
            state.clone(),
            super::PtyLaunch {
                shell: crate::test_support::host_shell().0.into(),
                cwd: None,
                rows: 24,
                cols: 80,
                worktree: None,
                // Same absent caller identity as create_session (POST /sessions).
                requested: super::RequestedIdentity::default(),
                direct: None,
            },
            |event, payload| {
                assert_eq!(event, "session-created");
                emitted = Some(payload);
            },
        );
        let session_id = result.expect("generic POST /sessions PTY must spawn");
        let bound = super::live_tuic_sessions_by_pty(&state)
            .remove(&session_id)
            .expect("generic session must bind its identity");
        crate::pty::close_pty_core(&state, &session_id, false);
        let payload = emitted.expect("generic native creation must publish");
        assert_eq!(bound, session_id);
        assert_eq!(payload["session_id"], session_id);
        assert_eq!(payload["tuic_session"], bound);
    }

    // Catches: native MCP/HTTP creation omits the live identity, so a new tab
    // cannot match its caller's worktree declaration until reconnect.
    #[cfg(feature = "desktop")]
    #[test]
    fn native_agent_created_event_retains_bound_identity() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut command = portable_pty::CommandBuilder::new("unused");
        crate::pty::bind_pty_identity(&state, &mut command, "pty-child", Some("stable-child"));
        for expected in [Some("stable-child"), Some("pty-child"), None] {
            match expected {
                Some("pty-child") => {
                    state.session_maps.live_pty_by_tuic_session.clear();
                    crate::pty::bind_pty_identity(&state, &mut command, "pty-child", None);
                }
                None => state.session_maps.live_pty_by_tuic_session.clear(),
                _ => {}
            }
            let mut emitted = None;
            super::emit_native_agent_session_created(
                |event, payload| {
                    assert_eq!(event, "session-created");
                    emitted = Some(payload);
                },
                &state,
                "pty-child",
                Some("/repo"),
                Some("ego"),
                Some("child"),
                Some("parent"),
            );
            let payload = emitted.expect("native event must be emitted");
            assert!(payload.get("tuic_session").is_some());
            assert_eq!(payload["tuic_session"], serde_json::json!(expected));
            assert_eq!(payload["session_id"], "pty-child");
            assert_eq!(payload["cwd"], "/repo");
            assert_eq!(payload["agent_type"], "ego");
            assert_eq!(payload["display_name"], "child");
            assert_eq!(payload["parent_session"], "parent");
        }
    }

    use super::*;

    /// Catches: HTTP detects a hand-launched agent but leaves the backend as a
    /// shell, rejecting submit and refusing a mail wake in the headless build.
    #[cfg(unix)]
    #[tokio::test]
    async fn http_foreground_hand_launched_claude_enables_submit_and_mail_wake() {
        use crate::pty::{AgentSubmissionWrite, PtyDelivery, SHELL_IDLE};
        use crate::test_support::ForegroundIdentityProbe;

        let state = super::super::tests::test_state();
        let sid = "http-hand-launched-claude";
        let probe = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
        let response = get_foreground_process(State(state.clone()), Path(sid.into()))
            .await
            .into_response();
        let snapshot = state.session_state_with_shell(sid).unwrap();
        let submit = crate::pty::write_agent_submission_to_pty(&state, sid, "task");
        // Model the next independently confirmed idle window, after submit.
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .store(SHELL_IDLE, Ordering::Release);
        state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .confirm_idle();
        let wake_allowed = crate::pty::managed_mail_wake_allowed(&state, sid);
        let wake = crate::pty::deliver_notice_to_pty(&state, sid, crate::pty::PEER_MAIL_WAKE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            body["agent"], "claude",
            "the OS foreground probe must detect the executable"
        );
        assert_eq!(
            snapshot.agent_state.as_deref(),
            Some("idle"),
            "HTTP must persist detected identity for lifecycle consumers"
        );
        assert!(
            matches!(submit, AgentSubmissionWrite::Complete { .. }),
            "submit: {submit:?}"
        );
        assert!(
            wake_allowed,
            "a recognised idle agent must be eligible for mail wake"
        );
        assert_eq!(wake, PtyDelivery::Typed);
        let written = probe.bytes.lock().unwrap().clone();
        assert!(written.windows(4).any(|part| part == b"task"));
        assert!(
            String::from_utf8(written)
                .unwrap()
                .contains("agent action=inbox")
        );
    }

    /// Catches: the HTTP path loses configured wrapper identity, or promotes a
    /// plain shell/unknown executable to an agent and enables unsafe injection.
    #[cfg(unix)]
    #[tokio::test]
    async fn http_foreground_preserves_wrapper_fallback_and_rejects_plain_shells() {
        for (name, preset, expected) in [
            ("cat", None, None),
            ("bash", None, None),
            ("cat", Some("claude"), Some("claude")),
            ("bash", Some("claude"), None),
        ] {
            let state = super::super::tests::test_state();
            let sid = "http-foreground-neighbour";
            let probe = if name == "bash" {
                crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, name)
            } else {
                crate::test_support::ForegroundIdentityProbe::new(state.clone(), sid, name)
            };
            {
                let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
                session.agent_type = preset.map(str::to_string);
                session.agent_type_from_run_config = preset.is_some();
            }
            let response = get_foreground_process(State(state.clone()), Path(sid.into()))
                .await
                .into_response();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                body["agent"],
                serde_json::json!(expected),
                "{name}, preset={preset:?}"
            );
            assert_eq!(
                state
                    .session_maps
                    .session_states
                    .get(sid)
                    .unwrap()
                    .agent_type
                    .as_deref(),
                preset,
                "run-config preset survives wrapper and shell foreground"
            );
            if preset.is_none() {
                assert_eq!(
                    state.session_state_with_shell(sid).unwrap().agent_state,
                    None
                );
                assert!(matches!(
                    crate::pty::write_agent_submission_to_pty(&state, sid, "task"),
                    crate::pty::AgentSubmissionWrite::Rejected {
                        reason: "not_managed_agent",
                        ..
                    }
                ));
                assert!(probe.bytes.lock().unwrap().is_empty());
            }
        }
    }

    /// Catches: mobile queue retaining its own draft or sending Enter mid-turn.
    #[cfg(unix)]
    #[tokio::test]
    async fn mobile_owned_draft_queues_without_mid_turn_enter() {
        let state = super::super::tests::test_state();
        let sid = "mobile-owned-draft";
        crate::test_support::agent_session(&state, sid, crate::pty::SHELL_BUSY);
        let bytes = crate::test_support::insert_recording_session(&state, sid);
        write_pty_input(&state, sid, "next task").unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "text": "next task", "expectedDraft": "next task"
        }))
        .unwrap();
        let response = enqueue_command(State(state.clone()), Path(sid.into()), Json(request))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(*bytes.lock().unwrap(), b"next task\x15");
        assert_eq!(
            state
                .session_maps
                .input_buffers
                .get(sid)
                .unwrap()
                .lock()
                .content(),
            ""
        );
        assert_eq!(
            crate::pty::list_queued_commands(&state, sid)[0].text,
            "next task"
        );
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("amp".into());
        state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .confirm_idle();
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .store(crate::pty::SHELL_IDLE, Ordering::Release);
        crate::pty::flush_pending_injections_blocking(&state, sid);
        crate::pty::flush_pending_injections_blocking(&state, sid);
        assert_eq!(*bytes.lock().unwrap(), b"next task\x15\x15next task\r");
        assert!(crate::pty::list_queued_commands(&state, sid).is_empty());
    }

    /// Catches: clearing somebody else's changed composer or a non-agent shell.
    #[cfg(unix)]
    #[tokio::test]
    async fn mobile_owned_draft_rejection_preserves_input() {
        for (sid, expected, agent) in [
            ("changed-draft", "old task", true),
            ("shell-draft", "new task", false),
        ] {
            let state = super::super::tests::test_state();
            if agent {
                crate::test_support::agent_session(&state, sid, crate::pty::SHELL_BUSY);
            }
            let bytes = crate::test_support::insert_recording_session(&state, sid);
            write_pty_input(&state, sid, "new task").unwrap();
            let request = serde_json::from_value(
                serde_json::json!({ "text": expected, "expectedDraft": expected }),
            )
            .unwrap();
            let response = enqueue_command(State(state.clone()), Path(sid.into()), Json(request))
                .await
                .into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_eq!(*bytes.lock().unwrap(), b"new task");
            assert_eq!(
                state
                    .session_maps
                    .input_buffers
                    .get(sid)
                    .unwrap()
                    .lock()
                    .content(),
                "new task"
            );
            assert!(crate::pty::list_queued_commands(&state, sid).is_empty());
        }
    }

    /// Catches: retrying an earlier job after a later acceptance duplicates the
    /// first job, or text-based deduplication silently drops the second job.
    #[cfg(unix)]
    #[tokio::test]
    async fn interleaved_bg_job_retries_preserve_two_identical_commands() {
        let state = super::super::tests::test_state();
        let sid = "interleaved-bg-retries";
        crate::test_support::agent_session(&state, sid, crate::pty::SHELL_BUSY);
        let bytes = crate::test_support::insert_recording_session(&state, sid);
        for key in ["job-a", "job-b", "job-a", "job-b"] {
            let request = serde_json::from_value(serde_json::json!({
                "text": "BG DONE", "idempotencyKey": key
            }))
            .unwrap();
            let response = enqueue_command(State(state.clone()), Path(sid.into()), Json(request))
                .await
                .into_response();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let queue = crate::pty::list_queued_commands(&state, sid);
        assert_eq!(queue.len(), 2, "two jobs survive; neither retry appends");
        assert!(queue.iter().all(|command| command.text == "BG DONE"));
        assert!(
            bytes.lock().unwrap().is_empty(),
            "busy composer is untouched"
        );
    }

    /// Catches: a lost queue reply lets a retry append and submit the same wake again.
    #[cfg(unix)]
    #[tokio::test]
    async fn queue_accepted_before_reply_is_lost_is_not_submitted_twice_on_retry() {
        let state = super::super::tests::test_state();
        let sid = "lost-queue-reply";
        crate::test_support::agent_session(&state, sid, crate::pty::SHELL_IDLE);
        // No external CLI acceptance is asserted: observe the production PTY bytes.
        let bytes = crate::test_support::insert_recording_session(&state, sid);
        for attempt in 0..2 {
            let request: EnqueueCommandRequest = serde_json::from_value(serde_json::json!({
                "text": "BG DONE", "idempotencyKey": "bg-job-1"
            }))
            .unwrap();
            let response = enqueue_command(State(state.clone()), Path(sid.into()), Json(request))
                .await
                .into_response();
            assert_eq!(response.status(), StatusCode::OK);
            if attempt == 0 {
                // Acceptance happened; the caller never receives this response.
                drop(response);
            } else {
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(
                    body,
                    serde_json::json!({"accepted": true, "typed": false, "queued": 0})
                );
            }
        }
        assert!(crate::pty::list_queued_commands(&state, sid).is_empty());
        assert_eq!(*bytes.lock().unwrap(), b"\x15BG DONE\r");
    }

    /// A silent agent may use the full confirmation window, but the async
    /// request handler must yield its runtime worker during that window.
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn enqueue_command_yields_while_confirmation_waits() {
        let state = super::super::tests::test_state();
        let sid = "http-queued-confirmation";
        crate::test_support::agent_session(&state, sid, crate::pty::SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("codex".into());
        crate::test_support::insert_recording_session(&state, sid);
        state.grid.vt_log_buffers.insert(
            sid.into(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        state
            .session_maps
            .output_buffers
            .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));

        let other_task_ran = Arc::new(AtomicBool::new(false));
        let marker = Arc::clone(&other_task_ran);
        tokio::spawn(async move {
            marker.store(true, Ordering::SeqCst);
        });
        let _ = enqueue_command(
            State(state),
            Path(sid.into()),
            Json(EnqueueCommandRequest {
                text: "wake".into(),
                expected_draft: None,
                idempotency_key: None,
            }),
        )
        .await;
        assert!(
            other_task_ran.load(Ordering::SeqCst),
            "the request must yield before its confirmation wait ends"
        );
    }

    /// Catches: a remote PTY spawn error is swallowed before the HTTP response.
    #[tokio::test]
    async fn remote_create_session_reports_missing_cwd_over_http() {
        let state = super::super::tests::test_state();
        let missing = state
            .data_dir
            .join(format!("missing-cwd-{}", Uuid::new_v4()));
        let app = super::super::build_router(state.clone(), false, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind isolated test server");
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/sessions"))
            .json(&serde_json::json!({
                "rows": 24,
                "cols": 80,
                "shell": tuic_test_support::host_shell().0,
                "cwd": missing.to_string_lossy(),
            }))
            .send()
            .await
            .expect("test server answers");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|message| message.contains("Working directory"))
        );
        assert!(
            state.session_maps.sessions.is_empty(),
            "a rejected cwd must not register a PTY"
        );

        let regular_file = state.data_dir.join(format!("file-cwd-{}", Uuid::new_v4()));
        std::fs::write(&regular_file, b"file").unwrap();
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/sessions"))
            .json(&serde_json::json!({
                "rows": 24,
                "cols": 80,
                "shell": tuic_test_support::host_shell().0,
                "cwd": regular_file.to_string_lossy(),
            }))
            .send()
            .await
            .expect("test server answers");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|message| message.contains("not a directory"))
        );
        assert!(state.session_maps.sessions.is_empty());
        server.abort();
    }

    /// Catches: attaching after output ended waits for a new watch update or skips existing scrollback.
    #[cfg(unix)]
    #[tokio::test]
    async fn grid_ws_attach_replays_existing_scrollback_without_new_output() {
        for (offset, first_line) in [(0, "history 7"), (7, "history 0")] {
            let state = super::super::tests::test_state();
            let sid = "existing-scrollback-replay";
            crate::state::tests_support::insert_dummy_session(&state, sid);
            // Feed our real terminal parser before any client attaches. No
            // reader/ticker publishes frames after the handshake.
            let mut vt = VtLogBuffer::new(4, 32, 1000);
            for line in 0..10 {
                vt.process(format!("history {line}\r\n").as_bytes());
            }
            assert_eq!(vt.grid_history_size(), 7);
            vt.grid_scroll_to_offset(offset);
            state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
            state
                .grid
                .watch
                .insert(sid.into(), crate::grid_watch::new_grid_watch());
            let app = super::super::build_router(state, false, true);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let _ = axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await;
            });
            let (mut socket, _) = tokio_tungstenite::connect_async(format!(
                "ws://{addr}/sessions/{sid}/stream?format=grid"
            ))
            .await
            .expect("complete attach setup before replay deadline");
            let replay = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next())
                .await
                .expect("existing history must replay without fresh PTY output")
                .expect("stream open")
                .expect("replay frame");
            assert!(
                replay.is_binary(),
                "attach must deliver a renderable grid frame"
            );
            let bytes = replay.into_data();
            assert_eq!(u16::from_le_bytes(bytes[0..2].try_into().unwrap()), 4);
            assert_eq!(
                u32::from_le_bytes(bytes[7..11].try_into().unwrap()),
                offset as u32
            );
            assert_eq!(u32::from_le_bytes(bytes[11..15].try_into().unwrap()), 7);
            // Full-frame row 0 starts after the 26-byte header and 4-byte
            // row header; each cell stores a UTF-32 character plus styling.
            let text: String = bytes[30..30 + 32 * 11]
                .as_chunks::<11>()
                .0
                .iter()
                .map(|cell| {
                    char::from_u32(u32::from_le_bytes(cell[0..4].try_into().unwrap())).unwrap()
                })
                .collect();
            assert_eq!(text.trim_end(), first_line);
            socket.close(None).await.expect("close disposable client");
            server.abort();
        }
    }

    /// Catches: a live but empty grid stream sends nothing on attach, causing
    /// the client replay watchdog to report a false failure on an idle PTY.
    #[cfg(unix)]
    #[tokio::test]
    async fn grid_ws_empty_replay_does_not_leave_healthy_idle_session_silent() {
        let state = super::super::tests::test_state();
        let sid = "empty-grid-replay";
        crate::state::tests_support::insert_dummy_session(&state, sid);
        state
            .grid
            .watch
            .insert(sid.into(), crate::grid_watch::new_grid_watch());
        assert!(!state.grid.vt_log_buffers.contains_key(sid));

        let app = super::super::build_router(state, false, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind isolated test server");
        let addr = listener.local_addr().expect("bound address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://{addr}/sessions/{sid}/stream?format=grid"
        ))
        .await
        .expect("complete WS setup before replay deadline");
        // This bound measures replay delivery, not setup reaching the server.
        let replay = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next())
            .await
            .expect("an idle session must explicitly finish its empty replay")
            .expect("stream must stay open")
            .expect("receive replay marker");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(replay.to_text().expect("text marker"))
                .expect("valid replay JSON"),
            serde_json::json!({"type": "grid-replay-empty"})
        );
        socket.close(None).await.expect("close disposable client");
        server.abort();
    }

    /// Catches: the empty-replay marker path returns or skips the live
    /// subscription, so a session that is idle at attach and prints later
    /// never reaches the client after the marker.
    #[cfg(unix)]
    #[tokio::test]
    async fn grid_ws_empty_replay_marker_is_followed_by_later_live_frames() {
        let state = super::super::tests::test_state();
        let sid = "empty-grid-then-output";
        crate::state::tests_support::insert_dummy_session(&state, sid);
        state
            .grid
            .watch
            .insert(sid.into(), crate::grid_watch::new_grid_watch());

        let app = super::super::build_router(state.clone(), false, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind isolated test server");
        let addr = listener.local_addr().expect("bound address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://{addr}/sessions/{sid}/stream?format=grid"
        ))
        .await
        .expect("complete WS setup before replay deadline");
        let marker = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next())
            .await
            .expect("marker must arrive")
            .expect("stream open")
            .expect("marker message");
        assert!(marker.is_text(), "the empty replay is a text control frame");

        let tx = state.grid.watch.get(sid).expect("watch channel").clone();
        crate::grid_watch::publish_grid_frame(&tx, vec![7, 7, 7]);
        let live = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next())
            .await
            .expect("a frame published after the marker must be delivered")
            .expect("stream must stay open after the marker")
            .expect("live frame message");
        assert!(live.is_binary(), "live output is a binary grid frame");
        socket.close(None).await.expect("close disposable client");
        server.abort();
    }

    /// Catches: the marker is sent as a bare text message on a negotiated
    /// socket, where the client reads every message as a tagged binary frame
    /// and would reject it (or ignore it, leaving the replay watchdog armed).
    #[cfg(unix)]
    #[tokio::test]
    async fn grid_ws_empty_replay_marker_is_tagged_on_a_negotiated_socket() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let state = super::super::tests::test_state();
        let sid = "empty-grid-negotiated";
        crate::state::tests_support::insert_dummy_session(&state, sid);
        state
            .grid
            .watch
            .insert(sid.into(), crate::grid_watch::new_grid_watch());

        let app = super::super::build_router(state, false, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind isolated test server");
        let addr = listener.local_addr().expect("bound address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        let mut request = format!("ws://{addr}/sessions/{sid}/stream?format=grid&compress=deflate")
            .into_client_request()
            .expect("client request");
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            super::super::ws_compression::DEFLATE_SUBPROTOCOL
                .parse()
                .expect("header value"),
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .expect("complete WS setup before replay deadline");
        let marker = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next())
            .await
            .expect("marker must arrive")
            .expect("stream open")
            .expect("marker message");
        let bytes = marker.into_data();
        let tag = *bytes.first().expect("tagged frame is never empty");
        assert!(
            tag == 0x02 || tag == 0x03,
            "marker must carry a text tag on a tagged socket, got {tag:#04x}"
        );
        if tag == 0x02 {
            let json: serde_json::Value =
                serde_json::from_slice(&bytes[1..]).expect("tagged text body is JSON");
            assert_eq!(json, serde_json::json!({"type": "grid-replay-empty"}));
        }
        socket.close(None).await.expect("close disposable client");
        server.abort();
    }

    /// One WebSocket handshake, start to finish, against a real socket.
    ///
    /// `oneshot` cannot reach this: `WebSocketUpgrade` reads hyper's `OnUpgrade`
    /// out of the request extensions, which only a served connection puts there,
    /// and rejects the request without it. So the only way to see whether the
    /// response actually carries the subprotocol is to speak HTTP at a listener.
    #[cfg(unix)]
    async fn handshake_response_head(query: &str, offer_subprotocol: bool) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let state = super::super::tests::test_state();
        let session_id = "ws-subprotocol-handshake";
        crate::state::tests_support::insert_dummy_session(&state, session_id);

        let app = super::super::build_router(state, false, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a loopback port");
        let addr = listener.local_addr().expect("the port just bound");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });

        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to the test server");
        let offer = if offer_subprotocol {
            format!("Sec-WebSocket-Protocol: {DEFLATE_SUBPROTOCOL}\r\n")
        } else {
            String::new()
        };
        // A fixed key: the handshake's own accept value is not what this test is
        // about, and a random one would only make the request harder to read.
        let request = format!(
            "GET /sessions/{session_id}/stream?{query} HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             {offer}\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write the upgrade request");

        // Read until the blank line: everything after it is WebSocket frames,
        // and reading to EOF would block until the handler gives up.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                stream.read_exact(&mut byte),
            )
            .await
            {
                Ok(Ok(_)) => head.push(byte[0]),
                other => panic!("the server did not finish its response head: {other:?}"),
            }
        }
        server.abort();
        String::from_utf8(head).expect("an HTTP response head is ASCII")
    }

    /// **The acknowledgement, on the wire.** A client cannot tell a tagging
    /// server from one that ignored `?compress=deflate` by looking at its own
    /// request, and guessing wrong strips the first byte off every grid frame.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_tagging_server_names_the_subprotocol_in_its_handshake() {
        let head = handshake_response_head("format=grid&compress=deflate", true).await;

        assert!(
            head.starts_with("HTTP/1.1 101"),
            "the upgrade must succeed: {head}"
        );
        assert!(
            head.to_ascii_lowercase()
                .contains(&format!("sec-websocket-protocol: {DEFLATE_SUBPROTOCOL}")),
            "a tagged socket must say so on the handshake: {head}"
        );
    }

    /// The other half, and the one that keeps an untagged socket readable: a
    /// client that offers the subprotocol but does not ask for compression gets
    /// the original framing, so the server must not claim otherwise.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_untagged_socket_selects_no_subprotocol() {
        let head = handshake_response_head("format=grid", true).await;

        assert!(
            head.starts_with("HTTP/1.1 101"),
            "the upgrade must succeed: {head}"
        );
        assert!(
            !head
                .to_ascii_lowercase()
                .contains("sec-websocket-protocol:"),
            "an untagged socket must not claim a subprotocol: {head}"
        );
    }

    // Production builds a grid through `AppState::new_vt_log_buffer` so it picks
    // up the config; tests that only exercise the grid construct it directly.
    use crate::state::VtLogBuffer;

    #[cfg(unix)]
    struct WriteProbe {
        writes: Arc<parking_lot::Mutex<Vec<Vec<u8>>>>,
        flushes: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[cfg(unix)]
    impl std::io::Write for WriteProbe {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes.lock().push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
    }

    #[cfg(unix)]
    #[test]
    fn split_escape_then_slash_opens_slash_mode_but_concatenated_input_does_not() {
        let state = super::super::tests::test_state();
        let split_session_id = "split-escape-slash";
        crate::state::tests_support::insert_dummy_session(&state, split_session_id);

        write_pty_input_parts(&state, split_session_id, &["\x1b", "/"])
            .expect("split input writes to the PTY");

        let split_slash_mode = state
            .session_maps
            .slash_mode
            .get(split_session_id)
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(
            split_slash_mode,
            "Escape and slash delivered as separate parts must open slash mode"
        );

        let concatenated_session_id = "concatenated-escape-slash";
        crate::state::tests_support::insert_dummy_session(&state, concatenated_session_id);
        write_pty_input(&state, concatenated_session_id, "\x1b/")
            .expect("concatenated input writes to the PTY");

        let concatenated_slash_mode = state
            .session_maps
            .slash_mode
            .get(concatenated_session_id)
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(
            !concatenated_slash_mode,
            "the concatenated Escape/slash request must remain distinguishable from two parts"
        );
    }

    #[cfg(unix)]
    #[test]
    fn split_choice_key_clears_the_prompt_but_concatenated_input_does_not() {
        fn choice_state() -> crate::state::SessionState {
            crate::state::SessionState {
                awaiting_input: true,
                choice_prompt: Some(crate::output_parser::ChoicePromptPayload {
                    title: "Choose an option".to_string(),
                    options: vec![crate::output_parser::ChoiceOption {
                        key: "1".to_string(),
                        label: "Proceed".to_string(),
                        highlighted: true,
                        destructive: false,
                        hint: None,
                    }],
                    selection_mode: None,
                    dismiss_key: None,
                    amend_key: None,
                }),
                ..Default::default()
            }
        }

        let state = super::super::tests::test_state();
        let concatenated_session_id = "concatenated-choice-key";
        crate::state::tests_support::insert_dummy_session(&state, concatenated_session_id);
        state
            .session_maps
            .session_states
            .insert(concatenated_session_id.to_string(), choice_state());
        write_pty_input(&state, concatenated_session_id, "1x")
            .expect("concatenated input writes to the PTY");
        assert!(
            state
                .session_maps
                .session_states
                .get(concatenated_session_id)
                .unwrap()
                .choice_prompt
                .is_some(),
            "a concatenated option key must not resolve an exact-key choice prompt"
        );

        let split_session_id = "split-choice-key";
        crate::state::tests_support::insert_dummy_session(&state, split_session_id);
        state
            .session_maps
            .session_states
            .insert(split_session_id.to_string(), choice_state());
        write_pty_input_parts(&state, split_session_id, &["1", "x"])
            .expect("split input writes to the PTY");
        assert!(
            state
                .session_maps
                .session_states
                .get(split_session_id)
                .unwrap()
                .choice_prompt
                .is_none(),
            "an exact option key delivered as its own part must resolve the choice prompt"
        );
    }

    /// A full OS pipe, not a mocked writer: the next byte blocks in the kernel
    /// until the read end is dropped. The bounded release keeps RED runs finite.
    #[cfg(unix)]
    fn full_kernel_pipe() -> (std::fs::File, std::fs::File) {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut fds = [-1; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
        let fd = writer.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        loop {
            match std::io::Write::write(&mut writer, &[0; 4096]) {
                Ok(n) => assert!(n > 0),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("fill kernel pipe: {error}"),
            }
        }
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFL, flags) }, 0);
        (reader, writer)
    }

    /// Catches: a blocked kernel PTY write parks an HTTP runtime worker and
    /// prevents the request from returning its own bounded uncertainty error.
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn blocked_kernel_write_returns_a_bounded_http_error() {
        let state = super::super::tests::test_state();
        let sid = "blocked-http-write";
        crate::state::tests_support::insert_dummy_session(&state, sid);
        let (reader, writer) = full_kernel_pipe();
        state.session_maps.sessions.get(sid).unwrap().lock().writer =
            Arc::new(parking_lot::Mutex::new(Box::new(writer)));
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            drop(reader);
        });
        let started = std::time::Instant::now();
        let response = write_to_session(
            State(state.clone()),
            Path(sid.to_string()),
            Json(WriteRequest { data: "x".into() }),
        )
        .await
        .into_response();
        let elapsed = started.elapsed();
        release.join().unwrap();
        crate::pty::close_pty_core(&state, sid, false);
        assert!(response.status().is_server_error());
        assert!(
            elapsed < std::time::Duration::from_secs(3),
            "blocked write held HTTP for {elapsed:?}"
        );
    }

    /// Catches: close waits behind a blocked writer before it can kill the
    /// session, so its nominal 100ms grace window is never reached.
    #[cfg(unix)]
    #[test]
    fn close_reaches_kill_fallback_while_kernel_writer_is_blocked() {
        let state = super::super::tests::test_state();
        let sid = "blocked-writer-close";
        crate::state::tests_support::insert_dummy_session(&state, sid);
        let (reader, writer) = full_kernel_pipe();
        let shared: crate::state::SharedPtyWriter =
            Arc::new(parking_lot::Mutex::new(Box::new(writer)));
        state.session_maps.sessions.get(sid).unwrap().lock().writer = shared.clone();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let blocked = std::thread::spawn(move || {
            let mut writer = shared.lock();
            locked_tx.send(()).unwrap();
            let _ = std::io::Write::write_all(writer.as_mut(), b"x");
        });
        locked_rx.recv().unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            drop(reader);
        });
        let started = std::time::Instant::now();
        crate::pty::close_pty_core(&state, sid, false);
        let elapsed = started.elapsed();
        release.join().unwrap();
        blocked.join().unwrap();
        assert!(!state.session_maps.sessions.contains_key(sid));
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "close waited for writer for {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn n_input_parts_share_one_writer_flush_and_preserve_write_boundaries() {
        let state = super::super::tests::test_state();
        let session_id = "n-part-single-lock";
        crate::state::tests_support::insert_dummy_session(&state, session_id);
        let writes = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let flushes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let writer: crate::state::SharedPtyWriter =
            Arc::new(parking_lot::Mutex::new(Box::new(WriteProbe {
                writes: Arc::clone(&writes),
                flushes: Arc::clone(&flushes),
            })));
        state
            .session_maps
            .sessions
            .get(session_id)
            .unwrap()
            .lock()
            .writer = writer;

        write_pty_input_parts(&state, session_id, &["first", "second", "third"])
            .expect("all parts write to the PTY");

        assert_eq!(
            flushes.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "N parts must use one write_pty_parts call, which flushes once"
        );
        assert_eq!(
            *writes.lock(),
            vec![b"first".to_vec(), b"second".to_vec(), b"third".to_vec()],
            "the single locked write must still deliver every input as its own part"
        );
    }

    #[test]
    fn mcp_regression_input_bookkeeping_releases_guard_before_pending_delivery() {
        let state = super::super::tests::test_state();
        let session_id = "deadlock-regression";
        state.session_maps.session_states.insert(
            session_id.to_string(),
            crate::state::SessionState {
                agent_type: Some("codex".to_string()),
                ..Default::default()
            },
        );
        state
            .pending_injections
            .entry(session_id.to_string())
            .or_default()
            .push_back(crate::state::PendingInjection::notice("queued message"));

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            apply_input_bookkeeping(&state, session_id, "\r");
            let _ = done_tx.send(());
        });

        done_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("input bookkeeping must not self-deadlock while checking pending delivery");
    }

    #[test]
    fn bare_enter_uses_the_same_submission_bookkeeping_as_desktop_input() {
        let state = super::super::tests::test_state();
        let session_id = "http-bare-enter";
        state.session_maps.session_states.insert(
            session_id.to_string(),
            crate::state::SessionState {
                agent_type: Some("codex".to_string()),
                awaiting_input: true,
                question_text: Some("Apply these edits?".to_string()),
                question_confident: true,
                ..Default::default()
            },
        );
        let mut events = state.event_bus.subscribe();

        apply_input_bookkeeping(&state, session_id, "\r");

        assert_eq!(
            state
                .session_maps
                .session_states
                .get(session_id)
                .unwrap()
                .turn_epoch,
            1
        );
        let event = events.try_recv().expect("bare Enter emits UserInput");
        let crate::state::AppEvent::PtyParsed { parsed, .. } = event else {
            panic!("expected parsed input event");
        };
        assert_eq!(parsed["type"], "user-input");
        assert_eq!(parsed["content"], "");
    }

    // is_separator_line tests live in chrome.rs (canonical location)

    // --- trim_screen_chrome ---

    #[test]
    fn trim_removes_prompt_and_separator() {
        let rows: Vec<String> = vec![
            "content line 1".into(),
            "content line 2".into(),
            "────────────────────────────────────────".into(),
            "❯ ".into(),
            "────────────────────────────────────────".into(),
            "  [Opus 4.6 | Max] tuicommander git:(main)".into(),
            "  ⏵⏵ bypass permissions on".into(),
        ];
        let result = trim_screen_chrome(&rows);
        assert_eq!(result.cutoff, 2);
    }

    #[test]
    fn trim_handles_decorated_separator_with_badge() {
        let rows: Vec<String> = vec![
            "some output".into(),
            "──────────────────────────────── pwa ──".into(),
            "❯ hello".into(),
            "──────────────────────────────── pwa ──".into(),
            "  status bar".into(),
        ];
        let result = trim_screen_chrome(&rows);
        assert_eq!(result.cutoff, 1);
    }

    #[test]
    fn trim_no_chrome_keeps_all() {
        let rows: Vec<String> = vec!["line 1".into(), "line 2".into(), "line 3".into()];
        let result = trim_screen_chrome(&rows);
        assert_eq!(result.cutoff, 3);
        // The caller keeps its rows: the trim only reports a cutoff.
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn trim_empty_input() {
        let result = trim_screen_chrome(&[]);
        assert_eq!(result.cutoff, 0);
    }

    // Catches: mobile log capture or chrome trimming drops a tool header while
    // later output remains visible under an older header.
    #[test]
    fn captured_claude_consecutive_tools_keep_mobile_headers() {
        for fixture in [
            include_bytes!("../fixtures/agent_prompts/claude-mobile-consecutive-tools-2109.tcap")
                .as_slice(),
            include_bytes!("../fixtures/agent_prompts/claude-mobile-verbose-tools-2109.tcap")
                .as_slice(),
        ] {
            let capture = crate::pty_capture::decode_capture(fixture)
                .expect("recorded Claude consecutive tool calls");
            let (rows, cols) = capture.geometry.expect("recorded geometry");
            let mut grid = crate::state::VtLogBuffer::new(rows, cols, 10_000);
            let mut utf8 = crate::state::Utf8ReadBuffer::new();
            let mut escape = crate::state::EscapeAwareBuffer::new();
            let mut checked = 0;
            for (index, record) in capture.records.into_iter().enumerate() {
                if record.direction != crate::pty_capture::CaptureDirection::Output {
                    continue;
                }
                let data = utf8.push(&record.data);
                let data = escape.push(&data);
                let (clean, _) = crate::state::strip_kitty_sequences(&data);
                grid.process(clean.as_bytes());
                let (log, _) = grid.lines_since_owned(0, usize::MAX);
                let mut mobile: Vec<_> = log.iter().map(|line| line.text()).collect();
                let screen = grid.screen_log_lines();
                mobile.extend(
                    screen
                        .into_iter()
                        .take(screen_chrome_cutoff(&grid).cutoff)
                        .map(|line| line.text()),
                );
                let mobile = mobile.join("\n");
                for header in grid
                    .grid_get_lines(0, grid.grid_total_lines())
                    .into_iter()
                    .filter(|row| row.starts_with("⏺ Bash(") || row.starts_with("⏺ Write("))
                {
                    checked += 1;
                    assert!(
                        mobile.contains(header.trim_end()),
                        "record {index}: mobile dropped canonical header {header:?}\n{mobile}"
                    );
                }
            }
            assert!(
                checked > 6,
                "capture must exercise consecutive tool headers across frames"
            );
            let headers = grid
                .grid_get_lines(0, grid.grid_total_lines())
                .into_iter()
                .filter(|row| row.starts_with("⏺ Bash(") || row.starts_with("⏺ Write("))
                .count();
            assert_eq!(
                headers, 6,
                "capture must finish all three Bash/Write rounds"
            );
        }
    }

    // Catches: the highlighted Claude option is mistaken for the composer prompt,
    // so the mobile screen loses the remaining choices and its selection footer.
    #[test]
    fn captured_claude_askuser_dialog_survives_mobile_screen_trim() {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "../fixtures/agent_prompts/claude-askuser-esc-20260929.tcap"
        ))
        .expect("real Claude capture");
        let (rows, cols) = capture.geometry.expect("captured terminal geometry");
        let mut grid = crate::state::VtLogBuffer::new(rows, cols, 2000);
        let mut utf8 = crate::state::Utf8ReadBuffer::new();
        let mut escape = crate::state::EscapeAwareBuffer::new();
        let mut dialog_rows = None;
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                let data = utf8.push(&record.data);
                let data = escape.push(&data);
                let (clean, _) = crate::state::strip_kitty_sequences(&data);
                grid.process(clean.as_bytes());
                let rows = grid.screen_rows();
                if rows
                    .iter()
                    .any(|row| row.contains("Which color do you prefer?"))
                    && rows.iter().any(|row| row.contains("Enter to select"))
                {
                    dialog_rows = Some(rows);
                }
            }
        }
        let rows = dialog_rows.expect("the captured dialog must render before Esc");
        let visible = &rows[..trim_screen_chrome(&rows).cutoff];
        for expected in [
            "Which color do you prefer?",
            "1. Red",
            "2. Green",
            "3. Blue",
            "4. Type something.",
            "5. Chat about this",
            "Enter to select",
        ] {
            assert!(
                visible.iter().any(|row| row.contains(expected)),
                "mobile screen lost {expected:?}: {visible:#?}"
            );
        }
    }

    // Catches: the resume search-box border or highlighted session is treated
    // as composer chrome, removing session rows from HTTP and log-WS screens.
    #[test]
    fn captured_claude_mobile_resume_survives_screen_trim() {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "../fixtures/agent_prompts/claude-mobile-resume-1971.tcap"
        ))
        .expect("real Claude resume capture");
        let (rows, cols) = capture.geometry.expect("captured terminal geometry");
        let mut grid = crate::state::VtLogBuffer::new(rows, cols, 2000);
        let mut utf8 = crate::state::Utf8ReadBuffer::new();
        let mut escape = crate::state::EscapeAwareBuffer::new();
        let mut populated = false;
        let mut searched = false;
        let mut ready_rows = None;
        let mut dialog_rows = None;
        for record in capture.records {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                continue;
            }
            let data = utf8.push(&record.data);
            let data = escape.push(&data);
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            grid.process(clean.as_bytes());
            let rows = grid.screen_rows();
            if rows.iter().any(|row| row.trim() == "❯") {
                ready_rows = Some(rows.clone());
            }
            if !rows.iter().any(|row| row.trim() == "Resume session") {
                continue;
            }
            let expected = if rows.iter().any(|row| row.contains("❯ Mobile resume seed"))
                && rows.iter().any(|row| row.contains("Esc to cancel"))
            {
                populated = true;
                [
                    "Mobile resume seed",
                    "Test color selection question",
                    "Esc to cancel",
                ]
            } else if rows
                .iter()
                .any(|row| row.contains("No sessions match \"z\"."))
                && rows.iter().any(|row| row.contains("Esc to clear"))
            {
                searched = true;
                ["│ ⌕ z", "No sessions match \"z\".", "Esc to clear"]
            } else {
                continue;
            };
            let visible = &rows[..trim_screen_chrome(&rows).cutoff];
            let streamed = poll_screen(&grid, 0, true).screen.expect("log-WS screen");
            for expected in expected {
                assert!(
                    visible.iter().any(|row| row.contains(expected)),
                    "HTTP screen lost {expected:?}: {visible:#?}"
                );
                assert!(
                    streamed.iter().any(|row| row.text().contains(expected)),
                    "log-WS screen lost {expected:?}"
                );
            }
            // Rendering a modal is not permission to widen the parser's input.
            let refs: Vec<_> = rows.iter().map(String::as_str).collect();
            assert!(find_chrome_cutoff(&refs).is_some());
            dialog_rows = Some(rows);
        }
        assert!(populated, "capture must contain the populated picker");
        assert!(searched, "capture must contain the no-matches search state");
        // A retained modal transcript above a ready composer is not an open
        // picker: its old footer must not disable ordinary composer trimming.
        let mut dismissed = dialog_rows.unwrap();
        dismissed.extend(ready_rows.expect("captured ready composer"));
        assert!(trim_screen_chrome(&dismissed).cutoff < dismissed.len());
    }

    // Catches: selection markers and modal rules mistaken for composer chrome,
    // dropping the options from both mobile screen transports.
    fn assert_captured_mobile_dialog(capture: &[u8], expected: &[&str]) {
        let capture = crate::pty_capture::decode_capture(capture).expect("real Claude capture");
        let (rows, cols) = capture.geometry.expect("captured geometry");
        let mut grid = crate::state::VtLogBuffer::new(rows, cols, 2000);
        let mut utf8 = crate::state::Utf8ReadBuffer::new();
        let mut escape = crate::state::EscapeAwareBuffer::new();
        let mut observed = false;
        for record in capture.records {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                continue;
            }
            let data = utf8.push(&record.data);
            let data = escape.push(&data);
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            grid.process(clean.as_bytes());
            let rows = grid.screen_rows();
            if !expected
                .iter()
                .all(|text| rows.iter().any(|row| row.contains(text)))
            {
                continue;
            }
            observed = true;
            let visible = &rows[..trim_screen_chrome(&rows).cutoff];
            let streamed = poll_screen(&grid, 0, true).screen.expect("log-WS screen");
            for text in expected {
                assert!(
                    visible.iter().any(|row| row.contains(text)),
                    "HTTP screen lost {text:?}: {visible:#?}"
                );
                assert!(
                    streamed.iter().any(|row| row.text().contains(text)),
                    "log-WS screen lost {text:?}"
                );
            }
            // Old modal rows retained above a new composer must not bypass trim.
            let mut dismissed = rows;
            dismissed.extend([
                "────────────────────".into(),
                "❯ ".into(),
                "────────────────────".into(),
                "status".into(),
            ]);
            assert!(trim_screen_chrome(&dismissed).cutoff < dismissed.len());
        }
        assert!(
            observed,
            "capture must contain the complete dialog: {expected:?}"
        );
    }

    #[test]
    fn captured_mobile_trust_dialog_keeps_options() {
        assert_captured_mobile_dialog(
            include_bytes!("../fixtures/agent_prompts/claude-mobile-startup-1987.tcap"),
            &[
                "Security guide",
                "No, exit",
                "Yes, I trust this folder",
                "Enter to confirm",
            ],
        );
    }

    #[test]
    fn captured_mobile_imports_dialog_keeps_options() {
        assert_captured_mobile_dialog(
            include_bytes!("../fixtures/agent_prompts/claude-mobile-startup-1987.tcap"),
            &[
                "Allow external CLAUDE.md file imports?",
                "No, disable external imports",
                "Yes, allow external imports",
                "Enter to confirm",
            ],
        );
    }

    #[test]
    fn captured_mobile_permission_dialog_keeps_options() {
        assert_captured_mobile_dialog(
            include_bytes!("../fixtures/agent_prompts/claude-mobile-permission-1987.tcap"),
            &[
                "Do you want to proceed?",
                "1. Yes",
                "2. Yes, and always allow access to",
                "3. Yes, and switch to auto mode",
                "4. No",
                "Esc to cancel · Tab to amend",
            ],
        );
    }

    // Catches: retained agent chrome hides shell output and revives the old input
    // after a handled exit or SIGTERM, even though the canonical grid advanced.
    #[tokio::test]
    async fn captured_agent_exit_log_projection_keeps_shell_output() {
        let mut failures = Vec::new();
        for (mode, bytes, marker) in [
            (
                "normal",
                include_bytes!("../fixtures/agent_prompts/claude-exit-normal-2178.raw").as_slice(),
                "SHELL_NORMAL_2178",
            ),
            (
                "sigterm",
                include_bytes!("../fixtures/agent_prompts/claude-exit-sigterm-2178.raw").as_slice(),
                "SHELL_SIGTERM_2178",
            ),
        ] {
            let state = super::super::tests::test_state();
            // Recorded PTY geometry; the captured /sessions snapshot has no agent
            // identity after shell return. Do not infer identity from old rows.
            let mut buf = VtLogBuffer::new(41, 128, 10_000);
            buf.process(bytes);
            assert!(
                buf.grid_get_lines(0, buf.grid_total_lines())
                    .join("\n")
                    .contains(marker),
                "{mode}: fixture must contain executed shell output"
            );
            state
                .grid
                .vt_log_buffers
                .insert(mode.into(), parking_lot::Mutex::new(buf));
            let response = get_output(
                State(state),
                Path(mode.into()),
                Query(serde_json::from_value(serde_json::json!({"format":"log"})).unwrap()),
            )
            .await
            .into_response();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let text = ["lines", "screen"]
                .into_iter()
                .flat_map(|key| json[key].as_array().unwrap())
                .flat_map(|row| row["spans"].as_array().unwrap())
                .map(|span| span["text"].as_str().unwrap())
                .collect::<String>();
            if !text.contains(marker)
                || !text.contains("shell2178$")
                || json.get("input_line").is_some()
            {
                failures.push(format!(
                    "{mode}: output={}, prompt={}, stale_input={:?}",
                    text.contains(marker),
                    text.contains("shell2178$"),
                    json.get("input_line")
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    // Catches: the log WS keeps its old trim/hash after identity release when
    // no further PTY bytes arrive to repaint the retained agent prompt.
    #[test]
    fn screen_poll_releases_retained_chrome_when_agent_identity_is_gone() {
        let mut buf = VtLogBuffer::new(41, 128, 10_000);
        buf.process(include_bytes!(
            "../fixtures/agent_prompts/claude-exit-sigterm-2178.raw"
        ));
        let before = poll_screen(&buf, 0, true);
        assert_eq!(before.input_line.as_deref(), Some("✔ Dark mode"));
        let after = poll_screen(&buf, before.hash, false);
        let text = after
            .screen
            .expect("identity release must publish the untrimmed screen")
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("SHELL_SIGTERM_2178"));
        assert!(text.contains("shell2178$"));
        assert!(after.input_line.is_none());
        assert!(poll_screen(&buf, after.hash, false).screen.is_none());
    }

    // --- Log-WS screen polling (604-cb45 F14) ---
    //
    // The poll runs 5x/s per connected log client. Building the styled screen
    // before asking whether it changed made an idle session pay for a full
    // Vec<LogLine> — under the VtLogBuffer mutex — and throw it away.

    fn vt_log_with(output: &str) -> crate::state::VtLogBuffer {
        let mut buf = crate::state::VtLogBuffer::new(24, 80, 1000);
        buf.process(output.as_bytes());
        buf
    }

    #[test]
    fn screen_poll_materializes_only_on_a_change() {
        let buf = vt_log_with("hello world\r\n");

        let first = poll_screen(&buf, 0, true);
        assert!(
            first.screen.is_some(),
            "the first poll has nothing to compare against"
        );

        // Same buffer, same hash: nothing to send, so nothing to build.
        let second = poll_screen(&buf, first.hash, true);
        assert!(
            second.screen.is_none(),
            "an unchanged screen must not be materialized"
        );
        assert_eq!(
            second.hash, first.hash,
            "the signature must be stable across polls"
        );
    }

    #[test]
    fn screen_poll_reports_a_change_after_new_output() {
        let mut buf = vt_log_with("first\r\n");
        let first = poll_screen(&buf, 0, true);

        buf.process(b"second\r\n");
        let second = poll_screen(&buf, first.hash, true);

        assert!(second.screen.is_some(), "new output must reach the client");
        assert_ne!(second.hash, first.hash);
    }

    #[test]
    fn screen_poll_returns_the_same_rows_the_old_path_built() {
        let buf = vt_log_with("alpha\r\nbeta\r\n");

        let polled = poll_screen(&buf, 0, true)
            .screen
            .expect("a fresh screen is a change");
        let expected: Vec<_> = buf
            .screen_log_lines()
            .into_iter()
            .take(screen_chrome_cutoff(&buf).cutoff)
            .collect();

        assert_eq!(polled.len(), expected.len());
        for (got, want) in polled.iter().zip(expected.iter()) {
            assert_eq!(
                got.spans
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>(),
                want.spans
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn screen_poll_treats_a_blank_screen_as_no_change() {
        // A buffer that has run but shows nothing: the old code sent no frame
        // because the styled screen came back empty after trailing-blank
        // trimming, and the cheap hash must not start sending one.
        let buf = vt_log_with("");
        assert!(poll_screen(&buf, 0, true).screen.is_none());
    }

    // --- WebSocket catch-up/subscribe race ---
    //
    // Regression guard for the ring-buffer catch-up race: the PTY writer and
    // the WS handler must serialize `ring.write` + `ws_clients.send` on both
    // sides via `ring.lock()`. If the two sides drift into separate critical
    // sections, bytes written during the window are delivered twice (once in
    // the catch-up snapshot, once through the live mpsc queue).
    //
    // This test reproduces the contract with tight concurrent loops and
    // asserts that every written byte is observed exactly once by a late
    // subscriber that reads its snapshot + drains its mpsc queue.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ws_catchup_no_duplicate_with_concurrent_writer() {
        use crate::state::OutputRingBuffer;
        use parking_lot::Mutex as PlMutex;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU64, Ordering};

        const CHUNK_COUNT: u64 = 5_000;
        const CHUNK_SIZE: usize = 8; // just the index, no filler
        const CAPACITY: usize = CHUNK_COUNT as usize * CHUNK_SIZE * 2;
        // Subscriber waits until the writer has committed this many chunks
        // before attaching — guarantees a non-empty snapshot without relying
        // on wall-clock sleep.
        const ATTACH_AFTER: u64 = 100;

        let ring: Arc<PlMutex<OutputRingBuffer>> =
            Arc::new(PlMutex::new(OutputRingBuffer::new(CAPACITY)));
        let clients: Arc<PlMutex<Vec<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>> =
            Arc::new(PlMutex::new(Vec::new()));
        let writer_progress: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
        let subscriber_attached: Arc<std::sync::atomic::AtomicBool> =
            Arc::new(std::sync::atomic::AtomicBool::new(false));

        // PTY writer: mirror the pty.rs critical section — ring.write + broadcast
        // under a single ring.lock().
        let writer_ring = ring.clone();
        let writer_clients = clients.clone();
        let writer_progress_w = writer_progress.clone();
        let subscriber_attached_w = subscriber_attached.clone();
        let writer = tokio::task::spawn_blocking(move || {
            for i in 0..CHUNK_COUNT {
                let payload = i.to_be_bytes().to_vec();

                // Mirror pty.rs: ring.write + broadcast under one ring.lock().
                let mut ring_guard = writer_ring.lock();
                ring_guard.write(&payload);
                {
                    let mut subs = writer_clients.lock();
                    subs.retain(|tx| tx.send(payload.clone()).is_ok());
                }
                drop(ring_guard);

                writer_progress_w.store(i + 1, Ordering::Release);

                // After ATTACH_AFTER chunks, wait for the subscriber to attach
                // before continuing — ensures the race window is exercised.
                if i + 1 == ATTACH_AFTER {
                    while !subscriber_attached_w.load(Ordering::Acquire) {
                        std::hint::spin_loop();
                    }
                }
            }
        });

        // Subscriber: attach mid-stream. Spins on the atomic counter instead
        // of sleeping, so the race window is exercised deterministically.
        let sub_ring = ring.clone();
        let sub_clients = clients.clone();
        let writer_progress_s = writer_progress.clone();
        let subscriber_attached_s = subscriber_attached.clone();
        let subscriber = tokio::task::spawn_blocking(move || {
            // Spin until the writer has made enough progress for a non-empty snapshot.
            while writer_progress_s.load(Ordering::Acquire) < ATTACH_AFTER {
                std::hint::spin_loop();
            }

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let (snapshot_bytes, snapshot_total) = {
                let r = sub_ring.lock();
                let snap = r.read_last(CAPACITY);
                sub_clients.lock().push(tx);
                snap
            };
            // Signal the writer that we're attached — it can resume writing.
            subscriber_attached_s.store(true, Ordering::Release);
            (snapshot_bytes, snapshot_total, rx)
        });

        let (snapshot_bytes, snapshot_total, mut rx) = subscriber.await.unwrap();
        writer.await.unwrap();

        // Drain remaining live frames.
        let mut live_bytes: Vec<u8> = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            live_bytes.extend_from_slice(&chunk);
        }

        // Reconstruct the chunk indices seen by the subscriber. Every chunk
        // is exactly CHUNK_SIZE bytes = one u64 BE index.
        let extract_indices = |bytes: &[u8]| -> Vec<u64> {
            bytes
                .as_chunks::<CHUNK_SIZE>()
                .0
                .iter()
                .map(|c| u64::from_be_bytes(*c))
                .collect()
        };

        let snapshot_indices = extract_indices(&snapshot_bytes);
        let live_indices = extract_indices(&live_bytes);

        // Precondition: both streams must be non-empty, otherwise the race
        // window was not exercised and the boundary invariants are vacuous.
        assert!(
            !snapshot_indices.is_empty(),
            "subscriber attached too late — no snapshot data"
        );
        assert!(
            !live_indices.is_empty(),
            "writer finished before subscriber — race not exercised"
        );

        // Invariants:
        // 1. Snapshot indices are strictly monotonically increasing by 1.
        for pair in snapshot_indices.windows(2) {
            assert_eq!(pair[1], pair[0] + 1, "snapshot not contiguous: {:?}", pair);
        }
        // 2. Live indices are strictly monotonically increasing by 1.
        for pair in live_indices.windows(2) {
            assert_eq!(pair[1], pair[0] + 1, "live not contiguous: {:?}", pair);
        }
        // 3. No overlap: the last snapshot index must be exactly one less
        //    than the first live index (no duplicate, no gap).
        let first_live = *live_indices
            .first()
            .expect("live_indices must be non-empty");
        let last_snap = *snapshot_indices
            .last()
            .expect("snapshot_indices must be non-empty");
        assert_eq!(
            first_live,
            last_snap + 1,
            "catch-up/live boundary wrong: last snapshot={last_snap}, first live={first_live}"
        );
        // 4. Every chunk written is accounted for exactly once.
        let total_seen = snapshot_indices.len() + live_indices.len();
        assert_eq!(
            total_seen as u64, CHUNK_COUNT,
            "expected all {CHUNK_COUNT} chunks, got {total_seen}"
        );
        // 5. total_written reported by the snapshot matches the number of
        //    bytes the writer had committed at snapshot time.
        assert!(snapshot_total <= CHUNK_COUNT * CHUNK_SIZE as u64);
        assert!(snapshot_total >= snapshot_indices.len() as u64 * CHUNK_SIZE as u64);
    }

    // --- Single-client full frames (story 601-82ef, 670-b9a2) ---
    //
    // Damage is tracked once per session, not per subscriber, and
    // `serialize_dirty_rows` CONSUMES it. So a full frame built for one WS client
    // silently takes the rows every other client was about to receive: the desktop
    // ticker's next serialize returns nothing and the desktop never learns those
    // rows changed. That is invisible row-map corruption on the other transport.
    //
    // This used to be paid for by damaging the whole grid again and re-arming the
    // ticker, which kept every transport whole at the price of pinning the
    // session into full frames — one slow browser made the desktop decode 108 KB
    // frames it never asked for. `serialize_full_frame` reads the rows without
    // consuming damage at all, so the resync is now invisible rather than merely
    // survivable, and these tests hold that stronger line.

    /// Feed enough output to dirty the grid, then drain the frame the ticker would
    /// have sent, leaving the buffer in the state a live session is in.
    fn dirty_session(state: &Arc<AppState>, session_id: &str, text: &str) {
        state.grid.vt_log_buffers.insert(
            session_id.to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        state.grid.frame_dirty.insert(
            session_id.to_string(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        let vt = state
            .grid
            .vt_log_buffers
            .get(session_id)
            .expect("just inserted");
        let mut vt = vt.lock();
        vt.process(text.as_bytes());
    }

    /// The frame the desktop ticker would take on its next tick.
    fn ticker_frame(state: &Arc<AppState>, session_id: &str) -> Vec<u8> {
        let vt = state
            .grid
            .vt_log_buffers
            .get(session_id)
            .expect("session exists");
        let mut vt = vt.lock();
        vt.serialize_dirty_rows().bytes
    }

    /// Feed more output into a session that is already painted.
    fn feed(state: &Arc<AppState>, session_id: &str, text: &str) {
        let vt = state
            .grid
            .vt_log_buffers
            .get(session_id)
            .expect("session exists");
        vt.lock().process(text.as_bytes());
    }

    /// Rows a frame carries, off the `row_count` header field.
    fn frame_rows(frame: &[u8]) -> u16 {
        u16::from_le_bytes([frame[0], frame[1]])
    }

    #[test]
    fn a_full_frame_for_one_client_does_not_consume_the_others_rows() {
        let state = super::super::tests::test_state();
        dirty_session(&state, "shared-damage", "hello from the pty\r\n");

        let frame = full_frame_for_single_client(&state, "shared-damage")
            .expect("a dirty session must produce a frame");
        assert_ne!(frame, [] as [u8; 0]);

        assert!(
            !ticker_frame(&state, "shared-damage").is_empty(),
            "the WS resync ate the rows the desktop channel was about to be sent"
        );
    }

    /// The strong form of the test above: the resync must be *invisible* to the
    /// shared stream, not merely survivable. A control session is fed the same
    /// bytes with nobody resyncing, and the two ticker frames have to match to
    /// the byte — re-damaging the grid would hand the desktop all 24 rows where
    /// the control gets the one that changed.
    #[test]
    fn a_full_frame_for_one_client_leaves_the_shared_delta_byte_identical() {
        let state = super::super::tests::test_state();
        dirty_session(&state, "resynced", "hello from the pty\r\n");
        dirty_session(&state, "control", "hello from the pty\r\n");
        // A fresh grid has no previous viewport to diff against, so its first
        // frame is full by construction and would say nothing about damage.
        // Drain it, then change exactly one row: what the desktop is owed now is
        // a one-row delta, and a re-damaged grid turns that into a whole screen.
        assert_eq!(frame_rows(&ticker_frame(&state, "resynced")), 24);
        assert_eq!(frame_rows(&ticker_frame(&state, "control")), 24);
        feed(&state, "resynced", "and one more line\r\n");
        feed(&state, "control", "and one more line\r\n");

        full_frame_for_single_client(&state, "resynced").expect("frame");

        let resynced = ticker_frame(&state, "resynced");
        let control = ticker_frame(&state, "control");
        assert!(
            frame_rows(&control) < 24,
            "the control must be a delta, or this test compares two full frames"
        );
        assert_eq!(
            resynced, control,
            "one client's resync changed what every other client is sent"
        );
    }

    /// The resync used to damage the whole grid on its way out, so it also had to
    /// wake the ticker or that damage would sit unsent. It damages nothing now,
    /// and waking the ticker would cost every transport a full frame to deliver a
    /// screen that has not changed.
    #[test]
    fn a_full_frame_for_one_client_does_not_wake_the_ticker() {
        let state = super::super::tests::test_state();
        dirty_session(&state, "wake-ticker", "hello\r\n");
        // Drain what the ticker owes, so the flag below can only be set by the
        // resync itself.
        let _ = ticker_frame(&state, "wake-ticker");
        state
            .grid
            .frame_dirty
            .get("wake-ticker")
            .expect("flag exists")
            .store(false, std::sync::atomic::Ordering::Relaxed);

        full_frame_for_single_client(&state, "wake-ticker").expect("frame");

        assert!(
            !state
                .grid
                .frame_dirty
                .get("wake-ticker")
                .expect("flag exists")
                .load(std::sync::atomic::Ordering::Relaxed),
            "a resync that damages nothing must not spend a tick on every transport"
        );
    }

    #[test]
    fn a_full_frame_for_a_session_that_is_gone_is_none() {
        let state = super::super::tests::test_state();
        assert!(full_frame_for_single_client(&state, "no-such-session").is_none());
    }

    // --- Browser-mode scroll (story 658-3ce1) ---
    //
    // `pending_scroll` used to be inserted by `subscribe_terminal_grid`, a
    // desktop-only Tauri command. A session driven from a browser over the grid
    // WebSocket therefore had no entry at all: this handler answered
    // `{"ok":true}` and recorded the target nowhere, so the wheel and the
    // scrollbar drag did nothing — and closing the desktop terminal took the
    // entry away from an already attached browser. The map belongs to the
    // session, next to the `grid_frame_dirty` the same handler sets.

    /// A reader that keeps the session alive until the test releases it, then
    /// reports EOF so the reader and ticker threads shut down normally.
    struct StopOnFlag(Arc<AtomicBool>);

    impl std::io::Read for StopOnFlag {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            while !self.0.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(0)
        }
    }

    /// A session as every creation path leaves it: a vt buffer holding
    /// scrollback plus the reader and ticker threads the session owns. Returns
    /// the stop flag — set it to tear the session down.
    fn browser_session(state: &Arc<AppState>, session_id: &str, lines: usize) -> Arc<AtomicBool> {
        let mut vt = VtLogBuffer::new(24, 80, 1000);
        for i in 0..lines {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        state
            .grid
            .vt_log_buffers
            .insert(session_id.to_string(), Mutex::new(vt));
        let stop = Arc::new(AtomicBool::new(false));
        spawn_reader_thread(
            Box::new(StopOnFlag(stop.clone())),
            Arc::new(AtomicBool::new(false)),
            session_id.to_string(),
            state.clone(),
            None,
        );
        stop
    }

    /// Wait for the frame ticker to apply the requested offset. The ticker owns
    /// the vt lock on a 16 ms interval, so the wait is real work, not setup; the
    /// bound is generous because what it has to catch is "never applied".
    async fn await_display_offset(state: &Arc<AppState>, session_id: &str, expected: usize) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let offset = {
                let vt = state
                    .grid
                    .vt_log_buffers
                    .get(session_id)
                    .expect("the session outlives the scroll");
                vt.lock().grid_display_offset()
            };
            if offset == expected {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the frame ticker never applied the pending scroll: display offset \
                 is {offset}, the client asked for {expected} — the scroll answered \
                 ok and did nothing"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    /// The bug exactly as a browser hits it: a grid WebSocket client attached, no
    /// desktop channel anywhere, and the wheel asking for an absolute offset.
    #[tokio::test]
    async fn a_browser_scroll_moves_the_grid_with_no_desktop_subscriber() {
        let state = super::super::tests::test_state();
        let sid = "browser-wheel".to_string();
        let stop = browser_session(&state, &sid, 200);
        // What `handle_ws_grid_session` holds for as long as a browser is attached.
        let watch = crate::grid_watch::new_grid_watch();
        let _browser = watch.subscribe();
        state.grid.watch.insert(sid.clone(), watch);

        terminal_scroll_to_offset(
            State(state.clone()),
            Path(sid.clone()),
            Json(TerminalScrollToOffsetRequest { offset: 40 }),
        )
        .await;

        await_display_offset(&state, &sid, 40).await;
        stop.store(true, Ordering::Relaxed);
    }

    /// The same scroll from a plain HTTP client with nothing attached at all —
    /// what a `curl` check does. `/terminal/scroll-info` and the row reads answer
    /// from the display offset, so a scroll the ticker drops for want of a
    /// subscriber would silently disagree with every later read of the session.
    #[tokio::test]
    async fn an_http_scroll_moves_the_grid_with_nothing_attached() {
        let state = super::super::tests::test_state();
        let sid = "http-only-scroll".to_string();
        let stop = browser_session(&state, &sid, 200);

        terminal_scroll_to_offset(
            State(state.clone()),
            Path(sid.clone()),
            Json(TerminalScrollToOffsetRequest { offset: 25 }),
        )
        .await;

        await_display_offset(&state, &sid, 25).await;
        stop.store(true, Ordering::Relaxed);
    }

    /// Catches: the HTTP anchor route enqueues a tail offset instead of a history row.
    #[tokio::test]
    async fn http_scroll_anchor_moves_to_the_same_history_row_without_desktop() {
        let state = super::super::tests::test_state();
        let sid = "http-anchor-scroll".to_string();
        let stop = browser_session(&state, &sid, 200);
        let history = state
            .grid
            .vt_log_buffers
            .get(&sid)
            .unwrap()
            .lock()
            .grid_history_size();
        terminal_scroll_to_anchor(
            State(state.clone()),
            Path(sid.clone()),
            Json(super::super::types::TerminalScrollToAnchorRequest {
                anchor: Some(history - 25),
            }),
        )
        .await;
        await_display_offset(&state, &sid, 25).await;
        terminal_scroll_to_anchor(
            State(state.clone()),
            Path(sid.clone()),
            Json(super::super::types::TerminalScrollToAnchorRequest { anchor: None }),
        )
        .await;
        await_display_offset(&state, &sid, 0).await;
        stop.store(true, Ordering::Relaxed);
    }

    // --- Styled rows over HTTP (story 601-82ef) ---
    //
    // The desktop command hands these bytes over raw (`tauri::ipc::Response`), so
    // the browser transport must get them raw too, or the same `fetchChunk` code
    // has to branch per transport. `rpcImpl` picks `resp.arrayBuffer()` off the
    // content-type alone — the header IS the contract.

    #[tokio::test]
    async fn styled_rows_travel_as_binary_not_as_a_json_number_array() {
        use axum::body::to_bytes;

        let bytes = vec![26u8, 0, 255, 7];
        let response = styled_rows_response(bytes.clone()).into_response();

        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/octet-stream"),
            "rpcImpl branches on this header to call arrayBuffer()"
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body must be readable");
        assert_eq!(body.as_ref(), bytes.as_slice(), "bytes must survive intact");
    }

    /// A closed session or an out-of-range request serializes to nothing. That is
    /// a valid empty chunk, not an error — and it must still be typed binary so
    /// the client decodes it the same way as any other chunk.
    #[tokio::test]
    async fn an_empty_styled_row_range_is_still_a_binary_body() {
        use axum::body::to_bytes;

        let response = styled_rows_response(Vec::new()).into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/octet-stream")
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body must be readable");
        assert!(body.is_empty());
    }

    // --- Grid WS frame shapes (story 623-d369) ---
    //
    // The client destructures every frame as `const { type, ...payload } = event`
    // and hands `payload` to the same handler the desktop `listen()` feeds. So a
    // frame is correct only if it equals its Tauri payload plus a `type` key.
    //
    // These drive the real `grid_ws_frame` and compare against the serialized
    // Rust struct the desktop side emits — NOT against a hand-built object. A
    // test that rebuilds the expected shape by hand agrees only with itself and
    // stays green while the wire carries something the client cannot read.

    /// Strip the discriminator: what is left must be the Tauri event payload.
    fn frame_payload(frame: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        let mut map = frame.as_object().expect("frame must be an object").clone();
        assert!(map.remove("type").is_some(), "frame must carry a type");
        map
    }

    /// Catches browser bells disappearing after desktop stops decoding hidden frames.
    #[test]
    fn grid_ws_bell_is_independent_of_grid_frames() {
        let frame = grid_ws_frame(&crate::state::AppEvent::PtyBell {
            session_id: "s1".into(),
        })
        .unwrap();
        assert_eq!(frame, serde_json::json!({"type":"bell"}));
    }

    #[test]
    fn grid_ws_osc133_frame_matches_the_desktop_event_payload() {
        for (marker, exit_code) in [("A", None), ("D", Some(0)), ("D", Some(130))] {
            let frame = grid_ws_frame(&crate::state::AppEvent::PtyOsc133 {
                session_id: "s1".to_string(),
                marker: marker.to_string(),
                line: 42,
                exit_code,
            })
            .expect("osc133 must be carried by the grid WS");

            assert_eq!(frame["type"], "osc133");

            // The exact payload the desktop AppHandle emits for the same marker.
            let desktop = serde_json::to_value(crate::terminal_grid::Osc133Event {
                marker: marker.to_string(),
                line: 42,
                exit_code,
            })
            .expect("Osc133Event must serialize");

            assert_eq!(
                serde_json::Value::Object(frame_payload(&frame)),
                desktop,
                "grid WS payload drifted from the desktop Osc133Event ({marker})"
            );
        }
    }

    /// `exit_code: None` must survive as an explicit `null`, not vanish. The
    /// client reads `exit_code ?? undefined`, so a missing key and a null key
    /// happen to behave alike today — but a dropped key is one `skip_serializing_if`
    /// away from meaning "field removed" to any other consumer.
    #[test]
    fn grid_ws_osc133_frame_keeps_a_null_exit_code() {
        let frame = grid_ws_frame(&crate::state::AppEvent::PtyOsc133 {
            session_id: "s1".to_string(),
            marker: "A".to_string(),
            line: 0,
            exit_code: None,
        })
        .expect("osc133 must be carried by the grid WS");

        assert!(frame.get("exit_code").is_some(), "exit_code key must exist");
        assert!(frame["exit_code"].is_null());
    }

    /// The cwd payload is `{ cwd }` on BOTH transports. It cannot be a bare
    /// string on the wire — a frame needs its `type` discriminator — so the
    /// desktop emit was changed to match rather than the client made to branch.
    #[test]
    fn grid_ws_cwd_frame_carries_the_same_object_as_the_desktop_event() {
        let frame = grid_ws_frame(&crate::state::AppEvent::PtyCwd {
            session_id: "s1".to_string(),
            cwd: "/tmp/project".to_string(),
        })
        .expect("cwd must be carried by the grid WS");

        assert_eq!(frame["type"], "cwd");
        assert_eq!(
            serde_json::Value::Object(frame_payload(&frame)),
            serde_json::json!({ "cwd": "/tmp/project" })
        );
    }

    /// The regression this story fixes: both events used to reach the desktop
    /// AppHandle alone, so a browser/PWA client got no command blocks, no gutter
    /// marks, no Cmd+Up/Down navigation and no cwd tracking.
    #[test]
    fn grid_ws_carries_osc133_and_cwd_at_all() {
        assert!(
            grid_ws_frame(&crate::state::AppEvent::PtyOsc133 {
                session_id: "s1".to_string(),
                marker: "A".to_string(),
                line: 1,
                exit_code: None,
            })
            .is_some(),
            "OSC 133 must reach browser clients"
        );
        assert!(
            grid_ws_frame(&crate::state::AppEvent::PtyCwd {
                session_id: "s1".to_string(),
                cwd: "/tmp".to_string(),
            })
            .is_some(),
            "OSC 7 cwd must reach browser clients"
        );
    }

    /// Not every bus event belongs on this socket. The activity pulse
    /// (story 625-56b0) rides the subscribePty stream and has no consumer here,
    /// and waking every grid client for it would be pure cost.
    #[test]
    fn grid_ws_drops_events_it_has_no_consumer_for() {
        assert!(
            grid_ws_frame(&crate::state::AppEvent::PtyActivity {
                session_id: "s1".to_string(),
            })
            .is_none(),
            "the activity pulse must not be forwarded on the grid WS"
        );
    }

    /// Catches: a title bus event never reaches raw/log/text WS consumers.
    #[cfg(unix)]
    #[tokio::test]
    async fn remote_title_crosses_live_ws_without_starving_log_output() {
        for format in ["raw", "log", "text"] {
            let state = super::super::tests::test_state();
            let sid = "remote-title-wire";
            crate::state::tests_support::insert_dummy_session(&state, sid);
            state
                .session_maps
                .output_buffers
                .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
            state
                .grid
                .vt_log_buffers
                .insert(sid.into(), Mutex::new(VtLogBuffer::new(4, 32, 1000)));
            let app = super::super::build_router(state.clone(), false, true);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let _ = axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await;
            });
            let (mut socket, _) = tokio_tungstenite::connect_async(format!(
                "ws://{addr}/sessions/{sid}/stream?format={format}"
            ))
            .await
            .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                while state
                    .session_maps
                    .pty_event_channels
                    .get(sid)
                    .is_none_or(|tx| tx.receiver_count() == 0)
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("WS subscriber setup");
            for title in ["Claude Code", ""] {
                state.emit_pty_event(crate::state::AppEvent::PtyTitle {
                    session_id: sid.into(),
                    title: title.into(),
                });
                let received = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        let message = socket.next().await.unwrap().unwrap();
                        if let Ok(frame) =
                            serde_json::from_str::<serde_json::Value>(&message.into_text().unwrap())
                            && frame["type"] == "title"
                        {
                            break frame;
                        }
                    }
                })
                .await
                .expect("title event must be streamed without another PTY output chunk");
                assert_eq!(
                    received,
                    serde_json::json!({"type": "title", "title": title})
                );
            }

            if format != "raw" {
                state
                    .grid
                    .vt_log_buffers
                    .get(sid)
                    .unwrap()
                    .lock()
                    .process(b"stream while titles change\r\n");
                let animated_state = state.clone();
                let animation = tokio::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_millis(20));
                    loop {
                        tick.tick().await;
                        animated_state.emit_pty_event(crate::state::AppEvent::PtyTitle {
                            session_id: sid.into(),
                            title: "animated title".into(),
                        });
                    }
                });
                let output = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        let message = socket.next().await.unwrap().unwrap();
                        let frame: serde_json::Value =
                            serde_json::from_str(&message.into_text().unwrap()).unwrap();
                        if frame["type"] == "log"
                            && frame.to_string().contains("stream while titles change")
                        {
                            break;
                        }
                    }
                })
                .await;
                animation.abort();
                output.expect("title traffic must not keep restarting the log batching deadline");
            }
            socket.close(None).await.unwrap();
            server.abort();
        }
    }

    // --- Grid watch channel (format=grid WS endpoint) ---

    /// Verifies that the grid_watch channel delivers frames with latest-frame-wins
    /// semantics: a slow receiver that misses intermediate sends still gets the
    /// most recent frame on its next `changed().await`.
    #[tokio::test]
    async fn grid_watch_latest_frame_wins() {
        let (tx, mut rx) = tokio::sync::watch::channel(Vec::<u8>::new());

        // Send 3 frames without the receiver polling
        tx.send(vec![1, 2, 3]).unwrap();
        tx.send(vec![4, 5, 6]).unwrap();
        tx.send(vec![7, 8, 9]).unwrap();

        // Receiver sees only the latest
        rx.changed().await.unwrap();
        let frame = rx.borrow_and_update().clone();
        assert_eq!(frame, vec![7, 8, 9]);

        // No pending change after consuming latest
        let result = tokio::time::timeout(std::time::Duration::from_millis(10), rx.changed()).await;
        assert!(result.is_err(), "should timeout — no new frame");
    }

    /// Verifies that a newly subscribed receiver gets the current value
    /// immediately (supports initial full-frame delivery in handle_ws_grid_session).
    #[tokio::test]
    async fn grid_watch_subscriber_gets_current_value() {
        let (tx, _rx) = tokio::sync::watch::channel(Vec::<u8>::new());

        // Publish a frame
        tx.send(vec![10, 20, 30]).unwrap();

        // New subscriber sees current value via borrow()
        let rx2 = tx.subscribe();
        let current = rx2.borrow().clone();
        assert_eq!(current, vec![10, 20, 30]);
    }

    /// Catches: registration floors the VT at 220 columns, and a no-op resize
    /// preserves that incorrect width instead of the real headless PTY geometry.
    #[tokio::test]
    async fn headless_registration_and_same_size_resize_preserve_requested_width() {
        let state = super::super::tests::test_state();
        let (shell, _) = crate::test_support::host_shell();
        let session_id = spawn_pty_session(
            state.clone(),
            shell.into(),
            None,
            24,
            148,
            None,
            RequestedIdentity::default(),
        )
        .expect("create isolated geometry PTY");
        assert_eq!(
            state
                .grid
                .vt_log_buffers
                .get(&session_id)
                .unwrap()
                .lock()
                .grid_columns(),
            148,
            "registration must preserve the requested width"
        );
        crate::pty::resize_session_off_thread(&state, session_id.clone(), 24, 148, "test")
            .await
            .expect("same-size resize");
        assert_eq!(
            state
                .grid
                .vt_log_buffers
                .get(&session_id)
                .unwrap()
                .lock()
                .grid_columns(),
            148,
            "same-size resize must not preserve a stale 220-column floor"
        );
        crate::pty::close_pty_core(&state, &session_id, false);
    }

    /// Verifies that spawn_pty_session registers a grid_watch channel for the session,
    /// so that handle_ws_grid_session can subscribe to it (regression for BUG-2).
    #[tokio::test]
    async fn spawn_pty_session_registers_grid_watch() {
        let state = super::super::tests::test_state();

        assert!(state.grid.watch.is_empty());

        let result = super::spawn_pty_session(
            state.clone(),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            None,
            24,
            80,
            None,
            super::RequestedIdentity::default(),
        );

        let session_id = match result {
            Ok(id) => id,
            Err(_) => return, // PTY unavailable in CI — skip gracefully
        };

        // Catches: HTTP shell creation leaves the role unknown or labels its
        // root as a direct agent, preventing shell-return revocation.
        assert_eq!(
            state
                .session_maps
                .session_states
                .get(&session_id)
                .unwrap()
                .spawn_root_role,
            crate::state::SpawnRootRole::Shell
        );
        assert!(
            state
                .session_maps
                .sessions
                .get(&session_id)
                .unwrap()
                .lock()
                ._child
                .process_id()
                .is_some()
        );

        assert!(
            state.grid.watch.contains_key(&session_id),
            "spawn_pty_session must register a grid_watch channel"
        );

        // Verify the channel is functional, and that a published frame carries
        // the sequence number the WS reader needs to spot a dropped delta.
        let tx = state.grid.watch.get(&session_id).unwrap();
        let mut rx = tx.subscribe();
        let first_seq = rx.borrow_and_update().seq;
        crate::grid_watch::publish_grid_frame(&tx, vec![1, 2, 3]);
        rx.changed().await.unwrap();
        let slot = rx.borrow_and_update();
        assert_eq!(slot.frame, vec![1, 2, 3]);
        assert_eq!(slot.seq, first_seq + 1);
    }

    /// A client-provided session id is honored (browser duplicate-tab fix): the
    /// browser pre-registers this id locally so the session-created echo is
    /// recognized as locally-created.
    #[tokio::test]
    async fn spawn_pty_session_honors_requested_id() {
        let state = super::super::tests::test_state();
        let result = super::spawn_pty_session(
            state.clone(),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            None,
            24,
            80,
            None,
            super::RequestedIdentity {
                session_id: Some("client-provided-id".to_string()),
                alias: None,
                ..Default::default()
            },
        );
        // PTY unavailable in CI — skip gracefully
        if let Ok(id) = result {
            assert_eq!(
                id, "client-provided-id",
                "must honor the client-provided id"
            );
        }
    }

    /// A requested id that collides with an existing session is rejected in
    /// favor of a fresh uuid, so a buggy/duplicate client id can never hijack
    /// or alias another live session.
    #[tokio::test]
    async fn spawn_pty_session_rejects_duplicate_requested_id() {
        let state = super::super::tests::test_state();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let first = match super::spawn_pty_session(
            state.clone(),
            shell.clone(),
            None,
            24,
            80,
            None,
            super::RequestedIdentity {
                session_id: Some("dup-id".to_string()),
                alias: None,
                ..Default::default()
            },
        ) {
            Ok(id) => id,
            Err(_) => return, // PTY unavailable in CI — skip gracefully
        };
        assert_eq!(first, "dup-id");
        let second = super::spawn_pty_session(
            state.clone(),
            shell,
            None,
            24,
            80,
            None,
            super::RequestedIdentity {
                session_id: Some("dup-id".to_string()),
                alias: None,
                ..Default::default()
            },
        )
        .expect("second spawn should succeed with a fresh id");
        assert_ne!(
            second, "dup-id",
            "duplicate requested id must fall back to a fresh uuid"
        );
    }

    /// The session list is the only record of an alias after a WebView reload:
    /// `term-alias-assigned` fires once, at spawn, and the reload re-adopts every
    /// tab from this list. A row without it leaves the tab unaddressable in the UI
    /// and makes the next restart snapshot request a fresh alias.
    #[tokio::test]
    async fn session_rows_carry_the_terminal_alias() {
        let state = super::super::tests::test_state();
        let session_id = match super::spawn_pty_session(
            state.clone(),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            None,
            24,
            80,
            None,
            super::RequestedIdentity {
                session_id: Some("aliased-row".to_string()),
                alias: Some("tu-7".to_string()),
                ..Default::default()
            },
        ) {
            Ok(id) => id,
            Err(_) => return, // PTY unavailable in CI — skip gracefully
        };
        let row = super::session_rows_including_remote(&state)
            .into_iter()
            .find(|row| row.session_id == session_id)
            .expect("the spawned session is listed");
        assert_eq!(row.alias.as_deref(), Some("tu-7"));
        // Catches: the session list publishes an agent leaf PID instead of the PTY child.
        let child_pid = state
            .session_maps
            .sessions
            .get(&session_id)
            .unwrap()
            .lock()
            ._child
            .process_id();
        assert_eq!(row.child_pid, child_pid);
        assert!(row.child_pid.is_some());
        state.session_maps.exit_codes.insert(session_id.clone(), 0);
        let ended = super::local_session_rows(&state)
            .into_iter()
            .find(|row| row.session_id == session_id)
            .unwrap();
        assert_eq!(
            ended.child_pid, None,
            "an exited child must not remain copyable"
        );
    }

    #[tokio::test]
    async fn session_rows_expose_the_live_tuic_identity_that_children_name() {
        let state = super::super::tests::test_state();
        let session_id = match super::spawn_pty_session(
            state.clone(),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            None,
            24,
            80,
            None,
            super::RequestedIdentity {
                session_id: Some("coordinator-pty".to_string()),
                ..Default::default()
            },
        ) {
            Ok(id) => id,
            Err(_) => return, // PTY unavailable in CI — skip gracefully
        };
        state.bind_live_pty("coordinator-tuic", &session_id);
        let row = super::session_rows_including_remote(&state)
            .into_iter()
            .find(|row| row.session_id == session_id)
            .expect("the parent session is listed");
        let wire = serde_json::to_value(row).expect("session row serializes over both transports");
        assert_eq!(wire["tuic_session"], "coordinator-tuic");
    }

    /// Every OSC 0/2 and intent title is synced back through `PUT name` as a
    /// non-custom name. When the reload inferred "spawn-named" from that shape,
    /// the agent's next OSC titles were all rejected and the tab title froze. The
    /// origin is recorded where the name is born and survives only a non-custom
    /// sync, which is exactly how the live tab treats it.
    #[tokio::test]
    async fn only_an_agent_spawn_marks_a_row_as_spawn_named() {
        use axum::extract::{Path as AxPath, State as AxState};
        let state = super::super::tests::test_state();
        let session_id = match super::spawn_pty_session(
            state.clone(),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            None,
            24,
            80,
            None,
            super::RequestedIdentity::default(),
        ) {
            Ok(id) => id,
            Err(_) => return, // PTY unavailable in CI — skip gracefully
        };
        let rename = |name: &str, is_custom: bool| {
            super::set_session_name(
                AxState(state.clone()),
                AxPath(session_id.clone()),
                axum::Json(super::SetNameRequest {
                    name: Some(name.to_string()),
                    is_custom: Some(is_custom),
                }),
            )
        };
        let row = || {
            super::local_session_rows(&state)
                .into_iter()
                .find(|row| row.session_id == session_id)
                .expect("the spawned session is listed")
        };

        rename("main-wise-beacon", false).await;
        let synced = row();
        assert_eq!(synced.display_name.as_deref(), Some("main-wise-beacon"));
        assert!(
            !synced.display_name_from_spawn,
            "an OSC title is not a spawn name"
        );

        // What `agent spawn` with a `name` records.
        state
            .session_maps
            .sessions
            .get(&session_id)
            .unwrap()
            .lock()
            .display_name_from_spawn = true;
        rename("Call mapping", false).await;
        assert!(
            row().display_name_from_spawn,
            "an intent title refines a spawn name; the live tab still refuses OSC titles"
        );
        rename("mine", true).await;
        let renamed = row();
        assert!(renamed.display_name_is_custom);
        assert!(
            !renamed.display_name_from_spawn,
            "a user rename replaces the spawn name"
        );
    }

    /// The parent is published once, on `session-created`; a reload or a
    /// late-joining browser has only the row. A `pending-mcp:` placeholder is
    /// never a session a tab can match, so it is withheld until `register`
    /// resolves it.
    #[tokio::test]
    async fn session_rows_carry_only_a_resolved_parent() {
        let state = super::super::tests::test_state();
        let session_id = match super::spawn_pty_session(
            state.clone(),
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            None,
            24,
            80,
            None,
            super::RequestedIdentity::default(),
        ) {
            Ok(id) => id,
            Err(_) => return, // PTY unavailable in CI — skip gracefully
        };
        let parent = || {
            super::local_session_rows(&state)
                .into_iter()
                .find(|row| row.session_id == session_id)
                .expect("the spawned session is listed")
                .parent_session
        };
        assert_eq!(parent(), None);
        state
            .session_maps
            .session_parent
            .insert(session_id.clone(), "pending-mcp:mcp-late".to_string());
        assert_eq!(parent(), None);
        state
            .session_maps
            .session_parent
            .insert(session_id.clone(), "lead-session".to_string());
        assert_eq!(parent().as_deref(), Some("lead-session"));
    }
}
