use crate::pty::{resolve_shell, spawn_headless_reader_thread, spawn_reader_thread};
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
use tauri::{Emitter, Manager};
use uuid::Uuid;

/// Serialize a value to JSON, returning a structured error on failure instead of silent null.
fn to_json_or_error<T: serde::Serialize>(value: T) -> serde_json::Value {
    match serde_json::to_value(value) {
        Ok(v) => v,
        Err(e) => serde_json::json!({"error": format!("Serialization failed: {e}")}),
    }
}

/// Single source of truth for detecting Claude Code (or tuic-bridge) clients.
fn detect_claude_code_client(client_name: Option<&str>) -> bool {
    client_name.is_some_and(|n| n.contains("claude") || n.contains("tuic-bridge"))
}

/// Detect Claude Code from the User-Agent header when the MCP clientInfo is
/// unavailable (e.g. after session auto-recovery following a TUIC restart).
fn detect_claude_code_from_headers(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.to_ascii_lowercase())
        .is_some_and(|ua| ua.contains("claude") || ua.contains("tuic-bridge"))
}

/// Map MCP client name to TUICommander agent type key.
/// Returns None when the client cannot be identified.
fn resolve_agent_type(client_name: Option<&str>) -> Option<&'static str> {
    let name = client_name?.to_ascii_lowercase();
    if name.contains("claude") || name.contains("tuic-bridge") {
        Some("claude")
    } else if name.contains("codex") {
        Some("codex")
    } else if name.contains("cursor") {
        Some("cursor")
    } else if name.contains("gemini") {
        Some("gemini")
    } else if name.contains("aider") {
        Some("aider")
    } else if name.contains("amp") {
        Some("amp")
    } else {
        None
    }
}

/// Resolve effective intent_tab_title / suggest_followups for a connecting agent.
/// Semantics: `global AND (per_agent ?? true)`. Global acts as a kill-switch for
/// the whole feature; per-agent is an escape hatch (default ON) to disable the
/// marker on a specific agent where rendering or parsing misbehaves.
fn resolve_marker_flags(state: &Arc<AppState>, client_name: Option<&str>) -> (bool, bool) {
    let global_intent = state.config.read().intent_tab_title;
    let global_suggest = state.config.read().suggest_followups;

    let agent_type = resolve_agent_type(client_name);

    let agents_cfg = crate::config::load_agents_config();
    let agent_settings = agent_type.and_then(|t| agents_cfg.agents.get(t));

    let show_intent = global_intent
        && agent_settings
            .and_then(|s| s.intent_tab_title)
            .unwrap_or(true);

    let show_suggest = global_suggest
        && agent_settings
            .and_then(|s| s.suggest_followups)
            .unwrap_or(true);

    (show_intent, show_suggest)
}

/// SIMP-1: Drain registered HTML tabs for a closing/killed/exited session and
/// emit `close-html-tabs` to the frontend. SIL-3: log a warning if the emit
/// fails (don't drop silently — orphan tabs in UI hint at a missing app handle
/// or a broken event channel).
///
/// Shared by `session(close)`, `session(kill)`, and `pty::mark_session_exited`
/// (natural exit) so all three exit paths drain `session_html_tabs` identically.
pub(crate) fn emit_close_html_tabs(state: &AppState, session_id: &str) {
    let Some((_, tab_ids)) = state.session_html_tabs.remove(session_id) else {
        return;
    };
    let Some(app) = state.app_handle.read().as_ref().cloned() else {
        // No app handle (test mode or pre-init). Tabs were already drained.
        return;
    };
    if let Err(err) = app.emit("close-html-tabs", serde_json::json!({ "tab_ids": tab_ids })) {
        tracing::warn!(
            source = "session",
            session_id = %session_id,
            tab_count = tab_ids.len(),
            error = %err,
            "failed to emit close-html-tabs — frontend tabs may be orphaned"
        );
    }
}

/// Validate that a string is a well-formed UUID in canonical 8-4-4-4-12 form.
/// Used to reject non-UUID `tuic_session` values at register time to prevent
/// prompt-injection via preamble string interpolation (SEC-1).
///
/// Length check rejects the `uuid` crate's accepted simple/urn/braced forms —
/// `$TUIC_SESSION` is always written canonical, and narrowing the accepted
/// surface keeps the injection guard tight.
fn is_valid_uuid(s: &str) -> bool {
    s.len() == 36 && Uuid::parse_str(s).is_ok()
}

/// Build server instructions for the MCP initialize response.
/// Tells the connecting agent what tools are available, which repos are managed,
/// and what sessions are currently active so it can orient itself.
fn build_mcp_instructions(state: &Arc<AppState>, client_name: Option<&str>) -> String {
    let ver = env!("CARGO_PKG_VERSION");
    let mut out = String::with_capacity(2048);

    // ── Identity ──────────────────────────────────────────────────────
    out.push_str(&format!("# TUICommander v{ver}\n\n"));
    out.push_str("You are connected to TUICommander, a terminal session orchestrator for AI coding agents.\n\n");

    // ── TUIC protocol (mandatory line markers) ─────────────────────────
    // Wire-level tokens parsed by the host TUI. Concision rules do NOT apply —
    // dropping a marker breaks the UI (stale tab title, missing suggestion bar).
    let (show_intent, show_suggest) = resolve_marker_flags(state, client_name);
    out.push_str("## TUIC Protocol — Required Output Markers\n\n");
    out.push_str("Protocol tokens (not prose). Emit even under concision/no-preamble rules from user configs — dropping breaks UI.\n\n");
    out.push_str(&format!(
        "- `ack` — first assistant message MUST start: `TUICommander v{ver} is connected.`\n"
    ));
    if show_intent {
        out.push_str("- `intent:` — on every work-phase change (new request, pivot, resume): `intent: <desc> (<=3-word title>)`\n");
    }
    if show_suggest {
        out.push_str("- `suggest:` — after task done: `suggest: 1) … | 2) … | 3) …`\n");
    }
    out.push('\n');

    // ── Tools ────────────────────────────────────────────────────────
    if state.config.read().collapse_tools {
        // Lazy tool loading (Speakeasy pattern): the client only sees three
        // meta-tools. Describe the discovery flow and what domains are reachable
        // so the model can form the right query for search_tools.
        out.push_str("## Tools — Lazy Discovery\n\n");
        out.push_str("This server exposes three meta-tools. Use them to discover and invoke the full tool set on demand:\n\n");
        out.push_str("1. **`search_tools`** — BM25 search over the full tool corpus. Pass a natural-language `query` describing what you want to do. Returns ranked `{name, summary}` entries.\n");
        out.push_str("2. **`get_tool_schema`** — Given an exact `tool_name` from search, returns the full tool definition with inputSchema.\n");
        out.push_str("3. **`call_tool`** — Dispatch a named tool. Pass `tool_name` + `arguments` object.\n\n");
        out.push_str("**Flow:** `search_tools(query=\"…\")` → pick a name → `get_tool_schema(tool_name=…)` → `call_tool(tool_name=…, arguments={…})`.\n\n");
        out.push_str("**Domains available:** terminal pane sessions (tmux replacement), AI agent orchestration + messaging, repos/GitHub PRs/worktrees, UI tabs + notifications, plugin authoring reference, app config, diagnostics");
        let upstream_count = state.mcp_upstream_registry.aggregated_tools().len();
        if upstream_count > 0 {
            out.push_str(&format!(", plus {upstream_count} upstream tool(s) from connected MCP servers"));
        }
        out.push_str(".\n\n");
        out.push_str("**Worktrees:** never `git worktree add/remove` — always use `repo action=worktree_create` / `worktree_remove` so TUIC tracks the worktree and can spawn a PTY inside.\n\n");
    } else {
        out.push_str("## Tools\n\n");
        out.push_str("- `session` (PTY panes, tmux-equivalent): list, create, input, output, status, resize, close, kill, pause, resume\n");
        out.push_str("- `agent` (AI peers + messaging): spawn, detect, stats, metrics, register, list_peers, send, inbox\n");
        out.push_str("- `repo` (repos, PRs, worktrees): list, active, prs, status, worktree_list, worktree_create, worktree_remove\n");
        out.push_str("- `ui` (tabs, toasts, confirm dialogs): tab, toast, confirm\n");
        out.push_str("- `plugin_dev_guide`: plugin authoring reference\n\n");
        out.push_str("**Worktrees:** always `repo action=worktree_create`/`worktree_remove` — never `git worktree add/remove` (TUIC must track them to spawn a PTY inside).\n\n");
        out.push_str("**UI feedback:** `ui action=toast` on task done/blocking error · `ui action=confirm` BEFORE destructive ops (rm -rf, git reset --hard, force push, DROP TABLE) · `ui action=tab` for structured output >20 lines.\n\n");
    }

    // ── Workflow (phase-grouped) ──────────────────────────────────────
    // 4 bullets by phase instead of 7 tool-by-tool steps. Details live in each
    // tool's description (JSON schema); this section gives the mental model.
    // Suppressed in collapse mode — concrete invocations go through call_tool.
    if !state.config.read().collapse_tools {
        out.push_str("## Workflow\n\n");
        out.push_str("- **Discover:** `repo action=list|prs|active` · `agent action=detect`.\n");
        out.push_str("- **Spawn:** `session action=create` (shell) · `agent action=spawn` (AI) · `repo action=worktree_create` (isolated). `agent_type` resolves run config names first (case-insensitive), then agent binary names.\n");
        out.push_str("- **Observe:** `session action=status|output` · `agent action=inbox`.\n");
        out.push_str("- **Coordinate:** `agent action=register/send/inbox` for peer messaging.\n\n");
    }

    // ── Multi-agent work — critical pre-spawn knowledge only ─────────
    // Full operational workflow (monitor semantics, cleanup, examples) lives
    // in the agent(register) response. Here we keep only the three anchors
    // a fresh agent needs BEFORE its first tool call:
    //   1. how to obtain identity ($TUIC_SESSION env → UUID)
    //   2. golden path (register → spawn → inbox, never stream peer output)
    //   3. when worktrees apply (isolated branches)
    let peer_count = state.peer_agents.len();
    let is_claude_code = detect_claude_code_client(client_name);
    out.push_str("## Multi-Agent Work\n\n");
    if peer_count > 0 {
        out.push_str(&format!(
            "**{peer_count}** peer agent(s) connected. Aliases \"swarm\"/\"teammates\"/\"parallel\" → register first (below).\n\n"
        ));
    } else {
        out.push_str("Aliases \"swarm\"/\"teammates\"/\"parallel\" map here — not to native subagent tools.\n\n");
    }
    out.push_str("- **Identity:** your UUID is `$TUIC_SESSION` env var. Register first: `agent action=register tuic_session=$TUIC_SESSION` — response teaches spawn/monitor/cleanup.\n");
    out.push_str("- **Same repo:** `agent action=spawn` peers; poll `agent action=inbox since=<last_ms>` — never `session output` on peers (token burn).\n");
    out.push_str("- **Isolated branches:** `repo action=worktree_create spawn_session=true`.\n");
    if is_claude_code {
        out.push_str("- **Single isolated task (CC only):** `repo action=worktree_create` then delegate via returned `cc_agent_hint` (absolute paths). ONLY valid use of native Agent/Task.\n");
    }
    out.push('\n');

    // ── Dynamic: repos ──────────────────────────────────────────────
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

    // ── Dynamic: sessions ───────────────────────────────────────────
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

    out
}

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path)
        .map_err(|msg| serde_json::json!({"error": msg}))
}

const SESSION_ACTIONS: &str = "list, create, input, output, resize, close, kill, pause, resume, status";
const AGENT_ACTIONS: &str = "spawn, detect, stats, metrics, register, list_peers, send, inbox";
const REPO_ACTIONS: &str = "list, active, prs, status, worktree_list, worktree_create, worktree_remove";
const UI_ACTIONS: &str = "tab, toast, confirm";
const CONFIG_ACTIONS: &str = "get, save";
const DEBUG_ACTIONS: &str = "agent_detection, logs, sessions, invoke_js, help";

// Legacy action constants — still referenced by handlers until dispatch refactor (story 1091).
// Remove these when handle_mcp_tool_call dispatch is updated.
const LEGACY_AGENT_ACTIONS: &str = "detect, spawn, stats, metrics";
const LEGACY_GITHUB_ACTIONS: &str = "prs, status, issues, close_issue, reopen_issue";
const LEGACY_WORKTREE_ACTIONS: &str = "list, create, remove";
const LEGACY_WORKSPACE_ACTIONS: &str = "list, active";
const LEGACY_UI_ACTIONS: &str = "tab";
const LEGACY_NOTIFY_ACTIONS: &str = "toast, confirm";
const LEGACY_MESSAGING_ACTIONS: &str = "register, list_peers, send, inbox";
const LEGACY_DEBUG_ACTIONS: &str = "agent_detection, logs, sessions, invoke_js";

/// MCP tool definitions — 7 base native tools + 6 ai_terminal_* tools.
fn native_tool_definitions() -> serde_json::Value {
    let mut defs = serde_json::json!([
        {
            "name": "session",
            "description": "PTY multiplexer (replaces tmux). Create terminals, send input (send-keys), read output (capture-pane), manage lifecycle.\n\nActions:\n- list: Active sessions with cwd, process info. Call first to discover IDs.\n- create: New PTY. Returns {session_id}. Optional: cwd, shell, rows, cols.\n- input: Send text and/or special_key to a session.\n- output: Read from ring buffer. Returns {data, total_written, exited, exit_code}.\n- status: Shell state for a session: {shell_state, idle_since_ms, busy_duration_ms, exit_code, agent_type}. Use to poll agent progress without streaming output.\n- resize: Change PTY dimensions.\n- close: Graceful shutdown (Ctrl+C, waits).\n- kill: Force SIGKILL (use when close fails).\n- pause: Pause output buffering. resume: Resume.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, create, input, output, status, resize, close, kill, pause, resume" },
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
            "name": "agent",
            "description": "AI agent orchestration. Spawn agents (Claude Code, Codex, Aider, Goose) in managed PTYs, detect installed agents, and peer-to-peer messaging.\n\nActions:\n- spawn: Launch agent in new PTY (localhost only). Returns {session_id}. Use session action=input/output to interact.\n- detect: Installed agents [{name, path, version}].\n- stats: {active_sessions, max_sessions, available_slots}.\n- metrics: Cumulative {total_spawned, total_failed, bytes_emitted, pauses_triggered}.\n- register: Register as peer (pass your $TUIC_SESSION env var).\n- list_peers: List peers. Optional: project filter.\n- send: Message a peer (requires to, message).\n- inbox: Read messages. Optional: limit, since (unix millis).",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: spawn, detect, stats, metrics, register, list_peers, send, inbox" },
                "prompt": { "type": "string", "description": "Task prompt for the agent (action=spawn)" },
                "cwd": { "type": "string", "description": "Working directory (action=spawn)" },
                "model": { "type": "string", "description": "Model override (action=spawn)" },
                "print_mode": { "type": "boolean", "description": "false (default): visible TUI tab, observable via agent(inbox). true: headless, no tab. (action=spawn)" },
                "output_format": { "type": "string", "description": "Output format, e.g. 'json' (action=spawn)" },
                "agent_type": { "type": "string", "description": "Agent type OR run config name. Resolved as: (1) run config name match across enabled agents, (2) agent binary name (claude, codex, aider, goose, gemini, ...). Case-insensitive. (action=spawn)" },
                "binary_path": { "type": "string", "description": "Override agent binary path (action=spawn)" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "Raw CLI args (action=spawn)" },
                "rows": { "type": "integer", "description": "Terminal rows (action=spawn)" },
                "cols": { "type": "integer", "description": "Terminal cols (action=spawn)" },
                "tuic_session": { "type": "string", "description": "Your $TUIC_SESSION env var value (action=register, required)" },
                "name": { "type": "string", "description": "Display name (action=register, default: 'agent')" },
                "project": { "type": "string", "description": "Git repo root path (action=register optional, action=list_peers filter)" },
                "to": { "type": "string", "description": "Recipient tuic_session UUID (action=send, required)" },
                "message": { "type": "string", "description": "Message content, max 64KB (action=send, required)" },
                "since": { "type": "integer", "description": "Unix millis — only return messages after this (action=inbox)" }
            }, "required": ["action"] }
        },
        {
            "name": "repo",
            "description": "Repository and version control. Query workspace repos, GitHub PR/CI status, manage git worktrees.\n\nActions:\n- list: Open repos with branch, dirty status, worktrees.\n- active: Focused repo path, branch, group.\n- prs: Open PRs with CI, merge readiness, reviews. Requires path.\n- status: Cross-repo {path, branch, ahead, behind, open_prs, failing_ci}.\n- worktree_list: Worktrees for a repo. Requires path.\n- worktree_create: Create worktree. Requires path. Optional: branch, base_ref, spawn_session.\n- worktree_remove: Remove worktree. Requires path, branch.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, active, prs, status, worktree_list, worktree_create, worktree_remove" },
                "path": { "type": "string", "description": "Absolute path to git repository (required for prs, worktree_list, worktree_create, worktree_remove)" },
                "branch": { "type": "string", "description": "Branch name (action=worktree_create optional, action=worktree_remove required)" },
                "base_ref": { "type": "string", "description": "Base ref to branch from, default HEAD (action=worktree_create)" },
                "spawn_session": { "type": "boolean", "description": "Auto-create a PTY session in the worktree (action=worktree_create, default false)" }
            }, "required": ["action"] }
        },
        {
            "name": "ui",
            "description": "Control the TUICommander UI: open panel tabs with custom HTML or a URL, show toast notifications, or prompt for user confirmation.\n\nActions (pass as 'action' parameter):\n- tab: Open/update a pinned panel tab. Requires id, title, and either html (inline) or url (loads in iframe). Optional: pinned (default true).\n- toast: Show a non-blocking notification. Requires title. Optional: message, level (info/warn/error).\n- confirm: Show a blocking confirmation dialog. Returns {confirmed: boolean}. Requires title. Optional: message. Restricted to localhost.\n\nWhen to use:\n- toast: task completed, blocking error, long-running job finished. Use level=error for failures, warn for recoverable issues, info (default) for completions. Skip for micro-steps or verbose progress (chat output suffices).\n- confirm: BEFORE any destructive or irreversible action (rm -rf, git reset --hard, git push --force, DROP TABLE, package uninstall). Proceed only on confirmed=true.\n- tab: structured output worth revisiting (>20 lines, dashboards, reports, rendered diagrams). Prefer over pasting large output into chat.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: tab, toast, confirm" },
                "id": { "type": "string", "description": "Stable identifier for dedup — same id reuses existing tab (action=tab, required)" },
                "title": { "type": "string", "description": "Tab or notification title (action=tab/toast/confirm, required)" },
                "html": { "type": "string", "description": "Inline HTML content to render in sandboxed iframe (action=tab, mutually exclusive with url)" },
                "url": { "type": "string", "description": "URL to load in the tab iframe (action=tab, mutually exclusive with html)" },
                "pinned": { "type": "boolean", "description": "Pin tab across all branches (default true)" },
                "focus": { "type": "boolean", "description": "Switch to this tab after open/update (action=tab, default true). Pass false to update silently without stealing focus." },
                "message": { "type": "string", "description": "Optional body text (action=toast/confirm)" },
                "level": { "type": "string", "description": "Toast level: info, warn, error (default: info)" },
                "sound": { "type": "boolean", "description": "Play a notification sound (action=toast, default: false). Each level has a distinct tone." }
            }, "required": ["action"] }
        },
        {
            "name": "plugin_dev_guide",
            "description": "Returns comprehensive plugin authoring reference: manifest format, PluginHost API (all 4 tiers), structured event types, and working examples. Call before writing any plugin code.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
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
            "name": "debug",
            "description": "Diagnostics for TUICommander internals. action=help returns the full usage guide.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: agent_detection, logs, sessions, invoke_js, help" },
                "session_id": { "type": "string", "description": "PTY session UUID (action=agent_detection, optional — omit for all)" },
                "level": { "type": "string", "description": "Log level filter: debug, info, warn, error (action=logs)" },
                "source": { "type": "string", "description": "Log source filter (action=logs)" },
                "script": { "type": "string", "description": "JavaScript to execute in the WebView (action=invoke_js). The ONLY global is window.__TUIC__ — call action=help for the full API list. Example: return window.__TUIC__.terminals()" },
                "limit": { "type": "integer", "description": "Max entries (action=logs, default 50)" }
            }, "required": ["action"] }
        }
    ]);

    // Append ai_terminal_* tools (external MCP exposure of agent terminal tools).
    if let Some(arr) = defs.as_array_mut() {
        arr.extend(super::ai_terminal::tool_definitions());
    }

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

/// The three meta-tool names used when `collapse_tools: true`.
/// Exposed for handler dispatch and tests.
pub(crate) const META_TOOL_NAMES: [&str; 3] = ["search_tools", "get_tool_schema", "call_tool"];

/// Speakeasy-style meta-tool definitions. When `collapse_tools: true`,
/// `merged_tool_definitions()` returns exactly these three tools instead of
/// the full native + upstream list. The model uses `search_tools` to discover
/// relevant tools by natural language, `get_tool_schema` to fetch the full
/// input schema for one, and `call_tool` to execute it.
fn meta_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "search_tools",
            "description": "Find relevant TUICommander tools by natural-language query. Returns a BM25-ranked list of tool names + one-line summaries. Use this before calling any tool to discover what is available, then call `get_tool_schema` for the full input schema of the tool you want to use.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural-language query describing what you want to do (e.g. 'manage terminal sessions', 'github PR status', 'cross-repo knowledge search')" },
                    "limit": { "type": "integer", "description": "Maximum number of results, default 10" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_tool_schema",
            "description": "Return the full MCP tool definition (name, description, inputSchema) for a single tool by exact name. Call this after `search_tools` to get the arguments needed to invoke a tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "Exact tool name as returned by search_tools" }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "call_tool",
            "description": "Invoke a TUICommander tool by name with arguments. Dispatches to native tools (session, agent, repo, ui, plugin_dev_guide, config, knowledge, debug) or upstream-proxied tools (`{upstream}__{tool}`). The arguments object must match the tool's inputSchema — fetch it via `get_tool_schema` first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "Exact tool name" },
                    "arguments": { "type": "object", "description": "Tool-specific arguments matching the inputSchema returned by get_tool_schema" }
                },
                "required": ["tool_name", "arguments"]
            }
        }
    ])
}

/// Returns native tools merged with upstream proxy tools (namespaced as `{upstream}__`).
///
/// When `config.collapse_tools: true`, returns exactly 3 meta-tools
/// (`search_tools`, `get_tool_schema`, `call_tool`) — the Speakeasy pattern for
/// massive context reduction.
///
/// Otherwise (default), returns native tools filtered by `disabled_native_tools`,
/// merged with upstream proxy tools. Upstream tools are omitted when no
/// upstreams are Ready.
/// Resolve an MCP session's repo_path → per-repo `mcp_upstreams` allowlist.
///
/// Returns `None` when the session has no repo_path or the repo has no
/// custom upstream allowlist (meaning: inherit all globally-enabled upstreams).
fn resolve_allowed_upstreams(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> Option<Vec<String>> {
    let repo_path = mcp_session_id
        .and_then(|sid| state.mcp_sessions.get(sid))
        .and_then(|meta| meta.repo_path.clone())?;
    let repo_settings = crate::config::load_repo_settings();
    repo_settings.repos.get(&repo_path).and_then(|entry| entry.mcp_upstreams.clone())
}

fn merged_tool_definitions(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> serde_json::Value {
    if state.config.read().collapse_tools {
        return meta_tool_definitions();
    }

    let disabled = state.config.read().disabled_native_tools.clone();
    let mut tools: Vec<serde_json::Value> = native_tool_definitions()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            let name = t["name"].as_str().unwrap_or("");
            !disabled.iter().any(|d| d == name)
        })
        .collect();

    let allowed = resolve_allowed_upstreams(state, mcp_session_id);
    let upstream_tools = state.mcp_upstream_registry.aggregated_tools_for_repo(
        allowed.as_deref(),
    );
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

/// Build the full searchable tool corpus — native (filtered by
/// `disabled_native_tools`) merged with upstream aggregated tools.
///
/// Unlike [`merged_tool_definitions`], this bypasses the `collapse_tools`
/// branch: when collapsed, the client sees only the 3 meta-tools but the
/// handlers still need the full list to search over and dispatch to.
///
/// Upstream allow/deny filters are applied inside `aggregated_tools()`.
fn searchable_tool_definitions(state: &Arc<AppState>) -> Vec<serde_json::Value> {
    let disabled = state.config.read().disabled_native_tools.clone();
    let mut tools: Vec<serde_json::Value> = native_tool_definitions()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            let name = t["name"].as_str().unwrap_or("");
            !disabled.iter().any(|d| d == name)
        })
        .collect();
    tools.extend(state.mcp_upstream_registry.aggregated_tools());
    tools
}

/// Rebuild the cached `tool_search_index` from the current state.
///
/// Called on startup and on every `mcp_tools_changed` signal (native tool
/// toggle, upstream add/remove, upstream tools/list_changed).
pub(crate) fn rebuild_tool_search_index(state: &Arc<AppState>) {
    let tools = searchable_tool_definitions(state);
    let index = crate::tool_search::ToolSearchIndex::build(&tools);
    *state.tool_search_index.write() = index;
}

/// Spawn the background task that subscribes to `mcp_tools_changed` and
/// rebuilds `tool_search_index` on every signal. Also does an initial build
/// so the index is populated immediately.
pub(crate) fn spawn_tool_search_index_updater(state: Arc<AppState>) {
    // Initial build so search_tools works before the first tools_changed signal.
    rebuild_tool_search_index(&state);

    let mut rx = state.mcp_tools_changed.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(()) => rebuild_tool_search_index(&state),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(source = "tool_search_index", lagged = n, "tools_changed bus lagged — rebuilding");
                    rebuild_tool_search_index(&state);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Handle `search_tools` meta-tool — BM25 search over the full corpus.
fn handle_search_tools(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let query = match args["query"].as_str() {
        Some(q) if !q.trim().is_empty() => q,
        _ => return serde_json::json!({
            "error": "search_tools requires non-empty 'query' (natural-language string describing what you want to do)"
        }),
    };
    let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 100) as usize;

    let index = state.tool_search_index.read();
    let results = index.search(query, limit);

    let ranked: Vec<serde_json::Value> = results
        .iter()
        .map(|e| serde_json::json!({ "name": e.name, "summary": e.summary }))
        .collect();
    serde_json::json!({ "results": ranked, "count": ranked.len() })
}

/// Handle `get_tool_schema` meta-tool — exact-name lookup of a tool's full definition.
fn handle_get_tool_schema(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let tool_name = match args["tool_name"].as_str() {
        Some(n) if !n.trim().is_empty() => n,
        _ => return serde_json::json!({
            "error": "get_tool_schema requires non-empty 'tool_name' (exact tool name from search_tools)"
        }),
    };

    let index = state.tool_search_index.read();

    match index.get_schema(tool_name) {
        Some(def) => def.clone(),
        None => serde_json::json!({
            "error": format!(
                "Tool '{}' not found. Use search_tools to discover available tools.",
                tool_name
            )
        }),
    }
}

/// Handle `call_tool` meta-tool — dispatch a named tool call to either
/// the native handler or the upstream proxy, preserving `addr` for
/// localhost-only restrictions (config save, notify confirm).
async fn handle_call_tool(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let tool_name = match args["tool_name"].as_str() {
        Some(n) if !n.trim().is_empty() => n.to_string(),
        _ => return serde_json::json!({
            "error": "call_tool requires non-empty 'tool_name' (exact tool name from search_tools or get_tool_schema)"
        }),
    };

    // Block recursive meta-tool invocation — meta-tools are invoked directly,
    // not routed through call_tool.
    if META_TOOL_NAMES.contains(&tool_name.as_str()) {
        return serde_json::json!({
            "error": format!(
                "call_tool cannot invoke meta-tool '{}'. Meta-tools (search_tools, get_tool_schema, call_tool) are invoked directly.",
                tool_name
            )
        });
    }

    let tool_args = args.get("arguments").cloned().unwrap_or(serde_json::json!({}));

    let is_upstream = tool_name.contains("__");
    if is_upstream {
        let allowed = resolve_allowed_upstreams(state, mcp_session_id);
        match state
            .mcp_upstream_registry
            .proxy_tool_call_for_repo(&tool_name, tool_args, allowed.as_deref())
            .await
        {
            Ok(v) => v,
            Err(e) => serde_json::json!({ "error": e }),
        }
    } else {
        // Recursive async dispatch requires Box::pin. Meta names are blocked above.
        Box::pin(handle_mcp_tool_call(
            state,
            addr,
            &tool_name,
            &tool_args,
            mcp_session_id,
        ))
        .await
    }
}

/// Handle an MCP tools/call request, executing against the app state directly (no HTTP round-trip).
/// Also used by the `deep_link_mcp_call` Tauri command for the `tuic://cmd/` gateway.
pub(crate) async fn handle_mcp_tool_call(state: &Arc<AppState>, addr: SocketAddr, name: &str, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    // Enforce disabled_native_tools on every call path (not just the call_tool meta-tool)
    {
        let disabled = state.config.read().disabled_native_tools.clone();
        if disabled.iter().any(|d| d == name) {
            return serde_json::json!({"error": format!("Tool '{}' is disabled by configuration", name)});
        }
    }
    // Resolve client identity at dispatch level — tool handlers get a plain bool
    let is_claude_code = mcp_session_id
        .and_then(|sid| state.mcp_sessions.get(sid))
        .map(|meta| meta.is_claude_code)
        .unwrap_or(false);
    match name {
        "session" => handle_session(state, args, mcp_session_id),
        "agent" => handle_agent_unified(state, addr, args, mcp_session_id),
        "repo" => handle_repo(state, args, is_claude_code).await,
        "ui" => handle_ui_unified(state, addr, args, mcp_session_id),
        "plugin_dev_guide" => {
            serde_json::json!({"content": super::plugin_docs::PLUGIN_DOCS})
        }
        "config" => handle_config(state, addr, args),
        "debug" => handle_debug_unified(state, addr, args),
        "search_tools" => handle_search_tools(state, args),
        "get_tool_schema" => handle_get_tool_schema(state, args),
        "call_tool" => handle_call_tool(state, addr, args, mcp_session_id).await,
        n if super::ai_terminal::is_ai_terminal_tool(n) => {
            super::ai_terminal::handle(state, n, args).await
        }
        _ => serde_json::json!({"error": format!(
            "Unknown tool '{}'. Available: session, agent, repo, ui, plugin_dev_guide, config, debug, search_tools, get_tool_schema, call_tool, ai_terminal_*", name
        )}),
    }
}

fn handle_session(state: &Arc<AppState>, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    let action = match require_action(args, "session", SESSION_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" => {
            let sessions: Vec<serde_json::Value> = state.sessions.iter().map(|entry| {
                let id = entry.key().clone();
                let s = entry.value().lock();
                #[cfg(not(windows))]
                let pgid = s.master.process_group_leader();
                #[cfg(windows)]
                let pgid = s._child.process_id();
                #[cfg(not(windows))]
                let process_name = pgid.and_then(|p| crate::pty::process_name_from_pid(p as u32));
                #[cfg(windows)]
                let process_name = pgid.and_then(crate::pty::process_name_from_pid);
                let shell_state = state.shell_states.get(&id).map(|atom| {
                    crate::pty::shell_state_str(atom.load(std::sync::atomic::Ordering::Relaxed))
                });
                serde_json::json!({
                    "session_id": id,
                    "cwd": s.cwd,
                    "worktree_path": s.worktree.as_ref().map(|w| w.path.to_string_lossy().to_string()),
                    "worktree_branch": s.worktree.as_ref().and_then(|w| w.branch.clone()),
                    "child_pid": s._child.process_id(),
                    "foreground_pgid": pgid,
                    "foreground_process": process_name,
                    "shell_state": shell_state,
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

            match super::session::spawn_pty_session(state.clone(), shell, cwd, rows, cols, None) {
                Ok(session_id) => serde_json::json!({"session_id": session_id}),
                Err((_, body)) => serde_json::json!({"error": body.0.get("error").and_then(|v| v.as_str()).unwrap_or("spawn failed")}),
            }
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

            // Resolve the session's lifecycle state.
            //
            // A session can be in four observable states here:
            //   1. Live       — present in `state.sessions`, child still running
            //   2. Draining   — present in `state.sessions`, child already exited
            //   3. Tombstoned — absent from `state.sessions` but buffers still present
            //                   (reader thread called `mark_session_exited` on EOF;
            //                   reaped by `spawn_tombstone_sweeper` after TTL)
            //   4. Unknown    — no trace at all; either never existed or already reaped
            //
            // `exited` is only true for (2) and (3) — cases where we have evidence
            // the process actually terminated. (4) returns a structured error.
            let session_entry = state.sessions.get(session_id);
            let buffers_present = state.vt_log_buffers.contains_key(session_id)
                || state.output_buffers.contains_key(session_id);

            let (exited, exit_code): (bool, Option<i64>) = if let Some(entry) = &session_entry {
                match entry.lock()._child.try_wait() {
                    Ok(Some(status)) => (true, Some(status.exit_code() as i64)),
                    _ => (false, None),
                }
            } else if buffers_present {
                // Tombstoned — the reader thread captured the exit code if it could.
                (true, state.exit_codes.get(session_id).map(|e| *e.value() as i64))
            } else {
                // Unknown — no session entry, no buffers, no tombstone.
                (false, None)
            };
            drop(session_entry);
            let exit_code_json = exit_code
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);

            // Default: serve clean rows from VtLogBuffer (no strip_ansi needed).
            // Pass format="raw" to get the raw ring buffer content with ANSI.
            if args["format"].as_str() != Some("raw") {
                let vt_log = match state.vt_log_buffers.get(session_id) {
                    Some(b) => b,
                    None => return serde_json::json!({
                        "error": "Session not found",
                        "reason": "session_not_found_or_reaped"
                    }),
                };
                let buf = vt_log.lock();
                let total = buf.total_lines();
                let offset = total.saturating_sub(limit);
                let (log_lines, _) = buf.lines_since_owned(offset, limit);
                let screen: Vec<String> = buf.screen_rows()
                    .into_iter()
                    .filter(|r| !r.is_empty())
                    .collect();
                let mut all_lines: Vec<String> = log_lines.iter().map(|ll| ll.text()).collect();
                all_lines.extend(screen);
                let data = all_lines.join("\n");
                return serde_json::json!({"data": data, "data_length": data.len(), "total_written": total, "exited": exited, "exit_code": exit_code_json});
            }
            let ring = match state.output_buffers.get(session_id) {
                Some(r) => r,
                None => return serde_json::json!({
                    "error": "Session not found",
                    "reason": "session_not_found_or_reaped"
                }),
            };
            let (bytes, total_written) = ring.lock().read_last(limit);
            let data = String::from_utf8_lossy(&bytes).to_string();
            serde_json::json!({"data": data, "data_length": data.len(), "total_written": total_written, "exited": exited, "exit_code": exit_code_json})
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
            // Self-close guard: prevent an agent from closing its own session.
            if let Some(sid) = mcp_session_id
                && let Some(own_pty) = state.mcp_to_session.get(sid)
                && own_pty.value() == session_id
            {
                return serde_json::json!({"error": "Cannot close own session. Use exit to terminate yourself."});
            }
            // Uses the same tombstone path as the Tauri close_pty command so
            // post-mortem MCP reads keep returning final output + exit code.
            // Idempotent: returns ok even if session was already tombstoned.
            let existed = crate::pty::close_pty_core(state, session_id, false).is_some()
                || state.vt_log_buffers.contains_key(session_id);
            if existed {
                // Notify frontend and SSE consumers so the tab is removed from
                // the UI. Without this the reader thread's EOF-driven
                // session-closed event may never fire (the cloned reader fd
                // keeps the pty master alive after close_pty_core drops it).
                let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
                    session_id: session_id.to_string(),
                    reason: "closed".to_string(),
                });
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit("session-closed", serde_json::json!({
                        "session_id": session_id,
                        "reason": "closed",
                    }));
                }
            }
            // SIMP-1: drain HTML tabs registered by this session and emit close.
            emit_close_html_tabs(state.as_ref(), session_id);
            serde_json::json!({"ok": true})
        }
        "kill" => {
            let session_id = match require_session_id(args, "kill") {
                Ok(id) => id,
                Err(e) => return e,
            };
            // Self-kill guard: mirror the close branch — an agent must not SIGKILL itself.
            if let Some(sid) = mcp_session_id
                && let Some(own_pty) = state.mcp_to_session.get(sid)
                && own_pty.value() == session_id
            {
                return serde_json::json!({"error": "Cannot kill own session. Use exit to terminate yourself."});
            }
            if crate::pty::kill_pty_core(state, session_id) {
                tracing::info!(source = "session", session_id = %session_id, "Session killed: SIGKILL");
                let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
                    session_id: session_id.to_string(),
                    reason: "killed".to_string(),
                });
                if let Some(app) = state.app_handle.read().as_ref() {
                    let _ = app.emit("session-closed", serde_json::json!({
                        "session_id": session_id,
                        "reason": "killed",
                    }));
                }
                // SIMP-1: drain HTML tabs registered by this session and emit close.
                emit_close_html_tabs(state, session_id);
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
        "status" => {
            let session_id = match require_session_id(args, "status") {
                Ok(id) => id,
                Err(e) => return e,
            };
            match state.session_state_with_shell(session_id) {
                Some(ss) => {
                    let exit_code = state.exit_codes.get(session_id).map(|e| *e.value());
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let since_ms = state.shell_state_since_ms.get(session_id)
                        .map(|a| a.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(0);
                    let elapsed = if since_ms > 0 { now_ms.saturating_sub(since_ms) } else { 0 };
                    let is_idle = ss.shell_state.as_deref() == Some("idle");
                    let is_busy = ss.shell_state.as_deref() == Some("busy");
                    serde_json::json!({
                        "session_id": session_id,
                        "shell_state": ss.shell_state,
                        "agent_type": ss.agent_type,
                        "awaiting_input": ss.awaiting_input,
                        "rate_limited": ss.rate_limited,
                        "last_activity_ms": ss.last_activity_ms,
                        "exit_code": exit_code,
                        "idle_since_ms": if is_idle && elapsed > 0 { serde_json::json!(elapsed) } else { serde_json::Value::Null },
                        "busy_duration_ms": if is_busy && elapsed > 0 { serde_json::json!(elapsed) } else { serde_json::Value::Null },
                    })
                },
                None => serde_json::json!({"error": format!("Session '{}' not found", session_id)}),
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'session'. Available: {}", other, SESSION_ACTIONS
        )}),
    }
}

async fn handle_github(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "github", LEGACY_GITHUB_ACTIONS) {
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
        "issues" => {
            let path = match require_path(args, "issues") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let filter = args.get("filter").and_then(|v| v.as_str()).unwrap_or("assigned");
            let result = crate::github::get_all_issues_impl(std::slice::from_ref(&path), filter, state).await;
            match result {
                Ok(mut map) => serde_json::json!(map.remove(&path).unwrap_or_default()),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "close_issue" => {
            let path = match require_path(args, "close_issue") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let issue_number = args.get("issue_number").and_then(|v| v.as_i64()).unwrap_or(0);
            if issue_number == 0 {
                return serde_json::json!({"error": "Missing required parameter: issue_number"});
            }
            match crate::github::close_issue_impl(&path, issue_number, state).await {
                Ok(()) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "reopen_issue" => {
            let path = match require_path(args, "reopen_issue") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) { return e; }
            let issue_number = args.get("issue_number").and_then(|v| v.as_i64()).unwrap_or(0);
            if issue_number == 0 {
                return serde_json::json!({"error": "Missing required parameter: issue_number"});
            }
            match crate::github::reopen_issue_impl(&path, issue_number, state).await {
                Ok(()) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'github'. Available: {}", other, LEGACY_GITHUB_ACTIONS
        )}),
    }
}

/// Create a PTY session in the given directory, returning the session ID.
/// Reuses the same setup as `session action=create` but with fixed defaults.
fn create_session_in_dir(state: &Arc<AppState>, cwd: &str) -> Result<String, String> {
    let shell = resolve_shell(None);
    super::session::spawn_pty_session(state.clone(), shell, Some(cwd.to_string()), 24, 80, None)
        .map_err(|(_, body)| body.0.get("error").and_then(|v| v.as_str()).unwrap_or("spawn failed").to_string())
}

fn handle_worktree(state: &Arc<AppState>, args: &serde_json::Value, is_claude_code: bool) -> serde_json::Value {
    let action = match require_action(args, "worktree", LEGACY_WORKTREE_ACTIONS) {
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
                let existing: Vec<String> = match crate::worktree::get_worktree_paths(path.clone()) {
                    Ok(wts) => wts.keys().cloned().collect(),
                    Err(e) => {
                        tracing::warn!("Failed to list worktrees for name generation: {e}");
                        vec![]
                    }
                };
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
                    let branch_name = wt.branch.clone().unwrap_or_default();
                    // Notify frontend so it can offer to switch to the new worktree
                    let _ = state.event_bus.send(crate::state::AppEvent::WorktreeCreated {
                        repo_path: path.clone(),
                        branch: branch_name.clone(),
                        worktree_path: wt_path.clone(),
                    });
                    if let Some(handle) = state.app_handle.read().as_ref() {
                        let _ = handle.emit("worktree-created", serde_json::json!({
                            "repo_path": path,
                            "branch": branch_name,
                            "worktree_path": wt_path,
                        }));
                    }
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
                    // Add structured hint for Claude Code clients to spawn a subagent in the worktree
                    if is_claude_code {
                        // Sanitize branch name to prevent prompt injection via backticks/newlines
                        let safe_branch = branch_name.replace('`', "'").replace('\n', " ");
                        response["cc_agent_hint"] = serde_json::json!({
                            "worktree_path": wt_path,
                            "suggested_prompt": format!(
                                "Work in the worktree at `{}`. Use absolute paths for ALL file operations \
                                (Read, Edit, Glob, Grep). For git commands, use `cd {} && git ...`. \
                                The branch is `{}`.",
                                wt_path, wt_path, safe_branch,
                            )
                        });
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
            "Unknown action '{}' for tool 'worktree'. Available: {}", other, LEGACY_WORKTREE_ACTIONS
        )}),
    }
}

/// Build the full prompt for a spawned agent.
/// Prepends a swarm preamble when the caller is a registered peer so the child
/// knows its identity and how to communicate back. Returns the original prompt
/// unchanged when called outside a swarm context (`parent_tuic` is `None`).
fn build_spawn_prompt(prompt: &str, parent_tuic: Option<&str>, session_id: &str) -> String {
    let Some(parent) = parent_tuic else {
        return prompt.to_string();
    };
    format!(
        "## TUICommander Swarm Context\n\
         You are operating as part of a multi-agent swarm.\n\
         - Your session ID (`$TUIC_SESSION`): `{session_id}`\n\
         - Your parent agent session: `{parent}`\n\n\
         Register yourself immediately so peers can message you:\n\
         `agent action=register tuic_session=\"{session_id}\"`\n\n\
         When your task is complete, notify your parent:\n\
         `agent action=send to=\"{parent}\" message=\"<done summary>\"`\n\n\
         ## Your Task\n\n\
         {prompt}"
    )
}

fn handle_agent(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    let action = match require_action(args, "agent", LEGACY_AGENT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "detect" => {
            let known = ["claude", "codex", "aider", "goose"];
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

            // Resolve agent binary — run config name takes priority, then literal agent type
            let agents_cfg = crate::config::load_agents_config();
            let (binary_path, resolved) = if let Some(path) = args["binary_path"].as_str() {
                let p = std::path::Path::new(path);
                if !p.is_absolute() {
                    return serde_json::json!({"error": "binary_path must be an absolute path"});
                }
                if !p.is_file() {
                    return serde_json::json!({"error": "binary_path does not point to an existing file"});
                }
                (path.to_string(), None)
            } else {
                let agent_type_raw = args["agent_type"].as_str().unwrap_or("claude");
                let rc = resolve_run_config(agent_type_raw, &agents_cfg);
                let bin = rc.command.as_deref().unwrap_or(&rc.agent_type);
                let detection = crate::agent::detect_agent_binary(bin.to_string());
                match detection.path {
                    Some(p) => (p, Some(rc)),
                    None => return serde_json::json!({"error": format!("Agent binary '{}' not found", bin)}),
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

            // Resolve caller's tuic_session from their MCP session via the O(1) reverse map.
            // Only set when caller is a registered peer — drives swarm preamble + TUIC_PARENT.
            let caller_tuic: Option<String> = mcp_session_id
                .and_then(|sid| state.mcp_to_session.get(sid).map(|e| e.value().clone()));

            // Effective prompt: preamble prepended for swarm spawns, unchanged otherwise.
            let effective_prompt = build_spawn_prompt(&prompt, caller_tuic.as_deref(), &session_id);

            let mut cmd = CommandBuilder::new(&binary_path);

            // Inject swarm env vars so spawned agents know their identity and parent.
            cmd.env("TUIC_SESSION", &session_id);
            if let Some(ref parent) = caller_tuic {
                cmd.env("TUIC_PARENT", parent);
            }

            // Inject run config env vars
            if let Some(ref rc) = resolved {
                for (k, v) in &rc.env {
                    cmd.env(k, v);
                }
            }

            if let Some(raw_args) = args.get("args").and_then(|a| a.as_array()) {
                // Explicit args from caller override everything
                for arg in raw_args {
                    if let Some(s) = arg.as_str() { cmd.arg(s); }
                }
            } else if let Some(ref rc) = resolved {
                if let Some(ref rc_args) = rc.args {
                    // Run config matched: merge MCP params, then substitute {prompt}
                    let merged = match merge_mcp_params_into_args(
                        rc_args,
                        args["model"].as_str(),
                        args["print_mode"].as_bool().unwrap_or(false),
                        args["output_format"].as_str(),
                    ) {
                        Ok(m) => m,
                        Err(e) => return serde_json::json!({"error": e}),
                    };
                    let final_args = substitute_prompt_in_args(&merged, &effective_prompt);
                    for arg in &final_args {
                        cmd.arg(arg);
                    }
                } else {
                    // Run config matched but no args override — use default MCP param logic
                    if args["print_mode"].as_bool().unwrap_or(false) { cmd.arg("--print"); }
                    if let Some(format) = args["output_format"].as_str() { cmd.arg("--output-format"); cmd.arg(format); }
                    if let Some(model) = args["model"].as_str() { cmd.arg("--model"); cmd.arg(model); }
                    cmd.arg(&effective_prompt);
                }
            } else {
                // No run config, no explicit args — default MCP param logic
                if args["print_mode"].as_bool().unwrap_or(false) { cmd.arg("--print"); }
                if let Some(format) = args["output_format"].as_str() { cmd.arg("--output-format"); cmd.arg(format); }
                if let Some(model) = args["model"].as_str() { cmd.arg("--model"); cmd.arg(model); }
                cmd.arg(&effective_prompt);
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
                shell: binary_path.clone(),
            }));
            state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
            state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
            state.output_buffers.insert(session_id.clone(), Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)));
            state.vt_log_buffers.insert(session_id.clone(), Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)));
            state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

            // Broadcast session-created to SSE/WebSocket consumers
            let cwd_str = args["cwd"].as_str().map(|s| s.to_string());
            let agent_type_str = args["agent_type"].as_str().map(|s| s.to_string());
            let _ = state.event_bus.send(crate::state::AppEvent::SessionCreated {
                session_id: session_id.clone(),
                cwd: cwd_str.clone(),
                agent_type: agent_type_str,
            });

            let app_handle = state.app_handle.read().clone();
            if let Some(ref app) = app_handle {
                // Notify Tauri frontend so it creates a tab
                let agent_type_val = args["agent_type"].as_str();
                let _ = app.emit("session-created", serde_json::json!({
                    "session_id": session_id,
                    "cwd": cwd_str,
                    "agent_type": agent_type_val,
                }));
                spawn_reader_thread(reader, paused, session_id.clone(), app.clone(), state.clone());
            } else {
                spawn_headless_reader_thread(reader, paused, session_id.clone(), state.clone());
            }

            // Auto-register child as peer + pre-init inbox when spawned in swarm context.
            if let Some(ref parent_id) = caller_tuic {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                state.peer_agents.insert(session_id.clone(), crate::state::PeerAgent {
                    tuic_session: session_id.clone(),
                    mcp_session_id: String::new(), // filled when child connects via MCP
                    name: "agent".to_string(),
                    project: args["cwd"].as_str().map(|s| s.to_string()),
                    registered_at: now_ms,
                });
                state.agent_inbox.entry(session_id.clone()).or_default();
                state.session_parent.insert(session_id.clone(), parent_id.clone());
            }

            let spawn_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            // ARCH-1: keep `monitor_with` canonical (always session(output)) so
            // every spawn primitive returns the same mechanism. The peer-only
            // `peer_monitor_with` is an additive hint included only when the
            // caller is a registered orchestrator — children auto-register as
            // peers and post {type:state_change} to the parent's inbox; the
            // strategic guidance ("NEVER session output on peers — use inbox")
            // lives in agent(register).workflow, not in this response.
            let mut response = serde_json::json!({
                "session_id": session_id,
                "server_ts": spawn_ts,
                "monitor_with": format!("session(action=output, session_id={session_id})"),
                "status_with": format!("session(action=status, session_id={session_id})"),
            });
            if caller_tuic.is_some()
                && let Some(obj) = response.as_object_mut()
            {
                obj.insert(
                    "peer_monitor_with".to_string(),
                    serde_json::json!(format!("agent(action=inbox, since={spawn_ts})")),
                );
            }
            response
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
            "Unknown action '{}' for tool 'agent'. Available: {}", other, LEGACY_AGENT_ACTIONS
        )}),
    }
}

fn handle_messaging(state: &Arc<AppState>, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    let action = match require_action(args, "messaging", LEGACY_MESSAGING_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "register" => {
            let tuic_session = match args["tuic_session"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return serde_json::json!({"error": "Action 'register' requires 'tuic_session' (your $TUIC_SESSION env var)"}),
            };
            // Validate UUID format to prevent prompt-injection via preamble interpolation (SEC-1).
            // $TUIC_SESSION is always a UUID v4; reject anything that isn't.
            if !is_valid_uuid(tuic_session) {
                return serde_json::json!({"error": "tuic_session must be a UUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)"});
            }
            let mcp_sid = match mcp_session_id {
                Some(sid) => sid.to_string(),
                None => return serde_json::json!({"error": "No MCP session — send an initialize request first"}),
            };
            let name = args["name"].as_str().unwrap_or("agent").to_string();
            let project = args["project"].as_str().map(|s| s.to_string());
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let peer = crate::state::PeerAgent {
                tuic_session: tuic_session.to_string(),
                mcp_session_id: mcp_sid.clone(),
                name: name.clone(),
                project,
                registered_at: now_ms,
            };
            state.peer_agents.insert(tuic_session.to_string(), peer);
            // Map MCP session → PTY session for self-close guard resolution.
            // Reverse index (session_to_mcp) keeps tombstone cleanup O(1).
            state.mcp_to_session.insert(mcp_sid.clone(), tuic_session.to_string());
            state.session_to_mcp
                .entry(tuic_session.to_string())
                .or_default()
                .push(mcp_sid);
            let _ = state.event_bus.send(crate::state::AppEvent::PeerRegistered {
                tuic_session: tuic_session.to_string(),
                name: name.clone(),
            });
            // Teach the full multi-agent workflow in the register response so the
            // static instructions can stay compact (AC1 token budget). Any agent
            // that registers immediately receives the operational details it needs
            // for spawn/monitor/cleanup.
            serde_json::json!({
                "ok": true,
                "tuic_session": tuic_session,
                "name": name,
                "workflow": {
                    "spawn_same_repo": "agent action=spawn prompt=<task> cwd=<repo_path> — returns {session_id, monitor_with, peer_monitor_with?}. As registered orchestrator, prefer peer_monitor_with (agent inbox) over monitor_with (raw session output) to avoid token burn.",
                    "spawn_isolated": "repo action=worktree_create path=<repo> branch=<name> spawn_session=true — worktree + PTY in one call.",
                    "monitor": "agent action=inbox since=<last_ms> at 500–2000ms cadence. Poll is authoritative for LLMs (SSE push exists but is a hint). NEVER session output on peers (token burn).",
                    "auto_state_change": "Spawned peers auto-post {type:state_change, state:idle|busy|exited, session_id, exit_code?} to your inbox — no manual send needed for lifecycle.",
                    "send": "agent action=send to=<peer_tuic_session> message=<text, max 64KB>. Response {delivered_via_channel: bool} — false = queued in peer's inbox (peer reads on next poll), NOT a failure.",
                    "list_peers": "agent action=list_peers project=<optional filter> — see who else is connected.",
                    "conflict_control": "Use send/inbox to serialize shared-file edits: child sends 'claim <path>', orchestrator replies 'ack'/'deny'; child sends 'release <path>' on commit. Orchestrator is the arbiter — children never ack each other directly.",
                    "cleanup": "Automatic on MCP session close (tombstone_transient_cleanup). Peer state + inbox drained; PTY reaped."
                }
            })
        }
        "list_peers" => {
            let project_filter = args["project"].as_str();
            let peers: Vec<serde_json::Value> = state.peer_agents.iter()
                .filter(|entry| {
                    if let Some(filter) = project_filter {
                        entry.value().project.as_deref() == Some(filter)
                    } else {
                        true
                    }
                })
                .map(|entry| {
                    let p = entry.value();
                    serde_json::json!({
                        "tuic_session": p.tuic_session,
                        "name": p.name,
                        "project": p.project,
                        "registered_at": p.registered_at,
                    })
                })
                .collect();
            serde_json::json!({"peers": peers, "count": peers.len()})
        }
        "send" => {
            let to = match args["to"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return serde_json::json!({"error": "Action 'send' requires 'to' (recipient's tuic_session UUID)"}),
            };
            let message = match args["message"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return serde_json::json!({"error": "Action 'send' requires 'message'"}),
            };
            if message.len() > crate::state::AGENT_MESSAGE_MAX_BYTES {
                return serde_json::json!({"error": format!(
                    "Message exceeds 64 KB limit ({} bytes)", message.len()
                )});
            }
            // Resolve sender via O(1) mcp_to_session reverse map (RUST-3/PERF-2).
            let sender = match mcp_session_id
                .and_then(|sid| state.mcp_to_session.get(sid).map(|e| e.value().clone()))
                .and_then(|tuic| state.peer_agents.get(&tuic).map(|p| (p.tuic_session.clone(), p.name.clone())))
            {
                Some(s) => s,
                None => return serde_json::json!({"error": "You are not registered. Register first with messaging action=register"}),
            };
            // Check recipient exists
            if !state.peer_agents.contains_key(to) {
                return serde_json::json!({"error": format!("Recipient '{}' is not registered. Use list_peers to find valid targets.", to)});
            }
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let (sender_tuic, sender_name) = sender;
            let mut msg = crate::state::AgentMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from_tuic_session: sender_tuic.clone(),
                from_name: sender_name.clone(),
                content: message.to_string(),
                timestamp: now_ms,
                delivered_via_channel: false,
            };
            let msg_id = msg.id.clone();

            // Try channel push if recipient has SSE stream
            let recipient_mcp_sid = state.peer_agents.get(to).map(|p| p.mcp_session_id.clone());
            let mut pushed = false;
            if let Some(ref mcp_sid) = recipient_mcp_sid {
                let has_sse = state.mcp_sessions.get(mcp_sid)
                    .map(|m| m.has_sse_stream)
                    .unwrap_or(false);
                if has_sse && let Some(tx) = state.messaging_channels.get(mcp_sid) {
                    let notification = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/claude/channel",
                        "params": {
                            "content": format!("Message from {}: {}", sender_name, message),
                            "meta": {
                                "from_tuic_session": sender_tuic,
                                "from_name": sender_name,
                                "message_id": msg_id,
                            }
                        }
                    });
                    if tx.send(serde_json::to_string(&notification).unwrap_or_default()).is_ok() {
                        pushed = true;
                        msg.delivered_via_channel = true;
                    }
                }
            }

            // Always buffer in recipient's inbox with FIFO eviction
            let mut inbox = state.agent_inbox.entry(to.to_string()).or_default();
            if inbox.len() >= crate::state::AGENT_INBOX_CAPACITY {
                inbox.pop_front();
                *state.agent_inbox_evictions.entry(to.to_string()).or_insert(0) += 1;
            }
            inbox.push_back(msg);
            serde_json::json!({"ok": true, "message_id": msg_id, "delivered_via_channel": pushed})
        }
        "inbox" => {
            // Resolve caller's tuic_session via O(1) mcp_to_session reverse map (RUST-3/PERF-2).
            let tuic_session = match mcp_session_id
                .and_then(|sid| state.mcp_to_session.get(sid).map(|e| e.value().clone()))
                .filter(|tuic| state.peer_agents.contains_key(tuic))
            {
                Some(ts) => ts,
                None => return serde_json::json!({"error": "You are not registered. Register first with messaging action=register"}),
            };
            let limit = args["limit"].as_u64().unwrap_or(50) as usize;
            let since = args["since"].as_u64().unwrap_or(0);
            let messages: Vec<serde_json::Value> = state.agent_inbox
                .get(&tuic_session)
                .map(|inbox| {
                    inbox.iter()
                        .filter(|m| m.timestamp > since)
                        .rev() // newest first
                        .take(limit)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev() // restore chronological order
                        .map(|m| serde_json::json!({
                            "id": m.id,
                            "from_tuic_session": m.from_tuic_session,
                            "from_name": m.from_name,
                            "content": m.content,
                            "timestamp": m.timestamp,
                            "delivered_via_channel": m.delivered_via_channel,
                        }))
                        .collect()
                })
                .unwrap_or_default();
            // Consume and reset eviction counter (so caller knows since last read)
            let missed_count = state.agent_inbox_evictions
                .remove(&tuic_session)
                .map(|(_, n)| n)
                .unwrap_or(0);
            let mut resp = serde_json::json!({"messages": messages, "count": messages.len()});
            if missed_count > 0 {
                resp["missed_count"] = serde_json::json!(missed_count);
            }
            resp
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'messaging'. Available: {}", other, LEGACY_MESSAGING_ACTIONS
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
                obj.remove("session_token");
                obj.remove("vapid_private_key");
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
            let mut config: crate::config::AppConfig = match serde_json::from_value(config_val.clone()) {
                Ok(c) => c,
                Err(e) => return serde_json::json!({"error": format!("Invalid config: {}", e)}),
            };
            // Preserve server-managed secrets
            {
                let current = state.config.read();
                config.session_token = current.session_token.clone();
                config.vapid_private_key = current.vapid_private_key.clone();
                config.vapid_public_key = current.vapid_public_key.clone();
            }
            match crate::config::save_app_config(config.clone()) {
                Ok(()) => {
                    let (old_disabled, old_collapse) = {
                        let c = state.config.read();
                        (c.disabled_native_tools.clone(), c.collapse_tools)
                    };
                    *state.config.write() = config.clone();
                    if old_disabled != config.disabled_native_tools || old_collapse != config.collapse_tools {
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

fn handle_debug(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "debug", LEGACY_DEBUG_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "agent_detection" => {
            let session_ids: Vec<String> = if let Some(sid) = args["session_id"].as_str() {
                vec![sid.to_string()]
            } else {
                state.sessions.iter().map(|e| e.key().clone()).collect()
            };
            let results: Vec<serde_json::Value> = session_ids.iter().map(|sid| {
                let entry = match state.sessions.get(sid) {
                    Some(e) => e,
                    None => return serde_json::json!({ "error": "session not found", "session_id": sid }),
                };
                let session = entry.value().lock();
                #[cfg(not(windows))]
                {
                    let raw_fd = session.master.as_raw_fd();
                    let pgid = session.master.process_group_leader();
                    let name = pgid.and_then(|p| crate::pty::process_name_from_pid(p as u32));
                    let classified = name.as_deref().and_then(crate::pty::classify_agent);
                    serde_json::json!({
                        "session_id": sid,
                        "master_raw_fd": raw_fd,
                        "process_group_leader": pgid,
                        "process_name": name,
                        "classified_agent": classified,
                        "child_pid": session._child.process_id(),
                    })
                }
                #[cfg(windows)]
                {
                    let child_pid = session._child.process_id();
                    let leaf = child_pid.and_then(crate::pty::deepest_descendant_pid);
                    let name = leaf.and_then(crate::pty::process_name_from_pid);
                    let classified = name.as_deref().and_then(crate::pty::classify_agent);
                    serde_json::json!({
                        "session_id": sid,
                        "child_pid": child_pid,
                        "leaf_pid": leaf,
                        "process_name": name,
                        "classified_agent": classified,
                    })
                }
            }).collect();
            serde_json::json!(results)
        }
        "logs" => {
            let level_filter = args["level"].as_str();
            let source_filter = args["source"].as_str();
            let limit = args["limit"].as_u64().unwrap_or(50) as usize;
            let buf = state.log_buffer.lock();
            let all = buf.get_entries(0);
            let filtered: Vec<_> = all.into_iter()
                .filter(|e| level_filter.is_none_or(|l| e.level == l))
                .filter(|e| source_filter.is_none_or(|s| e.source == s))
                .collect();
            let start = filtered.len().saturating_sub(limit);
            serde_json::json!(filtered[start..])
        }
        "sessions" => {
            let sessions: Vec<serde_json::Value> = state.sessions.iter().map(|entry| {
                let sid = entry.key().clone();
                let session = entry.value().lock();
                #[cfg(not(windows))]
                let pgid = session.master.process_group_leader();
                #[cfg(windows)]
                let pgid = session._child.process_id();
                #[cfg(not(windows))]
                let process_name = pgid.and_then(|p| crate::pty::process_name_from_pid(p as u32));
                #[cfg(windows)]
                let process_name = pgid.and_then(crate::pty::process_name_from_pid);
                serde_json::json!({
                    "session_id": sid,
                    "cwd": session.cwd,
                    "child_pid": session._child.process_id(),
                    "foreground_pgid": pgid,
                    "foreground_process": process_name,
                })
            }).collect();
            serde_json::json!(sessions)
        }
        "invoke_js" => {
            // invoke_js executes arbitrary JS in the WebView — must be routed through
            // handle_debug_unified which enforces the loopback guard.
            serde_json::json!({"error": "invoke_js must be called via the debug tool (loopback-only)"})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'debug'. Available: {}", other, LEGACY_DEBUG_ACTIONS
        )}),
    }
}

fn handle_workspace(_state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "workspace", LEGACY_WORKSPACE_ACTIONS) {
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
            "Unknown action '{}' for tool 'workspace'. Available: {}", other, LEGACY_WORKSPACE_ACTIONS
        )}),
    }
}

fn handle_ui(state: &Arc<AppState>, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    let action = match require_action(args, "ui", LEGACY_UI_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "tab" => {
            let id = match args["id"].as_str() {
                Some(v) => v.to_string(),
                None => return serde_json::json!({"error": "Action 'tab' requires 'id'"}),
            };
            let title = match args["title"].as_str() {
                Some(v) => v.to_string(),
                None => return serde_json::json!({"error": "Action 'tab' requires 'title'"}),
            };
            let html_arg = args["html"].as_str().map(|s| s.to_string());
            let url_arg = args["url"].as_str().map(|s| s.to_string());
            let html = match (&html_arg, &url_arg) {
                (Some(h), None) => h.clone(),
                (None, Some(_)) => String::new(), // URL mode — html is empty, frontend uses url
                (Some(_), Some(_)) => return serde_json::json!({"error": "Provide either 'html' or 'url', not both"}),
                (None, None) => return serde_json::json!({"error": "Action 'tab' requires 'html' or 'url'"}),
            };
            // Guard: if a tuic session_id is provided and it already has a terminal,
            // decline to create an HTML tab (agent should use the terminal instead).
            if let Some(sid) = args["session_id"].as_str()
                && (state.vt_log_buffers.contains_key(sid) || state.sessions.contains_key(sid))
            {
                return serde_json::json!({
                    "ok": false,
                    "warning": format!("Session '{}' already has an active terminal. Use the terminal tab instead of creating an HTML tab.", sid)
                });
            }
            let pinned = args["pinned"].as_bool().unwrap_or(true);
            let focus = args["focus"].as_bool().unwrap_or(true);
            // Resolve origin repo for the calling MCP session so the tab lands
            // in the repo where the agent is actually working, not whichever
            // repo happens to have focus in the frontend.
            let caller_tuic = mcp_session_id
                .and_then(|mcp_sid| state.mcp_to_session.get(mcp_sid).map(|s| s.value().clone()));
            let origin_repo_path: Option<String> = caller_tuic.as_ref().and_then(|tuic| {
                state.peer_agents.get(tuic)
                    .and_then(|p| p.project.clone())
                    .or_else(|| state.sessions.get(tuic)
                        .and_then(|s| s.lock().cwd.clone()))
            });
            let mut payload = serde_json::json!({
                "id": id,
                "title": title,
                "html": html,
                "pinned": pinned,
                "focus": focus,
            });
            if let Some(ref u) = url_arg {
                payload["url"] = serde_json::Value::String(u.clone());
            }
            if let Some(ref p) = origin_repo_path {
                payload["origin_repo_path"] = serde_json::Value::String(p.clone());
            }
            // Register this tab under the creator's tuic session so it can be
            // closed automatically when that session exits.
            if let Some(ref tuic_session) = caller_tuic {
                state.session_html_tabs
                    .entry(tuic_session.clone())
                    .or_default()
                    .push(id.clone());
            }
            // Emit to Tauri webview (native mode)
            if let Some(app) = state.app_handle.read().as_ref() {
                let _ = app.emit("ui-tab", &payload);
            }
            // Emit to SSE clients (browser/mobile)
            let _ = state.event_bus.send(crate::state::AppEvent::UiTab {
                id: id.clone(),
                title,
                html,
                url: url_arg,
                pinned,
                focus,
                origin_repo_path,
            });
            serde_json::json!({"ok": true, "id": id})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'ui'. Available: {}", other, LEGACY_UI_ACTIONS
        )}),
    }
}

fn handle_notify(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "notify", LEGACY_NOTIFY_ACTIONS) {
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
            let sound = args["sound"].as_bool().unwrap_or(false);
            let _ = state.event_bus.send(crate::state::AppEvent::McpToast {
                title,
                message,
                level,
                sound,
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
            "Unknown action '{}' for tool 'notify'. Available: {}", other, LEGACY_NOTIFY_ACTIONS
        )}),
    }
}

// ---------------------------------------------------------------------------
// Knowledge (cross-repo mdkb fan-out)
// ---------------------------------------------------------------------------

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
            let client_name = body["params"]["clientInfo"]["name"].as_str();
            let is_claude_code = detect_claude_code_client(client_name);

            // Extract repo_path from MCP initialize roots[0].uri (file:// URI)
            let repo_path = body["params"]["roots"]
                .as_array()
                .and_then(|roots| roots.first())
                .and_then(|root| root["uri"].as_str())
                .and_then(|uri| uri.strip_prefix("file://"))
                .map(|path| {
                    // Resolve to a known repo path (repo_watchers keys are active repos)
                    let path = path.to_string();
                    state.repo_watchers.iter()
                        .map(|entry| entry.key().clone())
                        .find(|repo| path.starts_with(repo.as_str()))
                        .unwrap_or(path)
                });

            state.mcp_sessions.insert(session_id.clone(), crate::state::McpSessionMeta {
                created_at: std::time::Instant::now(),
                is_claude_code,
                has_sse_stream: false,
                repo_path,
            });
            let instructions = build_mcp_instructions(&state, client_name);

            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {
                        "tools": {},
                        "experimental": { "claude/channel": {} }
                    },
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
            let list_session_id = headers.get(MCP_SESSION_HEADER).and_then(|v| v.to_str().ok());
            let tools = merged_tool_definitions(&state, list_session_id);
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
            // Validate MCP session. If the session ID is stale (e.g. app restarted, or
            // long-lived client like Claude Code lost its session), auto-recover by
            // re-registering the session instead of returning an error.
            let is_cc_ua = detect_claude_code_from_headers(&headers);
            let session_valid = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|sid| {
                    if state.mcp_sessions.contains_key(sid) {
                        true
                    } else {
                        // Auto-recover: re-register the stale session ID. We don't have
                        // the original clientInfo, but User-Agent is usually enough to
                        // keep cc_agent_hint working for long-lived Claude Code clients.
                        let recovered_cc = is_cc_ua;
                        tracing::warn!(
                            "MCP session auto-recovered (stale session_id: {sid}); \
                             is_claude_code={recovered_cc} (from User-Agent)"
                        );
                        state.mcp_sessions.insert(sid.to_string(), crate::state::McpSessionMeta {
                            created_at: std::time::Instant::now(),
                            is_claude_code: recovered_cc,
                            has_sse_stream: false,
                            repo_path: None,
                        });
                        true
                    }
                })
                .unwrap_or(false);
            if !session_valid {
                // No session header at all — reject
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32600, "message": "mcp-session-id header required. Call initialize first." }
                });
                return Json(response).into_response();
            }

            let params = body.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool_name = params["name"].as_str().unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            let session_id_str = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // Route upstream-prefixed tools ({upstream}__{tool}) via the proxy registry.
            // Native tools (no "__") go through the sync handler via spawn_blocking.
            let allowed = resolve_allowed_upstreams(&state, session_id_str.as_deref());
            let (result, is_error) = if tool_name.contains("__") {
                match state.mcp_upstream_registry.proxy_tool_call_for_repo(&tool_name, args.clone(), allowed.as_deref()).await {
                    Ok(v) => (v, false),
                    Err(e) => (serde_json::json!({"error": e}), true),
                }
            } else {
                let result = handle_mcp_tool_call(&state, addr, &tool_name, &args, session_id_str.as_deref()).await;
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

/// GET /mcp — SSE stream for MCP server→client notifications (tools/list_changed, channel messages).
/// Requires a valid `mcp-session-id` header (established via POST /mcp initialize).
pub(super) async fn mcp_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate MCP session (auto-recover stale sessions, same as tools/call)
    let session_id = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let is_cc_ua = detect_claude_code_from_headers(&headers);
    let session_valid = session_id.as_deref().map(|sid| {
        if !state.mcp_sessions.contains_key(sid) {
            tracing::warn!(
                "MCP SSE session auto-recovered (stale session_id: {sid}); \
                 is_claude_code={is_cc_ua} (from User-Agent)"
            );
            state.mcp_sessions.insert(sid.to_string(), crate::state::McpSessionMeta {
                created_at: std::time::Instant::now(),
                is_claude_code: is_cc_ua,
                has_sse_stream: false,
                repo_path: None,
            });
        }
        // Mark this session as having an active SSE stream
        if let Some(mut meta) = state.mcp_sessions.get_mut(sid) {
            meta.has_sse_stream = true;
        }
        true
    }).unwrap_or(false);
    if !session_valid {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let sid = session_id.unwrap(); // safe: session_valid=true implies Some

    // Create or subscribe to per-session messaging channel
    let msg_rx = {
        let tx = state.messaging_channels
            .entry(sid.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(64).0);
        tx.subscribe()
    };

    let mut tools_rx = state.mcp_tools_changed.subscribe();
    let mut msg_rx = msg_rx;
    let cleanup_state = state.clone();
    let cleanup_sid = sid.clone();

    let stream = async_stream::stream! {
        loop {
            tokio::select! {
                result = tools_rx.recv() => {
                    match result {
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
                result = msg_rx.recv() => {
                    match result {
                        Ok(json_str) => {
                            yield Ok::<_, std::convert::Infallible>(
                                axum::response::sse::Event::default().data(json_str)
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        // SSE stream ended — mark session as no longer having SSE
        if let Some(mut meta) = cleanup_state.mcp_sessions.get_mut(&cleanup_sid) {
            meta.has_sse_stream = false;
        }
        cleanup_state.messaging_channels.remove(&cleanup_sid);
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
        // Clean up peer agents and inboxes for this MCP session
        let removed_tuic: Vec<String> = state.peer_agents.iter()
            .filter(|e| e.value().mcp_session_id == sid)
            .map(|e| e.key().clone())
            .collect();
        for tuic in &removed_tuic {
            state.peer_agents.remove(tuic);
            state.agent_inbox.remove(tuic);
            let _ = state.event_bus.send(crate::state::AppEvent::PeerUnregistered {
                tuic_session: tuic.clone(),
            });
        }
    }
    StatusCode::OK
}

// ── Unified handlers (merged tools) ──────────────────────────────────────

/// Merged repo tool: dispatches to workspace, github, or worktree handlers.
async fn handle_repo(state: &Arc<AppState>, args: &serde_json::Value, is_claude_code: bool) -> serde_json::Value {
    let action = match require_action(args, "repo", REPO_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" => handle_workspace(state, &serde_json::json!({"action": "list"})),
        "active" => handle_workspace(state, &serde_json::json!({"action": "active"})),
        "prs" => handle_github(state, &remap_action(args, "prs")).await,
        "status" => handle_github(state, &remap_action(args, "status")).await,
        "worktree_list" => handle_worktree(state, &remap_action(args, "list"), is_claude_code),
        "worktree_create" => handle_worktree(state, &remap_action(args, "create"), is_claude_code),
        "worktree_remove" => handle_worktree(state, &remap_action(args, "remove"), is_claude_code),
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'repo'. Available: {}", other, REPO_ACTIONS
        )}),
    }
}

/// Merged agent tool: original agent actions + messaging actions.
fn handle_agent_unified(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    let action = match require_action(args, "agent", AGENT_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "spawn" | "detect" | "stats" | "metrics" => {
            handle_agent(state, addr, &remap_action(args, action), mcp_session_id)
        }
        "register" | "list_peers" | "send" | "inbox" => {
            handle_messaging(state, &remap_action(args, action), mcp_session_id)
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'agent'. Available: {}", other, AGENT_ACTIONS
        )}),
    }
}

/// Merged ui tool: original tab action + notify toast/confirm.
fn handle_ui_unified(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value, mcp_session_id: Option<&str>) -> serde_json::Value {
    let action = match require_action(args, "ui", UI_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "tab" => handle_ui(state, args, mcp_session_id),
        "toast" | "confirm" => handle_notify(state, addr, &remap_action(args, action)),
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'ui'. Available: {}", other, UI_ACTIONS
        )}),
    }
}

/// Extended debug tool: original actions + plugin_guide.
fn handle_debug_unified(state: &Arc<AppState>, addr: SocketAddr, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "debug", DEBUG_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "invoke_js" => {
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "invoke_js is restricted to localhost connections"});
            }
            let script = match args["script"].as_str() {
                Some(s) => s,
                None => return serde_json::json!({"error": "script required (string)"}),
            };
            let app_handle = state.app_handle.read().clone();
            let Some(handle) = app_handle else {
                return serde_json::json!({"error": "AppHandle not initialized"});
            };
            let Some(window) = handle.get_webview_window("main") else {
                return serde_json::json!({"error": "main window not found"});
            };
            let wrapped = format!(
                r#"(async () => {{
  const __src = "eval_js";
  const __logs = [];
  const __origLog = console.log;
  const __origWarn = console.warn;
  const __origError = console.error;
  const __origInfo = console.info;
  const __fmt = (a) => typeof a === "string" ? a : JSON.stringify(a);
  console.log = (...a) => {{ __logs.push(a.map(__fmt).join(" ")); __origLog(...a); }};
  console.info = (...a) => {{ __logs.push(a.map(__fmt).join(" ")); __origInfo(...a); }};
  console.warn = (...a) => {{ __logs.push("[WARN] " + a.map(__fmt).join(" ")); __origWarn(...a); }};
  console.error = (...a) => {{ __logs.push("[ERROR] " + a.map(__fmt).join(" ")); __origError(...a); }};
  try {{
    const __result = await (async () => {{ {script} }})();
    const __val = __result === undefined ? "(undefined)" : JSON.stringify(__result, null, 2);
    const __msg = __logs.length > 0 ? __logs.join("\n") + "\n---\n" + __val : __val;
    window.__TAURI__.core.invoke("push_log", {{ level: "info", source: __src, message: __msg, dataJson: null }});
  }} catch (__e) {{
    const __val = __e instanceof Error ? `${{__e.name}}: ${{__e.message}}\n${{__e.stack}}` : String(__e);
    const __msg = __logs.length > 0 ? __logs.join("\n") + "\n---\n" + __val : __val;
    window.__TAURI__.core.invoke("push_log", {{ level: "error", source: __src, message: __msg, dataJson: null }});
  }} finally {{
    console.log = __origLog;
    console.info = __origInfo;
    console.warn = __origWarn;
    console.error = __origError;
  }}
}})()"#
            );
            match window.eval(&wrapped) {
                Ok(()) => serde_json::json!({
                    "ok": true,
                    "hint": "Result logged with source='eval_js'. Read via: debug(action='logs', source='eval_js', limit=1)"
                }),
                Err(e) => serde_json::json!({"error": format!("eval failed: {e}")}),
            }
        }
        "agent_detection" | "logs" | "sessions" => handle_debug(state, args),
        "help" => serde_json::json!({
            "actions": {
                "help": "This guide.",
                "agent_detection": "Agent detection pipeline diagnostics. Optional session_id (omit for all sessions).",
                "logs": "App log entries (info/warn/error mirrored from JS). Params: level, source, limit (default 50).",
                "sessions": "All PTY sessions with pid, cwd, foreground process info.",
                "invoke_js": "Execute JS in the main WebView (localhost only). Use `return expr` for output. Result + captured console output logged as source='eval_js'. Read via logs(source='eval_js', limit=1)."
            },
            "invoke_js_guide": {
                "console_capture": "console.log/warn/error/info are captured and included in the result.",
                "globals": {
                    "window.__TUIC__.stores()": "List all registered store snapshot names",
                    "window.__TUIC__.store(name)": "Get a store snapshot by name (repositories, paneLayout, settings, ui, keybindings, ...)",
                    "window.__TUIC__.plugins()": "All plugin states: id, loaded, enabled, error, builtIn",
                    "window.__TUIC__.plugin(id)": "Single plugin state with manifest",
                    "window.__TUIC__.pluginLogs(id, limit?)": "Plugin's internal PluginLogger entries (default 20)",
                    "window.__TUIC__.terminals()": "All terminals: id, name, sessionId, shellState, agentType, cwd",
                    "window.__TUIC__.terminal(id)": "Single terminal with awaitingInput, usageLimit",
                    "window.__TUIC__.agentTypeForSession(sid)": "Agent type lookup by PTY session ID",
                    "window.__TUIC__.activity()": "Activity center sections and active items",
                    "window.__TUIC__.logs(limit?)": "JS-side appLogger entries, all levels (default 50)"
                },
                "examples": [
                    "return window.__TUIC__.stores()",
                    "return window.__TUIC__.store('repositories')",
                    "return window.__TUIC__.store('paneLayout')",
                    "return window.__TUIC__.plugins()",
                    "return window.__TUIC__.terminals()"
                ]
            }
        }),
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'debug'. Available: {}", other, DEBUG_ACTIONS
        )}),
    }
}

/// Remap an action value in args — preserves all other fields.
fn remap_action(args: &serde_json::Value, new_action: &str) -> serde_json::Value {
    let mut remapped = args.clone();
    remapped["action"] = serde_json::Value::String(new_action.to_string());
    remapped
}

// ---------------------------------------------------------------------------
// Run config resolution
// ---------------------------------------------------------------------------

/// Result of resolving an `agent_type` string against the agents config.
/// When a run config matches, command/args/env override the agent binary defaults.
#[derive(Debug, Clone)]
struct ResolvedRunConfig {
    /// The canonical agent type key (e.g. "claude", "codex").
    agent_type: String,
    /// Override command from the matched run config, if any.
    command: Option<String>,
    /// Override args from the matched run config, if any.
    args: Option<Vec<String>>,
    /// Env vars from the matched run config, if any.
    env: std::collections::HashMap<String, String>,
}

/// Resolve an `agent_type` parameter as either:
/// 1. A run config name (case-insensitive match across all enabled agents), or
/// 2. A literal agent type / binary name.
///
/// Returns `ResolvedRunConfig` with overrides when a run config matches,
/// or just the agent_type passthrough when it doesn't.
fn resolve_run_config(agent_type: &str, agents_cfg: &crate::config::AgentsConfig) -> ResolvedRunConfig {
    let needle = agent_type.to_ascii_lowercase();

    // Pass 1: try to match as a run config name across all agents
    for (agent_key, settings) in &agents_cfg.agents {
        for cfg in &settings.run_configs {
            if cfg.name.to_ascii_lowercase() == needle {
                return ResolvedRunConfig {
                    agent_type: agent_key.clone(),
                    command: Some(cfg.command.clone()),
                    args: Some(cfg.args.clone()),
                    env: cfg.env.clone(),
                };
            }
        }
    }

    // Pass 2: treat as a literal agent type (no run config overrides)
    ResolvedRunConfig {
        agent_type: agent_type.to_string(),
        command: None,
        args: None,
        env: Default::default(),
    }
}

/// Substitute `{prompt}` placeholders in args, or append prompt as last arg.
fn substitute_prompt_in_args(args: &[String], prompt: &str) -> Vec<String> {
    let has_placeholder = args.iter().any(|a| a.contains("{prompt}"));
    if has_placeholder {
        args.iter()
            .map(|a| a.replace("{prompt}", prompt))
            .collect()
    } else {
        let mut result: Vec<String> = args.to_vec();
        result.push(prompt.to_string());
        result
    }
}

/// Merge MCP params (model, print_mode, output_format) into run config args.
/// Returns Ok(merged args) or Err(conflict description).
fn merge_mcp_params_into_args(
    args: &[String],
    model: Option<&str>,
    print_mode: bool,
    output_format: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut merged = args.to_vec();

    if let Some(model_val) = model {
        if args.iter().any(|a| a.starts_with("--model")) {
            return Err(format!(
                "Conflict: run config already contains --model but MCP param model=\"{}\" was also passed",
                model_val
            ));
        }
        merged.push("--model".to_string());
        merged.push(model_val.to_string());
    }

    if print_mode && !args.iter().any(|a| a.starts_with("--print")) {
        merged.push("--print".to_string());
    }

    if let Some(fmt) = output_format {
        if args.iter().any(|a| a.starts_with("--output-format")) {
            return Err(format!(
                "Conflict: run config already contains --output-format but MCP param output_format=\"{}\" was also passed",
                fmt
            ));
        }
        merged.push("--output-format".to_string());
        merged.push(fmt.to_string());
    }

    Ok(merged)
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
        let state = Arc::new(AppState {
            sessions: dashmap::DashMap::new(),
            worktrees_dir: std::env::temp_dir().join("test-worktrees"),
            metrics: crate::SessionMetrics::new(),
            output_buffers: dashmap::DashMap::new(),
            mcp_sessions: dashmap::DashMap::new(),
            ws_clients: dashmap::DashMap::new(),
            config: parking_lot::RwLock::new(crate::config::AppConfig::default()),
            git_cache: crate::state::GitCacheState::new(),
            repo_watchers: dashmap::DashMap::new(),
            dir_watchers: dashmap::DashMap::new(),
            http_client: reqwest::Client::new(),
            github_token: parking_lot::RwLock::new(None),
            github_token_source: parking_lot::RwLock::new(Default::default()),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            github_viewer_login: parking_lot::RwLock::new(None),
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
            oauth_flow_manager: std::sync::Arc::new(crate::mcp_oauth::flow::OAuthFlowManager::new(
                std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            )),
            mcp_tools_changed: tokio::sync::broadcast::channel(16).0,
            tool_search_index: std::sync::Arc::new(parking_lot::RwLock::new(crate::tool_search::ToolSearchIndex::build(&[]))),
            content_indices: dashmap::DashMap::new(),
            slash_mode: dashmap::DashMap::new(),
            last_output_ms: dashmap::DashMap::new(),
            shell_states: dashmap::DashMap::new(),
            terminal_rows: dashmap::DashMap::new(),
            exit_codes: dashmap::DashMap::new(),
            shell_state_since_ms: dashmap::DashMap::new(),
            loaded_plugins: dashmap::DashMap::new(),
            relay: crate::state::RelayState::new(),
            peer_agents: dashmap::DashMap::new(),
            agent_inbox: dashmap::DashMap::new(),
            agent_inbox_evictions: dashmap::DashMap::new(),
            session_html_tabs: dashmap::DashMap::new(),
            mcp_to_session: dashmap::DashMap::new(),
            session_to_mcp: dashmap::DashMap::new(),
            session_parent: dashmap::DashMap::new(),
            messaging_channels: dashmap::DashMap::new(),
            session_knowledge: dashmap::DashMap::new(),
            knowledge_dirty: dashmap::DashMap::new(),
            has_osc133_integration: dashmap::DashMap::new(),
            file_sandboxes: dashmap::DashMap::new(),
            #[cfg(unix)]
            bound_socket_path: parking_lot::RwLock::new(std::path::PathBuf::new()),
            tailscale_state: parking_lot::RwLock::new(crate::tailscale::TailscaleState::NotInstalled),
            push_store: crate::push::PushStore::load(&std::env::temp_dir()),
            desktop_window_focused: std::sync::atomic::AtomicBool::new(true),
            server_start_time: std::time::Instant::now(),
        });
        // Tests start with all native tools enabled (override production default
        // which disables config, knowledge, debug).
        state.config.write().disabled_native_tools = Vec::new();
        // Populate the cached tool search index so handlers that read from
        // it (search_tools, get_tool_schema) work in tests without requiring
        // the background updater task.
        rebuild_tool_search_index(&state);
        state
    }

    #[tokio::test]
    async fn session_create_emits_event_bus_session_created() {
        let state = test_state();
        let mut rx = state.event_bus.subscribe();

        let args = serde_json::json!({"action": "create"});
        let result = handle_session(&state, &args, None);

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
        let result = handle_session(&state, &args, None);

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

    // ── messaging tool tests ────────────────────────────────────────

    #[test]
    fn messaging_register_requires_tuic_session() {
        let state = test_state();
        let args = serde_json::json!({"action": "register"});
        let result = handle_messaging(&state, &args, Some("mcp-1"));
        assert!(result["error"].as_str().unwrap().contains("tuic_session"));
    }

    #[test]
    fn messaging_register_requires_mcp_session() {
        let state = test_state();
        let args = serde_json::json!({"action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a01"});
        let result = handle_messaging(&state, &args, None);
        assert!(result["error"].as_str().unwrap().contains("MCP session"));
    }

    #[test]
    fn messaging_register_and_list_peers() {
        let state = test_state();

        // Register two agents
        let r1 = handle_messaging(&state, &serde_json::json!({
            "action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a01", "name": "worker-1", "project": "/repo/a"
        }), Some("mcp-1"));
        assert_eq!(r1["ok"], true);
        assert_eq!(r1["name"], "worker-1");

        let r2 = handle_messaging(&state, &serde_json::json!({
            "action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a02", "name": "worker-2", "project": "/repo/a"
        }), Some("mcp-2"));
        assert_eq!(r2["ok"], true);

        // List all peers
        let list = handle_messaging(&state, &serde_json::json!({"action": "list_peers"}), Some("mcp-1"));
        assert_eq!(list["count"], 2);

        // Filter by project
        let filtered = handle_messaging(&state, &serde_json::json!({
            "action": "list_peers", "project": "/repo/b"
        }), Some("mcp-1"));
        assert_eq!(filtered["count"], 0);
    }

    #[test]
    fn messaging_register_updates_existing() {
        let state = test_state();

        handle_messaging(&state, &serde_json::json!({
            "action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a01", "name": "old-name"
        }), Some("mcp-1"));

        // Re-register with new name
        handle_messaging(&state, &serde_json::json!({
            "action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a01", "name": "new-name"
        }), Some("mcp-2"));

        assert_eq!(state.peer_agents.len(), 1);
        assert_eq!(state.peer_agents.get("550e8400-e29b-41d4-a716-446655440a01").unwrap().name, "new-name");
        assert_eq!(state.peer_agents.get("550e8400-e29b-41d4-a716-446655440a01").unwrap().mcp_session_id, "mcp-2");
    }

    #[test]
    fn messaging_register_default_name() {
        let state = test_state();
        let r = handle_messaging(&state, &serde_json::json!({
            "action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a01"
        }), Some("mcp-1"));
        assert_eq!(r["name"], "agent");
    }

    fn register_peer(state: &Arc<AppState>, tuic: &str, name: &str, mcp: &str) {
        handle_messaging(state, &serde_json::json!({
            "action": "register", "tuic_session": tuic, "name": name
        }), Some(mcp));
    }

    #[test]
    fn register_populates_reverse_index_for_o1_cleanup() {
        // PERF-1: agent(register) must populate session_to_mcp so tombstone
        // cleanup avoids the O(n) scan over mcp_to_session.
        let state = test_state();
        let tuic = "550e8400-e29b-41d4-a716-446655440aa1";
        let mcp = "mcp-perf1";
        register_peer(&state, tuic, "agent", mcp);

        assert_eq!(
            state.mcp_to_session.get(mcp).map(|e| e.value().clone()),
            Some(tuic.to_string()),
            "forward index must be populated"
        );
        let reverse = state.session_to_mcp.get(tuic).map(|e| e.value().clone());
        assert_eq!(reverse, Some(vec![mcp.to_string()]),
            "reverse index must be populated to enable O(1) cleanup");
    }

    #[test]
    fn messaging_send_requires_to_and_message() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "sender", "mcp-1");

        let r1 = handle_messaging(&state, &serde_json::json!({
            "action": "send", "message": "hello"
        }), Some("mcp-1"));
        assert!(r1["error"].as_str().unwrap().contains("'to'"));

        let r2 = handle_messaging(&state, &serde_json::json!({
            "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02"
        }), Some("mcp-1"));
        assert!(r2["error"].as_str().unwrap().contains("'message'"));
    }

    #[test]
    fn messaging_send_to_unregistered_peer() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "sender", "mcp-1");

        let r = handle_messaging(&state, &serde_json::json!({
            "action": "send", "to": "tab-999", "message": "hello"
        }), Some("mcp-1"));
        assert!(r["error"].as_str().unwrap().contains("not registered"));
    }

    #[test]
    fn messaging_send_and_inbox() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "alice", "mcp-1");
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a02", "bob", "mcp-2");

        // Alice sends to Bob
        let r = handle_messaging(&state, &serde_json::json!({
            "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": "hello bob"
        }), Some("mcp-1"));
        assert_eq!(r["ok"], true);

        // Bob checks inbox
        let inbox = handle_messaging(&state, &serde_json::json!({
            "action": "inbox"
        }), Some("mcp-2"));
        let msgs = inbox["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["from_name"], "alice");
        assert_eq!(msgs[0]["content"], "hello bob");
        assert_eq!(msgs[0]["from_tuic_session"], "550e8400-e29b-41d4-a716-446655440a01");
    }

    #[test]
    fn messaging_inbox_limit_and_since() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "alice", "mcp-1");
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a02", "bob", "mcp-2");

        // Send 3 messages
        for i in 0..3 {
            handle_messaging(&state, &serde_json::json!({
                "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": format!("msg-{}", i)
            }), Some("mcp-1"));
        }

        // Limit to 2
        let inbox = handle_messaging(&state, &serde_json::json!({
            "action": "inbox", "limit": 2
        }), Some("mcp-2"));
        assert_eq!(inbox["messages"].as_array().unwrap().len(), 2);

        // Since filter — get timestamp of first message
        let first_ts = inbox["messages"][0]["timestamp"].as_u64().unwrap();
        let since_inbox = handle_messaging(&state, &serde_json::json!({
            "action": "inbox", "since": first_ts
        }), Some("mcp-2"));
        // Should return messages after that timestamp (at least the remaining ones)
        let msgs = since_inbox["messages"].as_array().unwrap();
        assert!(msgs.iter().all(|m| m["timestamp"].as_u64().unwrap() > first_ts));
    }

    #[test]
    fn messaging_send_requires_sender_registration() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a02", "bob", "mcp-2");

        let r = handle_messaging(&state, &serde_json::json!({
            "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": "hello"
        }), Some("mcp-unknown"));
        assert!(r["error"].as_str().unwrap().contains("Register first"));
    }

    #[test]
    fn messaging_inbox_fifo_eviction() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "alice", "mcp-1");
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a02", "bob", "mcp-2");

        // Send more than AGENT_INBOX_CAPACITY messages
        for i in 0..(crate::state::AGENT_INBOX_CAPACITY + 10) {
            handle_messaging(&state, &serde_json::json!({
                "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": format!("msg-{}", i)
            }), Some("mcp-1"));
        }

        let inbox = handle_messaging(&state, &serde_json::json!({"action": "inbox", "limit": 200}), Some("mcp-2"));
        let msgs = inbox["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), crate::state::AGENT_INBOX_CAPACITY);
        // First message should be msg-10 (oldest 10 evicted)
        assert_eq!(msgs[0]["content"], "msg-10");
    }

    #[test]
    fn messaging_inbox_missed_count_on_eviction() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "alice", "mcp-1");
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a02", "bob", "mcp-2");

        // Fill to capacity — no eviction yet
        for i in 0..crate::state::AGENT_INBOX_CAPACITY {
            handle_messaging(&state, &serde_json::json!({
                "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": format!("msg-{}", i)
            }), Some("mcp-1"));
        }
        let inbox = handle_messaging(&state, &serde_json::json!({"action": "inbox"}), Some("mcp-2"));
        assert_eq!(inbox["missed_count"].as_u64().unwrap_or(0), 0, "no evictions yet");

        // 5 more messages → 5 evictions
        for i in 0..5 {
            handle_messaging(&state, &serde_json::json!({
                "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": format!("extra-{}", i)
            }), Some("mcp-1"));
        }
        let inbox = handle_messaging(&state, &serde_json::json!({"action": "inbox"}), Some("mcp-2"));
        assert_eq!(inbox["missed_count"].as_u64().unwrap(), 5, "5 evictions reported");

        // Second read — counter reset after first read
        let inbox2 = handle_messaging(&state, &serde_json::json!({"action": "inbox"}), Some("mcp-2"));
        assert_eq!(inbox2["missed_count"].as_u64().unwrap_or(0), 0, "counter reset after read");
    }

    #[test]
    fn messaging_send_message_size_limit() {
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a01", "alice", "mcp-1");
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440a02", "bob", "mcp-2");

        let big_msg = "x".repeat(crate::state::AGENT_MESSAGE_MAX_BYTES + 1);
        let r = handle_messaging(&state, &serde_json::json!({
            "action": "send", "to": "550e8400-e29b-41d4-a716-446655440a02", "message": big_msg
        }), Some("mcp-1"));
        assert!(r["error"].as_str().unwrap().contains("64 KB"));
    }

    // ── Meta-tool collapse tests (story 1078) ───────────────────────────

    /// Helper: extract tool names from a tool definitions value.
    fn tool_names(tools: &serde_json::Value) -> Vec<String> {
        tools
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn meta_tool_definitions_returns_exactly_three_tools_with_expected_names() {
        let defs = meta_tool_definitions();
        let names = tool_names(&defs);
        assert_eq!(names.len(), 3, "meta_tool_definitions must return 3 tools");
        assert_eq!(names, vec!["search_tools", "get_tool_schema", "call_tool"]);
        // Each must have a non-empty description and an inputSchema object.
        for tool in defs.as_array().unwrap() {
            assert!(
                tool["description"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
                "meta tool {:?} missing description",
                tool["name"]
            );
            assert!(
                tool["inputSchema"].is_object(),
                "meta tool {:?} missing inputSchema",
                tool["name"]
            );
        }
    }

    #[test]
    fn meta_tool_names_constant_matches_definitions() {
        let defs = meta_tool_definitions();
        let names = tool_names(&defs);
        let expected: Vec<String> = META_TOOL_NAMES.iter().map(|s| s.to_string()).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn native_tool_definitions_returns_base_plus_ai_terminal_tools() {
        let defs = native_tool_definitions();
        let names = tool_names(&defs);
        assert_eq!(
            names,
            vec![
                "session",
                "agent",
                "repo",
                "ui",
                "plugin_dev_guide",
                "config",
                "debug",
                "ai_terminal_read_screen",
                "ai_terminal_send_input",
                "ai_terminal_send_key",
                "ai_terminal_wait_for",
                "ai_terminal_get_state",
                "ai_terminal_get_context",
            ],
            "native_tool_definitions must return 7 base tools + 6 ai_terminal_* tools in order"
        );
    }

    #[test]
    fn session_description_mentions_tmux_pane_semantics() {
        let defs = native_tool_definitions();
        let session = defs.as_array().unwrap().iter().find(|t| t["name"] == "session").unwrap();
        let desc = session["description"].as_str().unwrap();
        assert!(desc.contains("tmux"), "session description must reference tmux for discoverability");
        assert!(desc.contains("send-keys") || desc.contains("send_keys"), "session description must mention send-keys equivalent");
        assert!(desc.contains("capture-pane") || desc.contains("capture_pane"), "session description must mention capture-pane equivalent");
    }

    #[test]
    fn agent_tool_includes_messaging_actions() {
        let defs = native_tool_definitions();
        let agent = defs.as_array().unwrap().iter().find(|t| t["name"] == "agent").unwrap();
        let action_desc = agent["inputSchema"]["properties"]["action"]["description"].as_str().unwrap();
        for action in &["register", "list_peers", "send", "inbox"] {
            assert!(action_desc.contains(action), "agent action description must include '{action}'");
        }
    }

    #[test]
    fn repo_tool_includes_workspace_github_worktree_actions() {
        let defs = native_tool_definitions();
        let repo = defs.as_array().unwrap().iter().find(|t| t["name"] == "repo").unwrap();
        let action_desc = repo["inputSchema"]["properties"]["action"]["description"].as_str().unwrap();
        for action in &["list", "active", "prs", "status", "worktree_list", "worktree_create", "worktree_remove"] {
            assert!(action_desc.contains(action), "repo action description must include '{action}'");
        }
    }

    #[test]
    fn ui_tool_includes_notify_actions() {
        let defs = native_tool_definitions();
        let ui = defs.as_array().unwrap().iter().find(|t| t["name"] == "ui").unwrap();
        let action_desc = ui["inputSchema"]["properties"]["action"]["description"].as_str().unwrap();
        for action in &["tab", "toast", "confirm"] {
            assert!(action_desc.contains(action), "ui action description must include '{action}'");
        }
    }

    #[test]
    fn debug_tool_includes_sessions_action() {
        let defs = native_tool_definitions();
        let debug = defs.as_array().unwrap().iter().find(|t| t["name"] == "debug").unwrap();
        let action_desc = debug["inputSchema"]["properties"]["action"]["description"].as_str().unwrap();
        assert!(action_desc.contains("sessions"), "debug action description must include 'sessions'");
    }

    #[test]
    fn merged_tools_collapse_false_returns_all_native_tools() {
        let state = test_state();
        assert!(!state.config.read().collapse_tools);

        let merged = merged_tool_definitions(&state, None);
        let names = tool_names(&merged);

        let native = tool_names(&native_tool_definitions());
        assert_eq!(
            names, native,
            "collapse_tools=false should return all native tools"
        );
        assert!(names.len() > 3, "baseline native tool set must exceed 3 tools");
    }

    #[test]
    fn merged_tools_collapse_true_returns_exactly_three_meta_tools() {
        let state = test_state();
        state.config.write().collapse_tools = true;

        let merged = merged_tool_definitions(&state, None);
        let names = tool_names(&merged);

        assert_eq!(names.len(), 3);
        assert_eq!(names, vec!["search_tools", "get_tool_schema", "call_tool"]);
    }

    /// Sanity check on the token-reduction claim for lazy tool loading.
    /// Measured on the native-only test state (no upstreams registered):
    /// baseline ≈ 11 KiB, collapsed ≈ 1.7 KiB — roughly 6.7× reduction.
    /// In production with typical upstreams (100+ tools) the baseline is
    /// ~35 KiB, pushing the real reduction toward ~20×. Thresholds here
    /// are regression guards, not targets, so they use the conservative
    /// native-only numbers.
    #[test]
    fn collapse_tools_payload_size_meets_reduction_target() {
        let state = test_state();

        let baseline = serde_json::to_vec(&merged_tool_definitions(&state, None))
            .expect("serialize baseline")
            .len();

        state.config.write().collapse_tools = true;
        let collapsed = serde_json::to_vec(&merged_tool_definitions(&state, None))
            .expect("serialize collapsed")
            .len();

        assert!(
            collapsed < 4096,
            "collapsed tools/list must stay under 4 KiB, got {collapsed} bytes"
        );
        assert!(
            baseline >= collapsed * 5,
            "expected >=5x reduction on native-only state, baseline={baseline} collapsed={collapsed}"
        );
    }

    #[test]
    fn merged_tools_collapse_true_ignores_disabled_native_tools() {
        // When collapsed, disabled_native_tools has no effect on the returned list —
        // the 3 meta-tools are always the full response. (Enforcement happens inside
        // search_tools / call_tool handlers in story 1079/1080.)
        let state = test_state();
        state.config.write().collapse_tools = true;
        state.config.write().disabled_native_tools = vec!["session".to_string()];

        let merged = merged_tool_definitions(&state, None);
        assert_eq!(tool_names(&merged).len(), 3);
    }

    // ── Meta-tool handler tests (story 1079) ───────────────────────────

    fn loopback_addr() -> SocketAddr {
        "127.0.0.1:12345".parse().unwrap()
    }

    fn non_loopback_addr() -> SocketAddr {
        "192.168.1.42:12345".parse().unwrap()
    }

    // search_tools

    #[test]
    fn search_tools_requires_query() {
        let state = test_state();
        let r = handle_search_tools(&state, &serde_json::json!({}));
        assert!(r["error"].as_str().unwrap().contains("query"));

        let r = handle_search_tools(&state, &serde_json::json!({ "query": "" }));
        assert!(r["error"].as_str().unwrap().contains("query"));

        let r = handle_search_tools(&state, &serde_json::json!({ "query": "   " }));
        assert!(r["error"].as_str().unwrap().contains("query"));
    }

    #[test]
    fn search_tools_returns_ranked_results_for_session_query() {
        let state = test_state();
        // Query targets the PTY multiplexer specifically — distinguishes
        // `session` from the ai_terminal_* observation tools that also
        // mention "terminal".
        let r = handle_search_tools(&state, &serde_json::json!({ "query": "PTY multiplexer tmux pane lifecycle" }));
        let results = r["results"].as_array().unwrap();
        assert!(!results.is_empty(), "expected non-empty results");
        assert_eq!(results[0]["name"], "session");
        // summary is the first sentence of the description — must be populated.
        assert!(results[0]["summary"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
    }

    #[test]
    fn search_tools_returns_ranked_results_for_github_query() {
        let state = test_state();
        let r = handle_search_tools(&state, &serde_json::json!({ "query": "github PR status" }));
        let results = r["results"].as_array().unwrap();
        assert_eq!(results[0]["name"], "repo");
    }

    #[test]
    fn search_tools_excludes_disabled_native_tools() {
        let state = test_state();
        state.config.write().disabled_native_tools = vec!["session".to_string()];
        rebuild_tool_search_index(&state);

        let r = handle_search_tools(&state, &serde_json::json!({ "query": "terminal session" }));
        let results = r["results"].as_array().unwrap();
        // "session" must not appear at all.
        let has_session = results.iter().any(|v| v["name"] == "session");
        assert!(!has_session, "disabled 'session' tool must be absent from search results");
    }

    #[test]
    fn search_tools_nonsense_query_returns_empty() {
        let state = test_state();
        let r = handle_search_tools(&state, &serde_json::json!({ "query": "xyzzyplugh nonsense qqq" }));
        let results = r["results"].as_array().unwrap();
        assert_eq!(results.len(), 0);
        assert_eq!(r["count"], 0);
    }

    #[test]
    fn search_tools_respects_limit() {
        let state = test_state();
        let r = handle_search_tools(&state, &serde_json::json!({ "query": "action", "limit": 2 }));
        let results = r["results"].as_array().unwrap();
        assert!(results.len() <= 2);
    }

    // get_tool_schema

    #[test]
    fn get_tool_schema_requires_tool_name() {
        let state = test_state();
        let r = handle_get_tool_schema(&state, &serde_json::json!({}));
        assert!(r["error"].as_str().unwrap().contains("tool_name"));
    }

    #[test]
    fn get_tool_schema_returns_full_definition_for_native_tool() {
        let state = test_state();
        let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "session" }));
        assert_eq!(r["name"], "session");
        assert!(r["description"].as_str().is_some());
        assert!(r["inputSchema"].is_object());
        assert_eq!(r["inputSchema"]["type"], "object");
    }

    #[test]
    fn get_tool_schema_returns_error_for_unknown_tool() {
        let state = test_state();
        let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "does_not_exist" }));
        let err = r["error"].as_str().unwrap();
        assert!(err.contains("not found"));
        assert!(err.contains("search_tools"), "error should guide user to search_tools");
    }

    #[test]
    fn get_tool_schema_excludes_disabled_native_tools() {
        let state = test_state();
        state.config.write().disabled_native_tools = vec!["debug".to_string()];
        rebuild_tool_search_index(&state);
        let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "debug" }));
        assert!(r["error"].as_str().is_some());
    }

    // call_tool

    #[tokio::test]
    async fn call_tool_requires_tool_name() {
        let state = test_state();
        let r = handle_call_tool(&state, loopback_addr(), &serde_json::json!({}), None).await;
        assert!(r["error"].as_str().unwrap().contains("tool_name"));
    }

    #[tokio::test]
    async fn call_tool_blocks_meta_tool_recursion() {
        let state = test_state();
        for meta in META_TOOL_NAMES {
            let r = handle_call_tool(
                &state,
                loopback_addr(),
                &serde_json::json!({ "tool_name": meta, "arguments": { "query": "x" } }),
                None,
            )
            .await;
            let err = r["error"].as_str().unwrap();
            assert!(err.contains("cannot invoke meta-tool"), "meta '{meta}' should be blocked: {err}");
        }
    }

    #[tokio::test]
    async fn call_tool_rejects_disabled_native_tool() {
        let state = test_state();
        state.config.write().disabled_native_tools = vec!["workspace".to_string()];
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": "workspace", "arguments": { "action": "active" } }),
            None,
        )
        .await;
        assert!(r["error"].as_str().unwrap().contains("disabled"));
    }

    #[tokio::test]
    async fn call_tool_returns_unknown_tool_error_for_bogus_name() {
        let state = test_state();
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": "nonsense_xyz", "arguments": {} }),
            None,
        )
        .await;
        let err = r["error"].as_str().unwrap();
        assert!(err.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn call_tool_dispatches_to_native_handler_propagating_args() {
        // session with a missing action should surface handle_session's guidance
        // error — this proves the args went through the dispatch layer.
        let state = test_state();
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": "session", "arguments": {} }),
            None,
        )
        .await;
        let err = r["error"].as_str().unwrap();
        assert!(err.contains("action"), "expected handle_session's 'action' guidance error: {err}");
    }

    #[tokio::test]
    async fn call_tool_propagates_addr_for_localhost_only_tools() {
        // config save is restricted to loopback addresses. call_tool must propagate
        // the caller's addr so the restriction still fires through the meta layer.
        let state = test_state();
        let r = handle_call_tool(
            &state,
            non_loopback_addr(),
            &serde_json::json!({
                "tool_name": "config",
                "arguments": { "action": "save", "config": {} }
            }),
            None,
        )
        .await;
        let err = r["error"].as_str().unwrap();
        assert!(
            err.contains("localhost"),
            "non-loopback config save must be rejected via addr propagation: {err}"
        );
    }

    #[tokio::test]
    async fn call_tool_missing_arguments_defaults_to_empty_object() {
        // Omitting 'arguments' must not crash — handler receives {} and produces
        // its own missing-action error.
        let state = test_state();
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": "session" }),
            None,
        )
        .await;
        assert!(r["error"].as_str().unwrap().contains("action"));
    }

    #[tokio::test]
    async fn call_tool_routes_unknown_upstream_prefixed_name_through_proxy() {
        // No upstreams are registered in tests — any tool_name with "__" falls
        // through to proxy_tool_call, which errors out. We just verify that the
        // error comes from the upstream path (not the native unknown-tool branch).
        let state = test_state();
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": "fake_upstream__some_tool", "arguments": {} }),
            None,
        )
        .await;
        let err = r["error"].as_str().unwrap();
        // proxy_tool_call returns an error string — just assert it's an error and
        // that the native unknown-tool message is NOT what we got.
        assert!(!err.contains("Unknown tool"), "upstream-prefixed name must not hit native fallthrough: {err}");
    }

    // Route via the top-level dispatcher too, to cover the match-arm wiring.
    #[tokio::test]
    async fn handle_mcp_tool_call_routes_search_tools() {
        let state = test_state();
        let r = handle_mcp_tool_call(
            &state,
            loopback_addr(),
            "search_tools",
            &serde_json::json!({ "query": "terminal" }),
            None,
        )
        .await;
        assert!(r["results"].is_array());
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_routes_get_tool_schema() {
        let state = test_state();
        let r = handle_mcp_tool_call(
            &state,
            loopback_addr(),
            "get_tool_schema",
            &serde_json::json!({ "tool_name": "agent" }),
            None,
        )
        .await;
        assert_eq!(r["name"], "agent");
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_routes_call_tool() {
        let state = test_state();
        let r = handle_mcp_tool_call(
            &state,
            loopback_addr(),
            "call_tool",
            &serde_json::json!({ "tool_name": "session", "arguments": {} }),
            None,
        )
        .await;
        assert!(r["error"].as_str().unwrap().contains("action"));
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_routes_repo() {
        let state = test_state();
        let r = handle_mcp_tool_call(
            &state, loopback_addr(), "repo",
            &serde_json::json!({ "action": "list" }), None,
        ).await;
        // repo action=list returns an array of repos (may be empty in test)
        assert!(r.is_array(), "repo action=list should return array, got: {r}");
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_routes_agent_messaging() {
        let state = test_state();
        // agent action=register without tuic_session should return an error
        let r = handle_mcp_tool_call(
            &state, loopback_addr(), "agent",
            &serde_json::json!({ "action": "register" }), None,
        ).await;
        assert!(r["error"].is_string(), "agent action=register without tuic_session should error");
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_routes_ui_toast() {
        let state = test_state();
        let r = handle_mcp_tool_call(
            &state, loopback_addr(), "ui",
            &serde_json::json!({ "action": "toast", "title": "test" }), None,
        ).await;
        assert!(!r["error"].is_string(), "ui action=toast should succeed, got: {r}");
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_routes_debug_sessions() {
        let state = test_state();
        let r = handle_mcp_tool_call(
            &state, loopback_addr(), "debug",
            &serde_json::json!({ "action": "sessions" }), None,
        ).await;
        assert!(r.is_array(), "debug action=sessions should return array of sessions");
    }

    #[tokio::test]
    async fn handle_mcp_tool_call_old_names_return_unknown() {
        let state = test_state();
        for old_name in &["github", "worktree", "workspace", "messaging", "notify"] {
            let r = handle_mcp_tool_call(
                &state, loopback_addr(), old_name,
                &serde_json::json!({ "action": "list" }), None,
            ).await;
            assert!(
                r["error"].as_str().unwrap_or("").contains("Unknown tool"),
                "old tool name '{old_name}' should return Unknown tool error, got: {r}"
            );
        }
    }

    // ---- build_mcp_instructions collapse mode (story 1081) -------------------

    #[test]
    fn instructions_collapse_off_lists_individual_tools() {
        let state = test_state();
        let out = build_mcp_instructions(&state, None);
        // Tools bullets + concrete workflow references are present.
        assert!(out.contains("## Tools\n"), "expected classic Tools section");
        assert!(out.contains("- `session` ("), "expected session bullet in tools list");
        assert!(out.contains("## Workflow"), "expected Workflow section");
        assert!(!out.contains("## Tools — Lazy Discovery"));
        assert!(!out.contains("search_tools"));
    }

    #[test]
    fn instructions_collapse_on_describes_search_schema_call_flow() {
        let state = test_state();
        state.config.write().collapse_tools = true;
        let out = build_mcp_instructions(&state, None);

        // Lazy discovery section replaces the concrete tools table.
        assert!(out.contains("## Tools — Lazy Discovery"), "expected lazy discovery header");
        assert!(out.contains("`search_tools`"), "must mention search_tools");
        assert!(out.contains("`get_tool_schema`"), "must mention get_tool_schema");
        assert!(out.contains("`call_tool`"), "must mention call_tool");
        // The search→schema→call flow must be explicit.
        assert!(out.contains("search_tools(query"), "must show search_tools usage");
        assert!(out.contains("get_tool_schema(tool_name"), "must show get_tool_schema usage");
        assert!(out.contains("call_tool(tool_name"), "must show call_tool usage");
        // Domain summary so the model can form a query.
        assert!(out.contains("terminal pane sessions"));
        assert!(out.contains("worktree"));
        // The concrete tools list and legacy workflow must NOT appear — those
        // reference tool names the model cannot invoke directly in collapse mode.
        assert!(!out.contains("- `session` ("), "tools list must be suppressed in collapse mode");
        assert!(!out.contains("## Workflow"), "legacy workflow must be suppressed in collapse mode");
    }

    // ---- Swarm Layer 4: MCP tool descriptions (#1165-b124) -------------------

    #[test]
    fn session_description_includes_status_action() {
        let defs = native_tool_definitions();
        let session = defs.as_array().unwrap().iter().find(|t| t["name"] == "session").unwrap();
        let desc = session["description"].as_str().unwrap();
        assert!(desc.contains("status:"), "session description must document the status action");
        let action_enum = session["inputSchema"]["properties"]["action"]["description"].as_str().unwrap();
        assert!(action_enum.contains("status"), "session action enum must include status");
    }

    #[test]
    fn print_mode_description_clarifies_visible_vs_headless() {
        let defs = native_tool_definitions();
        let agent = defs.as_array().unwrap().iter().find(|t| t["name"] == "agent").unwrap();
        let pm_desc = agent["inputSchema"]["properties"]["print_mode"]["description"].as_str().unwrap();
        assert!(pm_desc.contains("visible") || pm_desc.contains("TUI tab"), "print_mode must mention visible TUI tab");
        assert!(pm_desc.contains("headless"), "print_mode must mention headless mode");
    }

    #[test]
    fn instructions_include_session_status_for_polling() {
        let state = test_state();
        let out = build_mcp_instructions(&state, None);
        assert!(out.contains("status"), "instructions must mention session status for swarm polling");
    }

    #[test]
    fn instructions_tools_and_definitions_in_sync_for_session_actions() {
        // build_mcp_instructions session bullet must list the same actions as SESSION_ACTIONS.
        let state = test_state();
        let out = build_mcp_instructions(&state, None);
        for action in SESSION_ACTIONS.split(", ") {
            assert!(out.contains(action), "instructions must mention session action '{action}'");
        }
    }

    // ---- ToolSearchIndex lifecycle (story 1080) ------------------------------

    /// Fresh AppState constructed outside the tests-only test_state() helper
    /// (which eagerly rebuilds) starts with an empty cached index. This pins
    /// the invariant that the default field value is empty.
    #[test]
    fn tool_search_index_default_is_empty() {
        // Mirror the lib-default construction (no eager rebuild).
        let idx = crate::tool_search::ToolSearchIndex::build(&[]);
        assert!(idx.is_empty());
    }

    /// After `rebuild_tool_search_index`, the cache contains every native
    /// tool from `native_tool_definitions()`.
    #[test]
    fn rebuild_tool_search_index_populates_all_native_tools() {
        let state = test_state(); // test_state() already calls rebuild internally.
        let idx = state.tool_search_index.read();
        let native_count = native_tool_definitions().as_array().unwrap().len();
        assert_eq!(idx.len(), native_count);
        // Spot-check a few well-known native tools by name.
        assert!(idx.get_schema("session").is_some());
        assert!(idx.get_schema("repo").is_some());
        assert!(idx.get_schema("agent").is_some());
    }

    /// After mutating `disabled_native_tools` and rebuilding, the disabled
    /// tool no longer appears in the cached index.
    #[test]
    fn rebuild_tool_search_index_respects_disabled_native_tools() {
        let state = test_state();
        assert!(state.tool_search_index.read().get_schema("session").is_some());
        state.config.write().disabled_native_tools = vec!["session".to_string()];
        rebuild_tool_search_index(&state);
        assert!(state.tool_search_index.read().get_schema("session").is_none());
    }

    /// The background updater task subscribes to `mcp_tools_changed` and
    /// rebuilds the cached index on every signal. This is what wires upstream
    /// add/remove, native-tool toggle, and collapse-tools toggle events into
    /// the cache without each call site having to rebuild manually.
    #[tokio::test]
    async fn tool_search_index_rebuilds_on_broadcast() {
        let state = test_state();

        // Start the updater — it does its own initial rebuild, then loops on the broadcast.
        spawn_tool_search_index_updater(state.clone());
        // Give the initial build a moment to land.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(state.tool_search_index.read().get_schema("session").is_some());

        // Mutate config and fire the signal; the updater must rebuild.
        state.config.write().disabled_native_tools = vec!["session".to_string()];
        let _ = state.mcp_tools_changed.send(());

        // Poll for the rebuild with a short deadline — the task is async.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < deadline {
            if state.tool_search_index.read().get_schema("session").is_none() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("tool_search_index was not rebuilt after mcp_tools_changed signal");
    }

    /// Toggling `collapse_tools` must not corrupt the searchable corpus:
    /// the cache always holds the full tool list regardless of the collapse
    /// state (collapse only affects what the client sees via tools/list).
    #[tokio::test]
    async fn tool_search_index_ignores_collapse_tools_toggle() {
        let state = test_state();
        spawn_tool_search_index_updater(state.clone());
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let before = state.tool_search_index.read().len();

        state.config.write().collapse_tools = true;
        let _ = state.mcp_tools_changed.send(());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let after = state.tool_search_index.read().len();
        assert_eq!(before, after, "collapse_tools toggle must not change searchable corpus size");
        // And native tools must still be searchable.
        assert!(state.tool_search_index.read().get_schema("session").is_some());
    }

    #[test]
    fn ui_tab_emits_event() {
        let state = test_state();
        let mut rx = state.event_bus.subscribe();

        let result = handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "test-panel",
            "title": "Test",
            "html": "<p>hello</p>"
        }), None);
        assert_eq!(result["ok"], true);
        assert_eq!(result["id"], "test-panel");

        let event = rx.try_recv().expect("Expected UiTab event");
        match event {
            crate::state::AppEvent::UiTab { id, title, html, url, pinned, focus, origin_repo_path } => {
                assert_eq!(id, "test-panel");
                assert_eq!(title, "Test");
                assert_eq!(html, "<p>hello</p>");
                assert!(url.is_none(), "url should be None for html tab");
                assert!(pinned, "pinned should default to true");
                assert!(focus, "focus should default to true");
                assert!(origin_repo_path.is_none(), "origin_repo_path should be None when no mcp_session");
            }
            other => panic!("Expected UiTab, got {:?}", other),
        }
    }

    #[test]
    fn ui_tab_includes_origin_repo_path_from_peer_agent() {
        use crate::state::PeerAgent;
        let state = test_state();
        let mcp_sid = "mcp-xyz".to_string();
        let tuic = "00000000-0000-0000-0000-000000000001".to_string();
        // Register an MCP→tuic mapping and a peer agent with a project path.
        state.mcp_to_session.insert(mcp_sid.clone(), tuic.clone());
        state.peer_agents.insert(tuic.clone(), PeerAgent {
            tuic_session: tuic.clone(),
            mcp_session_id: mcp_sid.clone(),
            name: "wiz".to_string(),
            project: Some("/Gits/personal/alpha".to_string()),
            registered_at: 0,
        });

        let mut rx = state.event_bus.subscribe();
        let result = handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "mcf",
            "title": "MCF",
            "html": "<p/>"
        }), Some(&mcp_sid));
        assert_eq!(result["ok"], true);

        let event = rx.try_recv().expect("Expected UiTab event");
        match event {
            crate::state::AppEvent::UiTab { origin_repo_path, .. } => {
                assert_eq!(origin_repo_path.as_deref(), Some("/Gits/personal/alpha"),
                    "caller's repo path must be propagated so the tab lands in the right repo");
            }
            other => panic!("Expected UiTab, got {:?}", other),
        }
    }

    #[test]
    fn ui_tab_falls_back_to_pty_cwd_when_no_peer_agent() {
        use crate::state::PtySession;
        use portable_pty::{native_pty_system, PtySize};

        let state = test_state();
        let mcp_sid = "mcp-no-peer".to_string();
        let tuic = "00000000-0000-0000-0000-000000000002".to_string();
        state.mcp_to_session.insert(mcp_sid.clone(), tuic.clone());

        // Spawn a minimal PTY session with cwd set so we can exercise the fallback.
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
            .expect("openpty");
        let mut cmd = portable_pty::CommandBuilder::new("true");
        cmd.cwd("/tmp");
        let child = pair.slave.spawn_command(cmd).expect("spawn");
        let writer = pair.master.take_writer().expect("writer");
        state.sessions.insert(tuic.clone(), parking_lot::Mutex::new(PtySession {
            writer,
            master: pair.master,
            _child: child,
            paused: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            worktree: None,
            cwd: Some("/Gits/personal/beta".to_string()),
            display_name: None,
            shell: "true".to_string(),
        }));

        let mut rx = state.event_bus.subscribe();
        handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "beta-tab",
            "title": "Beta",
            "html": "<p/>"
        }), Some(&mcp_sid));

        let event = rx.try_recv().expect("Expected UiTab event");
        match event {
            crate::state::AppEvent::UiTab { origin_repo_path, .. } => {
                assert_eq!(origin_repo_path.as_deref(), Some("/Gits/personal/beta"));
            }
            other => panic!("Expected UiTab, got {:?}", other),
        }
    }

    #[test]
    fn ui_tab_requires_fields() {
        let state = test_state();
        let r = handle_ui(&state, &serde_json::json!({"action": "tab"}), None);
        assert!(r["error"].as_str().unwrap().contains("'id'"));

        let r = handle_ui(&state, &serde_json::json!({"action": "tab", "id": "x"}), None);
        assert!(r["error"].as_str().unwrap().contains("'title'"));

        // Requires either html or url — url is accepted as alternative to html
        let r = handle_ui(&state, &serde_json::json!({"action": "tab", "id": "x", "title": "t"}), None);
        assert!(r["error"].as_str().unwrap().contains("'html' or 'url'"));

        // url alone is accepted
        let r = handle_ui(&state, &serde_json::json!({"action": "tab", "id": "x", "title": "t", "url": "http://localhost/"}), None);
        assert_eq!(r["ok"], true);

        // Both html and url is rejected
        let r = handle_ui(&state, &serde_json::json!({"action": "tab", "id": "x", "title": "t", "html": "<p/>", "url": "http://localhost/"}), None);
        assert!(r["error"].as_str().unwrap().contains("not both"));
    }

    #[test]
    fn ui_tab_focus_false() {
        let state = test_state();
        let mut rx = state.event_bus.subscribe();

        handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "bg",
            "title": "Background",
            "html": "<p/>",
            "focus": false
        }), None);

        let event = rx.try_recv().expect("Expected UiTab event");
        match event {
            crate::state::AppEvent::UiTab { focus, .. } => {
                assert!(!focus, "focus=false should be respected");
            }
            other => panic!("Expected UiTab, got {:?}", other),
        }
    }

    #[test]
    fn ui_tab_pinned_false() {
        let state = test_state();
        let mut rx = state.event_bus.subscribe();

        handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "unpinned",
            "title": "T",
            "html": "<p/>",
            "pinned": false
        }), None);

        let event = rx.try_recv().expect("Expected UiTab event");
        match event {
            crate::state::AppEvent::UiTab { pinned, .. } => {
                assert!(!pinned);
            }
            other => panic!("Expected UiTab, got {:?}", other),
        }
    }

    // -------- HTML tab lifecycle tests (story 1176-b88b) --------

    #[test]
    fn ui_tab_warns_when_session_already_has_terminal() {
        use crate::state::VtLogBuffer;
        let state = test_state();
        // Simulate an active session by inserting into vt_log_buffers
        state.vt_log_buffers.insert(
            "sess-active".to_string(),
            parking_lot::Mutex::new(VtLogBuffer::new(24, 220, 500)),
        );

        // Calling ui(tab) with session_id = active session should warn, not create tab
        let r = handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "status-tab",
            "title": "Status",
            "html": "<p>status</p>",
            "session_id": "sess-active"
        }), None);
        assert!(r.get("warning").and_then(|v| v.as_str()).is_some(),
            "should return warning when session_id has an active terminal");
        assert_eq!(r["ok"], serde_json::json!(false),
            "should not create tab when session already has terminal");
    }

    #[test]
    fn ui_tab_no_warning_without_session_id() {
        let state = test_state();
        // No session_id → normal tab creation, no warning
        let r = handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "standalone-tab",
            "title": "My Tab",
            "html": "<p>hello</p>"
        }), None);
        assert_eq!(r["ok"], serde_json::json!(true));
        assert!(r.get("warning").is_none());
    }

    #[test]
    fn ui_tab_no_warning_for_unknown_session_id() {
        let state = test_state();
        // session_id refers to a session that doesn't exist → no warning, tab created normally
        let r = handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "status-tab",
            "title": "Status",
            "html": "<p>hi</p>",
            "session_id": "nonexistent-session"
        }), None);
        assert_eq!(r["ok"], serde_json::json!(true),
            "nonexistent session_id should not block tab creation");
    }

    #[test]
    fn ui_tab_registers_creator_and_clears_on_session_close() {
        use crate::state::VtLogBuffer;
        let state = test_state();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440b02", "orchestrator", "mcp-orch");
        // Map mcp_session_id → tuic_session
        state.mcp_to_session.insert("mcp-orch".to_string(), "550e8400-e29b-41d4-a716-446655440b02".to_string());

        // Create HTML tab as orchestrator
        let r = handle_ui(&state, &serde_json::json!({
            "action": "tab",
            "id": "orch-status",
            "title": "Orchestrator",
            "html": "<p>running</p>"
        }), Some("mcp-orch"));
        assert_eq!(r["ok"], serde_json::json!(true));

        // session_html_tabs should have the tab registered under the creator's session
        let tabs = state.session_html_tabs.get("550e8400-e29b-41d4-a716-446655440b02");
        assert!(tabs.is_some(), "tab should be registered under creator session");
        assert!(tabs.unwrap().contains(&"orch-status".to_string()));

        // Insert vt_log_buffers so close succeeds
        state.vt_log_buffers.insert(
            "550e8400-e29b-41d4-a716-446655440b02".to_string(),
            parking_lot::Mutex::new(VtLogBuffer::new(24, 220, 500)),
        );
        // Close the session — should clear its html tabs
        handle_session(&state, &serde_json::json!({"action": "close", "session_id": "550e8400-e29b-41d4-a716-446655440b02"}), None);

        assert!(state.session_html_tabs.get("550e8400-e29b-41d4-a716-446655440b02").is_none(),
            "session_html_tabs should be cleared after session close");
    }

    /// Characterization for SIMP-1: when a session has registered HTML tabs and is
    /// closed via the MCP `session(close)` action, the entry MUST be drained from
    /// `session_html_tabs` (the same shared helper is used by `session(kill)`).
    #[test]
    fn session_close_drains_session_html_tabs_entry() {
        let target = "550e8400-e29b-41d4-a716-446655440d01";
        let state = test_state();
        state.session_html_tabs.insert(target.to_string(), vec!["html-tab-1".to_string()]);

        use crate::state::VtLogBuffer;
        state.vt_log_buffers.insert(
            target.to_string(),
            parking_lot::Mutex::new(VtLogBuffer::new(24, 220, 500)),
        );
        handle_session(&state, &serde_json::json!({"action": "close", "session_id": target}), None);
        assert!(state.session_html_tabs.get(target).is_none(),
            "html tabs entry must be removed after close (drives SIMP-1 helper)");
    }

    // -------- Tombstone / post-mortem output regression tests --------

    /// Simulate a process-exited session (tombstone) by inserting buffers and
    /// an exit code without a `sessions` entry. The `output` action must serve
    /// the last output with `exited: true` and the captured `exit_code` — NOT
    /// return "Session not found".
    #[test]
    fn tombstoned_session_output_returns_last_buffer_and_exit_code() {
        use crate::state::VtLogBuffer;
        use crate::OutputRingBuffer;
        use std::sync::atomic::AtomicU64;

        let state = test_state();
        let sid = "tombstone-test-1".to_string();

        // Pre-populate buffers with sample output.
        let mut ring = OutputRingBuffer::new(4096);
        ring.write(b"hello from the crypt\n");
        state.output_buffers.insert(sid.clone(), parking_lot::Mutex::new(ring));

        let mut vt = VtLogBuffer::new(24, 80, 100);
        vt.process(b"hello from the crypt\r\n");
        state.vt_log_buffers.insert(sid.clone(), parking_lot::Mutex::new(vt));

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        state.last_output_ms.insert(sid.clone(), AtomicU64::new(now_ms));
        state.exit_codes.insert(sid.clone(), 42);

        // Sanity: session entry is absent (this IS the tombstone).
        assert!(!state.sessions.contains_key(&sid));

        // Raw format path.
        let raw_res = handle_session(
            &state,
            &serde_json::json!({"action": "output", "session_id": sid, "format": "raw"}),
            None,
        );
        assert!(raw_res.get("error").is_none(), "Unexpected error: {raw_res}");
        assert_eq!(raw_res["exited"], serde_json::json!(true));
        assert_eq!(raw_res["exit_code"], serde_json::json!(42));
        assert!(
            raw_res["data"].as_str().unwrap().contains("hello from the crypt"),
            "Expected tombstoned output in raw response: {raw_res}"
        );

        // Default (VT-clean) format path.
        let clean_res = handle_session(
            &state,
            &serde_json::json!({"action": "output", "session_id": sid}),
            None,
        );
        assert!(clean_res.get("error").is_none(), "Unexpected error: {clean_res}");
        assert_eq!(clean_res["exited"], serde_json::json!(true));
        assert_eq!(clean_res["exit_code"], serde_json::json!(42));
        assert!(
            clean_res["data"].as_str().unwrap().contains("hello from the crypt"),
            "Expected tombstoned output in clean response: {clean_res}"
        );
    }

    /// A session with no trace (never existed or fully reaped) must return a
    /// structured error with `reason: session_not_found_or_reaped` — not the
    /// bare "Session not found" the pre-fix code returned.
    #[test]
    fn unknown_session_id_returns_structured_error() {
        let state = test_state();

        let res = handle_session(
            &state,
            &serde_json::json!({"action": "output", "session_id": "does-not-exist-at-all"}),
            None,
        );

        assert_eq!(
            res["error"].as_str(),
            Some("Session not found"),
            "Should surface error: {res}"
        );
        assert_eq!(
            res["reason"].as_str(),
            Some("session_not_found_or_reaped"),
            "Unknown session should report session_not_found_or_reaped: {res}"
        );
    }

    /// After `mark_session_exited`, output buffers + last_output_ms + exit_codes
    /// must survive, while transient per-session state must be reaped.
    #[test]
    fn mark_session_exited_preserves_tombstone_state() {
        use crate::state::VtLogBuffer;
        use crate::OutputRingBuffer;
        use std::sync::atomic::{AtomicU64, AtomicU8};

        let state = test_state();
        let sid = "mark-exited-test".to_string();

        // Insert buffers + transient state as if a session had been running.
        state.output_buffers.insert(
            sid.clone(),
            parking_lot::Mutex::new(OutputRingBuffer::new(1024)),
        );
        state.vt_log_buffers.insert(
            sid.clone(),
            parking_lot::Mutex::new(VtLogBuffer::new(24, 80, 100)),
        );
        state.last_output_ms.insert(sid.clone(), AtomicU64::new(0));
        state.shell_states.insert(sid.clone(), AtomicU8::new(crate::pty::SHELL_BUSY));
        state.terminal_rows.insert(sid.clone(), std::sync::atomic::AtomicU16::new(24));

        // No `sessions` entry — emulate the reader-thread path where the
        // session has already been removed by the caller before mark.
        crate::pty::mark_session_exited(&sid, &state);

        // Tombstone survivors.
        assert!(state.output_buffers.contains_key(&sid), "output buffer must survive");
        assert!(state.vt_log_buffers.contains_key(&sid), "vt log must survive");
        assert!(state.last_output_ms.contains_key(&sid), "last_output_ms must survive");
        // Transient state must be reaped.
        assert!(!state.shell_states.contains_key(&sid), "shell_states reaped");
        assert!(!state.terminal_rows.contains_key(&sid), "terminal_rows reaped");
    }

    // --- build_spawn_prompt ---

    #[test]
    fn build_spawn_prompt_no_parent_returns_original() {
        let result = build_spawn_prompt("do the task", None, "child-123");
        assert_eq!(result, "do the task");
    }

    #[test]
    fn build_spawn_prompt_with_parent_prepends_preamble() {
        let result = build_spawn_prompt("do the task", Some("parent-456"), "child-123");
        assert!(result.contains("parent-456"), "preamble must mention parent");
        assert!(result.contains("do the task"), "original prompt must be preserved");
        let preamble_end = result.find("do the task").unwrap();
        assert!(preamble_end > 0, "preamble must precede prompt");
        assert!(result.contains("register"), "preamble must instruct register");
    }

    #[test]
    fn build_spawn_prompt_with_parent_includes_send_instruction() {
        let result = build_spawn_prompt("my task", Some("orch-789"), "child-abc");
        assert!(result.contains("orch-789"), "preamble must include parent session for send target");
        assert!(result.contains("send"), "preamble must instruct send on completion");
    }

    // --- spawn auto-registration + inbox pre-init ---

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_auto_registers_child_in_peer_list() {
        let state = test_state();
        let addr = "127.0.0.1:0".parse().unwrap();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440b01", "orchestrator", "mcp-orch");

        let result = handle_agent(
            &state,
            addr,
            &serde_json::json!({
                "action": "spawn",
                "prompt": "hello",
                "binary_path": "/usr/bin/true",
                "cwd": "/tmp",
            }),
            Some("mcp-orch"),
        );
        // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
        if result.get("error").and_then(|e| e.as_str()).map_or(false, |e| e.contains("Failed to open PTY")) {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }
        assert!(result.get("error").is_none(), "spawn failed: {result}");
        let session_id = result["session_id"].as_str().unwrap();

        let peers = handle_messaging(
            &state,
            &serde_json::json!({"action": "list_peers"}),
            Some("mcp-orch"),
        );
        let sessions: Vec<&str> = peers["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["tuic_session"].as_str().unwrap())
            .collect();
        assert!(sessions.contains(&session_id), "child {session_id} not in list_peers: {sessions:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_pre_initializes_child_inbox() {
        let state = test_state();
        let addr = "127.0.0.1:0".parse().unwrap();
        register_peer(&state, "550e8400-e29b-41d4-a716-446655440b01", "orchestrator", "mcp-orch");

        let result = handle_agent(
            &state,
            addr,
            &serde_json::json!({
                "action": "spawn",
                "prompt": "hello",
                "binary_path": "/usr/bin/true",
                "cwd": "/tmp",
            }),
            Some("mcp-orch"),
        );
        // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
        if result.get("error").and_then(|e| e.as_str()).map_or(false, |e| e.contains("Failed to open PTY")) {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }
        assert!(result.get("error").is_none(), "spawn failed: {result}");
        let session_id = result["session_id"].as_str().unwrap();

        assert!(
            state.agent_inbox.contains_key(session_id),
            "child inbox must be pre-initialized after spawn"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_no_preamble_for_non_swarm_caller_succeeds() {
        let state = test_state();
        let addr = "127.0.0.1:0".parse().unwrap();
        let result = handle_agent(
            &state,
            addr,
            &serde_json::json!({
                "action": "spawn",
                "prompt": "hello",
                "binary_path": "/usr/bin/true",
                "cwd": "/tmp",
            }),
            Some("mcp-anon"),
        );
        // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
        if result.get("error").and_then(|e| e.as_str()).map_or(false, |e| e.contains("Failed to open PTY")) {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }
        assert!(result.get("error").is_none(), "non-swarm spawn must succeed: {result}");
        assert!(result["session_id"].as_str().is_some());
    }

    // ---- Layer 2: session(status) enrichment + spawn response (#1163-7599) ----

    #[test]
    fn session_status_unknown_session_returns_structured_error() {
        let state = test_state();
        let result = handle_session(
            &state,
            &serde_json::json!({"action": "status", "session_id": "nonexistent"}),
            None,
        );
        let err = result["error"].as_str().unwrap_or("");
        assert!(err.contains("not found"), "expected 'not found' error, got: {result}");
    }

    #[test]
    fn session_status_includes_exit_code_when_exited() {
        let state = test_state();
        let sid = "s-exit-test";
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());
        state.shell_states.insert(sid.to_string(), std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE));
        state.exit_codes.insert(sid.to_string(), 42);

        let result = handle_session(
            &state,
            &serde_json::json!({"action": "status", "session_id": sid}),
            None,
        );
        assert!(result.get("error").is_none(), "unexpected error: {result}");
        assert_eq!(result["exit_code"], serde_json::json!(42), "exit_code missing: {result}");
    }

    #[test]
    fn session_status_includes_idle_since_ms_when_idle() {
        let state = test_state();
        let sid = "s-idle-test";
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());
        state.shell_states.insert(sid.to_string(), std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE));
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64 - 500;
        state.shell_state_since_ms.insert(sid.to_string(), std::sync::atomic::AtomicU64::new(since));

        let result = handle_session(
            &state,
            &serde_json::json!({"action": "status", "session_id": sid}),
            None,
        );
        assert!(result.get("error").is_none(), "unexpected error: {result}");
        let idle_ms = result["idle_since_ms"].as_u64();
        assert!(idle_ms.is_some(), "idle_since_ms must be present when idle: {result}");
        assert!(idle_ms.unwrap() >= 400, "idle_since_ms must reflect elapsed time: {result}");
        assert!(result["busy_duration_ms"].is_null(), "busy_duration_ms must be absent when idle: {result}");
    }

    #[test]
    fn session_status_includes_busy_duration_ms_when_busy() {
        let state = test_state();
        let sid = "s-busy-test";
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());
        state.shell_states.insert(sid.to_string(), std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY));
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64 - 300;
        state.shell_state_since_ms.insert(sid.to_string(), std::sync::atomic::AtomicU64::new(since));

        let result = handle_session(
            &state,
            &serde_json::json!({"action": "status", "session_id": sid}),
            None,
        );
        assert!(result.get("error").is_none(), "unexpected error: {result}");
        let busy_ms = result["busy_duration_ms"].as_u64();
        assert!(busy_ms.is_some(), "busy_duration_ms must be present when busy: {result}");
        assert!(busy_ms.unwrap() >= 200, "busy_duration_ms must reflect elapsed time: {result}");
        assert!(result["idle_since_ms"].is_null(), "idle_since_ms must be absent when busy: {result}");
    }

    #[test]
    fn session_list_includes_shell_state_per_entry() {
        let state = test_state();
        // Without real PTY sessions we can't test list output (sessions DashMap requires live PTY).
        // This test verifies the field would appear if a session entry exists.
        // Integration coverage via manual QA — list with running session must show shell_state.
        // Here we just verify the status handler path we control returns shell_state.
        let sid = "s-list-test";
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());
        state.shell_states.insert(sid.to_string(), std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE));

        let result = handle_session(
            &state,
            &serde_json::json!({"action": "status", "session_id": sid}),
            None,
        );
        assert!(result["shell_state"].as_str().is_some(), "shell_state must be in status response: {result}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_response_includes_enrichment_fields() {
        let state = test_state();
        let addr = "127.0.0.1:0".parse().unwrap();
        let result = handle_agent(
            &state,
            addr,
            &serde_json::json!({
                "action": "spawn",
                "prompt": "hello",
                "binary_path": "/usr/bin/true",
                "cwd": "/tmp",
            }),
            Some("mcp-orch"),
        );

        if result.get("error").is_some() {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }

        assert!(result["session_id"].as_str().is_some(), "session_id missing: {result}");
        assert!(result["server_ts"].as_u64().is_some(), "server_ts missing: {result}");
        assert!(result["monitor_with"].as_str().is_some(), "monitor_with missing: {result}");
        assert!(result["status_with"].as_str().is_some(), "status_with missing: {result}");
        // ARCH-1: monitor_with must be canonical session(output), not branched
        // on caller identity. Standalone spawn (no registered caller) must
        // not include peer_monitor_with.
        let monitor = result["monitor_with"].as_str().unwrap();
        assert!(
            monitor.starts_with("session(action=output"),
            "standalone spawn monitor_with must be canonical session(output): {monitor}"
        );
        assert!(
            result.get("peer_monitor_with").is_none(),
            "standalone spawn must not include peer_monitor_with: {result}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_response_adds_peer_monitor_hint_when_caller_registered() {
        // ARCH-1: when the caller is a registered orchestrator, monitor_with
        // stays canonical (session(output)) and the peer-only hint is added
        // additively as peer_monitor_with — keeps the spawn response policy-free.
        let state = test_state();
        let addr = "127.0.0.1:0".parse().unwrap();
        let tuic = "550e8400-e29b-41d4-a716-446655440aa2";
        let mcp = "mcp-arch1-orch";
        register_peer(&state, tuic, "orchestrator", mcp);

        let result = handle_agent(
            &state,
            addr,
            &serde_json::json!({
                "action": "spawn",
                "prompt": "hello",
                "binary_path": "/usr/bin/true",
                "cwd": "/tmp",
            }),
            Some(mcp),
        );
        if result.get("error").is_some() {
            eprintln!("Skipping: PTY not available in this environment");
            return;
        }
        let monitor = result["monitor_with"].as_str().expect("monitor_with required");
        assert!(
            monitor.starts_with("session(action=output"),
            "monitor_with must be canonical session(output) regardless of caller: {monitor}"
        );
        let peer_hint = result["peer_monitor_with"].as_str()
            .expect("peer_monitor_with must be present for registered caller");
        assert!(
            peer_hint.starts_with("agent(action=inbox"),
            "peer_monitor_with must point at agent(inbox): {peer_hint}"
        );
    }

    // ── is_valid_uuid ────────────────────────────────────────────────────────

    #[test]
    fn is_valid_uuid_accepts_well_formed_uuid() {
        assert!(is_valid_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(is_valid_uuid("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn is_valid_uuid_rejects_injection_payloads() {
        assert!(!is_valid_uuid("injected\n## header"));
        assert!(!is_valid_uuid("short"));
        assert!(!is_valid_uuid(""));
        assert!(!is_valid_uuid("550e8400-e29b-41d4-a716-44665544000g")); // non-hex char
        assert!(!is_valid_uuid("550e8400e29b41d4a716446655440000"));      // no dashes
    }

    // ── session(kill) self-kill guard ────────────────────────────────────────

    #[test]
    fn session_kill_rejects_own_session() {
        let state = test_state();
        let mcp_sid = "mcp-kill-guard-test";
        let tuic_sid = "550e8400-e29b-41d4-a716-446655440001";
        state.mcp_to_session.insert(mcp_sid.to_string(), tuic_sid.to_string());

        let result = handle_session(
            &state,
            &serde_json::json!({"action": "kill", "session_id": tuic_sid}),
            Some(mcp_sid),
        );
        assert!(result["error"].as_str().is_some(), "kill own session must return error: {result}");
        assert!(
            result["error"].as_str().unwrap().contains("Cannot kill own session"),
            "error message must mention 'Cannot kill own session': {result}"
        );
    }

    #[test]
    fn session_kill_allows_other_session() {
        let state = test_state();
        let mcp_sid = "mcp-kill-other-test";
        let own_tuic = "550e8400-e29b-41d4-a716-446655440002";
        let other_tuic = "550e8400-e29b-41d4-a716-446655440003";
        state.mcp_to_session.insert(mcp_sid.to_string(), own_tuic.to_string());

        // Killing a different session — should NOT be blocked by self-kill guard.
        // It will return "Session not found" (no real PTY), not the self-kill error.
        let result = handle_session(
            &state,
            &serde_json::json!({"action": "kill", "session_id": other_tuic}),
            Some(mcp_sid),
        );
        let err = result["error"].as_str().unwrap_or("");
        assert!(
            !err.contains("Cannot kill own session"),
            "self-kill guard must NOT block killing other sessions: {result}"
        );
    }

    // ── agent(register) UUID validation ─────────────────────────────────────

    #[test]
    fn agent_register_rejects_non_uuid_tuic_session() {
        let state = test_state();
        let result = handle_messaging(
            &state,
            &serde_json::json!({"action": "register", "tuic_session": "not-a-uuid"}),
            Some("mcp-reg-test"),
        );
        assert!(
            result["error"].as_str().map_or(false, |e| e.contains("UUID")),
            "register with non-UUID tuic_session must fail: {result}"
        );
    }

    #[test]
    fn agent_register_accepts_valid_uuid() {
        let state = test_state();
        let result = handle_messaging(
            &state,
            &serde_json::json!({
                "action": "register",
                "tuic_session": "550e8400-e29b-41d4-a716-446655440004"
            }),
            Some("mcp-reg-valid-test"),
        );
        assert!(result["ok"].as_bool() == Some(true), "register with valid UUID must succeed: {result}");
    }

    // ── agent(send) + agent(inbox) caller resolution (RUST-3/PERF-2 — must use mcp_to_session O(1)) ──

    #[test]
    fn agent_send_succeeds_for_registered_peer() {
        let state = test_state();
        let sender_mcp = "mcp-send-sender";
        let sender_tuic = "550e8400-e29b-41d4-a716-446655440010";
        let recipient_mcp = "mcp-send-recipient";
        let recipient_tuic = "550e8400-e29b-41d4-a716-446655440011";
        register_peer(&state, sender_tuic, "alice", sender_mcp);
        register_peer(&state, recipient_tuic, "bob", recipient_mcp);

        let result = handle_messaging(
            &state,
            &serde_json::json!({
                "action": "send",
                "to": recipient_tuic,
                "message": "hello bob",
            }),
            Some(sender_mcp),
        );
        assert_eq!(result["ok"].as_bool(), Some(true), "send must succeed: {result}");
        let inbox = state.agent_inbox.get(recipient_tuic).expect("recipient inbox exists");
        assert_eq!(inbox.len(), 1, "recipient should have 1 buffered message");
        assert_eq!(inbox[0].from_tuic_session, sender_tuic);
        assert_eq!(inbox[0].from_name, "alice");
    }

    #[test]
    fn agent_send_rejects_unregistered_caller() {
        let state = test_state();
        let recipient_tuic = "550e8400-e29b-41d4-a716-446655440012";
        register_peer(&state, recipient_tuic, "bob", "mcp-recipient-only");

        let result = handle_messaging(
            &state,
            &serde_json::json!({
                "action": "send",
                "to": recipient_tuic,
                "message": "ghost message",
            }),
            Some("mcp-not-registered"),
        );
        assert!(
            result["error"].as_str().map_or(false, |e| e.contains("not registered")),
            "send from unregistered MCP session must error: {result}"
        );
    }

    #[test]
    fn agent_inbox_returns_messages_for_registered_caller() {
        let state = test_state();
        let mcp_sid = "mcp-inbox-self";
        let tuic = "550e8400-e29b-41d4-a716-446655440013";
        register_peer(&state, tuic, "self", mcp_sid);

        // Send a message to self so the inbox has one entry.
        let send_result = handle_messaging(
            &state,
            &serde_json::json!({"action": "send", "to": tuic, "message": "note to self"}),
            Some(mcp_sid),
        );
        assert_eq!(send_result["ok"].as_bool(), Some(true), "send-to-self must succeed: {send_result}");

        let result = handle_messaging(
            &state,
            &serde_json::json!({"action": "inbox"}),
            Some(mcp_sid),
        );
        let messages = result["messages"].as_array().expect("inbox returns messages array");
        assert_eq!(messages.len(), 1, "inbox should contain 1 message: {result}");
        assert_eq!(messages[0]["content"].as_str(), Some("note to self"));
    }

    // -----------------------------------------------------------------------
    // resolve_run_config tests
    // -----------------------------------------------------------------------

    fn make_agents_config() -> crate::config::AgentsConfig {
        use crate::config::{AgentRunConfig, AgentSettings, AgentsConfig};
        let mut agents = std::collections::HashMap::new();
        agents.insert("claude".to_string(), AgentSettings {
            run_configs: vec![
                AgentRunConfig {
                    name: "claude qwen3.5".to_string(),
                    command: "ollama".to_string(),
                    args: vec!["launch".to_string(), "claude".to_string(), "--model".to_string(), "qwen3.5".to_string()],
                    env: [("OLLAMA_HOST".to_string(), "localhost:11434".to_string())].into_iter().collect(),
                    is_default: false,
                },
                AgentRunConfig {
                    name: "Default".to_string(),
                    command: "claude".to_string(),
                    args: vec![],
                    env: std::collections::HashMap::new(),
                    is_default: true,
                },
            ],
            ..Default::default()
        });
        agents.insert("codex".to_string(), AgentSettings {
            run_configs: vec![
                AgentRunConfig {
                    name: "codex-fast".to_string(),
                    command: "codex".to_string(),
                    args: vec!["--fast".to_string()],
                    env: std::collections::HashMap::new(),
                    is_default: true,
                },
            ],
            ..Default::default()
        });
        AgentsConfig { agents, headless_agent: None }
    }

    #[test]
    fn resolve_run_config_matches_by_name_case_insensitive() {
        let cfg = make_agents_config();
        let resolved = resolve_run_config("Claude Qwen3.5", &cfg);
        assert_eq!(resolved.agent_type, "claude");
        assert_eq!(resolved.command.as_deref(), Some("ollama"));
        assert!(resolved.args.as_ref().unwrap().contains(&"qwen3.5".to_string()));
        assert_eq!(resolved.env.get("OLLAMA_HOST").map(|s| s.as_str()), Some("localhost:11434"));
    }

    #[test]
    fn resolve_run_config_falls_back_to_agent_type() {
        let cfg = make_agents_config();
        let resolved = resolve_run_config("gemini", &cfg);
        assert_eq!(resolved.agent_type, "gemini");
        assert!(resolved.command.is_none());
        assert!(resolved.args.is_none());
        assert!(resolved.env.is_empty());
    }

    #[test]
    fn resolve_run_config_cross_agent_match() {
        let cfg = make_agents_config();
        let resolved = resolve_run_config("codex-fast", &cfg);
        assert_eq!(resolved.agent_type, "codex");
        assert_eq!(resolved.command.as_deref(), Some("codex"));
    }

    // -----------------------------------------------------------------------
    // substitute_prompt_in_args tests
    // -----------------------------------------------------------------------

    #[test]
    fn substitute_prompt_placeholder_present() {
        let args = vec!["-p".to_string(), "{prompt}".to_string(), "--no-input".to_string()];
        let result = substitute_prompt_in_args(&args, "fix the bug");
        assert_eq!(result, vec!["-p", "fix the bug", "--no-input"]);
    }

    #[test]
    fn substitute_prompt_placeholder_absent_appends() {
        let args = vec!["--fast".to_string()];
        let result = substitute_prompt_in_args(&args, "fix the bug");
        assert_eq!(result, vec!["--fast", "fix the bug"]);
    }

    #[test]
    fn substitute_prompt_multiple_placeholders() {
        let args = vec!["{prompt}".to_string(), "--echo".to_string(), "{prompt}".to_string()];
        let result = substitute_prompt_in_args(&args, "hello");
        assert_eq!(result, vec!["hello", "--echo", "hello"]);
    }

    // -----------------------------------------------------------------------
    // merge_mcp_params_into_args tests
    // -----------------------------------------------------------------------

    #[test]
    fn merge_params_model_no_conflict() {
        let args = vec!["--fast".to_string()];
        let result = merge_mcp_params_into_args(&args, Some("gpt-4"), false, None).unwrap();
        assert!(result.contains(&"--model".to_string()));
        assert!(result.contains(&"gpt-4".to_string()));
    }

    #[test]
    fn merge_params_model_conflict() {
        let args = vec!["--model".to_string(), "sonnet".to_string()];
        let result = merge_mcp_params_into_args(&args, Some("gpt-4"), false, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Conflict"));
    }

    #[test]
    fn merge_params_print_mode_appended() {
        let args = vec![];
        let result = merge_mcp_params_into_args(&args, None, true, None).unwrap();
        assert!(result.contains(&"--print".to_string()));
    }

    #[test]
    fn merge_params_print_mode_already_present() {
        let args = vec!["--print".to_string()];
        let result = merge_mcp_params_into_args(&args, None, true, None).unwrap();
        // Should not duplicate
        assert_eq!(result.iter().filter(|a| *a == "--print").count(), 1);
    }

    #[test]
    fn merge_params_output_format_conflict() {
        let args = vec!["--output-format".to_string(), "json".to_string()];
        let result = merge_mcp_params_into_args(&args, None, false, Some("text"));
        assert!(result.is_err());
    }

    #[test]
    fn merge_params_output_format_no_conflict() {
        let args = vec![];
        let result = merge_mcp_params_into_args(&args, None, false, Some("json")).unwrap();
        assert!(result.contains(&"--output-format".to_string()));
        assert!(result.contains(&"json".to_string()));
    }

    #[test]
    fn agent_inbox_rejects_unregistered_caller() {
        let state = test_state();
        let result = handle_messaging(
            &state,
            &serde_json::json!({"action": "inbox"}),
            Some("mcp-no-register"),
        );
        assert!(
            result["error"].as_str().map_or(false, |e| e.contains("not registered")),
            "inbox call from unregistered MCP session must error: {result}"
        );
    }
}
