use super::*;
use serde_json::{Value, json};

fn action(project: &str, input: Value, session: Option<&str>) -> Value {
    let action = serde_json::from_value(input).expect("published note action");
    serde_json::to_value(story_action(project, action, session).expect("story action"))
        .expect("wire reply")
}

// Catches a note stored without its subject revision or backend-derived actor and time.
#[test]
fn story_note_is_returned_with_actor_time_and_revision() {
    let config = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("config");
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let repo = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("project");
    let project = repo.path().to_str().unwrap();
    let plan = action(
        project,
        json!({"action":"create_plan","title":"Notes","source":"plan.md"}),
        None,
    );
    let story = action(
        project,
        json!({"action":"create_story","input":{
            "planId":plan["value"]["id"], "title":"Work", "criteria":["Done"],
            "priority":1,"origin":{"type":"native"},"fileScope":[]
        }}),
        None,
    );
    let id = &story["value"]["id"];
    let revision = &story["value"]["revision"];
    let before = chrono::Utc::now().timestamp_millis();
    let added = action(
        project,
        json!({"action":"add_note", "story_id":id,
        "expected_revision":revision,"text":"Verified the effect"}),
        Some("managed-note-author"),
    );
    let after = chrono::Utc::now().timestamp_millis();
    assert_eq!(added["type"], "note");
    let note = &added["value"];
    assert_eq!(note["storyId"], *id);
    assert_eq!(note["storyRevision"], *revision);
    assert_eq!(note["text"], "Verified the effect");
    assert_eq!(
        note["actor"],
        json!({"type":"managed_session","session_id":"managed-note-author"})
    );
    let created = note["createdAt"]
        .as_i64()
        .expect("backend timestamp in milliseconds");
    assert!((before..=after).contains(&created));
    assert!(uuid::Uuid::parse_str(note["id"].as_str().unwrap()).is_ok());
    let listed = action(project, json!({"action":"list_notes","story_id":id}), None);
    assert_eq!(listed, json!({"type":"notes","value":[note]}));
    let current = action(project, json!({"action":"get_story","story_id":id}), None);
    assert_eq!(
        current, story,
        "commentary must not invalidate story receipts"
    );
}

fn with_story(test: impl FnOnce(&str, &StoryStore, &Story)) {
    let config = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let repo = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let project = repo.path().to_str().unwrap();
    let store = StoryStore::open().unwrap();
    let plan = store
        .create_plan(NewPlan {
            project: project.into(),
            title: "Notes".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let story = store
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Work".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    test(project, &store, &story);
}

// Catches overwriting earlier notes, losing them on reopen, or rebasing historical evidence.
#[test]
fn story_notes_survive_reopen_in_append_order_without_rewriting_subject_revisions() {
    with_story(|project, store, story| {
        let add = |revision, text| {
            action(
                project,
                json!({"action":"add_note", "story_id":story.id,
            "expected_revision":revision,"text":text}),
                None,
            )
        };
        let first = add(story.revision, "First observation");
        let blocked = store
            .transition_for_actor(&story.id, story.revision, StoryCommand::Block, None)
            .unwrap();
        let second = add(blocked.revision, "Second observation");
        assert_ne!(first["value"]["id"], second["value"]["id"]);
        assert_eq!(first["value"]["storyRevision"], story.revision);
        assert_eq!(second["value"]["storyRevision"], blocked.revision);
        assert_eq!(first["value"]["actor"], json!({"type":"human"}));
        let listed = action(
            project,
            json!({"action":"list_notes","story_id":story.id}),
            None,
        );
        assert_eq!(listed["value"], json!([first["value"], second["value"]]));
        assert_eq!(store.get_story(&story.id).unwrap(), blocked);
        for name in ["edit_note", "delete_note", "remove_note"] {
            assert!(
                serde_json::from_value::<StoryAction>(json!({"action":name,"story_id":story.id}))
                    .is_err()
            );
        }
    });
}

// Catches stale commentary being attached to a newer subject, invalid input, or project leakage.
#[test]
fn story_notes_reject_stale_invalid_or_cross_project_requests_without_appending() {
    with_story(|project, store, story| {
        let blocked = store
            .transition_for_actor(&story.id, story.revision, StoryCommand::Block, None)
            .unwrap();
        let base = json!({"action":"add_note", "story_id":story.id,"expected_revision":blocked.revision,"text":"Note"});
        let mut cases = vec![];
        for (key, value) in [
            ("text", json!("  \n")),
            ("text", json!("x".repeat(16001))),
            ("expected_revision", json!(story.revision)),
            (
                "receipts",
                json!([{"runId":"missing","commandId":"missing"}]),
            ),
        ] {
            let mut input = base.clone();
            input[key] = value;
            cases.push(input);
        }
        for input in cases {
            let request = serde_json::from_value(input).unwrap();
            let error = story_action(project, request, None).unwrap_err();
            assert!(is_user_error(&error), "{error}");
        }
        let other = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        for input in [
            base.clone(),
            json!({"action":"list_notes","story_id":story.id}),
        ] {
            assert!(
                story_action(
                    other.path().to_str().unwrap(),
                    serde_json::from_value(input).unwrap(),
                    None
                )
                .is_err()
            );
        }
        let mut forged = base.clone();
        forged["actor"] = json!({"type":"human"});
        assert!(serde_json::from_value::<StoryAction>(forged).is_err());
        let listed = action(
            project,
            json!({"action":"list_notes","story_id":story.id}),
            None,
        );
        assert_eq!(listed["value"], json!([]));
        assert_eq!(store.get_story(&story.id).unwrap(), blocked);
    });
}

// Catches accepting a receipt from another plan or copying its payload into the note.
#[test]
fn story_note_receipts_reference_real_persisted_events_in_the_same_plan() {
    with_story(|project, store, story| {
        use crate::workflows::{RunLimits, RunStore, WorkflowKind, WorkflowStore};
        let definitions = WorkflowStore::open().unwrap();
        let definition = definitions
            .seed_templates(project)
            .unwrap()
            .into_iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .unwrap();
        let runs = RunStore::open().unwrap();
        let run = runs
            .start_plan(
                project,
                &story.plan_id,
                &definition.id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let event = runs.events_after(&run.id, 0, 1).unwrap().remove(0);
        let reference = json!({"runId":run.id,"commandId":event.command_id});
        let request = |id: &str, receipts: Value| {
            json!({"action":"add_note","story_id":id,
            "expected_revision":story.revision,"text":"Historical workflow evidence","receipts":receipts})
        };
        let added = action(project, request(&story.id, json!([reference])), None);
        assert_eq!(added["value"]["receipts"], json!([reference]));
        assert!(added["value"].get("snapshot").is_none());
        assert!(added["value"].get("event").is_none());
        let other_plan = store
            .create_plan(NewPlan {
                project: project.into(),
                title: "Other".into(),
                source: "other.md".into(),
            })
            .unwrap();
        let mut input = NewStory {
            plan_id: other_plan.id,
            title: "Other".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let other_story = store.create_story(input.clone()).unwrap();
        assert!(
            story_action(
                project,
                serde_json::from_value(request(&other_story.id, json!([reference]))).unwrap(),
                None
            )
            .is_err()
        );
        input.plan_id = story.plan_id.clone();
        let sibling = store.create_story(input).unwrap();
        assert_eq!(
            action(project, request(&sibling.id, json!([reference])), None)["value"]["receipts"],
            json!([reference])
        );
        for references in [
            json!([{"runId":run.id,"commandId":"not-persisted"}]),
            json!([reference.clone(), reference.clone()]),
            json!(vec![reference.clone(); 17]),
        ] {
            assert!(
                story_action(
                    project,
                    serde_json::from_value(request(&story.id, references)).unwrap(),
                    None
                )
                .is_err()
            );
        }
        assert_eq!(
            action(
                project,
                json!({"action":"list_notes","story_id":story.id}),
                None
            )["value"],
            json!([added["value"]])
        );
    });
}

// Catches a pre-note store failing to acquire notes or losing existing records on upgrade.
#[test]
fn story_notes_upgrade_v1_and_v2_without_rewriting_existing_story_documents() {
    for version in [1, 2] {
        let config = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let db = config.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).unwrap();
        let plan = store
            .create_plan(NewPlan {
                project: config.path().to_string_lossy().into_owned(),
                title: "Existing".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let story = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Existing".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                story_type: None,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        drop(store);
        // Restore the pre-note table set. QA tests separately cover the old CHECK constraint.
        let legacy = rusqlite::Connection::open(&db).unwrap();
        legacy
            .execute_batch("DROP TABLE story_notes; ALTER TABLE plans DROP COLUMN revision; ALTER TABLE plans DROP COLUMN archived;")
            .unwrap();
        legacy.pragma_update(None, "user_version", version).unwrap();
        drop(legacy);
        let upgraded = StoryStore::open_at(&db).unwrap();
        assert_eq!(upgraded.get_story(&story.id).unwrap(), story);
        assert_eq!(
            upgraded.list_notes(&story.id).unwrap(),
            Vec::<StoryNote>::new()
        );
        let note = upgraded
            .add_note(
                &story.id,
                story.revision,
                "After upgrade",
                StoryTransitionActor::Human,
                vec![],
            )
            .unwrap();
        drop(upgraded);
        let reopened = StoryStore::open_at(&db).unwrap();
        assert_eq!(reopened.get_story(&story.id).unwrap(), story);
        assert_eq!(reopened.list_notes(&story.id).unwrap(), vec![note]);
    }
}

// Catches treating the character limit as a byte limit or destroying note formatting.
#[test]
fn story_notes_preserve_text_at_the_unicode_character_limit() {
    with_story(|project, _, story| {
        let text = format!("  {}\n", "é".repeat(15_997));
        let added = action(
            project,
            json!({"action":"add_note","story_id":story.id,
            "expected_revision":story.revision,"text":text}),
            None,
        );
        assert_eq!(added["value"]["text"], text);
        let listed = action(
            project,
            json!({"action":"list_notes","story_id":story.id}),
            None,
        );
        assert_eq!(listed["value"][0], added["value"]);
    });
}
