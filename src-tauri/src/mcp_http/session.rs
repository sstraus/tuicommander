use crate::pty::{build_shell_command, resolve_shell, spawn_headless_reader_thread, spawn_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::{OUTPUT_RING_BUFFER_CAPACITY, VtLogBuffer, VT_LOG_BUFFER_CAPACITY};
use tauri::Emitter;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::StreamExt;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, PtySize};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use super::types::*;

pub(super) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

pub(super) async fn list_sessions(State(state): State<Arc<AppState>>) -> Json<Vec<SessionInfo>> {
    let sessions: Vec<SessionInfo> = state
        .sessions
        .iter()
        .map(|entry| {
            let session_id = entry.key().clone();
            let session = entry.value().lock();
            let session_state = state.session_states.get(&session_id).map(|s| s.clone());
            SessionInfo {
                session_id,
                cwd: session.cwd.clone(),
                worktree_path: session
                    .worktree
                    .as_ref()
                    .map(|w| w.path.to_string_lossy().to_string()),
                worktree_branch: session
                    .worktree
                    .as_ref()
                    .and_then(|w| w.branch.clone()),
                state: session_state,
            }
        })
        .collect();
    Json(sessions)
}

pub(super) async fn write_to_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<WriteRequest>,
) -> impl IntoResponse {
    let entry = match state.sessions.get(&session_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Session not found"})),
            )
        }
    };
    let mut session = entry.lock();
    if let Err(e) = session.writer.write_all(body.data.as_bytes()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Write failed: {}", e)})),
        );
    }
    if let Err(e) = session.writer.flush() {
        eprintln!("Warning: PTY flush failed for session {session_id}: {e}");
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn resize_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<ResizeRequest>,
) -> impl IntoResponse {
    if let Err(msg) = super::validate_terminal_size(body.rows, body.cols) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg})));
    }
    let entry = match state.sessions.get(&session_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Session not found"})),
            )
        }
    };
    let session = entry.lock();
    if let Err(e) = session.master.resize(PtySize {
        rows: body.rows,
        cols: body.cols,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Resize failed: {}", e)})),
        );
    }
    drop(session);
    // Resize VT log buffer to match new terminal dimensions.
    if let Some(vt_log) = state.vt_log_buffers.get(&session_id) {
        vt_log.lock().resize(body.rows, body.cols);
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn get_output(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<OutputQuery>,
) -> impl IntoResponse {
    let format = query.format.as_deref().unwrap_or("raw");

    // format=log: return VT100-extracted clean log lines (best for mobile/REST consumers)
    if format == "log" {
        let vt_log = match state.vt_log_buffers.get(&session_id) {
            Some(b) => b,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "Session not found"})),
                )
            }
        };
        let buf = vt_log.lock();
        let limit = query.limit.unwrap_or(usize::MAX);
        let total = buf.total_lines();
        let offset = total.saturating_sub(limit);
        let (lines, _) = buf.lines_since_owned(offset);
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "lines": lines,
                "total_lines": total,
            })),
        );
    }

    // format=text: serve clean rows from VtLogBuffer (no strip_ansi needed)
    if format == "text" {
        let vt_log = match state.vt_log_buffers.get(&session_id) {
            Some(b) => b,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "Session not found"})),
                )
            }
        };
        let buf = vt_log.lock();
        let limit = query.limit.unwrap_or(usize::MAX);
        let total = buf.total_lines();
        let offset = total.saturating_sub(limit);
        let (log_lines, _) = buf.lines_since_owned(offset);
        // Append current visible screen rows (non-empty) after the log
        let screen: Vec<String> = buf.screen_rows()
            .into_iter()
            .filter(|r| !r.is_empty())
            .collect();
        let mut all_lines = log_lines;
        all_lines.extend(screen);
        let data = all_lines.join("\n");
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "data": data,
                "data_length": data.len(),
                "total_written": total,
            })),
        );
    }

    let ring = match state.output_buffers.get(&session_id) {
        Some(r) => r,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Session not found"})),
            )
        }
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
    if let Some((_, session_mutex)) = state.sessions.remove(&session_id) {
        state.output_buffers.remove(&session_id);
        state.vt_log_buffers.remove(&session_id);
        state.ws_clients.remove(&session_id);
        state.kitty_states.remove(&session_id);
        state.input_buffers.remove(&session_id);
        state.silence_states.remove(&session_id);
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);

        let mut session = session_mutex.into_inner();
        let _ = session.writer.write_all(&[0x03]);
        let _ = session.writer.flush();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            match session._child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() >= deadline => break,
                _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
        drop(session);

        // Broadcast to SSE/WebSocket consumers
        let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
            session_id: session_id.clone(),
        });
        // Tauri IPC for desktop backward compat
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit("session-closed", serde_json::json!({
                "session_id": session_id,
            }));
        }

        (StatusCode::OK, Json(serde_json::json!({"ok": true})))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session not found"})),
        )
    }
}

pub(super) async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    if state.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        );
    }

    let rows = body.rows.unwrap_or(24);
    let cols = body.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg})));
    }
    let shell = resolve_shell(body.shell);

    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    let pair = match pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to open PTY: {}", e)})),
            )
        }
    };

    let agent_teams = {
        let cfg = state.config.read();
        if cfg.agent_teams_shim {
            Some(crate::pty::AgentTeamsEnv {
                session_id: session_id.clone(),
                http_port: cfg.remote_access_port,
                socket_path: super::socket_path().to_string_lossy().to_string(),
            })
        } else {
            None
        }
    };
    let mut cmd = build_shell_command(&shell, agent_teams.as_ref());
    if let Some(ref cwd) = body.cwd {
        cmd.cwd(cwd);
    }

    let child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to spawn shell: {}", e)})),
            )
        }
    };

    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)})),
            )
        }
    };

    let reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)})),
            )
        }
    };

    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            cwd: body.cwd.clone(),
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)),
    );

    // Broadcast to SSE/WebSocket consumers (before state is moved to reader thread)
    let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
        session_id: session_id.clone(),
        cwd: body.cwd.clone(),
    });

    // Use full reader thread (with Tauri events) when AppHandle is available,
    // fall back to headless for tests or pre-setup scenarios
    let app_handle = state.app_handle.read().clone();
    if let Some(ref app) = app_handle {
        spawn_reader_thread(reader, paused, session_id.clone(), app.clone(), state);
    } else {
        spawn_headless_reader_thread(reader, paused, session_id.clone(), state);
    }

    // Tauri IPC for desktop backward compat
    if let Some(app) = app_handle {
        let _ = app.emit("session-created", serde_json::json!({
            "session_id": session_id,
            "cwd": body.cwd,
        }));
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"session_id": session_id})),
    )
}

pub(super) async fn pause_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let entry = match state.sessions.get(&session_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Session not found"})),
            )
        }
    };
    entry.lock().paused.store(true, Ordering::Relaxed);
    state.metrics.pauses_triggered.fetch_add(1, Ordering::Relaxed);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn resume_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let entry = match state.sessions.get(&session_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Session not found"})),
            )
        }
    };
    entry.lock().paused.store(false, Ordering::Relaxed);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn get_kitty_flags(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let flags = state
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
    let agent = (|| -> Option<String> {
        let entry = state.sessions.get(&session_id)?;
        let session = entry.value().lock();
        #[cfg(not(windows))]
        {
            let pgid = session.master.process_group_leader()?;
            let name = crate::pty::process_name_from_pid(pgid as u32)?;
            crate::pty::classify_agent(&name).map(|s| s.to_string())
        }
        #[cfg(windows)]
        {
            drop(session);
            None
        }
    })();

    match agent {
        Some(name) => (StatusCode::OK, Json(serde_json::json!({"agent": name}))),
        None => (StatusCode::OK, Json(serde_json::json!({"agent": null}))),
    }
}

pub(super) async fn get_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.orchestrator_stats())
}

pub(super) async fn get_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.session_metrics_json())
}

pub(super) async fn create_session_with_worktree(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionWithWorktreeRequest>,
) -> impl IntoResponse {
    if state.sessions.len() >= MAX_CONCURRENT_SESSIONS {
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
    let worktree = match crate::worktree::create_worktree_internal(&state.worktrees_dir, &wt_config, None) {
        Ok(wt) => wt,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    };

    let rows = body.config.rows.unwrap_or(24);
    let cols = body.config.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg})));
    }
    let shell = resolve_shell(body.config.shell);
    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    let pair = match pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }) {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to open PTY: {}", e)}))),
    };

    let agent_teams = {
        let cfg = state.config.read();
        if cfg.agent_teams_shim {
            Some(crate::pty::AgentTeamsEnv {
                session_id: session_id.clone(),
                http_port: cfg.remote_access_port,
                socket_path: super::socket_path().to_string_lossy().to_string(),
            })
        } else {
            None
        }
    };
    let mut cmd = build_shell_command(&shell, agent_teams.as_ref());
    cmd.cwd(worktree.path.to_string_lossy().as_ref());

    let child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to spawn shell: {}", e)}))),
    };
    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)}))),
    };
    let reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)}))),
    };

    let worktree_path_str = worktree.path.to_string_lossy().to_string();
    let worktree_branch = worktree.branch.clone();
    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: Some(worktree),
            cwd: Some(worktree_path_str.clone()),
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)),
    );

    // Broadcast to SSE/WebSocket consumers (before state is moved to reader thread)
    let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
        session_id: session_id.clone(),
        cwd: Some(worktree_path_str.clone()),
    });

    let app_handle = state.app_handle.read().clone();
    if let Some(ref app) = app_handle {
        spawn_reader_thread(reader, paused, session_id.clone(), app.clone(), state);
    } else {
        spawn_headless_reader_thread(reader, paused, session_id.clone(), state);
    }

    // Tauri IPC for desktop backward compat
    if let Some(app) = app_handle {
        let _ = app.emit("session-created", serde_json::json!({
            "session_id": session_id,
            "cwd": worktree_path_str,
        }));
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "session_id": session_id,
            "worktree_path": worktree_path_str,
            "branch": worktree_branch,
        })),
    )
}

/// WebSocket upgrade handler for streaming PTY output.
/// Bidirectional: server sends PTY output, client sends PTY input.
/// Supports `?format=text` to strip ANSI, `?format=log` for VT100 log lines.
pub(super) async fn ws_stream(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    Query(query): Query<OutputQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if !state.sessions.contains_key(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let format = query.format.as_deref().unwrap_or("raw");
    // format=text and format=log both serve clean VtLogBuffer rows (no strip_ansi).
    let log_mode = format == "log" || format == "text";
    ws.on_upgrade(move |socket| handle_ws_session(socket, id, state, log_mode))
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
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    if log_mode {
        // Log/text mode: stream clean VtLogBuffer rows, no raw PTY chunks
        handle_ws_log_session(ws_sender, ws_receiver, session_id, state).await;
        return;
    }

    // Register a channel for receiving PTY output broadcasts
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state
        .ws_clients
        .entry(session_id.clone())
        .or_default()
        .push(tx);

    // Subscribe to broadcast channel for parsed events (filtered by session_id)
    let mut event_rx = state.event_bus.subscribe();

    // Send existing ring buffer content as initial catch-up
    if let Some(ring) = state.output_buffers.get(&session_id) {
        let (data, _) = ring.lock().read_last(OUTPUT_RING_BUFFER_CAPACITY);
        if !data.is_empty() {
            let text = String::from_utf8_lossy(&data).into_owned();
            if !text.is_empty() {
                let frame = serde_json::json!({"type": "output", "data": text});
                let _ = futures_util::SinkExt::send(
                    &mut ws_sender,
                    Message::Text(frame.to_string().into()),
                ).await;
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
                    if futures_util::SinkExt::send(
                        &mut ws_sender,
                        Message::Text(frame.to_string().into()),
                    ).await.is_err() {
                        break;
                    }
                }
                // Parsed events from broadcast channel
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Filter: only forward events for this session
                            let matches = match &event {
                                crate::state::AppEvent::PtyParsed { session_id: sid, .. } => sid == &sid_for_events,
                                crate::state::AppEvent::PtyExit { session_id: sid } => sid == &sid_for_events,
                                crate::state::AppEvent::SessionClosed { session_id: sid } => sid == &sid_for_events,
                                _ => false,
                            };
                            if !matches { continue; }

                            // Extract the inner payload (without serde tag wrapping)
                            let payload = match &event {
                                crate::state::AppEvent::PtyParsed { parsed, .. } => {
                                    serde_json::json!({"type": "parsed", "event": parsed})
                                }
                                crate::state::AppEvent::PtyExit { session_id: sid } => {
                                    serde_json::json!({"type": "exit", "session_id": sid})
                                }
                                crate::state::AppEvent::SessionClosed { session_id: sid } => {
                                    serde_json::json!({"type": "closed", "session_id": sid})
                                }
                                _ => continue,
                            };
                            if futures_util::SinkExt::send(
                                &mut ws_sender,
                                Message::Text(payload.to_string().into()),
                            ).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            eprintln!("[ws] broadcast lagged by {n} events for session {sid_for_events}");
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
                if let Some(session) = state_clone.sessions.get(&sid) {
                    let mut s = session.lock();
                    if let Err(e) = s.writer.write_all(text.as_bytes()) {
                        eprintln!("[ws] PTY write failed for session {sid}: {e}");
                        break;
                    }
                    let _ = s.writer.flush();
                }
            }
            Message::Binary(data) => {
                if let Some(session) = state_clone.sessions.get(&sid) {
                    let mut s = session.lock();
                    if let Err(e) = s.writer.write_all(&data) {
                        eprintln!("[ws] PTY write failed for session {sid}: {e}");
                        break;
                    }
                    let _ = s.writer.flush();
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Client disconnected — abort the send task
    send_task.abort();
}

/// Handle a WebSocket connection in log mode (`?format=log`).
///
/// Sends VT100-extracted log lines: catch-up on connect, then polls for new
/// lines every 200 ms and batches them as `{"type":"log","lines":[...],"offset":N}`.
/// The client can still send PTY input (written as-is to the PTY).
async fn handle_ws_log_session(
    mut ws_sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut ws_receiver: futures_util::stream::SplitStream<WebSocket>,
    session_id: String,
    state: Arc<AppState>,
) {
    // Send catch-up: all lines accumulated so far.
    // Lock is dropped before the .await to satisfy Send bound.
    let initial_offset = {
        if let Some(vt_log) = state.vt_log_buffers.get(&session_id) {
            let (total, catchup_frame) = {
                let buf = vt_log.lock();
                let total = buf.total_lines();
                let frame = if total > 0 {
                    let (lines, _) = buf.lines_since_owned(0);
                    Some(serde_json::json!({"type": "log", "lines": lines, "offset": 0}).to_string())
                } else {
                    None
                };
                (total, frame)
            }; // lock released here
            if let Some(frame_str) = catchup_frame {
                let _ = futures_util::SinkExt::send(
                    &mut ws_sender,
                    Message::Text(frame_str.into()),
                ).await;
            }
            total
        } else {
            0
        }
    };

    // Spawn polling task: check for new lines every 200ms
    let sid_poll = session_id.clone();
    let state_poll = state.clone();
    let send_task = tokio::spawn(async move {
        let mut offset = initial_offset;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            let Some(vt_log) = state_poll.vt_log_buffers.get(&sid_poll) else {
                // Session was removed
                break;
            };
            let (lines, new_offset) = vt_log.lock().lines_since_owned(offset);
            if !lines.is_empty() {
                let frame = serde_json::json!({"type": "log", "lines": lines, "offset": offset});
                if futures_util::SinkExt::send(
                    &mut ws_sender,
                    Message::Text(frame.to_string().into()),
                ).await.is_err() {
                    break;
                }
                offset = new_offset;
            }
        }
    });

    // Read messages from the client and write to PTY (input passthrough)
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                if let Some(session) = state.sessions.get(&session_id) {
                    let mut s = session.lock();
                    if let Err(e) = s.writer.write_all(text.as_bytes()) {
                        eprintln!("[ws/log] PTY write failed for session {session_id}: {e}");
                        break;
                    }
                    let _ = s.writer.flush();
                }
            }
            Message::Binary(data) => {
                if let Some(session) = state.sessions.get(&session_id) {
                    let mut s = session.lock();
                    if let Err(e) = s.writer.write_all(&data) {
                        eprintln!("[ws/log] PTY write failed for session {session_id}: {e}");
                        break;
                    }
                    let _ = s.writer.flush();
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}
