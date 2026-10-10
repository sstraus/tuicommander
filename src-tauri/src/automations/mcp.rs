//! MCP schema and adapter; definition decisions stay in the shared action module.

use super::{
    actions::{self, DefinitionAction},
    definitions::DefinitionStore,
};
use serde_json::{Value, json};

/// Advertise the native automation definition tool.
pub fn tool_definition() -> Value {
    json!({
        "name": "automations",
        "description": "Manage scheduled agent definitions. list returns definitions; get requires id. create requires definition; update requires id and a complete definition. pause/resume change only enabled. delete retains run history. Creation records the bound caller session as provenance. Uses the same DefinitionStore validation as other adapters. This tool does not run or retry an automation.",
        "inputSchema": {
            "type": "object", "additionalProperties": false,
            "required": ["action"],
            "properties": {
                "action": {"type":"string", "enum":["list","get","create","update","pause","resume","delete"]},
                "id": {"type":"string", "description":"Definition id, required except list and create"},
                "definition": {
                    "type":"object", "additionalProperties":false,
                    "required":["id","name","prompt","run_config","repository","workspace","cron","enabled","grace_secs","overlap","max_duration_secs"],
                    "properties": {
                        "id":{"type":"string"}, "name":{"type":"string"}, "prompt":{"type":"string"},
                        "run_config":{"type":"string"}, "repository":{"type":"string"},
                        "workspace":{"oneOf":[
                            {"type":"object","additionalProperties":false,"required":["mode"],"properties":{"mode":{"const":"existing"}}},
                            {"type":"object","additionalProperties":false,"required":["mode","base_branch"],"properties":{"mode":{"const":"new_per_run"},"base_branch":{"type":"string"}}}
                        ]},
                        "cron":{"type":"string"}, "timezone":{"type":"string","description":"IANA zone; defaults to local on creation"},
                        "enabled":{"type":"boolean"}, "grace_secs":{"type":"integer","minimum":1},
                        "overlap":{"type":"string","enum":["skip"]}, "max_duration_secs":{"type":"integer","minimum":1},
                        "precheck":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["command","timeout_secs"],"properties":{"command":{"type":"string"},"timeout_secs":{"type":"integer","minimum":1}}}]}
                    }
                }
            }
        }
    })
}

/// Execute blocking storage outside the async runtime's worker threads.
pub async fn handle(args: &Value, creator_session: Option<String>) -> Value {
    let args = args.clone();
    match tokio::task::spawn_blocking(move || {
        execute(&DefinitionStore::new(), args, creator_session.as_deref())
    })
    .await
    {
        Ok(value) => value,
        Err(error) => json!({"error": format!("Automation handler failed: {error}")}),
    }
}

pub(super) fn execute(
    store: &DefinitionStore,
    args: Value,
    creator_session: Option<&str>,
) -> Value {
    let result = serde_json::from_value::<DefinitionAction>(args)
        .map_err(|error| format!("Invalid automation action: {error}"))
        .and_then(|action| {
            if matches!(&action, DefinitionAction::Create { .. }) && creator_session.is_none() {
                return Err("Automation creation requires a bound agent session".into());
            }
            actions::execute(store, action, creator_session)
        });
    match result {
        Ok(value) => value,
        Err(error) => json!({"error": error}),
    }
}
