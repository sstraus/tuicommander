// --- the agent tool per caller role (story 2805) ---

const ROLE_PARENT: &str = "550e8400-e29b-41d4-a716-446655442805";
const ROLE_PARENT_MCP: &str = "mcp-role-parent";
const ROLE_CHILD: &str = "550e8400-e29b-41d4-a716-446655442806";
const ROLE_CHILD_MCP: &str = "mcp-role-child";

/// A registered parent and a child linked to it by identity alone.
fn role_state() -> Arc<AppState> {
    let state = test_state();
    register_peer(&state, ROLE_PARENT, "parent", ROLE_PARENT_MCP);
    register_peer(&state, ROLE_CHILD, "child", ROLE_CHILD_MCP);
    state
        .session_maps
        .session_parent
        .insert(ROLE_CHILD.into(), ROLE_PARENT.into());
    state
}

fn agent_definition(state: &Arc<AppState>, mcp: &str) -> serde_json::Value {
    merged_tool_definitions_for_mode(state, Some(mcp), false)
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "agent")
        .cloned()
        .expect("agent listed")
}

fn agent_actions(definition: &serde_json::Value) -> Vec<String> {
    definition["inputSchema"]["properties"]["action"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action.as_str().unwrap().to_owned())
        .collect()
}

// Catches: a spawned child is still narrowed to mail only after the
// `children` capability was removed, so it cannot delegate.
#[tokio::test]
async fn spawned_child_may_spawn_but_not_orchestrate() {
    let state = role_state();
    let spawned = mcp_transport::handle_mcp_tool_call(
        &state,
        cap_addr(),
        "agent",
        &grandchild_spawn(),
        Some(ROLE_CHILD_MCP),
    )
    .await;
    let launched = mcp_transport::handle_mcp_tool_call(
        &state,
        cap_addr(),
        "agent",
        &serde_json::json!({"action": "launch", "launch_id": "l"}),
        Some(ROLE_CHILD_MCP),
    )
    .await;
    let created = kill_capability_sessions(&state, ROLE_CHILD, &[&spawned]);

    assert_eq!(created.len(), 1, "the child must spawn: {spawned}");
    assert_eq!(launched["code"], "role_denied", "{launched}");
    assert_eq!(launched["role"], "agent", "{launched}");
}

// Catches: the catalogue is chosen from the wrong identity — a child loses
// spawn, or a registered orchestrator loses launch.
#[test]
fn agent_catalogue_follows_the_caller_role() {
    let state = role_state();
    assert_eq!(
        agent_actions(&agent_definition(&state, ROLE_CHILD_MCP)),
        [
            "spawn",
            "register",
            "list_peers",
            "send",
            "inbox",
            "wait",
            "task_get",
            "task_cancel",
            "help"
        ]
    );
    state.orchestrator_peers.insert(ROLE_PARENT.into());
    let full = agent_actions(&agent_definition(&state, ROLE_PARENT_MCP));
    assert!(full.iter().any(|a| a == "spawn") && full.iter().any(|a| a == "launch"));
}

// Catches: a field added to an action the child keeps is missing from the
// ownership table, so every child silently loses it.
#[test]
fn every_agent_field_has_an_owning_action() {
    let definition = crate::mcp_http::mcp_transport_catalogue::native_validation_tool_definitions()
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "agent")
        .cloned()
        .unwrap();
    let owned: Vec<&str> = crate::mcp_http::caller_role::AGENT_ACTION_FIELDS
        .iter()
        .flat_map(|(_, fields)| fields.iter().copied())
        .collect();
    for field in definition["inputSchema"]["properties"]
        .as_object()
        .unwrap()
        .keys()
    {
        assert!(
            field == "action" || owned.contains(&field.as_str()),
            "agent field {field} has no owning action in AGENT_ACTION_FIELDS"
        );
    }
}

// Catches: a child's instructions omit the line that names what an agent
// lacks, or a registered orchestrator still gets it.
#[test]
fn agent_instructions_name_what_it_lacks() {
    let state = role_state();
    let child = build_mcp_instructions_for_session(&state, None, false, Some(ROLE_CHILD_MCP));
    assert!(
        child.contains(crate::mcp_http::caller_role::AGENT_MISSING_LINE),
        "{child}"
    );
    state.orchestrator_peers.insert(ROLE_PARENT.into());
    let boss = build_mcp_instructions_for_session(&state, None, false, Some(ROLE_PARENT_MCP));
    assert!(!boss.contains(crate::mcp_http::caller_role::AGENT_MISSING_LINE));
}

// Catches: the orchestrator-only actions reach a registered agent, or an agent
// that registers itself orchestrator=false/true is read from its arguments
// instead of the identity TUIC holds.
#[tokio::test]
async fn agent_keeps_spawn_but_launch_and_subscribe_belong_to_the_orchestrator() {
    use crate::mcp_http::caller_role::{CallerRole, resolve};
    let state = role_state();
    assert_eq!(resolve(&state, Some(ROLE_PARENT_MCP)), CallerRole::Agent);
    for (action, extra) in [
        ("launch", serde_json::json!({"launch_id": "x"})),
        ("subscribe", serde_json::json!({"subscription_id": "x"})),
        (
            "ack",
            serde_json::json!({"subscription_id": "x", "cursor": 0}),
        ),
        ("unsubscribe", serde_json::json!({"subscription_id": "x"})),
    ] {
        let mut args = extra;
        args["action"] = serde_json::json!(action);
        let refused = mcp_transport::handle_mcp_tool_call(
            &state,
            cap_addr(),
            "agent",
            &args,
            Some(ROLE_PARENT_MCP),
        )
        .await;
        assert_eq!(refused["code"], "role_denied", "{action}: {refused}");
        assert_eq!(refused["role"], "agent", "{action}: {refused}");
        assert!(refused["error"].as_str().unwrap().contains("Role agent"));
    }
    state.orchestrator_peers.insert(ROLE_PARENT.into());
    assert_eq!(
        resolve(&state, Some(ROLE_PARENT_MCP)),
        CallerRole::Orchestrator
    );
    let allowed = mcp_transport::handle_mcp_tool_call(
        &state,
        cap_addr(),
        "agent",
        &serde_json::json!({"action": "task_get"}),
        Some(ROLE_PARENT_MCP),
    )
    .await;
    assert_ne!(allowed["code"], "role_denied", "{allowed}");
    // A spawned child that registers itself orchestrator stays an agent.
    state.orchestrator_peers.insert(ROLE_CHILD.into());
    assert_eq!(resolve(&state, Some(ROLE_CHILD_MCP)), CallerRole::Agent);
}
