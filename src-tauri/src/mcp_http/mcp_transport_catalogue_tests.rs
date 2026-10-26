/// Catches: Anthropic rejects advertised action contracts with root combinators,
/// including schemas returned through lazy search and exact lookup.
#[test]
fn advertised_native_schemas_have_no_root_combinators() {
    let state = test_state();
    {
        let mut config = state.config.write();
        config.progress_tracking = true;
        config.settings_mcp_enabled = true;
    }
    rebuild_tool_search_index(&state);
    let mut failures = Vec::new();
    let mut check = |surface: &str, tool: &serde_json::Value| {
        let schema = &tool["inputSchema"];
        for keyword in ["oneOf", "anyOf", "allOf"] {
            if schema.get(keyword).is_some() {
                failures.push(format!("{surface}: {} has {keyword}", tool["name"]));
            }
        }
    };
    for tool in native_tool_definitions().as_array().unwrap() {
        check("native", tool);
    }
    for tool in merged_tool_definitions_for_mode(&state, None, false)
        .as_array()
        .unwrap()
    {
        check("tools/list", tool);
    }
    for tool in searchable_tool_definitions(&state) {
        let name = tool["name"].as_str().unwrap();
        let exact = handle_get_tool_schema(&state, &serde_json::json!({"tool_name":name}));
        assert_eq!(exact["name"], name);
        check("get_tool_schema", &exact);
        let search = handle_search_tools(
            &state,
            &serde_json::json!({
                "query":name, "schema_byte_budget":65536
            }),
        );
        assert!(search["count"].as_u64().unwrap() > 0, "{name}");
        if let Some(top) = search.get("top_schema") {
            check("search_tools", top);
        }
    }
    // Catches: an untagged wrapper hides a tagged action enum (settings
    // transactions), or a flattened owner union loses its action-specific fields.
    fn check_variant_fields(
        private: &serde_json::Value,
        advertised: &serde_json::Value,
        name: &str,
        failures: &mut Vec<String>,
    ) {
        if let Some(properties) = private["properties"].as_object() {
            for (field, schema) in properties {
                if advertised["properties"].get(field).is_none() {
                    failures.push(format!("{name} loses field {field}"));
                }
                if field == "action" {
                    let values: Vec<_> = schema
                        .get("const")
                        .into_iter()
                        .chain(schema["enum"].as_array().into_iter().flatten())
                        .collect();
                    for action in values {
                        if !advertised["properties"]["action"]["enum"]
                            .as_array()
                            .is_some_and(|actions| actions.contains(action))
                        {
                            failures.push(format!("{name} loses action {action}"));
                        }
                    }
                }
            }
        }
        // Follow only object-level wrappers, never parameter schemas.
        for keyword in ["oneOf", "anyOf", "allOf"] {
            if let Some(variants) = private[keyword].as_array() {
                for variant in variants {
                    check_variant_fields(variant, advertised, name, failures);
                }
            }
        }
    }
    let advertised = native_tool_definitions();
    for private in super::super::mcp_transport_catalogue::native_validation_tool_definitions()
        .as_array()
        .unwrap()
    {
        let name = private["name"].as_str().unwrap();
        let public = advertised
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap();
        check_variant_fields(
            &private["inputSchema"],
            &public["inputSchema"],
            name,
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Catches: Settings rows reuse long MCP paragraphs or omit newly registered tools.
#[test]
fn native_tool_catalog_summaries_are_short_and_not_description_prefixes() {
    let definitions = native_tool_definitions();
    let tools = definitions.as_array().unwrap();
    let catalog = native_tool_catalog();
    // Agent descriptions may shrink; Settings copy must stay unchanged.
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native-guidance-2069.json")).unwrap();
    assert_ne!(tools.as_slice(), [] as [serde_json::Value; 0]);
    assert_eq!(catalog.len(), tools.len());
    assert_eq!(NATIVE_TOOL_SUMMARIES.len(), tools.len());
    for (tool, entry) in tools.iter().zip(&catalog) {
        let name = tool["name"].as_str().unwrap();
        let description = tool["description"].as_str().unwrap();
        let summary = entry["summary"].as_str().unwrap();
        assert_eq!(entry["name"], tool["name"]);
        assert_eq!(
            &entry["description"],
            recorded
                .get(name)
                .filter(|_| name != "workflow")
                .unwrap_or(&tool["description"])
        );
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
        assert!(instructions.contains("at row start or list-item text start (marker + space)"));
        assert!(instructions.contains("Exclude status, tool and hook/task notices."));
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

// Catches: a configured voice is filtered out of the lazy discovery corpus.
#[tokio::test]
async fn conditional_configured_voice_is_discoverable_by_list_search_and_name() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    write_voice_config(dir.path(), true, true);
    let state = test_state();
    assert!(
        tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "voice")
    );

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
fn agent_task_actions_are_listed_and_searchable() {
    let state = test_state();

    let listed = merged_tool_definitions(&state, None, None);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "agent"
                && t["inputSchema"]["properties"]["action"]["enum"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|action| action == "task_get")),
        "agent task actions must appear in tools/list"
    );
    assert!(
        searchable_tool_definitions(&state)
            .iter()
            .any(|t| t["name"] == "agent"
                && t["inputSchema"]["properties"]["action"]["enum"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|action| action == "task_get")),
        "agent task actions must appear in the search corpus"
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
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = test_state();
    {
        let mut config = state.config.write();
        config.mcp_capabilities.denied.clear();
        config.settings_mcp_enabled = true;
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
            "session",
            "agent",
            "repo",
            "workflow",
            "progress",
            "ui",
            "plugin_dev_guide",
            "settings",
            "debug",
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
        called["error"], "Tool 'telegram' is unavailable: Telegram is not configured",
        "unconfigured Telegram must be refused before its native authority check"
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
    state.config.write().mcp_capabilities.denied = vec!["progress".to_string()];
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

// Catches: expanded discovery exposes optional tools without owner configuration.
#[test]
fn merged_tools_collapse_false_returns_all_native_tools() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    // No voice/Telegram configuration files or live remote connections. All
    // ordinary tools are enabled; Design Mode has no armed owner for this caller.
    let state = test_state();
    {
        let mut config = state.config.write();
        config.mcp_capabilities.denied.clear();
        config.settings_mcp_enabled = true;
        config.progress_tracking = true;
        config.collapse_tools = false;
    }

    let merged = merged_tool_definitions(&state, None, None);
    let names = tool_names(&merged);
    assert_eq!(
        names,
        vec![
            "automations",
            "secret",
            "session",
            "agent",
            "repo",
            "workflow",
            "progress",
            "ui",
            "plugin_dev_guide",
            "settings",
            "debug",
        ],
        "expanded discovery must return exactly the tools available to this unconfigured caller"
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
    state.config.write().mcp_capabilities.denied = vec!["progress".to_string()];

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
    let ctx = InstructionContext::default();
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

// Catches required name terms becoming optional BM25 description boosts, before and after limit.
#[test]
fn required_name_terms_do_not_fall_back_to_description_matches() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/tool-search-2300.json")).unwrap();
    let tools = fixture["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "quill__get_file"));
    let state = test_state();
    *state.mcp.tool_search_index.write() = crate::tool_search::ToolSearchIndex::build(tools);
    let mut failures = Vec::new();
    for query in ["+repo", "+repo status"] {
        for limit in [1, 100] {
            let response = handle_search_tools(
                &state,
                &serde_json::json!({"query":query,"limit":limit,"schema_byte_budget":65536}),
            );
            let names: Vec<_> = response["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["name"].as_str().unwrap())
                .collect();
            if names != ["repo"] || response["top_schema"]["name"] != "repo" {
                failures.push(format!("query={query:?}, limit={limit}: {response}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

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
    state.config.write().mcp_capabilities.denied = vec!["session".to_string()];
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
    state.config.write().mcp_capabilities.denied = vec!["diagnostics".to_string()];
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
    state.config.write().mcp_capabilities.denied = vec!["repo".to_string()];
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
    let state = test_state();
    state.config.write().settings_mcp_enabled = true;
    state.config.write().mcp_capabilities.denied.clear();
    let r = handle_call_tool(
        &state,
        non_loopback_addr(),
        &serde_json::json!({
            "tool_name":"settings", "arguments":{"action":"refresh","store":"ego"}
        }),
        Some("remote-test"),
        None,
    )
    .await;
    assert_eq!(r["error"]["code"], "forbidden_origin", "{r}");
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
    assert!(out.contains("once per MCP connection/reconnect"));
    assert!(out.contains("Never repeat per turn."));
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
        serde_json::json!(["normal", "urgent", "interrupt"])
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

/// `urgency` is a generic caller choice: the schema offers exactly the names
/// `agent action=send` uses, and the description states both effects.
#[test]
fn ui_toast_documents_urgency_generically() {
    let defs = native_tool_definitions();
    let ui = tool_description(&defs, "ui");
    assert!(ui.contains("urgency: normal (default) records the notice in the bell only"));
    assert!(ui.contains("urgent also shows a transient card"));
    let schema = &defs
        .as_array()
        .expect("tool definitions are an array")
        .iter()
        .find(|d| d["name"] == "ui")
        .expect("ui tool")["inputSchema"]["properties"]["urgency"];
    assert_eq!(schema["enum"], serde_json::json!(["normal", "urgent"]));
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

    let empty = InstructionContext::default();
    let loaded = InstructionContext {
        repos: RepoSummary {
            total: 8,
            groups: vec![("work".into(), 5)],
            remote: Vec::new(),
        },
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
        ..Default::default()
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
    let cases: [(&str, &str, bool); 6] = [
        ("session", SESSION_ACTIONS, true),
        ("agent", AGENT_ACTIONS, true),
        ("repo", REPO_ACTIONS, true),
        ("ui", UI_ACTIONS, true),
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

// Catches: index rebuild exposes unconfigured optional tools or drops available tools.
#[test]
fn rebuild_tool_search_index_populates_all_native_tools() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    // No voice/Telegram configuration files or live remote connections. The
    // shared index retains Design Mode; discovery applies its per-caller grant.
    let state = test_state();
    {
        let mut config = state.config.write();
        config.mcp_capabilities.denied.clear();
        config.settings_mcp_enabled = true;
        config.progress_tracking = true;
        config.collapse_tools = false;
    }
    rebuild_tool_search_index(&state);
    let idx = state.mcp.tool_search_index.read();
    let names: Vec<_> = idx
        .entries()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "automations",
            "secret",
            "session",
            "agent",
            "repo",
            "workflow",
            "progress",
            "ui",
            "plugin_dev_guide",
            "settings",
            "debug",
        ],
        "index rebuild must contain exactly the tools available with unconfigured owners"
    );
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
    state.config.write().mcp_capabilities.denied = vec!["session".to_string()];
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
    state.config.write().mcp_capabilities.denied = vec!["session".to_string()];
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

mod secret_form_dispatch {
    //! Critic tests for story 1435: the secret-form inspection block must hold on
    //! every dispatch path, not only the ones that reach `handle_mcp_tool_call_with_context`.
    use crate::mcp_http::mcp_transport::mcp_post;
    use crate::mcp_http::tests::test_state;
    use crate::secrets::{Field, FieldKind, Form};
    use axum::{
        Json,
        extract::{ConnectInfo, State},
        http::HeaderMap,
        response::IntoResponse,
    };

    async fn tools_call_text(state: std::sync::Arc<crate::AppState>, name: &str) -> String {
        let response = mcp_post(
            State(state),
            ConnectInfo("127.0.0.1:12345".parse().unwrap()),
            HeaderMap::new(),
            Json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": name, "arguments": {}}
            })),
        )
        .await
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        body["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn open_form(state: &crate::AppState) {
        let form = Form::request(
            vec![Field {
                name: "TOKEN".into(),
                kind: FieldKind::Password,
                display: None,
            }],
            "critic".into(),
        )
        .unwrap();
        state.secrets.open(form).unwrap();
    }

    /// Catches: a direct `tools/call` of an upstream-namespaced tool (maccontrol
    /// capture_screen / capture_window / do / shell) skips the secret-form block
    /// because the `__` branch of `mcp_post` proxies without consulting the store,
    /// so an agent can screenshot or keystroke the private form.
    #[tokio::test]
    async fn direct_upstream_tools_call_is_refused_while_secret_form_is_open() {
        let state = test_state();
        open_form(&state);
        let text = tools_call_text(state, "maccontrol__capture_screen").await;
        assert!(
            text.contains("private secret form"),
            "upstream tool was not gated by the open secret form: {text}"
        );
    }

    /// Catches: gating only the native names while `call_tool` wrapping an
    /// upstream name slips through (control for the case above).
    #[tokio::test]
    async fn call_tool_wrapped_upstream_is_refused_while_secret_form_is_open() {
        let state = test_state();
        open_form(&state);
        let response = super::mcp_transport::handle_mcp_tool_call(
            &state,
            "127.0.0.1:12345".parse().unwrap(),
            "call_tool",
            &serde_json::json!({"tool_name": "maccontrol__capture_screen", "arguments": {}}),
            None,
        )
        .await;
        assert!(
            response["error"]
                .as_str()
                .unwrap_or_default()
                .contains("private secret form"),
            "{response}"
        );
    }
}

// ---- initialize message: capability map, repo counts, child markers (1754-071d) ----

fn repo_document(local: usize, remote: usize) -> serde_json::Value {
    let paths: Vec<String> = (0..local + remote)
        .map(|i| format!("/Users/dev/src/some-long-repository-name-{i}"))
        .collect();
    let mut repos = serde_json::Map::new();
    for (i, path) in paths.iter().enumerate() {
        let entry = if i < local {
            serde_json::json!({})
        } else {
            serde_json::json!({"connectionId": "ssh-box"})
        };
        repos.insert(path.clone(), entry);
    }
    serde_json::json!({
        "repoOrder": paths,
        "repos": repos,
        "groups": {"g1": {"name": "work", "repoOrder": paths[..local.min(3)]}},
    })
}

fn tools(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

// Catches: a path list leaking back into the instructions, or counts that
// ignore groups and remote connections.
#[test]
fn repos_render_as_one_count_line_with_group_and_connection_counts() {
    let ctx = InstructionContext {
        repos: summarize_repos(&repo_document(37, 3)),
        ..Default::default()
    };
    let out = render_mcp_instructions(None, false, (true, true), true, &ctx);
    assert!(!out.contains("/Users/dev"), "no repo path may be rendered");
    assert!(
        out.contains("40 repositories (group work 3, remote ssh-box 3)"),
        "{out}"
    );
    assert!(out.contains("`repo action=list`"));
    let collapsed = render_mcp_instructions(None, true, (true, true), true, &ctx);
    assert!(collapsed.contains("call_tool tool_name=repo arguments={action:list}"));
}

// Catches: repo-specific text (paths, names) in the instructions, which made
// every added repository a new prompt-inspector row.
#[test]
fn adding_a_repo_changes_only_the_count() {
    let render = |n| {
        let ctx = InstructionContext {
            repos: summarize_repos(&repo_document(n, 0)),
            ..Default::default()
        };
        render_mcp_instructions(None, false, (true, true), true, &ctx)
    };
    let (a, b) = (render(10), render(11));
    assert_eq!(
        a.replace("10 repositories", "N repositories"),
        b.replace("11 repositories", "N repositories")
    );
}

// Catches: a remote repo row without its connection, or a local one claiming one.
#[test]
fn only_remote_repo_rows_carry_a_connection_id() {
    let doc = repo_document(1, 1);
    let order = doc["repoOrder"].as_array().unwrap();
    let mut local = serde_json::json!({"path": order[0]});
    let mut remote = serde_json::json!({"path": order[1]});
    tag_repo_connection(&mut local, doc["repos"].get(order[0].as_str().unwrap()));
    tag_repo_connection(&mut remote, doc["repos"].get(order[1].as_str().unwrap()));
    assert!(local.get("connection_id").is_none());
    assert_eq!(remote["connection_id"], "ssh-box");
    let mut absent = serde_json::json!({});
    tag_repo_connection(&mut absent, None);
    assert!(absent.get("connection_id").is_none());
}

// Catches: a capability map naming a tool this install did not register (voice
// without dictation), omitting a registered one, or inventing upstream servers.
#[test]
fn capability_map_names_exactly_the_registered_tools_and_upstreams() {
    let registered = tools(&[
        "ui",
        "session",
        "agent",
        "repo",
        "plugin_dev_guide",
        "telegram",
    ]);
    let ctx = InstructionContext {
        native_tools: registered.clone(),
        upstreams: vec![("quill".into(), 4), ("maccontrol".into(), 7)],
        ..Default::default()
    };
    let out = render_mcp_instructions(Some("claude-code"), false, (true, true), true, &ctx);
    for name in &registered {
        assert!(out.contains(&format!("`{name}`")), "{name} missing: {out}");
    }
    for absent in ["voice", "automations", "secret", "story"] {
        assert!(
            !out.contains(&format!("`{absent}`")),
            "{absent} is not registered"
        );
    }
    assert!(out.contains("`quill` (4 tools), `maccontrol` (7 tools)"));
    assert!(out.contains("ToolSearch"));
    assert!(out.contains("urgency=urgent"));
    let collapsed = render_mcp_instructions(None, true, (true, true), true, &ctx);
    assert!(collapsed.contains("`search_tools query=<server>`"));
    let plain = render_mcp_instructions(None, false, (true, true), true, &ctx);
    assert!(plain.contains("<server>__<tool>"));
    assert!(!plain.contains("ToolSearch"));
}

// Catches: an upstream line (or the whole map) rendered when nothing backs it.
#[test]
fn no_upstreams_means_no_upstream_line_and_no_tools_means_no_map() {
    let ctx = InstructionContext {
        native_tools: tools(&["session"]),
        ..Default::default()
    };
    let out = render_mcp_instructions(None, false, (true, true), true, &ctx);
    assert!(out.contains("## What TUIC gives you"));
    assert!(!out.contains("Other MCP servers"));
    let bare = render_mcp_instructions(
        None,
        false,
        (true, true),
        true,
        &InstructionContext::default(),
    );
    assert!(!bare.contains("## What TUIC gives you"));
}

// Catches: the live build listing tools that tools/list does not serve, or
// ignoring the disabled-tools setting and the connected upstreams.
#[test]
fn built_instructions_follow_the_served_tool_list_and_ready_upstreams() {
    let state = test_state();
    state.config.write().mcp_capabilities.denied = vec!["telegram".into()];
    state
        .mcp
        .upstream_registry
        .inject_ready_upstream("quill", &["a", "b"]);
    let out = build_mcp_instructions_for_mode(&state, Some("claude-code"), false);
    let served = filtered_native_tools(&state);
    assert_ne!(served, [] as [serde_json::Value; 0]);
    for tool in &served {
        let name = tool["name"].as_str().unwrap();
        assert!(
            out.contains(&format!("`{name}`")),
            "{name} is served but unnamed"
        );
    }
    assert!(
        !out.contains("`telegram`"),
        "disabled tool must not be named"
    );
    assert!(out.contains("`quill` (2 tools)"));
}

// Catches: a size regression — the map plus count line must cost less than the
// path list it replaces (the pre-change output is the bare text plus that list).
#[test]
fn instructions_with_forty_repos_and_six_upstreams_are_smaller_than_before() {
    let registered: Vec<String> = native_tool_definitions()
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let ctx = InstructionContext {
        repos: summarize_repos(&repo_document(40, 0)),
        native_tools: registered,
        upstreams: (0..6).map(|i| (format!("server-{i}"), 10)).collect(),
        ..Default::default()
    };
    let old_repos_section = {
        let mut s = String::from("## Repos\n\n");
        for i in 0..40 {
            s.push_str(&format!(
                "- **some-long-repository-name-{i}** `/Users/dev/src/some-long-repository-name-{i}`\n"
            ));
        }
        s.push('\n');
        s
    };
    for collapse in [false, true] {
        let bare = render_mcp_instructions(
            None,
            collapse,
            (true, true),
            true,
            &InstructionContext::default(),
        );
        let new = render_mcp_instructions(None, collapse, (true, true), true, &ctx);
        assert!(
            new.len() < bare.len() + old_repos_section.len(),
            "collapse={collapse}: {} >= {}",
            new.len(),
            bare.len() + old_repos_section.len()
        );
    }
}

// Catches: spawned children receiving the suggest marker nobody sees, or the
// root session losing it.
#[test]
fn suggest_marker_is_for_root_sessions_only() {
    let render = |spawned_child| {
        let ctx = InstructionContext {
            spawned_child,
            ..Default::default()
        };
        render_mcp_instructions(None, false, (true, true), true, &ctx)
    };
    assert!(render(false).contains("- `suggest:`"));
    assert!(!render(true).contains("suggest:"));
    assert!(render(true).contains("- `intent:"), "other markers stay");
}

// Catches: the child check reading the wrong identity so a real spawned child
// still gets suggest (or a root loses it).
#[test]
fn live_child_session_gets_no_suggest_and_root_does() {
    const ROOT: &str = "11111111-1111-4111-8111-111111111111";
    const CHILD: &str = "22222222-2222-4222-8222-222222222222";
    let state = test_state();
    for (tuic, mcp) in [(ROOT, "mcp-root"), (CHILD, "mcp-child")] {
        handle_messaging(
            &state,
            &serde_json::json!({"action": "register", "tuic_session": tuic, "name": mcp}),
            Some(mcp),
        );
    }
    state
        .session_maps
        .session_parent
        .insert(CHILD.to_string(), ROOT.to_string());
    let root = build_mcp_instructions_for_session(&state, None, false, Some("mcp-root"));
    let child = build_mcp_instructions_for_session(&state, None, false, Some("mcp-child"));
    assert!(root.contains("- `suggest:`"));
    assert!(!child.contains("suggest:"));
}

// Catches: a claim the server cannot know (the user is away) or a platform
// name in generic wording, in any rendering of the instructions.
#[test]
fn instructions_make_no_claim_about_the_user_or_the_platform() {
    let ctx = InstructionContext {
        native_tools: tools(&["ui", "voice", "telegram", "progress", "session"]),
        upstreams: vec![("quill".into(), 1)],
        ..Default::default()
    };
    for collapse in [false, true] {
        for client in [None, Some("claude-code")] {
            let out = render_mcp_instructions(client, collapse, (true, true), true, &ctx);
            for banned in ["away", "Mac"] {
                assert!(!out.contains(banned), "{banned:?} in: {out}");
            }
        }
    }
}

/// Catches: discovery forces a second schema fetch for an unchanged admitted tool.
#[test]
fn catalogue_schema_reuse_avoids_second_discovery_hop() {
    let state = test_state();
    let definition = serde_json::json!({
        "name": "needle", "description": "Needle discovery.",
        "inputSchema": {"type": "object", "properties": {}}
    });
    *state.mcp.tool_search_index.write() =
        crate::tool_search::ToolSearchIndex::build(std::slice::from_ref(&definition));
    let args = serde_json::json!({"query": "needle", "schema_byte_budget": 4096});
    let first = handle_search_tools(&state, &args);
    assert_eq!(first["top_schema"], definition);
    let second = handle_search_tools(&state, &args);
    assert_eq!(second["top_schema"], first["top_schema"]);
    assert_eq!(
        second["catalogue_generation"],
        first["catalogue_generation"]
    );
    assert!(first["catalogue_generation"].is_string());
    assert_eq!(
        handle_get_tool_schema(&state, &serde_json::json!({"tool_name": "needle"})),
        definition
    );
}

/// Catches: schema truncation or character-count budgeting admits an incomplete UTF-8 contract.
#[test]
fn catalogue_schema_reuse_budget_counts_complete_utf8_definition() {
    let state = test_state();
    let definition = serde_json::json!({"name":"needle", "description":"Needle éé discovery.", "inputSchema":{"type":"object"}});
    let bytes = serde_json::to_vec(&definition).unwrap().len();
    *state.mcp.tool_search_index.write() =
        crate::tool_search::ToolSearchIndex::build(std::slice::from_ref(&definition));
    for budget in [0, bytes - 1] {
        let result = handle_search_tools(
            &state,
            &serde_json::json!({"query":"needle", "schema_byte_budget":budget}),
        );
        assert!(result.get("top_schema").is_none());
        assert_eq!(result["results"][0]["name"], "needle");
    }
    let result = handle_search_tools(
        &state,
        &serde_json::json!({"query":"needle", "schema_byte_budget":bytes}),
    );
    assert_eq!(result["top_schema"], definition);
    let empty = handle_search_tools(&state, &serde_json::json!({"query":"zzzzabsent"}));
    assert!(empty.get("top_schema").is_none());
}

/// Catches: a removed tool keeps its admitted generation or inline schema after invalidation.
#[test]
fn catalogue_schema_reuse_generation_changes_with_visibility() {
    let state = test_state();
    rebuild_tool_search_index(&state);
    let before = handle_search_tools(
        &state,
        &serde_json::json!({"query":"PTY multiplexer tmux pane lifecycle"}),
    );
    assert_eq!(before["top_schema"]["name"], "session");
    rebuild_tool_search_index(&state);
    let unchanged = handle_search_tools(
        &state,
        &serde_json::json!({"query":"PTY multiplexer tmux pane lifecycle"}),
    );
    assert_eq!(
        before["catalogue_generation"],
        unchanged["catalogue_generation"]
    );
    state
        .config
        .write()
        .mcp_capabilities
        .denied
        .push("session".to_owned());
    rebuild_tool_search_index(&state);
    let after = handle_search_tools(
        &state,
        &serde_json::json!({"query":"PTY multiplexer tmux pane lifecycle"}),
    );
    assert_ne!(
        before["catalogue_generation"],
        after["catalogue_generation"]
    );
    assert_ne!(after["top_schema"]["name"], "session");
    assert!(
        handle_get_tool_schema(&state, &serde_json::json!({"tool_name":"session"}))
            .get("error")
            .is_some()
    );
}

/// Catches: observed bad inputs reach handlers and return opaque retry errors.
#[tokio::test]
async fn action_validation_rejects_observed_bad_inputs_before_effects() {
    let state = test_state();
    let cases = [
        (
            "progress",
            serde_json::json!({"type":"done", "text":"x".repeat(582)}),
        ),
        (
            "progress",
            serde_json::json!({"type":"done", "text":"x".repeat(543)}),
        ),
        (
            "progress",
            serde_json::json!({"type":"done", "text":"x".repeat(502)}),
        ),
        (
            "ui",
            serde_json::json!({"action":"tab", "id":"never-created", "html":"safe"}),
        ),
        ("agent", serde_json::json!({"action":"list"})),
        ("repo", serde_json::json!({"action":"worktree_create"})),
    ];
    for (tool, args) in cases {
        let result = handle_mcp_tool_call(&state, loopback_addr(), tool, &args, None).await;
        assert_eq!(result["code"], "invalid_arguments", "{tool}: {result}");
        assert_eq!(result["effect"], "not_started");
        assert_eq!(result["retryable"], false);
        assert_eq!(result["repair"]["retry_after_correction"], true);
        assert_ne!(
            result["violations"].as_array().unwrap().as_slice(),
            [] as [serde_json::Value; 0]
        );
        assert!(result["violations"][0]["path"].is_string());
    }
    assert!(state.session_maps.session_html_tabs.is_empty());
}

/// Catches: launch-capable agents receiving conflicting rules twice through MCP.
#[test]
fn launch_capable_clients_receive_no_duplicate_playbook() {
    for name in ["claude-code", "codex", "ego"] {
        let text = render_mcp_instructions(
            Some(name),
            false,
            (true, true),
            true,
            &InstructionContext::default(),
        );
        assert!(!text.contains("Required Output Markers"), "{name}: {text}");
        assert!(!text.contains("**Submit:**"));
        assert!(!text.contains("## Progress — mandatory"));
        assert!(!text.contains("never `git worktree add/remove`"));
        assert!(text.contains("## Multi-Agent Work"));
    }
}

/// Catches: merging launch bypasses static validation or omits its reusable discovery contract.
#[tokio::test]
async fn launch_flat_discovery_keeps_predispatch_validation() {
    let state = test_state();
    let definition = native_tool_definitions()
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "agent")
        .unwrap()
        .clone();
    *state.mcp.tool_search_index.write() =
        crate::tool_search::ToolSearchIndex::build(std::slice::from_ref(&definition));
    let discovery = handle_search_tools(
        &state,
        &serde_json::json!({
            "query":"agent", "schema_byte_budget":65536
        }),
    );
    assert_eq!(discovery["top_schema"], definition);
    let validator = jsonschema::validator_for(&discovery["top_schema"]["inputSchema"]).unwrap();
    let creation = serde_json::json!({
        "action":"launch", "idempotency_key":"stable-key", "parent_session_id":"parent",
        "name":"worker", "keep_open":true, "workspace":{"existing_path":"/fixture"},
        "spawn":{"prompt":"task", "agent_type":"ego", "rows":24, "cols":80}
    });
    let mut new_workspace = creation.clone();
    new_workspace["workspace"] = serde_json::json!({"repo_path":"/fixture", "branch":"feat/child"});
    for valid in [
        &creation,
        &new_workspace,
        &serde_json::json!({"action":"launch", "launch_id":"receipt"}),
    ] {
        assert!(validator.is_valid(valid), "{valid}");
        assert!(crate::mcp_http::mcp_input_contract::validate("agent", valid).is_none());
    }
    let mut missing = creation.clone();
    missing.as_object_mut().unwrap().remove("keep_open");
    let mut mixed = creation.clone();
    mixed["launch_id"] = serde_json::json!("receipt");
    let mut ambiguous_workspace = creation.clone();
    ambiguous_workspace["workspace"]["branch"] = serde_json::json!("feat/child");
    let mut missing_prompt = creation.clone();
    missing_prompt["spawn"]
        .as_object_mut()
        .unwrap()
        .remove("prompt");
    let mut reserved = creation.clone();
    reserved["spawn"]["cwd"] = serde_json::json!("/other");
    let mut malformed = creation.clone();
    malformed["spawn"]["env"] = serde_json::json!({"TOKEN":42});
    let mut oversized = creation.clone();
    oversized["spawn"]["rows"] = serde_json::json!(crate::mcp_http::MAX_TERMINAL_DIMENSION + 1);
    for invalid in [
        serde_json::json!({"action":"launch"}),
        missing,
        mixed,
        ambiguous_workspace,
        missing_prompt,
        reserved,
        malformed,
        oversized,
    ] {
        // Discovery is deliberately permissive for cross-field action requirements;
        // only the private contract enforces them, before effects can start.
        let result = handle_mcp_tool_call(&state, loopback_addr(), "agent", &invalid, None).await;
        assert_eq!(result["code"], "invalid_arguments", "{result}");
        assert_eq!(result["effect"], "not_started");
        assert_eq!(result["retryable"], false);
    }
    assert!(!state.data_dir.join("managed-launches.sqlite").exists());
    assert!(state.session_maps.sessions.is_empty());
}

// Catches: hiding remote in discovery while a direct call still reaches deployment.
#[tokio::test]
async fn conditional_remote_direct_call_without_connection_is_refused() {
    let state = test_state();
    state.config.write().mcp_capabilities.denied.clear();
    let result = handle_mcp_tool_call(
        &state,
        loopback_addr(),
        "remote",
        &serde_json::json!({"action":"preview", "connection_id":"absent"}),
        None,
    )
    .await;
    assert_eq!(
        result["error"], "Tool 'remote' is unavailable: no connected remote server",
        "{result}"
    );
}

fn write_voice_config(path: &std::path::Path, enabled: bool, replies: bool) {
    let config = crate::config::DictationConfig {
        enabled,
        hands_free_spoken_replies: replies,
        ..Default::default()
    };
    std::fs::write(
        path.join("dictation-config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
}

// Catches: native and meta calls bypass an absent owner configuration; a stale
// search index still reveals a tool after its owner is switched off.
#[tokio::test]
async fn conditional_owner_flags_gate_discovery_schema_and_both_call_paths() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    write_voice_config(dir.path(), true, true);
    let state = test_state();
    for (enabled, replies) in [(false, true), (true, false)] {
        write_voice_config(dir.path(), enabled, replies);
        for tool in ["voice", "telegram", "remote"] {
            assert!(
                !tool_names(&merged_tool_definitions(&state, None, None))
                    .iter()
                    .any(|name| name == tool)
            );
            let schema = handle_get_tool_schema(&state, &serde_json::json!({"tool_name": tool}));
            assert!(schema.get("error").is_some(), "{tool}: {schema}");
            let search =
                handle_search_tools(&state, &serde_json::json!({"query":tool,"limit":100}));
            assert!(
                !search["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["name"] == tool)
            );
            let direct = handle_mcp_tool_call(
                &state,
                loopback_addr(),
                tool,
                &serde_json::json!({"action":"status"}),
                None,
            )
            .await;
            let meta = handle_call_tool(
                &state,
                loopback_addr(),
                &serde_json::json!({"tool_name":tool,"arguments":{"action":"status"}}),
                None,
                None,
            )
            .await;
            assert_eq!(direct, meta);
            assert!(
                direct["error"]
                    .as_str()
                    .unwrap()
                    .contains("is unavailable:"),
                "{tool}: {direct}"
            );
        }
    }
    std::fs::write(dir.path().join("dictation-config.json"), "not json").unwrap();
    assert!(
        !tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "voice")
    );
}

// Catches: saved/configured remote records substitute for authenticated live
// connections, or removing one of two live servers hides the remaining one.
#[tokio::test]
async fn conditional_remote_tracks_live_connections_and_preserves_explicit_denial() {
    let state = test_state();
    state
        .remote
        .force_connected_for_test("first", "http://127.0.0.1:9", None);
    state
        .remote
        .force_connected_for_test("second", "http://127.0.0.1:9", None);
    assert!(
        tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "remote")
    );
    // Settings must distinguish configured availability from a policy denial.
    let remote = |state: &AppState| {
        native_capability_catalog(state)
            .into_iter()
            .find(|c| c["domain"] == "remote")
            .unwrap()
    };
    assert_eq!(remote(&state)["available"], true);
    crate::remote_runtime::teardown(&state, "first");
    assert!(
        tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "remote")
    );
    state
        .config
        .write()
        .mcp_capabilities
        .denied
        .push("remote_deployment".into());
    assert!(
        !tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "remote")
    );
    let denied = handle_mcp_tool_call(
        &state,
        loopback_addr(),
        "remote",
        &serde_json::json!({"action":"preview","connection_id":"second"}),
        None,
    )
    .await;
    assert_eq!(
        denied["error"],
        "Tool 'remote' is disabled by configuration"
    );
    assert_eq!(remote(&state)["available"], true);
    state.config.write().mcp_capabilities.denied.clear();
    crate::remote_runtime::teardown(&state, "second");
    assert_eq!(remote(&state)["available"], false);
    assert!(
        !tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "remote")
    );
}

// Catches: a configured Telegram tool remains hidden or bypasses its caller
// registration authority when the configuration predicate is satisfied.
#[tokio::test]
async fn conditional_configured_telegram_preserves_caller_authority() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let paths = crate::telegram::settings::paths().unwrap();
    std::fs::create_dir(&paths.directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&paths.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for (name, contents) in [
        ("config.json", r#"{"enabled":true,"bot_alias":"test-bot"}"#),
        ("bot.token", "111:fake-one"),
    ] {
        std::fs::write(paths.file(name), contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(paths.file(name), std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
    }
    let state = test_state();
    assert!(
        tool_names(&merged_tool_definitions(&state, None, None))
            .iter()
            .any(|name| name == "telegram")
    );
    let schema = handle_get_tool_schema(&state, &serde_json::json!({"tool_name":"telegram"}));
    assert_eq!(schema["name"], "telegram");
    let result = handle_mcp_tool_call(
        &state,
        loopback_addr(),
        "telegram",
        &serde_json::json!({"action":"register"}),
        None,
    )
    .await;
    assert_eq!(result["error"], "telegram_invalid_state");
}

// Catches: shortening the catalogue drops a rule instead of moving it to help.
// Oracle: descriptions recorded from d4882b0f6c before the catalogue rewrite.
// UI updated verbatim from main 06f115ae14 for the added 2067 Design Mode actions.
// Agent guidance updated for the explicit interrupt delivery contract in 2113.
// Telegram guidance updated for non-owner send and reply routing in 2119.
#[tokio::test]
async fn native_help_preserves_every_displaced_rule_2069() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    write_voice_config(dir.path(), true, true);
    let paths = crate::telegram::settings::paths().unwrap();
    std::fs::create_dir(&paths.directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&paths.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for (name, contents) in [
        ("config.json", r#"{"enabled":true,"bot_alias":"test-bot"}"#),
        ("bot.token", "111:fake-one"),
    ] {
        std::fs::write(paths.file(name), contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(paths.file(name), std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
    }
    let state = test_state();
    state.config.write().mcp_capabilities.denied.clear();
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native-guidance-2069.json")).unwrap();
    let definitions = native_tool_definitions();
    // Compact serialized UTF-8 definition bytes, including metadata, not tokens.
    for definition in definitions.as_array().unwrap() {
        println!(
            "definition_bytes {} {}",
            definition["name"].as_str().unwrap(),
            serde_json::to_vec(definition).unwrap().len()
        );
    }
    let mut failures = Vec::new();
    for (name, original) in recorded.as_object().unwrap() {
        let definition = handle_get_tool_schema(&state, &serde_json::json!({"tool_name":name}));
        let compact = definition["description"].as_str().unwrap();
        if compact.len() >= original.as_str().unwrap().len() {
            failures.push(format!("{name}: description was not shortened"));
        }
        for action in definition["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
        {
            if !compact.contains(&format!("{}:", action.as_str().unwrap())) {
                failures.push(format!("{name}: missing visible action {action}"));
            }
        }
        let reply = handle_mcp_tool_call(
            &state,
            loopback_addr(),
            name,
            &serde_json::json!({"action":"help"}),
            None,
        )
        .await;
        if reply["description"] != *original {
            failures.push(format!(
                "{name}: help lost recorded guidance ({})",
                reply.get("error").unwrap_or(&serde_json::Value::Null)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Catches: the generic help fast path leaks a disabled or unconfigured owner.
#[tokio::test]
async fn native_help_respects_owner_availability_2069() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = test_state();
    state.config.write().mcp_capabilities.denied = vec!["session".into()];
    for (name, reason) in [
        ("session", "disabled by configuration"),
        ("voice", "voice is not configured"),
        ("telegram", "Telegram is not configured"),
    ] {
        let reply = handle_mcp_tool_call(
            &state,
            loopback_addr(),
            name,
            &serde_json::json!({"action":"help"}),
            None,
        )
        .await;
        assert!(
            reply["error"].as_str().unwrap().contains(reason),
            "{name}: {reply}"
        );
        assert!(reply.get("description").is_none());
    }
}

// Catches: bare TUIC `agent` guidance being mistaken for a client's native Agent.
#[test]
fn instructions_name_tools_unambiguously() {
    let recording: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/tool-names-2108.json")).unwrap();
    let ctx = InstructionContext {
        native_tools: tools(&["agent", "session", "repo"]),
        peer_count: 1,
        ..Default::default()
    };
    for (client, key) in [("claude-code", "claude"), ("codex", "codex")] {
        let names = recording["clients"][key]["exposed_tool_names"]
            .as_array()
            .unwrap();
        let agent = names
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find(|name| name.ends_with("__agent") || name.ends_with(".agent"))
            .expect("capture must include the real client's qualified agent tool");
        for collapsed in [false, true] {
            let out = render_mcp_instructions(Some(client), collapsed, (true, true), true, &ctx);
            assert!(
                out.contains(agent),
                "{client} cannot find its exposed {agent}: {out}"
            );
            assert!(
                out.contains("not the native `Agent`"),
                "must distinguish the client built-in: {out}"
            );
            assert!(
                out.contains("protocol names"),
                "short examples need an explicit naming rule: {out}"
            );
        }
    }
}
