use crate::pty::{build_shell_command, resolve_shell, spawn_headless_reader_thread, spawn_reader_thread};
use crate::{AppState, OutputRingBuffer, PtySession, MAX_CONCURRENT_SESSIONS};
use crate::state::OUTPUT_RING_BUFFER_CAPACITY;
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
fn build_mcp_instructions(state: &Arc<AppState>) -> String {
    let mut out = String::with_capacity(2048);

    out.push_str("# TUICommander MCP Server\n\n");
    out.push_str("You are connected to TUICommander, a terminal session orchestrator. ");
    out.push_str("You can manage PTY terminals, query git repos, spawn AI agents, and read/write app config.\n\n");

    // Tools overview
    out.push_str("## Tools\n\n");
    out.push_str("All tools use an `action` parameter to select the operation.\n\n");
    out.push_str("| Tool | Actions |\n|---|---|\n");
    out.push_str("| `session` | list, create, input, output, resize, close, pause, resume |\n");
    out.push_str("| `git` | info, diff, files, branches, github, prs |\n");
    out.push_str("| `agent` | detect, spawn, stats, metrics |\n");
    out.push_str("| `config` | get, save |\n");
    out.push_str("| `workspace` | list, active |\n");
    out.push_str("| `notify` | toast, confirm |\n");
    out.push_str("| `plugin_dev_guide` | *(no action — returns plugin authoring reference)* |\n\n");

    out.push_str("**Workflow:** Call `session action=list` first to discover active sessions. ");
    out.push_str("Use `session action=output` to read terminal content and `session action=input` to type. ");
    out.push_str("For git queries, pass the repo `path` (absolute).\n\n");

    // Managed repositories
    let repo_settings = crate::config::load_repo_settings();
    if !repo_settings.repos.is_empty() {
        out.push_str("## Managed Repositories\n\n");
        out.push_str("| Name | Path |\n|---|---|\n");
        let mut repos: Vec<_> = repo_settings.repos.iter().collect();
        repos.sort_by_key(|(path, _)| path.to_string());
        for (path, entry) in &repos {
            let name = if entry.display_name.is_empty() {
                path.rsplit('/').next().unwrap_or(path)
            } else {
                &entry.display_name
            };
            out.push_str(&format!("| {} | `{}` |\n", name, path));
        }
        out.push('\n');
    }

    // Active PTY sessions
    let sessions: Vec<_> = state.sessions.iter().map(|entry| {
        let id = entry.key().clone();
        let session = entry.value().lock();
        (id, session.cwd.clone(), session.worktree.as_ref().and_then(|w| w.branch.clone()))
    }).collect();

    if !sessions.is_empty() {
        out.push_str("## Active Sessions\n\n");
        out.push_str("| Session ID | CWD | Branch |\n|---|---|---|\n");
        for (id, cwd, branch) in &sessions {
            out.push_str(&format!("| `{}` | {} | {} |\n",
                &id[..8.min(id.len())],
                cwd.as_deref().unwrap_or("—"),
                branch.as_deref().unwrap_or("—"),
            ));
        }
        out.push('\n');
    }

    // Intent declaration protocol
    out.push_str("## Intent Declaration\n");
    out.push_str("At the start of each distinct work phase, emit on its own line:\n");
    out.push_str("[[intent: <action, present tense, <60 chars>]]\n");
    out.push_str("Examples: `Reading auth module for token flow` · `Writing parser unit tests` · `Debugging login redirect`\n");

    out
}

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path)
        .map_err(|msg| serde_json::json!({"error": msg}))
}

const SESSION_ACTIONS: &str = "list, create, input, output, resize, close, pause, resume";
const GIT_ACTIONS: &str = "info, diff, files, branches, github, prs";
const AGENT_ACTIONS: &str = "detect, spawn, stats, metrics";
const CONFIG_ACTIONS: &str = "get, save";
const WORKSPACE_ACTIONS: &str = "list, active";
const NOTIFY_ACTIONS: &str = "toast, confirm";

/// MCP tool definitions — 5 meta-commands mirroring tui_mcp_bridge
fn native_tool_definitions() -> serde_json::Value {
    let defs = serde_json::json!([
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
                "limit": { "type": "integer", "description": "Bytes to read, default 8192 (action=output)" },
                "format": { "type": "string", "description": "Output format: ANSI escape codes are stripped by default; pass 'raw' to preserve them (action=output)" }
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
fn handle_mcp_tool_call(state: &Arc<AppState>, addr: SocketAddr, name: &str, args: &serde_json::Value) -> serde_json::Value {
    match name {
        "session" => handle_session(state, args),
        "git" => handle_git(state, args),
        "agent" => handle_agent(state, addr, args),
        "config" => handle_config(state, addr, args),
        "workspace" => handle_workspace(state, args),
        "notify" => handle_notify(state, addr, args),
        "plugin_dev_guide" => {
            serde_json::json!({"content": super::plugin_docs::PLUGIN_DOCS})
        }
        _ => serde_json::json!({"error": format!(
            "Unknown tool '{}'. Available: session, git, agent, config, workspace, notify, plugin_dev_guide", name
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
            let raw = String::from_utf8_lossy(&bytes).to_string();
            // Strip ANSI by default — AI agents don't need escape codes.
            // Pass format="raw" to preserve them (e.g. for terminal rendering).
            let data = if args["format"].as_str() == Some("raw") {
                raw
            } else {
                crate::output_parser::strip_ansi(&raw)
            };
            serde_json::json!({"data": data, "data_length": data.len(), "total_written": total_written})
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
            to_json_or_error(info)
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
                Ok(files) => to_json_or_error(files),
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
                Ok(branches) => to_json_or_error(branches),
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
            to_json_or_error(status)
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
            to_json_or_error(statuses)
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'git'. Available: {}", other, GIT_ACTIONS
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
                cwd: args["cwd"].as_str().map(|s| s.to_string()),
            }));
            state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
            state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
            state.output_buffers.insert(session_id.clone(), Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)));

            let app_handle = state.app_handle.read().clone();
            if let Some(ref app) = app_handle {
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
                    if let Some(order) = group["repoOrder"].as_array() {
                        if order.iter().any(|p| p.as_str() == Some(&active_path)) {
                            group_name = group["name"].as_str().map(|s| s.to_string());
                            break;
                        }
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
            let instructions = build_mcp_instructions(&state);

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
                let state_clone = state.clone();
                let result = match tokio::task::spawn_blocking(move || {
                    handle_mcp_tool_call(&state_clone, addr, &tool_name, &args)
                }).await {
                    Ok(r) => r,
                    Err(e) => serde_json::json!({"error": format!("Task failed: {e}")}),
                };
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
    Json(serde_json::json!({"instructions": build_mcp_instructions(&state)}))
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
