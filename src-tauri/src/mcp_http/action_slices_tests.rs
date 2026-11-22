// Action-scoped discovery of the multiplexed native tools (Adaptive Speakeasy).

const SLICED: [&str; 4] = ["agent", "session", "repo", "ui"];

fn advertised(name: &str) -> serde_json::Value {
    native_tool_definitions()
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == name)
        .cloned()
        .unwrap()
}

fn slice(tool: &str, action: &str) -> serde_json::Value {
    crate::mcp_http::action_slices::action_slice(&advertised(tool), action)
        .unwrap_or_else(|| panic!("{tool}.{action} has no slice"))
}

fn top(state: &Arc<AppState>, query: &str, session: Option<&str>) -> serde_json::Value {
    handle_search_tools_for_session(state, &serde_json::json!({ "query": query }), session)
}

/// Catches: a new field that no action claims never reaches any action schema, and a
/// mapped field name with a typo silently drops out of its action.
#[test]
fn every_advertised_field_belongs_to_an_action_and_every_mapped_field_exists() {
    let mut failures = Vec::new();
    for tool in SLICED {
        let definition = advertised(tool);
        let properties = definition["inputSchema"]["properties"].as_object().unwrap();
        let fields = crate::mcp_http::action_slices::action_fields(tool).unwrap();
        for name in properties.keys().filter(|name| *name != "action") {
            if !fields
                .iter()
                .any(|(_, reads)| reads.contains(&name.as_str()))
            {
                failures.push(format!("{tool}.{name} belongs to no action"));
            }
        }
        for (action, reads) in fields {
            for field in *reads {
                if !properties.contains_key(*field) {
                    failures.push(format!("{tool}.{action} maps missing field {field}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Catches: an action schema that drops the directives TUIC injects for the whole
/// tool, so an agent discovering one action loses the rules that bind every action.
#[test]
fn every_action_slice_keeps_the_tool_directives() {
    let directives = [
        (
            "agent",
            "Every worker must report task output or blockers with send.",
        ),
        ("agent", "Wait: do NOT poll in a loop; waits cap at 300s."),
        (
            "session",
            "Executing/destructive session actions require a loopback or authenticated caller.",
        ),
        ("session", "All session_id actions accept PTY id"),
        ("repo", "Repositories and version control."),
        ("ui", "TUIC interaction."),
    ];
    let mut failures = Vec::new();
    for tool in SLICED {
        let definition = advertised(tool);
        for action in definition["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
        {
            let action = action.as_str().unwrap();
            let description = slice(tool, action)["description"]
                .as_str()
                .unwrap()
                .to_owned();
            for (_, directive) in directives.iter().filter(|(owner, _)| *owner == tool) {
                if !description.contains(directive) {
                    failures.push(format!("{tool}.{action} lost {directive:?}"));
                }
            }
        }
    }
    let eval = slice("ui", "design_eval")["description"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(eval.contains("Design actions retain armed caller/repository checks"));
    assert!(
        !slice("ui", "toast")["description"]
            .as_str()
            .unwrap()
            .contains("Design actions retain")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Catches: an action schema that omits a field its action requires, or carries the
/// fields and guidance of other actions it was meant to leave behind.
#[test]
fn action_slice_requires_its_own_fields_and_leaves_other_actions_out() {
    let submit = slice("session", "submit");
    let required = submit["inputSchema"]["required"].as_array().unwrap();
    assert_eq!(submit["name"], "session_submit");
    for field in ["session_id", "input"] {
        assert!(
            required.contains(&serde_json::json!(field)),
            "{field}: {submit}"
        );
    }
    assert!(submit["inputSchema"]["properties"].get("shell").is_none());
    assert!(
        !submit["description"]
            .as_str()
            .unwrap()
            .contains("declare_worktree:")
    );

    let tab = slice("ui", "tab");
    let required = tab["inputSchema"]["required"].as_array().unwrap();
    for field in ["id", "title"] {
        assert!(
            required.contains(&serde_json::json!(field)),
            "{field}: {tab}"
        );
    }

    let send = slice("agent", "send");
    assert!(send["inputSchema"]["properties"].get("spawn").is_none());
    let bytes = serde_json::to_vec(&send).unwrap().len();
    let whole = serde_json::to_vec(&advertised("agent")).unwrap().len();
    assert!(
        bytes * 3 < whole,
        "agent.send slice {bytes} B vs whole {whole} B"
    );
}

/// Catches: search ranking a multiplexed tool by its whole, diluted description, or
/// ordering same-named action entries by catalogue position instead of relevance.
#[test]
fn search_ranks_the_requested_action_first() {
    let state = test_state();
    let caller = every_tool_caller(&state);
    rebuild_tool_search_index(&state);
    let cases = [
        ("send message to peer agent", "agent", "send"),
        ("read terminal output of a session", "session", "output"),
        ("create a git worktree", "repo", "worktree_create"),
        ("show a toast notification", "ui", "toast"),
        ("open a panel tab with html", "ui", "tab"),
        (
            "submit a command to an idle agent terminal",
            "session",
            "submit",
        ),
        ("spawn a new agent with a prompt", "agent", "spawn"),
        ("read my inbox mail", "agent", "inbox"),
        ("PTY multiplexer tmux pane lifecycle", "session", ""),
    ];
    let mut failures = Vec::new();
    for (query, tool, action) in cases {
        let response = top(&state, query, caller);
        let first = &response["results"][0];
        let expected = if action.is_empty() {
            tool.to_owned()
        } else {
            format!("{tool}_{action}")
        };
        let name = first["name"].as_str().unwrap_or_default();
        let hit = if action.is_empty() {
            name.starts_with(tool)
        } else {
            name == expected
        };
        if !hit {
            failures.push(format!("{query:?} -> {first}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Catches: `top_schema` still inlining the whole multiplexed definition instead of
/// the selected action, which is what made one `send` cost the full `agent` schema.
#[test]
fn top_schema_is_the_selected_action_only() {
    let state = test_state();
    let caller = every_tool_caller(&state);
    rebuild_tool_search_index(&state);
    let response = top(&state, "send message to peer agent", caller);
    let schema = &response["top_schema"];
    assert_eq!(schema["name"], "agent_send", "{response}");
    assert_eq!(schema["_meta"]["tuic/action"], "send", "{response}");
    assert!(
        schema["inputSchema"]["properties"].get("spawn").is_none(),
        "{response}"
    );
}

/// Catches: action discovery exposing an action the caller's role may not use.
#[test]
fn child_cannot_discover_a_role_denied_action() {
    let state = test_state();
    state
        .mcp
        .to_session
        .insert("child-slices".into(), "child-slices-peer".into());
    state
        .session_maps
        .session_parent
        .insert("child-slices-peer".into(), "parent-peer".into());
    rebuild_tool_search_index(&state);
    let child = Some("child-slices");

    let response = top(&state, "launch a managed agent in a workspace", child);
    let leaked = response["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "agent_launch");
    assert!(!leaked, "{response}");

    let denied = handle_get_tool_schema_for_session(
        &state,
        &serde_json::json!({"tool_name": "agent", "action": "launch"}),
        child,
    );
    assert!(denied["error"].as_str().is_some(), "{denied}");
    let denied = handle_get_tool_schema_for_session(
        &state,
        &serde_json::json!({"tool_name": "agent_launch"}),
        child,
    );
    assert!(denied["error"].as_str().is_some(), "{denied}");
    let allowed = handle_get_tool_schema_for_session(
        &state,
        &serde_json::json!({"tool_name": "agent", "action": "send"}),
        child,
    );
    assert_eq!(allowed["_meta"]["tuic/action"], "send", "{allowed}");
}

/// Catches: exact selection resolving a shared name to an arbitrary action, or no
/// longer reaching one action by `tool.action`.
#[test]
fn select_names_a_whole_tool_or_one_action() {
    let state = test_state();
    let caller = every_tool_caller(&state);
    rebuild_tool_search_index(&state);
    let whole = top(&state, "select:agent", caller);
    assert_eq!(whole["results"].as_array().unwrap().len(), 1, "{whole}");
    assert_eq!(whole["results"][0]["name"], "agent", "{whole}");
    for query in [
        "select:agent_inbox",
        "select:agent.inbox",
        "select:agent/inbox",
        "select:agent inbox",
    ] {
        let one = top(&state, query, caller);
        assert_eq!(one["results"][0]["name"], "agent_inbox", "{query}: {one}");
        assert_eq!(one["top_schema"]["name"], "agent_inbox", "{query}: {one}");
    }
    let one = top(&state, "select:agent_inbox", caller);
    assert!(
        one["results"][0]["summary"]
            .as_str()
            .unwrap()
            .starts_with("inbox:"),
        "summary must describe the action: {one}"
    );
}

/// Catches: a discovered action schema carrying fields outside the MCP Tool object.
/// Strict clients (ego) refuse it with "unknown field", then fall back to the
/// whole definition after failed calls: the discovery E2E measured that regression.
#[test]
fn every_action_slice_is_a_strict_mcp_tool_object() {
    let allowed = [
        "name",
        "title",
        "description",
        "inputSchema",
        "outputSchema",
        "annotations",
        "_meta",
    ];
    let mut failures = Vec::new();
    for tool in SLICED {
        let definition = advertised(tool);
        for action in definition["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
        {
            let action = action.as_str().unwrap();
            let slice = slice(tool, action);
            for key in slice.as_object().unwrap().keys() {
                if !allowed.contains(&key.as_str()) {
                    failures.push(format!("{tool}.{action}: top-level field {key}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Catches: two actions of one tool discovered under one name. Clients cache an
/// admitted schema by tool name, so the second action was validated against the
/// first one's schema ("send" is not one of "register") and the call failed.
#[test]
fn every_action_has_a_name_of_its_own() {
    let mut seen = std::collections::HashSet::new();
    let natives: Vec<String> = native_tool_definitions()
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    for tool in SLICED {
        let definition = advertised(tool);
        for action in definition["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
        {
            let name = slice(tool, action.as_str().unwrap())["name"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(!natives.contains(&name), "{name} shadows a native tool");
            assert!(seen.insert(name.clone()), "{name} is not unique");
        }
    }
}

/// Catches: an action's discovery name that `call_tool` cannot dispatch, or one
/// that silently runs a different action than its name says.
#[tokio::test]
async fn call_tool_dispatches_an_action_name_to_its_tool() {
    let state = test_state();
    let listed = handle_call_tool(
        &state,
        loopback_addr().into(),
        &serde_json::json!({"tool_name": "session_list", "arguments": {}}),
        None,
        None,
    )
    .await;
    assert!(listed.get("error").is_none(), "{listed}");
    let conflict = handle_call_tool(
        &state,
        loopback_addr().into(),
        &serde_json::json!({"tool_name": "session_list", "arguments": {"action": "kill"}}),
        None,
        None,
    )
    .await;
    assert!(
        conflict["error"].as_str().unwrap().contains("conflicts"),
        "{conflict}"
    );
}

/// Catches: a search that ships only the top schema, so an action with a
/// prerequisite (`agent_send` needs `agent_register` for an external caller) costs
/// a second discovery round trip, which the discovery E2E measured on `mail`.
#[test]
fn search_ships_prerequisites_and_next_actions_within_the_cap() {
    let state = test_state();
    let caller = every_tool_caller(&state);
    rebuild_tool_search_index(&state);
    let response = top(&state, "send message to peer agent", caller);
    assert_eq!(response["top_schema"]["name"], "agent_send", "{response}");
    let schemas = response["schemas"].as_array().expect("related schemas");
    let names: Vec<&str> = schemas
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.first(), Some(&"agent_register"), "{names:?}");
    assert!(
        !names.contains(&"agent_send"),
        "top schema repeated: {names:?}"
    );
    let bytes: usize = schemas
        .iter()
        .map(|s| serde_json::to_vec(s).unwrap().len())
        .sum();
    assert!(bytes <= 8 * 1024, "{bytes} bytes");

    let none = handle_search_tools_for_session(
        &state,
        &serde_json::json!({"query": "send message to peer agent", "schema_byte_budget": 0}),
        caller,
    );
    assert!(
        none.get("schemas").is_none() && none.get("top_schema").is_none(),
        "{none}"
    );
}
