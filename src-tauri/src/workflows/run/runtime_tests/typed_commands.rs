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
async fn lease_loss_keeps_effect_and_slot_uncertain() {
    // catches: an unknown admitted effect releasing a resource and spawning replacement work.
    // The missing executor injects loss at the admission/start boundary without a fake rb service.
    let (config, project, plan, story, _template, _guard) = fixture();
    let workspace_a = workspace(project.path(), config.path(), "uncertain");
    let mut spec = command("print('unexpected')");
    spec["invocation"]["argv"] = json!([config.path().join("absent-executor").to_str().unwrap()]);
    spec["resources"] = json!([{"type":"gui","key":"wf05-exclusive"}]);
    let (store, run) = start(
        config.path(),
        project.path(),
        &story,
        &workspace_a,
        spec,
        "lost",
    );
    let state = state(config.path());
    assert!(
        super::super::runtime::effects::drive_effect(&state, &store, &run)
            .await
            .is_err()
    );
    drop(store);
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    assert!(
        super::super::runtime::effects::drive_effect(&state, &store, &run)
            .await
            .is_err()
    );
    let first = observations(project.path(), &run.id);
    assert_eq!(
        first
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["state"] == "admitted")
            .count(),
        1
    );
    assert_eq!(
        first.as_array().unwrap().last().unwrap()["state"],
        "uncertain"
    );
    let second_story = sibling(&plan, "Replacement");
    let workspace_b = workspace(project.path(), config.path(), "replacement");
    let mut second = command("print('must not start')");
    second["resources"] = json!([{"type":"gui","key":"wf05-exclusive"}]);
    let (store, replacement) = start(
        config.path(),
        project.path(),
        &second_story,
        &workspace_b,
        second,
        "replacement",
    );
    assert!(
        super::super::runtime::effects::drive_effect(&state, &store, &replacement)
            .await
            .is_err()
    );
    let rows = observations(project.path(), &replacement.id);
    assert_eq!(rows.as_array().unwrap().last().unwrap()["state"], "queued");
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|row| row["receipt"].is_null())
    );
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
