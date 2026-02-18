use crate::pty::{build_shell_command, resolve_shell, spawn_headless_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::OUTPUT_RING_BUFFER_CAPACITY;
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
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn get_output(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<OutputQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(8192);
    let ring = match state.output_buffers.get(&session_id) {
        Some(r) => r,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Session not found"})),
            )
        }
    };
    let (bytes, total_written) = ring.lock().read_last(limit);
    let data = String::from_utf8_lossy(&bytes).to_string();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "data": data,
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

    let mut cmd = build_shell_command(&shell);
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

    spawn_headless_reader_thread(reader, paused, session_id.clone(), state);

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
            let _ = session;
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
    let worktree = match crate::worktree::create_worktree_internal(&state.worktrees_dir, &wt_config) {
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

    let mut cmd = build_shell_command(&shell);
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

    spawn_headless_reader_thread(reader, paused, session_id.clone(), state);

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
pub(super) async fn ws_stream(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if !state.sessions.contains_key(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.on_upgrade(move |socket| handle_ws_session(socket, id, state))
}

/// Handle a WebSocket connection for a PTY session.
async fn handle_ws_session(socket: WebSocket, session_id: String, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Register a channel for receiving PTY output broadcasts
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state
        .ws_clients
        .entry(session_id.clone())
        .or_default()
        .push(tx);

    // Send existing ring buffer content as initial catch-up
    if let Some(ring) = state.output_buffers.get(&session_id) {
        let (data, _) = ring.lock().read_last(64 * 1024);
        if !data.is_empty() {
            let text = String::from_utf8_lossy(&data).into_owned();
            let _ = futures_util::SinkExt::send(&mut ws_sender, Message::Text(text.into())).await;
        }
    }

    // Spawn a task to forward PTY output to the WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if futures_util::SinkExt::send(&mut ws_sender, Message::Text(data.into()))
                .await
                .is_err()
            {
                break;
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
                    let _ = s.writer.write_all(text.as_bytes());
                    let _ = s.writer.flush();
                }
            }
            Message::Binary(data) => {
                if let Some(session) = state_clone.sessions.get(&sid) {
                    let mut s = session.lock();
                    let _ = s.writer.write_all(&data);
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
