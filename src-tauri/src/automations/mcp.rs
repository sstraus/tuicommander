//! MCP schema and adapter; definition decisions stay in the shared action module.

use super::api::{self, Caller};
use crate::state::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

/// Advertise the native automation definition tool.
pub fn tool_definition() -> Value {
    json!({
        "name": "automations",
        "description": "One-shot: {\"action\":\"create\",\"definition\":{\"name\":\"Recheck\",\"prompt\":\"Check again\",\"in_secs\":3600,\"run_config\":\"Your saved profile\"}}. The host clock resolves in_secs, exclusive with cron/once_local. run_config must name the actual caller profile: its agent-type default is never substituted. Create/update/re-enable validate saved profile names; unresolved names return valid-profile repair choices. list marks unresolved saved profiles with run_config_error and no next_run_ms. Failed agent-created runs notify the creator inbox once, including pre-spawn failures. Bound agents may omit id, repository, workspace, cron, enabled, grace_secs, overlap and max_duration_secs; unbound HTTP/IPC callers must supply all normal definition fields (cron may be omitted for Once). Missing context names the required field. Repeating an identical generated create for the same caller returns the first saved occurrence; use an explicit new id for a separate identical reminder. Create returns the saved definition with local fire time and next_run_ms; no date/list/get needed. update accepts definition or id+enabled; list/get/pause/resume/delete manage definitions (history retained). profiles lists saved profile names; run_now/run/list_runs/summary manage runs; preview/preview_definition/preset resolve schedules; templates/template supply disabled drafts; worktree_preview/cleanup_worktree inspect/remove safe run workspaces.",
        "inputSchema": {
            "type": "object", "additionalProperties": false,
            "required": ["action"],
            "properties": {
                "action": {"type":"string", "enum":["profiles","list","get","create","update","pause","resume","delete","run_now","run","list_runs","summary","preview","preview_definition","preset","templates","template","worktree_preview","cleanup_worktree"]},
                "id": {"type":"string", "description":"Definition id, or saved run id for run/worktree_preview/cleanup_worktree"},
                "template_id":{"type":"string","description":"Built-in template id from templates"},
                "repository":{"type":"string"}, "run_config":{"type":"string"},
                "enabled":{"type":"boolean"},
                "limit":{"type":"integer","minimum":1,"maximum":100,"default":50},
                "offset":{"type":"integer","minimum":0,"default":0},
                "group_failures":{"type":"boolean","default":false,"description":"Group identical failures before pagination; each representative includes failure_count"},
                "window":{"type":"string","enum":["24h","7d"]},
                "cron":{"type":"string"}, "timezone":{"type":["string","null"]},
                "count":{"type":"integer","minimum":1,"maximum":20,"default":4},
                "preset":{"oneOf":[
                    {"type":"object","additionalProperties":false,"required":["kind","minute"],"properties":{"kind":{"const":"hourly"},"minute":{"type":"integer","minimum":0,"maximum":59}}},
                    {"type":"object","additionalProperties":false,"required":["kind","hour","minute"],"properties":{"kind":{"enum":["daily","weekdays"]},"hour":{"type":"integer","minimum":0,"maximum":23},"minute":{"type":"integer","minimum":0,"maximum":59}}},
                    {"type":"object","additionalProperties":false,"required":["kind","weekday","hour","minute"],"properties":{"kind":{"const":"weekly"},"weekday":{"type":"integer","minimum":0,"maximum":7},"hour":{"type":"integer","minimum":0,"maximum":23},"minute":{"type":"integer","minimum":0,"maximum":59}}}
                ]},
                "definition": {
                    "type":"object", "additionalProperties":false,
                    "required":["name","prompt","run_config"],
                    "properties": {
                        "id":{"type":"string"}, "name":{"type":"string"}, "prompt":{"type":"string"},
                        "run_config":{"type":"string"}, "repository":{"type":"string"},
                        "workspace":{"oneOf":[
                            {"type":"object","additionalProperties":false,"required":["mode"],"properties":{"mode":{"const":"existing"}}},
                            {"type":"object","additionalProperties":false,"required":["mode","base_branch"],"properties":{"mode":{"const":"new_per_run"},"base_branch":{"type":"string"}}}
                        ]},
                        "in_secs":{"type":"integer","minimum":1,"description":"Elapsed seconds from host now; exclusive with cron and once_local"},
                        "once_at":{"type":["string","null"],"description":"Exact UTC occurrence returned by the host; preserve on unchanged schedule edits"},
                        "once_local":{"type":["string","null"],"description":"Local YYYY-MM-DDTHH:MM:SS; cron must be empty"},
                        "created_by_session":{"type":["string","null"],"description":"Ignored on create; host owns provenance"},
                        "next_run_ms":{"type":["integer","null"],"readOnly":true,"description":"Computed next occurrence returned by create; ignored when a saved reply is reused for update or preview_definition"},
                        "cron":{"type":"string"}, "timezone":{"type":"string","description":"IANA zone; defaults to local on creation"},
                        "close_on_finish":{"type":"boolean","description":"Default false. Close only the run session after confirmed completion; failed, unknown and needs_you runs stay open. Worktrees are never removed."},
                        "enabled":{"type":"boolean"}, "grace_secs":{"type":"integer","minimum":1},
                        "overlap":{"type":"string","enum":["skip"]}, "max_duration_secs":{"type":"integer","minimum":1},
                        "precheck":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["command","timeout_secs"],"properties":{"command":{"type":"string"},"timeout_secs":{"type":"integer","minimum":1},"inject_output":{"type":"boolean","default":false,"description":"Prefix the prompt with up to 16 KiB of escaped, untrusted precheck stdout. stderr is never injected."}}}]}
                    }
                }
            }
        }
    })
}

/// Use the same async runtime and validation boundary as HTTP and IPC.
pub async fn handle(args: &Value, creator_session: Option<String>, state: Arc<AppState>) -> Value {
    reply(api::execute_as(args.clone(), state, Caller::Agent(creator_session)).await)
}

fn reply(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|error| {
        if error == super::create::PROFILE_REQUIRED {
            json!({"error": error, "code":"caller_profile_unresolved", "field":"run_config"})
        } else {
            json!({"error": error})
        }
    })
}

#[cfg(test)]
pub(super) fn execute(
    store: &super::definitions::DefinitionStore,
    args: Value,
    creator_session: Option<&str>,
) -> Value {
    reply(api::parse(args).and_then(|action| {
        api::execute_stored_as(
            store,
            &super::store::RunStore::open()?,
            action,
            chrono::Utc::now(),
            Caller::Agent(creator_session.map(str::to_owned)),
        )
    }))
}
