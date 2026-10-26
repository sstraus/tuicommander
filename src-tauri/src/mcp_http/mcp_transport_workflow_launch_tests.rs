// Catches: workflow launch applies detached-orphan removal rules to a registered branch checkout.
#[tokio::test]
async fn workflow_launch_accepts_registered_branch_worktree() {
    use crate::workflows::{RunCommand, RunLimits, RunStore, WorkflowKind, WorkflowStore};

    let (root, repo, _) = branch_delete_fixture();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let worktree = root.path().join("coordinator");
    crate::git_cli::git_cmd(&repo)
        .args([
            "worktree",
            "add",
            "-b",
            "coordinator",
            worktree.to_str().unwrap(),
        ])
        .run()
        .unwrap();
    let repo = repo.canonicalize().unwrap();
    let worktree = worktree.canonicalize().unwrap();
    let project = repo.to_str().unwrap();
    crate::config::save_repositories_request(serde_json::json!({
        "mutationVersion": 1, "groups": [],
        "repos": [{"id": project, "before": null, "after": {
            "path": project, "workspaces": {"coordinator": {
                "worktreePath": worktree, "parentRepoPath": project
            }}
        }}]
    }))
    .unwrap();
    let stories = crate::stories::StoryStore::open().unwrap();
    let plan = stories
        .create_plan(crate::stories::NewPlan {
            project: project.into(),
            title: "Launch branch coordinator".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    std::fs::write(
        worktree.join("plan.md"),
        "Coordinate the registered branch.\n",
    )
    .unwrap();
    let definition = WorkflowStore::open()
        .unwrap()
        .seed_templates(project)
        .unwrap()
        .into_iter()
        .find(|draft| draft.kind == WorkflowKind::Plan)
        .unwrap();
    let store = RunStore::open().unwrap();
    let run = store
        .start_plan(project, &plan.id, &definition.id, 1, RunLimits::default())
        .unwrap();
    let started = store
        .command(
            &run.id,
            "coordinate",
            RunCommand::StartPlanAgent {
                node_id: "coordinate".into(),
            },
        )
        .unwrap();
    let attempt_id = &started.snapshot.attempts[0].id;
    // An existing shell blocks on input: no model service, generated executable or credentials.
    let config = serde_json::from_value(serde_json::json!({"agents": {"amp": {
        "prevent_alt_screen": false,
        "skip_trust_dialog": false, "native_status_signals": false,
        "run_configs": [{"name": "Workflow shell", "command": "/bin/sh",
            "args": ["-c", "read -r hold"], "is_default": true}]
    }}}))
    .unwrap();
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = test_state();
    let mcp_sid = "workflow-launch-mcp";
    register_peer(&state, TEST_UUID_A, "workflow-parent", mcp_sid);
    state
        .mcp
        .to_session
        .insert(mcp_sid.into(), TEST_UUID_A.into());
    insert_managed_test_session(&state, "workflow-caller", worktree.to_str().unwrap());
    state.bind_live_pty(TEST_UUID_A, "workflow-caller");
    crate::repo_watcher::start_watching(project, &state).unwrap();
    let args = serde_json::json!({"action":"launch", "input": {
        "runId": run.id, "attemptId": attempt_id,
        "worktreePath": worktree, "agentType": "Workflow shell"
    }});
    // Catches: launch loosens the main-checkout and in-progress-operation guards.
    let mut main_args = args.clone();
    main_args["input"]["worktreePath"] = serde_json::json!(repo);
    let rejected = handle_mcp_tool_call_with_context(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        "workflow",
        &main_args,
        Some(mcp_sid),
        None,
    )
    .await;
    assert!(rejected.get("error").is_some(), "{rejected}");
    let merge_head = crate::git_cli::git_cmd(&worktree)
        .args(["rev-parse", "--git-path", "MERGE_HEAD"])
        .run()
        .unwrap()
        .stdout
        .trim()
        .to_owned();
    std::fs::write(&merge_head, "unfinished merge\n").unwrap();
    let rejected = handle_mcp_tool_call_with_context(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        "workflow",
        &args,
        Some(mcp_sid),
        None,
    )
    .await;
    std::fs::remove_file(&merge_head).unwrap();
    assert!(
        rejected["error"].as_str().unwrap().contains("operation"),
        "{rejected}"
    );
    assert_eq!(
        store.snapshot(&run.id).unwrap().effects,
        [] as [crate::workflows::EffectIntent; 0]
    );
    // Exercise the real async transport boundary, which owns the blocking
    // workflow worker; the transaction awaits must not run on the test executor.
    let launched = handle_mcp_tool_call_with_context(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        "workflow",
        &args,
        Some(mcp_sid),
        None,
    )
    .await;
    assert!(launched.get("error").is_none(), "{launched}");
    let session_id = launched["session_id"].as_str().unwrap();
    let launched_cwd = state
        .session_maps
        .sessions
        .get(session_id)
        .unwrap()
        .lock()
        .cwd
        .clone();
    // Stop our child before inspecting the durable binding, including on assertion failures.
    let stopped = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
    assert!(stopped.get("error").is_none(), "{stopped}");
    assert_eq!(launched_cwd.as_deref(), worktree.to_str());
    let snapshot = store.snapshot(&run.id).unwrap();
    let binding = snapshot.attempts[0].agent.as_ref().unwrap();
    assert_eq!(binding.session_id, session_id);
    assert_eq!(binding.prompt_sha256.len(), 64);
    assert!(
        binding
            .audit_preview
            .contains("Coordinate the registered branch.")
    );
    assert_eq!(
        snapshot.effects[0].state,
        crate::workflows::EffectState::Succeeded
    );
}
