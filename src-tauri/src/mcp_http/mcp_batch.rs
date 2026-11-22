//! `batch`: run an ordered list of tool calls in one MCP request.
//!
//! Every element goes through `call_tool`, so it passes the same gates as a
//! separate call: capability and role checks, the private-form inspection
//! guard, the per-repo upstream allowlist and the input contract. A batch adds
//! no authority; it only saves the request round trip and the model turn.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::AppState;

pub(super) const BATCH_TOOL_NAME: &str = "batch";

/// Bounds the time one request holds the caller and the size of one reply.
pub(super) const MAX_BATCH_CALLS: usize = 32;

pub(super) fn tool_definition() -> Value {
    json!({
        "name": BATCH_TOOL_NAME,
        "description": format!(
            "Run up to {MAX_BATCH_CALLS} independent calls of any tools (native, `<tool>_<action>`, upstream; not \
             meta-tools) in order, in one request, instead of one call each. Same arguments and checks as alone. \
             Returns `results` in order and `errors` count. A call cannot read an earlier result. \
             `stop_on_error=true` stops at the first failure (`stopped_at`)."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "calls": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_BATCH_CALLS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "arguments": { "type": "object" }
                        },
                        "required": ["tool"]
                    }
                },
                "stop_on_error": { "type": "boolean", "description": "Default false" }
            },
            "required": ["calls"]
        }
    })
}

pub(super) async fn handle(
    state: &Arc<AppState>,
    caller: super::guards::CallerAuthority,
    args: &Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> Value {
    let calls = match args.get("calls").and_then(Value::as_array) {
        Some(calls) if !calls.is_empty() => calls,
        _ => return invalid("batch requires a non-empty 'calls' array"),
    };
    if calls.len() > MAX_BATCH_CALLS {
        return invalid(&format!(
            "batch accepts at most {MAX_BATCH_CALLS} calls, got {}",
            calls.len()
        ));
    }
    let stop_on_error = match args.get("stop_on_error") {
        None => false,
        Some(value) => match value.as_bool() {
            Some(value) => value,
            None => return invalid("stop_on_error must be a boolean"),
        },
    };

    let mut results = Vec::with_capacity(calls.len());
    let mut errors = 0usize;
    let mut stopped_at = None;
    for (index, call) in calls.iter().enumerate() {
        let result = run_one(state, caller, call, mcp_session_id, managed_parent_cwd).await;
        let failed = super::mcp_transport::tool_result_is_error(&result);
        results.push(super::mcp_transport::unmark_upstream_tool_result(result));
        if failed {
            errors += 1;
            if stop_on_error {
                stopped_at = Some(index);
                break;
            }
        }
    }
    let mut reply = json!({ "results": results, "errors": errors });
    if let Some(index) = stopped_at {
        reply["stopped_at"] = json!(index);
    }
    reply
}

async fn run_one(
    state: &Arc<AppState>,
    caller: super::guards::CallerAuthority,
    call: &Value,
    mcp_session_id: Option<&str>,
    managed_parent_cwd: Option<&str>,
) -> Value {
    let Some(tool) = call.get("tool").and_then(Value::as_str) else {
        return json!({"error": "each batch call requires 'tool'", "effect": "not_started"});
    };
    if tool == BATCH_TOOL_NAME {
        return json!({"error": "batch cannot contain batch", "effect": "not_started"});
    }
    let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
    // Through `call_tool`, not the native dispatcher: that path also reaches
    // upstream tools and `<tool>_<action>` names, and its outer guard is the
    // one that inspects the inner tool while a private form is open.
    Box::pin(super::mcp_transport::handle_mcp_tool_call_with_context(
        state,
        caller,
        "call_tool",
        &json!({ "tool_name": tool, "arguments": arguments }),
        mcp_session_id,
        managed_parent_cwd,
    ))
    .await
}

fn invalid(message: &str) -> Value {
    json!({
        "error": message,
        "code": "invalid_arguments",
        "effect": "not_started",
        "retryable": false
    })
}

#[cfg(test)]
#[path = "mcp_batch_tests.rs"]
mod tests;
