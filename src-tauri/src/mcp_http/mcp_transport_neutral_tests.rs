// Catches: the grouped MCP path wraps neutral replies in Result::Ok, reshapes
// snake_case fields, loses ordinal maps, or accepts grants from tool arguments.
#[cfg(unix)]
#[tokio::test]
async fn neutral_transport_mcp_drafts_share_the_desktop_contract_without_result_wrapping() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = test_state();
    state
        .mcp
        .to_session
        .insert("neutral-mcp".into(), TEST_UUID_A.into());
    insert_managed_test_session(&state, "neutral-pty", &path);
    state.bind_live_pty(TEST_UUID_A, "neutral-pty");
    crate::repo_watcher::start_watching(&path, &state).unwrap();
    let definition = serde_json::json!({"control":{"nodes":{"start":{"type":"start"},"end":{"type":"end","outcome":"done"}},"edges":{"start":{"next":"end"}}},"inputs":{"fields":{}},"outputs":{"fields":{}},"outcomes":["done"],"subject":null,"instances":[],"input_bindings":{},"conditions":{},"collections":{},"loops":{},"forks":{},"output_bindings":{},"subflows":[]});
    let reply = super::super::mcp_transport_ancillary::handle_workflow(
        &state,
        loopback_addr().into(),
        &serde_json::json!({"action":"neutral","input":{"action":"create_draft","name":"MCP neutral","definition":definition}}),
        Some("neutral-mcp"),
    );
    assert_eq!(reply["type"], "draft", "{reply}");
    assert!(reply.get("Ok").is_none());
    let action =
        serde_json::from_value(serde_json::json!({"action":"get_draft","id":reply["value"]["id"]}))
            .unwrap();
    let direct = serde_json::to_value(
        crate::workflows::neutral_api::neutral_action(&state, &path, action, None).unwrap(),
    )
    .unwrap();
    assert_eq!(direct, reply);
    let forged = super::super::mcp_transport_ancillary::handle_workflow(
        &state,
        loopback_addr().into(),
        &serde_json::json!({"action":"neutral","input":{"action":"catalog","grants":["write"]}}),
        Some("neutral-mcp"),
    );
    assert!(
        forged["error"]
            .as_str()
            .unwrap()
            .contains("invalid workflow action")
    );
}

// Catches: neutral MCP discovery loses exact action schemas or advertises
// internal completion/reconciliation commands as public caller operations.
#[test]
fn neutral_transport_schema_keeps_typed_definitions_and_private_effect_observations() {
    let schema = serde_json::to_value(schemars::schema_for!(
        crate::workflows::neutral_api::model::NeutralAction
    ))
    .unwrap();
    let encoded = schema.to_string();
    for field in [
        "catalog",
        "create_draft",
        "clone_publication",
        "preview",
        "expected_revision",
        "input_bindings",
        "approved",
        "NeutralDefinition",
        "ActivityDescriptor",
    ] {
        assert!(encoded.contains(field), "missing {field}");
    }
    for forbidden in ["\"observe\"", "\"grants\"", "\"capability_grants\""] {
        assert!(!encoded.contains(forbidden), "private command {forbidden}");
    }
    assert!(
        serde_json::from_value::<crate::workflows::neutral_api::model::NeutralAction>(
            serde_json::json!({"action":"complete","run_id":"forged"})
        )
        .is_err()
    );
    let grouped = super::super::mcp_transport_ancillary::workflow_action_schema().to_string();
    assert!(grouped.contains("NeutralAction"));
}
