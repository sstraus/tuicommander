use crate::mcp_http::mcp_transport::handle_mcp_tool_call;
use serde_json::json;
use std::sync::Arc;

// Catches: the MCP create path bypassing the validation used by DefinitionStore callers.
#[tokio::test]
async fn mcp_create_cannot_skip_definition_validation() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_owned());
    std::fs::write(
        dir.path().join("agents.json"),
        serde_json::to_vec(&json!({"agents":{"codex":{"run_configs":[{
            "name":"codex", "command":"codex"
        }]}}}))
        .unwrap(),
    )
    .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    state.config.write().mcp_capabilities.denied.clear();
    state
        .mcp
        .to_session
        .insert("automation-mcp-test".into(), "creator-session".into());
    let result = handle_mcp_tool_call(
        &state,
        "127.0.0.1:1234".parse().unwrap(),
        "automations",
        &json!({"action":"create", "definition": {
            "id":"invalid", "name":"Review", "prompt":" ", "run_config":"codex",
            "repository":"/project", "workspace":{"mode":"existing"},
            "cron":"0 9 * * *", "timezone":"UTC", "enabled":true,
            "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600
        }}),
        Some("automation-mcp-test"),
    )
    .await;
    assert_eq!(result["error"], "Automation prompt must not be blank");
}

fn definition(id: &str) -> serde_json::Value {
    json!({"id":id, "name":"Review", "prompt":"Review the repository", "run_config":"codex",
        "repository":"/project", "workspace":{"mode":"existing"}, "cron":"0 9 * * *",
        "timezone":"UTC", "enabled":true, "grace_secs":43200, "overlap":"skip",
        "max_duration_secs":3600})
}

// Catches: MCP mutations discarding unrelated definitions or allowing creator identity rewrites.
#[test]
fn mcp_lifecycle_retains_other_definitions_and_original_creator() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = super::definitions::DefinitionStore::at_path(dir.path().join("automations.json"));
    let call = |args| super::mcp::execute(&store, args, Some("creator"));
    assert_eq!(call(json!({"action":"list"})), json!([]));
    for id in ["first", "second"] {
        let mut input = definition(id);
        input["created_by_session"] = json!("forged");
        assert_eq!(
            call(json!({"action":"create","definition":input}))["id"],
            id
        );
    }
    assert_eq!(
        call(json!({"action":"get","id":"first"}))["created_by_session"],
        "creator"
    );
    let mut edited = definition("first");
    edited["prompt"] = json!("Updated prompt");
    edited["created_by_session"] = json!("replacement");
    assert_eq!(
        call(json!({"action":"update","id":"first","definition":edited}))["prompt"],
        "Updated prompt"
    );
    for (action, enabled) in [("pause", false), ("resume", true)] {
        assert_eq!(
            call(json!({"action":action,"id":"first"})),
            json!({"ok":true})
        );
        let saved = call(json!({"action":"get","id":"first"}));
        assert_eq!(saved["enabled"], enabled);
        assert_eq!(saved["prompt"], "Updated prompt");
        assert_eq!(saved["created_by_session"], "creator");
        assert!(store.get("second").unwrap().enabled);
    }
    assert_eq!(
        call(json!({"action":"delete","id":"first"})),
        json!({"ok":true})
    );
    assert_eq!(
        call(json!({"action":"get","id":"first"}))["error"],
        "Automation not found"
    );
    assert_eq!(store.load().unwrap().definitions.len(), 1);
}

// Catches: transport-specific validation errors and invalid MCP edits overwriting valid disk state.
#[test]
fn mcp_and_shared_actions_reject_invalid_definitions_identically_without_writing() {
    use super::actions::{DefinitionAction, execute};
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("automations.json");
    let store = super::definitions::DefinitionStore::at_path(path.clone());
    store
        .create(serde_json::from_value(definition("first")).unwrap())
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    for (field, value) in [
        ("prompt", json!(" ")),
        ("cron", json!("bad")),
        ("timezone", json!("bad/zone")),
        ("max_duration_secs", json!(0)),
    ] {
        let mut invalid = definition("first");
        invalid[field] = value;
        let args = json!({"action":"update","id":"first","definition":invalid});
        let expected = execute(
            &store,
            serde_json::from_value::<DefinitionAction>(args.clone()).unwrap(),
            None,
        )
        .unwrap_err();
        assert_eq!(
            super::mcp::execute(&store, args, Some("creator"))["error"],
            expected
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    for action in ["get", "pause", "resume", "delete"] {
        assert_eq!(
            super::mcp::execute(&store, json!({"action":action,"id":"missing"}), None)["error"],
            "Automation not found"
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// Catches: unbound creation claiming fabricated provenance and malformed actions mutating storage.
#[test]
fn mcp_rejects_unbound_create_and_unknown_action_fields() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("automations.json");
    let store = super::definitions::DefinitionStore::at_path(path.clone());
    assert_eq!(
        super::mcp::execute(
            &store,
            json!({"action":"create","definition":definition("first")}),
            None
        )["error"],
        "Automation creation requires a bound agent session"
    );
    for args in [
        json!({"action":"pause"}),
        json!({"action":"list","id":"ignored"}),
        json!({"action":"unknown"}),
    ] {
        assert!(
            super::mcp::execute(&store, args, Some("creator"))["error"]
                .as_str()
                .unwrap()
                .starts_with("Invalid automation action:")
        );
    }
    assert!(!path.exists());
}

// Catches: native discovery omitting the MCP tool even when its handler is registered.
#[test]
fn mcp_automation_tool_is_discoverable_with_all_definition_actions() {
    let tools = crate::mcp_http::mcp_transport::test_mcp_tool_definitions();
    let tool = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "automations")
        .unwrap();
    let definition = &tool["inputSchema"]["properties"]["definition"];
    assert_eq!(
        definition["required"],
        json!(["name", "prompt", "run_config"])
    );
    assert_eq!(definition["properties"]["in_secs"]["minimum"], 1);
    assert_eq!(
        tool["inputSchema"]["properties"]["action"]["enum"],
        json!([
            "profiles",
            "list",
            "get",
            "create",
            "update",
            "pause",
            "resume",
            "delete",
            "run_now",
            "run",
            "list_runs",
            "summary",
            "preview",
            "preview_definition",
            "preset",
            "templates",
            "template",
            "worktree_preview",
            "cleanup_worktree",
            "help"
        ])
    );
}

// Catches: the MCP definition path rejecting the new opt-in or dropping it
// across create/update/get while shared HTTP/IPC definitions accept it.
#[test]
fn mcp_precheck_context_opt_in_round_trips_and_can_be_disabled() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = super::definitions::DefinitionStore::at_path(dir.path().join("automations.json"));
    let mut value = definition("context");
    value["precheck"] =
        json!({"command":"git status --short","timeout_secs":30,"inject_output":true});
    assert_eq!(
        super::mcp::execute(
            &store,
            json!({"action":"create","definition":value}),
            Some("creator")
        )["id"],
        "context"
    );
    let saved = super::mcp::execute(&store, json!({"action":"get","id":"context"}), None);
    assert_eq!(saved["precheck"]["inject_output"], true);
    let mut update = saved;
    update["precheck"]
        .as_object_mut()
        .unwrap()
        .remove("inject_output");
    assert_eq!(
        super::mcp::execute(
            &store,
            json!({"action":"update","id":"context","definition":update}),
            None
        )["precheck"]["inject_output"],
        false
    );
    let saved = super::mcp::execute(&store, json!({"action":"get","id":"context"}), None);
    assert_eq!(saved["precheck"]["inject_output"], false);
}

// Catches: additionalProperties=false rejecting the computed field in a saved
// create reply reused as an update or definition-preview request.
#[test]
fn mcp_create_reply_satisfies_update_definition_schema() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = super::definitions::DefinitionStore::at_path(dir.path().join("automations.json"));
    let tool = super::mcp::tool_definition();
    let schema = &tool["inputSchema"]["properties"]["definition"];
    assert_eq!(schema["additionalProperties"], false);
    for enabled in [true, false] {
        let mut input = definition(if enabled {
            "schema-enabled"
        } else {
            "schema-paused"
        });
        input["enabled"] = json!(enabled);
        let mut saved = super::mcp::execute(
            &store,
            json!({"action":"create","definition":input}),
            Some("creator"),
        );
        let fields = saved.as_object().expect("saved create definition");
        for required in schema["required"].as_array().unwrap() {
            assert!(fields.contains_key(required.as_str().unwrap()));
        }
        // Validate the schema's closed-object property boundary against the real
        // reply, including host-owned fields rather than a hand-built fixture.
        for field in fields.keys() {
            assert!(
                schema["properties"].get(field).is_some(),
                "create reply field {field} is rejected by the published update schema"
            );
        }
        let next_schema = &schema["properties"]["next_run_ms"];
        assert_eq!(next_schema["type"], json!(["integer", "null"]));
        assert_eq!(next_schema["readOnly"], true);
        assert!(if enabled {
            saved["next_run_ms"].is_i64()
        } else {
            saved["next_run_ms"].is_null()
        });
        saved["prompt"] = json!("Edited saved reply");
        let preview = super::mcp::execute(
            &store,
            json!({"action":"preview_definition","definition":saved}),
            Some("creator"),
        );
        assert!(preview.get("error").is_none(), "{preview}");
        let updated = super::mcp::execute(
            &store,
            json!({"action":"update","definition":saved}),
            Some("creator"),
        );
        assert_eq!(updated["prompt"], "Edited saved reply");
        assert!(updated.get("next_run_ms").is_none());
    }
}
