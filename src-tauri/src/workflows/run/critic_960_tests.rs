//! Critic test for 960-8670: a retried restart recovery must not interrupt work started after it.
use super::*;
use crate::stories::{NewPlan, NewStory, StoryOrigin, StoryStore};
use crate::workflows::{WorkflowKind, WorkflowStore};

#[test]
fn dependency_refresh_failure_after_interruption_does_not_requeue_recovery_of_live_work() {
    // catches: daemon-start recovery that interrupted the old attempt but failed in the story
    // dependency refresh stays "pending", so the next open interrupts a worker the user
    // resumed and started in between.
    let config = tempfile::tempdir().expect("config");
    let project = tempfile::tempdir().expect("project");
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project_path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(NewPlan {
            project: project_path.clone(),
            title: "Plan".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let story = stories
        .create_story(NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: "Work".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let definition_id = WorkflowStore::open()
        .unwrap()
        .seed_templates(&project_path)
        .unwrap()
        .into_iter()
        .find(|draft| draft.kind == WorkflowKind::Plan)
        .unwrap()
        .id;
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let run = store
        .start_plan(
            &project_path,
            &plan.id,
            &definition_id,
            1,
            RunLimits::default(),
        )
        .unwrap();
    store
        .command(
            &run.id,
            "old-attempt",
            RunCommand::StartAttempt {
                story_id: story.id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
    // Close the real SQLite handle before replacing its path on Windows.
    drop(stories);
    // Make only the story store unusable for the first open.
    let db = config.path().join("stories.sqlite3");
    let moved = config.path().join("stories.sqlite3.moved");
    std::fs::rename(&db, &moved).unwrap();
    std::fs::create_dir(&db).unwrap();
    let owner = super::runtime::RuntimeOwner::acquire(&config.path().join("workflow_runs.sqlite3"))
        .expect("a failed dependency refresh does not block owner startup");
    let first = &owner.store;
    std::fs::remove_dir(&db).unwrap();
    std::fs::rename(&moved, &db).unwrap();
    assert_eq!(
        first.snapshot(&run.id).unwrap().attempts[0].state,
        AttemptState::Interrupted
    );
    first
        .command(&run.id, "resume", RunCommand::Resume)
        .unwrap();
    first
        .command(
            &run.id,
            "new-attempt",
            RunCommand::StartAttempt {
                story_id: story.id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
    let live = first.snapshot(&run.id).unwrap();
    assert_eq!(live.attempts.last().unwrap().state, AttemptState::Running);
    let second = RunStore::open().unwrap();
    assert_eq!(
        second
            .snapshot(&run.id)
            .unwrap()
            .attempts
            .last()
            .unwrap()
            .state,
        AttemptState::Running,
        "second open interrupted a live worker"
    );
}
