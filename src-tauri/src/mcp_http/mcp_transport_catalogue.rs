use crate::AppState;
use std::net::SocketAddr;
use std::sync::Arc;

use super::mcp_transport::client_requires_meta_tools;
use super::mcp_transport::detect_claude_code_client;
use super::mcp_transport::handle_mcp_tool_call_with_context;
use super::mcp_transport::mark_upstream_tool_result;
use super::mcp_transport::resolve_agent_type;
use super::mcp_transport::resolve_marker_flags;

/// Build server instructions for the MCP initialize response.
/// Tells the connecting agent what tools are available, which repos are managed,
/// and what sessions are currently active so it can orient itself.
pub(super) fn build_mcp_instructions(state: &Arc<AppState>, client_name: Option<&str>) -> String {
    let collapse_tools = state.config.read().collapse_tools;
    build_mcp_instructions_for_mode(state, client_name, collapse_tools)
}

/// The live state the instructions render: managed repos, open sessions and
/// connected peers. Gathered by [`instruction_context`] and rendered by
/// [`render_mcp_instructions`], which reads nothing else.
///
/// The split exists so the rendered size is reproducible. `load_repo_settings`
/// reads the user's `repositories.json` and the session list reads live PTYs,
/// so a renderer that gathered its own inputs could only be measured against
/// whatever happened to be open on the machine running the test.
pub(super) struct InstructionContext {
    /// `(display name, absolute path)`, sorted by path.
    pub(super) repos: Vec<(String, String)>,
    /// `(short session id, cwd, worktree branch)`, already defaulted to `—`.
    pub(super) sessions: Vec<(String, String, String)>,
    pub(super) peer_count: usize,
}

fn instruction_context(state: &Arc<AppState>) -> InstructionContext {
    let repo_settings = crate::config::load_repo_settings();
    let mut repos: Vec<_> = repo_settings.repos.iter().collect();
    repos.sort_by_key(|(path, _)| path.to_string());
    let repos = repos
        .into_iter()
        .map(|(path, entry)| {
            let name = if entry.display_name.is_empty() {
                path.rsplit('/').next().unwrap_or(path)
            } else {
                &entry.display_name
            };
            (name.to_string(), path.to_string())
        })
        .collect();

    let sessions = state
        .session_maps
        .sessions
        .iter()
        .map(|entry| {
            let id = entry.key().clone();
            let session = entry.value().lock();
            (
                id[..8.min(id.len())].to_string(),
                session.cwd.clone().unwrap_or_else(|| "—".to_string()),
                session
                    .worktree
                    .as_ref()
                    .and_then(|w| w.branch.clone())
                    .unwrap_or_else(|| "—".to_string()),
            )
        })
        .collect();

    InstructionContext {
        repos,
        sessions,
        peer_count: state.peer_agents.len(),
    }
}

pub(super) fn build_mcp_instructions_for_mode(
    state: &Arc<AppState>,
    client_name: Option<&str>,
    collapse_tools: bool,
) -> String {
    render_mcp_instructions(
        client_name,
        collapse_tools,
        resolve_marker_flags(state, client_name),
        crate::progress::progress_tracking_enabled(state, resolve_agent_type(client_name)),
        &instruction_context(state),
    )
}

/// Render the initialize instructions.
///
/// These are one of **two** instruction surfaces, and the smaller one: a client
/// such as Codex never surfaces `instructions` at all, while every client
/// receives the tool descriptions. So anything a tool description already says
/// is deliberately absent here — see `every_documented_action_constant_matches_schema_and_description`
/// and `instructions_do_not_repeat_what_tool_descriptions_already_say`. What is
/// left is what belongs to no single tool: the wire protocol markers, the rules
/// that forbid a *non-TUIC* tool, and the live state below.
pub(super) fn render_mcp_instructions(
    client_name: Option<&str>,
    collapse_tools: bool,
    markers: (bool, bool),
    progress_tracking: bool,
    ctx: &InstructionContext,
) -> String {
    let ver = env!("CARGO_PKG_VERSION");
    let mut out = String::with_capacity(2048);

    // ── Identity ──────────────────────────────────────────────────────
    out.push_str(&format!("# TUICommander v{ver}\n\n"));

    // ── TUIC protocol (mandatory line markers) ─────────────────────────
    // Wire-level tokens parsed by the host TUI. Concision rules do NOT apply —
    // dropping a marker breaks the UI (stale tab title, missing suggestion bar).
    let (show_intent, show_suggest) = markers;
    out.push_str("## TUIC Protocol — Required Output Markers\n\n");
    out.push_str("Required even under concision rules; omission breaks the UI.\n\n");
    out.push_str(&format!(
        "- `ack` — exactly once per MCP connection or reconnect, the first assistant message MUST start: `TUICommander v{ver} is connected.` Never repeat it on each conversational turn.\n"
    ));
    if show_intent {
        out.push_str("- `intent: <desc> (<title>)` on its own line, at the start of every user task and on each material work-phase change. Describe the work currently in progress in present tense; `<title>` ≤3 words, spaces not hyphens.\n");
    }
    if show_suggest {
        out.push_str("- `suggest:` — after task done: `suggest: [ A | B | C ]` — bracket the WHOLE list; EXACTLY 3 items separated by `|`, each ≤40 chars.\n");
    }
    // Always on: the answers-only view is a per-terminal toggle with no setting
    // to switch the marker off, and `answersTurn.ts` anchors on a row that
    // starts with the marker followed by a space.
    out.push_str("- `💬 ` — prefix every sentence that directly answers the user's question with `💬 ` at row start (marker + space). Exclude status, tool and hook/task notices.\n");
    out.push('\n');

    // ── Cross-tool rules ─────────────────────────────────────────────
    // NOT a tool catalogue: `tools/list` already carries every name, action and
    // schema in the same turn, and restating it here bought a second copy for
    // the clients that read instructions and nothing at all for the clients that
    // do not. What survives is the pair of rules that no single tool description
    // owns — one forbids a tool that is not ours, the other spans two calls.
    out.push_str("## Tools\n\n");
    if collapse_tools {
        out.push_str("Discover/invoke via `search_tools` / `get_tool_schema` / `call_tool`; read their descriptions.\n\n");
        out.push_str("**Worktrees:** use `repo action=worktree_create`/`worktree_remove` for TUIC tracking/PTY spawn; never `git worktree add/remove`.\n\n");
        // Kept verbatim in both modes. It is not a restatement of the `session`
        // description: agents split the text and the Enter into two calls, or
        // polled after submitting, until this line existed.
        out.push_str("**Submit:** `call_tool tool_name=session arguments={action:submit,session_id,input}` once; never split text/Enter; never poll.\n\n");
    } else {
        out.push_str("Read each tool's description for actions and rules.\n\n");
        out.push_str("**Worktrees:** use `repo action=worktree_create`/`worktree_remove` for TUIC tracking/PTY spawn; never `git worktree add/remove`.\n\n");
        out.push_str("**Submit:** `session action=submit session_id=<id> input=<text>` once; never split text/Enter; never poll.\n\n");
    }

    // ── Progress — an obligation, not a capability ───────────────────
    // This is the one place the reporting duty is stated as a duty. The tool
    // description says what the tool does if you call it; nothing there says
    // you must. The shipped version had only the description, and 39
    // repositories recorded zero events. Rendered only when the flag is on, so
    // a listed tool without an obligation and an obligation naming an unlisted
    // tool are both unreachable.
    if progress_tracking {
        out.push_str("## Progress — mandatory\n\n");
        out.push_str(
            "Call `progress` to report work while the user is away. \
             `type=done` for finished `intent:` work or work continuing alongside user action; \
             `type=blocked` only when you stop and wait because you need the user.\n\n",
        );
    }

    // ── Multi-agent work — what the tool descriptions cannot say ─────
    // The orchestration primer, the identity rules and the reporting contract
    // all live in the `agent` description, which reaches every client. Only
    // three things are left here: how many peers are live (state, not prose),
    // the worktree entry point, and the Claude-Code-only delegation hint, which
    // is conditioned on the connecting client and so cannot sit in a static
    // description at all.
    let is_claude_code = detect_claude_code_client(client_name);
    out.push_str("## Multi-Agent Work\n\n");
    if ctx.peer_count > 0 {
        out.push_str(&format!(
            "**{}** peer agent(s) connected. Orchestrate them with the `agent` tool; read its description first.\n",
            ctx.peer_count
        ));
    }
    out.push_str("- **Isolated branches:** `repo action=worktree_create spawn_session=true`.\n");
    out.push_str("- **Mail:** default normal; `agent send urgency=urgent` requests a course change before the next step.\n");
    if is_claude_code {
        out.push_str("- **Single isolated task (CC only):** `repo action=worktree_create` then delegate via returned `cc_agent_hint` (absolute paths). ONLY valid use of native Agent/Task.\n");
    }
    out.push('\n');

    // ── Dynamic: repos ──────────────────────────────────────────────
    if !ctx.repos.is_empty() {
        out.push_str("## Repos\n\n");
        for (name, path) in &ctx.repos {
            out.push_str(&format!("- **{name}** `{path}`\n"));
        }
        out.push('\n');
    }

    // ── Dynamic: sessions ───────────────────────────────────────────
    if !ctx.sessions.is_empty() {
        out.push_str("## Sessions\n\n");
        for (short_id, cwd, branch) in &ctx.sessions {
            out.push_str(&format!("- `{short_id}` {cwd} ({branch})\n"));
        }
        out.push('\n');
    }

    out
}

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
pub(super) fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path).map_err(|msg| serde_json::json!({"error": msg}))
}

pub(super) const SESSION_ACTIONS: &str = "list, create, submit, input, output, resize, rename, keep_open, suspend, close, kill, pause, resume, status, wait, declare_worktree";
pub(super) const AGENT_ACTIONS: &str = "spawn, register, list_peers, send, inbox, wait";
pub(super) const REPO_ACTIONS: &str = "list, add, active, status, branch_integrations, branch_integration, worktree_list, worktree_lifecycle, worktree_create, worktree_remove, orphan_cleanup_answer, branch_delete, progress_list";
pub(super) const UI_ACTIONS: &str = "tab, toast, confirm, screenshot";
pub(super) const TASK_ACTIONS: &str = "get, cancel";
pub(super) const CONFIG_ACTIONS: &str = "get, save, list_prompts, load_prompt, save_prompt";
pub(super) const DEBUG_ACTIONS: &str = "agent_detection, logs, sessions, invoke_js, help";
#[cfg(any(feature = "dictation", test))]
pub(super) const VOICE_ACTIONS: &str = "speak, stop, status";
pub(super) const REMOTE_ACTIONS: &str = "preview, update";

pub(super) fn removed_native_action_replacement(tool: &str, action: &str) -> Option<&'static str> {
    match (tool, action) {
        ("agent", "detect") => Some("GET /agents"),
        ("agent", "stats") => Some("GET /stats"),
        ("agent", "metrics") => Some("GET /metrics"),
        ("session", "process_stats") => Some("GET /process/stats"),
        ("repo", "prs") => Some("GET /repo/prs"),
        ("repo", "issues") => Some("GET /repo/issues"),
        ("repo", "close_issue") => Some("POST /repo/issues/close"),
        ("repo", "reopen_issue") => Some("POST /repo/issues/reopen"),
        ("repo", "ci_logs") => Some("GET /repo/ci-failure-logs"),
        _ => None,
    }
}

/// User-facing Settings summaries, separate from MCP descriptions and schemas.
pub(super) const NATIVE_TOOL_SUMMARIES: &[(&str, &str)] = &[
    ("secret", "Request sensitive values from the user securely."),
    (
        "telegram",
        "Send messages and receive replies through Telegram.",
    ),
    (
        "session",
        "Create terminals, send input, and read their output.",
    ),
    ("agent", "Launch agents and exchange messages with them."),
    ("task", "Track background work and retrieve its results."),
    (
        "remote",
        "Manage connections to remote TUICommander instances.",
    ),
    ("repo", "Inspect repositories and manage their worktrees."),
    ("story", "Read project stories and update their progress."),
    (
        "automations",
        "Create and manage scheduled agent definitions.",
    ),
    (
        "workflow_story_create",
        "Create a story for a project workflow.",
    ),
    (
        "workflow_report",
        "Report the outcome of a workflow attempt.",
    ),
    (
        "workflow_launch",
        "Launch an agent for a workflow assignment.",
    ),
    ("progress", "Record and review project progress."),
    (
        "ui",
        "Show notifications and ask the user for confirmation.",
    ),
    (
        "plugin_dev_guide",
        "Read the guide for developing TUICommander plugins.",
    ),
    (
        "workflow_run",
        "Start workflow graphs and inspect their execution history.",
    ),
    ("config", "Read and update application settings."),
    ("debug", "Inspect application logs and runtime state."),
    (
        "voice",
        "Speak replies in the active hands-free conversation.",
    ),
];

/// Full MCP tool definitions — the one native tool family.
///
/// This returns the unfiltered schema list. Public listing/search paths MUST
/// route through [`filtered_native_tools`] to honour `disabled_native_tools`
/// and `progress_tracking`. Leaking the raw list to external clients exposes
/// tool metadata for gated tools.
pub(super) fn native_tool_definitions() -> serde_json::Value {
    let defs = serde_json::json!([
        crate::automations::mcp::tool_definition(),
        crate::secrets::tool_definition(),
        crate::telegram::tool_definition(),
        {
            "name": "session",
            "description": "PTY multiplexer (replaces tmux). Create terminals, send input (send-keys), read output (capture-pane), manage lifecycle.\n\nActions:\n- list: All active sessions and states in one call, including local and connected remote sessions. Remote rows carry connection_id and address=connection/session_id. Use for every global overview; never fan out per-session status calls. Returns display_name (assigned name), alias (independent repo-derived short address), tuic_session (the stable identity the tab persists), is_caller, shell_state (PTY activity), and agent_state (starting|working|awaiting_input|idle|completed; completed requires suggest marker). Absent optional fields are omitted, not null — background_work and standby appear only when true.\n\nEvery action that takes session_id accepts the PTY id, tuic_session, alias (e.g. tu-1), a unique short PTY-id prefix, or a unique display name.\n- create: New PTY. Returns {session_id}. Optional: cwd, shell, rows, cols.\n- submit: Submit one non-empty command to a confirmed-idle managed agent and wait internally for a bounded receipt. Use one call; never split text and Enter; never poll after it. Returns submission_id, submitted, write_state, acknowledged, retry_safe, turn_epoch, composer_state (tracked InputLineBuffer, not application state), and acknowledgement or a precise reason. Acknowledgement means child terminal movement after Enter, not semantic application acceptance. Never queues; partial composers, dialogs, busy agents, and older queued commands reject before writing.\n- input: Raw text/key compatibility surface. Send text and/or special_key; ok confirms PTY write only.\n- output: Read terminal output. Returns {data, cursor, scrollback_lines, oldest_offset, exited, exit_code}. Use as an anomaly fallback for a child that failed to send its result, not as the normal orchestration channel. The tail read omits an empty input box and everything below it (status line, HUD); format=raw keeps them. scrollback_lines = total lines in buffer (up to 10000); oldest_offset = first available line number. Patterns: (1) Snapshot: omit since_cursor, default limit=50 gives last 50 lines. (2) Delta read: since_cursor=<previous cursor> returns only new lines. (3) Navigate backwards: from_line=oldest_offset reads from the beginning of the buffer. (4) Arbitrary window: from_line=N, limit=50 reads any 50-line slice. Windowed reads report has_more and next_cursor; follow the continuation note to fetch retained output without rerunning the command. Raw pages use from_byte=oldest_offset then from_byte=next_cursor (source-byte positions). Evicted output cannot be recovered.\n- declare_worktree: Declare an existing linked worktree for this authenticated live caller. Requires worktree_path; rejects foreign sessions and repositories. Persists sidebar placement without changing shell cwd. Caller-bound MCP-only request, available via POST /mcp.\n- status: Session state; absent optional fields are omitted.\n- wait: Block (server-side) until session_id is idle or exited (until=idle|exited), or timeout_ms elapses. One cheap call instead of a status polling loop. Returns {met, timed_out, shell_state?, exit_code?}.\n- resize: Change PTY dimensions.\n- rename: Set the tab's display name. Requires name (non-empty). Sticky by default — protected from later OSC/intent title updates unless is_custom=false.\n- keep_open: Keep a managed child open by disabling idle closure with enabled=true; enabled=false restores automatic idle closure. Requires session_id.\n- suspend: End the tab's PTY and agent to free memory and CPU but keep the tab, restorable like after a TUIC restart; the user resumes it from the tab. Refused while the agent is working, a question awaits an answer, or a command runs. Not auto-standby, which only SIGSTOPs and keeps memory. Requires session_id.\n- close: Graceful shutdown (Ctrl+C, waits).\n- kill: Force SIGKILL (use when close fails).\n- pause: Pause output buffering. resume: Resume.",
            "inputSchema": { "type": "object", "properties": {
                "connection_id": { "type": "string", "description": "Configured remote connection qualifier (session list/output/submit; agent list_peers/send). Remote addresses also accept connection/id; local/id addresses the desktop hub." },
                "action": { "type": "string", "description": "One of: list, create, submit, input, output, status, wait, resize, rename, keep_open, suspend, close, kill, pause, resume, declare_worktree" },
                "session_id": { "type": "string", "description": "Session address — PTY id, tuic_session, alias, unique short PTY-id prefix, or unique display name. Ambiguous prefixes or names return an error. Required for submit, input, output, status, resize, rename, keep_open, suspend, close, kill, pause, resume, wait" },
                "name": { "type": "string", "description": "New tab display name, non-empty (action=rename, required)" },
                "is_custom": { "type": "boolean", "description": "action=rename, default true. true protects the name from later OSC/intent title updates; false lets them refine it." },
                "enabled": { "type": "boolean", "description": "Required for action=keep_open: true disables idle close for a managed child; false restores it." },
                "until": { "type": "string", "description": "Wait target: 'idle' or 'exited' (action=wait, default idle)" },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": 300000, "description": "action=submit: acknowledgement wait, clamped 250-10000ms, default 3000. action=wait: max wait, default 60000; values at or above 300000 run as 295000." },
                "input": { "type": "string", "description": "Non-empty command (action=submit) or raw text (action=input)" },
                "pty_description": { "type": ["string", "null"], "description": "Short orchestrator-supplied description shown above the PTY (action=submit or input)" },
                "special_key": { "type": "string", "description": "Special key: enter, tab, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+k, ctrl+u, ctrl+w, ctrl+r, up, down, left, right, home, end, backspace, delete, escape (action=input)" },
                "rows": { "type": "integer", "description": "Terminal rows (action=create or resize)" },
                "cols": { "type": "integer", "description": "Terminal cols (action=create or resize)" },
                "shell": { "type": "string", "description": "Shell binary path (action=create)" },
                "worktree_path": { "type": "string", "description": "Absolute existing linked worktree path (action=declare_worktree, required). Caller only; omit session_id." },
                "cwd": { "type": "string", "description": "Working directory (action=create)" },
                "limit": { "type": "integer", "description": "Max scrollback rows (raw: source bytes), default 50. Logical lines and UTF-8 codepoints stay whole, so pages can exceed this limit (action=output)" },
                "from_byte": { "type": "integer", "minimum": 0, "description": "Absolute source-byte offset for format=raw. Use oldest_offset to start, then next_cursor to continue retained output without rerunning a command (action=output)" },
                "from_line": { "type": "integer", "description": "Absolute line number to start reading from. Use oldest_offset from a previous response to read from the beginning of the buffer. Omit to read the tail (action=output)" },
                "format": { "type": "string", "description": "Output format: ANSI escape codes are stripped by default; pass 'raw' to preserve them (action=output)" },
                "since_cursor": { "type": "integer", "description": "Cursor from a previous output response (raw: source bytes; text: scrollback rows). Returns a bounded forward page; follow next_cursor while has_more. Omit for snapshot (action=output)" }
            }, "required": ["action"] }
        },
        {
            "name": "agent",
            "description": "AI agent orchestration. There is no separate swarm action: use these agent/session primitives to spawn and coordinate managed peers.\n\nOrchestration in 5 lines:\n1. Managed PTYs auto-bind from $TUIC_SESSION. A headerless external caller calls register without tuic_session to receive an MCP-scoped UUID, or supplies an explicit stable UUID to reclaim it.\n2. Spawn a named peer: spawn name=worker prompt=<task> [agent_type=codex|gemini|...] → {session_id, name}.\n3. Wait for it: agent action=wait (new mail; omit since, the cursor is kept server-side) or session action=wait session_id=<id> until=idle|exited. Cheap blocking call — do NOT poll in a loop. Both cap at 300s: for work that runs longer, or across a reconnect, poll the spawn's task_id with task action=get instead — the outcome is recorded even with nobody waiting.\n4. Talk to it: send to=<peer> message=<text> [urgency=normal|urgent]. Normal is the default; use urgent when the recipient must change course before its next step. Mail stays mail: the payload is never typed into the recipient's composer. It waits in the recipient's inbox, and an idle/completed recipient may be sent a payload-free generic `agent action=inbox` wake. Urgent sends a payload-free inbox notice to a safe busy Claude/Codex composer for the next tool boundary.\n5. Lifecycle notifications carry state only. Every worker must report task output or blockers with send; use session output only if a child anomalously failed to send.\n\nActions:\n- spawn: Launch agent in new PTY (localhost only). Optional name is assigned before prompt delivery. Returns {session_id, name, task_id, poll_interval_ms, server_ts, parent_session_id?}.\n- wait: Block until new inbox mail. Omit `since` — the server resumes from your last read position; pass it only to override (since=0 replays everything). Success inlines every retained fresh message (up to the 100-message inbox capacity) in chronological order. Every response carries next_since, timeout included. An active wait suppresses terminal wake.\n- register: Bind an external/headerless caller, or rename/set the repository path of an auto-bound managed peer. tuic_session is optional; omission generates a stable identity for this MCP connection. Reconnecting under a NEW uuid? Pass `replaces=<old_uuid>` or its inbox is stranded — the response reports superseded_identity, mail_migrated, and mail_stranded + identity_warning when the old identity still owns a live PTY (its mail is left alone). Check `terminal` in the response: false means no PTY notice can be typed into you; a subscribed ACP inbox can still notify you or wake idle ego. Otherwise consume mail with wait/inbox. Declare the orchestrator role with orchestrator=true and remove it with false; spawning a child never infers it, and omitting the field preserves the current role. The response reports mail_wake=managed_pty_lifecycle when a wake can reach you; external/headerless peers without a subscribed ACP inbox stay wait/inbox-only.\n- list_peers: List peers across the desktop mail hub and connected daemons. address is connection/peer_id (local/peer_id on the hub); connection_id selects one daemon. Returns tuic_session, name, orchestrator, plus alias and session_id for a peer that owns a live terminal. Optional: path filter. Absent fields are omitted.\n- send: Message any local or remote peer (requires to, message). Use to=connection/peer_id or connection_id with a daemon-local address. The owning daemon performs inbox delivery and wake; replies return through the desktop hub. `to` accepts the peer's tuic_session, PTY id, or terminal alias. `urgency` is normal (default) or urgent; use urgent when the recipient must change course before its next step. Urgent keeps the body in the inbox and writes only a notice to a safe busy Claude/Codex composer. `urgent_delivered` reports a PTY notice, subscribed ACP inbox update, inbox read, or waiter; false adds `urgent_fallback_reason` for queued mail. It does not prove model action or interrupt a tool. `delivered` and `delivery_path` describe the routing path; `recipient_state` appears only for a managed PTY.\n- inbox: Read up to 100 retained messages in FIFO order. Returns next_since and has_more; repeat while has_more is true. Optional: limit (default 100, max 100), since (omit to resume from the server-side cursor). On FIFO eviction, missed_count reports unread messages lost since the last inbox read.",
            "inputSchema": { "type": "object", "properties": {
                "connection_id": { "type": "string", "description": "Configured remote connection qualifier (session list/output/submit; agent list_peers/send). Remote addresses also accept connection/id; local/id addresses the desktop hub." },
                "action": { "type": "string", "description": "One of: spawn, wait, register, list_peers, send, inbox" },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": 300000, "description": "Max wait in ms (action=wait; default 60000). Values at or above 300000 run as 295000 so the reply beats a 300s client-side tool-call deadline. On timeout returns {timed_out:true}." },
                "prompt": { "type": "string", "description": "Task prompt for the agent (action=spawn)" },
                "pty_description": { "type": ["string", "null"], "description": "Short description of the PTY task shown above the terminal (action=spawn)" },
                "cwd": { "type": "string", "description": "Working directory (action=spawn)" },
                "model": { "type": "string", "description": "Structured model flag; preserved when args is also set (action=spawn)" },
                "env": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Child environment overrides. Applied after run-config env and before protected TUIC_SESSION/TUIC_PARENT (action=spawn)" },
                "print_mode": { "type": "boolean", "description": "false (default): visible TUI tab, observable via agent(inbox). true: headless, no tab. (action=spawn)" },
                "output_format": { "type": "string", "description": "Output format, e.g. 'json' (action=spawn)" },
                "agent_type": { "type": "string", "description": "Agent type OR run config name. Resolved as: (1) run config name match across enabled agents, (2) agent binary name (claude, codex, aider, goose, gemini, ...). Case-insensitive. (action=spawn)" },
                "binary_path": { "type": "string", "description": "Override agent binary path (action=spawn)" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "Additional CLI args; composed with structured flags and agent defaults. Native scrollback is controlled by the per-agent prevent_alt_screen setting (action=spawn)." },
                "rows": { "type": "integer", "description": "Terminal rows (action=spawn)" },
                "cols": { "type": "integer", "description": "Terminal cols (action=spawn)" },
                "tuic_session": { "type": "string", "description": "Optional explicit stable UUID (action=register). Managed PTYs normally auto-bind; a headerless caller may omit this to receive an MCP-scoped UUID." },
                "replaces": { "type": "string", "description": "Prior tuic_session this registration supersedes (action=register). Required to inherit the old identity's inbox when reconnecting under a new UUID — there is no implicit link across protocol sessions, and identity is never guessed. Ignored when that identity still owns a live PTY; the response then reports mail_stranded." },
                "name": { "type": "string", "description": "Non-empty peer/session display name (action=spawn optional; action=register optional; default: 'agent')" },
                "path": { "type": "string", "description": "Git repo root path (action=register optional, action=list_peers filter)" },
                "orchestrator": { "type": "boolean", "description": "Explicitly enable or remove orchestrator inbox-only routing (action=register). Omission preserves the current role; spawning a child never infers it." },
                "to": { "type": "string", "description": "Recipient address (action=send, required): its tuic_session UUID, PTY id, alias, unique short PTY-id prefix, or unique display name" },
                "message": { "type": "string", "description": "Message content, max 64KB (action=send, required)" },
                "urgency": { "type": "string", "enum": ["normal", "urgent"], "description": "action=send: normal (default), or urgent when the recipient must change course before its next step. Urgent writes only a payload-free inbox notice to a safe busy Claude/Codex composer; it never interrupts a running tool." },
                "keep_open": { "type": "boolean", "description": "action=spawn: keep this managed child open; action=send: set or clear the recipient managed child's keep-open flag." },
                "limit": { "type": "integer", "minimum": 1, "maximum": crate::state::AGENT_INBOX_CAPACITY, "description": "Maximum inbox entries to return (action=inbox; default 100, maximum 100). Read again while has_more is true." },
                "since": { "type": "integer", "description": "Logical unix-millis cursor (action=inbox|wait). OMIT IT: the server remembers your last read position and resumes from there. Pass it only to override — since=0 deliberately replays the whole inbox. Every wait/inbox response carries next_since, including on timeout" }
            }, "required": ["action"] }
        },
        {
            "name": "task",
            "description": "Poll a long-running task handle without blocking. `agent action=spawn` returns a task_id; use it here instead of holding a wait open, which is capped at 300s and loses the outcome if the connection drops.\n\nThe outcome is recorded when the agent exits whether or not anyone was listening, so a reconnecting orchestrator can still collect it (for up to 24h).\n\nActions:\n- get: Current state. Returns {task_id, status, status_message?, result?, error_detail?, poll_interval_ms}. status is working|input_required|completed|failed|cancelled; the last three are final and never change again. A failed task reports why in error_detail, NOT in error — a top-level `error` always means the call itself failed. Poll no faster than poll_interval_ms.\n- cancel: Mark the task cancelled. Does NOT kill the agent — use session action=kill for that. A cancel is final: the agent's later exit cannot overwrite it.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: get, cancel" },
                "task_id": { "type": "string", "description": "Task handle returned by agent action=spawn (required)" }
            }, "required": ["action", "task_id"] }
        },
        {
            "name": "remote",
            "description": "Update a connected remote daemon. preview reports both builds, source, target and live PTY count. update requires the count and selected SHA-256 from a preview after the user has confirmed session loss; it waits until the new build answers /health.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["preview", "update"] },
                "connection_id": { "type": "string", "description": "Configured remote connection ID" },
                "confirmed_sessions": { "type": "integer", "minimum": 0, "description": "Live session count confirmed by the user (update only)" },
                "expected_sha256": { "type": "string", "description": "Selected build digest from preview (update only)" }
            }, "required": ["action", "connection_id"] }
        },
        {
            "name": "repo",
            "description": "Repository and version control. Query workspace repos, their GitHub PR/CI status, and manage git worktrees.\n\nActions:\n- add: Register and activate an existing local directory. Requires absolute path. Persists a usable Git or shell workspace, preserves existing terminals/settings on repeat calls, and broadcasts repositories-changed. No desktop URL handler required; does not spawn a terminal. Returns ok, path, name, branch, is_git_repo, and an optional watcher warning.\n- list: Open repos with branch, dirty status, worktrees.\n- active: Focused repo path, branch, group.\n- status: Cross-repo GitHub PR and CI summary {path, branch, ahead, behind, open_prs, failing_ci}.\n- branch_integrations: Integration status and proof for every local branch, including worktree_paths. Requires path.\n- branch_integration: Same verdict for one branch. Requires path and branch. Returns tip, integrated, proof, archive_required, archived and archive_ref. Content-based proof requires an archive at that tip before deletion; unknown is never proof.\n- worktree_list: Worktrees for a repo. Requires path. Each entry includes lifecycle_status {commit_status: merged|unmerged|in_sync|unknown, dirty_files, removal_safety: safe|requires_force|unknown}, the same verdict worktree_lifecycle and the sidebar use.\n- worktree_lifecycle: Fresh safety, fingerprint and submodule commit counts. Requires path and branch.\n- worktree_create: Create a linked worktree. Requires path. Optional: branch, base_ref, spawn_session (starts a bare shell PTY, not an agent). Refs and objects are shared with the parent; parent tracked changes are not copied. Git-ignored build directories warm in the background. Wait for warm_artifacts.status in worktree_list to become done or failed before installing dependencies or building.\n- worktree_remove: Remove worktree. Requires path and branch, or path and worktree_path for a detached checkout (removed only when clean, reachable from a branch and free of live sessions).\n- orphan_cleanup_answer: Answer the pending orphan-removal dialog for path with decision=remove|keep; remove rechecks every worktree for uncommitted/untracked files and branch reachability.\n- branch_delete: Delete only a local branch with no checkout after proving its commits are integrated or preserved. Requires path and branch. Proof is in_sync, a merge proof, patch_equivalence, or archived (refs/archive/<branch> points at the exact tip; the response then carries archive_ref and the archive ref is kept). Refuses current/default branches, unmerged commits with no exact archive, and unsafe or changed refs; never touches a remote.\n- progress_list: The project's journal, newest first, paged with total and nextCursor. Requires path. Optional input.blockedOnly, input.ptyId, input.limit (default 8, maximum 100), input.cursor (previous nextCursor). Record a NEW outcome with the `progress` tool, not here.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, add, active, status, branch_integrations, branch_integration, worktree_list, worktree_lifecycle, worktree_create, worktree_remove, orphan_cleanup_answer, branch_delete, progress_list" },
                "path": { "type": "string", "description": "Absolute path to git repository (required for worktree_list, worktree_lifecycle, worktree_create, worktree_remove, branch_delete, progress_list)" },
                "force": { "type": "boolean", "description": "action=worktree_remove optional, default false. Explicitly permits discarding dirty workspace state; obtain user confirmation before setting it." },
                "delete_branch": { "type": "boolean", "description": "action=worktree_remove optional. Defaults to true unless force is true; an explicit true still requires branch safety proof." },
                "override_lock": { "type": "boolean", "description": "action=worktree_remove optional, default false. Override a locked worktree only after explicit user confirmation." },
                "expected_fingerprint": { "type": "string", "description": "action=worktree_remove: lifecycle fingerprint shown at force confirmation. Removal refuses if the worktree changed." },
                "branch": { "type": "string", "description": "Local branch name (required for worktree_lifecycle, worktree_remove and branch_delete; optional for worktree_create)" },
                "worktree_path": { "type": "string", "description": "action=worktree_remove: absolute path of a detached (orphan) checkout, instead of branch" },
                "decision": { "type": "string", "description": "action=orphan_cleanup_answer: remove or keep the pending orphan cleanup for path" },
                "base_ref": { "type": "string", "description": "Base ref to branch from, default HEAD (action=worktree_create)" },
                "spawn_session": { "type": "boolean", "description": "Auto-create a PTY session in the worktree (action=worktree_create, default false)" },
                "input": { "type": "object", "description": "Typed action payload for progress_list. Defaults to 8 entries; follow nextCursor until null. Unknown fields are rejected.", "properties": {
                    "blockedOnly": { "type": "boolean", "description": "Only blocked entries, applied before paging." },
                    "ptyId": { "type": "string", "description": "Only this terminal's entries, applied before paging." },
                    "limit": { "type": "integer", "minimum": 0, "description": "Entries per page, default 8; clamped to 1..100." },
                    "cursor": { "type": "integer", "description": "The previous page's nextCursor; omit for the newest page." }
                }, "additionalProperties": false }
            }, "required": ["action"] }
        },
        {
            "name": "story",
            "description": "Read and update native plans and stories in the calling managed session's project. Pass one StoryAction as `input`; the input schema lists every action, field, type and enum. Every mutation is checked by the Rust story service, and a refusal names its cause. A claim binds to the calling live PTY.\n\nAgent actions: create_plan, list_plans, list_plan_sources, add_plan_source, get_plan, plan_state, plan_view, create_story, list_stories, get_story, transition_history (a story's recorded transitions), add_dependency, claim, and transition with check_criterion, uncheck_criterion, submit_review or approve. add_dependency on a ready story moves it to backlog (it cannot be claimed until the dependency is done) when the dependency is not done, and also when it is done but has no integration receipt in a workflow-owned plan.\n\nAdministrative actions: remove_dependency, and transition with start_manual, reject_review, block, unblock or wont_fix. Actor identity is recorded as provenance and never restricts an action; status, revision, project and dependency checks still apply.\n\nThis tool reaches only the calling session's own project (another project's plan is refused with `plan does not belong to project`). To read or approve a plan of another project, use the CLI: `tuic story '<action JSON>' --project /abs/project` (see the native-stories user guide).",
            "inputSchema": { "type": "object", "properties": {
                "input": crate::stories::story_action_schema()
            }, "required": ["input"] }
        },
        {
            "name": "workflow_run",
            "description": "Start and inspect published workflow graphs in the calling managed session's project. Pass a generated RunAction as input: start_graph (payload-bound request_id, pinned definition revision, story expected_revision), get, list_plan_runs, events (after_sequence cursor), command, record_integration, recertify_canonical, execute_check. start_plan is the legacy record-only ledger, not executable delivery. Graph starts require the owning daemon; unsupported plan dispatch is refused visibly. Commands include pause, cancel, answer_input, resume_graph with explicit execution_id, activation_id and resolution. Legacy runs are inspect/cancel only in the UI. History is the same ordered ledger returned over HTTP/IPC, including decisions and evidence.",
            "inputSchema": { "type": "object", "properties": {
                "input": crate::workflows::run_action_schema()
            }, "required": ["input"], "additionalProperties": false }
        },
        {
            "name": "workflow_story_create",
            "description": "Create a native story once from a plan run. Only the active bound coordinator may call this. A stable proposalKey prevents duplicate stories after retries; a reused key with different story data is rejected.",
            "inputSchema": { "type": "object", "properties": {
                "input": { "type": "object", "description": "{runId,proposalKey,story:NewStory}. The story must use the run's planId and a plan_step origin." }
            }, "required": ["input"] }
        },
        {
            "name": "workflow_report",
            "description": "Submit a typed outcome for the workflow attempt bound to this live managed agent session. A report is idempotent for its attempt and does not itself advance the story status.",
            "inputSchema": { "type": "object", "properties": {
                "input": { "type": "object", "description": "AttemptReport with contractVersion, runId, storyId, storyRevision, attemptId, generation, outcome (completed|failed|needs_input|interrupted), summary, criterionResults [{index,satisfied,evidence}], and evidence [string]. needs_input requires inputRequest {question,options}; a completed reviewer report requires review {decision,artifactDigest,findings:[{criterionIndex,severity,summary,evidence}]} ." }
            }, "required": ["input"] }
        },
        {
            "name": "workflow_launch",
            "description": "Launch a pinned workflow agent attempt in a known isolated worktree. The backend renders and hashes the role prompt, reserves the spawn effect, starts the managed agent, and binds its session to the attempt. A story worker may be launched only by this run's active coordinator.",
            "inputSchema": { "type": "object", "properties": {
                "input": { "type": "object", "description": "{runId,attemptId,worktreePath,agentType,skills:[{name,location,available}],feedback?}. The caller must be a live managed session in the run's project." }
            }, "required": ["input"] }
        },
        {
            "name": "progress",
            // Claude Code defers MCP tools behind ToolSearch unless told not to;
            // a deferred `progress` is a tool Claude agents never call.
            "_meta": { "anthropic/alwaysLoad": true },
            "description": "Record what happened, for the user who walked away. Mandatory: type=done when the work an `intent:` announced is finished, type=blocked when you stop and wait for the user because you cannot proceed without them. A blocked report marks your tab as waiting for an answer until the user replies or you report done. If you keep working while the user does something in parallel, report that with type=done, not blocked.",
            "inputSchema": { "type": "object", "properties": {
                "type": { "type": "string", "enum": ["done", "blocked"] },
                "text": { "type": "string", "maxLength": 500, "description": "Outcome, not implementation." },
                "step": { "type": "string", "maxLength": 80, "description": "Optional free label — a crumb the human reads. Nothing is keyed on it." }
            }, "required": ["type", "text"] }
        },
        {
            "name": "ui",
            "description": "Control TUIC UI. Actions:\n- tab: open/update panel tab. Requires id, title, + html OR url.\n- toast: non-blocking notification. Requires title. Optional: message, level (info/warn/error), sound.\n- confirm: blocking dialog, shown on every client (desktop, browser, mobile PWA) plus a mobile push — the first answer wins. Returns {confirmed}, plus {reason} when it expired unanswered after 300s (treat that as a refusal, not a yes). Requires title.\n- screenshot: capture a panel as WebP. Requires id. Returns {path}. Read the path to view.\n\nURL schemes for tab:\n- http(s): loaded in sandboxed iframe.\n- file:///path: read via IPC and rendered as inline HTML (sandbox blocks direct file:// access).\n- tuic://edit/<path>?line=N: native code editor (no iframe). Prefix absolute paths with `//` (tuic://edit//Users/x/a.rs). Relative = active repo.\n- tuic://open/<path>: native markdown/preview tab.\n\n- tuic://open-repo is rejected; use repo action=add with path to register a directory. Other tuic actions are not tab content.\n\nCustom schemes (vscode://) do NOT work in iframes.\n\nUse:\n- A project outcome — a milestone, a decision, a discovery, a blocker — belongs to the `progress` tool, which records it AND shows a toast. `ui action=toast` is the transient half only: it says something happened and keeps no record of it.\n- toast for done/error/long-job end; error=failure, warn=recoverable. Skip for micro-steps.\n- toast with sound=attention when you are working unattended and are BLOCKED on the user (question, approval, ambiguous requirement). It is the only sound that carries across a room; do not spend it on progress updates.\n- confirm BEFORE destructive ops (rm -rf, git reset --hard, force-push, DROP). Only proceed if confirmed.\n- tab http(s) for dashboards, reports, >20-line structured output.\n- tab tuic://edit to point user at source file+line (review, bug discussion) — beats pasting snippets.\n- screenshot to visually verify rendered HTML content in a panel you created.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: tab, toast, confirm, screenshot" },
                "id": { "type": "string", "description": "Stable identifier for dedup — same id reuses existing tab (action=tab, required)" },
                "title": { "type": "string", "description": "Tab or notification title (action=tab/toast/confirm, required)" },
                "html": { "type": "string", "description": "Inline HTML content to render in sandboxed iframe (action=tab, mutually exclusive with url)" },
                "url": { "type": "string", "description": "Tab URL (action=tab, xor html). http(s) → iframe. file:///path → read and inline. tuic://edit/<path>?line=N → native editor. tuic://open/<path> → markdown tab. Absolute paths need `//` prefix." },
                "pinned": { "type": "boolean", "description": "Pin tab across all branches (default false)" },
                "focus": { "type": "boolean", "description": "Switch to this tab after open/update (action=tab, default true). Pass false to update silently without stealing focus." },
                "message": { "type": "string", "description": "Optional body text (action=toast/confirm)" },
                "level": { "type": "string", "description": "Toast level: info, warn, error (default: info)" },
                "sound": { "description": "Audible signal for action=toast (default: none). true = the tone matching `level`. Or name one: question, completion, error, warning, info, attention. `attention` is a triangular G4→G4→E5 callback, unlike any other sound in the app — use it when you are running unattended and need the user back (a blocking question, a decision only they can make). Plays through the user's notification settings, so volume, output device and mutes are respected.", "anyOf": [{ "type": "boolean" }, { "type": "string", "enum": ["question", "completion", "error", "warning", "info", "attention"] }] }
            }, "required": ["action"] }
        },
        {
            "name": "plugin_dev_guide",
            "description": "Returns comprehensive plugin authoring reference: manifest format, PluginHost API (all 4 tiers), structured event types, and working examples. Call before writing any plugin code.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "config",
            "description": "Read or write app configuration.\n\nActions (pass as 'action' parameter):\n- get: Returns app config (shell, font, theme, etc.). Password hash is stripped.\n- save: Persists configuration. Requires config object. Partial updates OK.\n- list_prompts: Lists saved smart prompts (id, label, pinned — no text).\n- load_prompt: Returns full prompt entry by id (requires 'id' param).\n- save_prompt: Upserts a prompt by id (requires 'id', 'label', 'text'; optional 'pinned'). Localhost only.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: get, save, list_prompts, load_prompt, save_prompt" },
                "config": { "type": "object", "description": "Config fields to save (action=save)" },
                "id": { "type": "string", "description": "Prompt id (action=load_prompt, save_prompt)" },
                "label": { "type": "string", "description": "Prompt label (action=save_prompt)" },
                "text": { "type": "string", "description": "Prompt text (action=save_prompt)" },
                "pinned": { "type": "boolean", "description": "Pin prompt (action=save_prompt, optional)" }
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
        },
        {
            "name": "voice",
            "description": "Speak a reply out loud, when the user is talking to you hands-free.\n\nOnly works while the user has armed hands-free dictation for YOUR terminal: speech belongs to that conversation, not to the application. Call action=status first — if available is false, the reason says why and you must reply in text as usual. Nothing here replaces your normal reply; speaking is in addition to it.\n\nThe conversation announces itself. Arming sends you a line saying hands-free voice is now on for this terminal; ending it sends one saying it is off. Both come from TUICommander rather than from the user, and the second is final — after it this tool reports available false and you reply in text again. The user can turn those notices off, so their absence is not proof either way: action=status is.\n\nYou do not choose the language or the voice. Both belong to the conversation: the voice is the one for the language the user is speaking, reported as status.language, and it will happily pronounce text in any other language badly. Write the reply in status.language — the same language the hands-free entry told you to answer in.\n\nAccepting a reply is NOT the same as the user hearing it. action=speak returns an utterance_id in state 'queued'; rendering takes seconds and the user can talk over it at any moment. Poll action=status with that utterance_id to learn the outcome: 'finished' means it was heard to the end, 'interrupted' means the user started talking (normal, not an error), 'failed' means it was never audible.\n\nActions:\n- speak: Queue one spoken reply. Requires text (max 2000 characters). Optional turn: the turn you are answering, from an earlier status or from the hands-free notice. Supply it — a reply written for a turn the user has already talked over is then refused instead of spoken over whatever they said next. Omitting it means 'right now'. Returns {utterance_id, state, turn}.\n- stop: Stop talking immediately and drop anything queued. Opens a new turn. Use it when you realise the reply in progress is wrong.\n- status: {available, unavailable_reason, session_id, language, turn, voice, queued, rendering, speaking, last_error}. Optional utterance_id adds {utterance: {utterance_id, state, error?}} for that one reply; a reply too old to remember comes back as state 'unknown'.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: speak, stop, status" },
                "text": { "type": "string", "description": "What to say, max 2000 characters (action=speak, required)" },
                "turn": { "type": "integer", "description": "The turn this reply answers (action=speak, optional but recommended)" },
                "utterance_id": { "type": "string", "description": "A reply returned by action=speak (action=status, optional)" }
            }, "required": ["action"] }
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

/// The three meta-tool names used when `collapse_tools: true`.
/// Exposed for handler dispatch and tests.
pub(crate) const META_TOOL_NAMES: [&str; 3] = ["search_tools", "get_tool_schema", "call_tool"];

/// Speakeasy-style meta-tool definitions. When `collapse_tools: true`, these
/// three tools replace the full native + upstream list. The model uses
/// `search_tools` to discover
/// relevant tools by natural language, `get_tool_schema` to fetch the full
/// input schema for one, and `call_tool` to execute it.
///
/// Domain context and discovery flow are embedded in the tool descriptions
/// (not in server instructions) so they don't compete with protocol markers
/// for the model's attention at turn 1.
pub(super) fn meta_tool_definitions(state: &Arc<AppState>) -> serde_json::Value {
    let upstream_count = state.mcp.upstream_registry.aggregated_tools().len();
    let upstream_suffix = if upstream_count > 0 {
        format!(", plus {upstream_count} upstream tool(s) from connected MCP servers")
    } else {
        String::new()
    };

    let search_desc = format!(
        "Find relevant TUICommander tools by natural-language query. Returns a BM25-ranked \
         list of tool names + one-line summaries. Use it to discover a tool you do not yet \
         know, then call `get_tool_schema` for its full input schema. A tool already on your \
         list, or one whose schema you fetched earlier on this connection, is callable \
         without searching for it again.\n\n\
         Domains available: terminal pane sessions (tmux replacement), AI agent orchestration + \
         messaging, repos/GitHub PRs/worktrees, UI tabs + notifications, plugin authoring \
         reference, app config, diagnostics{upstream_suffix}."
    );

    serde_json::json!([
        {
            "name": "search_tools",
            "description": search_desc,
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
            "description": "Invoke a TUICommander tool by name with arguments. Dispatches to native tools or upstream-proxied tools (`{upstream}__{tool}`). The arguments object must match the tool's inputSchema.\n\nFirst time with a tool: `search_tools(query=\"…\")` → pick a name → `get_tool_schema(tool_name=…)` → `call_tool(tool_name=…, arguments={…})`. After that, call it straight away with the schema you already have; re-fetch it only after a `notifications/tools/list_changed`, which is the one thing that can move it. Never skip discovery for a name you have NOT seen from `search_tools` or `get_tool_schema`: an invented name is a failed call, not a lucky guess.",
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
/// When `config.collapse_tools: true`, returns the 3 meta-tools plus the compact
/// `progress` tool when it is enabled. Progress stays directly callable because
/// reporting must not require preliminary discovery calls.
///
/// Otherwise (default), returns native tools filtered by `disabled_native_tools`,
/// merged with upstream proxy tools. Upstream tools are omitted when no
/// upstreams are Ready.
/// Resolve an MCP session's repo_path → per-repo `mcp_upstreams` allowlist.
///
/// Returns `None` when the session has no repo_path or the repo has no
/// custom upstream allowlist (meaning: inherit all globally-enabled upstreams).
pub(super) fn resolve_allowed_upstreams(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
) -> Option<Vec<String>> {
    let repo_path = mcp_session_id
        .and_then(|sid| state.mcp.sessions.get(sid))
        .and_then(|meta| meta.repo_path.clone())?;
    let repo_settings = crate::config::load_repo_settings();
    repo_settings
        .repos
        .get(&repo_path)
        .and_then(|entry| entry.mcp_upstreams.clone())
}

/// Settings inventory from the native registry, including disabled tools.
/// This is app metadata, not an MCP discovery surface. Every native tool can
/// be disabled; progress additionally requires global progress_tracking.
pub(crate) fn native_tool_catalog() -> Vec<serde_json::Value> {
    native_tool_definitions()
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| {
            let description = tool["description"].as_str().unwrap_or_default();
            serde_json::json!({
                "name": tool["name"],
                "summary": NATIVE_TOOL_SUMMARIES.iter()
                    .find(|(name, _)| Some(*name) == tool["name"].as_str())
                    .map(|(_, summary)| *summary)
                    .unwrap_or_default(),
                "description": description,
            })
        })
        .collect()
}

/// Apply the two config-driven filters (`disabled_native_tools`,
/// `progress_tracking`) to the full native tool list. Centralised so
/// every listing/search path uses the same rules — adding a future config
/// flag means editing one place instead of chasing duplicated closures.
pub(super) fn filtered_native_tools(state: &Arc<AppState>) -> Vec<serde_json::Value> {
    let (disabled, progress_tracking) = {
        let cfg = state.config.read();
        let disabled: std::collections::HashSet<String> =
            cfg.disabled_native_tools.iter().cloned().collect();
        (disabled, cfg.progress_tracking)
    };
    native_tool_definitions()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            let name = t["name"].as_str().unwrap_or("");
            // The global half of `progress_tracking`. It is deliberately the
            // global one and not the effective per-agent value: this list also
            // feeds `searchable_tool_definitions`, which builds ONE cached index
            // for every session, so a per-agent answer here would be whichever
            // agent happened to trigger the last rebuild. The per-agent escape
            // hatch is enforced where a report is recorded, which answers
            // `progress_tracking_disabled` instead of hiding the tool.
            if !progress_tracking && name == "progress" {
                return false;
            }
            !disabled.contains(name)
        })
        .collect()
}

/// Resolve which surface a caller gets, then build it.
///
/// A stateless caller has no session meta to read `requires_meta_tools` from,
/// so it says who it is in this request's `_meta` instead — that name arrives
/// as `stateless_client_name`. **That identity wins**, and the session flag is
/// the fallback for a legacy client that recorded its name once at `initialize`
/// and does not repeat it per request.
///
/// The order matters because a session is not always the caller. `tuic-bridge`
/// opens the transport session under its own name and then proxies ego's
/// requests through it verbatim, so reading the session first handed ego the
/// full catalogue — 615 tools against the 35.104 tokens a turn the collapsed
/// surface costs — while `server/discover`, which reads the request `_meta`,
/// had just advertised the collapsed one. A handshake that disagrees with the
/// list that follows it is worse than either answer alone.
pub(super) fn merged_tool_definitions(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
    stateless_client_name: Option<&str>,
) -> serde_json::Value {
    let force_meta_tools = match stateless_client_name {
        // The same expression `server/discover` evaluates for the same request.
        Some(name) => client_requires_meta_tools(Some(name)),
        None => mcp_session_id
            .and_then(|sid| state.mcp.sessions.get(sid))
            .is_some_and(|meta| meta.requires_meta_tools),
    };
    merged_tool_definitions_for_mode(state, mcp_session_id, force_meta_tools)
}

pub(super) fn merged_tool_definitions_for_mode(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
    force_meta_tools: bool,
) -> serde_json::Value {
    if force_meta_tools || state.config.read().collapse_tools {
        let mut tools = meta_tool_definitions(state)
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(progress) = filtered_native_tools(state)
            .into_iter()
            .find(|tool| tool["name"] == "progress")
        {
            tools.push(progress);
        }
        return serde_json::Value::Array(tools);
    }

    let mut tools = filtered_native_tools(state);
    let allowed = resolve_allowed_upstreams(state, mcp_session_id);
    let upstream_tools = state
        .mcp
        .upstream_registry
        .aggregated_tools_for_repo(allowed.as_deref());
    tools.extend(upstream_tools);

    serde_json::Value::Array(tools)
}

/// Build the full searchable tool corpus — native (filtered by
/// `disabled_native_tools`) merged with upstream aggregated tools.
///
/// Unlike [`merged_tool_definitions`], this bypasses the `collapse_tools`
/// branch: when collapsed, the client sees only the 3 meta-tools but the
/// handlers still need the full list to search over and dispatch to.
///
/// Upstream allow/deny filters are applied inside `aggregated_tools()`.
pub(super) fn searchable_tool_definitions(state: &Arc<AppState>) -> Vec<serde_json::Value> {
    let mut tools = filtered_native_tools(state);
    tools.extend(state.mcp.upstream_registry.aggregated_tools());
    tools
}

/// Rebuild the cached `tool_search_index` from the current state.
///
/// Called on startup and on every `mcp_tools_changed` signal (native tool
/// toggle, upstream add/remove, upstream tools/list_changed).
pub(crate) fn rebuild_tool_search_index(state: &Arc<AppState>) {
    let tools = searchable_tool_definitions(state);
    let index = crate::tool_search::ToolSearchIndex::build(&tools);
    *state.mcp.tool_search_index.write() = index;
}

/// Spawn the background task that subscribes to `mcp_tools_changed` and
/// rebuilds `tool_search_index` on every signal. Also does an initial build
/// so the index is populated immediately.
pub(crate) fn spawn_tool_search_index_updater(state: Arc<AppState>) {
    // Initial build so search_tools works before the first tools_changed signal.
    rebuild_tool_search_index(&state);

    let mut rx = state.mcp.tools_changed.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(()) => rebuild_tool_search_index(&state),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        source = "tool_search_index",
                        lagged = n,
                        "tools_changed bus lagged — rebuilding"
                    );
                    rebuild_tool_search_index(&state);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Handle `search_tools` meta-tool — BM25 search over the full corpus.
pub(super) fn handle_search_tools(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = match args["query"].as_str() {
        Some(q) if !q.trim().is_empty() => q,
        _ => {
            return serde_json::json!({
                "error": "search_tools requires non-empty 'query' (natural-language string describing what you want to do)"
            });
        }
    };
    let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 100) as usize;

    let index = state.mcp.tool_search_index.read();
    let results = index.search(query, limit);

    let ranked: Vec<serde_json::Value> = results
        .iter()
        .map(|e| serde_json::json!({ "name": e.name, "summary": e.summary }))
        .collect();
    serde_json::json!({ "results": ranked, "count": ranked.len() })
}

/// Handle `get_tool_schema` meta-tool — exact-name lookup of a tool's full definition.
pub(super) fn handle_get_tool_schema(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let tool_name = match args["tool_name"].as_str() {
        Some(n) if !n.trim().is_empty() => n,
        _ => {
            return serde_json::json!({
                "error": "get_tool_schema requires non-empty 'tool_name' (exact tool name from search_tools)"
            });
        }
    };

    let index = state.mcp.tool_search_index.read();

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
pub(super) async fn handle_call_tool(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> serde_json::Value {
    let tool_name = match args["tool_name"].as_str() {
        Some(n) if !n.trim().is_empty() => n.to_string(),
        _ => {
            return serde_json::json!({
                "error": "call_tool requires non-empty 'tool_name' (exact tool name from search_tools or get_tool_schema)"
            });
        }
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

    let tool_args = args
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let is_upstream = tool_name.contains("__");
    if is_upstream {
        let allowed = resolve_allowed_upstreams(state, mcp_session_id);
        match state
            .mcp
            .upstream_registry
            .proxy_tool_call_for_repo(&tool_name, tool_args, allowed.as_deref())
            .await
        {
            Ok(v) => mark_upstream_tool_result(v),
            Err(e) => serde_json::json!({ "error": e }),
        }
    } else {
        // Recursive async dispatch requires Box::pin. Meta names are blocked above.
        Box::pin(handle_mcp_tool_call_with_context(
            state,
            addr,
            &tool_name,
            &tool_args,
            mcp_session_id,
            managed_parent_cwd,
        ))
        .await
    }
}
