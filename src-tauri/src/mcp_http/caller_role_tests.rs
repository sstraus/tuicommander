// --- the agent tool per caller role (story 2805) ---

const ROLE_PARENT: &str = "550e8400-e29b-41d4-a716-446655442805";
const ROLE_PARENT_MCP: &str = "mcp-role-parent";
const ROLE_CHILD: &str = "550e8400-e29b-41d4-a716-446655442806";
const ROLE_CHILD_MCP: &str = "mcp-role-child";

/// A registered parent and a child linked to it by identity alone: no
/// capability set, the case the spawn-time capability gate lets through.
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

// RED — catches: a managed child can still call agent spawn after the split.
#[tokio::test]
async fn managed_child_cannot_call_agent_spawn() {
    let state = role_state();
    let direct = mcp_transport::handle_mcp_tool_call(
        &state,
        cap_addr(),
        "agent",
        &grandchild_spawn(),
        Some(ROLE_CHILD_MCP),
    )
    .await;
    let wrapped = mcp_transport::handle_mcp_tool_call(
        &state,
        cap_addr(),
        "call_tool",
        &serde_json::json!({"tool_name": "agent", "arguments": grandchild_spawn()}),
        Some(ROLE_CHILD_MCP),
    )
    .await;
    let leaked = kill_capability_sessions(&state, ROLE_CHILD, &[&direct, &wrapped]);

    assert!(
        leaked.is_empty(),
        "a managed child created sessions {leaked:?}"
    );
    for refused in [&direct, &wrapped] {
        assert_eq!(refused["code"], "role_denied", "{refused}");
        assert_eq!(refused["role"], "managed_child", "{refused}");
        assert_eq!(refused["effect"], "not_started", "{refused}");
        assert!(
            refused["error"].as_str().unwrap().contains("BLOCKED"),
            "the refusal must say how to proceed: {refused}"
        );
    }
}

// Catches: the gate checks only spawn, so another hidden action (or a
// subscription read through an allowed action name) still reaches its handler.
#[tokio::test]
async fn managed_child_is_refused_every_hidden_action_but_keeps_mail() {
    let state = role_state();
    let call = |args: serde_json::Value| {
        let state = state.clone();
        async move {
            mcp_transport::handle_mcp_tool_call(
                &state,
                cap_addr(),
                "agent",
                &args,
                Some(ROLE_CHILD_MCP),
            )
            .await
        }
    };
    for args in [
        serde_json::json!({"action": "task_get", "task_id": "t"}),
        serde_json::json!({"action": "subscribe", "subscription_id": "s"}),
        serde_json::json!({"action": "inbox", "subscription_id": "s"}),
        serde_json::json!({"action": "launch", "launch_id": "l"}),
    ] {
        let refused = call(args.clone()).await;
        assert_eq!(refused["code"], "role_denied", "{args} -> {refused}");
    }
    let sent =
        call(serde_json::json!({"action": "send", "to": ROLE_PARENT, "message": "RESULT"})).await;
    assert!(
        sent.get("error").is_none(),
        "a child must still report: {sent}"
    );
    let inbox = call(serde_json::json!({"action": "inbox"})).await;
    assert!(
        inbox.get("error").is_none(),
        "a child must still read mail: {inbox}"
    );
}

// Catches: the catalogue is chosen from the wrong identity — a child still
// lists spawn, or a root or a child granted `children` loses it.
#[test]
fn agent_catalogue_follows_the_caller_role() {
    let state = role_state();
    let child = agent_definition(&state, ROLE_CHILD_MCP);
    assert_eq!(
        agent_actions(&child),
        ["register", "list_peers", "send", "inbox", "wait", "help"]
    );
    let properties = child["inputSchema"]["properties"].as_object().unwrap();
    for hidden in ["prompt", "launch_id", "task_id", "subscription_id"] {
        assert!(
            !properties.contains_key(hidden),
            "child still lists {hidden}"
        );
    }
    let description = child["description"].as_str().unwrap();
    assert!(!description.contains("\nspawn:") && !description.contains("\nlaunch:"));
    assert!(description.contains("\nsend:") && description.contains("\nwait:"));

    state.orchestrator_peers.insert(ROLE_PARENT.into());
    let full = agent_actions(&agent_definition(&state, ROLE_PARENT_MCP));
    assert!(full.iter().any(|a| a == "spawn") && full.iter().any(|a| a == "launch"));

    state.agent_capabilities.insert(
        ROLE_CHILD.into(),
        crate::agent_capabilities::CapabilitySet::from_spawn_arg(&serde_json::json!(["children"]))
            .unwrap(),
    );
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

// Catches: a child's instructions still tell it to delegate, or omit the line
// that names what it lacks.
#[test]
fn managed_child_instructions_name_what_it_lacks() {
    let state = role_state();
    let child = build_mcp_instructions_for_session(&state, None, false, Some(ROLE_CHILD_MCP));
    assert!(
        child.contains(crate::mcp_http::caller_role::CHILD_MISSING_LINE),
        "{child}"
    );
    assert!(!child.contains("Delegate or parallelize"), "{child}");
    let root = build_mcp_instructions_for_session(&state, None, false, Some(ROLE_PARENT_MCP));
    assert!(!root.contains(crate::mcp_http::caller_role::CHILD_MISSING_LINE));
    assert!(
        root.contains(crate::mcp_http::caller_role::AGENT_MISSING_LINE),
        "{root}"
    );
    state.orchestrator_peers.insert(ROLE_PARENT.into());
    let boss = build_mcp_instructions_for_session(&state, None, false, Some(ROLE_PARENT_MCP));
    assert!(!boss.contains(crate::mcp_http::caller_role::AGENT_MISSING_LINE));
}

// Catches: a child with an explicit empty capability set is treated as a
// spawner (set present, so "no entry" logic is skipped) and reaches the
// hidden actions.
#[tokio::test]
async fn empty_capability_set_child_is_refused_subscribe_and_task_get() {
    let state = role_state();
    state.agent_capabilities.insert(
        ROLE_CHILD.into(),
        crate::agent_capabilities::CapabilitySet::from_spawn_arg(&serde_json::json!([])).unwrap(),
    );
    for args in [
        serde_json::json!({"action": "subscribe", "subscription_id": "s"}),
        serde_json::json!({"action": "task_get", "task_id": "t"}),
    ] {
        let refused = mcp_transport::handle_mcp_tool_call(
            &state,
            cap_addr(),
            "agent",
            &args,
            Some(ROLE_CHILD_MCP),
        )
        .await;
        assert_eq!(refused["code"], "role_denied", "{args} -> {refused}");
        assert_eq!(refused["role"], "managed_child", "{refused}");
    }
}

// Catches: a child holding `children` is demoted to ManagedChild.
#[test]
fn child_holding_children_is_an_agent() {
    let state = role_state();
    state.agent_capabilities.insert(
        ROLE_CHILD.into(),
        crate::agent_capabilities::CapabilitySet::from_spawn_arg(&serde_json::json!(["children"]))
            .unwrap(),
    );
    assert_eq!(
        crate::mcp_http::caller_role::resolve(&state, Some(ROLE_CHILD_MCP)),
        crate::mcp_http::caller_role::CallerRole::Agent
    );
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
    // A managed child that registers itself orchestrator stays a child.
    state.orchestrator_peers.insert(ROLE_CHILD.into());
    assert_eq!(
        resolve(&state, Some(ROLE_CHILD_MCP)),
        CallerRole::ManagedChild
    );
}
