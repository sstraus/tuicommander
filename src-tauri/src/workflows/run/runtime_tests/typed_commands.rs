//! Protocol-level tests deliberately use only the pre-WF05 Rust API. The same
//! test-only overlay can run against the pre-change production commit for RED.
use super::*;
use serde_json::{Value, json};
use std::path::Path;

fn command(script: &str) -> Value {
    json!({"version":1,"invocation":{"type":"argv","argv":[if cfg!(windows) {"python"} else {"python3"},"-c",script]},"workspace":"assigned","target":{"type":"local","os":std::env::consts::OS,"arch":std::env::consts::ARCH},"resources":[],"secretRefs":{},"stdinArtifact":null,"timeoutSecs":15,"output":{"type":"exit_status"}})
}
fn workspace(project: &Path, config: &Path, name: &str) -> String {
    if !project.join(".git").exists() {
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "WF05"],
            vec!["config", "user.email", "wf05@example.test"],
            vec!["commit", "--allow-empty", "-qm", "source"],
        ] {
            crate::git_cli::git_cmd(project).args(args).run().unwrap();
        }
    }
    let path = config.join(name);
    crate::git_cli::git_cmd(project)
        .args(["worktree", "add", "-q", "-b", name, path.to_str().unwrap()])
        .run()
        .unwrap();
    path.canonicalize().unwrap().to_string_lossy().into_owned()
}
fn start(
    config: &Path,
    project: &Path,
    story: &str,
    workspace: &str,
    command: Value,
    key: &str,
) -> (RunStore, RunSnapshot) {
    let graph: WorkflowGraph = serde_json::from_value(json!({"nodes":[
        {"id":"start","kind":{"type":"start"}},
        {"id":"command","kind":{"type":"run_command","command":command}},
        {"id":"passed","kind":{"type":"end"}},
        {"id":"failed","kind":{"type":"end"}}
    ],"edges":[{"from":"start","to":"command","outcome":null},{"from":"command","to":"passed","outcome":"pass"},{"from":"command","to":"failed","outcome":"fail"}]})).expect("published typed command graph must decode");
    let published = semantics::publish(project.to_str().unwrap(), graph);
    let store = RunStore::open_at(&config.join("workflow_runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(project.to_str().unwrap(), story, &published, key))
        .unwrap();
    store
        .command(
            &run.id,
            &format!("assign:{key}"),
            RunCommand::AssignWorktree {
                story_id: story.into(),
                path: workspace.into(),
            },
        )
        .unwrap();
    let snapshot = drive_turn(&store, &run.id).unwrap();
    (store, snapshot)
}
fn observations(project: &Path, run: &str) -> Value {
    let action: RunAction =
        serde_json::from_value(json!({"action":"command_executions","run_id":run})).unwrap();
    serde_json::to_value(run_action(project.to_str().unwrap(), action).unwrap()).unwrap()["value"]
        .clone()
}
fn sibling(plan: &str, title: &str) -> String {
    let stories = StoryStore::open().unwrap();
    let project = stories.get_plan(plan).unwrap().project;
    let second_plan = stories
        .create_plan(crate::stories::NewPlan {
            project,
            title: title.into(),
            source: "plan.md".into(),
        })
        .unwrap();
    stories
        .create_story(crate::stories::NewStory {
            plan_id: second_plan.id,
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 2,
            story_type: None,
            origin: crate::stories::StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap()
        .id
}

#[tokio::test]
async fn zero_matched_tests_cannot_pass_required_scope() {
    // catches: graph dispatch bypassing the nonempty test-scope check on exit zero.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let workspace = workspace(project.path(), config.path(), "zero");
    let mut spec = command("print('{\"matched\":0,\"passed\":0,\"failed\":0}')");
    spec["output"] = json!({"type":"test_summary","minimum_matched":1});
    let (store, run) = start(
        config.path(),
        project.path(),
        &story,
        &workspace,
        spec,
        "zero",
    );
    assert!(
        super::super::runtime::effects::drive_effect(&state(config.path()), &store, &run)
            .await
            .unwrap()
    );
    let finished = drive_turn(&store, &run.id).unwrap();
    assert_eq!(
        finished.graph_executions[0]
            .activations
            .last()
            .unwrap()
            .node_id,
        "failed"
    );
    let rows = observations(project.path(), &run.id);
    assert_eq!(
        rows.as_array().unwrap().last().unwrap()["receipt"]["succeeded"],
        false
    );
    assert_eq!(store.replay(&run.id).unwrap(), finished);
}

#[tokio::test]
async fn confirmed_spawn_failure_releases_resource_for_next_run() {
    // catches F5: a known failed spawn being confused with uncertain remote ownership.
    let (config, project, plan, story, _template, _guard) = fixture();
    let workspace_a = workspace(project.path(), config.path(), "no-start");
    let mut spec = command("print('unexpected')");
    spec["invocation"]["argv"] = json!([config.path().join("absent-executor").to_str().unwrap()]);
    spec["resources"] = json!([{"type":"gui","key":"wf05-exclusive"}]);
    let (store, run) = start(config.path(), project.path(), &story, &workspace_a, spec, "no-start");
    let state = state(config.path());
    super::super::runtime::effects::drive_effect(&state, &store, &run).await.expect("known no-start yields failed receipt");
    let first = observations(project.path(), &run.id);
    let receipt = &first.as_array().unwrap().last().unwrap()["receipt"];
    assert_eq!(receipt["succeeded"], false);
    assert_eq!(receipt["stopped"], true);
    assert_eq!(receipt["released"], true);
    let second_story = sibling(&plan, "Replacement");
    let workspace_b = workspace(project.path(), config.path(), "replacement");
    let mut second = command("print('replacement started')");
    second["resources"] = json!([{"type":"gui","key":"wf05-exclusive"}]);
    let (store, replacement) = start(config.path(), project.path(), &second_story, &workspace_b, second, "replacement");
    super::super::runtime::effects::drive_effect(&state, &store, &replacement).await.unwrap();
    assert_eq!(observations(project.path(), &replacement.id).as_array().unwrap().last().unwrap()["receipt"]["succeeded"], true);
}

#[tokio::test]
async fn linux_receipt_cannot_discharge_native_mac_requirement() {
    // catches: force-rb admission accepting native proof from the wrong real host platform.
    let (config, project, plan, story, _template, _guard) = fixture();
    let workspace_a = workspace(project.path(), config.path(), "native-proof");
    let spec = command("print('native proof')");
    let (store, run) = start(
        config.path(),
        project.path(),
        &story,
        &workspace_a,
        spec.clone(),
        "native",
    );
    let state = state(config.path());
    super::super::runtime::effects::drive_effect(&state, &store, &run)
        .await
        .unwrap();
    let rows = observations(project.path(), &run.id);
    let native_effect = rows.as_array().unwrap().last().unwrap()["identity"]["effect"]
        .as_str()
        .unwrap();
    let second_story = sibling(&plan, "Requires native target");
    let workspace_b = workspace(project.path(), config.path(), "wrong-platform");
    let mut second = spec;
    // On rb this is the required Linux -> macOS refusal; the Mac checks the reciprocal mismatch.
    second["target"] = json!({"type":"rb_after_native","native_effect":native_effect,"os":if std::env::consts::OS=="macos" {"linux"} else {"macos"},"arch":std::env::consts::ARCH});
    let (store, forced) = start(
        config.path(),
        project.path(),
        &second_story,
        &workspace_b,
        second,
        "forced",
    );
    let error = super::super::runtime::effects::drive_effect(&state, &store, &forced)
        .await
        .unwrap_err();
    assert!(
        error.contains("lacks matching native artifact/command proof"),
        "{error}"
    );
    assert!(
        observations(project.path(), &forced.id)
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["receipt"].is_null())
    );
}

#[tokio::test]
async fn freeze_blocks_new_admission_not_inspection() {
    // catches: freeze starting a new command or denying read-only history inspection.
    // Isolate the policy environment in an owned child, never the concurrent test process.
    if std::env::var_os("WF05_FREEZE_CHILD").is_none() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let freeze = root.path().join("BUILD_FREEZE");
        std::fs::write(&freeze, "allowed: someone-else\n").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","workflows::run::runtime_tests::typed_commands::freeze_blocks_new_admission_not_inspection","--nocapture"]).env("WF05_FREEZE_CHILD","1").env("RB_BUILD_FREEZE",&freeze).output().unwrap();
        assert!(
            output.status.success(),
            "child freeze check: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let (config, project, _plan, story, _template, _guard) = fixture();
    let workspace = workspace(project.path(), config.path(), "frozen");
    let (store, run) = start(
        config.path(),
        project.path(),
        &story,
        &workspace,
        command("print('admitted after thaw')"),
        "freeze",
    );
    let state = state(config.path());
    assert!(
        super::super::runtime::effects::drive_effect(&state, &store, &run)
            .await
            .is_err()
    );
    let rows = observations(project.path(), &run.id);
    assert_eq!(rows.as_array().unwrap().last().unwrap()["state"], "queued");
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|row| row["receipt"].is_null())
    );
    std::fs::write(
        std::env::var_os("RB_BUILD_FREEZE").unwrap(),
        "allowed: tuic-workflow\n",
    )
    .unwrap();
    super::super::runtime::effects::drive_effect(&state, &store, &run)
        .await
        .unwrap();
    let finished = drive_turn(&store, &run.id).unwrap();
    assert_eq!(
        finished.graph_executions[0]
            .activations
            .last()
            .unwrap()
            .node_id,
        "passed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn restarted_paused_remote_command_reconciles_without_replay_or_clean_workspace() {
    // catches F1: restart's generic Uncertain fence preventing remote stop reconciliation.
    if std::env::var_os("WF05_RECOVERY_CHILD").is_none() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let adapters = root.path().join("adapters");
        for directory in ["build", "remote"] { std::fs::create_dir_all(adapters.join(directory)).unwrap(); }
        let mut lookup: Value = serde_json::from_str(include_str!("../../command/fixtures/rb-lookup.json")).unwrap();
        lookup["run_dir"] = json!(root.path().to_str().unwrap());
        std::fs::write(root.path().join("lookup.json"), serde_json::to_vec(&lookup).unwrap()).unwrap();
        std::fs::write(root.path().join("manifest.z"), b"retained source manifest").unwrap();
        // Replay only the recorded lookup boundary; the executor admission/start boundary is real.
        let wrapper = "#!/usr/bin/env python3\nimport os,pathlib\nr=pathlib.Path(os.environ['WF05_RECOVERY_CHILD'])\nwith open(r/'starts','a') as f: f.write('start\\n')\npathlib.Path(os.environ['RB_HANDLE_FILE']).write_text('{}')\n";
        let rb = "#!/usr/bin/env python3\nimport os,pathlib\nprint((pathlib.Path(os.environ['WF05_RECOVERY_CHILD'])/'lookup.json').read_text())\n";
        use std::os::unix::fs::PermissionsExt;
        for (path, text) in [(adapters.join("build/build-slot.sh"), wrapper), (adapters.join("remote/rb"), rb)] {
            std::fs::write(&path, text).unwrap(); std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "workflows::run::runtime_tests::typed_commands::restarted_paused_remote_command_reconciles_without_replay_or_clean_workspace", "--nocapture"])
            .env("WF05_RECOVERY_CHILD", root.path()).env("TUIC_WORKFLOW_ADAPTER_ROOT", adapters)
            .env("RB_BUILD_FREEZE", root.path().join("no-freeze")).output().unwrap();
        assert!(output.status.success(), "{} {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    let (config, project, plan, story, _template, _guard) = fixture();
    let path = workspace(project.path(), config.path(), "remote-recovery");
    let mut spec = command("print('not replayed')"); spec["target"] = json!({"type":"rb"});
    spec["resources"] = json!([{"type":"gui","key":"recovery-exclusive"}]);
    let (store, run) = start(config.path(), project.path(), &story, &path, spec, "recover");
    let state = state(config.path());
    assert!(super::super::runtime::effects::drive_effect(&state, &store, &run).await.is_err());
    drop(store);
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let paused = store.reconcile_after_restart(&run.id).unwrap();
    assert_eq!(paused.status, RunStatus::Paused);
    assert!(paused.effects.iter().any(|e| e.state == EffectState::Uncertain));
    std::fs::write(Path::new(&path).join("dirty-after-restart"), "source moved").unwrap();
    assert!(super::super::runtime::effects::drive_effect(&state, &store, &paused).await.unwrap(), "paused recovery must make durable progress");
    let recovered = store.snapshot(&run.id).unwrap();
    assert!(recovered.effects.iter().all(|e| e.state != EffectState::Uncertain));
    let rows = observations(project.path(), &run.id);
    let receipt = &rows.as_array().unwrap().last().unwrap()["receipt"];
    assert_eq!(receipt["succeeded"], false); assert_eq!(receipt["stopped"], true); assert_eq!(receipt["released"], true);
    assert!(receipt["reason"].as_str().unwrap().contains("lost command output"));
    let root = std::path::PathBuf::from(std::env::var_os("WF05_RECOVERY_CHILD").unwrap());
    assert_eq!(std::fs::read_to_string(root.join("starts")).unwrap(), "start\n");
    // A subsequent effect can acquire the same named resource without replaying the first.
    let second_story = sibling(&plan, "After recovery");
    let second_path = workspace(project.path(), config.path(), "after-recovery");
    let mut spec = command("print('released')"); spec["resources"] = json!([{"type":"gui","key":"recovery-exclusive"}]);
    let (store, next) = start(config.path(), project.path(), &second_story, &second_path, spec, "after-recovery");
    assert!(super::super::runtime::effects::drive_effect(&state, &store, &next).await.unwrap());
}
