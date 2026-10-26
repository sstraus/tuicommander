use super::*;

fn with_plan(title: &str, test: impl FnOnce(&str, &StoryStore, &Plan)) {
    let config = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("config");
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let repo = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("project");
    let project = repo.path().to_string_lossy().to_string();
    let store = StoryStore::open().expect("store");
    let plan = store
        .create_plan(NewPlan {
            project: project.clone(),
            title: title.into(),
            source: "plan.md".into(),
        })
        .expect("plan");
    test(&project, &store, &plan);
}

#[test]
fn list_stories_filters_by_state_and_priority() {
    // Catches accepting filters but returning stories with another state or priority.
    with_plan("List filters", |project, store, plan| {
        let mut wanted = None;
        for (title, priority, blocked) in [
            ("Ready P1", 1, false),
            ("Blocked P1", 1, true),
            ("Ready P2", 2, false),
        ] {
            let story = store
                .create_story(NewStory {
                    plan_id: plan.id.clone(),
                    title: title.into(),
                    criteria: vec!["Verified".into()],
                    priority,
                    story_type: None,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .expect("story");
            if blocked {
                store
                    .transition(&story.id, story.revision, StoryCommand::Block)
                    .expect("block");
            } else if priority == 1 {
                wanted = Some(story.id);
            }
        }
        let action: StoryAction = serde_json::from_value(serde_json::json!({"action":"list_stories","plan_id":plan.id,"state":"ready","priority":1})).expect("list filter wire contract");
        let reply =
            serde_json::to_value(story_action(project, action, None).expect("filtered list"))
                .expect("reply");
        let ids: Vec<_> = reply["value"]
            .as_array()
            .expect("stories array")
            .iter()
            .map(|story| story["id"].as_str().expect("id").to_string())
            .collect();
        assert_eq!(ids, vec![wanted.expect("ready P1")]);
        assert_eq!(
            store
                .list_stories(&plan.id)
                .expect("internal dispatch read")
                .len(),
            3
        );
    });
}

#[test]
fn list_stories_pages_filtered_ids_without_capping_legacy_reads_or_losing_wire_shape() {
    // Catches paging before filtering, dropping the boundary id, confusing type with origin,
    // and changing the legacy value array into a nested page object.
    with_plan("Pages", |project, store, plan| {
        let mut expected = Vec::new();
        for index in 0..105 {
            let kind = (index < 103).then_some(StoryType::Feature);
            let story = store
                .create_story(NewStory {
                    plan_id: plan.id.clone(),
                    title: format!("Story {index}"),
                    criteria: vec!["Verified".into()],
                    priority: 2,
                    story_type: kind,
                    origin: StoryOrigin::GitHubIssue { number: 7 },
                    file_scope: vec![],
                })
                .expect("story");
            if kind.is_some() {
                expected.push(story.id);
            }
        }
        expected.sort();
        let read = |limit: Option<u16>, cursor: Option<&str>| {
            let action: StoryAction = serde_json::from_value(serde_json::json!({"action":"list_stories","plan_id":plan.id,"story_type":"feature","limit":limit,"cursor":cursor})).expect("wire");
            let reply = story_action(project, action, None).expect("page");
            let json = serde_json::to_value(&reply).expect("serialize page");
            let round_trip: StoryReply =
                serde_json::from_value(json.clone()).expect("deserialize page");
            assert_eq!(
                serde_json::to_value(round_trip).expect("serialize round trip"),
                json
            );
            assert_eq!(json["type"], "stories");
            json
        };
        let ids = |json: &serde_json::Value| -> Vec<String> {
            json["value"]
                .as_array()
                .expect("legacy array")
                .iter()
                .map(|s| s["id"].as_str().expect("id").to_string())
                .collect()
        };
        let unpaged = read(None, None);
        assert_eq!(ids(&unpaged), expected);
        assert!(unpaged.get("next_cursor").is_none());
        let first = read(Some(100), None);
        assert_eq!(ids(&first), expected[..100]);
        let cursor = first["next_cursor"].as_str().expect("more matches");
        assert!(cursor.len() <= 1024);
        let last = read(Some(100), Some(cursor));
        assert_eq!(ids(&last), expected[100..]);
        assert!(last.get("next_cursor").is_none());
        let one = read(Some(1), None);
        assert_eq!(ids(&one), expected[..1]);
        let full = store.list_stories(&plan.id).expect("internal list");
        assert_eq!(full.len(), 105, "dispatch still sees the whole plan");
        assert!(full.windows(2).all(|pair| pair[0].id < pair[1].id));
    });
}

#[test]
fn list_stories_rejects_invalid_bounds_and_cursors_from_another_plan_or_query() {
    // Catches unbounded page input and reusing a cursor to silently skip another query's records.
    with_plan("Cursor", |project, store, plan| {
        for _ in 0..2 {
            store
                .create_story(NewStory {
                    plan_id: plan.id.clone(),
                    title: "Story".into(),
                    criteria: vec!["Verified".into()],
                    priority: 1,
                    story_type: Some(StoryType::Fix),
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .expect("story");
        }
        let action = |input| serde_json::from_value::<StoryAction>(input).expect("wire action");
        let page = serde_json::to_value(
            story_action(
                project,
                action(serde_json::json!({"action":"list_stories","plan_id":plan.id,"limit":1})),
                None,
            )
            .expect("page"),
        )
        .expect("serialize");
        let cursor = page["next_cursor"].as_str().expect("next cursor");
        let other = store
            .create_plan(NewPlan {
                project: project.to_string(),
                title: "Other".into(),
                source: "other.md".into(),
            })
            .expect("other plan");
        let mut failures = Vec::new();
        for (extra, message) in [
            (serde_json::json!({"priority":0}), "priority"),
            (serde_json::json!({"priority":4}), "priority"),
            (serde_json::json!({"limit":0}), "limit"),
            (serde_json::json!({"limit":101}), "limit"),
            (serde_json::json!({"cursor":cursor}), "requires limit"),
            (serde_json::json!({"limit":1,"cursor":""}), "invalid"),
            (serde_json::json!({"limit":1,"cursor":"garbage"}), "invalid"),
            (
                serde_json::json!({"limit":1,"cursor":"x".repeat(1025)}),
                "exceeds",
            ),
            (
                serde_json::json!({"limit":1,"cursor":cursor,"plan_id":other.id}),
                "does not match",
            ),
            (
                serde_json::json!({"limit":1,"cursor":cursor,"priority":1}),
                "does not match",
            ),
            (
                serde_json::json!({"limit":1,"cursor":cursor,"state":"ready"}),
                "does not match",
            ),
            (
                serde_json::json!({"limit":1,"cursor":cursor,"story_type":"fix"}),
                "does not match",
            ),
        ] {
            let mut input = serde_json::json!({"action":"list_stories","plan_id":plan.id});
            input
                .as_object_mut()
                .expect("object")
                .extend(extra.as_object().expect("extra").clone());
            match story_action(project, action(input.clone()), None) {
                Err(error) if error.contains(message) && is_user_error(&error) => {}
                other => failures.push(format!("{input}: expected {message}, received {other:?}")),
            }
        }
        assert_eq!(failures, Vec::<String>::new());
    });
}
