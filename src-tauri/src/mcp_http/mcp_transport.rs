use crate::AppState;
use axum::Json;
use axum::extract::ConnectInfo;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::Emitter;
#[cfg(all(test, unix))]
use uuid::Uuid;

#[cfg(test)]
use super::mcp_transport_ancillary::CONFIRM_TIMEOUT;
#[cfg(test)]
use super::mcp_transport_ancillary::SESSION_HTML_TAB_LIMIT;
pub(crate) use super::mcp_transport_ancillary::create_daemon_workflow_worktree;
pub(crate) use super::mcp_transport_ancillary::emit_progress_entry;
use super::mcp_transport_ancillary::handle_config;
#[cfg(test)]
use super::mcp_transport_ancillary::handle_confirm;
use super::mcp_transport_ancillary::handle_debug_unified;
#[cfg(test)]
use super::mcp_transport_ancillary::handle_github;
use super::mcp_transport_ancillary::handle_progress;
use super::mcp_transport_ancillary::handle_remote_update;
#[cfg(test)]
use super::mcp_transport_ancillary::handle_repo;
use super::mcp_transport_ancillary::handle_repo_with_caller;
#[cfg(test)]
use super::mcp_transport_ancillary::handle_screenshot;
use super::mcp_transport_ancillary::handle_story;
#[cfg(test)]
use super::mcp_transport_ancillary::handle_ui;
use super::mcp_transport_ancillary::handle_ui_unified;
use super::mcp_transport_ancillary::handle_voice;
use super::mcp_transport_ancillary::handle_workflow_launch;
use super::mcp_transport_ancillary::handle_workflow_report;
use super::mcp_transport_ancillary::handle_workflow_run;
use super::mcp_transport_ancillary::handle_workflow_story_create;
#[cfg(test)]
use super::mcp_transport_ancillary::handle_worktree;
pub(crate) use super::mcp_transport_ancillary::launch_daemon_workflow_agent;
#[cfg(test)]
use super::mcp_transport_ancillary::queue_workflow_coordinator_wake;
pub(crate) use super::mcp_transport_ancillary::report_progress;
#[cfg(test)]
use super::mcp_transport_ancillary::resolve_mcp_origin_pty;
#[cfg(all(test, unix))]
use super::mcp_transport_ancillary::resolve_mcp_origin_repo_path;
use super::mcp_transport_ancillary::resolve_mcp_origin_session;
#[cfg(test)]
use super::mcp_transport_ancillary::resolve_toast_sound;
#[cfg(test)]
use super::mcp_transport_ancillary::worktree_remove_success_response;
#[cfg(test)]
use super::mcp_transport_catalogue::AGENT_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::CONFIG_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::DEBUG_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::InstructionContext;
#[cfg(test)]
pub(crate) use super::mcp_transport_catalogue::META_TOOL_NAMES;
#[cfg(test)]
use super::mcp_transport_catalogue::NATIVE_TOOL_SUMMARIES;
#[cfg(test)]
use super::mcp_transport_catalogue::REPO_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::SESSION_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::TASK_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::UI_ACTIONS;
#[cfg(test)]
use super::mcp_transport_catalogue::VOICE_ACTIONS;
use super::mcp_transport_catalogue::build_mcp_instructions;
use super::mcp_transport_catalogue::build_mcp_instructions_for_mode;
#[cfg(test)]
use super::mcp_transport_catalogue::filtered_native_tools;
use super::mcp_transport_catalogue::handle_call_tool;
use super::mcp_transport_catalogue::handle_get_tool_schema;
use super::mcp_transport_catalogue::handle_search_tools;
use super::mcp_transport_catalogue::merged_tool_definitions;
#[cfg(test)]
use super::mcp_transport_catalogue::merged_tool_definitions_for_mode;
#[cfg(test)]
use super::mcp_transport_catalogue::meta_tool_definitions;
pub(crate) use super::mcp_transport_catalogue::native_tool_catalog;
#[cfg(test)]
use super::mcp_transport_catalogue::native_tool_definitions;
#[cfg(test)]
pub(crate) use super::mcp_transport_catalogue::rebuild_tool_search_index;
use super::mcp_transport_catalogue::removed_native_action_replacement;
#[cfg(test)]
use super::mcp_transport_catalogue::render_mcp_instructions;
use super::mcp_transport_catalogue::resolve_allowed_upstreams;
#[cfg(test)]
use super::mcp_transport_catalogue::searchable_tool_definitions;
pub(crate) use super::mcp_transport_catalogue::spawn_tool_search_index_updater;
#[cfg(test)]
use super::mcp_transport_catalogue::validate_mcp_repo_path;
#[cfg(test)]
pub(super) use super::mcp_transport_peer::CLIENT_PID_HEADER;
use super::mcp_transport_peer::InitializeKind;
#[cfg(test)]
use super::mcp_transport_peer::MCP_OWNER_ACTIVITY_GRACE;
pub(super) use super::mcp_transport_peer::PEER_IDENTITY_BIND_LOCK;
#[cfg(test)]
use super::mcp_transport_peer::TAKEOVER_REJECT_ENTRY_TTL;
#[cfg(test)]
use super::mcp_transport_peer::TAKEOVER_REJECT_LOG;
#[cfg(test)]
use super::mcp_transport_peer::TAKEOVER_REJECT_SUMMARY_INTERVAL;
#[cfg(test)]
use super::mcp_transport_peer::TASK_POLL_INTERVAL_MS;
pub(super) use super::mcp_transport_peer::TUIC_SESSION_HEADER;
use super::mcp_transport_peer::apply_initialize_identity;
pub(super) use super::mcp_transport_peer::client_pid_header;
use super::mcp_transport_peer::drop_identity_buffers;
use super::mcp_transport_peer::handle_agent_wait;
#[cfg(test)]
use super::mcp_transport_peer::handle_messaging;
use super::mcp_transport_peer::initialize_session_id;
pub(crate) use super::mcp_transport_peer::is_pending_parent;
#[cfg(test)]
use super::mcp_transport_peer::is_valid_uuid;
pub(super) use super::mcp_transport_peer::local_peer_call;
pub(crate) use super::mcp_transport_peer::local_peer_call_with_message_id;
use super::mcp_transport_peer::managed_parent_cwd_from_header;
#[cfg(test)]
use super::mcp_transport_peer::pending_parent_id;
pub(super) use super::mcp_transport_peer::refresh_mcp_session;
#[cfg(test)]
use super::mcp_transport_peer::retire_repaired_phantom_identity;
#[cfg(test)]
use super::mcp_transport_peer::takeover_rejection_report;
#[cfg(test)]
use super::mcp_transport_peer::task_owner_identity;
#[cfg(test)]
use super::mcp_transport_session_agent::INFERRED_PTY_DESCRIPTION_MAX_CHARS;
#[cfg(test)]
use super::mcp_transport_session_agent::McpSpawnArgs;
#[cfg(test)]
use super::mcp_transport_session_agent::PtyDescriptionUpdate;
#[cfg(all(test, unix))]
use super::mcp_transport_session_agent::SUBMIT_ACK_MIN_MS;
#[cfg(test)]
use super::mcp_transport_session_agent::WAIT_DEFAULT_MS;
#[cfg(test)]
use super::mcp_transport_session_agent::WAIT_EFFECTIVE_MAX_MS;
#[cfg(test)]
use super::mcp_transport_session_agent::WAIT_MAX_MS;
#[cfg(test)]
use super::mcp_transport_session_agent::apply_managed_claude_tmpdir;
#[cfg(test)]
use super::mcp_transport_session_agent::build_spawn_prompt;
#[cfg(test)]
use super::mcp_transport_session_agent::clamp_wait_timeout;
pub(crate) use super::mcp_transport_session_agent::close_idle_managed_session;
#[cfg(test)]
use super::mcp_transport_session_agent::compose_mcp_spawn_args;
#[cfg(test)]
use super::mcp_transport_session_agent::finalize_explicit_spawn_args;
#[cfg(test)]
use super::mcp_transport_session_agent::finalize_spawn_args;
#[cfg(test)]
use super::mcp_transport_session_agent::handle_agent;
#[cfg(test)]
use super::mcp_transport_session_agent::handle_agent_unified;
use super::mcp_transport_session_agent::handle_agent_unified_with_parent_cwd;
use super::mcp_transport_session_agent::handle_session;
pub(super) use super::mcp_transport_session_agent::handle_session_submit;
use super::mcp_transport_session_agent::handle_session_suspend;
use super::mcp_transport_session_agent::handle_session_wait;
use super::mcp_transport_session_agent::handle_task;
#[cfg(test)]
use super::mcp_transport_session_agent::is_direct_codex_executable;
#[cfg(test)]
use super::mcp_transport_session_agent::merge_mcp_params_into_args;
#[cfg(test)]
use super::mcp_transport_session_agent::parse_pty_description;
#[cfg(all(test, unix))]
use super::mcp_transport_session_agent::resolve_effective_spawn_cwd;
#[cfg(test)]
use super::mcp_transport_session_agent::resolve_run_config;
pub(crate) use super::mcp_transport_session_agent::resolve_session_suspend;
#[cfg(test)]
use super::mcp_transport_session_agent::resolve_spawn_agent_type;
#[cfg(test)]
use super::mcp_transport_session_agent::resolve_spawn_pty_description;
pub(super) use super::mcp_transport_session_agent::session_output;
#[cfg(test)]
use super::mcp_transport_session_agent::session_wait_met;
#[cfg(test)]
use super::mcp_transport_session_agent::spawn_response;
#[cfg(test)]
use super::mcp_transport_session_agent::substitute_prompt_in_args;
#[cfg(test)]
use super::mcp_transport_session_agent::suspend_refusal;
#[cfg(test)]
use super::mcp_transport_session_agent::translate_special_key;
#[cfg(test)]
use super::mcp_transport_session_agent::uses_agent_command_injection;

/// Serialize a value to JSON, returning a structured error on failure instead of silent null.
pub(super) fn to_json_or_error<T: serde::Serialize>(value: T) -> serde_json::Value {
    match serde_json::to_value(value) {
        Ok(v) => v,
        Err(e) => serde_json::json!({"error": format!("Serialization failed: {e}")}),
    }
}

fn serialize_tool_result(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// Private transport marker used to carry a proxied `tools/call` result through
/// the collapsed `call_tool` meta-path without confusing it with a native
/// handler value. The marker is removed before any public response is emitted.
const UPSTREAM_TOOL_RESULT_MARKER: &str = "__tuic_upstream_tool_result";

pub(super) fn mark_upstream_tool_result(value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ UPSTREAM_TOOL_RESULT_MARKER: value })
}

fn unmark_upstream_tool_result(value: serde_json::Value) -> serde_json::Value {
    value
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.get(UPSTREAM_TOOL_RESULT_MARKER))
        .cloned()
        .unwrap_or(value)
}

fn upstream_tool_result(value: &serde_json::Value) -> Option<&serde_json::Value> {
    value
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.get(UPSTREAM_TOOL_RESULT_MARKER))
}

/// Produce the JSON-RPC `result` for a `tools/call` response.
///
/// A successful upstream call already speaks the MCP CallToolResult contract,
/// so preserve a valid object field-for-field at the JSON value level. Malformed
/// upstream values and every native TUIC value retain the compact text fallback.
fn format_tool_call_result(result: &serde_json::Value, is_error: bool) -> serde_json::Value {
    if let Some(upstream) = upstream_tool_result(result)
        && upstream
            .as_object()
            .and_then(|object| object.get("content"))
            .is_some_and(serde_json::Value::is_array)
    {
        return upstream.clone();
    }

    let fallback = upstream_tool_result(result).unwrap_or(result);
    serde_json::json!({
        "content": [{ "type": "text", "text": serialize_tool_result(fallback) }],
        "isError": is_error
    })
}

pub(super) fn insert_optional_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<serde_json::Value>,
) {
    if let Some(value) = value {
        object.insert(key.to_string(), value);
    }
}

/// Single source of truth for detecting Claude Code (or tuic-bridge) clients.
pub(super) fn detect_claude_code_client(client_name: Option<&str>) -> bool {
    client_name.is_some_and(|n| n.contains("claude") || n.contains("tuic-bridge"))
}

/// Two clients take the collapsed surface whatever `collapse_tools` says, for
/// two unrelated reasons.
///
/// Grok accepts exactly one `__` delimiter in a qualified MCP tool id. TUIC's
/// upstream names would become `tuicommander__upstream__tool` after Grok adds
/// the server namespace, so expose the existing meta-tool surface for that
/// client instead of letting it silently discard every proxied tool.
///
/// ego pays for the catalogue on every model call: it captures one immutable
/// tool list per generation and sends every definition with every request,
/// while `/mcp` proxies 200+ upstream tools on a normal day. Measured
/// 2026-09-13 at 190 tools: 35.104 tokens per turn against 615 collapsed, and
/// flat as upstreams grow. Everything stays reachable through `call_tool`; the
/// cost is one extra round trip before the first use of an unfamiliar tool.
pub(super) fn client_requires_meta_tools(client_name: Option<&str>) -> bool {
    client_name
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| name.starts_with("grok-shell-") || name == "ego")
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

/// External Claude clients can consume channel mail while subscribed to SSE.
/// Managed peers use their PTY wake so unread mail starts a later turn.
pub(super) fn external_recipient_supports_claude_channel(
    state: &AppState,
    mcp_session_id: &str,
) -> bool {
    state
        .mcp
        .sessions
        .get(mcp_session_id)
        .is_some_and(|session| session.is_claude_code && session.has_sse_stream)
}

pub(super) const SSE_INLINE_MESSAGE_MAX_BYTES: usize = 200;
pub(super) const SSE_POINTER_SUBJECT_MAX_BYTES: usize = 80;

/// Map MCP client name to TUICommander agent type key.
/// Returns None when the client cannot be identified.
pub(super) fn resolve_agent_type(client_name: Option<&str>) -> Option<&'static str> {
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
    } else if name.contains("goose") {
        Some("goose")
    } else {
        None
    }
}

/// Resolve effective intent_tab_title / suggest_followups for a connecting agent.
/// Semantics: `global AND (per_agent ?? true)`. Global acts as a kill-switch for
/// the whole feature; per-agent is an escape hatch (default ON) to disable the
/// marker on a specific agent where rendering or parsing misbehaves.
pub(super) fn resolve_marker_flags(
    state: &Arc<AppState>,
    client_name: Option<&str>,
) -> (bool, bool) {
    marker_flags_for_agent(state, resolve_agent_type(client_name))
}

/// Same rule, keyed by agent type instead of by client name, so a caller that
/// already knows the agent (a live session, the marker diagnostics) does not
/// have to reverse-engineer a client name to ask the question (#4421).
pub(crate) fn marker_flags_for_agent(state: &AppState, agent_type: Option<&str>) -> (bool, bool) {
    let global_intent = state.config.read().intent_tab_title;
    let global_suggest = state.config.read().suggest_followups;

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
    let Some((_, tab_ids)) = state.session_maps.session_html_tabs.remove(session_id) else {
        return;
    };
    let _ = state.event_bus.send(crate::state::AppEvent::CloseHtmlTabs {
        tab_ids: tab_ids.clone(),
    });
    #[cfg(feature = "desktop")]
    #[allow(clippy::collapsible_if)]
    if let Some(app) = state.app_handle.read().as_ref() {
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
}

/// Handle an MCP tools/call request, executing against the app state directly (no HTTP round-trip).
/// Also used by the `deep_link_mcp_call` Tauri command for the `tuic://cmd/` gateway.
pub(crate) async fn handle_mcp_tool_call(
    state: &Arc<AppState>,
    addr: SocketAddr,
    name: &str,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    unmark_upstream_tool_result(
        Box::pin(handle_mcp_tool_call_with_context(
            state,
            addr,
            name,
            args,
            mcp_session_id,
            None,
        ))
        .await,
    )
}

/// Run a synchronous tool handler on the blocking pool.
///
/// The sync handlers reach genuinely blocking work: `session close`/`kill` wait on the
/// child with `std::thread::sleep` (up to 200ms in `close_pty_core`/`kill_pty_core`),
/// agent injection sleeps `INJECT_ENTER_GAP` between the payload and the Enter, and
/// spawn/config paths issue blocking syscalls and disk I/O. Called inline from an async
/// handler these park a tokio worker for the whole duration.
pub(super) async fn run_blocking_handler<F>(f: F) -> serde_json::Value
where
    F: FnOnce() -> serde_json::Value + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(value) => value,
        Err(e) => serde_json::json!({
            "error": format!("tool handler failed to complete: {e}")
        }),
    }
}

fn session_action_requires_blocking_pool(action: &str) -> bool {
    matches!(
        action,
        "create" | "input" | "kill" | "close" | "resize" | "declare_worktree"
    )
}

fn agent_action_requires_blocking_pool(action: &str) -> bool {
    matches!(action, "spawn" | "send")
}

fn secret_inspection_tool(name: &str, args: &serde_json::Value) -> bool {
    matches!(name, "ui" | "debug")
        || name.contains("__")
        || (name == "call_tool"
            && secret_inspection_tool(args["tool_name"].as_str().unwrap_or(""), &args["arguments"]))
}

pub(super) async fn handle_mcp_tool_call_with_context(
    state: &Arc<AppState>,
    addr: SocketAddr,
    name: &str,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> serde_json::Value {
    Box::pin(guard_secret_inspection(
        state,
        name,
        args,
        dispatch_mcp_tool_call_with_context(
            state,
            addr,
            name,
            args,
            mcp_session_id,
            managed_parent_cwd,
        ),
    ))
    .await
}

/// Gate native and direct upstream inspection while a private form is open.
async fn guard_secret_inspection(
    state: &Arc<AppState>,
    name: &str,
    args: &serde_json::Value,
    call: impl std::future::Future<Output = serde_json::Value>,
) -> serde_json::Value {
    let inspection = secret_inspection_tool(name, args);
    if inspection && state.secrets.tools_blocked() {
        return serde_json::json!({"error": "Agent inspection is disabled while a private secret form is open"});
    }
    call.await
}

async fn dispatch_mcp_tool_call_with_context(
    state: &Arc<AppState>,
    addr: SocketAddr,
    name: &str,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> serde_json::Value {
    // Enforce disabled_native_tools on every call path (not just the call_tool meta-tool).
    // Read-guard does not span an await and is released at the end of the `if` expression.
    if state
        .config
        .read()
        .disabled_native_tools
        .iter()
        .any(|d| d == name)
    {
        return serde_json::json!({"error": format!("Tool '{}' is disabled by configuration", name)});
    }
    if let Some(action) = args["action"].as_str()
        && let Some(replacement) = removed_native_action_replacement(name, action)
    {
        return serde_json::json!({
            "error": format!("{name} action={action} was removed; use {replacement}")
        });
    }
    if name == "repo"
        && matches!(
            args["action"].as_str(),
            Some("worktree_lifecycle" | "worktree_remove")
        )
        && args.get("workspace_id").is_some()
    {
        return serde_json::json!({"error": "repo parameter workspace_id was renamed to branch"});
    }
    if matches!(name, "session" | "agent")
        && let Some(connection) = args.get("connection_id")
        && !connection
            .as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 128 && !id.contains('/'))
    {
        return serde_json::json!({"error":"connection_id must be a nonempty connection qualifier"});
    }
    // Resolve client identity at dispatch level — tool handlers get a plain bool
    let is_claude_code = mcp_session_id
        .and_then(|sid| state.mcp.sessions.get(sid))
        .map(|meta| meta.is_claude_code)
        .unwrap_or(false);
    match name {
        "automations" => {
            crate::automations::mcp::handle(args, resolve_mcp_origin_session(state, mcp_session_id))
                .await
        }
        "secret" => crate::secrets::handle_secret(state, args).await,
        "telegram" => crate::telegram::handle_tool(state, args, mcp_session_id).await,
        "session" => {
            let action = args["action"].as_str().unwrap_or("");
            if let Some(remote) = super::remote_mcp_sessions::resolve(state, args) {
                if action == "submit" && !addr.ip().is_loopback() {
                    return serde_json::json!({"error":"This session action is restricted to localhost connections"});
                }
                return match remote {
                    Ok((host, id)) => {
                        super::remote_mcp_sessions::call(state, &host, &id, args).await
                    }
                    Err(error) => error,
                };
            }
            let mut local_args = args.clone();
            if let Some(reference) = args["session_id"]
                .as_str()
                .and_then(|id| id.strip_prefix("local/"))
            {
                local_args["session_id"] = serde_json::json!(reference);
            }
            let args = &local_args;
            // Executing / destructive session actions carry the same loopback
            // restriction as `agent spawn`: `submit` executes a managed-agent
            // composer command, while `input` writes raw bytes to a PTY's stdin
            // (arbitrary command execution on a shell session, unfiltered context
            // injection on an agent session). `create`/`kill`/`close` spawn or
            // destroy sessions, `resize`/`rename` change their presentation, and
            // `pause`/`resume` halt/resume output buffering
            // (a remote `pause` on any session is a DoS). A non-loopback MCP client
            // (authenticated remote, or admitted via lan_auth_bypass) must not reach
            // them — remote terminal control is served separately by the auth-gated
            // POST /sessions/{id}/write route. Read-only actions (list/output/status/…)
            // stay open for monitoring.
            let action = args["action"].as_str().unwrap_or("");
            if action == "wait" {
                // Read-only blocking wait — needs the async runtime for its poll
                // loop, so it can't live in the sync handle_session.
                handle_session_wait(state, args).await
            } else if action == "submit" && !addr.ip().is_loopback() {
                serde_json::json!({
                    "error": "This session action is restricted to localhost connections"
                })
            } else if action == "submit" {
                handle_session_submit(state, args, false).await
            } else if action == "suspend" && addr.ip().is_loopback() {
                handle_session_suspend(state, args, mcp_session_id).await
            } else if matches!(
                action,
                "create"
                    | "input"
                    | "kill"
                    | "close"
                    | "pause"
                    | "resume"
                    | "resize"
                    | "rename"
                    | "suspend"
            ) && !addr.ip().is_loopback()
            {
                serde_json::json!({
                    "error": "This session action is restricted to localhost connections"
                })
            } else if session_action_requires_blocking_pool(action) {
                let state = state.clone();
                let args = args.clone();
                let sid = mcp_session_id.map(str::to_owned);
                run_blocking_handler(move || handle_session(&state, &args, sid.as_deref())).await
            } else {
                handle_session(state, args, mcp_session_id)
            }
        }
        "agent" => {
            let action = args["action"].as_str().unwrap_or("");
            if matches!(action, "list_peers" | "send")
                && addr.ip().is_loopback()
                && let Some(result) =
                    super::remote_peer::dispatch(state, args, mcp_session_id).await
            {
                return result;
            }
            if action == "wait" && !addr.ip().is_loopback() {
                serde_json::json!({
                    "error": "Inter-agent messaging is restricted to localhost connections"
                })
            } else if action == "wait" {
                handle_agent_wait(state, args, mcp_session_id).await
            } else if agent_action_requires_blocking_pool(action) {
                let state = state.clone();
                let args = args.clone();
                let sid = mcp_session_id.map(str::to_owned);
                let parent_cwd = managed_parent_cwd.map(str::to_owned);
                run_blocking_handler(move || {
                    handle_agent_unified_with_parent_cwd(
                        &state,
                        addr,
                        &args,
                        sid.as_deref(),
                        parent_cwd.as_deref(),
                    )
                })
                .await
            } else {
                handle_agent_unified_with_parent_cwd(
                    state,
                    addr,
                    args,
                    mcp_session_id,
                    managed_parent_cwd,
                )
            }
        }
        "task" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || handle_task(&state, addr, &args, sid.as_deref())).await
        }
        "repo" => handle_repo_with_caller(state, args, is_claude_code, mcp_session_id).await,
        "remote" => handle_remote_update(state, args).await,
        "story" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || handle_story(&state, &args, sid.as_deref())).await
        }
        "workflow_run" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || handle_workflow_run(&state, &args, sid.as_deref())).await
        }
        "workflow_story_create" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || {
                handle_workflow_story_create(&state, &args, sid.as_deref())
            })
            .await
        }
        "workflow_report" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || handle_workflow_report(&state, &args, sid.as_deref()))
                .await
        }
        "workflow_launch" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || {
                handle_workflow_launch(&state, addr, &args, sid.as_deref())
            })
            .await
        }
        "progress" => handle_progress(state, args, mcp_session_id).await,
        "ui" => handle_ui_unified(state, addr, args, mcp_session_id).await,
        "plugin_dev_guide" => {
            serde_json::json!({"content": super::plugin_docs::PLUGIN_DOCS})
        }
        "config" => {
            let state = state.clone();
            let args = args.clone();
            run_blocking_handler(move || handle_config(&state, addr, &args)).await
        }
        "debug" => handle_debug_unified(state, addr, args),
        // Blocking pool: `speak` builds nothing, but `status` reaps a finished
        // hands-free runtime, which joins a thread.
        "voice" => {
            let state = state.clone();
            let args = args.clone();
            let sid = mcp_session_id.map(str::to_owned);
            run_blocking_handler(move || handle_voice(&state, &args, sid.as_deref())).await
        }
        "search_tools" => handle_search_tools(state, args),
        "get_tool_schema" => handle_get_tool_schema(state, args),
        "call_tool" => {
            handle_call_tool(state, addr, args, mcp_session_id, managed_parent_cwd).await
        }
        _ => serde_json::json!({"error": format!(
            "Unknown tool '{}'. Available: session, agent, task, remote, repo, story, progress, ui, plugin_dev_guide, config, debug, voice, search_tools, get_tool_schema, call_tool", name
        )}),
    }
}

// ---------------------------------------------------------------------------
// Knowledge (cross-repo mdkb fan-out)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Streamable HTTP transport (legacy MCP spec 2025-11-25)
// Single /mcp endpoint — POST for JSON-RPC, GET for SSE notifications, DELETE ends session
// ---------------------------------------------------------------------------

pub(super) const MCP_SESSION_HEADER: &str = "mcp-session-id";
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] = ["2026-07-28", "2025-11-25", "2025-03-26"];

/// Answered when the client asks for a revision we do not implement, or sends
/// none at all. `initialize` is the legacy 2025-11-25 transport, so that is the
/// revision it can promise — not the newest entry in the supported list.
const DEFAULT_PROTOCOL_VERSION: &str = SUPPORTED_PROTOCOL_VERSIONS[1];

/// The stateless revision: no `initialize`, no session id, and every list
/// result carries its own cache envelope. ego pins it as a const and cannot
/// fall back, so it is the revision that decides whether ego works at all.
const MODERN_PROTOCOL_VERSION: &str = SUPPORTED_PROTOCOL_VERSIONS[0];

/// Where a stateless client puts its identity, once per request.
const CLIENT_INFO_META_KEY: &str = "io.modelcontextprotocol/clientInfo";

/// Where a stateless client names the revision it is speaking, once per
/// request. The `MCP-Protocol-Version` header carries the same value; a client
/// may send either, so both are read.
const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";

/// The header a client puts its revision in on every request after the
/// handshake.
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Where the server puts its own, in a `DiscoverResult`. The modern result has
/// no top-level `serverInfo` field — it travels in `_meta` (rmcp 3.1.4
/// `DiscoverResult::set_server_info`).
const SERVER_INFO_META_KEY: &str = "io.modelcontextprotocol/serverInfo";

/// The client name carried by a single request's `_meta`.
///
/// The legacy lifecycle records the name once at `initialize` and looks it up
/// by session id afterwards. A stateless caller has no session to look up, so
/// every request that wants to know who is asking reads it from here.
fn request_meta_client_name(body: &serde_json::Value) -> Option<&str> {
    body["params"]["_meta"][CLIENT_INFO_META_KEY]["name"].as_str()
}

/// Whether this one request is on the stateless 2026-07-28 lifecycle.
///
/// There is no session to remember the answer in — that is the whole point of
/// the revision — so it is re-read per request from the header, falling back to
/// the `_meta` key. A caller that names neither is legacy: the revision governs
/// the *shape* of what we answer, so guessing "modern" would send cache fields
/// to a client whose deserializer never asked for them.
fn request_is_modern_lifecycle(headers: &HeaderMap, body: &serde_json::Value) -> bool {
    headers
        .get(MCP_PROTOCOL_VERSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .or_else(|| body["params"]["_meta"][PROTOCOL_VERSION_META_KEY].as_str())
        .is_some_and(|version| version == MODERN_PROTOCOL_VERSION)
}

/// 2026-07-28 makes a list result a *cache* entry: `resultType` says the page
/// is the whole list, and `ttlMs`/`cacheScope` say how long it may be held. ego
/// refuses to admit a server whose `tools/list` omits any of the three —
/// measured 2026-09-19, the handshake and the list both succeeded and
/// `session/new` still answered "the supplied MCP servers could not be
/// admitted" (#783-3c1b); `resources/list` did the same (#1318-abd7). Same
/// values as `server/discover`: the tool surface moves with upstream connects
/// and config toggles, so nothing here is cacheable.
pub(crate) fn add_result_envelope(result: &mut serde_json::Value) {
    result["resultType"] = serde_json::json!("complete");
    result["ttlMs"] = serde_json::json!(0);
    result["cacheScope"] = serde_json::json!("private");
}

/// Agree on a protocol revision: the client's own when we support it, otherwise
/// [`DEFAULT_PROTOCOL_VERSION`]. Echoing an unsupported version back would be a
/// promise we cannot keep; answering our own version to a client that named a
/// supported one tells it to switch dialects for no reason.
fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|want| {
            SUPPORTED_PROTOCOL_VERSIONS
                .iter()
                .find(|supported| **supported == want)
                .copied()
        })
        .unwrap_or(DEFAULT_PROTOCOL_VERSION)
}

/// Resolve a filesystem path to one of the known repo roots, picking the longest
/// matching prefix that respects path-component boundaries (so `/foo/bar` does
/// not match `/foo/bar-other`). Falls back to the original path when no repo
/// matches. (#1373-6e2f)
fn resolve_repo_for_path(path: &str, known: &[String]) -> String {
    registered_repo_for_path(path, known).unwrap_or_else(|| path.to_string())
}

/// The registered repository a path belongs to, or `None` when no registered
/// repository contains it.
///
/// [`resolve_repo_for_path`] falls back to the path itself, which is right for
/// scoping tool access and wrong for ownership: a journal entry filed under an
/// unregistered directory belongs to no project anybody can open.
pub(crate) fn registered_repo_for_path(path: &str, known: &[String]) -> Option<String> {
    known
        .iter()
        .filter(|repo| path == repo.as_str() || path.starts_with(&format!("{repo}/")))
        .max_by_key(|repo| repo.len())
        .cloned()
}

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
            let (session_id, init_kind) = initialize_session_id(&state, &headers);
            let client_name = body["params"]["clientInfo"]["name"].as_str();
            let is_claude_code = detect_claude_code_client(client_name);
            let requires_meta_tools = client_requires_meta_tools(client_name);

            // Extract repo_path from MCP initialize roots[0].uri (file:// URI)
            let repo_path = body["params"]["roots"]
                .as_array()
                .and_then(|roots| roots.first())
                .and_then(|root| root["uri"].as_str())
                .and_then(|uri| uri.strip_prefix("file://"))
                .map(|path| {
                    let known: Vec<String> = state
                        .repo_watchers
                        .iter()
                        .map(|entry| entry.key().clone())
                        .collect();
                    resolve_repo_for_path(path, &known)
                });

            let now = std::time::Instant::now();
            if let Some(mut meta) = state.mcp.sessions.get_mut(&session_id) {
                meta.last_activity = now;
                meta.is_claude_code = is_claude_code;
                meta.requires_meta_tools = requires_meta_tools;
                if repo_path.is_some() {
                    meta.repo_path = repo_path;
                }
            } else {
                state.mcp.sessions.insert(
                    session_id.clone(),
                    crate::state::McpSessionMeta {
                        prompt_instructions: None,
                        last_activity: now,
                        is_claude_code,
                        requires_meta_tools,
                        has_sse_stream: false,
                        sse_generation: 0,
                        repo_path,
                    },
                );
            }

            // Auto-bind managed-peer identity from the `x-tuic-session` header the bridge
            // asserts (it inherits `TUIC_SESSION` from the agent PTY). This makes
            // `agent register` optional — the caller already has a working peer
            // identity for spawn/send, which is what clients that ignore initialize
            // `instructions` (e.g. Codex) otherwise never obtain.
            let tuic_session_header = headers
                .get(TUIC_SESSION_HEADER)
                .and_then(|v| v.to_str().ok());
            apply_initialize_identity(&state, &session_id, tuic_session_header);

            // One record per handshake. `initialize` happens once per client
            // connection, so this is not a hot path, and it is the only evidence
            // that an agent's MCP connection actually dropped and came back.
            tracing::info!(
                source = "mcp_initialize",
                event = init_kind.as_str(),
                client = client_name.unwrap_or("unknown"),
                mcp_session = %session_id,
                tuic_session = tuic_session_header.unwrap_or(""),
                client_pid = client_pid_header(&headers),
                presented_session = match &init_kind {
                    InitializeKind::Reconnected { presented } => presented.as_str(),
                    _ => "",
                },
                "MCP initialize"
            );

            let protocol_version =
                negotiate_protocol_version(body["params"]["protocolVersion"].as_str());
            let effective_collapse = state.config.read().collapse_tools || requires_meta_tools;
            let instructions =
                build_mcp_instructions_for_mode(&state, client_name, effective_collapse);

            crate::prompt_receipt::record_mcp_instructions(&state, &session_id, &instructions);
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": protocol_version,
                    "capabilities": {
                        // The GET /mcp SSE stream emits notifications/tools/list_changed
                        // (upstream connect/disconnect, config toggles). A client is
                        // entitled to drop a notification for a capability the server
                        // never declared, so this declaration is what makes the stream
                        // mean anything.
                        "tools": { "listChanged": true },
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
            )
                .into_response()
        }

        // The modern (2026-07-28) lifecycle's whole handshake. One request, one
        // response, no session: rmcp's `Discover` mode treats a JSON-RPC error
        // here as a hard failure rather than a cue to try `initialize`, so this
        // arm answering is the difference between every tool reaching the
        // client and none of them.
        // The modern lifecycle: no `initialize`, no `notifications/initialized`,
        // no session id, and a `_meta` block carrying the client's identity on
        // every request. Deliberately a *second* entry point rather than a
        // replacement — Claude Code and every existing client speak the legacy
        // lifecycle and must keep working, while ego pins the modern revision as
        // a const and has no fallback, so one endpoint has to answer both.
        "server/discover" => {
            let client_name = request_meta_client_name(&body);
            let effective_collapse =
                state.config.read().collapse_tools || client_requires_meta_tools(client_name);
            let instructions =
                build_mcp_instructions_for_mode(&state, client_name, effective_collapse);

            // Shape and casing are rmcp 3.1.4's `DiscoverResult`, which is what
            // the client deserializes into. `ttlMs: 0` with `cacheScope:
            // private` says "do not cache this": the tool surface moves with
            // upstream connects and config toggles, so a cached discovery would
            // go stale within one session.
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "resultType": "complete",
                    "supportedVersions": SUPPORTED_PROTOCOL_VERSIONS,
                    "capabilities": {
                        "tools": { "listChanged": true },
                        "experimental": { "claude/channel": {} }
                    },
                    "instructions": instructions,
                    "ttlMs": 0,
                    "cacheScope": "private",
                    "_meta": {
                        SERVER_INFO_META_KEY: {
                            "name": "tuicommander",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                }
            }))
            .into_response()
        }

        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),

        // Standard MCP liveness request. The bridge uses this instead of
        // rebuilding and serializing the complete tools/list payload every
        // three seconds per terminal.
        "ping" => {
            if let Some(sid) = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
            {
                refresh_mcp_session(
                    &state,
                    sid,
                    detect_claude_code_from_headers(&headers),
                    headers
                        .get(TUIC_SESSION_HEADER)
                        .and_then(|v| v.to_str().ok()),
                );
            }
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }))
            .into_response()
        }

        "tools/list" => {
            let list_session_id = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok());
            if let Some(sid) = list_session_id {
                refresh_mcp_session(
                    &state,
                    sid,
                    detect_claude_code_from_headers(&headers),
                    headers
                        .get(TUIC_SESSION_HEADER)
                        .and_then(|v| v.to_str().ok()),
                );
            }
            // On the first list after boot, wait (bounded) for upstream MCP
            // servers to finish connecting so their proxied tools are included.
            // CC fetches tools/list during the handshake — before async upstream
            // init completes — and never refetches on tools/list_changed
            // (anthropics/claude-code#4118), so a stale list would otherwise stick.
            state
                .mcp
                .upstream_registry
                .await_initial_settle(std::time::Duration::from_secs(3))
                .await;
            let tools =
                merged_tool_definitions(&state, list_session_id, request_meta_client_name(&body));
            let mut result = serde_json::json!({ "tools": tools });
            // Withheld from the legacy revision, which has no such fields and
            // no reader for them.
            if request_is_modern_lifecycle(&headers, &body) {
                add_result_envelope(&mut result);
            }
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            });
            let mut resp = Json(response).into_response();
            if let Some(sid) = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
                && let Ok(val) = sid.parse()
            {
                resp.headers_mut().insert(MCP_SESSION_HEADER, val);
            }
            resp
        }

        "tools/call" => {
            // Identity is resolved per call, not demanded up front. A caller that
            // sends the header gets its session refreshed — stale ids (app restart,
            // or a long-lived client like Claude Code that lost its session)
            // auto-recover rather than erroring. A caller that sends none is served
            // anyway: most tools need no identity, and the ones that do already
            // refuse with guidance the caller can act on, which a blanket -32600
            // never gave them.
            let is_cc_ua = detect_claude_code_from_headers(&headers);
            if let Some(sid) = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
            {
                refresh_mcp_session(
                    &state,
                    sid,
                    is_cc_ua,
                    headers
                        .get(TUIC_SESSION_HEADER)
                        .and_then(|v| v.to_str().ok()),
                );
            }

            let params = body
                .get("params")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let tool_name = params["name"].as_str().unwrap_or("").to_string();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let session_id_str = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let managed_parent_cwd = managed_parent_cwd_from_header(
                &state,
                session_id_str.as_deref(),
                headers
                    .get(TUIC_SESSION_HEADER)
                    .and_then(|value| value.to_str().ok()),
            );

            // Route upstream-prefixed tools ({upstream}__{tool}) via the proxy registry.
            // Native tools (no "__") dispatch through handle_mcp_tool_call_with_context,
            // which puts each blocking sync handler on the blocking pool itself
            // (see run_blocking_handler) — this call site does not wrap anything.
            let (result, is_error) = if tool_name.contains("__") {
                // Resolving the allowlist reads and parses repo-settings.json from
                // disk. Only a proxied call consults it, so a native call must not
                // pay for it.
                let result = guard_secret_inspection(&state, &tool_name, &args, async {
                    let allowed = resolve_allowed_upstreams(&state, session_id_str.as_deref());
                    match state
                        .mcp
                        .upstream_registry
                        .proxy_tool_call_for_repo(&tool_name, args.clone(), allowed.as_deref())
                        .await
                    {
                        Ok(v) => mark_upstream_tool_result(v),
                        Err(e) => serde_json::json!({"error": e}),
                    }
                })
                .await;
                let is_error = result.get("error").is_some();
                (result, is_error)
            } else {
                let result = Box::pin(handle_mcp_tool_call_with_context(
                    &state,
                    addr,
                    &tool_name,
                    &args,
                    session_id_str.as_deref(),
                    managed_parent_cwd.as_deref(),
                ))
                .await;
                let is_error = result.get("error").is_some();
                (result, is_error)
            };
            let mut tool_result = format_tool_call_result(&result, is_error);
            if request_is_modern_lifecycle(&headers, &body) {
                // A completed 2026-07-28 tool call has this discriminator,
                // including tool-level errors. Preserve any discriminator an
                // upstream already supplied for a non-complete result.
                tool_result
                    .as_object_mut()
                    .expect("tool-call results are objects")
                    .entry("resultType")
                    .or_insert_with(|| serde_json::json!("complete"));
            }
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": tool_result
            });
            let mut resp = Json(response).into_response();
            if let Some(sid) = headers
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
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

/// Streams are numbered from one counter for the whole process, never from the
/// session. A session id can be removed and auto-recovered while an older stream
/// for the previous incarnation is still draining; a per-session counter would
/// restart at zero and hand that stale stream a number the new one also holds,
/// and its teardown would then close the replacement.
fn next_sse_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Releases the per-session SSE resources whenever the stream goes away —
/// normal end, client disconnect, or server shutdown alike. Owned by the stream
/// generator so it runs on drop; the only path a disconnect actually takes.
///
/// A half-open stream can still be alive when its own reconnect is accepted, so
/// the release is conditional: the guard only clears the session it still owns.
/// Unconditional cleanup let the older stream's drop remove the sender the new
/// stream had just subscribed to, which closed the replacement immediately.
///
/// Lock order is `mcp_sessions` → `messaging_channels`, here and in `mcp_get`.
/// Nothing may hold a `messaging_channels` reference while taking `mcp_sessions`.
struct SseStreamTeardown {
    pub(super) state: Arc<AppState>,
    pub(super) mcp_sid: String,
    pub(super) generation: u64,
}

impl Drop for SseStreamTeardown {
    fn drop(&mut self) {
        use dashmap::mapref::entry::Entry;

        // The session entry is held across the channel removal: releasing it
        // first would let a reconnect take ownership and subscribe in between,
        // and this drop would then remove the sender out from under it.
        //
        // `entry` rather than `get_mut` because the absent case needs the hold
        // just as much: DELETE or the reaper can remove the session while this
        // stream is still draining, and `get_mut` returning `None` releases the
        // shard before the removal below. A reconnect landing in that window
        // recreates the session, subscribes to a fresh sender, and then loses it
        // to this drop. Only `entry` keeps the shard locked over an absent key.
        match self.state.mcp.sessions.entry(self.mcp_sid.clone()) {
            // Superseded: a newer stream owns the session and its channel.
            Entry::Occupied(meta) if meta.get().sse_generation != self.generation => {}
            Entry::Occupied(mut meta) => {
                meta.get_mut().has_sse_stream = false;
                self.state
                    .session_maps
                    .messaging_channels
                    .remove(&self.mcp_sid);
            }
            // The session itself is gone; nobody is left to own the channel.
            Entry::Vacant(_) => {
                self.state
                    .session_maps
                    .messaging_channels
                    .remove(&self.mcp_sid);
            }
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
    let Some(sid) = session_id else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if !state.mcp.sessions.contains_key(&sid) {
        tracing::warn!(
            "MCP SSE session auto-recovered (stale session_id: {sid}); \
             is_claude_code={is_cc_ua} (from User-Agent)"
        );
        state.mcp.sessions.insert(
            sid.clone(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: std::time::Instant::now(),
                is_claude_code: is_cc_ua,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );
    }

    // Take ownership of the session and subscribe to its channel under one hold
    // of the session entry — the same one the teardown takes. Split apart, a
    // superseded stream's teardown could pass its generation check, then remove
    // the sender *after* this stream had already subscribed to it, and the
    // replacement would open onto a closed channel.
    let (generation, msg_rx) = {
        let Some(mut meta) = state.mcp.sessions.get_mut(&sid) else {
            // Removed between the insert above and here (DELETE /mcp, reaper).
            return StatusCode::UNAUTHORIZED.into_response();
        };
        meta.has_sse_stream = true;
        meta.sse_generation = next_sse_generation();
        let tx = state
            .session_maps
            .messaging_channels
            .entry(sid.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(64).0);
        (meta.sse_generation, tx.subscribe())
    };

    let mut tools_rx = state.mcp.tools_changed.subscribe();
    let mut msg_rx = msg_rx;
    // Teardown hangs off a drop guard, not off code after the loop. A client that
    // walks away has axum drop the response body mid-`select!`, so anything
    // trailing the loop never runs and every reconnect would leak a sender.
    let cleanup = SseStreamTeardown {
        state: state.clone(),
        mcp_sid: sid.clone(),
        generation,
    };

    let stream = async_stream::stream! {
        let _cleanup = cleanup;
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
pub(super) async fn mcp_instructions_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({"instructions": build_mcp_instructions(&state, None)}))
}

/// DELETE /mcp — End an MCP session
pub(super) async fn mcp_delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(sid) = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        end_mcp_session(&state, sid);
    }
    StatusCode::OK
}

/// End one MCP protocol session: its routes, and any peer identity it was the
/// last transport for. Shared by DELETE `/mcp` and an MCP-over-ACP disconnect,
/// which end the same kind of session over different transports.
pub(super) fn end_mcp_session(state: &AppState, sid: &str) {
    state.mcp.sessions.remove(sid);
    // Now that an identity can have co-owners, teardown has to read the
    // survivor list and act on it as one step: a bridge joining in the middle
    // would otherwise re-create the routes this loop is about to delete and be
    // left pointing at an identity that no longer exists. Same lock as the
    // binds, so a join lands entirely before or entirely after the teardown.
    // No `.await` inside — the guard never crosses a suspension point.
    let _bind_guard = PEER_IDENTITY_BIND_LOCK.lock();
    // Routes belong to the protocol session, so a sibling bridge that never
    // became delivery owner still drops its own — otherwise its mapping
    // outlives it and keeps resolving to an identity it no longer serves.
    state.mcp.to_session.remove(sid);
    let routed: Vec<String> = state
        .mcp
        .session_to_mcp
        .iter()
        .filter(|entry| entry.value().iter().any(|mapped| mapped == sid))
        .map(|entry| entry.key().clone())
        .collect();
    for tuic in &routed {
        let survivors = match state.mcp.session_to_mcp.get_mut(tuic) {
            Some(mut reverse) => {
                reverse.retain(|mapped_sid| mapped_sid != sid);
                reverse.clone()
            }
            None => Vec::new(),
        };
        match survivors.first() {
            // Another bridge in this PTY is still reading the identity, so
            // hand it the delivery ownership rather than tearing down a
            // mailbox and a role that are still in use.
            Some(next_owner) => {
                if let Some(mut peer) = state.peer_agents.get_mut(tuic)
                    && peer.mcp_session_id == sid
                {
                    peer.mcp_session_id = next_owner.clone();
                }
            }
            None => {
                state.mcp.session_to_mcp.remove(tuic);
            }
        }
    }
    // Clean up peer agents and inboxes left with no protocol session at all,
    // except identities someone can still reach: a one-shot `tuic` call ends
    // its session while its PTY lives on, and dropping the identity would
    // drop that PTY's inbox. Same rule as the idle reaper.
    let removed_tuic: Vec<String> = state
        .peer_agents
        .iter()
        .filter(|e| e.value().mcp_session_id == sid)
        .map(|e| e.key().clone())
        .filter(|tuic| state.peer_identity_is_reapable(tuic))
        .collect();
    for tuic in &removed_tuic {
        crate::mcp_http::remote_peer::unregister_peer(state, tuic);
        state.orchestrator_peers.remove(tuic);
        state.agent_inbox.remove(tuic);
        drop_identity_buffers(state, tuic);
        let _ = state
            .event_bus
            .send(crate::state::AppEvent::PeerUnregistered {
                tuic_session: tuic.clone(),
            });
    }
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

#[path = "mcp_transport_tests.rs"]
#[cfg(test)]
mod tests;
#[cfg(test)]
mod critic_story_tool_text {
    use super::*;

    /// Catches: a StoryAction variant (transition_history) published in the input schema but
    /// missing from the tool text, so an agent never learns it may call it.
    #[test]
    fn story_tool_text_names_every_published_action() {
        let definition = native_tool_definitions()
            .as_array()
            .expect("definitions")
            .iter()
            .find(|tool| tool["name"] == "story")
            .expect("story tool")
            .clone();
        let description = definition["description"].as_str().unwrap_or_default();
        let variants = definition["inputSchema"]["properties"]["input"]["oneOf"]
            .as_array()
            .expect("oneOf");
        let mut missing = Vec::new();
        for variant in variants {
            let action = &variant["properties"]["action"];
            let name = action["const"]
                .as_str()
                .or_else(|| action["enum"][0].as_str())
                .expect("action tag");
            if !description.contains(name) {
                missing.push(name.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "actions absent from the tool text: {missing:?}"
        );
    }

    /// Catches: the dependency sentence promising demotion only for a not-done dependency while
    /// a Done dependency without a receipt also demotes in a workflow-owned plan.
    #[test]
    fn story_tool_text_mentions_the_receipt_condition_for_dependencies() {
        let definition = native_tool_definitions()
            .as_array()
            .expect("definitions")
            .iter()
            .find(|tool| tool["name"] == "story")
            .expect("story tool")
            .clone();
        let description = definition["description"].as_str().unwrap_or_default();
        assert!(description.contains("receipt"), "{description}");
    }
}

#[cfg(test)]
mod session_placement_tests {
    use super::super::tests::test_state;
    use super::*;

    // Catches: accepting another caller's target, cross-repo navigation, main/unknown
    // checkouts, duplicate saved tabs, or an association lost when backend state restarts.
    #[test]
    #[serial_test::serial]
    fn declared_worktree_is_caller_scoped_and_durable() {
        fn insert_session(state: &AppState, id: &str) {
            use portable_pty::{CommandBuilder, PtySize, native_pty_system};
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .unwrap();
            let (shell, script_arg) = crate::test_support::host_shell();
            let mut command = CommandBuilder::new(shell);
            command.args([script_arg, &tuic_test_support::wait_for_stdin_script()]);
            let child = pair.slave.spawn_command(command).unwrap();
            let writer = pair.master.take_writer().unwrap();
            state.session_maps.sessions.insert(
                id.into(),
                parking_lot::Mutex::new(crate::state::PtySession {
                    writer: Arc::new(parking_lot::Mutex::new(writer)),
                    master: pair.master,
                    _child: child,
                    paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    worktree: None,
                    initial_cwd: None,
                    cwd: None,
                    display_name: None,
                    display_name_is_custom: false,
                    display_name_from_spawn: false,
                    is_remote: false,
                    shell: shell.into(),
                    launch_receipt: None,
                }),
            );
        }
        fn git(dir: &std::path::Path, args: &[&str]) {
            let output = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=Placement Test",
                    "-c",
                    "user.email=placement@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(dir.path().join("config"));
        let root = dir.path().join("repo");
        let foreign = dir.path().join("foreign");
        let worktree = dir.path().join("feature");
        let foreign_worktree = dir.path().join("foreign-feature");
        for repo in [&root, &foreign] {
            std::fs::create_dir(repo).unwrap();
            git(repo, &["init", "-b", "main"]);
            git(repo, &["commit", "--allow-empty", "-m", "initial"]);
        }
        git(
            &root,
            &[
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ],
        );
        git(
            &foreign,
            &[
                "worktree",
                "add",
                "-b",
                "foreign-feature",
                foreign_worktree.to_str().unwrap(),
            ],
        );
        let root_path = root.to_str().unwrap();
        let foreign_path = foreign.to_str().unwrap();
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": {
                root_path: {"path":root_path,"workspaces":{"main":{"worktreePath":root_path,"savedTerminals":[
                    {"tuicSession":"caller-peer","name":"Caller","cwd":root_path,"fontSize":14,"agentType":"codex"},
                    {"tuicSession":"sibling-peer","name":"Sibling","cwd":root_path,"fontSize":14,"agentType":"codex"}
                ]}}},
                foreign_path: {"path":foreign_path,"workspaces":{}}
            }, "repoOrder":[root_path,foreign_path]
        })).unwrap();
        let state = test_state();
        insert_session(&state, "caller-pty");
        {
            let entry = state.session_maps.sessions.get("caller-pty").unwrap();
            let mut session = entry.lock();
            session.initial_cwd = Some(root_path.into());
            // An OSC 7 navigation must not redefine which repo owns the caller.
            session.cwd = Some(foreign_path.into());
        }
        state.bind_live_pty("caller-peer", "caller-pty");
        state
            .mcp
            .to_session
            .insert("placement-mcp".into(), "caller-peer".into());
        let mut events = state.event_bus.subscribe();
        let request = serde_json::json!({"action":"declare_worktree","worktree_path":worktree});
        let before = crate::config::load_repositories();
        for rejected in [
            serde_json::json!({"action":"declare_worktree","worktree_path":worktree,"session_id":"sibling-pty"}),
            serde_json::json!({"action":"declare_worktree","worktree_path":foreign_worktree}),
            serde_json::json!({"action":"declare_worktree","worktree_path":root}),
            serde_json::json!({"action":"declare_worktree","worktree_path":dir.path().join("missing")}),
            serde_json::json!({"action":"declare_worktree"}),
        ] {
            assert!(handle_session(&state, &rejected, Some("placement-mcp"))["error"].is_string());
            assert_eq!(crate::config::load_repositories(), before);
            assert!(
                events.try_recv().is_err(),
                "rejection emitted a placement mutation"
            );
        }
        assert!(handle_session(&state, &request, None)["error"].is_string());
        let placed = handle_session(&state, &request, Some("placement-mcp"));
        assert!(placed.get("error").is_none(), "{placed}");
        assert_eq!(placed["creator_session"], "caller-pty");
        assert_eq!(placed["workspace_id"], "feature");
        let saved = crate::config::load_repositories();
        assert_eq!(
            saved["repos"][root_path]["workspaces"]["main"]["savedTerminals"][0]["tuicSession"],
            "sibling-peer"
        );
        assert_eq!(
            saved["repos"][root_path]["workspaces"]["feature"]["savedTerminals"][0]["tuicSession"],
            "caller-peer"
        );
        assert_eq!(
            handle_session(&state, &request, Some("placement-mcp")),
            placed
        );
        assert_eq!(
            crate::config::load_repositories(),
            saved,
            "a retry duplicated or reordered the saved caller"
        );
        let declared = std::iter::from_fn(|| events.try_recv().ok())
            .find_map(|event| {
                if let crate::state::AppEvent::SessionWorktreeDeclared(payload) = event {
                    Some(payload)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            crate::mcp_http::sse_routes::event_payload_for_test(
                &crate::state::AppEvent::SessionWorktreeDeclared(declared.clone())
            ),
            serde_json::to_value(declared).unwrap()
        );
        let restarted = test_state();
        insert_session(&restarted, "restored-pty");
        restarted
            .session_maps
            .sessions
            .get("restored-pty")
            .unwrap()
            .lock()
            .cwd = Some(root_path.into());
        restarted.bind_live_pty("caller-peer", "restored-pty");
        let row = super::super::session::local_session_rows(&restarted)
            .into_iter()
            .find(|r| r.session_id == "restored-pty")
            .unwrap();
        assert_eq!(
            row.worktree_path.as_deref(),
            Some(worktree.to_str().unwrap())
        );
        assert_eq!(row.worktree_branch.as_deref(), Some("feature"));
        assert_eq!(row.cwd.as_deref(), Some(root_path));
        assert!(
            restarted
                .session_maps
                .sessions
                .get("restored-pty")
                .unwrap()
                .lock()
                .worktree
                .is_none(),
            "declaration must not acquire automatic worktree cleanup ownership"
        );
        git(&root, &["worktree", "list", "--porcelain"]);
        state
            .session_maps
            .sessions
            .get("caller-pty")
            .unwrap()
            .lock()
            ._child
            .kill()
            .unwrap();
        restarted
            .session_maps
            .sessions
            .get("restored-pty")
            .unwrap()
            .lock()
            ._child
            .kill()
            .unwrap();
    }

    // Catches: an implemented declaration omitted from the discoverable request schema.
    #[test]
    fn declare_worktree_schema_documents_caller_binding_and_http_transport() {
        let definitions = native_tool_definitions();
        let session = definitions
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "session")
            .unwrap();
        assert!(
            session["description"]
                .as_str()
                .unwrap()
                .contains("POST /mcp")
        );
        assert!(
            session["inputSchema"]["properties"]["action"]["description"]
                .as_str()
                .unwrap()
                .contains("declare_worktree")
        );
        assert_eq!(
            session["inputSchema"]["properties"]["worktree_path"]["type"],
            "string"
        );
    }
}
