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
#[derive(Default)]
pub(super) struct InstructionContext {
    pub(super) repos: RepoSummary,
    /// `(short session id, cwd, worktree branch)`, already defaulted to `—`.
    pub(super) sessions: Vec<(String, String, String)>,
    pub(super) peer_count: usize,
    /// Names of the native tools registered on this install, in registration order.
    pub(super) native_tools: Vec<String>,
    /// `(name, tool count)` of the connected upstream servers this session may use.
    pub(super) upstreams: Vec<(String, usize)>,
    /// A spawned child: its suggestion buttons are never shown to anyone.
    pub(super) spawned_child: bool,
}

/// What the instructions say about the registered repositories: counts only,
/// so adding a repository never changes the text beyond one number.
#[derive(Default)]
pub(super) struct RepoSummary {
    pub(super) total: usize,
    /// `(group name, repo count)`, sorted by name.
    pub(super) groups: Vec<(String, usize)>,
    /// `(connection id, repo count)` for repos on a remote connection, sorted by id.
    pub(super) remote: Vec<(String, usize)>,
}

/// Summarize the `repositories.json` document that `repo action=list` serves:
/// the same `repoOrder`, `groups` and `connectionId` fields.
pub(super) fn summarize_repos(repo_data: &serde_json::Value) -> RepoSummary {
    let order: Vec<&str> = repo_data["repoOrder"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str()).collect())
        .unwrap_or_default();
    let mut groups: std::collections::BTreeMap<String, usize> = Default::default();
    if let Some(all) = repo_data["groups"].as_object() {
        for group in all.values() {
            let name = group["name"].as_str().unwrap_or("").to_string();
            let members = group["repoOrder"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_str())
                        .filter(|p| order.contains(p))
                        .count()
                })
                .unwrap_or(0);
            if members > 0 && !name.is_empty() {
                *groups.entry(name).or_default() += members;
            }
        }
    }
    let mut remote: std::collections::BTreeMap<String, usize> = Default::default();
    for path in &order {
        if let Some(id) = repo_connection_id(&repo_data["repos"][*path]) {
            *remote.entry(id.to_string()).or_default() += 1;
        }
    }
    RepoSummary {
        total: order.len(),
        groups: groups.into_iter().collect(),
        remote: remote.into_iter().collect(),
    }
}

/// The remote connection a stored repository entry lives on, if any.
fn repo_connection_id(stored: &serde_json::Value) -> Option<&str> {
    stored["connectionId"].as_str().filter(|id| !id.is_empty())
}

/// Tag a `repo action=list` row with the connection its repository lives on.
/// A local repository gets no `connection_id` key.
pub(super) fn tag_repo_connection(row: &mut serde_json::Value, stored: Option<&serde_json::Value>) {
    if let Some(id) = stored.and_then(repo_connection_id) {
        row["connection_id"] = serde_json::json!(id);
    }
}

fn instruction_context(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> InstructionContext {
    let repos = summarize_repos(&crate::config::load_repositories());

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

    let native_tools = filtered_native_tools(state)
        .iter()
        .filter_map(|t| t["name"].as_str().map(str::to_string))
        .collect();
    let allowed = resolve_allowed_upstreams(state, mcp_session_id);
    let upstreams = state
        .mcp
        .upstream_registry
        .ready_upstreams(allowed.as_deref());

    InstructionContext {
        repos,
        sessions,
        peer_count: state.peer_agents.len(),
        native_tools,
        upstreams,
        spawned_child: mcp_session_id.is_some_and(|id| is_spawned_child(state, id)),
    }
}

/// An MCP session bound to a peer that has a parent session: it was spawned
/// by another agent rather than started by the user.
fn is_spawned_child(state: &AppState, mcp_session_id: &str) -> bool {
    state
        .mcp
        .to_session
        .get(mcp_session_id)
        .is_some_and(|peer| state.session_maps.session_parent.contains_key(peer.value()))
}

pub(super) fn build_mcp_instructions_for_mode(
    state: &Arc<AppState>,
    client_name: Option<&str>,
    collapse_tools: bool,
) -> String {
    build_mcp_instructions_for_session(state, client_name, collapse_tools, None)
}

/// Same, for the MCP session that is connecting: it decides which upstream
/// servers are listed and whether the caller is a spawned child.
pub(super) fn build_mcp_instructions_for_session(
    state: &Arc<AppState>,
    client_name: Option<&str>,
    collapse_tools: bool,
    mcp_session_id: Option<&str>,
) -> String {
    render_mcp_instructions(
        client_name,
        collapse_tools,
        resolve_marker_flags(state, client_name),
        crate::progress::progress_tracking_enabled(state, resolve_agent_type(client_name)),
        &instruction_context(state, mcp_session_id),
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

    // Launch-capable harnesses receive the playbook through their native flag.
    // Do not re-enable disabled modules or deliver a conflicting second copy.
    let launch_channel = resolve_agent_type(client_name)
        .is_some_and(|agent| crate::launch_instructions::AGENTS.contains(&agent));
    if !launch_channel {
        // ── TUIC protocol (mandatory line markers) ─────────────────────────
        // Wire-level tokens parsed by the host TUI. Concision rules do NOT apply —
        // dropping a marker breaks the UI (stale tab title, missing suggestion bar).
        let (show_intent, show_suggest) = markers;
        // A spawned child's suggestion buttons are never shown to anyone.
        let show_suggest = show_suggest && !ctx.spawned_child;
        out.push_str("## TUIC Protocol — Required Output Markers\n\n");
        out.push_str("Required for the UI.\n\n");
        out.push_str(&format!(
        "- `ack` — once per MCP connection/reconnect: start your first message with `TUICommander v{ver} is connected.` Never repeat per turn.\n"
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
        out.push_str("- `💬 ` — prefix every sentence that directly answers the user's question at row start or list-item text start (marker + space). Exclude status, tool and hook/task notices.\n");
        out.push('\n');
    }

    // Tool identity belongs to the connecting client's namespace, not the
    // case-sensitive protocol name. In particular native Agent is not ours.
    if ctx.native_tools.iter().any(|name| name == "agent") {
        out.push_str("**Tool identity:** Names below are TUIC MCP protocol names, not the native `Agent` tool. Use the callable name exposed by your client: Claude Code `mcp__tuicommander__agent`; Codex `mcp__tuicommander.agent` (namespace + member). The server alias may differ from `tuicommander`; use your actual tool list, never guess a name or change its case. Under lazy discovery, bare `tool_name=agent` is a target inside TUIC's `call_tool`, not a direct client tool call. The same qualification rule applies to every tool below.\n\n");
    }

    // ── Capability map ───────────────────────────────────────────────
    out.push_str(&render_capability_map(
        ctx,
        collapse_tools,
        detect_claude_code_client(client_name),
    ));

    // ── Cross-tool rules ─────────────────────────────────────────────
    // NOT a tool catalogue: `tools/list` already carries every name, action and
    // schema in the same turn, and restating it here bought a second copy for
    // the clients that read instructions and nothing at all for the clients that
    // do not. What survives is the pair of rules that no single tool description
    // owns — one forbids a tool that is not ours, the other spans two calls.
    out.push_str("## Tools\n\n");
    if launch_channel {
        out.push_str(if collapse_tools { "Discover/invoke via `search_tools` / `get_tool_schema` / `call_tool`; read their descriptions.\n\n" } else { "Read each tool's description for actions and rules.\n\n" });
    }
    if !launch_channel {
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
                "Call `progress` to record finished or blocked work; the user reads it later. \
             `type=done` for finished `intent:` work or work continuing alongside user action; \
             `type=blocked` only when you stop and wait because you need the user.\n\n",
            );
        }
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
    // Counts only: a path list changed with every added repository, which also
    // made every reconnect a new row in the prompt inspector (#1753-f6b8).
    if ctx.repos.total > 0 {
        out.push_str("## Repos\n\n");
        out.push_str(&render_repo_line(&ctx.repos, collapse_tools));
        out.push_str("\n\n");
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

fn render_repo_line(repos: &RepoSummary, collapse_tools: bool) -> String {
    let mut line = format!("{} repositories", repos.total);
    let mut parts: Vec<String> = repos
        .groups
        .iter()
        .map(|(name, n)| format!("group {name} {n}"))
        .collect();
    parts.extend(
        repos
            .remote
            .iter()
            .map(|(id, n)| format!("remote {id} {n}")),
    );
    if !parts.is_empty() {
        line.push_str(&format!(" ({})", parts.join(", ")));
    }
    let list = if collapse_tools {
        "`call_tool tool_name=repo arguments={action:list}`"
    } else {
        "`repo action=list`"
    };
    line.push_str(&format!(
        ". {list} returns each with its path, group and, on a remote connection, its `connection_id`."
    ));
    line
}

/// Tools that reach the user, in display order.
const USER_TOOLS: &[&str] = &["ui", "voice", "telegram", "progress", "secret"];
/// Tools that drive the platform, in display order.
const PLATFORM_TOOLS: &[&str] = &["session", "agent", "repo", "workflow", "automations"];

/// One line per purpose, naming only what this install registered. Deferred
/// clients see tool names but no schema until they search, so without this
/// map the tools that talk to the user drown among the aggregated upstreams.
fn render_capability_map(
    ctx: &InstructionContext,
    collapse_tools: bool,
    claude_code: bool,
) -> String {
    let has = |name: &str| ctx.native_tools.iter().any(|t| t == name);
    let named = |names: &[&str]| -> String {
        names
            .iter()
            .filter(|n| has(n))
            .map(|n| match *n {
                "ui" => "`ui` (toast — `urgency=urgent` shows it on screen, not only in the bell; confirm; tab)".to_string(),
                other => format!("`{other}`"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let other: Vec<String> = ctx
        .native_tools
        .iter()
        .filter(|t| !USER_TOOLS.contains(&t.as_str()) && !PLATFORM_TOOLS.contains(&t.as_str()))
        .map(|t| format!("`{t}`"))
        .collect();
    let mut lines = String::new();
    for (label, text) in [
        ("Reach the user", named(USER_TOOLS)),
        ("Drive the platform", named(PLATFORM_TOOLS)),
        ("Also", other.join(", ")),
    ] {
        if !text.is_empty() {
            lines.push_str(&format!("- **{label}:** {text}\n"));
        }
    }
    if !ctx.upstreams.is_empty() {
        let servers = ctx
            .upstreams
            .iter()
            .map(|(name, tools)| format!("`{name}` ({tools} tools)"))
            .collect::<Vec<_>>()
            .join(", ");
        let find = if collapse_tools {
            "`search_tools query=<server>`"
        } else if claude_code {
            "ToolSearch `<server>`"
        } else {
            "tools named `<server>__<tool>`"
        };
        lines.push_str(&format!(
            "- **Other MCP servers through TUIC:** {servers}. Find their tools with {find}.\n"
        ));
    }
    if lines.is_empty() {
        return lines;
    }
    let hint = if collapse_tools {
        "Names below are `call_tool` targets; use a complete search `top_schema` or load one with `get_tool_schema` first."
    } else if claude_code {
        "Load a tool's schema before the first call (ToolSearch `select:mcp__tuicommander__<tool>`)."
    } else {
        "Read a tool's description before the first call."
    };
    format!("## What TUIC gives you\n\n{hint}\n\n{lines}\n")
}

/// Validate a repo path for MCP tool calls, returning a JSON error value on failure.
pub(super) fn validate_mcp_repo_path(path: &str) -> Result<(), serde_json::Value> {
    super::validate_path_string(path).map_err(|msg| serde_json::json!({"error": msg}))
}

pub(super) const SESSION_ACTIONS: &str = "list, create, submit, force_retry, input, output, resize, rename, keep_open, suspend, close, kill, pause, resume, status, wait, declare_worktree, help";
pub(super) const WORKFLOW_ACTIONS: &str = "story, run, proposal_create, report, launch";

pub(super) const AGENT_ACTIONS: &str = "launch, spawn, register, list_peers, send, inbox, wait, subscribe, ack, unsubscribe, task_get, task_cancel, help";
pub(super) const REPO_ACTIONS: &str = "list, add, active, status, branch_integrations, branch_integration, worktree_list, worktree_lifecycle, worktree_create, worktree_remove, orphan_cleanup_answer, branch_delete, progress_list, work_item, work_item_act, help";
pub(super) const UI_ACTIONS: &str = "tab, toast, confirm, screenshot, design_start, design_stop, design_status, design_reload, design_navigate, design_screenshot, design_errors, design_eval, help";
pub(super) const TASK_ACTIONS: &str = "get, cancel";
pub(super) const DEBUG_ACTIONS: &str =
    "agent_detection, logs, sessions, invoke_js, frame_workload_dump, help";
pub(super) const VOICE_ACTIONS: &str = "speak, stop, status, help";
pub(super) const REMOTE_ACTIONS: &str = "preview, update";

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
    (
        "agent",
        "Launch agents, exchange messages and track background outcomes.",
    ),
    ("remote", "Preview and deploy remote daemon updates."),
    ("repo", "Inspect repositories and manage their worktrees."),
    (
        "workflow",
        "Manage stories, workflow runs and bound agent attempts.",
    ),
    (
        "automations",
        "Create and manage scheduled agent definitions.",
    ),
    ("progress", "Record and review project progress."),
    (
        "ui",
        "Show tabs, notices, confirmation and browser Design Mode.",
    ),
    (
        "plugin_dev_guide",
        "Read the guide for developing TUICommander plugins.",
    ),
    ("settings", "Read and propose application settings."),
    ("debug", "Inspect application logs and runtime state."),
    (
        "voice",
        "Speak replies in the active hands-free conversation.",
    ),
];

/// Full MCP tool definitions — the one native tool family.
///
/// This returns the unfiltered schema list. Public listing/search paths MUST
/// route through [`filtered_native_tools`] to honour `mcp_capabilities`
/// and `progress_tracking`. Leaking the raw list to external clients exposes
/// tool metadata for gated tools.
pub(super) fn native_tool_definitions() -> serde_json::Value {
    let mut tools = native_validation_tool_definitions();
    for tool in tools.as_array_mut().expect("native tool array") {
        super::mcp_input_contract::advertise_schema(tool);
    }
    tools
}

/// Complete private contracts, including conditions unsupported by tool clients.
pub(super) fn native_validation_tool_definitions() -> serde_json::Value {
    let mut tools = native_reference_tool_definitions();
    for tool in tools.as_array_mut().expect("native tool array") {
        let name = tool["name"].as_str().expect("native name");
        if let Some(description) = compact_native_description(name) {
            tool["description"] = serde_json::json!(description);
            let action = &mut tool["inputSchema"]["properties"]["action"];
            let actions = action["enum"].as_array_mut().expect("native actions");
            if !actions.iter().any(|action| action == "help") {
                actions.push(serde_json::json!("help"));
            }
            let action_names = actions
                .iter()
                .map(|action| action.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ");
            if action.get("description").is_some() {
                action["description"] = serde_json::json!(format!("One of: {action_names}"));
            }
        }
    }
    tools
}

/// Full guidance stays on demand; Settings retains its existing tooltip copy.
fn native_reference_tool_definitions() -> serde_json::Value {
    let mut defs = serde_json::json!([
        crate::automations::mcp::tool_definition(),
        crate::secrets::tool_definition(),
        crate::telegram::tool_definition(),
        {
            "name": "session",
            "description": "PTY multiplexer (replaces tmux). Create terminals, send input (send-keys), read output (capture-pane), manage lifecycle.\n\nActions:\n- list: Optional session_id scopes one session; full rows expose known last_activity_ms. compact=true derives activity_age_ms from it (null when unknown) and keeps identity, state and omitted_fields. All active sessions and states in one call, including local and connected remote sessions. Remote rows carry connection_id and address=connection/session_id. Use for every global overview; never fan out per-session status calls. Returns display_name (assigned name), alias (independent repo-derived short address), tuic_session (the stable identity the tab persists), is_caller, shell_state (PTY activity), and agent_state (starting|working|awaiting_input|idle|completed; completed requires suggest marker). Absent optional fields are omitted, not null — background_work and standby appear only when true.\n\nEvery action that takes session_id accepts the PTY id, tuic_session, alias (e.g. tu-1), a unique short PTY-id prefix, or a unique display name.\n- create: New PTY. Returns {session_id}. Optional: cwd, shell, rows, cols.\n- submit: Submit one non-empty command to a confirmed-idle managed agent and wait internally for a bounded receipt. Use one call; never split text and Enter; never poll after it. Returns submission_id, submitted, write_state, acknowledged, retry_safe, turn_epoch, composer_state (tracked InputLineBuffer, not application state), and acknowledgement or a precise reason. Acknowledgement means child terminal movement after Enter, not semantic application acceptance. Never queues; partial composers, dialogs, busy agents, and older queued commands reject before writing.\n- force_retry: Submit /retry once to an ego terminal currently reporting provider recovery. Requires session_id; never queues or retries automatically.\n- input: Raw text/key compatibility surface. Send text and/or special_key; ok confirms PTY write only.\n- output: Read terminal output. Optional byte_budget=1024..65536 returns JSON-text chunks bounded by compact response-envelope UTF-8 bytes (not MCP framing); concatenate chunk values then parse JSON. Follow the opaque continuation with the same action/session; expires after 300s or cache eviction. Omit byte_budget to keep the existing shape. Returns {data, cursor, scrollback_lines, oldest_offset, exited, exit_code}. Use as an anomaly fallback for a child that failed to send its result, not as the normal orchestration channel. The tail read omits an empty input box and everything below it (status line, HUD); format=raw keeps them. scrollback_lines = total lines in buffer (up to 10000); oldest_offset = first available line number. Patterns: (1) Snapshot: omit since_cursor, default limit=50 gives last 50 lines. (2) Delta read: since_cursor=<previous cursor> returns only new lines. (3) Navigate backwards: from_line=oldest_offset reads from the beginning of the buffer. (4) Arbitrary window: from_line=N, limit=50 reads any 50-line slice. Windowed reads report has_more and next_cursor; follow the continuation note to fetch retained output without rerunning the command. Raw pages use from_byte=oldest_offset then from_byte=next_cursor (source-byte positions). Evicted output cannot be recovered.\n- declare_worktree: Declare an existing linked worktree for this authenticated live caller. Requires worktree_path; rejects foreign sessions and repositories. Persists sidebar placement without changing shell cwd. Caller-bound MCP-only request, available via POST /mcp.\n- status: Session state; absent optional fields are omitted.\n- wait: Block (server-side) until session_id is idle or exited (until=idle|exited), or timeout_ms elapses. One cheap call instead of a status polling loop. Returns {met, timed_out, shell_state?, exit_code?}.\n- resize: Change PTY dimensions.\n- rename: Set the tab's display name. Requires name (non-empty). Sticky by default — protected from later OSC/intent title updates unless is_custom=false.\n- keep_open: Keep a managed child open by disabling idle closure with enabled=true; enabled=false restores automatic idle closure. Requires session_id.\n- suspend: End the tab's PTY and agent to free memory and CPU but keep the tab, restorable like after a TUIC restart; the user resumes it from the tab. Refused while the agent is working, a question awaits an answer, or a command runs. Not auto-standby, which only SIGSTOPs and keeps memory. Requires session_id.\n- close: Graceful shutdown (Ctrl+C, waits).\n- kill: Force SIGKILL (use when close fails).\n- pause: Pause output buffering. resume: Resume.",
            "inputSchema": { "type": "object", "properties": {
                "connection_id": { "type": "string", "description": "Configured remote connection qualifier (session list/output/submit; agent list_peers/send). Remote addresses also accept connection/id; local/id addresses the desktop hub." },
                "compact": { "type": "boolean", "description": "Inventory only: identity, state, activity_age_ms and explicit omitted_fields." },
                "byte_budget": { "type": "integer", "minimum":1024, "maximum":65536, "description": "Opt into byte-bounded JSON-text chunks (inbox/output). Bound includes compact JSON page envelope, excludes MCP framing. Default on continuation: 4096." },
                "continuation": { "type": "string", "description": "Opaque token from a byte-budget page; repeat with same action and target. Concatenate chunk values then parse JSON. Expires after 300s or cache eviction." },
                "action": { "type": "string", "description": "One of: list, create, submit, force_retry, input, output, status, wait, resize, rename, keep_open, suspend, close, kill, pause, resume, declare_worktree" },
                "session_id": { "type": "string", "description": "Session address — PTY id, tuic_session, alias, unique short PTY-id prefix, or unique display name. Ambiguous prefixes or names return an error. Required for submit, force_retry, input, output, status, resize, rename, keep_open, suspend, close, kill, pause, resume, wait" },
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
            "description": "AI agent orchestration. There is no separate swarm action: use these agent/session primitives to spawn and coordinate managed peers.\n\nOrchestration in 5 lines:\n1. Managed PTYs auto-bind from $TUIC_SESSION. A headerless external caller calls register without tuic_session to receive an MCP-scoped UUID, or supplies an explicit stable UUID to reclaim it.\n2. Spawn a named peer: spawn name=worker prompt=<task> [agent_type=codex|gemini|...] → {session_id, name}.\n3. Wait for it: agent action=wait (new mail; omit since, the cursor is kept server-side) or session action=wait session_id=<id> until=idle|exited. Cheap blocking call — do NOT poll in a loop. Both cap at 300s: for work that runs longer, or across a reconnect, poll the spawn's task_id with agent action=task_get instead — the outcome is recorded even with nobody waiting.\n4. Talk to it: send to=<peer> message=<text> [urgency=normal|urgent|interrupt]. Normal is the default; use urgent when the recipient must change course before its next step. For normal and urgent mail, the payload is never typed into the recipient's composer. It waits in the recipient's inbox, and an idle/completed recipient may be sent a payload-free generic `agent action=inbox` wake. Urgent sends a payload-free inbox notice to a safe busy Claude/Codex/ego composer for the next tool boundary.\n5. Lifecycle notifications carry state only. Every worker must report task output or blockers with send; use session output only if a child anomalously failed to send.\n\nActions:\n- launch: One durable idempotent managed launch (localhost only). Requires idempotency_key, parent_session_id (registered peer), stable name, keep_open, workspace={existing_path} or {repo_path,branch,base_ref?}, and spawn={prompt,...spawn options}. Read a receipt with launch_id alone, or retry the identical request after a lost reply: the same launch_id/session_id returns, never another checkout or PTY. Conflicting key reuse errors explicitly. Receipt status=created|prompt_delivered|ready|blocked, prompt_delivery=queued|delivered|unknown and warm_artifacts are separate. Delivered proves argv handoff or PTY submission, not model acceptance; ready also requires native Ready-screen evidence. Interrupted uncertain creation blocks rather than spawning again; retain the handle and inspect the named resources. Model/account/brief policy remains caller-owned. Also POST /agents/launch and managed_agent_launch IPC, with identical validation.\n- spawn: Launch agent in new PTY (localhost only). Optional name is assigned before prompt delivery. Returns {session_id, name, task_id, poll_interval_ms, server_ts, parent_session_id?}.\n- task_get: Read a task_id owned by the spawning caller. Returns {task_id, status, status_message?, result?, error_detail?, poll_interval_ms}; optional fields are omitted. Status is working|input_required|completed|failed|cancelled; the last three are immutable. Poll no faster than poll_interval_ms. Outcomes survive client reconnects for up to 24h within this server process.\n- task_cancel: Mark an owned task_id cancelled (localhost only). Does NOT kill the PTY; use session action=kill for that. Already-final tasks return their unchanged status with cancelled=false.\n- wait: Block until new inbox mail. Not byte-bounded: byte_budget/continuation are rejected before mail consumption; use inbox for bounded payloads. Omit `since` — the server resumes from your last read position; pass it only to override (since=0 replays everything). Success inlines every retained fresh message (up to the 100-message inbox capacity) in chronological order. Every response carries next_since, timeout included. An active wait suppresses terminal wake.\n- register: Bind an external/headerless caller, or rename/set the repository path of an auto-bound managed peer. tuic_session is optional; omission generates a stable identity for this MCP connection. Reconnecting under a NEW uuid? Pass `replaces=<old_uuid>` or its inbox is stranded — the response reports superseded_identity, mail_migrated, and mail_stranded + identity_warning when the old identity still owns a live PTY (its mail is left alone). Check `terminal` in the response: false means no PTY notice can be typed into you; a subscribed ACP inbox can still notify you or wake idle ego. Otherwise consume mail with wait/inbox. Declare the orchestrator role with orchestrator=true and remove it with false; spawning a child never infers it, and omitting the field preserves the current role. The response reports mail_wake=managed_pty_lifecycle when a wake can reach you; external/headerless peers without a subscribed ACP inbox stay wait/inbox-only.\n- list_peers: Optional to scopes a recipient through the same resolver as send; compact=true returns identity, state (unknown if absent), activity_age_ms (null if absent), and omitted_fields. List peers across the desktop mail hub and connected daemons. address is connection/peer_id (local/peer_id on the hub); connection_id selects one daemon. Returns tuic_session, name, orchestrator, plus alias and session_id for a peer that owns a live terminal. Optional: path filter. Absent fields are omitted.\n- send: Message any local or remote peer (requires to, message). Use to=connection/peer_id or connection_id with a daemon-local address. The owning daemon performs inbox delivery and wake; replies return through the desktop hub. `to` accepts the peer's tuic_session, PTY id, or terminal alias. `urgency` is normal (default) or urgent; use urgent when the recipient must change course before its next step. Urgent keeps the body in the inbox and writes only a notice to a safe busy Claude/Codex/ego composer. `urgent_delivered` reports a PTY notice, subscribed ACP inbox update, inbox read, or waiter; false adds `urgent_fallback_reason` for queued mail. It does not prove model action or interrupt a tool. `delivered` and `delivery_path` describe the routing path; `recipient_state` appears only for a managed PTY. Opt-in urgency=interrupt sends Esc to a safe working Claude Code, Codex or ego, waits up to 10 seconds for confirmed idle, then submits the full message as one new input (including for an orchestrator). An already idle recipient needs no Esc. A dialog/approval or draft blocks both Esc and payload. Unsupported agents, no PTY and idle timeout use the existing urgent notice/inbox fallback. The result adds interrupt_delivered and interrupt_fallback_reason; delivery_path=interrupt_payload_and_inbox means the full input was written, not that the model acted. Interrupted tools can leave partial work; child processes may continue. An active waiter owns delivery and needs no interrupt.\n- inbox: Optional byte_budget=1024..65536 returns JSON-text chunks; the budget includes the compact page envelope, not MCP framing. Concatenate chunk values then parse JSON; continue with the opaque continuation and same action. Snapshots expire after 300s or cache eviction; errors require a fresh read. The server cursor advances only on the final chunk. Read up to 100 retained messages in FIFO order. Returns next_since and has_more; repeat while has_more is true. Optional: limit (default 100, max 100), since (omit to resume from the server-side cursor). On FIFO eviction, missed_count reports unread messages lost since the last inbox read.\n- subscribe: Create or resume a durable independent reader with subscription_id (1..64 ASCII letters/digits/_/-) and filter=actionable (default), all, exit or failure. Actionable excludes routine system lifecycle/idle traffic, not peer mail. A subscription does not consume legacy inbox or another reader.\n- inbox/wait with subscription_id: Return up to limit (1..100) matching journal messages, cursor, acknowledged_cursor, missed_count and has_more. No byte_budget/continuation; ACK then read has_more batches. Reads do not acknowledge. An unacknowledged batch is replayed unchanged while retained. wait returns only for matching mail or matching retention loss; routine traffic stays inside the server. For continuous event-driven delivery use GET /agent/subscriptions/{id}/events with your Mcp-Session-Id header, or over ACP subscribe to resource tuic://inbox/subscriptions/{id}.\n- ack: Acknowledge a processed subscription batch with subscription_id and its cursor. Acknowledged results never replay on reconnect under the same peer identity and subscription_id. Lost replies before ack can replay: deduplicate side effects by message id. Journal retains the newest 1000 messages per peer across restarts; missed_count counts matching unacknowledged evictions, not routine filtered events. since is legacy-only.\n- unsubscribe: Delete one subscription cursor; retained mail and other readers are unchanged.",
            "inputSchema": { "type": "object", "properties": {
                "connection_id": { "type": "string", "description": "Configured remote connection qualifier (session list/output/submit; agent list_peers/send). Remote addresses also accept connection/id; local/id addresses the desktop hub." },
                "compact": { "type": "boolean", "description": "Inventory only: identity, state, activity_age_ms and explicit omitted_fields." },
                "byte_budget": { "type": "integer", "minimum":1024, "maximum":65536, "description": "Opt into byte-bounded JSON-text chunks (inbox/output). Bound includes compact JSON page envelope, excludes MCP framing. Default on continuation: 4096." },
                "continuation": { "type": "string", "description": "Opaque token from a byte-budget page; repeat with same action and target. Concatenate chunk values then parse JSON. Expires after 300s or cache eviction." },
                "action": { "type": "string", "description": "One of: launch, spawn, wait, register, list_peers, send, inbox, subscribe, ack, unsubscribe, task_get, task_cancel" },
                "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256, "description": "launch: stable retry key scoped to parent; do not change after a lost reply." },
                "launch_id": { "type": "string", "description": "launch: read only a durable receipt by handle; omit all creation fields." },
                "task_id": { "type": "string", "description": "Task handle returned by agent spawn (required for task_get/task_cancel)." },
                "parent_session_id": { "type": "string", "description": "launch: registered parent peer identity." },
                "workspace": { "type": "object", "description": "launch: exactly {existing_path} or {repo_path,branch,base_ref?}. Never falls back to another checkout." },
                "spawn": { "type": "object", "description": "launch: existing spawn options including prompt, agent_type, binary_path, args, model and env. cwd/name/keep_open belong to the transaction and are rejected here." },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": 300000, "description": "Max wait in ms (action=wait; default 60000). Values at or above 300000 run as 295000 so the reply beats a 300s client-side tool-call deadline. On timeout returns {timed_out:true}." },
                "prompt": { "type": "string", "description": "Task prompt for the agent (action=spawn)" },
                "pty_description": { "type": ["string", "null"], "description": "Short description of the PTY task shown above the terminal (action=spawn)" },
                "cwd": { "type": "string", "description": "Working directory (action=spawn)" },
                "model": { "type": "string", "description": "Structured model flag; preserved when args is also set (action=spawn)" },
                "env": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Child environment overrides. Applied after run-config env and before protected TUIC_SESSION/TUIC_PARENT (action=spawn)" },
                "print_mode": { "type": "boolean", "description": "false (default): visible TUI tab, observable via agent(inbox). true: headless, no tab. (action=spawn)" },
                "output_format": { "type": "string", "description": "Output format, e.g. 'json' (action=spawn)" },
                "agent_type": { "type": "string", "description": "Agent type OR run config name. Resolved as: (1) run config name match across enabled agents, (2) agent binary name (claude, codex, goose, gemini, ...). Case-insensitive. (action=spawn)" },
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
                "urgency": { "type": "string", "enum": ["normal", "urgent", "interrupt"], "description": "action=send: normal (default) and urgent keep payloads in the inbox; urgent also sends a safe busy inbox notice without interrupting. interrupt explicitly requests Esc, up to 10 seconds for confirmed idle, then full message submission for Claude/Codex/ego. Dialogs/drafts block input; unsupported or timed-out sessions use urgent fallback. Inspect interrupt_delivered, interrupt_fallback_reason and delivery_path. Interrupted tools may leave partial work." },
                "keep_open": { "type": "boolean", "description": "action=launch: required explicit child lifetime; action=spawn: keep this managed child open; action=send: set or clear the recipient managed child's keep-open flag." },
                "limit": { "type": "integer", "minimum": 1, "maximum": crate::state::AGENT_INBOX_CAPACITY, "description": "Maximum inbox entries to return (action=inbox; default 100, maximum 100). Read again while has_more is true." },
                "subscription_id": { "type":"string", "minLength":1, "maxLength":64, "pattern":"^[A-Za-z0-9_-]+$", "description":"Independent durable reader (subscribe, inbox, wait, ack, unsubscribe). Reuse the same peer identity and id on reconnect." },
                "filter": { "type":"string", "enum":["actionable","all","exit","failure"], "description":"Immutable subscription filter; default actionable. Exit includes failed exits; failure includes nonzero exits and delivery failures." },
                "cursor": { "type":"integer", "minimum":0, "description":"Returned subscription batch cursor to acknowledge (ack only); not a legacy since timestamp." },
                "since": { "type": "integer", "description": "Logical unix-millis cursor (action=inbox|wait). OMIT IT: the server remembers your last read position and resumes from there. Pass it only to override — since=0 deliberately replays the whole inbox. Every wait/inbox response carries next_since, including on timeout" }
            }, "required": ["action"] }
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
            "description": "Repository and version control. Query workspace repos, their GitHub PR/CI status, and manage git worktrees.\n\nActions:\n- add: Register and activate an existing local directory. Requires absolute path. Persists a usable Git or shell workspace, preserves existing terminals/settings on repeat calls, and broadcasts repositories-changed. No desktop URL handler required; does not spawn a terminal. Returns ok, path, name, branch, is_git_repo, and an optional watcher warning.\n- list: Open repos with branch, dirty status, worktrees.\n- active: Focused repo path, branch, group.\n- status: Cross-repo GitHub PR and CI summary {path, branch, ahead, behind, open_prs, failing_ci}. Optional path limits it to that registered repo; an unregistered path is an error.\n- branch_integrations: Integration status and proof for every local branch, including worktree_paths. Requires path.\n- branch_integration: Same verdict for one branch. Requires path and branch. Returns tip, integrated, proof, archive_required, archived and archive_ref. Content-based proof requires an archive at that tip before deletion; unknown is never proof.\n- worktree_list: Worktrees for a repo. Requires path. Each entry includes lifecycle_status {commit_status: merged|unmerged|in_sync|unknown, dirty_files, removal_safety: safe|requires_force|unknown}, the same verdict worktree_lifecycle and the sidebar use.\n- worktree_lifecycle: Fresh safety, fingerprint and submodule commit counts. Requires path and branch.\n- worktree_create: Create a linked worktree. Requires path. Optional: branch, base_ref, spawn_session (starts a bare shell PTY, not an agent). Refs and objects are shared with the parent; parent tracked changes are not copied. Git-ignored build directories warm in the background. Wait for warm_artifacts.status in worktree_list to become done or failed before installing dependencies or building.\n- worktree_remove: Remove worktree. Requires path and branch, or path and worktree_path for a detached checkout (removed only when clean, reachable from a branch and free of live sessions).\n- orphan_cleanup_answer: Answer the pending orphan-removal dialog for path with decision=remove|keep; remove rechecks every worktree for uncommitted/untracked files and branch reachability.\n- branch_delete: Delete only a local branch with no checkout after proving its commits are integrated or preserved. Requires path and branch. Proof is in_sync, a merge proof, patch_equivalence, or archived (refs/archive/<branch> points at the exact tip; the response then carries archive_ref and the archive ref is kept). Refuses current/default branches, unmerged commits with no exact archive, and unsafe or changed refs; never touches a remote.\n- work_item: Read Tasks detail and comments. Omit the target for the caller worktree link, or pass reference {providerId,accountId,id}, url, or key with provider_id/account_id. Pass cursor from comments.nextCursor for another comment page (limit 1..100, default 50). All provider text is returned in an untrusted-content block. Requires a running frontend with the provider loaded.\n- work_item_act: Comment or transition through the Tasks provider. Requires mcp_work_item_writes=true (default false) and operation {kind:comment|transition,value:string}; transition values are provider action IDs, not labels. Uses the same target selection. A timed-out write may have completed: read before retrying.\n- progress_list: The project's journal, newest first, paged with total and nextCursor. Requires path. Optional input.blockedOnly, input.ptyId, input.limit (default 8, maximum 100), input.cursor (previous nextCursor). Record a NEW outcome with the `progress` tool, not here.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: list, add, active, status, branch_integrations, branch_integration, worktree_list, worktree_lifecycle, worktree_create, worktree_remove, orphan_cleanup_answer, branch_delete, progress_list, work_item, work_item_act" },
                "path": { "type": "string", "description": "Absolute path to git repository (required for worktree_list, worktree_lifecycle, worktree_create, worktree_remove, branch_delete, progress_list; optional filter for status)" },
                "reference": { "type": "object", "description": "WorkItemRef for work_item/work_item_act", "properties": {"providerId":{"type":"string"},"accountId":{"type":"string"},"id":{"type":"string"}}, "required":["providerId","accountId","id"], "additionalProperties":false },
                "url": {"type":"string", "description":"Work-item URL resolved by the loaded plugin"},
                "key": {"type":"string", "description":"Exact provider key; requires provider_id and account_id"},
                "provider_id": {"type":"string", "description":"Provider owning key"},
                "account_id": {"type":"string", "description":"Account owning key"},
                "cursor": {"type":"string", "description":"Next comment cursor returned inside the content block"},
                "limit": {"type":"integer", "minimum":1, "maximum":100, "description":"Comment page size (default 50)"},
                "operation": {"type":"object", "description":"Opt-in work_item_act operation", "properties":{"kind":{"type":"string","enum":["comment","transition"]},"value":{"type":"string"}}, "required":["kind","value"], "additionalProperties":false},
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
            "name": "workflow",
            "description": "Durable project work for the calling managed session. Actions: story (input=StoryAction), run (input=RunAction), proposal_create, report, launch. Each action has a generated typed input; unknown fields are rejected before effects. story reads and updates native stories, plans and Ideas: add_idea (text, optional source; repo is the calling project; source defaults to bound session), list_ideas (project plus global Ideas), update_idea (idea_id, text; project-owned only); Idea replies use type=idea or ideas and camelCase notes.json records; all writes publish ideas-changed and stale UI saves merge per id and field. Native plan actions: create_plan, update_plan (plan_id, expected_revision, patch={title?,source?}; atomic persisted revision check; changing source requires no running plan coordinator, finish or cancel it first; title-only/unchanged source allowed), list_plans (optional archived=true selects archived plans; default false), list_plan_sources, add_plan_source, get_plan, plan_state, plan_view, filtered_plan_view (plan_id and filter={visibility: all|open|closed, storyType: feature|fix|refactor|chore|task|null, untypedOnly: boolean, archived: boolean (default false)}; type and untypedOnly are mutually exclusive), summary, create_story, update_story, list_stories (plan_id; optional conjunctive state, priority=1..3, story_type, archived (default false; true selects archived stories); optional limit=1..100 and cursor; absent limit returns all filtered stories in id order, limited replies keep value as Story[] and add next_cursor when more matches remain; cursor is bound to plan and filters), get_story, transition_history, archive_story, unarchive_story, archive_plan, unarchive_plan (id and expected_revision; reversible, refused while claimed or run-owned), story_archive_history, plan_archive_history, add_note (story_id, expected_revision, nonblank text up to 16000 characters, optional receipts=[{runId,commandId}] up to 16 distinct persisted same-project/plan receipt references; backend actor/time/current subject revision, no story revision change, append-only), list_notes (story_id; append order), add_dependency, remove_dependency, claim and transition; actor identity is provenance and never restricts an action. update_story takes story_id, expected_revision and a camelCase patch for title, priority, fileScope, origin, storyType, problem, risk, ordered criteria and dependencies; only unclaimed backlog/ready stories without active run ownership may be amended; nullable storyType/problem/risk clear their fields, changed criteria are unchecked; remove_dependency remains administrative; add_dependency on a ready story moves it to backlog when its dependency is not done or lacks a required integration receipt; run starts pinned published graphs and reads ordered run history. proposal_create requires the active bound coordinator and {runId,proposalKey,story}; retry the same key and payload to return the same story, never substitute ordinary story create. report submits the typed AttemptReport for this live session's bound attempt; it does not advance story status. launch binds a pinned workflow attempt to an isolated registered worktree and renders its role prompt. Existing project, revision, coordinator, attempt ownership and local launch checks apply. Scheduled definitions remain in the separate automations tool.",
            "inputSchema": super::mcp_transport_ancillary::workflow_action_schema()
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
            "description": "Control TUIC UI. Actions:\n- tab: open/update panel tab. Requires id, title, + html OR url.\n- toast: non-blocking notification. Requires title. Optional: message, level (info/warn/error), sound, urgency (normal/urgent).\n- confirm: blocking dialog, shown on every client (desktop, browser, mobile PWA) plus a mobile push — the first answer wins. Returns {confirmed}, plus {reason} when it expired unanswered after 300s (treat that as a refusal, not a yes). Requires title, except settings proposals: pass proposalId to show the backend redacted diff; model title/message/confirmed cannot grant approval.\n- design_start: start Design Mode with session_id and optional browser/url. Requires a live terminal agent.\n- design_stop: stop the caller-bound repository (repo_path).\n- design_status: list Design Mode bindings for an armed caller.\n- design_reload: reload the reviewed tab (repo_path).\n- design_navigate: navigate the reviewed tab (repo_path, url); remote hosts require an extension host grant.\n- design_screenshot: capture the reviewed tab as a PNG path (repo_path).\n- design_errors: read captured page errors (repo_path).\n- design_eval: evaluate script in the reviewed tab (repo_path, script); requires an explicit optional extension host grant.\n- screenshot: capture a panel as WebP. Requires id. Returns {path}. Read the path to view.\n\nURL schemes for tab:\n- http(s): loaded in sandboxed iframe using the permissive default-src CSP (external HTTPS and localhost HTTP supported). Site frame policies can still refuse embedding.\n- file:///path: read via IPC and rendered as inline HTML (sandbox blocks direct file:// access).\n- tuic://edit/<path>?line=N: native code editor (no iframe). Prefix absolute paths with `//` (tuic://edit//Users/x/a.rs). Relative = active repo.\n- tuic://open/<path>: native markdown/preview tab.\n\n- tuic://open-repo is rejected; use repo action=add with path to register a directory. Other tuic actions are not tab content.\n\nCustom schemes (vscode://) do NOT work in iframes.\n\nUse:\n- A project outcome — a milestone, a decision, a discovery, a blocker — belongs to the `progress` tool, which records it AND shows a toast. `ui action=toast` is the transient half only: it says something happened and keeps no record of it.\n- toast for done/error/long-job end; error=failure, warn=recoverable. Skip for micro-steps.\n- toast urgency: normal (default) records the notice in the bell only; urgent also shows a transient card on the desktop window. The caller chooses; the card never covers the focused input.\n- toast with sound=attention when you are working unattended and are BLOCKED on the user (question, approval, ambiguous requirement). It is the only sound that carries across a room; do not spend it on progress updates.\n- confirm BEFORE destructive ops (rm -rf, git reset --hard, force-push, DROP). Only proceed if confirmed.\n- tab http(s) for dashboards, reports, >20-line structured output.\n- tab tuic://edit to point user at source file+line (review, bug discussion) — beats pasting snippets.\n- screenshot to visually verify rendered HTML content in a panel you created.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: tab, toast, confirm, screenshot, design_start, design_stop, design_status, design_reload, design_navigate, design_screenshot, design_errors, design_eval" },
                "id": { "type": "string", "description": "Stable identifier for dedup — same id reuses existing tab (action=tab, required)" },
                "title": { "type": "string", "description": "Tab or notification title (action=tab/toast/confirm, required except settings proposal confirmation)" },
                "proposalId": { "type": "string", "description": "Opaque backend-issued settings proposal (action=confirm). Displays the backend redacted diff; ignores supplied title/message. Human answer required; never accepts a confirmed flag." },
                "html": { "type": "string", "description": "Inline HTML content to render in sandboxed iframe (action=tab, mutually exclusive with url)" },
                "url": { "type": "string", "description": "Tab URL (action=tab, xor html). http(s) → iframe. file:///path → read and inline. tuic://edit/<path>?line=N → native editor. tuic://open/<path> → markdown tab. Absolute paths need `//` prefix." },
                "pinned": { "type": "boolean", "description": "Pin tab across all branches (default false)" },
                "focus": { "type": "boolean", "description": "Switch to this tab when it is first opened (action=tab, default false: reports open in the background and never steal terminal input focus). Updates of an open tab never switch." },
                "message": { "type": "string", "description": "Optional body text (action=toast/confirm)" },
                "level": { "type": "string", "description": "Toast level: info, warn, error (default: info)" },
                "urgency": { "type": "string", "enum": ["normal", "urgent"], "description": "action=toast (default: normal, same names as agent action=send). normal = bell only. urgent = a transient card on the desktop window plus the bell entry." },
                "sound": { "description": "Audible signal for action=toast (default: none). true = the tone matching `level`. Or name one: question, completion, error, warning, info, attention. `attention` is a triangular G4→G4→E5 callback, unlike any other sound in the app — use it when you are running unattended and need the user back (a blocking question, a decision only they can make). Plays through the user's notification settings, so volume, output device and mutes are respected.", "anyOf": [{ "type": "boolean" }, { "type": "string", "enum": ["question", "completion", "error", "warning", "info", "attention"] }] }
            }, "required": ["action"] }
        },
        {
            "name": "plugin_dev_guide",
            "description": "Returns comprehensive plugin authoring reference: manifest format, PluginHost API (all 4 tiers), structured event types, and working examples. Call before writing any plugin code.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "settings",
            "description": "Describe, read and edit Rust-owned settings. Actions: describe, get, set, reset, patch, perform, refresh. Describe pages are bounded; writes require the authoritative expectedRevision and explicit target. Secrets are write-only. Exposure or execution-authority edits require ui confirm with the backend proposalId, then replay the exact request with approval. Prompts use the prompts store and stable IDs. Disabled by default; only trusted UI can enable access.",
            "inputSchema": crate::settings::api::request_schema()
        },
        {
            "name": "debug",
            "description": "Diagnostics for TUICommander internals. action=help returns the full usage guide.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "description": "One of: agent_detection, logs, sessions, invoke_js, frame_workload_dump, help" },
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

    for tool in defs.as_array_mut().expect("native tool array") {
        super::mcp_input_contract::complete_schema(tool);
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

/// Return the complete displaced guidance, never a synthesized subset.
pub(super) fn native_tool_help(name: &str) -> Option<serde_json::Value> {
    compact_native_description(name)?;
    native_reference_tool_definitions()
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
        .map(|tool| serde_json::json!({"description":tool["description"]}))
}

/// Agent-facing action contracts. Field types and bounds remain in the schema.
fn compact_native_description(name: &str) -> Option<&'static str> {
    Some(match name {
        "session" => {
            "PTY multiplexer (tmux): input=send-keys, output=capture-pane. help: full usage and response guide. All session_id actions accept PTY id, tuic_session, alias, unique short PTY prefix or unique display name; ambiguity errors.\nlist: local and connected remote inventory; optional session_id/compact. All active sessions and states in one call; never fan out per-session status calls. Optional fields are omitted, not null; background_work/standby only when true. completed requires a suggest marker.\ncreate: new PTY. submit: one non-empty command to a confirmed-idle managed agent; bounded receipt. Use one call; never split text and Enter; never poll after it. Never queues; partial composers, dialogs, busy agents, and older queued commands reject before writing. Acknowledgement means child terminal movement after Enter, not semantic application acceptance. composer_state (tracked InputLineBuffer, not application state). force_retry: submit /retry once only during ego provider recovery; never queues or retries automatically. input: Raw text/key compatibility surface. Send text and/or special_key; ok confirms PTY write only.\noutput: bounded retained terminal output; anomaly fallback only when a child failed to send its result, not normal orchestration. Tail hides empty composer/HUD; raw preserves it. Byte-budget chunks bound compact envelope bytes, not MCP framing; concatenate chunk values then parse JSON. Continue with same action/session and opaque token; expires after 300s/eviction. Follow has_more/next_cursor without rerunning commands; raw cursors are source bytes. Evicted output is unrecoverable.\n- status: state. wait: server-side idle/exited wait, not a polling loop. resize: dimensions. rename: non-empty display name; sticky unless is_custom=false. \n- keep_open: managed child enabled=true disables idle closure; enabled=false restores it. suspend: end PTY/agent, keep restorable tab; refused while working, awaiting answer or running a command; not SIGSTOP standby. close: graceful shutdown. kill: SIGKILL only if close fails. pause: stop buffering. resume: restart buffering. declare_worktree: authenticated live caller only, existing linked worktree in its repository, no foreign session; persists placement without changing cwd; MCP-only via POST /mcp. Executing/destructive session actions require localhost."
        }
        "agent" => {
            "Managed peers and mail. There is no separate swarm action. help: full orchestration, response and subscription guide. Managed PTYs auto-bind from TUIC_SESSION; a headerless external caller calls register without tuic_session. Every worker must report task output or blockers with send. Lifecycle notifications carry state only. Use session output only for anomalously missing results. Wait: do NOT poll in a loop; waits cap at 300s.\nlaunch: localhost-only durable idempotent launch. Requires registered parent_session_id, stable idempotency_key/name, explicit keep_open, workspace={existing_path}|{repo_path,branch,base_ref?}, spawn={prompt,...}. Read with launch_id alone; identical retry returns same resources, conflicting key errors. Uncertain interrupted creation blocks: retain handle and inspect, never spawn again. Delivered proves argv/PTY handoff, not model acceptance; ready also needs native Ready-screen evidence. Model/account/brief policy is caller-owned.\nspawn: localhost-only new PTY, name assigned before prompt; returns task_id. task_get: caller-owned outcome retained up to 24h in this server process, including reconnect; working|input_required|completed|failed|cancelled, last three immutable. Poll no faster than poll_interval_ms. task_cancel: localhost-only, caller-owned; final status unchanged, does NOT kill PTY (session kill does).\nregister: optional stable UUID (omission generates MCP-scoped identity); reconnect with a NEW UUID must pass replaces or strand mail. Live-PTY identity cannot be superseded: check mail_stranded/identity_warning. No terminal means no PTY notice; subscribed ACP can notify/wake idle ego, otherwise wait/inbox only. orchestrator=true/false sets/removes role; omission preserves it, spawning a child never infers it. mail_wake=managed_pty_lifecycle identifies reachable PTY wake. list_peers: local/remote peers; optional to/path/connection_id/compact, absent fields omitted.\nsend: requires to/message; peer UUID, PTY id, alias, unique prefix/name or connection/peer_id. Normal is the default; urgent when the recipient must change course before its next step. For normal and urgent mail, the payload is never typed into the recipient's composer. Idle/completed recipients may get a payload-free generic `agent action=inbox` wake. Safe busy Claude/Codex/ego gets only a payload-free notice. urgent_delivered does not prove model action or interrupt tools; false supplies urgent_fallback_reason. Explicit urgency=interrupt sends Esc to a safe working Claude/Codex/ego, waits up to 10 seconds for confirmed idle, then submits the full message as one input; an idle recipient skips Esc. Dialogs/approvals and drafts block Esc/text. Unsupported agents or idle timeout use urgent fallback. Inspect interrupt_delivered, interrupt_fallback_reason and delivery_path; interrupt_payload_and_inbox proves input written, not model action. Interrupted tools may leave partial work or running children. Active waiters own delivery without interruption.\ninbox: FIFO up to 100, omitted since resumes server cursor, since=0 deliberately replays; follow has_more. Byte-budget chunks include compact envelope, not MCP framing; concatenate/parse, same action/token; expire after 300s/eviction. Cursor advances only on final chunk; fresh read after error; missed_count reports unread eviction. wait: Block for fresh mail inline, next_since even on timeout; active wait suppresses terminal wake. byte_budget/continuation rejected before consumption.\nsubscribe: independent durable subscription_id (1..64 ASCII letters/digits/_/-), immutable actionable|all|exit|failure filter; actionable excludes routine lifecycle, never peer mail. Subscription inbox/wait: limit 1..100, no byte_budget/continuation or since; reads do NOT acknowledge, unacked retained batch replays unchanged; ACK before following has_more. Matching mail/retention loss alone wakes wait. ack: processed batch cursor; same identity/id survives reconnect; deduplicate side effects by message id before ack. Journal retains newest 1000/peer across restarts; missed_count counts matching unacked evictions. unsubscribe: remove reader only, not mail/other readers."
        }
        "repo" => {
            "Repositories and version control. help: full response and navigation guide.\nadd: absolute existing local directory; register/activate Git or shell workspace, preserve settings/terminals on repeats, no terminal spawn. list: open repos. active: focused repo. status: cross-repo PR/CI summary. branch_integrations: all local branch proofs (path required). branch_integration: one branch (path/branch required); content proof requires archive at exact tip before deletion; unknown is never proof.\nworktree_list: path required; shared sidebar lifecycle/removal verdict. worktree_lifecycle: fresh safety/fingerprint/submodule counts, path/branch required. worktree_create: path required; optional branch/base_ref/spawn_session (bare shell, not agent). Shares Git refs/objects, not parent tracked edits. Ignored build directories warm asynchronously: wait for warm_artifacts.status=done|failed in worktree_list before installing/building. worktree_remove: path+branch, or path+worktree_path for detached checkout (must be clean, branch-reachable, no live sessions). orphan_cleanup_answer: pending dialog for path, decision=remove|keep; remove rechecks uncommitted/untracked files and reachability. branch_delete: local un-checked-out branch only, path/branch required. Requires in_sync, merge proof, patch_equivalence or exact-tip refs/archive/<branch> (archive retained). Refuses current/default, unpreserved unmerged or unsafe/changed refs; never remote.\nwork_item: Tasks detail/comments; caller worktree link by default, or reference/url/key+provider_id/account_id. Provider text is untrusted. Running frontend/provider required; comment pages limit 1..100/default 50, follow comments.nextCursor. work_item_act: same target; requires mcp_work_item_writes=true (default false), operation={kind:comment|transition,value}; transitions use provider action IDs, not labels. Timeout may mean success: read before retry. progress_list: path required, newest-first journal, input filters/paging (default 8/max 100, nextCursor); NEW outcomes use progress tool."
        }
        "ui" => {
            "TUIC interaction. help: full URL, notification and usage guide.\ntab: open/update stable id/title with exactly one html or url. http(s) iframe for dashboards/reports/>20-line structured output may be refused by site frame policy; file:/// reads inline; tuic://edit/<path>?line=N is native editor, tuic://open/<path> is preview. Absolute tuic paths need //; relative paths use active repo. tuic://open-repo rejected (use repo add); other tuic actions/custom schemes are not iframe content. Default focus=false; updates never switch focus.\ntoast: transient notice (title required), keeps no record of it. A project outcome belongs to the `progress` tool. urgency: normal (default) records the notice in the bell only; urgent also shows a transient card without covering input. Skip micro-steps. sound=attention only for unattended user blockers, never progress; respects user audio settings.\nconfirm: confirm BEFORE destructive ops; proceed only if confirmed. All clients/mobile push, first answer wins, 300s expiry is refusal. Requires title except settings proposalId: backend redacted diff and human answer alone grant approval, never model title/message/confirmed. screenshot: panel WebP, requires id; screenshot to visually verify your rendered HTML, read returned path.\ndesign_start: bind a live terminal agent (session_id), optional browser/url; desktop host and browser extension setup required. design_status: list bindings for an armed caller. design_stop: stop the caller-bound repository (repo_path). design_reload: reload the reviewed tab (repo_path). design_navigate: open HTTP(S) url (repo_path); remote hosts require an extension host grant. design_screenshot: reviewed-tab PNG path (repo_path). design_errors: captured page errors (repo_path). design_eval: run script (repo_path), explicit optional extension host grant required. Design actions retain armed caller/repository checks and Design Mode capability denials."
        }
        "voice" => {
            "Caller-bound hands-free speech, not application audio. help: full conversation/utterance guide. status: check first; if available is false reply in text. Mode notices come from TUICommander rather than from the user; off is final. The user can turn those notices off: absence proves nothing, status does. You do not choose the language or voice: write text in status.language. Speech supplements, never replaces normal reply.\nspeak: text required, max 2000 characters; supply status/notice turn to reject a stale reply, omission means now. Accepting a reply is NOT the same as the user hearing it: queued utterance_id is not heard; inspect status with that id; finished=heard to end, interrupted=user talked (normal), failed=never audible, unknown=too old. stop: immediately stop/drop queue and open new turn; use when reply is wrong."
        }
        "telegram" => {
            "Caller-bound Telegram. help: full request/draft/routing guide. register: opt in own MCP-bound peer, replaces/notifies previous agent and revokes pending requests. unregister: release. Registration ends on agent exit, PTY close, MCP end or daemon restart. Without registration latest-connected live AI Chat receives inbound; only absent both is text dropped with bot notice.\nbegin: optional idempotent acknowledgement for current request. activity: concise safe unapproved preview. finish: complete exact authoritative reply, even after draft/turn ends, deduplicates completion. Request begin/activity/finish remain bound to recipient peer, never another AI Chat. AI Chat streams assistant text only; Telegram-woken turn settles its own final text unless finish replied. send: any live MCP-bound peer without registration; single allowlisted destination. Non-owner: server-derived [session name] on every chunk, no buttons, 30-second per-peer interval. Owner: unprefixed, byte-exact text. Replies to prefixed text select the unique live exact-name peer; other replies and new messages use inbound owner. Optional {label,data} button rows are owner-only. Buttons yield structured mail, NOT publish approval. Never stream raw terminal output or hidden reasoning."
        }
        "automations" => {
            "Scheduled definitions and runs (separate from workflows). help: full create/response guide. create: definition={name,prompt,run_config,in_secs?}; host resolves in_secs, exclusive with cron/once_local. run_config must be actual caller saved profile, never agent-type default. Create/update/re-enable validate profile names; unresolved returns repair choices; list reports run_config_error/no next_run_ms. Agent-created failed runs notify creator once, including pre-spawn. Bound caller may omit context fields; unbound HTTP/IPC supplies normal definition fields (cron optional for Once); missing context names field. Identical generated create/caller deduplicates; explicit new id creates separate reminder. create returns saved local fire time/next_run_ms: no date/list/get needed. update: definition or id+enabled. list: definitions. get: definition. pause: disable. resume: enable. delete: remove definition, retain history. run_now: trigger. run: inspect run. list_runs: history. summary: totals. preview: schedule. preview_definition: draft schedule. preset: resolve. templates: disabled drafts. template: one draft. worktree_preview: inspect. cleanup_worktree: safe removal."
        }
        _ => return None,
    })
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
         list of tool names + one-line summaries, plus a complete top_schema when it fits \
         schema_byte_budget (default 16384 UTF-8 bytes, maximum 65536; 0 disables it). \
         Use top_schema directly; otherwise call `get_tool_schema` for the selected tool. A tool already on your \
         list, or one whose schema you fetched earlier on this connection, is callable \
         without searching for it again.\n\n\
         Domains available: terminal pane sessions (tmux replacement), AI agent orchestration + \
         messaging, repos/GitHub PRs/worktrees, UI tabs + notifications, plugin authoring \
         reference, typed settings, diagnostics{upstream_suffix}."
    );

    serde_json::json!([
        {
            "name": "search_tools",
            "description": search_desc,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Plain words rank names and descriptions. Each +term must occur in the tool name (case-insensitive, including alias prefixes); several use AND. Optional words rank matching names. select:<name> selects an exact catalogue name without fallback." },
                    "limit": { "type": "integer", "description": "Maximum number of results, default 10" },
                    "schema_byte_budget": { "type": "integer", "minimum": 0, "maximum": 65536, "description": "Serialized UTF-8 byte budget for the complete top_schema; default 16384, 0 omits it" }
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
            "description": "Invoke a TUICommander tool by name with arguments. Dispatches to native tools or upstream-proxied tools (`{upstream}__{tool}`). The arguments object must match the tool's inputSchema.\n\nFirst time with a tool: `search_tools(query=\"…\")` → pick a name → use the returned `top_schema` or `get_tool_schema(tool_name=…)` → `call_tool(tool_name=…, arguments={…})`. After that, call it straight away with the schema you already have; re-fetch it only after a `notifications/tools/list_changed`, which is the one thing that can move it. Never skip discovery for a name you have NOT seen from `search_tools` or `get_tool_schema`: an invented name is a failed call, not a lucky guess.",
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
/// Otherwise (default), returns native tools filtered by `mcp_capabilities`,
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
    native_reference_tool_definitions()
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

/// Shared discovery/dispatch gate. Conditions describe owner configuration, not
/// caller authority: conversation binding, registration and deployment consent
/// remain enforced by the respective handlers.
pub(super) fn native_tool_unavailability(state: &AppState, name: &str) -> Option<String> {
    native_action_unavailability(state, name, None)
}

pub(super) fn native_action_unavailability(
    state: &AppState,
    name: &str,
    action: Option<&str>,
) -> Option<String> {
    if !state.config.read().mcp_capabilities.allows(name, action) {
        return Some(format!("Tool '{name}' is disabled by configuration"));
    }
    native_configuration_unavailability(state, name)
}

fn native_configuration_unavailability(state: &AppState, name: &str) -> Option<String> {
    let reason = match name {
        "remote" if !state.remote.has_connected_server() => "no connected remote server",
        "voice" if !voice_configured() => "voice is not configured",
        "telegram" if !crate::telegram::settings::configured() => "Telegram is not configured",
        _ => return None,
    };
    Some(format!("Tool '{name}' is unavailable: {reason}"))
}

/// Settings keeps unavailable domains visible and reports configuration independently of denials.
pub(crate) fn native_capability_catalog(state: &AppState) -> Vec<serde_json::Value> {
    crate::mcp_capabilities::catalog()
        .into_iter()
        .map(|mut capability| {
            if capability["section"] == "conditional" {
                let domain = capability["domain"].as_str().unwrap_or_default();
                let reason = native_configuration_unavailability(state, domain);
                let available = reason.is_none();
                let availability = reason.unwrap_or_else(|| match domain {
                    "remote" => "Remote server connected".into(),
                    "voice" => "Voice configured".into(),
                    "telegram" => "Telegram configured".into(),
                    _ => unreachable!(),
                });
                capability["available"] = serde_json::json!(available);
                capability["availability"] = serde_json::json!(availability);
            }
            capability
        })
        .collect()
}

fn voice_configured() -> bool {
    // An unset engine resolves to Edge, so engine selection is not an opt-in.
    // Read the persisted owner flags and fail closed on a missing/corrupt file.
    std::fs::read(crate::config::config_dir().join("dictation-config.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<crate::config::DictationConfig>(&bytes).ok())
        .is_some_and(|config| config.enabled && config.hands_free_spoken_replies)
}

/// Apply the same native availability gate used at dispatch to every catalogue.
pub(super) fn filtered_native_tools(state: &Arc<AppState>) -> Vec<serde_json::Value> {
    let (progress_tracking, settings_enabled) = {
        let cfg = state.config.read();
        (cfg.progress_tracking, cfg.settings_mcp_enabled)
    };
    native_validation_tool_definitions()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            let name = tool["name"].as_str().unwrap_or("");
            // This index is shared across callers; per-agent progress policy
            // remains enforced by the report owner rather than discovery.
            (progress_tracking || name != "progress")
                && (settings_enabled || name != "settings")
                && native_tool_unavailability(state, name).is_none()
        })
        .filter_map(|mut tool| {
            let name = tool["name"].as_str().unwrap_or_default().to_owned();
            if let Some(variants) = tool["inputSchema"]
                .get_mut("oneOf")
                .and_then(serde_json::Value::as_array_mut)
            {
                variants.retain(|variant| {
                    let action = variant["properties"]["action"]["const"].as_str();
                    native_action_unavailability(state, &name, action).is_none()
                });
                if variants.is_empty() {
                    return None;
                }
            }
            if let Some(actions) = tool["inputSchema"]
                .get_mut("properties")
                .and_then(|properties| properties.get_mut("action"))
                .and_then(|action| action.get_mut("enum"))
                .and_then(serde_json::Value::as_array_mut)
            {
                actions.retain(|action| {
                    native_action_unavailability(state, &name, action.as_str()).is_none()
                });
                if actions.is_empty() {
                    return None;
                }
            }
            super::mcp_input_contract::advertise_schema(&mut tool);
            Some(tool)
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
/// `mcp_capabilities`) merged with upstream aggregated tools.
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
    let generation = crate::tool_search::ToolSearchIndex::catalogue_generation(&tools);
    let mut index = state.mcp.tool_search_index.write();
    if index.generation() != generation {
        *index = crate::tool_search::ToolSearchIndex::build(&tools);
    }
}

/// Spawn the background task that subscribes to `mcp_tools_changed` and
/// rebuilds `tool_search_index` on every signal. Also does an initial build
/// so the index is populated immediately.
pub(crate) fn spawn_tool_search_index_updater(state: Arc<AppState>) {
    // Subscribe first so a change during the initial build is not lost.
    let mut rx = state.mcp.tools_changed.subscribe();
    rebuild_tool_search_index(&state);
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
#[cfg(test)]
pub(super) fn handle_search_tools(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    handle_search_tools_for_session(state, args, None)
}

pub(super) fn handle_search_tools_for_session(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    _mcp_session_id: Option<&str>,
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
    // Re-read current native policy even before the asynchronous index refresh.
    let native = filtered_native_tools(state);
    let registered = native_tool_definitions();
    let available = |name: &str| {
        !registered
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == name)
            || native.iter().any(|tool| tool["name"] == name)
    };
    let results = index.search(query, 100);

    let ranked: Vec<serde_json::Value> = results
        .iter()
        .filter(|e| available(&e.name))
        .take(limit)
        .map(|e| serde_json::json!({ "name": e.name, "summary": e.summary }))
        .collect();
    // Include one complete definition only: never truncate a schema into an
    // unusable contract. The budget measures its serialized UTF-8 bytes.
    let budget = args["schema_byte_budget"]
        .as_u64()
        .unwrap_or(16384)
        .min(65536) as usize;
    let top_schema = ranked
        .first()
        .and_then(|entry| entry["name"].as_str())
        .and_then(|name| {
            native
                .iter()
                .find(|tool| tool["name"] == name)
                .or_else(|| index.get_schema(name))
        })
        .filter(|schema| serde_json::to_vec(schema).is_ok_and(|bytes| bytes.len() <= budget));
    let mut response = serde_json::json!({
        "results": ranked, "count": ranked.len(), "catalogue_generation": index.generation()
    });
    if let Some(schema) = top_schema {
        response["top_schema"] = schema.clone();
    }
    response
}

/// Handle `get_tool_schema` meta-tool — exact-name lookup of a tool's full definition.
#[cfg(test)]
pub(super) fn handle_get_tool_schema(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    handle_get_tool_schema_for_session(state, args, None)
}

pub(super) fn handle_get_tool_schema_for_session(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    _mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let tool_name = match args["tool_name"].as_str() {
        Some(n) if !n.trim().is_empty() => n,
        _ => {
            return serde_json::json!({
                "error": "get_tool_schema requires non-empty 'tool_name' (exact tool name from search_tools)"
            });
        }
    };

    if native_tool_definitions()
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == tool_name)
    {
        return filtered_native_tools(state).into_iter().find(|tool| tool["name"] == tool_name)
            .unwrap_or_else(|| serde_json::json!({"error": format!("Tool '{tool_name}' not found. Use search_tools to discover available tools.")}));
    }
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
/// localhost-only restrictions (settings mutation, notify confirm).
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
            Err(e) => serde_json::json!({ "error": e.message, "effect": e.effect }),
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
