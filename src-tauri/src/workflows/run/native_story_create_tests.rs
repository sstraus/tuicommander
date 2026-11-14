use super::*;
use crate::stories::{NewPlan, StoryOrigin, StoryStore};
use crate::workflows::run::native_test_support::Fixture;
use crate::workflows::run::neutral_effects::EffectCommand;

fn request(f: &Fixture, stories: &StoryStore) -> CreateRequest {
    let plan = stories
        .create_plan(NewPlan {
            project: f.context.workspace_path().to_string_lossy().into_owned(),
            title: "Proposal plan".into(),
            source: "fixture".into(),
        })
        .unwrap();
    CreateRequest {
        problem: Some("Preserve the original issue body and URL".into()),
        review: "review-1".into(),
        cap: 2,
        source_story: None,
        source_revision: None,
        evidence: vec!["native finding".into()],
        proposal: "finding-1".into(),
        story: NewStory {
            plan_id: plan.id,
            title: "Fix the finding".into(),
            criteria: vec!["Finding is resolved".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        },
    }
}

// Catches creation without exact approval, caller responsibility or project fencing.
#[tokio::test]
async fn native_story_create_requires_owned_approval_and_returns_backend_story() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("stories.sqlite3")).unwrap();
    let r = request(&f, &stories);
    let run = f.start("story.create", serde_json::to_value(&r).unwrap());
    let waiting = f.dispatch(&run).await;
    let key = waiting.effects.values().next().unwrap().key.clone();
    assert!(create(&f.context, &f.owner.store, &key, &stories, &r).is_err());
    let approved = f.approve(&waiting);
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let mut altered = r.clone();
    altered.cap = 50;
    assert!(create(&f.context, &f.owner.store, &key, &stories, &altered).is_err());
    assert!(approved.effects.values().next().unwrap().approved());
    let receipt = create(&f.context, &f.owner.store, &key, &stories, &r).unwrap();
    assert_eq!(receipt.story.problem, r.problem);
    assert_eq!(receipt.responsible, "agent:trusted");
    assert_eq!(receipt.review, "review-1");
    assert_eq!(receipt.proposal, "finding-1");
    assert_eq!(receipt.effect, key);
    let dispatched = f.owner.store.neutral_execution(&run.run.id).unwrap();
    let original = dispatched.effect(&key).unwrap();
    let recovered = reconcile_with(&f.context, &f.owner.store, original, &stories).unwrap();
    assert_eq!(recovered.story.id, receipt.story.id);
    let mut changed = original.clone();
    changed.invocation.inputs.get_mut("request").unwrap()["proposal"] = "altered".into();
    assert!(reconcile_with(&f.context, &f.owner.store, &changed, &stories).is_err());
    assert_eq!(
        stories.get_story(&receipt.story.id).unwrap().title,
        r.story.title
    );
    assert!(create(&f.context, &f.owner.store, &key, &stories, &r).is_err());
    assert_eq!(stories.list_stories(&r.story.plan_id).unwrap().len(), 1);
}

// Catches an approved proposal with an explicit "storyType": null failing the exact
// request check: the stored input keeps the null, the typed request serializes
// without it. A real coordinator model sent exactly this (live run 2026-11-13).
#[tokio::test]
async fn native_story_create_accepts_an_approved_explicit_null_story_type() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("stories.sqlite3")).unwrap();
    let mut input = serde_json::to_value(request(&f, &stories)).unwrap();
    input["story"]["storyType"] = serde_json::Value::Null;
    let r: CreateRequest = serde_json::from_value(input.clone()).unwrap();
    let run = f.start("story.create", input);
    let approved = f.approve(&f.dispatch(&run).await);
    let key = approved.effects.values().next().unwrap().key.clone();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let receipt = create(&f.context, &f.owner.store, &key, &stories, &r).unwrap();
    assert_eq!(receipt.story.title, r.story.title);
    assert_eq!(stories.list_stories(&r.story.plan_id).unwrap().len(), 1);
}

// Catches cap escalation, ambiguous-write replay, and caller changes resetting capacity.
#[tokio::test]
async fn native_story_create_reservations_pin_cap_and_consume_unknown_results() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("stories.sqlite3")).unwrap();
    let mut r = request(&f, &stories);
    let run = f.start("story.create", serde_json::to_value(&r).unwrap());
    let waiting = f.dispatch(&run).await;
    let key = waiting.effects.values().next().unwrap().key.clone();
    reserve(&f.context, &f.owner.store, &key, &r).unwrap();
    assert!(reserve(&f.context, &f.owner.store, &key, &r).is_err());
    r.proposal = "finding-2".into();
    r.cap = 3;
    assert!(reserve(&f.context, &f.owner.store, &key, &r).is_err());
    r.cap = 2;
    reserve(&f.context, &f.owner.store, &key, &r).unwrap();
    r.proposal = "finding-3".into();
    assert!(reserve(&f.context, &f.owner.store, &key, &r).is_err());
    r.review = "review-2".into();
    r.cap = 0;
    assert!(reserve(&f.context, &f.owner.store, &key, &r).is_err());
    r.cap = 1;
    assert!(reserve(&f.context, &f.owner.store, &key, &r).is_err());
    r.review = "review-3".into();
    reserve(&f.context, &f.owner.store, &key, &r).unwrap();
}

// Catches an approved request writing into a plan outside its captured checkout.
#[tokio::test]
async fn native_story_create_refuses_cross_project_plan() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("stories.sqlite3")).unwrap();
    let mut r = request(&f, &stories);
    r.story.plan_id = stories
        .create_plan(NewPlan {
            project: f.dir.path().to_string_lossy().into_owned(),
            title: "Other".into(),
            source: "fixture".into(),
        })
        .unwrap()
        .id;
    let run = f.start("story.create", serde_json::to_value(&r).unwrap());
    let waiting = f.dispatch(&run).await;
    let approved = f.approve(&waiting);
    let key = approved.effects.values().next().unwrap().key.clone();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    assert!(create(&f.context, &f.owner.store, &key, &stories, &r).is_err());
    assert_eq!(
        stories.list_stories(&r.story.plan_id).unwrap(),
        [] as [crate::stories::Story; 0]
    );
}

// Catches using a stale source revision or a revoked actor to create follow-up work.
#[tokio::test]
async fn native_story_create_refuses_stale_source_and_revoked_actor() {
    let f = Fixture::new();
    let stories = StoryStore::open_at(&f.dir.path().join("stories.sqlite3")).unwrap();
    let mut r = request(&f, &stories);
    let source = stories.create_story(r.story.clone()).unwrap();
    r.source_story = Some(source.id.clone());
    r.source_revision = Some(source.revision + 1);
    let run = f.start("story.create", serde_json::to_value(&r).unwrap());
    let waiting = f.dispatch(&run).await;
    let approved = f.approve(&waiting);
    let key = approved.effects.values().next().unwrap().key.clone();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let error = create(&f.context, &f.owner.store, &key, &stories, &r).unwrap_err();
    assert!(
        matches!(error, super::super::native_git::NativeFailure::Refused(ref s) if s.contains("stale"))
    );
    f.state.agent_capabilities.remove("trusted");
    let error = create(&f.context, &f.owner.store, &key, &stories, &r).unwrap_err();
    assert!(
        matches!(error, super::super::native_git::NativeFailure::Refused(ref s) if s.contains("revoked"))
    );
    assert_eq!(stories.list_stories(&r.story.plan_id).unwrap().len(), 1);
}
