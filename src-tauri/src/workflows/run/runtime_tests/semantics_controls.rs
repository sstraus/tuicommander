use super::*;

#[test]
fn daemon_loop_exhaustion_and_pause_resume_keep_the_pinned_cap() {
    // catches: daemon retries resetting the node counter or using the run-wide limit.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let published = publish(
        project.path().to_str().unwrap(),
        WorkflowGraph {
            nodes: vec![
                Node {
                    id: "start".into(),
                    kind: NodeKind::Start,
                },
                Node {
                    id: "loop".into(),
                    kind: NodeKind::Loop { max_iterations: 2 },
                },
                Node {
                    id: "end".into(),
                    kind: NodeKind::End,
                },
            ],
            edges: vec![
                edge("start", "loop", None),
                edge("loop", "loop", Some("repeat")),
                edge("loop", "end", Some("exhausted")),
            ],
        },
    );
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story,
            &published,
            "loop",
        ))
        .unwrap();
    let graph = &run.graph_executions[0];
    let activation = graph.activations.last().unwrap();
    for (key, transition) in [
        (
            "test:activate",
            GraphTransition::Activate {
                execution_id: graph.id.clone(),
                activation_id: activation.id.clone(),
            },
        ),
        (
            "test:first-repeat",
            GraphTransition::Complete {
                execution_id: graph.id.clone(),
                activation_id: activation.id.clone(),
                outcome: None,
                evidence: None,
            },
        ),
    ] {
        store
            .command(&run.id, key, RunCommand::Graph { transition })
            .unwrap();
    }
    store
        .command(&run.id, "test:pause", RunCommand::Pause)
        .unwrap();
    let current = store.snapshot(&run.id).unwrap();
    let graph = &current.graph_executions[0];
    store
        .command(
            &run.id,
            "test:resume",
            RunCommand::ResumeGraph {
                execution_id: graph.id.clone(),
                activation_id: graph.activations.last().unwrap().id.clone(),
                resolution: "Continue the pinned loop".into(),
            },
        )
        .unwrap();
    let current = drive_turn(&store, &run.id).unwrap();
    assert_eq!(current.loops, 2);
    assert_eq!(current.graph_executions[0].loops[0].repeats, 2);
    assert!(current.graph_executions[0].completed);
    assert_eq!(store.replay(&run.id).unwrap(), current);
}

#[test]
fn manual_pause_excludes_elapsed_time_without_resetting_active_budget() {
    // catches: a manual pause consuming the 24-hour active-run duration.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let published = definition(project.path().to_str().unwrap(), false, false);
    let mut request = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "paused-duration",
    );
    request.limits.max_duration_secs = 1;
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let run = store.start_graph_run(&request).unwrap();
    let pause_at = run.started_ms + 100;
    store
        .command_at(&run.id, "test:pause", RunCommand::Pause, pause_at)
        .unwrap();
    let db = config.path().join("workflow_runs.sqlite3");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut historical = serde_json::to_value(store.snapshot(&run.id).unwrap()).unwrap();
    historical.as_object_mut().unwrap().remove("pausedSinceMs");
    historical
        .as_object_mut()
        .unwrap()
        .remove("pausedDurationMs");
    conn.execute(
        "UPDATE workflow_runs SET snapshot_json=?1 WHERE id=?2",
        rusqlite::params![historical.to_string(), run.id],
    )
    .unwrap();
    drop(conn);
    assert_eq!(
        store.snapshot(&run.id).unwrap().paused_since_ms,
        Some(pause_at)
    );
    let resume_at = pause_at + 86_400_000;
    let graph = &run.graph_executions[0];
    let resumed = store
        .command_at(
            &run.id,
            "test:resume",
            RunCommand::ResumeGraph {
                execution_id: graph.id.clone(),
                activation_id: graph.activations.last().unwrap().id.clone(),
                resolution: "Operator continues".into(),
            },
            resume_at,
        )
        .unwrap()
        .snapshot;
    assert_eq!(resumed.paused_duration_ms, 86_400_000);
    assert_eq!(resumed.paused_since_ms, None);
    assert!(
        store
            .command_at(
                &run.id,
                "test:not-due",
                RunCommand::ExpireDeadline,
                resume_at + 899
            )
            .is_err()
    );
    let expired = store
        .command_at(
            &run.id,
            "test:due",
            RunCommand::ExpireDeadline,
            resume_at + 900,
        )
        .unwrap()
        .snapshot;
    assert_eq!(expired.status, RunStatus::Paused);
    assert!(
        store
            .command_at(
                &run.id,
                "test:expired-resume",
                RunCommand::ResumeGraph {
                    execution_id: graph.id.clone(),
                    activation_id: graph.activations.last().unwrap().id.clone(),
                    resolution: "Cannot extend expired active budget".into(),
                },
                resume_at + 1_000
            )
            .is_err()
    );
    assert_eq!(store.replay(&run.id).unwrap(), expired);
}

#[tokio::test]
async fn notify_retry_keeps_one_durable_notice() {
    // catches: re-entering a Notify node duplicating the repository journal notice.
    let (config, project, _plan, story, _template, _guard) = fixture();
    // Catches raw aliases being used as SQLite keys instead of the canonical owner.
    let project_alias = project.path().join(".");
    let project_path = project_alias.to_str().unwrap();
    let published = publish(
        project_path,
        WorkflowGraph {
            nodes: vec![
                Node {
                    id: "start".into(),
                    kind: NodeKind::Start,
                },
                Node {
                    id: "notify".into(),
                    kind: NodeKind::Notify,
                },
                Node {
                    id: "end".into(),
                    kind: NodeKind::End,
                },
            ],
            edges: vec![edge("start", "notify", None), edge("notify", "end", None)],
        },
    );
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(project_path, &story, &published, "notify"))
        .unwrap();
    let state = state(config.path());
    let current = drive_turn(&store, &run.id).unwrap();
    assert!(drive_effect(&state, &store, &current).await.unwrap());
    let current = store.snapshot(&run.id).unwrap();
    assert!(!drive_effect(&state, &store, &current).await.unwrap());
    let notices = crate::progress::ProgressStore::open()
        .unwrap()
        .list_limited(
            &canonical_owner(project_path),
            &crate::progress::ProgressListInput::default(),
            50,
        )
        .unwrap();
    assert_eq!(
        notices
            .entries
            .iter()
            .filter(|n| n.text.contains(&run.id))
            .count(),
        1
    );
    let current = drive_turn(&store, &run.id).unwrap();
    assert!(current.graph_executions[0].completed);
    assert_eq!(current.effects.len(), 1);
    assert_eq!(current.effects[0].state, EffectState::Succeeded);
    assert_eq!(store.replay(&run.id).unwrap(), current);
}

#[tokio::test]
async fn join_ignores_unselected_arrivals_and_actor_continues_after_turn_budget() {
    // catches: an exclusive Join waiting for every predecessor, or 32 transitions stranding a run.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let published = publish(
        project_path,
        WorkflowGraph {
            nodes: vec![
                Node {
                    id: "start".into(),
                    kind: NodeKind::Start,
                },
                Node {
                    id: "join".into(),
                    kind: NodeKind::Join {},
                },
                Node {
                    id: "loop".into(),
                    kind: NodeKind::Loop { max_iterations: 20 },
                },
                Node {
                    id: "end".into(),
                    kind: NodeKind::End,
                },
            ],
            edges: vec![
                edge("start", "join", None),
                edge("join", "loop", None),
                edge("loop", "join", Some("repeat")),
                edge("loop", "end", Some("exhausted")),
            ],
        },
    );
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let mut request = request(project_path, &story, &published, "join");
    request.limits.max_loops = 30;
    let mut hints = state.event_bus.subscribe();
    let run = state
        .workflow_runtime
        .start_graph(&state, &request)
        .unwrap();
    let store = RunStore::open().unwrap();
    // Completion is state convergence, not a 30-second behavior deadline.
    // Nextest's 120-second outer bound catches a genuinely stranded actor.
    loop {
        hints.recv().await.unwrap();
        let snapshot = store.snapshot(&run.id).unwrap();
        if snapshot.graph_executions[0].completed {
            break;
        }
    }
    let current = store.snapshot(&run.id).unwrap();
    assert_eq!(current.loops, 20);
    assert_eq!(
        current.graph_executions[0]
            .activations
            .iter()
            .filter(|a| a.node_id == "join")
            .count(),
        21
    );
    assert_eq!(store.replay(&run.id).unwrap(), current);
    store
        .command(&run.id, "test:cancel", RunCommand::Cancel)
        .unwrap();
}

#[test]
fn publication_visibly_refuses_nodes_without_an_executor() {
    // catches: a transport publishing unsupported plan nodes, or refusing the implemented Gate.
    let (_config, project, _plan, _story, template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let definitions = WorkflowStore::open().unwrap();
    let plan = definitions.get_draft(&template).unwrap();
    let mut graph = plan.graph;
    for node in &mut graph.nodes {
        if let NodeKind::Agent { role, .. } = &mut node.kind {
            *role = AgentRole::Implementer;
        }
    }
    let plan = definitions
        .update_draft(&plan.id, plan.draft_revision, graph)
        .unwrap();
    let error = crate::workflows::definition_action(
        project,
        crate::workflows::WorkflowAction::Publish {
            id: plan.id,
            expected_revision: plan.draft_revision,
        },
    )
    .unwrap_err();
    assert!(error.contains("not executable") && error.contains("draft"));
    let story = definition(project, true, false);
    let draft = definitions.get_draft(&story.id).unwrap();
    let draft = definitions
        .update_draft(&draft.id, draft.draft_revision, draft.graph)
        .unwrap();
    assert!(
        crate::workflows::definition_action(
            project,
            crate::workflows::WorkflowAction::Publish {
                id: draft.id,
                expected_revision: draft.draft_revision,
            }
        )
        .is_ok()
    );
}

#[tokio::test]
async fn agent_without_required_profile_pauses_with_a_durable_reason() {
    // catches: silently substituting a model, or leaving a reached Agent running without a spawn.
    let (config, project, _plan, story, _template, _guard) = fixture();
    // Catches raw aliases being used as SQLite keys instead of the canonical owner.
    let project_alias = project.path().join(".");
    let project_path = project_alias.to_str().unwrap();
    let published = definition(project_path, false, true);
    // A default install seeds a default Codex profile, so the pause needs two saved profiles and no default.
    crate::config::save_agents_config(
        crate::config::AgentsConfig::default(),
        agents(serde_json::json!({"agents": {
            "codex": {"codex_bypass_migrated": true},
            "claude": {"run_configs": [
                {"name": "alpha", "command": "claude"},
                {"name": "beta", "command": "claude"}
            ]}
        }})),
    )
    .unwrap();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let mut hints = state.event_bus.subscribe();
    let run = state
        .workflow_runtime
        .start_graph(
            &state,
            &request(project_path, &story, &published, "missing-profile"),
        )
        .unwrap();
    let store = RunStore::open().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            hints.recv().await.unwrap();
            if store.snapshot(&run.id).unwrap().status == RunStatus::Paused {
                break;
            }
        }
    })
    .await
    .expect("missing profile did not pause the Agent");
    let current = store.snapshot(&run.id).unwrap();
    assert_eq!(current.attempts.len(), 1);
    assert!(current.attempts[0].agent.is_none());
    assert_eq!(
        current.effects,
        [] as [crate::workflows::run::model::EffectIntent; 0]
    );
    let notices = crate::progress::ProgressStore::open()
        .unwrap()
        .list_limited(
            &canonical_owner(project_path),
            &crate::progress::ProgressListInput::default(),
            50,
        )
        .unwrap();
    assert!(
        notices
            .entries
            .iter()
            .any(|n| n.text.contains(&run.id) && n.text.contains("mark one as default"))
    );
    // catches: a pause whose cause lives only in the progress journal, so Incidents says none is retained.
    let cause = current.pause_cause.as_deref().unwrap();
    assert!(cause.contains("mark one as default"), "{cause}");
    let incidents = crate::workflows::run::incidents::project_incidents(&current, None);
    assert_eq!(incidents[0].cause, cause);
    assert_eq!(store.replay(&run.id).unwrap(), current);
    store
        .command(&run.id, "test:cancel", RunCommand::Cancel)
        .unwrap();
}

fn agents(json: serde_json::Value) -> crate::config::AgentsConfig {
    serde_json::from_value(json).unwrap()
}

#[test]
fn a_default_install_with_one_saved_profile_serves_every_role() {
    // catches: hardcoded sol/sonnet names pausing a run on an install that saved another profile.
    let config = agents(serde_json::json!({"agents": {"claude": {"run_configs": [
        {"name": "mine", "command": "claude"}
    ]}}}));
    for role in [
        AgentRole::Implementer,
        AgentRole::Reviewer,
        AgentRole::Coordinator,
    ] {
        assert_eq!(
            super::super::runtime::effects::resolve_run_profile(&config, &role).unwrap(),
            "mine"
        );
    }
}

#[test]
fn the_default_profile_wins_over_other_saved_profiles_and_named_ones_win_over_it() {
    // catches: picking an arbitrary saved profile; ignoring the legacy sol/sonnet names on configured installs.
    let config = agents(serde_json::json!({"agents": {"claude": {"run_configs": [
        {"name": "alpha", "command": "claude"},
        {"name": "beta", "command": "claude", "is_default": true},
        {"name": "sonnet", "command": "claude"}
    ]}}}));
    use super::super::runtime::effects::resolve_run_profile as resolve;
    assert_eq!(resolve(&config, &AgentRole::Implementer).unwrap(), "beta");
    assert_eq!(resolve(&config, &AgentRole::Reviewer).unwrap(), "sonnet");
}

#[test]
fn several_profiles_without_a_default_say_what_to_do() {
    // catches: guessing a model when the user never chose one.
    let config = agents(serde_json::json!({"agents": {"claude": {"run_configs": [
        {"name": "alpha", "command": "claude"},
        {"name": "beta", "command": "claude"}
    ]}}}));
    let error =
        super::super::runtime::effects::resolve_run_profile(&config, &AgentRole::Implementer)
            .unwrap_err();
    assert!(error.contains("mark one as default"), "{error}");
}

#[test]
fn readiness_names_each_problem_once_for_a_definition_with_agents() {
    // catches: the start dialog saying nothing before a run that cannot reach its first Agent step.
    let (_config, project, _plan, _story, _template, _guard) = fixture();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let problems = super::super::runtime::effects::profile_problems(
        &published,
        &crate::config::AgentsConfig::default(),
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("Settings > Agents"));
}

#[test]
fn a_default_profile_that_bypasses_the_sandbox_is_refused_for_workflows() {
    // catches: workflow agents inheriting a permission-skipping flag from the interactive default.
    let config = agents(serde_json::json!({"agents": {"codex": {"run_configs": [
        {"name": "Codex Default", "command": "codex",
         "args": ["--dangerously-bypass-approvals-and-sandbox"], "is_default": true}
    ]}}}));
    use super::super::runtime::effects::resolve_run_profile as resolve;
    let error = resolve(&config, &AgentRole::Implementer).unwrap_err();
    assert!(
        error.contains("Codex Default") && error.contains("--dangerously-bypass"),
        "{error}"
    );
    // The user choosing that profile for workflows by name is allowed.
    let chosen = agents(serde_json::json!({"agents": {"codex": {"run_configs": [
        {"name": "sol", "command": "codex", "args": ["--dangerously-bypass-approvals-and-sandbox"]}
    ]}}}));
    assert_eq!(resolve(&chosen, &AgentRole::Implementer).unwrap(), "sol");
}

/// What the Start dialog would be told for a published Agent workflow under this saved config.
fn readiness_under(agents_json: serde_json::Value) -> Vec<String> {
    let (_config, project, _plan, _story, _template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let published = definition(project_path, false, true);
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents(agents_json))
        .unwrap();
    match crate::workflows::run_action(
        project_path,
        crate::workflows::RunAction::StartReadiness {
            definition_id: published.id.clone(),
            definition_revision: published.revision,
        },
    )
    .unwrap()
    {
        crate::workflows::RunReply::Readiness(problems) => problems,
        other => panic!("unexpected reply {other:?}"),
    }
}

#[test]
fn readiness_refuses_every_effective_ego_yolo_default() {
    // catches: a fallback that checks two flag spellings but launches ego with yolo through --mode forms or ego_mode.
    for (args, ego_mode) in [
        (serde_json::json!(["--mode=yolo"]), serde_json::json!(null)),
        (
            serde_json::json!(["--mode", "yolo"]),
            serde_json::json!(null),
        ),
        (serde_json::json!([]), serde_json::json!("yolo")),
    ] {
        let problems = readiness_under(serde_json::json!({"agents": {
            "codex": {"codex_bypass_migrated": true},
            "ego": {"ego_mode": ego_mode, "run_configs": [
                {"name": "Ego", "command": "ego", "args": args, "is_default": true}
            ]}
        }}));
        assert!(
            problems
                .iter()
                .all(|p| p.contains("'Ego'") && p.contains("yolo")),
            "{args} {ego_mode}: {problems:?}"
        );
        assert!(!problems.is_empty(), "{args} {ego_mode}");
    }
}

#[test]
fn readiness_refuses_claude_permission_mode_and_codex_full_access_defaults() {
    // catches: fallback agents bypassing permissions via value-taking CLI options instead of --dangerously-*.
    // Options confirmed by installed claude --help and codex --help on 2026-10-23.
    let mut missed = Vec::new();
    for (agent, args) in [
        ("claude", vec!["--permission-mode", "bypassPermissions"]),
        ("claude", vec!["--permission-mode=bypassPermissions"]),
        ("codex", vec!["--sandbox", "danger-full-access"]),
        ("codex", vec!["-s", "danger-full-access"]),
        ("codex", vec!["--sandbox=danger-full-access"]),
    ] {
        let problems = readiness_under(serde_json::json!({"agents": {
            "codex": {"codex_bypass_migrated": true},
            (agent): {"codex_bypass_migrated": true, "run_configs": [
                {"name": "Interactive", "command": agent, "args": args, "is_default": true}
            ]}
        }}));
        if problems.is_empty() || problems.iter().any(|p| !p.contains("'Interactive'")) {
            missed.push(format!("{agent} {args:?}: {problems:?}"));
        }
    }
    assert!(missed.is_empty(), "unsafe defaults accepted: {missed:?}");
}

#[test]
fn readiness_picks_the_one_safe_default_among_per_agent_defaults_and_names_real_ambiguity() {
    // catches: choosing the alphabetically first agent's default as if defaults were global.
    let seeded_and_safe = readiness_under(serde_json::json!({"agents": {
        "codex": {"codex_bypass_migrated": true, "run_configs": [
            {"name": "Codex Default", "command": "codex",
             "args": ["--dangerously-bypass-approvals-and-sandbox"], "is_default": true}]},
        "claude": {"run_configs": [{"name": "Zed", "command": "claude", "is_default": true}]}
    }}));
    assert_eq!(seeded_and_safe, Vec::<String>::new());
    let two_safe = readiness_under(serde_json::json!({"agents": {
        "codex": {"codex_bypass_migrated": true, "run_configs": [
            {"name": "Alpha", "command": "codex", "is_default": true}]},
        "claude": {"run_configs": [{"name": "Zed", "command": "claude", "is_default": true}]}
    }}));
    assert!(
        two_safe[0].contains("Alpha") && two_safe[0].contains("Zed"),
        "{two_safe:?}"
    );
}
