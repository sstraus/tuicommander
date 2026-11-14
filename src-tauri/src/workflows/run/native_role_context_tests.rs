use super::native_test_support::Fixture;
use super::neutral_effects::EffectOutcome;
use super::neutral_execution::NeutralResult;
use crate::stories::{NewPlan, NewStory, StoryOrigin, StoryStore};
use serde_json::json;

// Catches identifiers-only role prompts: actual backend criteria and Git diff
// must reach the tool-less reviewer's literal request, without write authority.
#[tokio::test]
async fn native_role_context_reads_actual_content_and_bounds_role_request() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let backend = StoryStore::open().unwrap();
    let project = f.context.workspace_path().to_string_lossy().into_owned();
    let plan = backend
        .create_plan(NewPlan {
            project,
            title: "Actual plan".into(),
            source: "Preserve the exact artifact bytes".into(),
        })
        .unwrap();
    let story = backend
        .create_story(NewStory {
            plan_id: plan.id.clone(),
            title: "Actual story".into(),
            criteria: vec!["Publish only independently reviewed bytes".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec!["artifact.txt".into()],
        })
        .unwrap();
    let base =
        super::check::git_output(f.context.workspace_path(), &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(
        f.context.workspace_path().join("artifact.txt"),
        "actual changed bytes\n",
    )
    .unwrap();
    super::check::git_output(f.context.workspace_path(), &["add", "artifact.txt"]).unwrap();
    tuic_git::git::git_commit_blocking(
        f.context.workspace_path().to_string_lossy().into_owned(),
        "change artifact".into(),
        Some(false),
    )
    .unwrap();
    let (commit, tree) = super::check::clean_artifact(f.context.workspace_path()).unwrap();
    let source = json!({"origin": story.id, "generation":"1", "commit":commit,"tree":tree});
    let run = f.start(
        "agent.context.read",
        json!({"source":source,"kind":"story","base":base}),
    );
    let run = f.dispatch(&run).await;
    let effect = run.effects.values().next().unwrap();
    assert!(!effect.preparation.mutation);
    assert!(effect.preparation.approval.is_none());
    let Some(super::neutral_effects::EffectObservation {
        stopped: true,
        outcome:
            EffectOutcome::Completed {
                result: NeutralResult::Completed { outputs, .. },
            },
    }) = &effect.observation
    else {
        panic!("{effect:?}")
    };
    let data = &outputs["receipt"];
    assert_eq!(data["plan"]["source"], "Preserve the exact artifact bytes");
    assert_eq!(
        data["story"]["criteria"],
        json!(["Publish only independently reviewed bytes"])
    );
    assert_eq!(
        data["review_criteria"],
        json!({"criteria":["check:exact-tree","independent_review"]})
    );
    // Independent SHA256 of the literal canonical gate-list JSON above.
    assert_eq!(
        data["review_subject"],
        json!({"type":"story","id":story.id,"commit":commit,"tree":tree,"requirements":"eb72db4cffe41891d38b843f7e1abe4512d19c99ef58c96e1b09b7fab5167e49"})
    );
    assert!(
        data["diff"]
            .as_str()
            .unwrap()
            .contains("+actual changed bytes")
    );
    let (_, runtime) = super::neutral_model_adapter_tests::fixture("{\"next\":7}", false);
    let role_run = super::native_agent_test_support::role_run(&f, "reviewer", &runtime);
    let mut invocation = super::native_agent_test_support::invocation(&role_run);
    invocation.inputs.insert("source".into(), source);
    invocation.inputs.insert("data".into(), data.clone());
    let request = super::native_agent::request(&invocation).unwrap();
    let prompt = request.prompt().unwrap();
    assert!(prompt.contains("Publish only independently reviewed bytes"));
    assert!(prompt.contains("actual changed bytes"));
    assert!(prompt.contains("eb72db4cffe41891d38b843f7e1abe4512d19c99ef58c96e1b09b7fab5167e49"));
    assert!(prompt.contains("check:exact-tree"));
    let mut missing = invocation.clone();
    missing.inputs.get_mut("data").unwrap()["review_subject"] = serde_json::Value::Null;
    assert!(
        super::native_agent::request(&missing)
            .unwrap_err()
            .contains("subject")
    );
    let mut oversized = invocation.clone();
    oversized.inputs.get_mut("data").unwrap()["diff"] = json!("x".repeat(262_145));
    assert!(
        super::native_agent::request(&oversized)
            .unwrap_err()
            .contains("262144")
    );
    let mut stale = invocation;
    stale.inputs.get_mut("source").unwrap()["generation"] = json!("another generation");
    assert!(
        super::native_agent::request(&stale)
            .unwrap_err()
            .contains("source")
    );
    // No new write rights or model session is created by reading the context.
    assert!(effect.preparation.model_tools.is_none());
    assert!(effect.model_session.is_none());
}

// Catches plan data leaking across projects and unowned batch evidence being
// accepted merely because the caller names a plan and an artifact.
#[tokio::test]
async fn native_role_context_refuses_foreign_projects_and_unpinned_batches() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let backend = StoryStore::open().unwrap();
    let (commit, tree) = super::check::clean_artifact(f.context.workspace_path()).unwrap();
    for (kind, project, expected) in [
        (
            "plan",
            f.dir.path().join("foreign").to_string_lossy().into_owned(),
            "outside this run",
        ),
        (
            "batch",
            f.context.workspace_path().to_string_lossy().into_owned(),
            "owned settled batch pin",
        ),
    ] {
        let plan = backend
            .create_plan(NewPlan {
                project,
                title: "Scoped plan".into(),
                source: "actual source".into(),
            })
            .unwrap();
        let run = f.start("agent.context.read", json!({"source":{"origin":plan.id,"generation":"1","commit":commit,"tree":tree},"kind":kind,"base":commit}));
        let run = f.dispatch(&run).await;
        let effect = run.effects.values().next().unwrap();
        let Some(super::neutral_effects::EffectObservation {
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Failed { diagnostic },
                },
            ..
        }) = &effect.observation
        else {
            panic!("{effect:?}")
        };
        assert!(diagnostic.contains(expected), "{diagnostic}");
    }
}

// Catches a coordinator receiving Plan.source's filename instead of the pinned
// Markdown bytes, trimming those bytes, or accepting a missing/oversized document.
#[tokio::test]
async fn native_role_context_coordinator_requires_bounded_pinned_plan_document_bytes() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let backend = StoryStore::open().unwrap();
    let path = f.context.workspace_path();
    std::fs::create_dir(path.join("plans")).unwrap();
    let document = "# Actual delivery plan\n\nPreserve byte-exact criteria.\n\n";
    std::fs::write(path.join("plans/actual.md"), document).unwrap();
    std::fs::write(path.join("plans/large.md"), "x".repeat(262_145)).unwrap();
    super::check::git_output(path, &["add", "plans"]).unwrap();
    tuic_git::git::git_commit_blocking(
        path.to_string_lossy().into_owned(),
        "pin plan documents".into(),
        Some(false),
    )
    .unwrap();
    let (commit, tree) = super::check::clean_artifact(path).unwrap();
    for (source, error) in [
        ("plans/actual.md", None),
        ("plans/missing.md", Some("requires its document")),
        ("plans/large.md", Some("262144")),
    ] {
        let plan = backend
            .create_plan(NewPlan {
                project: path.to_string_lossy().into_owned(),
                title: "Actual plan".into(),
                source: source.into(),
            })
            .unwrap();
        let run = f.start("agent.context.read", json!({"source":{"origin":plan.id,"generation":"1","commit":commit,"tree":tree},"kind":"plan","base":commit}));
        let run = f.dispatch(&run).await;
        let effect = run.effects.values().next().unwrap();
        match (&effect.observation.as_ref().unwrap().outcome, error) {
            (
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
                None,
            ) => {
                assert_eq!(outputs["receipt"]["plan_document"], document);
                assert_eq!(outputs["receipt"]["plan"]["source"], "plans/actual.md");
            }
            (
                EffectOutcome::Completed {
                    result: NeutralResult::Failed { diagnostic },
                },
                Some(expected),
            ) => assert!(diagnostic.contains(expected), "{diagnostic}"),
            other => panic!("unexpected coordinator snapshot result: {other:?}"),
        }
    }
}

// Catches a role read that refuses an ordinary story artifact: a text line that
// merely quotes git's binary summary, or an added binary fixture such as a PNG
// screenshot or a .tcap capture. Git's text diff already reduces a binary file to
// one summary line, so refusing blocks the implementer and reviewer for no bound.
#[tokio::test]
async fn native_role_context_reads_story_diffs_that_add_binary_files_or_quote_their_summary() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let backend = StoryStore::open().unwrap();
    let path = f.context.workspace_path();
    let plan = backend
        .create_plan(NewPlan {
            project: path.to_string_lossy().into_owned(),
            title: "Fixture plan".into(),
            source: "not a pinned document".into(),
        })
        .unwrap();
    let story = backend
        .create_story(NewStory {
            plan_id: plan.id.clone(),
            title: "Add a capture fixture".into(),
            criteria: vec!["Review the fixture".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let mut base = super::check::git_output(path, &["rev-parse", "HEAD"]).unwrap();
    for (file, bytes, expected) in [
        (
            "notes.md",
            b"Git prints: Binary files a/x and b/x differ\n".to_vec(),
            "+Git prints: Binary files a/x and b/x differ",
        ),
        (
            "capture.tcap",
            vec![0x89, b'P', b'N', b'G', 0, 0, 1, 2, 0, 3],
            "capture.tcap",
        ),
    ] {
        std::fs::write(path.join(file), bytes).unwrap();
        super::check::git_output(path, &["add", file]).unwrap();
        tuic_git::git::git_commit_blocking(
            path.to_string_lossy().into_owned(),
            format!("add {file}"),
            Some(false),
        )
        .unwrap();
        let (commit, tree) = super::check::clean_artifact(path).unwrap();
        let source = json!({"origin": story.id, "generation":"1", "commit":commit,"tree":tree});
        let run = f.start(
            "agent.context.read",
            json!({"source":source,"kind":"story","base":base}),
        );
        let run = f.dispatch(&run).await;
        let effect = run.effects.values().next().unwrap();
        let Some(super::neutral_effects::EffectObservation {
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
            ..
        }) = &effect.observation
        else {
            panic!("{file}: {effect:?}")
        };
        let diff = outputs["receipt"]["diff"].as_str().unwrap();
        assert!(diff.contains(expected), "{file}: {diff}");
        base = commit;
    }
}

// Catches large plan diffs blocking review, including JSON escaping crossing the
// cap before raw bytes do. Story reads must not load an unrelated large plan file.
#[tokio::test]
async fn native_role_context_summarizes_oversized_diffs_without_loading_story_plan_document() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let backend = StoryStore::open().unwrap();
    let path = f.context.workspace_path();
    std::fs::write(path.join("large-plan.md"), "x".repeat(262_145)).unwrap();
    super::check::git_output(path, &["add", "."]).unwrap();
    tuic_git::git::git_commit_blocking(
        path.to_string_lossy().into_owned(),
        "pin large plan".into(),
        Some(false),
    )
    .unwrap();
    let plan = backend
        .create_plan(NewPlan {
            project: path.to_string_lossy().into_owned(),
            title: "Large plan".into(),
            source: "large-plan.md".into(),
        })
        .unwrap();
    let story = backend
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Large diff".into(),
            criteria: vec!["Review changes".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    for (name, content) in [
        ("raw.txt", "x\n".repeat(140_000)),
        ("escaped.txt", "\"\n".repeat(60_000)),
    ] {
        let base = super::check::git_output(path, &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(path.join(name), content).unwrap();
        super::check::git_output(path, &["add", name]).unwrap();
        tuic_git::git::git_commit_blocking(
            path.to_string_lossy().into_owned(),
            format!("add {name}"),
            Some(false),
        )
        .unwrap();
        let (commit, tree) = super::check::clean_artifact(path).unwrap();
        let run = f.start("agent.context.read", json!({"source":{"origin":story.id,"generation":"1","commit":commit,"tree":tree},"kind":"story","base":base}));
        let run = f.dispatch(&run).await;
        let effect = run.effects.values().next().unwrap();
        let Some(super::neutral_effects::EffectObservation {
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
            ..
        }) = &effect.observation
        else {
            panic!("{name}: {effect:?}")
        };
        let data = &outputs["receipt"];
        let diff = data["diff"].as_str().unwrap();
        assert!(
            diff.starts_with("[TRUNCATED:"),
            "{name}: missing truncation marker"
        );
        assert!(
            diff.contains(name) && diff.contains("insertions(+)"),
            "{name}: {diff}"
        );
        assert!(data["plan_document"].is_null());
        assert!(serde_json::to_vec(&data["diff"]).unwrap().len() <= 262_144);
    }
}
