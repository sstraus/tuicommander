//! The TUIC execution ledger over the reference adapter (story 2730-07ba).

use super::binding::SETTINGS_FILE;
use crate::stories::{NewPlan, StoryStore, story_action};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Bound {
    _config: TempDir,
    _guard: Box<dyn std::any::Any>,
    dir: TempDir,
}

impl Bound {
    fn project(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).unwrap()
    }

    /// Rewrites the reference adapter's file, as a writer outside TUIC does.
    fn edit_story(&self, id: &str, edit: impl FnOnce(&mut Value)) {
        let path = self.project().join(".tuic-reference-stories.json");
        let mut db: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let story = db["stories"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|story| story["id"] == id)
            .unwrap();
        edit(story);
        std::fs::write(&path, db.to_string()).unwrap();
    }

    fn adapter_story_count(&self) -> usize {
        let path = self.project().join(".tuic-reference-stories.json");
        let db: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        db["stories"].as_array().unwrap().len()
    }

    fn adapter_status(&self, id: &str) -> Value {
        let path = self.project().join(".tuic-reference-stories.json");
        let db: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        db["stories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|story| story["id"] == id)
            .unwrap()["status"]
            .clone()
    }
}

fn scratch() -> TempDir {
    TempDir::new_in(crate::test_support::test_temp_root()).unwrap()
}

fn bind_reference(native_plan: Option<&str>) -> Bound {
    let config = scratch();
    let guard = Box::new(crate::config::set_config_dir_override(
        config.path().to_path_buf(),
    ));
    let dir = scratch();
    let project = std::fs::canonicalize(dir.path()).unwrap();
    if let Some(title) = native_plan {
        StoryStore::open()
            .unwrap()
            .create_plan(NewPlan {
                project: project.to_string_lossy().into_owned(),
                title: title.into(),
                source: "plan.md".into(),
            })
            .unwrap();
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../scripts/story-adapter-reference/adapter.mjs");
    std::fs::write(
        config.path().join(SETTINGS_FILE),
        json!({ project.to_string_lossy():
            { "command": "node", "args": [script.to_string_lossy()] } })
        .to_string(),
    )
    .unwrap();
    Bound {
        _config: config,
        _guard: guard,
        dir,
    }
}

fn act(project: &Path, session: Option<&str>, action: Value) -> Result<Value, String> {
    let action = serde_json::from_value(action).expect("story action");
    story_action(project.to_str().unwrap(), action, session)
        .map(|reply| serde_json::to_value(reply).unwrap()["value"].clone())
}

fn create(project: &Path, title: &str, criteria: &[&str]) -> Value {
    act(
        project,
        None,
        json!({"action": "create_story", "input": {
            "planId": "default", "title": title, "criteria": criteria,
            "priority": 2, "origin": {"type": "native"}, "fileScope": []
        }}),
    )
    .unwrap()
}

fn get(project: &Path, id: &str) -> Value {
    act(
        project,
        None,
        json!({"action": "get_story", "story_id": id}),
    )
    .unwrap()
}

fn transition(project: &Path, story: &Value, command: Value) -> Result<Value, String> {
    act(
        project,
        None,
        json!({"action": "transition", "story_id": story["id"],
               "expected_revision": story["revision"], "command": command}),
    )
}

fn view_read(project: &Path, id: &str) -> Value {
    let view = act(
        project,
        None,
        json!({"action": "plan_view", "plan_id": "default"}),
    )
    .unwrap();
    view["stories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|read| read["id"] == id)
        .unwrap()
        .clone()
}

#[test]
fn review_and_qa_are_tuic_sub_states_over_the_adapter_in_progress() {
    // Catches: review/QA written to the adapter as a status it does not have
    // (or lost, so the story reads in_progress again), the claim or approval
    // stored outside the ledger, or a transition history that stays empty.
    let bound = bind_reference(None);
    let project = bound.project();
    let story = create(&project, "Fix login", &["works"]);
    let id = story["id"].as_str().unwrap().to_string();
    bound.edit_story(&id, |story| story["status"] = json!("ready"));
    let ready = get(&project, &id);
    assert_eq!(ready["status"], "ready", "{ready}");

    let claimed = act(
        &project,
        Some("session-1"),
        json!({"action": "claim", "story_id": id, "expected_revision": ready["revision"]}),
    )
    .unwrap();
    assert_eq!(claimed["status"], "in_progress", "{claimed}");
    assert_eq!(claimed["claimSession"], "session-1", "{claimed}");
    assert_eq!(bound.adapter_status(&id), "in_progress");

    let checked = transition(&project, &claimed, json!({"check_criterion": 0})).unwrap();
    let review = transition(&project, &checked, json!("submit_review")).unwrap();
    assert_eq!(review["status"], "review", "{review}");
    assert_eq!(get(&project, &id)["status"], "review");
    assert_eq!(
        bound.adapter_status(&id),
        "in_progress",
        "review is TUIC-only"
    );
    assert!(
        transition(&project, &checked, json!("approve"))
            .unwrap_err()
            .starts_with("stale story revision"),
        "a sub-state change moves the revision"
    );

    let done = transition(&project, &review, json!("approve")).unwrap();
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(bound.adapter_status(&id), "complete");
    assert_eq!(view_read(&project, &id).get("completedOutside"), None);

    let executions = StoryStore::open()
        .unwrap()
        .external_executions("tuic-reference")
        .unwrap();
    assert!(executions[&id].receipt_version.is_some(), "{executions:?}");
    let history = act(
        &project,
        None,
        json!({"action": "transition_history", "story_id": id}),
    )
    .unwrap();
    let commands: Vec<&Value> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| &entry["command"])
        .collect();
    assert_eq!(
        commands,
        [
            &json!("start_manual"),
            &json!({"check_criterion": 0}),
            &json!("submit_review"),
            &json!("approve")
        ]
    );
}

#[test]
fn a_story_completed_outside_tuic_releases_no_dependent() {
    // Catches: an adapter `complete` taken as proof, so a dependent becomes
    // claimable without a TUIC integration receipt, or the story not marked as
    // completed outside TUIC in the plan view.
    let bound = bind_reference(None);
    let project = bound.project();
    let base = create(&project, "Base", &["x"]);
    let dependent = create(&project, "Dependent", &["y"]);
    let (base_id, dep_id) = (
        base["id"].as_str().unwrap().to_string(),
        dependent["id"].as_str().unwrap().to_string(),
    );
    bound.edit_story(&base_id, |story| {
        story["status"] = json!("complete");
        story["criteria"][0]["checked"] = json!(true);
    });
    bound.edit_story(&dep_id, |story| {
        story["status"] = json!("ready");
        story["dependencies"] = json!([base_id]);
    });

    let base_read = view_read(&project, &base_id);
    assert_eq!(base_read["status"], "done", "{base_read}");
    assert_eq!(base_read["completedOutside"], true, "{base_read}");
    let held = get(&project, &dep_id);
    assert_eq!(held["status"], "backlog", "{held}");
    let refused = act(
        &project,
        Some("session-2"),
        json!({"action": "claim", "story_id": dep_id, "expected_revision": held["revision"]}),
    )
    .unwrap_err();
    assert!(refused.contains("integration receipt"), "{refused}");
    assert_eq!(
        bound.adapter_status(&dep_id),
        "ready",
        "the adapter is untouched"
    );
}

#[test]
fn a_project_with_an_open_native_plan_keeps_its_native_backend() {
    // Catches: binding an external backend over open native work, which would
    // hide those plans (or delete them) instead of refusing the switch.
    let bound = bind_reference(Some("Native work"));
    let project = bound.project();
    let error = act(&project, None, json!({"action": "list_plans"})).unwrap_err();
    assert!(
        error.starts_with(super::UNAVAILABLE) && error.contains("Native work"),
        "{error}"
    );
    let native = StoryStore::open()
        .unwrap()
        .list_plans(&project.to_string_lossy())
        .unwrap();
    assert_eq!(native.len(), 1, "the native record stays");

    StoryStore::open()
        .unwrap()
        .set_plan_archived(
            &native[0].id,
            native[0].revision,
            true,
            crate::stories::StoryTransitionActor::Human,
        )
        .unwrap();
    let plans = act(&project, None, json!({"action": "list_plans"})).unwrap();
    assert_eq!(plans[0]["id"], "default", "{plans}");
}

#[test]
fn a_claim_on_a_story_completed_outside_tuic_does_not_hold_the_session() {
    // Catches: the ledger keeps the claim of a story the adapter completed
    // outside TUIC. The story shows done with no claim, yet its session cannot
    // claim the next ready story ("session already claims another story").
    let bound = bind_reference(None);
    let project = bound.project();
    let first = create(&project, "First", &["works"]);
    let first_id = first["id"].as_str().unwrap().to_string();
    let second = create(&project, "Second", &["works"]);
    let second_id = second["id"].as_str().unwrap().to_string();
    bound.edit_story(&first_id, |story| story["status"] = json!("ready"));
    bound.edit_story(&second_id, |story| story["status"] = json!("ready"));
    let ready = get(&project, &first_id);
    act(
        &project,
        Some("session-1"),
        json!({"action": "claim", "story_id": first_id, "expected_revision": ready["revision"]}),
    )
    .unwrap();

    bound.edit_story(&first_id, |story| story["status"] = json!("complete"));
    let done = get(&project, &first_id);
    assert_eq!(done["status"], "done", "{done}");
    assert!(done["claimSession"].is_null(), "{done}");

    let next = get(&project, &second_id);
    let claimed = act(
        &project,
        Some("session-1"),
        json!({"action": "claim", "story_id": second_id, "expected_revision": next["revision"]}),
    );
    assert!(claimed.is_ok(), "{claimed:?}");
}

fn proposal(title: &str) -> crate::stories::NewStory {
    crate::stories::NewStory {
        story_type: None,
        plan_id: "default".into(),
        title: title.into(),
        criteria: vec!["Delivered".into()],
        priority: 2,
        origin: crate::stories::StoryOrigin::PlanStep {
            step: "second".into(),
        },
        file_scope: vec!["second.txt".into()],
    }
}

#[test]
fn a_run_story_proposal_is_created_once_in_the_adapter_and_recorded_in_the_ledger() {
    // RED: a_run_story_proposal_is_created_once_in_the_adapter_and_recorded_in_the_ledger —
    // catches: a proposal on an external project written to the native store (the adapter
    // never sees it), a retry after a lost response that creates a second adapter story, a
    // reused key with another payload accepted, or the plan-step origin and file scope
    // (which the protocol has no field for) lost on the next read.
    let bound = bind_reference(None);
    let project = bound.project();
    let stories = crate::stories::story_backend_for(project.to_str().unwrap()).unwrap();
    let first = stories
        .create_story_once("run-1", "second", proposal("Dependent"))
        .unwrap();
    let again = stories
        .create_story_once("run-1", "second", proposal("Dependent"))
        .unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(
        bound.adapter_story_count(),
        1,
        "the retry must not create a second adapter story"
    );
    assert_eq!(bound.adapter_status(&first.id), json!("pending"));
    let read = stories.get_story(&first.id).unwrap();
    assert_eq!(read.origin, proposal("x").origin);
    assert_eq!(read.file_scope, vec!["second.txt".to_string()]);
    assert_eq!(
        stories
            .existing_story_for_proposal("run-1", "second", &proposal("Dependent"))
            .unwrap()
            .map(|s| s.id),
        Some(first.id.clone())
    );
    assert!(
        stories
            .existing_story_for_proposal("run-1", "other", &proposal("Dependent"))
            .unwrap()
            .is_none()
    );
    let reused = stories.create_story_once("run-1", "second", proposal("Changed"));
    assert!(
        reused.unwrap_err().contains("different payload"),
        "reused key must be rejected"
    );
    assert_eq!(bound.adapter_story_count(), 1);
    let native = StoryStore::open().unwrap();
    assert!(
        native.get_story(&first.id).is_err(),
        "the proposal must not exist in the native store"
    );
}
