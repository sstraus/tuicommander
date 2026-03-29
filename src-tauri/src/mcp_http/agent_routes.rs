use crate::pty::{spawn_headless_reader_thread, spawn_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::{OUTPUT_RING_BUFFER_CAPACITY, VtLogBuffer, VT_LOG_BUFFER_CAPACITY};
use tauri::Emitter;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use super::types::*;

pub(super) async fn detect_agents() -> impl IntoResponse {
    let known_agents = ["claude", "codex", "aider", "goose"];
    let results: Vec<serde_json::Value> = known_agents
        .iter()
        .map(|name| {
            let detection = crate::agent::detect_agent_binary(name.to_string());
            serde_json::json!({
                "name": name,
                "path": detection.path,
                "version": detection.version,
            })
        })
        .collect();
    Json(results)
}

pub(super) async fn detect_agent_binary_http(Query(q): Query<DetectBinaryQuery>) -> Response {
    const KNOWN_AGENTS: &[&str] = &["claude", "codex", "aider", "goose"];
    if !KNOWN_AGENTS.contains(&q.binary.as_str()) {
        return Json(serde_json::json!({"error": "Unknown agent"})).into_response();
    }
    let detection = crate::agent::detect_agent_binary(q.binary);
    Json(serde_json::json!({
        "path": detection.path,
        "version": detection.version,
    })).into_response()
}

pub(super) async fn detect_installed_ides_http() -> impl IntoResponse {
    Json(crate::agent::detect_installed_ides())
}

pub(super) async fn process_prompt_http(Json(body): Json<ProcessPromptRequest>) -> impl IntoResponse {
    Json(crate::prompt::process_prompt_content(body.content, body.variables))
}

pub(super) async fn extract_prompt_variables_http(
    Json(body): Json<ExtractVariablesRequest>,
) -> impl IntoResponse {
    Json(crate::prompt::extract_prompt_variables(body.content))
}

pub(super) async fn resolve_context_variables_http(
    Json(body): Json<serde_json::Value>,
) -> Response {
    let repo_path = body.get("repoPath").and_then(|v| v.as_str()).unwrap_or("").to_string();
    match crate::prompt::resolve_context_variables(repo_path).await {
        Ok(vars) => Json(vars).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn execute_headless_prompt_http(
    Json(body): Json<serde_json::Value>,
) -> Response {
    let command_line = body.get("commandLine").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let stdin_content = body.get("stdinContent").and_then(|v| v.as_str()).map(String::from);
    let timeout_ms = body.get("timeoutMs").and_then(|v| v.as_u64()).unwrap_or(300_000);
    let repo_path = body.get("repoPath").and_then(|v| v.as_str()).unwrap_or("").to_string();
    match crate::smart_prompt::execute_headless_prompt(command_line, stdin_content, timeout_ms, repo_path).await {
        Ok(output) => Json(output).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn execute_api_prompt_http(
    Json(body): Json<serde_json::Value>,
) -> Response {
    let system_prompt = body.get("systemPrompt").and_then(|v| v.as_str()).map(String::from);
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let timeout_ms = body.get("timeoutMs").and_then(|v| v.as_u64()).unwrap_or(60_000);
    match crate::llm_api::execute_api_prompt(system_prompt, content, timeout_ms).await {
        Ok(output) => Json(output).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn verify_agent_session_http(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let agent_type = body.get("agentType").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string();
    Json(crate::agent_session::verify_agent_session(agent_type, session_id, cwd))
}

pub(super) async fn spawn_agent_session(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<SpawnAgentRequest>,
) -> Response {
    if !addr.ip().is_loopback() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Agent session spawning is only allowed from localhost"})),
        ).into_response();
    }
    if state.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        ).into_response();
    }

    // Determine binary path
    let binary_path = if let Some(ref path) = body.binary_path {
        let p = std::path::Path::new(path);
        if !p.is_absolute() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "binary_path must be an absolute path"})),
            ).into_response();
        }
        if !p.is_file() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "binary_path does not point to an existing file"})),
            ).into_response();
        }
        path.clone()
    } else if let Some(ref agent_type) = body.agent_type {
        let detection = crate::agent::detect_agent_binary(agent_type.clone());
        match detection.path {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("Agent binary '{}' not found", agent_type)})),
                ).into_response()
            }
        }
    } else {
        // Default to claude
        let detection = crate::agent::detect_agent_binary("claude".to_string());
        match detection.path {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Claude binary not found. Install with: npm install -g @anthropic-ai/claude-code"})),
                ).into_response()
            }
        }
    };

    let rows = body.rows.unwrap_or(24);
    let cols = body.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg}))).into_response();
    }
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
            ).into_response()
        }
    };

    let mut cmd = CommandBuilder::new(&binary_path);

    if let Some(ref args) = body.args {
        for arg in args {
            cmd.arg(arg);
        }
    } else {
        if body.print_mode.unwrap_or(false) {
            cmd.arg("--print");
        }
        if let Some(ref format) = body.output_format {
            cmd.arg("--output-format");
            cmd.arg(format);
        }
        if let Some(ref model) = body.model {
            cmd.arg("--model");
            cmd.arg(model);
        }
        cmd.arg(&body.prompt);
    }

    if let Some(ref cwd) = body.cwd {
        cmd.cwd(cwd);
    }

    let child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to spawn agent: {}", e)})),
            ).into_response()
        }
    };

    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)})),
            ).into_response()
        }
    };

    let reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)})),
            ).into_response()
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
            display_name: None,
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
    state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

    // Broadcast to SSE/WebSocket consumers (before state is moved to reader thread)
    let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
        session_id: session_id.clone(),
        cwd: body.cwd.clone(),
        agent_type: body.agent_type.clone(),
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
            "cwd": body.cwd,
        }));
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"session_id": session_id})),
    ).into_response()
}
