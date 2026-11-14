use super::native_story::{StoryRequest, transition};
use super::native_test_support::Fixture;
use super::neutral_effects::EffectCommand;
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStore};

// Catches adapter bypass of native story revision checks and status rules.
#[tokio::test]
async fn native_story_adapter_keeps_revision_checks_and_domain_transitions() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("stories.sqlite3")).unwrap();
    let plan = stories
        .create_plan(NewPlan {
            project: f.context.workspace_path().to_string_lossy().into_owned(),
            title: "Native fixture".into(),
            source: "fixture".into(),
        })
        .unwrap();
    let story = stories
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Native transition".into(),
            criteria: vec!["Keep current revision".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let story = stories
        .transition(&story.id, story.revision, StoryCommand::StartManual)
        .unwrap();
    let request = StoryRequest {
        story: story.id.clone(),
        revision: story.revision,
        command: StoryCommand::CheckCriterion(0),
    };
    let run = f.start("story.transition", serde_json::to_value(&request).unwrap());
    let waiting = f.dispatch(&run).await;
    let approved = f.approve(&waiting);
    let key = approved.effects.values().next().unwrap().key.clone();
    // The adapter executes under the real ledger claim. Its native story store is
    // an isolated backend, never the host's selected live story store.
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let changed = transition(&f.context, &f.owner.store, &key, &stories, &request).unwrap();
    assert!(changed.checked[0]);
    assert!(changed.revision > story.revision);
    assert!(transition(&f.context, &f.owner.store, &key, &stories, &request).is_err());
    assert_eq!(
        stories.get_story(&story.id).unwrap().revision,
        changed.revision
    );
    // Supplying another command cannot reuse approval for this exact request.
    let mut altered = request;
    altered.command = StoryCommand::Approve;
    assert!(transition(&f.context, &f.owner.store, &key, &stories, &altered).is_err());
}

// Catches native.story.read (decision G2) reading a story of another project, or
// reporting a criteria count that differs from the story.
#[test]
fn native_story_read_is_scoped_to_the_run_project() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("read.sqlite3")).unwrap();
    let story_in = |project: String, criteria: usize| {
        let plan = stories
            .create_plan(NewPlan {
                project,
                title: "Read fixture".into(),
                source: "fixture".into(),
            })
            .unwrap();
        stories
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Readable".into(),
                criteria: (0..criteria).map(|i| format!("criterion {i}")).collect(),
                priority: 2,
                story_type: None,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap()
    };
    let own = story_in(f.context.workspace_path().to_string_lossy().into_owned(), 3);
    let foreign = story_in(
        f.dir
            .path()
            .join("elsewhere")
            .to_string_lossy()
            .into_owned(),
        1,
    );
    let read = |id: &str| {
        super::native_story::read(
            &f.context,
            &stories,
            &super::native_story::ReadRequest { story: id.into() },
        )
    };
    let receipt = read(&own.id).unwrap();
    assert_eq!(receipt["criteria"], 3);
    assert_eq!(receipt["revision"], own.revision);
    assert!(
        read(&foreign.id)
            .unwrap_err()
            .contains("outside this run's project")
    );
}
