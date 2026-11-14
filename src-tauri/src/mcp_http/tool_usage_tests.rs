// --- workflow and automations listed by state (story 2807) ---

const USAGE_ORCHESTRATOR: &str = "550e8400-e29b-41d4-a716-446655442807";
const USAGE_ORCHESTRATOR_MCP: &str = "mcp-usage-orchestrator";

fn listed_tool_names(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> Vec<String> {
    merged_tool_definitions_for_mode(state, mcp_session_id, false)
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

fn usage_for(project: &str) -> crate::mcp_http::tool_usage::ToolUsage {
    crate::mcp_http::tool_usage::ToolUsage {
        workflow_projects: [project.to_string()].into(),
        automations: false,
    }
}

// Catches: workflow and automations listed in a repository with no definition,
// no run and no automation, so every session can load the workflow schema.
#[test]
fn workflow_is_not_listed_without_a_definition_or_run() {
    let state = test_state();
    state.config.write().mcp_capabilities.denied.clear();
    *state.mcp.tool_usage.write() = usage_for("/another/project");

    let names = listed_tool_names(&state, None);
    assert!(!names.iter().any(|name| name == "workflow"), "{names:?}");
    assert!(!names.iter().any(|name| name == "automations"), "{names:?}");
    assert!(names.iter().any(|name| name == "story"), "{names:?}");

    let search = handle_search_tools_for_session(
        &state,
        &serde_json::json!({"query": "workflow run definition launch"}),
        None,
    );
    let found: Vec<&str> = search["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert!(!found.contains(&"workflow"), "{search}");
    let schema = handle_get_tool_schema_for_session(
        &state,
        &serde_json::json!({"tool_name": "workflow"}),
        None,
    );
    assert!(schema.get("error").is_some(), "{schema}");

    let instructions = build_mcp_instructions_for_session(&state, None, false, None);
    assert!(
        instructions.contains(crate::mcp_http::tool_usage::HIDDEN_LINE),
        "{instructions}"
    );
}

// Catches: the coordinator loses workflow and automations while no repository
// uses them, so it cannot start the first run; or its registration as
// orchestrator never signals the changed catalogue.
#[test]
fn an_orchestrator_lists_workflow_and_automations_without_usage() {
    let state = test_state();
    state.config.write().mcp_capabilities.denied.clear();
    let mut changes = state.mcp.tools_changed.subscribe();
    handle_messaging(
        &state,
        &serde_json::json!({
            "action": "register", "tuic_session": USAGE_ORCHESTRATOR,
            "name": "orchestrator", "orchestrator": true
        }),
        Some(USAGE_ORCHESTRATOR_MCP),
    );
    assert!(
        changes.try_recv().is_ok(),
        "the orchestrator role sent no list change"
    );

    let names = listed_tool_names(&state, Some(USAGE_ORCHESTRATOR_MCP));
    assert!(names.iter().any(|name| name == "workflow"), "{names:?}");
    assert!(names.iter().any(|name| name == "automations"), "{names:?}");
    let instructions =
        build_mcp_instructions_for_session(&state, None, false, Some(USAGE_ORCHESTRATOR_MCP));
    assert!(!instructions.contains(crate::mcp_http::tool_usage::HIDDEN_LINE));
}

// Catches: a definition in one project lists workflow in every project.
#[test]
fn workflow_usage_is_scoped_to_the_callers_project() {
    let usage = usage_for("/repo/a");
    assert!(usage.lists("workflow", Some("/repo/a"), false));
    assert!(!usage.lists("workflow", Some("/repo/b"), false));
    assert!(!usage.lists("workflow", None, false));
    assert!(!usage.lists("automations", Some("/repo/a"), false));
    let automations = crate::mcp_http::tool_usage::ToolUsage {
        automations: true,
        ..Default::default()
    };
    assert!(automations.lists("automations", None, false));
    assert!(usage.lists("session", None, false));
}

// Catches: every refresh re-sends tools/list_changed, or a change never sends it.
#[test]
fn refresh_signals_a_catalogue_change_only_when_usage_moved() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let state = test_state();
    let mut changes = state.mcp.tools_changed.subscribe();

    crate::mcp_http::tool_usage::refresh(&state);
    assert!(changes.try_recv().is_err(), "unchanged usage signalled");

    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let project = project.path().to_str().unwrap();
    create_neutral_draft(project);
    crate::mcp_http::tool_usage::refresh(&state);
    assert!(
        changes.try_recv().is_ok(),
        "a new definition sent no list change"
    );
    crate::mcp_http::tool_usage::refresh(&state);
    assert!(
        changes.try_recv().is_err(),
        "a second refresh signalled again"
    );
}

/// One user-made neutral draft in `project`.
fn create_neutral_draft(project: &str) {
    let definition = serde_json::from_value(serde_json::json!({
        "control": {
            "nodes": {"start": {"type": "start"}, "end": {"type": "end", "outcome": "completed"}},
            "edges": {"start": {"next": "end"}}
        },
        "inputs": {"fields": {}}, "outputs": {"fields": {}}, "outcomes": ["completed"],
        "subject": null, "instances": [], "input_bindings": {}, "conditions": {},
        "collections": {}, "loops": {}, "forks": {}, "output_bindings": {}, "subflows": []
    }))
    .unwrap();
    crate::workflows::WorkflowStore::open()
        .unwrap()
        .create_neutral_draft(
            project,
            "My delivery".into(),
            definition,
            Default::default(),
        )
        .unwrap();
}

// Catches: a fresh store counting as workflow usage, or a user-made neutral draft
// not making its project list `workflow`.
#[test]
fn a_neutral_draft_makes_its_project_use_workflow() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let project = project.path().to_str().unwrap();
    let usage = crate::mcp_http::tool_usage::read_usage().unwrap();
    assert!(usage.workflow_projects.is_empty(), "{usage:?}");
    assert!(!usage.automations);
    create_neutral_draft(project);
    let usage = crate::mcp_http::tool_usage::read_usage().unwrap();
    assert!(usage.workflow_projects.contains(project), "{usage:?}");
}
