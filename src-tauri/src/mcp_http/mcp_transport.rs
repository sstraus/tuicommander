use crate::pty::{build_shell_command, resolve_shell, spawn_headless_reader_thread, spawn_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::{OUTPUT_RING_BUFFER_CAPACITY, VtLogBuffer, VT_LOG_BUFFER_CAPACITY};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use uuid::Uuid;

/// Serialize a value to JSON, returning a structured error on failure instead of silent null.
fn to_json_or_error<T: serde::Serialize>(value: T) -> serde_json::Value {
    match serde_json::to_value(value) {
        Ok(v) => v,
        Err(e) => serde_json::json!({"error": format!("Serialization failed: {e}")}),
    }
}

/// Build server instructions for the MCP initialize response.
/// Tells the connecting agent what tools are available, which repos are managed,
/// and what sessions are currently active so it can orient itself.
fn build_mcp_instructions(state: &Arc<AppState>, client_name: Option<&str>) -> String {
    let ver = env!("CARGO_PKG_VERSION");
    let mut out = String::with_capacity(2048);

    // Header — identity + capabilities (drives MCP Tool Search routing)
    out.push_str(&format!("# TUICommander v{ver}\n\n"));
    out.push_str("Terminal session orchestrator. Manage PTY terminals, query git repos, spawn AI agents, and configure the app.\n\n");
    out.push_str(&format!(
        "When you start a conversation, mention that TUICommander v{ver} is connected.\n\n"
    ));

    // Workflow — guide agents to use the right tools
    out.push_str("## Workflow\n\n");
    out.push_str("- `workspace action=list` → discover all repos, branches, ahead/behind\n");
    out.push_str("- `session action=create` with `cwd` → spawn new terminal (auto-appears in TUI)\n");
    out.push_str("- `session action=output` → read terminal (`exited`/`exit_code` tell you when done)\n");
    out.push_str("- `agent action=spawn` → launch AI agent in new PTY\n");
    out.push_str("- `github action=prs` → all open PRs with CI rollup (single GraphQL batch)\n");
    out.push_str("- `worktree action=create` → create isolated worktree, optional `spawn_session`\n");
    out.push_str("- **Git operations:** use native `git` CLI — no MCP wrapper needed\n\n");
    // Claude Code-specific: guide teammate spawning through TUICommander PTY.
    // The bridge binary (`tuic-bridge`) is exclusively used by Claude Code.
    let is_claude_code = client_name.is_some_and(|n| n.contains("claude") || n.contains("tuic-bridge"));
    if is_claude_code {
        out.push_str("**Teammates:** When spawning teammates for parallel work, use `worktree action=create` with `spawn_session=true` — creates an isolated worktree + PTY visible in the UI.\n\n");
    }

    // Managed repositories (dynamic)
    let repo_settings = crate::config::load_repo_settings();
    if !repo_settings.repos.is_empty() {
        out.push_str("## Repos\n\n");
        let mut repos: Vec<_> = repo_settings.repos.iter().collect();
        repos.sort_by_key(|(path, _)| path.to_string());
        for (path, entry) in &repos {
            let name = if entry.display_name.is_empty() {
                path.rsplit('/').next().unwrap_or(path)
            } else {
                &entry.display_name
            };
            out.push_str(&format!("- **{name}** `{path}`\n"));
        }
        out.push('\n');
    }

    // Active PTY sessions (dynamic)
    let sessions: Vec<_> = state.sessions.iter().map(|entry| {
        let id = entry.key().clone();
        let session = entry.value().lock();
        (id, session.cwd.clone(), session.worktree.as_ref().and_then(|w| w.branch.clone()))
    }).collect();

    if !sessions.is_empty() {
        out.push_str("## Sessions\n\n");
        for (id, cwd, branch) in &sessions {
            let short_id = &id[..8.min(id.len())];
            let cwd = cwd.as_deref().unwrap_or("—");
            let branch = branch.as_deref().unwrap_or("—");
            out.push_str(&format!("- `{short_id}` {cwd} ({branch})\n"));
        }
        out.push('\n');
    }

    // Protocols — compact format
    out.push_str("## Protocols\n\n");
    out.push_str("**Intent:** At each work phase start, emit: `[[intent: <action, <60 chars>(<tab title, max 3 words>)]]`\n");

    if state.config.read().suggest_followups {
        out.push_str("**Follow-ups:** After tasks, emit: `[[suggest: Action1 | Action2 | Action3]]` (2-4 items, 2-5 words each)\n");
    }

    out
}

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path)
        .map_err(|msg| serde_json::json!({"error": msg}))
}

const SESSION_ACTIONS: &str = "list, create, input, output, resize, close, pause, resume";
const AGENT_ACTIONS: &str = "detect, spawn, stats, metrics";
const GITHUB_ACTIONS: &str = "prs, status";
const WORKTREE_ACTIONS: &str = "list, create, remove";
const CONFIG_ACTIONS: &str = "get, save";
const WORKSPACE_ACTIONS: &str = "list, active";
const NOTIFY_ACTIONS: &str = "toast, confirm";

/// MCP tool definitions — 5 meta-commands mirroring tui_mcp_bridge
fn native_tool_definitions() -> serde_json::Value {
    let defs = serde_json::json!([
        {
            "name": "session",
            "description": "Manage PTY terminal sessions.\n\nActions (pass as 'action' parameter):\n- list: Returns [{session_id, cwd, worktree_path, worktree_branch}] for all active sessions. Call first to discover IDs.\n- create: Creates a new PTY session. Returns {session_id}. Optional: rows, cols, shell, cwd.\n- input: Sends text and/or a special key to a session. Requires session_id, plus input and/or special_key.\n- output: Returns {data, total_written, exited, exit_code} from session ring buffer. Requires session_id. Optional: limit. exited=true when process has terminated; exit_code is the process return code (null if still running).\n- resize: Resizes PTY dimensions. Requires session_id, rows, cols.\n- close: Terminates a session. Requires session_id.\n- pause: Pauses output buffering. Requires session_id.\n- resume: Resumes output buffering. Requires session_id.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, create, input, output, resize, close, pause, resume" },
                "session_id": { "type": "string", "description": "Session ID (required for input, output, resize, close, pause, resume)" },
                "input": { "type": "string", "description": "Raw text to write (action=input)" },
                "special_key": { "type": "string", "description": "Special key: enter, tab, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+k, ctrl+u, ctrl+w, ctrl+r, up, down, left, right, home, end, backspace, delete, escape (action=input)" },
                "rows": { "type": "integer", "description": "Terminal rows (action=create or resize)" },
                "cols": { "type": "integer", "description": "Terminal cols (action=create or resize)" },
                "shell": { "type": "string", "description": "Shell binary path (action=create)" },
                "cwd": { "type": "string", "description": "Working directory (action=create)" },
                "limit": { "type": "integer", "description": "Bytes to read, default 8192 (action=output)" },
                "format": { "type": "string", "description": "Output format: ANSI escape codes are stripped by default; pass 'raw' to preserve them (action=output)" }
            }, "required": ["action"] }
        },
        {
            "name": "github",
            "description": "Query GitHub integration: PR statuses, CI rollup, merge readiness.\n\nActions (pass as 'action' parameter):\n- prs: Returns all open PRs with CI rollup, merge readiness labels, review state. Requires path. Single GraphQL batch — replaces N individual `gh pr` calls.\n- status: Returns cross-repo aggregate: for each workspace repo, returns {path, branch, ahead, behind, open_prs, failing_ci}.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: prs, status" },
                "path": { "type": "string", "description": "Absolute path to git repository (required for prs action)" }
            }, "required": ["action"] }
        },
        {
            "name": "worktree",
            "description": "Manage git worktrees for parallel work.\n\nActions (pass as 'action' parameter):\n- list: Returns [{branch, path}] for all worktrees of a repo. Requires path.\n- create: Creates a new worktree with optional branch. Requires path. Optional: branch, base_ref, spawn_session (auto-creates PTY). Returns {worktree_path, branch}.\n- remove: Removes a worktree by branch name. Requires path, branch.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, create, remove" },
                "path": { "type": "string", "description": "Absolute path to base git repository" },
                "branch": { "type": "string", "description": "Branch name (action=create optional, action=remove required)" },
                "base_ref": { "type": "string", "description": "Base ref to branch from, default HEAD (action=create)" },
                "spawn_session": { "type": "boolean", "description": "Auto-create a PTY session in the worktree (action=create, default false)" }
            }, "required": ["action", "path"] }
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
            "description": "Read or write app configuration.\n\nActions (pass as 'action' parameter):\n- get: Returns app config (shell, font, theme, etc.). Password hash is stripped.\n- save: Persists configuration. Requires config object. Partial updates OK.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: get, save" },
                "config": { "type": "object", "description": "Config fields to save (action=save)" }
            }, "required": ["action"] }
        },
        {
            "name": "workspace",
            "description": "Query the workspace: open repositories, groups, worktrees, and active focus.\n\nActions (pass as 'action' parameter):\n- list: Returns all open repos with group membership, branch, dirty status, and worktrees.\n- active: Returns the currently focused repo path, branch, and group.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, active" }
            }, "required": ["action"] }
        },
        {
            "name": "notify",
            "description": "Show notifications to the TUIC user.\n\nActions (pass as 'action' parameter):\n- toast: Shows a temporary notification. Requires title. Optional: message, level (info/warn/error).\n- confirm: Shows a blocking confirmation dialog. Requires title. Optional: message. Returns {confirmed: boolean}. Restricted to localhost.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: toast, confirm" },
                "title": { "type": "string", "description": "Notification title (required)" },
                "message": { "type": "string", "description": "Optional body text" },
                "level": { "type": "string", "description": "Toast level: info, warn, error (default: info)" }
            }, "required": ["action", "title"] }
        },
        {
            "name": "plugin_dev_guide",
            "description": "Returns comprehensive plugin authoring reference: manifest format, PluginHost API (all 4 tiers), structured event types, and working examples. Call before writing any plugin code.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }
    ]);

    // Guard invariant: native tool names must never contain "__" — that prefix
    // is the routing discriminator for upstream proxy tools.
    #[cfg(debug_assertions)]
    if let Some(arr) = defs.as_array() {
        for tool in arr {
            let name = tool["name"].as_str().unwrap_or("");
            debug_assert!(
                !name.contains("__"),
                "Native tool name '{name}' contains '__' — reserved for upstream namespace separator"
            );
        }
    }

    defs
}

/// Returns native tools merged with upstream proxy tools (namespaced as `{upstream}__`).
///
/// Upstream tools are omitted when no upstreams are Ready.
/// Native tools listed in `config.disabled_native_tools` are excluded.
fn merged_tool_definitions(state: &Arc<AppState>) -> serde_json::Value {
    let disabled = &state.config.read().disabled_native_tools;
    let mut tools: Vec<serde_json::Value> = native_tool_definitions()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            let name = t["name"].as_str().unwrap_or("");
            !disabled.contains(&name.to_string())
        })
        .collect();

    let upstream_tools = state.mcp_upstream_registry.aggregated_tools();
    tools.extend(upstream_tools);

    serde_json::Value::Array(tools)
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
async fn handle_mcp_tool_call(state: &Arc<AppState>, addr: SocketAddr, name: &str, args: &serde_json::Value) -> serde_json::Value {
    match name {
        "session" => handle_session(state, args),
        "github" => handle_github(state, args).await,
        "worktree" => handle_worktree(state, args),
        "agent" => handle_agent(state, addr, args),
        "config" => handle_config(state, addr, args),
        "workspace" => handle_workspace(state, args),
        "notify" => handle_notify(state, addr, args),
        "plugin_dev_guide" => {
            serde_json::json!({"content": super::plugin_docs::PLUGIN_DOCS})
        }
        _ => serde_json::json!({"error": format!(
            "Unknown tool '{}'. Available: session, github, worktree, agent, config, workspace, notify, plugin_dev_guide", name
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
                writer, master: pair.master, _child: child, paused: paused.clone(), worktree: None, cwd: cwd.clone(), display_name: None,
            }));
            state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
            state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
            state.output_buffers.insert(session_id.clone(), Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)));
            state.vt_log_buffers.insert(session_id.clone(), Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)));
            state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

            // Broadcast to SSE/WebSocket consumers
            let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
                session_id: session_id.clone(),
                cwd: cwd.clone(),
            });

            // Use full reader thread (with Tauri events) when AppHandle is available
            let app_handle = state.app_handle.read().clone();
            if let Some(ref app) = app_handle {
                spawn_reader_thread(reader, paused, session_id.clone(), app.clone(), state.clone());
            } else {
                spawn_headless_reader_thread(reader, paused, session_id.clone(), state.clone());
            }

            // Emit Tauri IPC event so frontend creates a tab
            if let Some(app) = app_handle {
                let _ = app.emit("session-created", serde_json::json!({
                    "session_id": session_id,
                    "cwd": cwd,
                }));
            }

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
                tracing::warn!(session_id = %session_id, "PTY flush failed: {e}");
            }
            serde_json::json!({"ok": true})
        }
        "output" => {
            let session_id = match require_session_id(args, "output") {
                Ok(id) => id,
                Err(e) => return e,
            };
            let limit = args["limit"].as_u64().unwrap_or(8192) as usize;

            // Probe child exit status (non-blocking).
            // If session was already removed from the DashMap but buffers survive, treat as exited.
            let (exited, exit_code) = match state.sessions.get(session_id) {
                Some(entry) => {
                    match entry.lock()._child.try_wait() {
                        Ok(Some(status)) => (true, status.exit_code() as i64),
                        _ => (false, 0),
                    }
                }
                None => (true, 0), // session removed but buffers remain
            };

            // Default: serve clean rows from VtLogBuffer (no strip_ansi needed).
            // Pass format="raw" to get the raw ring buffer content with ANSI.
            if args["format"].as_str() != Some("raw") {
                let vt_log = match state.vt_log_buffers.get(session_id) {
                    Some(b) => b,
                    None => return serde_json::json!({"error": "Session not found"}),
                };
                let buf = vt_log.lock();
                let total = buf.total_lines();
                let offset = total.saturating_sub(limit);
                let (log_lines, _) = buf.lines_since_owned(offset);
                let screen: Vec<String> = buf.screen_rows()
                    .into_iter()
                    .filter(|r| !r.is_empty())
                    .collect();
                let mut all_lines: Vec<String> = log_lines.iter().map(|ll| ll.text()).collect();
                all_lines.extend(screen);
                let data = all_lines.join("\n");
                return serde_json::json!({"data": data, "data_length": data.len(), "total_written": total, "exited": exited, "exit_code": if exited { serde_json::Value::from(exit_code) } else { serde_json::Value::Null }});
            }
            let ring = match state.output_buffers.get(session_id) {
                Some(r) => r,
                None => return serde_json::json!({"error": "Session not found"}),
            };
            let (bytes, total_written) = ring.lock().read_last(limit);
            let data = String::from_utf8_lossy(&bytes).to_string();
            serde_json::json!({"data": data, "data_length": data.len(), "total_written": total_written, "exited": exited, "exit_code": if exited { serde_json::Value::from(exit_code) } else { serde_json::Value::Null }})
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
                state.vt_log_buffers.remove(session_id);
                state.ws_clients.remove(session_id);
                state.kitty_states.remove(session_id);
                state.input_buffers.remove(session_id);
                state.silence_states.remove(session_id);
                state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);

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

async fn handle_github(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "github", GITHUB_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "prs" => {
            let path = match require_path(args, "prs") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let statuses = crate::github::get_repo_pr_statuses_impl(
                &path,
                false,
                state,
            ).await;
            to_json_or_error(statuses)
        }
        "status" => {
            // Cross-repo aggregate: for each workspace repo, return branch/ahead/behind/open PRs
            let repo_data = crate::config::load_repositories();
            let repo_order = repo_data.get("repoOrder")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut results: Vec<serde_json::Value> = Vec::new();
            for path_val in &repo_order {
                let Some(path) = path_val.as_str() else { continue };
                let info = crate::git::get_repo_info_impl(path);
                if !info.is_git_repo { continue; }
                let gh = crate::github::get_github_status_impl(path);
                let prs = crate::github::get_repo_pr_statuses_impl(path, false, state).await;
                let (open_prs, failing_ci) = match &prs {
                    Ok(v) => (v.len(), v.iter().filter(|p| p.checks.failed > 0).count()),
                    Err(_) => (0, 0),
                };
                results.push(serde_json::json!({
                    "path": path,
                    "branch": info.branch,
                    "status": info.status,
                    "ahead": gh.ahead,
                    "behind": gh.behind,
                    "open_prs": open_prs,
                    "failing_ci": failing_ci,
                }));
            }
            serde_json::json!(results)
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'github'. Available: {}", other, GITHUB_ACTIONS
        )}),
    }
}

/// Create a PTY session in the given directory, returning the session ID.
/// Reuses the same setup as `session action=create` but with fixed defaults.
fn create_session_in_dir(state: &Arc<AppState>, cwd: &str) -> Result<String, String> {
    let shell = resolve_shell(None);
    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;
    let mut cmd = build_shell_command(&shell);
    cmd.cwd(cwd);
    let child = pair.slave.spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn shell: {e}"))?;
    let writer = pair.master.take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;
    let reader = pair.master.try_clone_reader()
        .map_err(|e| format!("Failed to get PTY reader: {e}"))?;
    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(session_id.clone(), Mutex::new(PtySession {
        writer, master: pair.master, _child: child, paused: paused.clone(),
        worktree: None, cwd: Some(cwd.to_string()), display_name: None,
    }));
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
    state.output_buffers.insert(session_id.clone(), Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)));
    state.vt_log_buffers.insert(session_id.clone(), Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)));
    state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));
    let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
        session_id: session_id.clone(), cwd: Some(cwd.to_string()),
    });
    let app_handle = state.app_handle.read().clone();
    if let Some(ref app) = app_handle {
        spawn_reader_thread(reader, paused, session_id.clone(), app.clone(), state.clone());
    } else {
        spawn_headless_reader_thread(reader, paused, session_id.clone(), state.clone());
    }
    if let Some(app) = app_handle {
        let _ = app.emit("session-created", serde_json::json!({
            "session_id": session_id, "cwd": cwd,
        }));
    }
    Ok(session_id)
}

fn handle_worktree(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "worktree", WORKTREE_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" => {
            let path = match require_path(args, "list") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            match crate::worktree::get_worktree_paths(path) {
                Ok(wts) => to_json_or_error(wts),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "create" => {
            let path = match require_path(args, "create") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let branch = args["branch"].as_str().map(|s| s.to_string());
            let base_ref = args["base_ref"].as_str();

            // Generate a branch name if not specified
            let branch_name = branch.unwrap_or_else(|| {
                let existing: Vec<String> = crate::worktree::get_worktree_paths(path.clone())
                    .unwrap_or_default()
                    .keys()
                    .cloned()
                    .collect();
                crate::worktree::generate_worktree_name(&existing)
            });

            let config = crate::worktree::WorktreeConfig {
                task_name: branch_name.clone(),
                base_repo: path.clone(),
                branch: Some(branch_name),
                create_branch: true,
            };

            let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
                std::path::Path::new(&path),
                &state.worktrees_dir,
            );
            match crate::worktree::create_worktree_internal(&worktrees_dir, &config, base_ref) {
                Ok(wt) => {
                    state.invalidate_repo_caches(&path);
                    let wt_path = wt.path.to_string_lossy().to_string();
                    let mut response = serde_json::json!({
                        "worktree_path": wt_path,
                        "branch": wt.branch,
                    });
                    // Optionally spawn a PTY session in the new worktree
                    if args["spawn_session"].as_bool().unwrap_or(false) {
                        match create_session_in_dir(state, &wt_path) {
                            Ok(sid) => { response["session_id"] = serde_json::json!(sid); }
                            Err(e) => { response["session_error"] = serde_json::json!(e); }
                        }
                    }
                    response
                }
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "remove" => {
            let path = match require_path(args, "remove") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let branch = match args["branch"].as_str() {
                Some(b) => b.to_string(),
                None => return serde_json::json!({"error": "Action 'remove' requires 'branch' parameter"}),
            };
            let archive = crate::worktree::resolve_archive_script(&path);
            match crate::worktree::remove_worktree_by_branch(&path, &branch, true, archive.as_deref()) {
                Ok(()) => {
                    state.invalidate_repo_caches(&path);
                    serde_json::json!({"ok": true})
                }
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'worktree'. Available: {}", other, WORKTREE_ACTIONS
        )}),
    }
}

fn handle_agent(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value) -> serde_json::Value {
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
            // Agent spawning is restricted to localhost — matches the HTTP route guard in agent_routes.rs
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "Agent spawning is restricted to localhost connections"});
            }
            let prompt = match args["prompt"].as_str() {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"error": "Action 'spawn' requires 'prompt'"}),
            };
            if state.sessions.len() >= MAX_CONCURRENT_SESSIONS {
                return serde_json::json!({"error": "Max concurrent sessions reached"});
            }

            // Resolve agent binary
            let binary_path = if let Some(path) = args["binary_path"].as_str() {
                let p = std::path::Path::new(path);
                if !p.is_absolute() {
                    return serde_json::json!({"error": "binary_path must be an absolute path"});
                }
                if !p.is_file() {
                    return serde_json::json!({"error": "binary_path does not point to an existing file"});
                }
                path.to_string()
            } else {
                let agent_type = args["agent_type"].as_str().unwrap_or("claude");
                let detection = crate::agent::detect_agent_binary(agent_type.to_string());
                match detection.path {
                    Some(p) => p,
                    None => return serde_json::json!({"error": format!("Agent binary '{}' not found", agent_type)}),
                }
            };

            let rows = args["rows"].as_u64().unwrap_or(24) as u16;
            let cols = args["cols"].as_u64().unwrap_or(80) as u16;
            if let Err(msg) = super::validate_terminal_size(rows, cols) {
                return serde_json::json!({"error": msg});
            }

            let session_id = Uuid::new_v4().to_string();
            let pty_system = native_pty_system();
            let pair = match pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }) {
                Ok(p) => p,
                Err(e) => return serde_json::json!({"error": format!("Failed to open PTY: {}", e)}),
            };

            let mut cmd = CommandBuilder::new(&binary_path);
            if let Some(raw_args) = args.get("args").and_then(|a| a.as_array()) {
                for arg in raw_args {
                    if let Some(s) = arg.as_str() { cmd.arg(s); }
                }
            } else {
                if args["print_mode"].as_bool().unwrap_or(false) {
                    cmd.arg("--print");
                }
                if let Some(format) = args["output_format"].as_str() {
                    cmd.arg("--output-format");
                    cmd.arg(format);
                }
                if let Some(model) = args["model"].as_str() {
                    cmd.arg("--model");
                    cmd.arg(model);
                }
                cmd.arg(&prompt);
            }
            if let Some(cwd) = args["cwd"].as_str() { cmd.cwd(cwd); }

            let child = match pair.slave.spawn_command(cmd) {
                Ok(c) => c,
                Err(e) => return serde_json::json!({"error": format!("Failed to spawn agent: {}", e)}),
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
                writer, master: pair.master, _child: child, paused: paused.clone(), worktree: None,
                cwd: args["cwd"].as_str().map(|s| s.to_string()), display_name: None,
            }));
            state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
            state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
            state.output_buffers.insert(session_id.clone(), Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)));
            state.vt_log_buffers.insert(session_id.clone(), Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)));
            state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

            // Broadcast session-created to SSE/WebSocket consumers
            let cwd_str = args["cwd"].as_str().map(|s| s.to_string());
            let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
                session_id: session_id.clone(),
                cwd: cwd_str.clone(),
            });

            let app_handle = state.app_handle.read().clone();
            if let Some(ref app) = app_handle {
                // Notify Tauri frontend so it creates a tab
                let _ = app.emit("session-created", serde_json::json!({
                    "session_id": session_id,
                    "cwd": cwd_str,
                }));
                spawn_reader_thread(reader, paused, session_id.clone(), app.clone(), state.clone());
            } else {
                spawn_headless_reader_thread(reader, paused, session_id.clone(), state.clone());
            }

            serde_json::json!({"session_id": session_id})
        }
        "stats" => {
            let stats = state.orchestrator_stats();
            to_json_or_error(stats)
        }
        "metrics" => {
            let metrics = state.session_metrics_json();
            to_json_or_error(metrics)
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
            let mut json = to_json_or_error(config);
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
                    let old_disabled = state.config.read().disabled_native_tools.clone();
                    *state.config.write() = config.clone();
                    if old_disabled != config.disabled_native_tools {
                        let _ = state.mcp_tools_changed.send(());
                    }
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

fn handle_workspace(_state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "workspace", WORKSPACE_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" => {
            let repo_data = crate::config::load_repositories();
            let repos = repo_data.get("repos").cloned().unwrap_or(serde_json::json!({}));
            let repo_order = repo_data.get("repoOrder")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let groups = repo_data.get("groups").cloned().unwrap_or(serde_json::json!({}));

            // Build group membership lookup: repo_path → group name
            let mut repo_group: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            if let Some(groups_obj) = groups.as_object() {
                for (_gid, group) in groups_obj {
                    let group_name = group["name"].as_str().unwrap_or("").to_string();
                    if let Some(order) = group["repoOrder"].as_array() {
                        for path_val in order {
                            if let Some(path) = path_val.as_str() {
                                repo_group.insert(path.to_string(), group_name.clone());
                            }
                        }
                    }
                }
            }

            let mut results: Vec<serde_json::Value> = Vec::new();
            for path_val in &repo_order {
                let path = match path_val.as_str() {
                    Some(p) => p,
                    None => continue,
                };
                let repo_entry = repos.get(path);
                let display_name = repo_entry
                    .and_then(|r| r["displayName"].as_str())
                    .unwrap_or("")
                    .to_string();

                let info = crate::git::get_repo_info_impl(path);
                let worktrees = crate::worktree::get_worktree_paths(path.to_string())
                    .unwrap_or_default();

                let mut entry = serde_json::json!({
                    "path": path,
                    "name": if display_name.is_empty() { &info.name } else { &display_name },
                    "branch": info.branch,
                    "status": info.status,
                    "is_git_repo": info.is_git_repo,
                });
                // Include ahead/behind for git repos with remotes
                if info.is_git_repo {
                    let gh = crate::github::get_github_status_impl(path);
                    if gh.has_remote {
                        entry["ahead"] = serde_json::json!(gh.ahead);
                        entry["behind"] = serde_json::json!(gh.behind);
                    }
                }
                if let Some(group_name) = repo_group.get(path) {
                    entry["group"] = serde_json::json!(group_name);
                }
                if !worktrees.is_empty() {
                    entry["worktrees"] = to_json_or_error(&worktrees);
                }
                results.push(entry);
            }
            serde_json::json!(results)
        }
        "active" => {
            let repo_data = crate::config::load_repositories();
            let active_path = match repo_data.get("activeRepoPath").and_then(|v| v.as_str()) {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"active": null}),
            };

            let info = crate::git::get_repo_info_impl(&active_path);

            // Find group membership
            let groups = repo_data.get("groups").cloned().unwrap_or(serde_json::json!({}));
            let mut group_name: Option<String> = None;
            if let Some(groups_obj) = groups.as_object() {
                for (_gid, group) in groups_obj {
                    if let Some(order) = group["repoOrder"].as_array()
                        && order.iter().any(|p| p.as_str() == Some(&active_path))
                    {
                        group_name = group["name"].as_str().map(|s| s.to_string());
                        break;
                    }
                }
            }

            let mut result = serde_json::json!({
                "path": active_path,
                "name": info.name,
                "branch": info.branch,
                "status": info.status,
            });
            if let Some(gn) = group_name {
                result["group"] = serde_json::json!(gn);
            }
            result
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'workspace'. Available: {}", other, WORKSPACE_ACTIONS
        )}),
    }
}

fn handle_notify(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "notify", NOTIFY_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "toast" => {
            let title = match args["title"].as_str() {
                Some(t) => t.to_string(),
                None => return serde_json::json!({"error": "Action 'toast' requires 'title'"}),
            };
            let message = args["message"].as_str().map(|s| s.to_string());
            let level = args["level"].as_str().unwrap_or("info");
            let level = match level {
                "info" | "warn" | "error" => level.to_string(),
                other => return serde_json::json!({"error": format!(
                    "Invalid level '{}'. Must be: info, warn, error", other
                )}),
            };
            let _ = state.event_bus.send(crate::state::AppEvent::McpToast {
                title,
                message,
                level,
            });
            serde_json::json!({"ok": true})
        }
        "confirm" => {
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "Action 'confirm' is restricted to localhost connections"});
            }
            let title = match args["title"].as_str() {
                Some(t) => t.to_string(),
                None => return serde_json::json!({"error": "Action 'confirm' requires 'title'"}),
            };
            let message = args["message"].as_str().unwrap_or("").to_string();

            let app_handle = state.app_handle.read();
            let handle = match app_handle.as_ref() {
                Some(h) => h,
                None => return serde_json::json!({"error": "App handle not available (headless mode)"}),
            };

            use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
            let confirmed = handle.dialog()
                .message(&message)
                .title(&title)
                .buttons(MessageDialogButtons::OkCancel)
                .blocking_show();

            serde_json::json!({"confirmed": confirmed})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'notify'. Available: {}", other, NOTIFY_ACTIONS
        )}),
    }
}

// ---------------------------------------------------------------------------
// Streamable HTTP transport (MCP spec 2025-03-26)
// Single /mcp endpoint — POST for JSON-RPC, GET for SSE notifications, DELETE ends session
// ---------------------------------------------------------------------------

const MCP_SESSION_HEADER: &str = "mcp-session-id";

/// POST /mcp — Handle all MCP JSON-RPC requests via Streamable HTTP
pub(super) async fn mcp_post(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let method = body["method"].as_str().unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);

    match method {
        "initialize" => {
            let session_id = Uuid::new_v4().to_string();
            state.mcp_sessions.insert(session_id.clone(), std::time::Instant::now());
            let client_name = body["params"]["clientInfo"]["name"].as_str();
            let instructions = build_mcp_instructions(&state, client_name);

            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "tuicommander",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": instructions
                }
            });

            (
                StatusCode::OK,
                [(MCP_SESSION_HEADER, session_id)],
                Json(response),
            ).into_response()
        }

        "notifications/initialized" => {
            StatusCode::ACCEPTED.into_response()
        }

        "tools/list" => {
            let tools = merged_tool_definitions(&state);
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools }
            });
            let mut resp = Json(response).into_response();
            if let Some(sid) = headers.get(MCP_SESSION_HEADER).and_then(|v| v.to_str().ok())
                && let Ok(val) = sid.parse()
            {
                resp.headers_mut().insert(MCP_SESSION_HEADER, val);
            }
            resp
        }

        "tools/call" => {
            // Validate that the caller has an active MCP session (established via initialize)
            let session_valid = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|sid| state.mcp_sessions.contains_key(sid))
                .unwrap_or(false);
            if !session_valid {
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32600, "message": "Valid mcp-session-id header required. Call initialize first." }
                });
                return Json(response).into_response();
            }

            let params = body.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool_name = params["name"].as_str().unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

            // Route upstream-prefixed tools ({upstream}__{tool}) via the proxy registry.
            // Native tools (no "__") go through the sync handler via spawn_blocking.
            let (result, is_error) = if tool_name.contains("__") {
                match state.mcp_upstream_registry.proxy_tool_call(&tool_name, args.clone()).await {
                    Ok(v) => (v, false),
                    Err(e) => (serde_json::json!({"error": e}), true),
                }
            } else {
                let result = handle_mcp_tool_call(&state, addr, &tool_name, &args).await;
                let is_error = result.get("error").is_some();
                (result, is_error)
            };
            let text = serde_json::to_string_pretty(&result).unwrap_or_default();
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": text }],
                    "isError": is_error
                }
            });
            let mut resp = Json(response).into_response();
            if let Some(sid) = headers.get(MCP_SESSION_HEADER).and_then(|v| v.to_str().ok())
                && let Ok(val) = sid.parse()
            {
                resp.headers_mut().insert(MCP_SESSION_HEADER, val);
            }
            resp
        }

        other => {
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {}", other) }
            });
            Json(response).into_response()
        }
    }
}

/// GET /mcp — SSE stream for MCP server→client notifications (tools/list_changed).
/// Requires a valid `mcp-session-id` header (established via POST /mcp initialize).
pub(super) async fn mcp_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate MCP session
    let session_valid = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|sid| state.mcp_sessions.contains_key(sid))
        .unwrap_or(false);
    if !session_valid {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let mut rx = state.mcp_tools_changed.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(()) => {
                    let notification = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/tools/list_changed"
                    });
                    yield Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default()
                            .data(serde_json::to_string(&notification).unwrap_or_default())
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    axum::response::sse::Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}

/// GET /mcp/instructions — Returns dynamic server instructions for the bridge binary
pub(super) async fn mcp_instructions_http(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(serde_json::json!({"instructions": build_mcp_instructions(&state, None)}))
}

/// DELETE /mcp — End an MCP session
pub(super) async fn mcp_delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(sid) = headers.get(MCP_SESSION_HEADER).and_then(|v| v.to_str().ok()) {
        state.mcp_sessions.remove(sid);
    }
    StatusCode::OK
}

// Re-export for tests — these need to be public enough for sibling test module
#[cfg(test)]
pub(crate) fn test_mcp_tool_definitions() -> serde_json::Value {
    native_tool_definitions()
}
#[cfg(test)]
pub(crate) fn test_translate_special_key(key: &str) -> Option<&'static str> {
    translate_special_key(key)
}
#[cfg(test)]
pub(crate) fn test_validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    validate_mcp_repo_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            sessions: dashmap::DashMap::new(),
            worktrees_dir: std::env::temp_dir().join("test-worktrees"),
            metrics: crate::SessionMetrics::new(),
            output_buffers: dashmap::DashMap::new(),
            mcp_sessions: dashmap::DashMap::new(),
            ws_clients: dashmap::DashMap::new(),
            config: parking_lot::RwLock::new(crate::config::AppConfig::default()),
            git_cache: crate::state::GitCacheState::new(),
            head_watchers: dashmap::DashMap::new(),
            repo_watchers: dashmap::DashMap::new(),
            dir_watchers: dashmap::DashMap::new(),
            http_client: reqwest::Client::new(),
            github_token: parking_lot::RwLock::new(None),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: parking_lot::RwLock::new(uuid::Uuid::new_v4().to_string()),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: dashmap::DashMap::new(),
            vt_log_buffers: dashmap::DashMap::new(),
            kitty_states: dashmap::DashMap::new(),
            input_buffers: dashmap::DashMap::new(),
            last_prompts: dashmap::DashMap::new(),
            silence_states: dashmap::DashMap::new(),
            claude_usage_cache: parking_lot::Mutex::new(std::collections::HashMap::new()),
            log_buffer: std::sync::Arc::new(parking_lot::Mutex::new(crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY))),
            event_bus: tokio::sync::broadcast::channel(256).0,
            event_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            session_states: dashmap::DashMap::new(),
            mcp_upstream_registry: std::sync::Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            mcp_tools_changed: tokio::sync::broadcast::channel(16).0,
            slash_mode: dashmap::DashMap::new(),
            last_output_ms: dashmap::DashMap::new(),
            shell_states: dashmap::DashMap::new(),
            loaded_plugins: dashmap::DashMap::new(),
            relay: crate::state::RelayState::new(),
        })
    }

    #[tokio::test]
    async fn session_create_emits_event_bus_session_created() {
        let state = test_state();
        let mut rx = state.event_bus.subscribe();

        let args = serde_json::json!({"action": "create"});
        let result = handle_session(&state, &args);

        // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
        if result.get("error").is_some() {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }

        // Session should have been created successfully
        assert!(result.get("session_id").is_some(), "Expected session_id in result: {result}");

        // event_bus should have received SessionCreated
        let event = rx.try_recv().expect("Expected SessionCreated event on event_bus");
        match event {
            crate::state::AppEvent::SessionCreated { session_id, .. } => {
                assert_eq!(session_id, result["session_id"].as_str().unwrap());
            }
            other => panic!("Expected SessionCreated, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn session_create_registers_vt_log_and_last_output() {
        let state = test_state();
        let args = serde_json::json!({"action": "create"});
        let result = handle_session(&state, &args);

        // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
        if result.get("error").is_some() {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }

        let sid = result["session_id"].as_str().unwrap();

        assert!(state.vt_log_buffers.contains_key(sid), "vt_log_buffers should contain session");
        assert!(state.last_output_ms.contains_key(sid), "last_output_ms should contain session");
        assert!(state.output_buffers.contains_key(sid), "output_buffers should contain session");
    }
}
