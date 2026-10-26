//! Bounded, body-free observations at the shared HTTP/ACP tools/call boundary.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, LazyLock},
};

use axum::{Json, extract::State};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppState;

const CAPACITY: usize = 1024;
const PROJECTION_LIMIT: usize = 100;

static OPERATIONS: LazyLock<HashMap<String, Vec<String>>> = LazyLock::new(|| {
    super::mcp_transport_catalogue::native_tool_definitions()
        .as_array()
        .expect("native catalogue is an array")
        .iter()
        .map(|tool| {
            let actions = tool["inputSchema"]["properties"]["action"]["enum"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            (
                tool["name"].as_str().unwrap_or_default().to_owned(),
                actions,
            )
        })
        .collect()
});

#[derive(Clone, Serialize)]
pub(crate) struct Record {
    call_id: Uuid,
    client_call_id: Option<Uuid>,
    parent_operation_id: Option<Uuid>,
    operation: String,
    target_digest: Option<String>,
    target: Option<Target>,
    started_ms: u64,
    response_text_bytes: Option<usize>,
    application_error: Option<bool>,
    transport_error: &'static str,
    effect: &'static str,
    /// `replied`, `error` or `cancelled`. Says nothing about `effect`.
    outcome: &'static str,
}

/// Only identities resolved from runtime state, never arbitrary argument values.
#[derive(Clone, Serialize)]
struct Target {
    kind: &'static str,
    id: String,
    name: Option<String>,
}

fn target(state: &AppState, args: &Value) -> Option<Target> {
    if let Some(reference) = args["to"].as_str()
        && let Some(id) = state.resolve_peer_ref_checked(reference).ok().flatten()
    {
        let name = state.peer_agents.get(&id).map(|peer| peer.name.clone());
        return Some(Target {
            kind: "agent",
            id,
            name,
        });
    }
    let reference = args["session_id"].as_str()?;
    let id = state
        .resolve_session_ref_checked(reference)
        .ok()
        .flatten()?;
    let name = state
        .session_maps
        .sessions
        .get(&id)
        .and_then(|session| session.lock().display_name.clone());
    Some(Target {
        kind: "session",
        id,
        name,
    })
}

fn read_only(name: &str, action: &str) -> bool {
    matches!(
        (name, action),
        ("session", "list" | "output" | "status" | "wait" | "help")
            | ("agent", "list_peers" | "task_get" | "help")
            | (
                "repo",
                "list"
                    | "active"
                    | "status"
                    | "branch_integrations"
                    | "branch_integration"
                    | "worktree_list"
                    | "worktree_lifecycle"
                    | "progress_list"
                    | "work_item"
                    | "help"
            )
            | (
                "ui",
                "screenshot" | "design_status" | "design_screenshot" | "design_errors" | "help"
            )
            | ("settings", "describe" | "get")
            | ("debug", "logs" | "sessions" | "agent_detection" | "help")
            | ("voice", "status" | "help")
            | ("plugin_dev_guide" | "search_tools" | "get_tool_schema", _)
    )
}

#[derive(Default)]
pub(crate) struct Audit {
    rows: VecDeque<Record>,
    evicted: u64,
}

impl Audit {
    fn push(&mut self, row: Record) {
        if self.rows.len() == CAPACITY {
            self.rows.pop_front();
            self.evicted = self.evicted.saturating_add(1);
        }
        self.rows.push_back(row);
    }

    fn projection(&self) -> Value {
        json!({
            "records": self.rows.iter().rev().take(PROJECTION_LIMIT).collect::<Vec<_>>(),
            "retained": self.rows.len(), "evicted": self.evicted,
            "capacity": CAPACITY, "limit": PROJECTION_LIMIT,
            "scope": "runtime_mcp_tools_call_ingress",
            "delivery_observed": false
        })
    }
}

/// One projection for the HTTP route and the Tauri command, so the transports cannot drift.
pub(crate) fn snapshot(state: &AppState) -> Value {
    if !state.config.read().mcp_call_audit_enabled {
        return json!({"enabled": false, "records": []});
    }
    let mut projection = state.mcp.call_audit.lock().projection();
    projection["enabled"] = json!(true);
    projection
}

pub(super) async fn endpoint(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(snapshot(&state))
}

/// Owned across dispatch. Dropping an unfinished call must not assert rollback:
/// a blocking handler or upstream may still complete after its future is gone.
pub(super) struct Call<'a> {
    state: &'a AppState,
    read_only: bool,
    upstream: bool,
    /// `None` while the audit is disabled: the call is not observed at all.
    row: Option<Record>,
}

fn metadata_uuid(params: &Value, key: &str) -> Option<Uuid> {
    params["_meta"][key]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
}

impl<'a> Call<'a> {
    pub(super) fn start(state: &'a AppState, params: &Value, enabled: bool) -> Self {
        if !enabled {
            return Self {
                state,
                read_only: false,
                upstream: false,
                row: None,
            };
        }
        let name = params["name"].as_str().unwrap_or("");
        let args = if name == "call_tool" {
            &params["arguments"]["arguments"]
        } else {
            &params["arguments"]
        };
        let name = if name == "call_tool" {
            params["arguments"]["tool_name"].as_str().unwrap_or("")
        } else {
            name
        };
        // Native/meta catalogue and connected upstream definitions supply safe operation names.
        // Unrecognized caller strings stay digested; bodies are never copied.
        let operation = match OPERATIONS.get(name) {
            Some(actions) => {
                let action = args["action"]
                    .as_str()
                    .filter(|action| actions.iter().any(|value| value == action));
                action.map_or_else(|| name.to_owned(), |action| format!("{name}.{action}"))
            }
            None if matches!(name, "search_tools" | "get_tool_schema") => name.to_owned(),
            None => {
                let tools = state.mcp.upstream_registry.aggregated_tools();
                if let Some(tool) = tools.iter().find(|tool| tool["name"] == name) {
                    let action = args["action"].as_str().filter(|action| {
                        tool["inputSchema"]["properties"]["action"]["enum"]
                            .as_array()
                            .is_some_and(|actions| actions.iter().any(|value| value == action))
                    });
                    action.map_or_else(|| name.to_owned(), |action| format!("{name}.{action}"))
                } else {
                    format!("opaque:{}", hex::encode(Sha256::digest(name.as_bytes())))
                }
            }
        };
        let target_digest = ["to", "session_id", "path", "worktree_path", "id"]
            .into_iter()
            .find_map(|key| {
                args[key].as_str().map(|value| {
                    let mut digest = Sha256::new();
                    digest.update(key.as_bytes());
                    digest.update([0]);
                    digest.update(value.as_bytes());
                    hex::encode(digest.finalize())
                })
            });
        Self {
            state,
            read_only: read_only(name, args["action"].as_str().unwrap_or_default()),
            upstream: name.contains("__"),
            row: Some(Record {
                call_id: Uuid::new_v4(),
                client_call_id: metadata_uuid(params, "tuic/call_id"),
                parent_operation_id: metadata_uuid(params, "tuic/parent_operation_id"),
                operation,
                target_digest,
                target: target(state, args),
                started_ms: super::mcp_transport_peer::now_unix_ms(),
                response_text_bytes: None,
                application_error: None,
                transport_error: "cancelled_before_reply",
                effect: "unknown",
                outcome: "cancelled",
            }),
        }
    }

    pub(super) fn finish(mut self, raw: &Value, formatted: &Value) {
        let Some(mut row) = self.row.take() else {
            return;
        };
        let application_error = formatted["isError"].as_bool().unwrap_or(false);
        row.application_error = Some(application_error);
        row.outcome = if application_error {
            "error"
        } else {
            "replied"
        };
        row.response_text_bytes = Some(formatted["content"].as_array().map_or(0, |content| {
            content
                .iter()
                .filter_map(|block| {
                    if block["type"] == "text" {
                        block["text"].as_str()
                    } else {
                        None
                    }
                })
                .map(str::len)
                .sum()
        }));
        // Reply construction observes handler completion, not delivery to the client
        // or completion of a queued external effect. An untyped upstream error can
        // still be a lost reply after send, so it must not imply rollback.
        row.transport_error = "unobserved_after_reply_construction";
        row.effect = match raw["effect"].as_str() {
            Some("not_started") => "not_started",
            Some("completed") => "completed",
            Some("unknown") => "unknown",
            _ if self.upstream && super::mcp_transport::upstream_tool_result(raw).is_none() => {
                "unknown"
            }
            _ if application_error => "failed",
            _ if self.read_only => "read_only",
            _ => "completed",
        };
        if let Some(target) = row.target.as_mut() {
            target.name = if target.kind == "agent" {
                self.state
                    .peer_agents
                    .get(&target.id)
                    .map(|peer| peer.name.clone())
            } else {
                self.state
                    .session_maps
                    .sessions
                    .get(&target.id)
                    .and_then(|session| session.lock().display_name.clone())
            }
            .or_else(|| target.name.take());
        }
        self.state.mcp.call_audit.lock().push(row);
    }
}

impl Drop for Call<'_> {
    fn drop(&mut self) {
        if let Some(row) = self.row.take() {
            self.state.mcp.call_audit.lock().push(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: runtime audit retention or its public projection grows without bound.
    #[test]
    fn mcp_outcome_record_projection_evicts_without_retaining_arbitrary_metadata() {
        let state = super::super::tests::test_state();
        for _ in 0..=CAPACITY {
            drop(Call::start(
                &state,
                &json!({"name":"SECRET_TOOL","_meta":{"tuic/call_id":"SECRET_ID"}}),
                true,
            ));
        }
        let projection = state.mcp.call_audit.lock().projection();
        assert_eq!(projection["retained"], CAPACITY);
        assert_eq!(projection["evicted"], 1);
        assert_eq!(
            projection["records"].as_array().unwrap().len(),
            PROJECTION_LIMIT
        );
        assert!(!projection.to_string().contains("SECRET"));
    }
}
