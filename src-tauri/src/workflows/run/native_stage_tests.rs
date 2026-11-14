use super::super::check::{clean_artifact, git_output, staged_artifact};
use super::super::native_agent_test_support::{invocation, role_run_with};
use super::super::native_stage::{StageFile, StageRequest, stage};
use super::super::native_test_support::Fixture;
use super::super::neutral_effects::{
    EffectCommand, EffectObservation, EffectOutcome, NeutralEffect,
};
use super::super::neutral_execution::NeutralResult;
use super::super::neutral_model_tools::{
    ModelSession, ModelToolBinding, ModelToolCall, ModelToolPolicy,
};
use super::super::neutral_runtime::NeutralHost;
use serde_json::json;
use std::collections::BTreeMap;

fn request(files: &[(&str, Option<&str>)]) -> StageRequest {
    StageRequest {
        files: files
            .iter()
            .map(|(path, content)| StageFile {
                path: (*path).into(),
                content: content.map(Into::into),
            })
            .collect(),
    }
}

// Catches a path or symlink escaping the workspace, foreign dirt being staged and
// attributed, a repeated path, and a receipt tree that differs from the index.
#[cfg(unix)]
#[test]
fn native_stage_writes_and_stages_only_listed_plain_workspace_paths() {
    let f = Fixture::new();
    let root = f.context.workspace_path();
    for path in [
        "/etc/passwd",
        "../outside",
        ".git/config",
        "a/../b",
        "",
        "C:\\x",
        "a\\b",
    ] {
        let refused = stage(&f.context, &request(&[(path, Some("x"))])).unwrap_err();
        assert!(
            refused.contains("plain workspace path"),
            "{path}: {refused}"
        );
    }
    let twice = stage(&f.context, &request(&[("a", Some("1")), ("a", Some("2"))]));
    assert!(twice.unwrap_err().contains("twice"));
    let outside = f.dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    git_output(root, &["add", "link"]).unwrap();
    tuic_git::git::git_commit_blocking(
        root.to_string_lossy().into_owned(),
        "link".into(),
        Some(false),
    )
    .unwrap();
    let base = git_output(root, &["rev-parse", "HEAD"]).unwrap();
    let refused = stage(&f.context, &request(&[("link/escape.txt", Some("x"))])).unwrap_err();
    assert!(refused.contains("symlink"), "{refused}");
    assert!(!outside.join("escape.txt").exists());
    std::fs::write(root.join("dirt.txt"), "foreign").unwrap();
    let refused = stage(&f.context, &request(&[("src/new.txt", Some("x"))])).unwrap_err();
    assert!(refused.contains("unstaged or untracked"), "{refused}");
    assert!(!root.join("src/new.txt").exists());
    std::fs::remove_file(root.join("dirt.txt")).unwrap();
    let refused = stage(&f.context, &request(&[("never-tracked.txt", None)])).unwrap_err();
    assert!(refused.contains("untracked path"), "{refused}");
    let receipt = stage(
        &f.context,
        &request(&[("src/new.txt", Some("hello")), ("artifact.txt", None)]),
    )
    .unwrap();
    assert_eq!(receipt.base, base);
    assert_eq!(receipt.tree, git_output(root, &["write-tree"]).unwrap());
    assert_eq!(receipt.paths, vec!["src/new.txt", "artifact.txt"]);
    assert_eq!(
        git_output(root, &["diff", "--cached", "--name-status"]).unwrap(),
        "D\tartifact.txt\nA\tsrc/new.txt"
    );
    assert_eq!(staged_artifact(root).unwrap(), (base, receipt.tree));
}

// Catches staged work being refused as a dirty artifact, a later unstaged edit
// still passing as the staged tree, and the clean-only rule silently returning.
#[test]
fn native_staged_artifact_is_current_until_an_unstaged_edit() {
    let f = Fixture::new();
    let root = f.context.workspace_path();
    let receipt = stage(&f.context, &request(&[("artifact.txt", Some("staged"))])).unwrap();
    assert!(clean_artifact(root).is_err());
    f.context
        .require_artifact(&receipt.base, &receipt.tree)
        .unwrap();
    let head_tree = git_output(root, &["rev-parse", "HEAD^{tree}"]).unwrap();
    assert!(
        f.context
            .require_artifact(&receipt.base, &head_tree)
            .is_err()
    );
    std::fs::write(root.join("artifact.txt"), "edited after staging").unwrap();
    let stale = f
        .context
        .require_artifact(&receipt.base, &receipt.tree)
        .unwrap_err();
    assert!(stale.contains("unstaged or untracked"), "{stale}");
}

// Catches the implementer being unable to write through its exact host tool, the
// tool needing a shell or approval, and its report dropping the staged work.
#[tokio::test]
async fn native_implementer_stages_through_its_host_tool_and_reports_the_staged_tree() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let root = f.context.workspace_path();
    let catalog = crate::workflows::native_operation_catalog().unwrap();
    let tool = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.workspace.stage")
        .unwrap();
    let policy = ModelToolPolicy {
        tools: BTreeMap::from([(
            "stage_files".into(),
            ModelToolBinding {
                descriptor: tool.clone(),
                instance: crate::workflows::catalog::ActivityInstance {
                    descriptor: tool.id.clone(),
                    config: BTreeMap::new(),
                    execution_policy: "native".into(),
                },
            },
        )]),
        max_calls: 2,
        max_concurrency: 1,
    };
    let runtime = crate::acp::EgoAcpConfig {
        executable: std::env::current_exe().unwrap(),
        profile: "exact-profile".into(),
        master_prompt: String::new(),
        ai_chat: false,
    };
    let run = role_run_with(
        &f,
        "implementer",
        &runtime,
        super::super::NativeCaller::Agent("trusted".into()),
        Some(policy),
    );
    let invocation = invocation(&run);
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    let key = effect.key.clone();
    for command in [
        EffectCommand::Prepare {
            effect: Box::new(effect.clone()),
        },
        EffectCommand::Dispatch { key: key.clone() },
        EffectCommand::BindModel {
            key: key.clone(),
            session: ModelSession {
                connection: "connection".into(),
                session: "implementer".into(),
                generation: 1,
            },
        },
    ] {
        f.owner
            .neutral_effect_command(&run.run.id, command)
            .unwrap();
    }
    let parent = f.owner.store.neutral_execution(&run.run.id).unwrap();
    let call = ModelToolCall {
        parent: key.clone(),
        session: parent.effect(&key).unwrap().model_session.clone().unwrap(),
        id: "stage-1".into(),
        name: "stage_files".into(),
    };
    let arguments = BTreeMap::from([(
        "request".into(),
        json!({"files":[{"path":"src/feature.txt","content":"implemented"}]}),
    )]);
    let child = f.owner.start_model_tool(call, arguments).unwrap();
    let settled = loop {
        let changed = f.owner.neutral_effect_updates.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let snapshot = f.owner.store.neutral_execution(&run.run.id).unwrap();
        let effect = snapshot.effect(&child.key).unwrap().clone();
        if effect.settled() {
            break effect;
        }
        changed.await;
    };
    assert!(
        settled.question.is_none(),
        "stage must not wait for an approval"
    );
    let Some(EffectObservation {
        outcome:
            EffectOutcome::Completed {
                result: NeutralResult::Completed { outputs, .. },
            },
        ..
    }) = &settled.observation
    else {
        panic!("{settled:?}")
    };
    let tree = outputs["receipt"]["tree"].as_str().unwrap().to_owned();
    assert_eq!(tree, git_output(root, &["write-tree"]).unwrap());
    let bound = f.host.binding(&effect.invocation).unwrap();
    let report = super::super::native_agent::report(
        &bound,
        &effect,
        BTreeMap::from([("next".into(), json!(7))]),
    )
    .unwrap();
    assert_eq!(report["observed_artifact"]["tree"], tree);
    std::fs::write(root.join("src/feature.txt"), "unstaged edit").unwrap();
    assert!(
        super::super::native_agent::report(
            &bound,
            &effect,
            BTreeMap::from([("next".into(), json!(7))])
        )
        .is_err()
    );
}
