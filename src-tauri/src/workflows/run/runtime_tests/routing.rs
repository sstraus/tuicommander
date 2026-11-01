//! Policy is captured at start, never re-read during retry or replay.
use super::*;

pub(super) fn policy(model: &str) -> crate::workflows::AgentPolicyInput {
    crate::workflows::AgentPolicyInput {
        source: "explicit fixture operator policy".into(),
        source_revision: "fixture-1".into(),
        routes: vec![crate::workflows::RoleRoute {
            role: AgentRole::Implementer,
            execution: crate::workflows::AgentExecution::Acp {
                profile: "worker".into(),
                model: model.into(),
                effort: "high".into(),
            },
            acp_runtime: Some(crate::workflows::AcpRuntimeRoute {
                executable: std::env::current_exe().unwrap(),
                machine_profile: "fixture".into(),
            }),
            model: model.into(),
            effort: "high".into(),
            capabilities: vec!["story_read".into(), "story_report".into()],
            scope: crate::workflows::AgentScope::IsolatedWorkspace,
            output_contract_version: 1,
            default_for_role: true,
        }],
    }
}

// Catches: retries/replay silently adopting the latest host policy, incomplete
// model/effort/runtime/tool/scope pins, or an invalid import overwriting policy.
#[test]
fn run_routing_is_immutable_across_policy_edit_retry_and_replay() {
    let (config, project, _plan, story, _template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let published = definition(project, false, true);
    let definitions = WorkflowStore::open().unwrap();
    let first_policy = definitions
        .import_agent_policy(0, policy("model-a"))
        .unwrap();
    let runs = RunStore::open_at(&config.path().join("routing.sqlite3")).unwrap();
    let request = request(project, &story, &published, "routing-start");
    let initial = runs.start_graph_run(&request).unwrap();
    let contract = initial.graph_executions[0].contract.as_ref().unwrap();
    assert_eq!(contract.agent_policy.as_ref().unwrap(), &first_policy);
    let route = &contract.agent_policy.as_ref().unwrap().routes[0].route;
    assert_eq!(route.model, "model-a");
    assert_eq!(route.effort, "high");
    assert_eq!(route.capabilities, ["story_read", "story_report"]);
    assert_eq!(route.output_contract_version, 1);
    assert_eq!(contract.policy_imports[0].sha256, first_policy.sha256);
    let newer = definitions
        .import_agent_policy(1, policy("model-b"))
        .unwrap();
    assert_ne!(newer.sha256, first_policy.sha256);
    assert_eq!(runs.start_graph_run(&request).unwrap(), initial);
    assert_eq!(runs.replay(&initial.id).unwrap(), initial);
    let reopened = RunStore::open_at(&config.path().join("routing.sqlite3")).unwrap();
    assert_eq!(reopened.snapshot(&initial.id).unwrap(), initial);
    assert_eq!(reopened.start_graph_run(&request).unwrap(), initial);
    let mut invalid = policy("model-c");
    invalid.routes[0].effort = "low".into();
    assert!(
        definitions
            .import_agent_policy(2, invalid)
            .unwrap_err()
            .contains("exact profile/model/effort")
    );
    assert_eq!(definitions.agent_policy().unwrap(), Some(newer));
    assert!(definitions.import_agent_policy(1, policy("stale")).is_err());
    assert!(
        first_policy
            .select(AgentRole::Implementer, None, &["publication".into()])
            .is_err()
    );
    let cancelled = runs
        .command(&initial.id, "cancel-pinned", RunCommand::Cancel)
        .unwrap()
        .snapshot;
    assert_eq!(runs.start_graph_run(&request).unwrap(), cancelled);
    assert_eq!(runs.replay(&initial.id).unwrap(), cancelled);
}

// Catches: a friendly PTY profile claiming a different model/effort, hash-map
// iteration causing false drift, or a changed argv silently changing a run.
#[test]
fn pty_policy_requires_explicit_settings_and_detects_real_drift() {
    let (_config, _project, _plan, _story, _template, _guard) = fixture();
    let base = crate::config::load_agents_config();
    let desired = serde_json::from_value(serde_json::json!({"agents":{"claude":{
        "run_configs":[{"name":"pinned","command":"claude","model":"model-a","args":["--effort","high"]}]
    }}})).unwrap();
    crate::config::save_agents_config(base, desired).unwrap();
    let mut input = policy("model-a");
    input.routes[0].execution = crate::workflows::AgentExecution::Pty {
        profile: "pinned".into(),
    };
    input.routes[0].acp_runtime = None;
    let definitions = WorkflowStore::open().unwrap();
    let imported = definitions.import_agent_policy(0, input.clone()).unwrap();
    for _ in 0..4 {
        imported.routes[0].validate_live_settings().unwrap();
    }
    let mut bad_model = input.clone();
    bad_model.routes[0].model = "different-model".into();
    assert!(
        definitions
            .import_agent_policy(1, bad_model)
            .unwrap_err()
            .contains("explicit model")
    );
    let mut bad_effort = input;
    bad_effort.routes[0].effort = "low".into();
    assert!(
        definitions
            .import_agent_policy(1, bad_effort)
            .unwrap_err()
            .contains("effort")
    );
    let base = crate::config::load_agents_config();
    let mut changed = base.clone();
    changed.agents.get_mut("claude").unwrap().run_configs[0]
        .args
        .push("--changed".into());
    crate::config::save_agents_config(base, changed).unwrap();
    assert!(
        imported.routes[0]
            .validate_live_settings()
            .unwrap_err()
            .contains("differ")
    );
    assert_eq!(definitions.agent_policy().unwrap(), Some(imported));
}

// Catches: unrelated profile and idle-close edits refusing an unchanged route.
#[test]
fn unrelated_settings_do_not_refuse_a_pinned_route() {
    let (_config, _project, _plan, _story, _template, _guard) = fixture();
    let base = crate::config::load_agents_config();
    let desired = serde_json::from_value(serde_json::json!({"agents":{"claude":{
        "run_configs":[{"name":"pinned","command":"claude","model":"model-a","args":["--effort","high"]},
                       {"name":"other","command":"claude","model":"other-model"}]
    }}})).unwrap();
    crate::config::save_agents_config(base, desired).unwrap();
    let mut input = policy("model-a");
    input.routes[0].execution = crate::workflows::AgentExecution::Pty {
        profile: "pinned".into(),
    };
    input.routes[0].acp_runtime = None;
    let imported = WorkflowStore::open()
        .unwrap()
        .import_agent_policy(0, input)
        .unwrap();
    for edit in [
        "other_profile",
        "other_agent",
        "idle_close",
        "default_choice",
    ] {
        let base = crate::config::load_agents_config();
        let mut changed = base.clone();
        match edit {
            "other_profile" => {
                changed.agents.get_mut("claude").unwrap().run_configs[1].command =
                    "unrelated-binary".into()
            }
            "other_agent" => {
                changed
                    .agents
                    .insert("gemini".into(), crate::config::AgentSettings::default());
            }
            "idle_close" => {
                changed.hibernate_idle_minutes += 1;
                changed.abandoned_shell_hours += 1;
                changed.agents.get_mut("claude").unwrap().idle_close_minutes += 1;
            }
            "default_choice" => {
                changed.agents.get_mut("claude").unwrap().run_configs[0].is_default = true
            }
            _ => unreachable!(),
        }
        crate::config::save_agents_config(base, changed).unwrap();
        assert!(
            imported.routes[0].validate_live_settings().is_ok(),
            "unrelated edit refused route: {edit}"
        );
    }
    // Catches: narrowing the digest also dropping selected env or shared launch text.
    for edit in ["selected_env", "common_instructions", "native_status"] {
        let base = crate::config::load_agents_config();
        let mut changed = base.clone();
        match edit {
            "selected_env" => {
                changed.agents.get_mut("claude").unwrap().run_configs[0]
                    .env
                    .insert("WF04_LAUNCH".into(), "changed".into());
            }
            "common_instructions" => {
                changed.launch_instructions = Some("Additional pinned launch requirement".into())
            }
            "native_status" => {
                changed
                    .agents
                    .get_mut("claude")
                    .unwrap()
                    .native_status_signals = Some(false)
            }
            _ => unreachable!(),
        }
        crate::config::save_agents_config(base.clone(), changed).unwrap();
        assert!(
            imported.routes[0].validate_live_settings().is_err(),
            "effective edit was missed: {edit}"
        );
        let current = crate::config::load_agents_config();
        crate::config::save_agents_config(current, base).unwrap();
    }
}
