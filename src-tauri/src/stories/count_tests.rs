use super::*;
use serde_json::json;

fn create(store: &StoryStore, plan: &str, title: &str) -> Story {
    store
        .create_story(NewStory {
            plan_id: plan.into(),
            title: title.into(),
            criteria: vec!["Verified".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap()
}

// Catches: a plan-only, closed-inclusive, archived-inclusive or cross-project count;
// also prevents the aggregate API exposing story content to plugins.
#[test]
fn open_count_uses_all_plan_open_projection_without_content() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let project_dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let project = std::fs::canonicalize(project_dir.path()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    crate::story_adapter::binding::set_override(&project, "native").unwrap();
    let store = StoryStore::open().unwrap();
    let mut plans = Vec::new();
    for title in ["First", "Second", "Archived"] {
        plans.push(
            store
                .create_plan(NewPlan {
                    project: project.to_string_lossy().into_owned(),
                    title: title.into(),
                    source: format!("{title}.md"),
                })
                .unwrap(),
        );
    }
    create(&store, &plans[0].id, "First open");
    let blocked = create(&store, &plans[1].id, "Second open");
    store
        .transition(&blocked.id, 1, StoryCommand::Block)
        .unwrap();
    let cancelled = create(&store, &plans[0].id, "Cancelled");
    store
        .transition(&cancelled.id, 1, StoryCommand::WontFix)
        .unwrap();
    let done = create(&store, &plans[0].id, "Done");
    let mut done = store
        .transition(&done.id, 1, StoryCommand::StartManual)
        .unwrap();
    for command in [
        StoryCommand::CheckCriterion(0),
        StoryCommand::SubmitReview,
        StoryCommand::Approve,
    ] {
        done = store.transition(&done.id, done.revision, command).unwrap();
    }
    let archived_story = create(&store, &plans[1].id, "Archived story");
    store
        .set_story_archived(&archived_story.id, 1, true, StoryTransitionActor::Human)
        .unwrap();
    create(&store, &plans[2].id, "Archived plan story");
    store
        .set_plan_archived(&plans[2].id, 1, true, StoryTransitionActor::Human)
        .unwrap();
    let other = store
        .create_plan(NewPlan {
            project: dir.path().to_string_lossy().into_owned(),
            title: "Other".into(),
            source: "other.md".into(),
        })
        .unwrap();
    create(&store, &other.id, "Other project");
    let reply = serde_json::to_value(
        story_action(project.to_str().unwrap(), StoryAction::OpenStoryCount, None).unwrap(),
    )
    .unwrap();
    assert_eq!(reply["type"], "open_story_count");
    assert_eq!(
        reply["value"],
        json!({"project":project, "backend":"native", "generation":null, "openCount":2})
    );
}

// Catches: confusing an empty projection with failure or falling back to native
// when the explicitly selected external connector cannot initialize.
#[test]
fn empty_count_is_zero_but_unavailable_backend_is_an_error() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let project = std::fs::canonicalize(repo.path()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    crate::story_adapter::binding::set_override(&project, "native").unwrap();
    let reply = serde_json::to_value(
        story_action(project.to_str().unwrap(), StoryAction::OpenStoryCount, None).unwrap(),
    )
    .unwrap();
    assert_eq!(reply["value"]["openCount"], 0);
    std::fs::write(
        dir.path().join("story-backends.json"),
        json!({project.to_str().unwrap():{"command":"/nonexistent/story-count-adapter","args":[]}})
            .to_string(),
    )
    .unwrap();
    let error =
        story_action(project.to_str().unwrap(), StoryAction::OpenStoryCount, None).unwrap_err();
    assert!(error.contains("story backend unavailable"), "{error}");
}
