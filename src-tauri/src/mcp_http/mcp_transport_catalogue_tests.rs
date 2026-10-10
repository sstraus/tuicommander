// Catches: Settings rows reuse long MCP paragraphs or omit newly registered tools.
#[test]
fn native_tool_catalog_summaries_are_short_and_not_description_prefixes() {
    let definitions = native_tool_definitions();
    let tools = definitions.as_array().unwrap();
    let catalog = native_tool_catalog();
    assert!(!tools.is_empty());
    assert_eq!(catalog.len(), tools.len());
    assert_eq!(NATIVE_TOOL_SUMMARIES.len(), tools.len());
    for (tool, entry) in tools.iter().zip(&catalog) {
        let name = tool["name"].as_str().unwrap();
        let description = tool["description"].as_str().unwrap();
        let summary = entry["summary"].as_str().unwrap();
        assert_eq!(entry["name"], tool["name"]);
        assert_eq!(entry["description"], tool["description"]);
        assert!(!summary.trim().is_empty(), "{name} needs a summary");
        assert!(summary.chars().count() <= 70, "{name} summary is too long");
        assert!(
            !summary.contains('\n'),
            "{name} summary must be one sentence"
        );
        assert!(
            !description.starts_with(summary),
            "{name} summary must not reuse its MCP description"
        );
        assert!(
            tool.get("summary").is_none(),
            "{name} leaks app metadata into MCP"
        );
        assert_eq!(
            NATIVE_TOOL_SUMMARIES
                .iter()
                .filter(|(key, _)| *key == name)
                .count(),
            1,
            "{name} must have exactly one declared summary"
        );
    }
}

/// Catches: the answers view stays empty for agents outside the orchestrator
/// repo because TUIC never teaches the 💬 marker, even with the intent and
/// suggest markers switched off.
#[test]
fn mcp_instructions_teach_the_answer_marker_regardless_of_other_markers() {
    let state = test_state();
    {
        let mut cfg = state.config.write();
        cfg.intent_tab_title = false;
        cfg.suggest_followups = false;
    }
    for collapse in [false, true] {
        let instructions = build_mcp_instructions_for_mode(&state, None, collapse);
        assert!(
            instructions.contains(
                "- `💬 ` — prefix every sentence that directly answers the user's question"
            )
        );
    }
}

#[test]
fn mcp_instructions_request_current_intent_at_task_start_and_phase_changes() {
    let state = test_state();
    let instructions = build_mcp_instructions_for_mode(&state, None, true);

    assert!(instructions.contains("at the start of every user task"));
    assert!(instructions.contains("on each material work-phase change"));
    assert!(instructions.contains("currently in progress in present tense"));

    state.config.write().intent_tab_title = false;
    let disabled = build_mcp_instructions_for_mode(&state, None, true);
    assert!(!disabled.contains("`intent: <desc> (<title>)`"));
}

// --- the voice tool (817-f67c) ---

fn native_tool_named(name: &str) -> serde_json::Value {
    native_tool_definitions()
        .as_array()
        .expect("the definitions are an array")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("no native tool named {name}"))
        .clone()
}

/// Discovery must work on the surface the client is already looking at.
///
/// `notifications/tools/list_changed` is not a mechanism we can rely on —
/// Claude Code does not refetch on it — so arming cannot be what makes the
/// tool appear. It is always in the list, always in the search corpus and
/// always fetchable by name; whether it can *do* anything is an answer the
/// tool itself gives.
#[tokio::test]
async fn voice_is_discoverable_by_list_search_and_name_without_a_list_change() {
    let state = test_state();
    spawn_tool_search_index_updater(Arc::clone(&state));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let definition = native_tool_named("voice");
    assert_eq!(
        definition["inputSchema"]["required"],
        serde_json::json!(["action"])
    );
    for action in ["speak", "stop", "status"] {
        assert!(
            definition["inputSchema"]["properties"]["action"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains(action),
            "the action list must name {action}"
        );
    }

    let found = handle_search_tools(
        &state,
        &serde_json::json!({"query": "speak a reply out loud to the user", "limit": 5}),
    );
    let names: Vec<&str> = found["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert!(
        names.contains(&"voice"),
        "a model describing what it wants must find the tool: {names:?}"
    );

    let schema = handle_get_tool_schema(&state, &serde_json::json!({"tool_name": "voice"}));
    assert_eq!(schema["inputSchema"], definition["inputSchema"]);
}

/// Per docs/sync-matrix.md a tool-surface change must land in the listing AND
/// the search corpus — a tool missing from the latter is invisible under
/// `collapse_tools` and to lazy discovery.
#[test]
fn the_task_tool_is_listed_and_searchable() {
    let state = test_state();

    let listed = merged_tool_definitions(&state, None, None);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "task"),
        "task must appear in tools/list"
    );
    assert!(
        searchable_tool_definitions(&state)
            .iter()
            .any(|t| t["name"] == "task"),
        "task must appear in the search corpus"
    );
}

#[test]
fn meta_tool_definitions_returns_exactly_three_tools_with_expected_names() {
    let state = test_state();
    let defs = meta_tool_definitions(&state);
    let names = tool_names(&defs);
    assert_eq!(names.len(), 3, "meta_tool_definitions must return 3 tools");
    assert_eq!(names, vec!["search_tools", "get_tool_schema", "call_tool"]);
    // Each must have a non-empty description and an inputSchema object.
    for tool in defs.as_array().unwrap() {
        assert!(
            tool["description"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "meta tool {:?} missing description",
            tool["name"]
        );
        assert!(
            tool["inputSchema"].is_object(),
            "meta tool {:?} missing inputSchema",
            tool["name"]
        );
    }
}

#[test]
fn meta_tool_names_constant_matches_definitions() {
    let state = test_state();
    let defs = meta_tool_definitions(&state);
    let names = tool_names(&defs);
    let expected: Vec<String> = META_TOOL_NAMES.iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected);
}

/// Criterion 2 of story 789-f6ed: exactly one tool family is registered.
///
/// This list used to carry 13 `ai_terminal_*` tools after `debug`. They
/// overlapped this family without being equivalent, and six of them needed
/// a filesystem sandbox only the embedded agent loop creates - so they
/// refused every external caller before dispatch. The comparison behind the
/// deletion is `plans/ego-integration/tool-family-comparison.md`.
///
/// These names are a public contract: they appear in users' ego rule files,
/// so renaming one silently stops a user's policy from matching.
#[tokio::test]
async fn native_tool_definitions_are_the_one_surviving_family() {
    let state = test_state();
    {
        let mut config = state.config.write();
        config.disabled_native_tools.clear();
        config.collapse_tools = false;
    }
    let listed = tools_list_result(
        &state,
        HeaderMap::new(),
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    )
    .await;
    let names = tool_names(&listed["result"]["tools"]);
    assert_eq!(
        names,
        vec![
            "automations",
            "secret",
            "telegram",
            "session",
            "agent",
            "task",
            "remote",
            "repo",
            "story",
            "workflow_run",
            "workflow_story_create",
            "workflow_report",
            "workflow_launch",
            "progress",
            "ui",
            "plugin_dev_guide",
            "config",
            "debug",
            "voice",
        ],
        "native_tool_definitions must return exactly the one family, in order"
    );
    let called = handle_mcp_tool_call(
        &state,
        loopback_addr(),
        "telegram",
        &serde_json::json!({"action":"send","text":"hello"}),
        None,
    )
    .await;
    assert_eq!(
        called["error"], "telegram_invalid_state",
        "unbound Telegram invocation must reach its native authority check"
    );
}

/// Claude Code defers MCP tools behind ToolSearch: the model sees only the
/// name and must load the schema first, which in practice it never does
/// for `progress` — so no Claude terminal ever reported done/blocked while
/// Codex, which loads every tool, did. `anthropic/alwaysLoad` exempts the
/// tool from deferral. Only `progress` carries it: it is the one tool the
/// protocol makes mandatory, and every always-loaded schema costs tokens.
#[test]
fn progress_is_the_only_tool_claude_code_must_not_defer() {
    let defs = native_tool_definitions();
    let always_loaded: Vec<&str> = defs
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["_meta"]["anthropic/alwaysLoad"] == serde_json::json!(true))
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(always_loaded, vec!["progress"]);
}

/// What ego actually receives, rather than only which flag was computed.
///
/// The collapsed surface is three meta-tools plus `progress`, which stays
/// directly callable because reporting must not require a discovery call
/// first. Everything else is reachable through `call_tool`, so this is a
/// smaller list and not a smaller capability.
#[test]
fn the_collapsed_surface_is_the_meta_tools_plus_progress() {
    let state = test_state();

    let collapsed = tool_names(&merged_tool_definitions_for_mode(&state, None, true));
    assert_eq!(
        collapsed,
        ["search_tools", "get_tool_schema", "call_tool", "progress"]
            .map(String::from)
            .to_vec(),
        "ego must get the discovery surface, not the catalogue"
    );

    let full = tool_names(&merged_tool_definitions_for_mode(&state, None, false));
    assert!(
        full.len() > collapsed.len(),
        "the uncollapsed surface must be the larger one, or this test proves \
             nothing about the saving: {full:?}"
    );
    assert!(
        full.iter().any(|name| name == "session"),
        "the uncollapsed surface is the native catalogue: {full:?}"
    );
}

#[tokio::test]
async fn disabled_progress_is_rejected_by_direct_and_collapsed_dispatch() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["progress".to_string()];
    rebuild_tool_search_index(&state);
    let args = serde_json::json!({"type":"done", "text":"Must not persist."});

    let direct = handle_mcp_tool_call(&state, loopback_addr(), "progress", &args, None).await;
    assert!(direct["error"].as_str().unwrap().contains("disabled"));
    let collapsed = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({"tool_name":"progress", "arguments": args}),
        None,
        None,
    )
    .await;
    assert!(collapsed["error"].as_str().unwrap().contains("disabled"));
    assert!(
        !tool_names(&merged_tool_definitions_for_mode(&state, None, true))
            .contains(&"progress".to_string())
    );
}

#[test]
fn session_description_mentions_tmux_pane_semantics() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session")
        .unwrap();
    let desc = session["description"].as_str().unwrap();
    assert!(
        desc.contains("tmux"),
        "session description must reference tmux for discoverability"
    );
    assert!(
        desc.contains("send-keys") || desc.contains("send_keys"),
        "session description must mention send-keys equivalent"
    );
    assert!(
        desc.contains("capture-pane") || desc.contains("capture_pane"),
        "session description must mention capture-pane equivalent"
    );
}

#[test]
fn agent_tool_includes_messaging_actions() {
    let defs = native_tool_definitions();
    let agent = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let action_desc = agent["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &["register", "list_peers", "send", "inbox", "wait"] {
        assert!(
            action_desc.contains(action),
            "agent action description must include '{action}'"
        );
    }
    assert!(
        agent["inputSchema"]["properties"]["args"]["description"]
            .as_str()
            .unwrap()
            .contains("prevent_alt_screen"),
        "spawn schema must name the per-agent screen setting"
    );
    assert!(
        agent["inputSchema"]["properties"]["limit"].is_object(),
        "inbox accepts limit, so the MCP schema must advertise it"
    );
}

#[test]
fn agent_tool_description_carries_orchestration_crash_course() {
    // Tool semantics belong in descriptions, available when discovered.
    // Initial model visibility of descriptions and initialize instructions
    // depends on the harness (see docs/backend/mcp-http.md). The primer
    // and wait/send delivery semantics must live here.
    let defs = native_tool_definitions();
    let agent = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let desc = agent["description"].as_str().unwrap();
    assert!(
        desc.contains("Managed PTYs auto-bind"),
        "must state the condition for auto-identity"
    );
    assert!(
        desc.contains("headerless external caller"),
        "must explain how external callers establish identity"
    );
    assert!(desc.contains("wait"), "must mention the wait primitive");
    assert!(
        desc.contains("the payload is never typed into the recipient's composer"),
        "must state the mail-stays-mail invariant: a send is never keystrokes"
    );
    assert!(
        desc.contains("generic `agent action=inbox` wake"),
        "must explain payload-free wake delivery"
    );
    assert!(
        desc.contains("do NOT poll"),
        "must discourage polling loops"
    );
    assert!(desc.contains("state only"));
    assert!(desc.contains("must report task output"));
}

#[test]
fn session_tool_description_includes_wait() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session")
        .unwrap();
    let action_desc = session["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(action_desc.contains("wait"), "session must advertise wait");
    assert!(
        session["inputSchema"]["properties"]["until"].is_object(),
        "session wait needs an 'until' param"
    );
}

#[test]
fn repo_tool_lists_retained_worktree_actions_and_branch_parameter() {
    let defs = native_tool_definitions();
    let repo = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "repo")
        .unwrap();
    let action_desc = repo["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &[
        "list",
        "active",
        "status",
        "worktree_list",
        "worktree_create",
        "worktree_remove",
    ] {
        assert!(
            action_desc.contains(action),
            "repo action description must include '{action}'"
        );
    }
    let params = &repo["inputSchema"]["properties"];
    assert!(params.get("issue_number").is_none());
    assert!(params.get("filter").is_none());
    assert!(
        params["branch"]["description"]
            .as_str()
            .unwrap()
            .contains("worktree_remove"),
        "branch's description must name worktree_remove as a consumer: {params}"
    );
}

#[test]
fn repo_progress_schema_advertises_paging_and_filters() {
    let defs = native_tool_definitions();
    let repo = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "repo")
        .unwrap();
    let input = &repo["inputSchema"]["properties"]["input"];
    assert_eq!(input["additionalProperties"], false);
    for field in ["blockedOnly", "ptyId", "limit", "cursor"] {
        assert!(
            input["properties"][field].is_object(),
            "missing {field} in repo progress input"
        );
    }
    assert!(input["description"].as_str().unwrap().contains("8 entries"));
}

#[test]
fn ui_tool_includes_notify_actions() {
    let defs = native_tool_definitions();
    let ui = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ui")
        .unwrap();
    let action_desc = ui["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    for action in &["tab", "toast", "confirm"] {
        assert!(
            action_desc.contains(action),
            "ui action description must include '{action}'"
        );
    }
}

#[test]
fn debug_tool_includes_sessions_action() {
    let defs = native_tool_definitions();
    let debug = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "debug")
        .unwrap();
    let action_desc = debug["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(
        action_desc.contains("sessions"),
        "debug action description must include 'sessions'"
    );
}

#[test]
fn merged_tools_collapse_false_returns_all_native_tools() {
    let state = test_state();
    assert!(!state.config.read().collapse_tools);

    let merged = merged_tool_definitions(&state, None, None);
    let names = tool_names(&merged);

    let native = tool_names(&native_tool_definitions());
    assert_eq!(
        names, native,
        "collapse_tools=false should return all native tools"
    );
    assert!(
        names.len() > 3,
        "baseline native tool set must exceed 3 tools"
    );
}

#[test]
fn merged_tools_collapse_true_keeps_progress_directly_available() {
    let state = test_state();
    state.config.write().collapse_tools = true;

    let merged = merged_tool_definitions(&state, None, None);
    let names = tool_names(&merged);

    assert_eq!(names.len(), 4);
    assert_eq!(
        names,
        vec!["search_tools", "get_tool_schema", "call_tool", "progress"]
    );
}

#[test]
fn grok_session_uses_meta_tools_without_mutating_global_config() {
    let state = test_state();
    assert!(!state.config.read().collapse_tools);
    state.mcp.sessions.insert(
        "grok-session".to_string(),
        crate::state::McpSessionMeta {
            prompt_instructions: None,
            last_activity: std::time::Instant::now(),
            is_claude_code: false,
            requires_meta_tools: true,
            has_sse_stream: false,
            sse_generation: 0,
            repo_path: None,
        },
    );

    let merged = merged_tool_definitions(&state, Some("grok-session"), None);
    assert_eq!(
        tool_names(&merged),
        vec!["search_tools", "get_tool_schema", "call_tool", "progress"]
    );
    assert!(!state.config.read().collapse_tools);
}

/// Sanity check on the token-reduction claim for lazy tool loading.
/// Measured on the native-only test state (no upstreams registered):
/// baseline ≈ 11 KiB, collapsed ≈ 1.7 KiB — roughly 6.7× reduction.
/// In production with typical upstreams (100+ tools) the baseline is
/// ~35 KiB, pushing the real reduction toward ~20×. Thresholds here
/// are regression guards, not targets, so they use the conservative
/// native-only numbers.
#[test]
fn collapse_tools_payload_size_meets_reduction_target() {
    let state = test_state();

    let baseline = serde_json::to_vec(&merged_tool_definitions(&state, None, None))
        .expect("serialize baseline")
        .len();

    state.config.write().collapse_tools = true;
    let collapsed = serde_json::to_vec(&merged_tool_definitions(&state, None, None))
        .expect("serialize collapsed")
        .len();

    assert!(
        collapsed < 4096,
        "collapsed tools/list must stay under 4 KiB, got {collapsed} bytes"
    );
    assert!(
        baseline >= collapsed * 5,
        "expected >=5x reduction on native-only state, baseline={baseline} collapsed={collapsed}"
    );
}

#[test]
fn merged_tools_collapse_true_hides_disabled_progress() {
    let state = test_state();
    state.config.write().collapse_tools = true;
    state.config.write().disabled_native_tools = vec!["progress".to_string()];

    let merged = merged_tool_definitions(&state, None, None);
    assert_eq!(tool_names(&merged).len(), 3);
    assert_eq!(
        tool_names(&merged),
        vec!["search_tools", "get_tool_schema", "call_tool"]
    );
}

/// The obligation and the tool are one switch, not two. A listed tool with
/// no stated duty is the shipped defect (39 repositories, zero entries); a
/// duty naming a tool the client cannot see is worse, because the agent
/// keeps trying. Both directions are asserted here so neither can drift.
#[test]
fn the_progress_obligation_and_the_progress_tool_appear_and_vanish_together() {
    let ctx = InstructionContext {
        repos: Vec::new(),
        sessions: Vec::new(),
        peer_count: 0,
    };
    let markers = (true, true);

    let on = render_mcp_instructions(None, false, markers, true, &ctx);
    let off = render_mcp_instructions(None, false, markers, false, &ctx);
    assert!(on.contains("## Progress — mandatory"));
    assert!(on.contains("Call `progress`"));
    // The word itself still occurs — the `intent:` marker is described as
    // "the work currently in progress" — so the assertion is on the
    // obligation and on naming the tool, not on the substring.
    assert!(!off.contains("## Progress — mandatory"));
    assert!(
        !off.contains("`progress`"),
        "an obligation must never name a tool the client was not listed"
    );

    let state = test_state();
    let listed = |state: &Arc<AppState>| {
        tool_names(&serde_json::Value::Array(filtered_native_tools(state)))
            .contains(&"progress".to_string())
    };

    state.config.write().progress_tracking = true;
    assert!(listed(&state));

    state.config.write().progress_tracking = false;
    assert!(!listed(&state));
}

// search_tools

#[test]
fn search_tools_requires_query() {
    let state = test_state();
    let r = handle_search_tools(&state, &serde_json::json!({}));
    assert!(r["error"].as_str().unwrap().contains("query"));

    let r = handle_search_tools(&state, &serde_json::json!({ "query": "" }));
    assert!(r["error"].as_str().unwrap().contains("query"));

    let r = handle_search_tools(&state, &serde_json::json!({ "query": "   " }));
    assert!(r["error"].as_str().unwrap().contains("query"));
}

#[test]
fn search_tools_returns_ranked_results_for_session_query() {
    let state = test_state();
    // Query targets the PTY multiplexer specifically, so the ranking is
    // asserted against a phrase only `session` describes.
    let r = handle_search_tools(
        &state,
        &serde_json::json!({ "query": "PTY multiplexer tmux pane lifecycle" }),
    );
    let results = r["results"].as_array().unwrap();
    assert!(!results.is_empty(), "expected non-empty results");
    assert_eq!(results[0]["name"], "session");
    // summary is the first sentence of the description — must be populated.
    assert!(
        results[0]["summary"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    );
}

#[test]
fn search_tools_returns_ranked_results_for_github_query() {
    let state = test_state();
    let r = handle_search_tools(&state, &serde_json::json!({ "query": "github PR status" }));
    let results = r["results"].as_array().unwrap();
    assert_eq!(results[0]["name"], "repo");
}

#[test]
fn search_tools_excludes_disabled_native_tools() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    rebuild_tool_search_index(&state);

    let r = handle_search_tools(&state, &serde_json::json!({ "query": "terminal session" }));
    let results = r["results"].as_array().unwrap();
    // "session" must not appear at all.
    let has_session = results.iter().any(|v| v["name"] == "session");
    assert!(
        !has_session,
        "disabled 'session' tool must be absent from search results"
    );
}

#[test]
fn search_tools_nonsense_query_returns_empty() {
    let state = test_state();
    let r = handle_search_tools(
        &state,
        &serde_json::json!({ "query": "xyzzyplugh nonsense qqq" }),
    );
    let results = r["results"].as_array().unwrap();
    assert_eq!(results.len(), 0);
    assert_eq!(r["count"], 0);
}

#[test]
fn search_tools_respects_limit() {
    let state = test_state();
    let r = handle_search_tools(
        &state,
        &serde_json::json!({ "query": "action", "limit": 2 }),
    );
    let results = r["results"].as_array().unwrap();
    assert!(results.len() <= 2);
}

// get_tool_schema

#[test]
fn get_tool_schema_requires_tool_name() {
    let state = test_state();
    let r = handle_get_tool_schema(&state, &serde_json::json!({}));
    assert!(r["error"].as_str().unwrap().contains("tool_name"));
}

#[test]
fn get_tool_schema_returns_full_definition_for_native_tool() {
    let state = test_state();
    let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "session" }));
    assert_eq!(r["name"], "session");
    assert!(r["description"].as_str().is_some());
    assert!(r["inputSchema"].is_object());
    assert_eq!(r["inputSchema"]["type"], "object");
}

#[test]
fn get_tool_schema_returns_error_for_unknown_tool() {
    let state = test_state();
    let r = handle_get_tool_schema(
        &state,
        &serde_json::json!({ "tool_name": "does_not_exist" }),
    );
    let err = r["error"].as_str().unwrap();
    assert!(err.contains("not found"));
    assert!(
        err.contains("search_tools"),
        "error should guide user to search_tools"
    );
}

#[test]
fn get_tool_schema_excludes_disabled_native_tools() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["debug".to_string()];
    rebuild_tool_search_index(&state);
    let r = handle_get_tool_schema(&state, &serde_json::json!({ "tool_name": "debug" }));
    assert!(r["error"].as_str().is_some());
}

// call_tool

#[tokio::test]
async fn call_tool_requires_tool_name() {
    let state = test_state();
    let r = handle_call_tool(&state, loopback_addr(), &serde_json::json!({}), None, None).await;
    assert!(r["error"].as_str().unwrap().contains("tool_name"));
}

#[tokio::test]
async fn call_tool_blocks_meta_tool_recursion() {
    let state = test_state();
    for meta in META_TOOL_NAMES {
        let r = handle_call_tool(
            &state,
            loopback_addr(),
            &serde_json::json!({ "tool_name": meta, "arguments": { "query": "x" } }),
            None,
            None,
        )
        .await;
        let err = r["error"].as_str().unwrap();
        assert!(
            err.contains("cannot invoke meta-tool"),
            "meta '{meta}' should be blocked: {err}"
        );
    }
}

#[tokio::test]
async fn call_tool_rejects_disabled_native_tool() {
    let state = test_state();
    state.config.write().disabled_native_tools = vec!["repo".to_string()];
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "repo", "arguments": { "action": "active" } }),
        None,
        None,
    )
    .await;
    assert!(r["error"].as_str().unwrap().contains("disabled"));
}

#[tokio::test]
async fn call_tool_returns_unknown_tool_error_for_bogus_name() {
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "nonsense_xyz", "arguments": {} }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    assert!(err.contains("Unknown tool"));
}

#[tokio::test]
async fn call_tool_propagates_addr_for_localhost_only_tools() {
    // config save is restricted to loopback addresses. call_tool must propagate
    // the caller's addr so the restriction still fires through the meta layer.
    let state = test_state();
    let r = handle_call_tool(
        &state,
        non_loopback_addr(),
        &serde_json::json!({
            "tool_name": "config",
            "arguments": { "action": "save", "config": {} }
        }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    assert!(
        err.contains("localhost"),
        "non-loopback config save must be rejected via addr propagation: {err}"
    );
}

#[tokio::test]
async fn call_tool_missing_arguments_defaults_to_empty_object() {
    // Omitting 'arguments' must not crash — handler receives {} and produces
    // its own missing-action error.
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "session" }),
        None,
        None,
    )
    .await;
    assert!(r["error"].as_str().unwrap().contains("action"));
}

#[tokio::test]
async fn call_tool_routes_unknown_upstream_prefixed_name_through_proxy() {
    // No upstreams are registered in tests — any tool_name with "__" falls
    // through to proxy_tool_call, which errors out. We just verify that the
    // error comes from the upstream path (not the native unknown-tool branch).
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "fake_upstream__some_tool", "arguments": {} }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    // proxy_tool_call returns an error string — just assert it's an error and
    // that the native unknown-tool message is NOT what we got.
    assert!(
        !err.contains("Unknown tool"),
        "upstream-prefixed name must not hit native fallthrough: {err}"
    );
}

// ---- build_mcp_instructions collapse mode (story 1081) -------------------

/// Classic mode keeps a `## Tools` section, but it holds cross-tool rules
/// rather than a catalogue — `tools/list` already carries every name and
/// action in the same turn. It must also stay free of the meta-tool names,
/// which are not callable in this mode.
#[test]
fn instructions_collapse_off_points_at_tool_descriptions() {
    let state = test_state();
    let out = build_mcp_instructions(&state, None);
    assert!(out.contains("## Tools\n"), "expected classic Tools section");
    assert!(
        out.contains("Read each tool's description for actions and rules."),
        "classic mode must delegate to the tool descriptions"
    );
    assert!(!out.contains("## Tools — Lazy Discovery"));
    assert!(!out.contains("search_tools"));
}

/// The ack rule is protocol and belongs to the instructions: no tool
/// description can state a rule about the *first assistant message*.
///
/// Retargeted for #754-affa: the swarm/lifecycle/reporting assertions that
/// used to sit here moved to `instructions_do_not_repeat_what_tool_descriptions_already_say`,
/// which asserts them on the `agent` description — the surface that reaches
/// clients ignoring `instructions` too.
#[test]
fn instructions_scope_ack_to_the_connection() {
    let state = test_state();
    let out = build_mcp_instructions(&state, None);
    assert!(out.contains("exactly once per MCP connection or reconnect"));
    assert!(out.contains("Never repeat it on each conversational turn"));
    assert!(!out.contains("Aliases \"swarm\""));
}

#[test]
fn instructions_collapse_on_describes_search_schema_call_flow() {
    let state = test_state();
    state.config.write().collapse_tools = true;
    let out = build_mcp_instructions(&state, None);

    // Slim section referencing meta-tools (detail lives in tool descriptions).
    assert!(out.contains("## Tools"), "expected tools header");
    assert!(out.contains("`search_tools`"), "must mention search_tools");
    assert!(
        out.contains("`get_tool_schema`"),
        "must mention get_tool_schema"
    );
    assert!(out.contains("`call_tool`"), "must mention call_tool");
    assert!(out.contains("worktree"), "must mention worktree caveat");
    // The concrete tools list and legacy workflow must NOT appear — those
    // reference tool names the model cannot invoke directly in collapse mode.
    assert!(
        !out.contains("- `session` ("),
        "tools list must be suppressed in collapse mode"
    );
    assert!(
        !out.contains("## Workflow"),
        "legacy workflow must be suppressed in collapse mode"
    );
}

#[test]
fn initialize_instructions_pin_the_one_call_submit_rule_in_both_modes() {
    let state = test_state();
    let classic = build_mcp_instructions_for_mode(&state, None, false);
    let collapsed = build_mcp_instructions_for_mode(&state, None, true);

    assert!(classic.contains(
            "**Submit:** `session action=submit session_id=<id> input=<text>` once; never split text/Enter; never poll."
        ));
    assert!(collapsed.contains(
            "**Submit:** `call_tool tool_name=session arguments={action:submit,session_id,input}` once; never split text/Enter; never poll."
        ));
    assert!(!classic.contains("text then"));
    assert!(!collapsed.contains("status after"));
}

/// There are two instruction surfaces and they are not equals. Every client
/// receives the tool descriptions; a client such as Codex never surfaces
/// initialize `instructions`. So a rule that a tool description already
/// carries must not be repeated in the instructions — the client that reads
/// both pays for it twice, and the client that reads one must not be the one
/// left without it.
#[test]
fn instructions_do_not_repeat_what_tool_descriptions_already_say() {
    let state = test_state();
    let classic = build_mcp_instructions_for_mode(&state, None, false);
    let defs = native_tool_definitions();

    // 1. The tool catalogue is `tools/list` restated in prose.
    for bullet in [
        "- `session` (",
        "- `agent` (",
        "- `task` (",
        "- `repo` (",
        "- `ui` (",
        "- `plugin_dev_guide`:",
    ] {
        assert!(
            !classic.contains(bullet),
            "instructions must not restate the tool catalogue: {bullet}"
        );
    }

    // 2. The Workflow section restated the `agent` description's primer …
    assert!(!classic.contains("## Workflow"));
    let agent = tool_description(&defs, "agent");
    assert!(agent.contains("There is no separate swarm action"));
    assert!(agent.contains("Lifecycle notifications carry state only"));
    assert!(agent.contains("Every worker must report task output or blockers with send"));

    // 3. … and the UI-feedback line restated the `ui` description's Use: block.
    assert!(!classic.contains("**UI feedback:**"));
    let ui = tool_description(&defs, "ui");
    assert!(ui.contains("confirm BEFORE destructive ops"));
    assert!(ui.contains(">20-line structured output"));
    assert!(ui.contains("screenshot to visually verify"));
}

/// The orchestrator role, its wake capability and the identity rules used to
/// be instructions-only, which left them invisible to exactly the clients
/// that most need them (a headerless caller reading no instructions cannot
/// learn that `register` is how it gets an identity at all).
#[test]
fn agent_description_owns_the_orchestrator_role_and_wake_contract() {
    let defs = native_tool_definitions();
    let agent = tool_description(&defs, "agent");
    assert!(
        agent.contains("orchestrator=true"),
        "agent description must say how the role is declared"
    );
    assert!(
        agent.contains("spawning a child never infers it"),
        "agent description must deny role inference from spawn"
    );
    assert!(
        agent.contains("mail_wake=managed_pty_lifecycle"),
        "agent description must name the wake capability field"
    );
    assert!(
        agent.contains("headerless external caller calls register without tuic_session"),
        "agent description must keep the identity rule"
    );

    let state = test_state();
    let classic = build_mcp_instructions_for_mode(&state, None, false);
    assert!(
        !classic.contains("**Orchestrator role:**"),
        "the orchestrator role moved to the agent description; it must not be repeated"
    );
    assert!(
        !classic.contains("- **Identity:**"),
        "the identity rule moved to the agent description; it must not be repeated"
    );
}

#[test]
fn agent_send_schema_and_description_explain_urgent_mail_without_exposing_payload() {
    let definitions = native_tool_definitions();
    let agent = definitions
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["name"] == "agent")
        .unwrap();
    assert_eq!(
        agent["inputSchema"]["properties"]["urgency"]["enum"],
        serde_json::json!(["normal", "urgent"])
    );
    let description = agent["description"].as_str().unwrap();
    assert!(description.contains("Normal is the default"));
    assert!(description.contains("recipient must change course before its next step"));
    assert!(description.contains("payload is never typed into the recipient's composer"));
    assert!(description.contains("urgent_delivered"));
    assert!(description.contains("urgent_fallback_reason"));
}

/// A recorded project outcome and a transient notification are different
/// things. `progress` persists and toasts; `ui action=toast` only toasts.
/// The `ui` description is where a caller reaching for a toast finds out.
#[test]
fn ui_description_routes_semantic_outcomes_to_the_progress_tool() {
    let defs = native_tool_definitions();
    let ui = tool_description(&defs, "ui");
    assert!(
        ui.contains("`progress` tool"),
        "ui description must name the progress tool"
    );
    assert!(
        ui.contains("A project outcome"),
        "ui description must say which outcomes belong to progress"
    );
    assert!(
        ui.contains("keeps no record of it"),
        "ui description must say what a toast does not do"
    );
}

/// AC3: discovery must not be demanded for a definition the caller already
/// holds, and must not be skipped for a name it has never seen.
#[test]
fn meta_tools_do_not_demand_a_schema_the_caller_already_has() {
    let state = test_state();
    let defs = meta_tool_definitions(&state);

    let call = tool_description(&defs, "call_tool");
    assert!(
        call.contains("call it straight away with the schema you already have"),
        "call_tool must allow reusing a known schema"
    );
    assert!(
        call.contains("notifications/tools/list_changed"),
        "call_tool must name the one event that invalidates a known schema"
    );
    assert!(
        call.contains("Never skip discovery for a name you have NOT seen"),
        "call_tool must still forbid inventing tool names"
    );
    assert!(
        !call.contains("fetch it via `get_tool_schema` first"),
        "the unconditional pre-fetch instruction must be gone"
    );

    let search = tool_description(&defs, "search_tools");
    assert!(
        search.contains("without searching for it again"),
        "search_tools must exempt a tool the caller already knows"
    );
    assert!(
        !search.contains("before calling any tool"),
        "search_tools must not demand a search before every call"
    );
}

/// AC1: reproducible size measurement of every instruction surface an MCP
/// client receives.
///
/// Reproducible because nothing here reads the host: the instructions
/// render from an explicit [`InstructionContext`] instead of the user's
/// `repositories.json` and live PTY list, and the spawn response renders
/// from fixed ids instead of a launched process.
///
/// The upstream dimension measures TUIC's own contribution only — injected
/// upstream tools carry stub descriptions, and a real upstream's payload is
/// its own bytes, not ours. What the numbers show is that those bytes leave
/// `tools/list` entirely under collapse.
///
/// Set `TUIC_MCP_SURFACE_DUMP=<path>` to write every measured string out for
/// out-of-band tokenizer counts (see docs/backend/mcp-http.md).
#[test]
fn mcp_instruction_surface_bytes_stay_within_budget() {
    let state = test_state();

    let empty = InstructionContext {
        repos: Vec::new(),
        sessions: Vec::new(),
        peer_count: 0,
    };
    let loaded = InstructionContext {
        repos: (0..8)
            .map(|i| (format!("repo-{i}"), format!("/Users/dev/src/repo-{i}")))
            .collect(),
        sessions: (0..12)
            .map(|i| {
                (
                    format!("sess{i:04}"),
                    format!("/Users/dev/src/repo-{i}"),
                    format!("feature/branch-{i}"),
                )
            })
            .collect(),
        peer_count: 6,
    };

    let mut dump = serde_json::Map::new();
    let mut record = |key: &str, text: String| {
        let bytes = text.len();
        dump.insert(key.to_string(), serde_json::json!(text));
        bytes
    };

    let markers = (true, true);
    let instructions_classic_empty = record(
        "instructions.classic.empty",
        render_mcp_instructions(None, false, markers, true, &empty),
    );
    let instructions_classic_loaded = record(
        "instructions.classic.loaded",
        render_mcp_instructions(None, false, markers, true, &loaded),
    );
    let instructions_collapsed_empty = record(
        "instructions.collapsed.empty",
        render_mcp_instructions(None, true, markers, true, &empty),
    );
    record(
        "instructions.collapsed.loaded",
        render_mcp_instructions(None, true, markers, true, &loaded),
    );

    // Discovered schemas: what `get_tool_schema` hands back, per tool.
    for tool in native_tool_definitions().as_array().unwrap() {
        let name = tool["name"].as_str().unwrap().to_string();
        record(
            &format!("schema.{name}"),
            serde_json::to_string(tool).unwrap(),
        );
    }

    // register response — the other place the workflow prose lives.
    let register = handle_messaging(
        &state,
        &serde_json::json!({"action": "register", "name": "surface"}),
        Some("surface-register"),
    );
    let register_bytes = record(
        "response.register",
        serde_json::to_string(&register).unwrap(),
    );

    // spawn response — no static prose block, so it stays small by design.
    let spawn_bytes = record(
        "response.spawn",
        serde_json::to_string(&spawn_response(
            "11111111-2222-3333-4444-555555555555",
            "66666666-7777-8888-9999-000000000000",
            "worker",
            1_700_000_000_000,
            Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
            None,
            false,
        ))
        .unwrap(),
    );

    // tools/list, with and without upstreams, classic and collapsed.
    let tools_classic = record(
        "tools.classic.no_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, false)).unwrap(),
    );
    let tools_collapsed = record(
        "tools.collapsed.no_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, true)).unwrap(),
    );
    state
        .mcp
        .upstream_registry
        .inject_ready_upstream("alpha", &["one", "two", "three"]);
    state
        .mcp
        .upstream_registry
        .inject_ready_upstream("beta", &["four", "five"]);
    let tools_classic_upstreams = record(
        "tools.classic.with_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, false)).unwrap(),
    );
    let tools_collapsed_upstreams = record(
        "tools.collapsed.with_upstreams",
        serde_json::to_string(&merged_tool_definitions_for_mode(&state, None, true)).unwrap(),
    );

    if let Ok(path) = std::env::var("TUIC_MCP_SURFACE_DUMP") {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::Value::Object(dump.clone())).unwrap(),
        )
        .expect("write surface dump");
    }

    let measured: Vec<String> = dump
        .iter()
        .map(|(k, v)| format!("{k}={}", v.as_str().unwrap().len()))
        .collect();
    let measured = measured.join(" ");

    // Budgets are regression guards on the surfaces this story shrank, not
    // targets. Each is the measured value rounded up to the next 64 bytes.
    //
    // The collapsed budget moved 1600 -> 1664 when Progress gained its
    // reporting obligation. That section is the point of the feature — the
    // shipped build had only a tool description, and 39 repositories
    // recorded zero entries — and it is paid for elsewhere: `repo` lost
    // eight progress actions in the same change. Raising a budget for prose
    // that carries no obligation is not the same trade.
    assert!(
        instructions_classic_empty <= 1600,
        "classic instructions grew past their budget — {measured}"
    );
    assert!(
        instructions_collapsed_empty <= 1664,
        "collapsed instructions grew past their budget — {measured}"
    );
    assert!(
        instructions_classic_loaded - instructions_classic_empty <= 1600,
        "the dynamic repo/session block grew past its budget — {measured}"
    );
    assert!(
        tools_collapsed < tools_classic / 5,
        "collapse must stay a >=5x reduction — {measured}"
    );
    assert!(
        tools_collapsed_upstreams < tools_classic_upstreams / 5,
        "collapse must stay a >=5x reduction with upstreams — {measured}"
    );
    assert!(
        spawn_bytes <= 832,
        "spawn response must stay free of static prose — {measured}"
    );
    assert!(
        register_bytes <= 4608,
        "register response grew past its budget — {measured}"
    );
}

// ---- Swarm Layer 4: MCP tool descriptions (#1165-b124) -------------------

#[test]
fn session_description_includes_status_action() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session")
        .unwrap();
    let desc = session["description"].as_str().unwrap();
    assert!(
        desc.contains("status:"),
        "session description must document the status action"
    );
    let action_enum = session["inputSchema"]["properties"]["action"]["description"]
        .as_str()
        .unwrap();
    assert!(
        action_enum.contains("status"),
        "session action enum must include status"
    );
}

#[test]
fn session_submit_schema_pins_receipt_and_raw_input_semantics() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    let description = session["description"].as_str().unwrap();
    let properties = &session["inputSchema"]["properties"];

    assert!(description.contains("Use one call; never split text and Enter; never poll after it."));
    assert!(description.contains(
            "Acknowledgement means child terminal movement after Enter, not semantic application acceptance."
        ));
    assert!(
        description.contains("composer_state (tracked InputLineBuffer, not application state)")
    );
    assert!(description.contains(
            "Never queues; partial composers, dialogs, busy agents, and older queued commands reject before writing."
        ));
    assert!(description.contains(
            "input: Raw text/key compatibility surface. Send text and/or special_key; ok confirms PTY write only."
        ));
    assert_eq!(
        properties["input"]["description"],
        "Non-empty command (action=submit) or raw text (action=input)"
    );
    assert_eq!(
        properties["timeout_ms"]["description"],
        "action=submit: acknowledgement wait, clamped 250-10000ms, default 3000. action=wait: max wait, default 60000; values at or above 300000 run as 295000."
    );
}

#[test]
fn session_keep_open_schema_explains_the_managed_child_toggle() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    let description = session["description"].as_str().unwrap();
    let keep_open = description
        .lines()
        .find(|line| line.starts_with("- keep_open:"))
        .expect("session tool must document keep_open for MCP clients");
    assert!(keep_open.contains("managed child"));
    assert!(keep_open.contains("enabled=true"));
    assert!(keep_open.contains("enabled=false"));
    assert!(
        session["inputSchema"]["properties"]["session_id"]["description"]
            .as_str()
            .unwrap()
            .contains("keep_open"),
        "keep_open must list session_id as required"
    );
}

#[test]
fn session_description_requires_list_for_global_overview() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    let description = session["description"].as_str().unwrap();
    assert!(description.contains("All active sessions and states in one call"));
    assert!(description.contains("never fan out per-session status calls"));
}

#[test]
fn print_mode_description_clarifies_visible_vs_headless() {
    let defs = native_tool_definitions();
    let agent = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "agent")
        .unwrap();
    let pm_desc = agent["inputSchema"]["properties"]["print_mode"]["description"]
        .as_str()
        .unwrap();
    assert!(
        pm_desc.contains("visible") || pm_desc.contains("TUI tab"),
        "print_mode must mention visible TUI tab"
    );
    assert!(
        pm_desc.contains("headless"),
        "print_mode must mention headless mode"
    );
}

/// Monitoring guidance is delegated, not dropped.
///
/// Retargeted for #754-affa. The original assertion was
/// `instructions.contains("status")`, which the removed tool catalogue
/// happened to satisfy — a substring that loose cannot tell "documented"
/// from "coincidence". The invariant worth keeping is that an orchestrator
/// reaching for the observation path still finds it, on the surface that
/// every client receives.
#[test]
fn monitoring_guidance_lives_in_the_tool_descriptions() {
    let state = test_state();
    let out = build_mcp_instructions(&state, None);
    assert!(
        out.contains("Read each tool's description for actions and rules."),
        "instructions must send the reader to the descriptions"
    );

    let defs = native_tool_definitions();
    let session = tool_description(&defs, "session");
    let agent = tool_description(&defs, "agent");
    assert!(
        session.contains("- status:"),
        "session description must document the status action"
    );
    assert!(
        agent.contains("agent action=wait") || agent.contains("wait: Block"),
        "agent description must document the blocking wait"
    );
    assert!(
        agent.contains("inbox:"),
        "agent description must document inbox"
    );
}

/// Every action a tool advertises must be documented by the tool itself.
///
/// Generalised for #754-affa from `instructions_tools_and_definitions_in_sync_for_session_actions`,
/// which checked one tool against the initialize instructions. The
/// instructions are the surface a client may never read (Codex ignores
/// them); the description is the surface every client receives, so that is
/// where the action list has to agree with the schema. This is the gate
/// that caught nine `progress_*` actions advertised by `REPO_ACTIONS` and
/// by the `repo` schema enum while the description body named none of them.
#[test]
fn every_documented_action_constant_matches_schema_and_description() {
    let defs = native_tool_definitions();

    // (tool, action constant, description documents each action)
    // `debug` is the one exemption: its description points at `action=help`,
    // which returns the full usage guide, so duplicating five action bullets
    // into a tool that is disabled by default would buy nothing.
    let cases: [(&str, &str, bool); 8] = [
        ("session", SESSION_ACTIONS, true),
        ("agent", AGENT_ACTIONS, true),
        ("task", TASK_ACTIONS, true),
        ("repo", REPO_ACTIONS, true),
        ("ui", UI_ACTIONS, true),
        ("config", CONFIG_ACTIONS, true),
        ("voice", VOICE_ACTIONS, true),
        ("debug", DEBUG_ACTIONS, false),
    ];

    for (tool, actions, documented) in cases {
        let def = defs
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == tool)
            .unwrap_or_else(|| panic!("native tool '{tool}' missing"));
        let enum_desc = def["inputSchema"]["properties"]["action"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("tool '{tool}' has no action description"));
        let schema_actions: std::collections::BTreeSet<&str> = enum_desc
            .strip_prefix("One of: ")
            .unwrap_or_else(|| {
                panic!(
                    "tool '{tool}' action description must start with 'One of: ', got: {enum_desc}"
                )
            })
            .split(", ")
            .collect();
        let constant_actions: std::collections::BTreeSet<&str> = actions.split(", ").collect();
        assert_eq!(
            schema_actions, constant_actions,
            "tool '{tool}': schema enum and its action constant disagree"
        );

        if !documented {
            continue;
        }
        let description = def["description"].as_str().unwrap();
        for action in &constant_actions {
            assert!(
                description.contains(&format!("{action}:")),
                "tool '{tool}' advertises action '{action}' but its description never documents it"
            );
        }
    }
}

/// After `rebuild_tool_search_index`, the cache contains every native
/// tool from `native_tool_definitions()`.
#[test]
fn rebuild_tool_search_index_populates_all_native_tools() {
    let state = test_state();
    rebuild_tool_search_index(&state);
    let idx = state.mcp.tool_search_index.read();
    let native_count = native_tool_definitions().as_array().unwrap().len();
    assert_eq!(idx.len(), native_count);
    // Spot-check a few well-known native tools by name.
    assert!(idx.get_schema("session").is_some());
    assert!(idx.get_schema("repo").is_some());
    assert!(idx.get_schema("agent").is_some());
}

/// After mutating `disabled_native_tools` and rebuilding, the disabled
/// tool no longer appears in the cached index.
#[test]
fn rebuild_tool_search_index_respects_disabled_native_tools() {
    let state = test_state();
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_some()
    );
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    rebuild_tool_search_index(&state);
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_none()
    );
}

/// The background updater task subscribes to `mcp_tools_changed` and
/// rebuilds the cached index on every signal. This is what wires upstream
/// add/remove, native-tool toggle, and collapse-tools toggle events into
/// the cache without each call site having to rebuild manually.
#[tokio::test]
async fn tool_search_index_rebuilds_on_broadcast() {
    let state = test_state();

    // Start the updater — it does its own initial rebuild, then loops on the broadcast.
    spawn_tool_search_index_updater(state.clone());
    // Give the initial build a moment to land.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_some()
    );

    // Mutate config and fire the signal; the updater must rebuild.
    state.config.write().disabled_native_tools = vec!["session".to_string()];
    let _ = state.mcp.tools_changed.send(());

    // Poll for the rebuild with a short deadline — the task is async.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        if state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_none()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("tool_search_index was not rebuilt after mcp_tools_changed signal");
}

/// Toggling `collapse_tools` must not corrupt the searchable corpus:
/// the cache always holds the full tool list regardless of the collapse
/// state (collapse only affects what the client sees via tools/list).
#[tokio::test]
async fn tool_search_index_ignores_collapse_tools_toggle() {
    let state = test_state();
    spawn_tool_search_index_updater(state.clone());
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let before = state.mcp.tool_search_index.read().len();

    state.config.write().collapse_tools = true;
    let _ = state.mcp.tools_changed.send(());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let after = state.mcp.tool_search_index.read().len();
    assert_eq!(
        before, after,
        "collapse_tools toggle must not change searchable corpus size"
    );
    // And native tools must still be searchable.
    assert!(
        state
            .mcp
            .tool_search_index
            .read()
            .get_schema("session")
            .is_some()
    );
}
