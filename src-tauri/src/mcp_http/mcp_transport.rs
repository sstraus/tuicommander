use crate::pty::{build_shell_command, resolve_shell, spawn_headless_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::OUTPUT_RING_BUFFER_CAPACITY;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::stream::Stream;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, PtySize};
use std::convert::Infallible;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use super::types::*;

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path)
        .map_err(|msg| serde_json::json!({"error": msg}))
}

const SESSION_ACTIONS: &str = "list, create, input, output, resize, close, pause, resume";
const GIT_ACTIONS: &str = "info, diff, files, branches, github, prs";
const AGENT_ACTIONS: &str = "detect, spawn, stats, metrics";
const CONFIG_ACTIONS: &str = "get, save";

/// MCP tool definitions — 5 meta-commands mirroring tui_mcp_bridge
fn mcp_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "session",
            "description": "Manage PTY terminal sessions.\n\nActions (pass as 'action' parameter):\n- list: Returns [{session_id, cwd, worktree_path, worktree_branch}] for all active sessions. Call first to discover IDs.\n- create: Creates a new PTY session. Returns {session_id}. Optional: rows, cols, shell, cwd.\n- input: Sends text and/or a special key to a session. Requires session_id, plus input and/or special_key.\n- output: Returns {data, total_written} from session ring buffer. Requires session_id. Optional: limit.\n- resize: Resizes PTY dimensions. Requires session_id, rows, cols.\n- close: Terminates a session. Requires session_id.\n- pause: Pauses output buffering. Requires session_id.\n- resume: Resumes output buffering. Requires session_id.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, create, input, output, resize, close, pause, resume" },
                "session_id": { "type": "string", "description": "Session ID (required for input, output, resize, close, pause, resume)" },
                "input": { "type": "string", "description": "Raw text to write (action=input)" },
                "special_key": { "type": "string", "description": "Special key: enter, tab, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+k, ctrl+u, ctrl+w, ctrl+r, up, down, left, right, home, end, backspace, delete, escape (action=input)" },
                "rows": { "type": "integer", "description": "Terminal rows (action=create or resize)" },
                "cols": { "type": "integer", "description": "Terminal cols (action=create or resize)" },
                "shell": { "type": "string", "description": "Shell binary path (action=create)" },
                "cwd": { "type": "string", "description": "Working directory (action=create)" },
                "limit": { "type": "integer", "description": "Bytes to read, default 8192 (action=output)" }
            }, "required": ["action"] }
        },
        {
            "name": "git",
            "description": "Query git repository state and GitHub integration.\n\nActions (pass as 'action' parameter):\n- info: Returns {name, branch, status, remote_url, is_dirty, ahead, behind}. Requires path.\n- diff: Returns {diff} with unified diff of unstaged changes. Requires path.\n- files: Returns [{path, status, insertions, deletions}] for changed files. Requires path.\n- branches: Returns [{name, is_current, is_remote}] branch list. Requires path.\n- github: Returns GitHub integration data (remote, PR, CI, ahead/behind). Requires path.\n- prs: Returns all open PR statuses with CI rollup. Requires path.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: info, diff, files, branches, github, prs" },
                "path": { "type": "string", "description": "Absolute path to git repository (required for all actions)" }
            }, "required": ["action"] }
        },
        {
            "name": "agent",
            "description": "Detect and manage AI agents.\n\nActions (pass as 'action' parameter):\n- detect: Returns [{name, path, version}] for known agents (claude, codex, aider, goose, lazygit).\n- spawn: Launches an agent in a new PTY session. Requires prompt. Returns {session_id}. Use session action=input/output to interact.\n- stats: Returns {active_sessions, max_sessions, available_slots}.\n- metrics: Returns cumulative metrics {total_spawned, total_failed, active_sessions, bytes_emitted, pauses_triggered}.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: detect, spawn, stats, metrics" },
                "prompt": { "type": "string", "description": "Task prompt for the agent (action=spawn)" },
                "cwd": { "type": "string", "description": "Working directory (action=spawn)" },
                "model": { "type": "string", "description": "Model override (action=spawn)" },
                "print_mode": { "type": "boolean", "description": "Non-interactive mode (action=spawn)" },
                "output_format": { "type": "string", "description": "Output format, e.g. 'json' (action=spawn)" },
                "agent_type": { "type": "string", "description": "Agent binary: claude, codex, aider, goose (action=spawn)" },
                "binary_path": { "type": "string", "description": "Override agent binary path (action=spawn)" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "Raw CLI args (action=spawn)" },
                "rows": { "type": "integer", "description": "Terminal rows (action=spawn)" },
                "cols": { "type": "integer", "description": "Terminal cols (action=spawn)" }
            }, "required": ["action"] }
        },
        {
            "name": "config",
            "description": "Read or write app configuration.\n\nActions (pass as 'action' parameter):\n- get: Returns app config (shell, font, theme, worktree_dir, etc.). Password hash is stripped.\n- save: Persists configuration. Requires config object. Partial updates OK.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: get, save" },
                "config": { "type": "object", "description": "Config fields to save (action=save)" }
            }, "required": ["action"] }
        },
        {
            "name": "plugin_dev_guide",
            "description": "Returns comprehensive plugin authoring reference: manifest format, PluginHost API (all 4 tiers), structured event types, and working examples. Call before writing any plugin code.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
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

/// Extract action from args, returning a guidance error if missing
fn require_action<'a>(args: &'a serde_json::Value, tool: &str, available: &str) -> Result<&'a str, serde_json::Value> {
    args["action"]
        .as_str()
        .ok_or_else(|| serde_json::json!({"error": format!("Missing 'action'. Available actions for '{}': {}", tool, available)}))
}

/// Extract session_id from args with guidance error
fn require_session_id<'a>(args: &'a serde_json::Value, action: &str) -> Result<&'a str, serde_json::Value> {
    args["session_id"]
        .as_str()
        .ok_or_else(|| serde_json::json!({"error": format!("Action '{}' requires 'session_id'. Get valid IDs with session action='list'", action)}))
}

/// Extract path from args with guidance error
fn require_path(args: &serde_json::Value, action: &str) -> Result<String, serde_json::Value> {
    args["path"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| serde_json::json!({"error": format!("Action '{}' requires 'path' (absolute path to git repository)", action)}))
}

/// Handle an MCP tools/call request, executing against the app state directly (no HTTP round-trip)
fn handle_mcp_tool_call(state: &Arc<AppState>, addr: SocketAddr, name: &str, args: &serde_json::Value) -> serde_json::Value {
    match name {
        "session" => handle_session(state, args),
        "git" => handle_git(state, args),
        "agent" => handle_agent(state, args),
        "config" => handle_config(state, addr, args),
        "plugin_dev_guide" => {
            serde_json::json!({"content": super::plugin_docs::PLUGIN_DOCS})
        }
        _ => serde_json::json!({"error": format!(
            "Unknown tool '{}'. Available: session, git, agent, config, plugin_dev_guide", name
        )}),
    }
}

fn handle_session(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "session", SESSION_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" => {
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
        "create" => {
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
        "input" => {
            let session_id = match require_session_id(args, "input") {
                Ok(id) => id,
                Err(e) => return e,
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
                return serde_json::json!({"error": "Action 'input' requires 'input' (text) and/or 'special_key'"});
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
        "output" => {
            let session_id = match require_session_id(args, "output") {
                Ok(id) => id,
                Err(e) => return e,
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
        "resize" => {
            let session_id = match require_session_id(args, "resize") {
                Ok(id) => id,
                Err(e) => return e,
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
        "close" => {
            let session_id = match require_session_id(args, "close") {
                Ok(id) => id,
                Err(e) => return e,
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
        "pause" => {
            let session_id = match require_session_id(args, "pause") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let entry = match state.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            entry.lock().paused.store(true, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        "resume" => {
            let session_id = match require_session_id(args, "resume") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let entry = match state.sessions.get(session_id) {
                Some(e) => e,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            entry.lock().paused.store(false, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'session'. Available: {}", other, SESSION_ACTIONS
        )}),
    }
}

fn handle_git(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "git", GIT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "info" => {
            let path = match require_path(args, "info") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let info = crate::git::get_repo_info_impl(&path);
            serde_json::to_value(info).unwrap_or_default()
        }
        "diff" => {
            let path = match require_path(args, "diff") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::git::get_git_diff(path, None) {
                Ok(diff) => serde_json::json!({"diff": diff}),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "files" => {
            let path = match require_path(args, "files") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::git::get_changed_files(path, None) {
                Ok(files) => serde_json::to_value(files).unwrap_or_default(),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "branches" => {
            let path = match require_path(args, "branches") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::git::get_git_branches(path) {
                Ok(branches) => serde_json::to_value(branches).unwrap_or_default(),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "github" => {
            let path = match require_path(args, "github") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let status = crate::github::get_github_status_impl(&path);
            serde_json::to_value(status).unwrap_or_default()
        }
        "prs" => {
            let path = match require_path(args, "prs") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let statuses = crate::github::get_repo_pr_statuses_impl(
                &path,
                state,
            );
            serde_json::to_value(statuses).unwrap_or_default()
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'git'. Available: {}", other, GIT_ACTIONS
        )}),
    }
}

fn handle_agent(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "agent", AGENT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "detect" => {
            let known = ["claude", "codex", "aider", "goose", "lazygit"];
            let results: Vec<serde_json::Value> = known.iter().map(|name| {
                let det = crate::agent::detect_agent_binary(name.to_string());
                serde_json::json!({"name": name, "path": det.path, "version": det.version})
            }).collect();
            serde_json::json!(results)
        }
        "spawn" => {
            let prompt = match args["prompt"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "Action 'spawn' requires 'prompt'"}),
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
            serde_json::json!({"error": "agent action='spawn' via SSE not yet implemented — use the bridge binary or REST API POST /sessions/agent"})
        }
        "stats" => {
            let stats = state.orchestrator_stats();
            serde_json::to_value(stats).unwrap_or_default()
        }
        "metrics" => {
            let metrics = state.session_metrics_json();
            serde_json::to_value(metrics).unwrap_or_default()
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'agent'. Available: {}", other, AGENT_ACTIONS
        )}),
    }
}

fn handle_config(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "config", CONFIG_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "get" => {
            let config = state.config.read().clone();
            let mut json = serde_json::to_value(config).unwrap_or_default();
            if let Some(obj) = json.as_object_mut() {
                obj.remove("remote_access_password_hash");
            }
            json
        }
        "save" => {
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "Config save is restricted to localhost connections"});
            }
            let config_val = match args.get("config") {
                Some(c) => c,
                None => return serde_json::json!({"error": "Action 'save' requires 'config' object"}),
            };
            let config: crate::config::AppConfig = match serde_json::from_value(config_val.clone()) {
                Ok(c) => c,
                Err(e) => return serde_json::json!({"error": format!("Invalid config: {}", e)}),
            };
            match crate::config::save_app_config(config.clone()) {
                Ok(()) => {
                    *state.config.write() = config;
                    serde_json::json!({"ok": true})
                }
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'config'. Available: {}", other, CONFIG_ACTIONS
        )}),
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
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
                        "name": "tuicommander",
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

            let result = handle_mcp_tool_call(&state, addr, tool_name, &args);
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
