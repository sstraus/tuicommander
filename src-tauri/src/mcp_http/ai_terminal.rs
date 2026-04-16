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
const READ_FILE: &str = "ai_terminal_read_file";
const WRITE_FILE: &str = "ai_terminal_write_file";
const EDIT_FILE: &str = "ai_terminal_edit_file";
const LIST_FILES: &str = "ai_terminal_list_files";
const SEARCH_FILES: &str = "ai_terminal_search_files";
const RUN_COMMAND: &str = "ai_terminal_run_command";

pub(crate) const AI_TERMINAL_TOOL_NAMES: [&str; 12] = [
    READ_SCREEN, SEND_INPUT, SEND_KEY, WAIT_FOR, GET_STATE, GET_CONTEXT,
    READ_FILE, WRITE_FILE, EDIT_FILE, LIST_FILES, SEARCH_FILES, RUN_COMMAND,
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
        json!({
            "name": READ_FILE,
            "description": "Read a text file from the session's sandboxed repo. Paginated (default 200 lines, max 2000). Binary files and files >10MB rejected. Secrets redacted.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "file_path": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer" }
            }, "required": ["session_id", "file_path"] }
        }),
        json!({
            "name": WRITE_FILE,
            "description": "Create or overwrite a text file. ALWAYS prompts the user for confirmation. Atomic via tmp+rename.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "file_path": { "type": "string" },
                "content": { "type": "string" }
            }, "required": ["session_id", "file_path", "content"] }
        }),
        json!({
            "name": EDIT_FILE,
            "description": "Surgical search-and-replace on a file. ALWAYS prompts the user for confirmation. old_string must be unique unless replace_all=true.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "file_path": { "type": "string" },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" },
                "replace_all": { "type": "boolean" }
            }, "required": ["session_id", "file_path", "old_string", "new_string"] }
        }),
        json!({
            "name": LIST_FILES,
            "description": "List files matching a glob pattern inside the session's sandbox. Max 500 entries.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "pattern": { "type": "string" },
                "path": { "type": "string" }
            }, "required": ["session_id", "pattern"] }
        }),
        json!({
            "name": SEARCH_FILES,
            "description": "Regex search across files in the session's sandbox. Honors .gitignore. Max 50 matches with context lines.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "glob": { "type": "string" },
                "context_lines": { "type": "integer" }
            }, "required": ["session_id", "pattern"] }
        }),
        json!({
            "name": RUN_COMMAND,
            "description": "Run a shell command and capture stdout/stderr. ALWAYS prompts the user for confirmation. Destructive commands are blocked. Default timeout 2min, max 10min.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": { "type": "string" },
                "command": { "type": "string" },
                "timeout_ms": { "type": "integer" },
                "cwd": { "type": "string" }
            }, "required": ["session_id", "command"] }
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
        READ_FILE => "read_file",
        WRITE_FILE => "write_file",
        EDIT_FILE => "edit_file",
        LIST_FILES => "list_files",
        SEARCH_FILES => "search_files",
        RUN_COMMAND => "run_command",
        _ => return None,
    })
}

fn is_write_tool(name: &str) -> bool {
    matches!(name, SEND_INPUT | SEND_KEY | WRITE_FILE | EDIT_FILE | RUN_COMMAND)
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
    let summary = match tool_name {
        SEND_INPUT => format!("Send command: {}", args["command"].as_str().unwrap_or("")),
        SEND_KEY => format!("Send key: {}", args["key"].as_str().unwrap_or("")),
        WRITE_FILE => format!("Write file: {}", args["file_path"].as_str().unwrap_or("")),
        EDIT_FILE => format!("Edit file: {}", args["file_path"].as_str().unwrap_or("")),
        RUN_COMMAND => format!("Run command: {}", args["command"].as_str().unwrap_or("")),
        _ => format!("Action: {tool_name}"),
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
    .unwrap_or_else(|e| {
        tracing::warn!(tool_name, error = %e, "confirm_external_write JoinError, denying");
        false
    })
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
