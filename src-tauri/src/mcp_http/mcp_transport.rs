use crate::pty::{build_shell_command, resolve_shell, spawn_headless_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::OUTPUT_RING_BUFFER_CAPACITY;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::stream::Stream;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, PtySize};
use std::convert::Infallible;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use super::types::*;

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path)
        .map_err(|msg| serde_json::json!({"error": msg}))
}

/// MCP tool definitions — mirrors tui_mcp_bridge tool_definitions()
fn mcp_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "list_sessions",
            "description": "List all active terminal sessions with their IDs, working directories, and worktree info",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "create_session",
            "description": "Create a new terminal session (PTY). Returns session_id for subsequent operations.",
            "inputSchema": { "type": "object", "properties": {
                "rows": { "type": "integer", "description": "Terminal rows (default: 24)" },
                "cols": { "type": "integer", "description": "Terminal columns (default: 80)" },
                "shell": { "type": "string", "description": "Shell path (default: platform shell)" },
                "cwd": { "type": "string", "description": "Working directory for the session" }
            }, "required": [] }
        },
        {
            "name": "send_input",
            "description": "Send text or a special key to a terminal session. Use 'input' for text, 'special_key' for keys like 'enter', 'ctrl+c', 'tab', 'up', 'down', etc.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string", "description": "Terminal session ID" },
                "input": { "type": "string", "description": "Text to type into the terminal" },
                "special_key": { "type": "string", "description": "Special key: enter, tab, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+k, ctrl+u, ctrl+w, ctrl+r, up, down, left, right, home, end, backspace, delete, escape" }
            }, "required": ["session_id"] }
        },
        {
            "name": "get_output",
            "description": "Read recent terminal output from a session's ring buffer (default 8KB, max 64KB)",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string", "description": "Terminal session ID" },
                "limit": { "type": "integer", "description": "Max bytes to read (default 8192, max 65536)" }
            }, "required": ["session_id"] }
        },
        {
            "name": "resize_terminal",
            "description": "Resize a terminal session",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string", "description": "Terminal session ID" },
                "rows": { "type": "integer", "description": "Number of rows" },
                "cols": { "type": "integer", "description": "Number of columns" }
            }, "required": ["session_id", "rows", "cols"] }
        },
        {
            "name": "close_session",
            "description": "Close a terminal session. Sends Ctrl+C and waits briefly for graceful shutdown.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string", "description": "Terminal session ID" }
            }, "required": ["session_id"] }
        },
        {
            "name": "pause_session",
            "description": "Pause a terminal session's output reader (flow control)",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string", "description": "Terminal session ID" }
            }, "required": ["session_id"] }
        },
        {
            "name": "resume_session",
            "description": "Resume a paused terminal session's output reader (flow control)",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string", "description": "Terminal session ID" }
            }, "required": ["session_id"] }
        },
        {
            "name": "get_stats",
            "description": "Get orchestrator stats: active sessions, max sessions, available slots",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "get_metrics",
            "description": "Get session metrics: total spawned, failed spawns, bytes emitted, pauses triggered",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "get_repo_info",
            "description": "Get git repository info (branch, status, name) for a path",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Repository path" }
            }, "required": ["path"] }
        },
        {
            "name": "get_git_diff",
            "description": "Get unified diff for a repository (unstaged changes)",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Repository path" }
            }, "required": ["path"] }
        },
        {
            "name": "get_changed_files",
            "description": "Get list of changed files with status and per-file +/- stats",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Repository path" }
            }, "required": ["path"] }
        },
        {
            "name": "get_github_status",
            "description": "Get GitHub status: remote info, current branch, PR status, CI status, ahead/behind counts",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Repository path" }
            }, "required": ["path"] }
        },
        {
            "name": "get_pr_statuses",
            "description": "Get all PR statuses for a repository (branch, title, state, CI checks, review decision)",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Repository path" }
            }, "required": ["path"] }
        },
        {
            "name": "get_branches",
            "description": "Get list of git branches (local and remote) with current branch indicator",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Repository path" }
            }, "required": ["path"] }
        },
        {
            "name": "get_config",
            "description": "Get TUI Commander application configuration",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "save_config",
            "description": "Save TUI Commander application configuration",
            "inputSchema": { "type": "object", "properties": {
                "config": { "type": "object", "description": "Configuration object with fields: shell, font_family, font_size, theme, worktree_dir, mcp_server_enabled" }
            }, "required": ["config"] }
        },
        {
            "name": "detect_agents",
            "description": "Detect installed AI agent binaries (claude, codex, aider, goose, lazygit)",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "spawn_agent",
            "description": "Spawn an AI agent in a new terminal session. Returns session_id to interact with the agent.",
            "inputSchema": { "type": "object", "properties": {
                "prompt": { "type": "string", "description": "Prompt/task for the agent" },
                "cwd": { "type": "string", "description": "Working directory" },
                "model": { "type": "string", "description": "Model to use (if supported by agent)" },
                "print_mode": { "type": "boolean", "description": "Use --print mode (non-interactive)" },
                "output_format": { "type": "string", "description": "Output format (e.g., 'json')" },
                "agent_type": { "type": "string", "description": "Agent binary name (default: claude)" },
                "binary_path": { "type": "string", "description": "Explicit path to agent binary" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "Custom args (overrides default arg building)" },
                "rows": { "type": "integer", "description": "Terminal rows (default: 24)" },
                "cols": { "type": "integer", "description": "Terminal columns (default: 80)" }
            }, "required": ["prompt"] }
        }
    ])
}

/// Translate special key names to terminal escape sequences
fn translate_special_key(key: &str) -> Option<&'static str> {
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

/// Handle an MCP tools/call request, executing against the app state directly (no HTTP round-trip)
fn handle_mcp_tool_call(state: &Arc<AppState>, name: &str, args: &serde_json::Value) -> serde_json::Value {
    match name {
        "list_sessions" => {
            let sessions: Vec<serde_json::Value> = state.sessions.iter().map(|entry| {
                let id = entry.key().clone();
                let s = entry.value().lock();
                serde_json::json!({
                    "session_id": id,
                    "cwd": s.cwd,
                    "worktree_path": s.worktree.as_ref().map(|w| w.path.to_string_lossy().to_string()),
                    "worktree_branch": s.worktree.as_ref().and_then(|w| w.branch.clone()),
                })
            }).collect();
            serde_json::json!(sessions)
        }
        "create_session" => {
            // Delegate to the HTTP handler logic by creating the session directly
            if state.sessions.len() >= MAX_CONCURRENT_SESSIONS {
                return serde_json::json!({"error": "Max concurrent sessions reached"});
            }
            let rows = args["rows"].as_u64().unwrap_or(24) as u16;
            let cols = args["cols"].as_u64().unwrap_or(80) as u16;
            if let Err(msg) = super::validate_terminal_size(rows, cols) {
                return serde_json::json!({"error": msg});
            }
            let shell = resolve_shell(args["shell"].as_str().map(|s| s.to_string()));
            let cwd = args["cwd"].as_str().map(|s| s.to_string());

            let session_id = Uuid::new_v4().to_string();
            let pty_system = native_pty_system();
            let pair = match pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }) {
                Ok(p) => p,
                Err(e) => return serde_json::json!({"error": format!("Failed to open PTY: {}", e)}),
            };
            let mut cmd = build_shell_command(&shell);
            if let Some(ref dir) = cwd { cmd.cwd(dir); }
            let child = match pair.slave.spawn_command(cmd) {
                Ok(c) => c,
                Err(e) => return serde_json::json!({"error": format!("Failed to spawn shell: {}", e)}),
            };
            let writer = match pair.master.take_writer() {
                Ok(w) => w,
                Err(e) => return serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)}),
            };
            let reader = match pair.master.try_clone_reader() {
                Ok(r) => r,
                Err(e) => return serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)}),
            };
            let paused = Arc::new(AtomicBool::new(false));
            state.sessions.insert(session_id.clone(), Mutex::new(PtySession {
                writer, master: pair.master, _child: child, paused: paused.clone(), worktree: None, cwd,
            }));
            state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
            state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
            state.output_buffers.insert(session_id.clone(), Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)));
            spawn_headless_reader_thread(reader, paused, session_id.clone(), state.clone());
            serde_json::json!({"session_id": session_id})
        }
        "send_input" => {
            let session_id = match args["session_id"].as_str() {
                Some(id) => id,
                None => return serde_json::json!({"error": "missing session_id"}),
            };
            let mut data = String::new();
            if let Some(input) = args["input"].as_str() { data.push_str(input); }
            if let Some(key) = args["special_key"].as_str() {
                match translate_special_key(key) {
                    Some(seq) => data.push_str(seq),
                    None => return serde_json::json!({"error": format!("Unknown special key: {}", key)}),
                }
            }
            if data.is_empty() {
                return serde_json::json!({"error": "Either 'input' or 'special_key' must be provided"});
            }
            let entry = match state.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            let mut session = entry.lock();
            if let Err(e) = session.writer.write_all(data.as_bytes()) {
                return serde_json::json!({"error": format!("Write failed: {}", e)});
            }
            if let Err(e) = session.writer.flush() {
                eprintln!("Warning: PTY flush failed for session {session_id}: {e}");
            }
            serde_json::json!({"ok": true})
        }
        "get_output" => {
            let session_id = match args["session_id"].as_str() {
                Some(id) => id,
                None => return serde_json::json!({"error": "missing session_id"}),
            };
            let limit = args["limit"].as_u64().unwrap_or(8192) as usize;
            let ring = match state.output_buffers.get(session_id) {
                Some(r) => r,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            let (bytes, total_written) = ring.lock().read_last(limit);
            let data = String::from_utf8_lossy(&bytes).to_string();
            serde_json::json!({"data": data, "total_written": total_written})
        }
        "resize_terminal" => {
            let session_id = match args["session_id"].as_str() {
                Some(id) => id,
                None => return serde_json::json!({"error": "missing session_id"}),
            };
            let rows = args["rows"].as_u64().unwrap_or(24) as u16;
            let cols = args["cols"].as_u64().unwrap_or(80) as u16;
            if let Err(msg) = super::validate_terminal_size(rows, cols) {
                return serde_json::json!({"error": msg});
            }
            let entry = match state.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            if let Err(e) = entry.lock().master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }) {
                return serde_json::json!({"error": format!("Resize failed: {}", e)});
            }
            serde_json::json!({"ok": true})
        }
        "close_session" => {
            let session_id = match args["session_id"].as_str() {
                Some(id) => id,
                None => return serde_json::json!({"error": "missing session_id"}),
            };
            if let Some((_, session_mutex)) = state.sessions.remove(session_id) {
                state.output_buffers.remove(session_id);
                let mut session = session_mutex.into_inner();
                let _ = session.writer.write_all(&[0x03]);
                let _ = session.writer.flush();
                drop(session);
                serde_json::json!({"ok": true})
            } else {
                serde_json::json!({"error": "Session not found"})
            }
        }
        "pause_session" => {
            let session_id = match args["session_id"].as_str() {
                Some(id) => id,
                None => return serde_json::json!({"error": "missing session_id"}),
            };
            let entry = match state.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            entry.lock().paused.store(true, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        "resume_session" => {
            let session_id = match args["session_id"].as_str() {
                Some(id) => id,
                None => return serde_json::json!({"error": "missing session_id"}),
            };
            let entry = match state.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            entry.lock().paused.store(false, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        "get_stats" => {
            let stats = state.orchestrator_stats();
            serde_json::to_value(stats).unwrap_or_default()
        }
        "get_metrics" => {
            let metrics = state.session_metrics_json();
            serde_json::to_value(metrics).unwrap_or_default()
        }
        "get_repo_info" => {
            let path = match args["path"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing path"}),
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let info = crate::git::get_repo_info_impl(&path);
            serde_json::to_value(info).unwrap_or_default()
        }
        "get_git_diff" => {
            let path = match args["path"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing path"}),
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::git::get_git_diff(path, None) {
                Ok(diff) => serde_json::json!({"diff": diff}),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "get_changed_files" => {
            let path = match args["path"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing path"}),
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::git::get_changed_files(path, None) {
                Ok(files) => serde_json::to_value(files).unwrap_or_default(),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "get_github_status" => {
            let path = match args["path"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing path"}),
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let status = crate::github::get_github_status_impl(&path);
            serde_json::to_value(status).unwrap_or_default()
        }
        "get_pr_statuses" => {
            let path = match args["path"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing path"}),
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let statuses = crate::github::get_repo_pr_statuses_impl(
                &path,
                state,
            );
            serde_json::to_value(statuses).unwrap_or_default()
        }
        "get_branches" => {
            let path = match args["path"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing path"}),
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::git::get_git_branches(path) {
                Ok(branches) => serde_json::to_value(branches).unwrap_or_default(),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "get_config" => {
            let config = state.config.read().unwrap().clone();
            let mut json = serde_json::to_value(config).unwrap_or_default();
            if let Some(obj) = json.as_object_mut() {
                obj.remove("remote_access_password_hash");
            }
            json
        }
        "save_config" => {
            let config_val = match args.get("config") {
                Some(c) => c,
                None => return serde_json::json!({"error": "missing config"}),
            };
            let config: crate::config::AppConfig = match serde_json::from_value(config_val.clone()) {
                Ok(c) => c,
                Err(e) => return serde_json::json!({"error": format!("Invalid config: {}", e)}),
            };
            match crate::config::save_app_config(config.clone()) {
                Ok(()) => {
                    *state.config.write().unwrap() = config;
                    serde_json::json!({"ok": true})
                }
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "detect_agents" => {
            let known = ["claude", "codex", "aider", "goose", "lazygit"];
            let results: Vec<serde_json::Value> = known.iter().map(|name| {
                let det = crate::agent::detect_agent_binary(name.to_string());
                serde_json::json!({"name": name, "path": det.path, "version": det.version})
            }).collect();
            serde_json::json!(results)
        }
        "spawn_agent" => {
            let prompt = match args["prompt"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "missing prompt"}),
            };
            let _body = SpawnAgentRequest {
                rows: args["rows"].as_u64().map(|v| v as u16),
                cols: args["cols"].as_u64().map(|v| v as u16),
                cwd: args["cwd"].as_str().map(|s| s.to_string()),
                prompt,
                model: args["model"].as_str().map(|s| s.to_string()),
                print_mode: args["print_mode"].as_bool(),
                output_format: args["output_format"].as_str().map(|s| s.to_string()),
                agent_type: args["agent_type"].as_str().map(|s| s.to_string()),
                binary_path: args["binary_path"].as_str().map(|s| s.to_string()),
                args: args.get("args").and_then(|a| serde_json::from_value(a.clone()).ok()),
            };
            // Reuse the HTTP handler's agent spawn logic is complex; call it inline
            serde_json::json!({"error": "spawn_agent via SSE not yet implemented — use REST API"})
        }
        _ => serde_json::json!({"error": format!("Unknown tool: {}", name)}),
    }
}

/// GET /sse — Establish MCP SSE transport connection
pub(super) async fn mcp_sse_connect(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    state.mcp_sse_sessions.insert(session_id.clone(), tx);

    let messages_url = format!("/messages?sessionId={}", session_id);
    let sid_for_cleanup = session_id.clone();
    let state_for_cleanup = state.clone();

    let stream = async_stream::stream! {
        // First event: tell the client where to POST messages
        yield Ok(Event::default().event("endpoint").data(messages_url));

        // Stream JSON-RPC responses from the channel
        while let Some(msg) = rx.recv().await {
            yield Ok(Event::default().event("message").data(msg));
        }

        // Cleanup on disconnect
        state_for_cleanup.mcp_sse_sessions.remove(&sid_for_cleanup);
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// POST /messages?sessionId=xxx — Handle MCP JSON-RPC requests
pub(super) async fn mcp_messages(
    State(state): State<Arc<AppState>>,
    Query(query): Query<McpSessionQuery>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let session_id = &query.session_id;

    // Look up the SSE session's sender channel
    let tx = match state.mcp_sse_sessions.get(session_id) {
        Some(entry) => entry.value().clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "MCP session not found"})),
            );
        }
    };

    let method = body["method"].as_str().unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);

    match method {
        "initialize" => {
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "tui-commander",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            });
            let _ = tx.send(serde_json::to_string(&response).unwrap_or_default());
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }

        "notifications/initialized" => {
            // Client acknowledgment, no response needed
            (StatusCode::ACCEPTED, Json(serde_json::json!({})))
        }

        "tools/list" => {
            let tools = mcp_tool_definitions();
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools }
            });
            let _ = tx.send(serde_json::to_string(&response).unwrap_or_default());
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }

        "tools/call" => {
            let params = body.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool_name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

            let result = handle_mcp_tool_call(&state, tool_name, &args);
            let text = serde_json::to_string_pretty(&result).unwrap_or_default();

            let is_error = result.get("error").is_some();
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": text }],
                    "isError": is_error
                }
            });
            let _ = tx.send(serde_json::to_string(&response).unwrap_or_default());
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }

        other => {
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {}", other) }
            });
            let _ = tx.send(serde_json::to_string(&response).unwrap_or_default());
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
    }
}

// Re-export for tests — these need to be public enough for sibling test module
#[cfg(test)]
pub(crate) fn test_mcp_tool_definitions() -> serde_json::Value {
    mcp_tool_definitions()
}
#[cfg(test)]
pub(crate) fn test_translate_special_key(key: &str) -> Option<&'static str> {
    translate_special_key(key)
}
#[cfg(test)]
pub(crate) fn test_validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    validate_mcp_repo_path(path)
}
