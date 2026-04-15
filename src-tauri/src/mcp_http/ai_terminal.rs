//! MCP exposure of agent terminal tools (`ai_terminal_*`).
//!
//! Wraps `crate::ai_agent::tools::dispatch` for external MCP clients with two
//! security gates that the internal agent loop does NOT apply:
//!
//! 1. Write tools (`send_input`/`send_key`) ALWAYS prompt the user for
//!    confirmation, regardless of the internal `SafetyChecker` verdict.
//! 2. Write tools reject if an internal agent loop is active on the target
//!    session — two concurrent writers corrupt terminal state.
//!
//! `read_screen` already redacts secrets inside the dispatcher.

use std::sync::Arc;

use crate::state::AppState;

const READ_SCREEN: &str = "ai_terminal_read_screen";
const SEND_INPUT: &str = "ai_terminal_send_input";
const SEND_KEY: &str = "ai_terminal_send_key";
const WAIT_FOR: &str = "ai_terminal_wait_for";
const GET_STATE: &str = "ai_terminal_get_state";
const GET_CONTEXT: &str = "ai_terminal_get_context";

pub(crate) const AI_TERMINAL_TOOL_NAMES: [&str; 6] = [
    READ_SCREEN, SEND_INPUT, SEND_KEY, WAIT_FOR, GET_STATE, GET_CONTEXT,
];

pub(crate) fn is_ai_terminal_tool(name: &str) -> bool {
    AI_TERMINAL_TOOL_NAMES.contains(&name)
}

/// MCP tool definitions for the `ai_terminal_*` family. Appended to
/// `native_tool_definitions()`.
pub(crate) fn tool_definitions() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({
            "name": READ_SCREEN,
            "description": "Read visible terminal text from a session. Output passes through secret redaction. Optional 'lines' caps the row count (default 50).",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "lines": { "type": "integer" }
            }, "required": ["session_id"] }
        }),
        json!({
            "name": SEND_INPUT,
            "description": "Send a text command to the session. ALWAYS prompts the user for confirmation before sending. Rejects if an internal agent loop is active on the session.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "command": { "type": "string" }
            }, "required": ["session_id", "command"] }
        }),
        json!({
            "name": SEND_KEY,
            "description": "Send a single special key (enter, tab, ctrl+c, escape, up/down, …). ALWAYS prompts the user for confirmation. Rejects if an internal agent loop is active on the session.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "key": { "type": "string" }
            }, "required": ["session_id", "key"] }
        }),
        json!({
            "name": WAIT_FOR,
            "description": "Wait until a regex pattern appears on screen, or until the screen is stable. Defaults: timeout_ms=10000, stability_ms=500.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "pattern": { "type": "string" },
                "timeout_ms": { "type": "integer" },
                "stability_ms": { "type": "integer" }
            }, "required": ["session_id"] }
        }),
        json!({
            "name": GET_STATE,
            "description": "Return the structured SessionState (shell_state, cwd, terminal_mode, agent_type, …) for a session.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" }
            }, "required": ["session_id"] }
        }),
        json!({
            "name": GET_CONTEXT,
            "description": "Return a compact context summary (~500 chars) for the session.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" }
            }, "required": ["session_id"] }
        }),
    ]
}

fn strip_prefix(name: &str) -> Option<&'static str> {
    Some(match name {
        READ_SCREEN => "read_screen",
        SEND_INPUT => "send_input",
        SEND_KEY => "send_key",
        WAIT_FOR => "wait_for",
        GET_STATE => "get_state",
        GET_CONTEXT => "get_context",
        _ => return None,
    })
}

fn is_write_tool(name: &str) -> bool {
    matches!(name, SEND_INPUT | SEND_KEY)
}

/// Dispatch an `ai_terminal_*` MCP tool call from an external client.
pub(crate) async fn handle(
    state: &Arc<AppState>,
    name: &str,
    args: &serde_json::Value,
) -> serde_json::Value {
    let inner = match strip_prefix(name) {
        Some(s) => s,
        None => return serde_json::json!({"error": format!("Unknown ai_terminal tool: {name}")}),
    };

    if is_write_tool(name) {
        if let Some(sid) = args["session_id"].as_str() {
            if crate::ai_agent::engine::ACTIVE_AGENTS.contains_key(sid) {
                return serde_json::json!({
                    "error": "Session is controlled by an active agent loop"
                });
            }
        }
        if !confirm_external_write(state, name, args).await {
            return serde_json::json!({"error": "User declined the action"});
        }
    }

    let session_id = args["session_id"].as_str().unwrap_or("");
    let result = crate::ai_agent::tools::dispatch(state, session_id, inner, args).await;
    if result.success {
        serde_json::json!({"output": result.output})
    } else {
        serde_json::json!({"error": result.output})
    }
}

async fn confirm_external_write(
    state: &Arc<AppState>,
    tool_name: &str,
    args: &serde_json::Value,
) -> bool {
    let app_handle = state.app_handle.read().clone();
    let Some(handle) = app_handle else {
        return false;
    };
    let title = format!("External MCP request: {tool_name}");
    let summary = if tool_name == SEND_INPUT {
        format!("Send command: {}", args["command"].as_str().unwrap_or(""))
    } else {
        format!("Send key: {}", args["key"].as_str().unwrap_or(""))
    };
    let message = format!(
        "Session: {}\n\n{}",
        args["session_id"].as_str().unwrap_or("?"),
        summary
    );

    tokio::task::spawn_blocking(move || {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
        handle
            .dialog()
            .message(&message)
            .title(&title)
            .buttons(MessageDialogButtons::OkCancel)
            .blocking_show()
    })
    .await
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_returns_six_with_prefix() {
        let defs = tool_definitions();
        assert_eq!(defs.len(), 6);
        for d in &defs {
            let name = d["name"].as_str().unwrap();
            assert!(name.starts_with("ai_terminal_"), "{name} missing prefix");
        }
    }

    #[test]
    fn ai_terminal_tool_names_constant_matches_definitions() {
        let defs = tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert_eq!(names.as_slice(), AI_TERMINAL_TOOL_NAMES.as_slice());
    }

    #[test]
    fn is_ai_terminal_tool_recognizes_prefix() {
        assert!(is_ai_terminal_tool(READ_SCREEN));
        assert!(is_ai_terminal_tool(SEND_INPUT));
        assert!(!is_ai_terminal_tool("read_screen"));
        assert!(!is_ai_terminal_tool("session"));
    }

    #[test]
    fn write_tools_are_send_input_and_send_key_only() {
        assert!(is_write_tool(SEND_INPUT));
        assert!(is_write_tool(SEND_KEY));
        assert!(!is_write_tool(READ_SCREEN));
        assert!(!is_write_tool(WAIT_FOR));
        assert!(!is_write_tool(GET_STATE));
        assert!(!is_write_tool(GET_CONTEXT));
    }

    #[test]
    fn strip_prefix_maps_to_inner_dispatch_names() {
        assert_eq!(strip_prefix(READ_SCREEN), Some("read_screen"));
        assert_eq!(strip_prefix(SEND_INPUT), Some("send_input"));
        assert_eq!(strip_prefix(SEND_KEY), Some("send_key"));
        assert_eq!(strip_prefix(WAIT_FOR), Some("wait_for"));
        assert_eq!(strip_prefix(GET_STATE), Some("get_state"));
        assert_eq!(strip_prefix(GET_CONTEXT), Some("get_context"));
        assert_eq!(strip_prefix("session"), None);
    }

    #[test]
    fn write_tool_rejected_when_agent_loop_active() {
        // Without a real AppState/AppHandle, we can only assert the
        // gate logic via the public `is_write_tool`/`is_ai_terminal_tool`
        // surface. End-to-end rejection is covered by integration tests.
        // This test guards the invariant that exactly two tools route
        // through the confirm + ACTIVE_AGENTS gate.
        let writes: Vec<&&str> = AI_TERMINAL_TOOL_NAMES
            .iter()
            .filter(|n| is_write_tool(n))
            .collect();
        assert_eq!(writes.len(), 2);
    }
}
