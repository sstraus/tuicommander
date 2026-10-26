use super::*;

fn create(store: &StoryStore, plan: &str, title: &str, kind: Option<StoryType>) -> Story {
    store
        .create_story(NewStory {
            plan_id: plan.into(),
            title: title.into(),
            criteria: vec!["Verified".into()],
            priority: 2,
            story_type: kind,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .expect("create story")
}

#[test]
fn story_filters_exclude_closed_records_across_the_whole_plan_without_hiding_dependencies() {
    // Catches filtering only an initial page, confusing origin with kind, or hiding detail dependencies.
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("scratch");
    let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
    let plan = store
        .create_plan(NewPlan {
            project: "/project".into(),
            title: "Filters".into(),
            source: "plan.md".into(),
        })
        .expect("plan");
    let mut done_ids = Vec::new();
    let mut open_ids = Vec::new();
    for index in 0..105 {
        let story = create(
            &store,
            &plan.id,
            &format!("Story {index}"),
            Some(StoryType::Feature),
        );
        if index % 2 == 0 {
            let story = store
                .transition(&story.id, 1, StoryCommand::WontFix)
                .expect("cancel");
            done_ids.push(story.id);
        } else {
            open_ids.push(story.id);
        }
    }
    let view = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                visibility: StoryVisibility::Open,
                ..Default::default()
            },
        )
        .expect("open view");
    assert_eq!(
        view.stories.len(),
        105,
        "full records remain available for dependency detail"
    );
    assert_eq!(view.visible_story_ids.len(), 52);
    assert!(
        open_ids
            .iter()
            .all(|id| view.visible_story_ids.contains(id))
    );
    assert!(
        done_ids
            .iter()
            .all(|id| !view.visible_story_ids.contains(id))
    );
    let board_ids: Vec<_> = view
        .columns
        .iter()
        .flat_map(|column| column.story_ids.iter())
        .collect();
    assert_eq!(board_ids.len(), 52);
    assert!(
        board_ids
            .iter()
            .all(|id| view.visible_story_ids.contains(id))
    );
    let closed = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                visibility: StoryVisibility::Closed,
                story_type: Some(StoryType::Feature),
                ..Default::default()
            },
        )
        .expect("closed view");
    assert_eq!(closed.visible_story_ids.len(), 53);
    assert!(
        done_ids
            .iter()
            .all(|id| closed.visible_story_ids.contains(id))
    );
    let untyped = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                untyped_only: true,
                ..Default::default()
            },
        )
        .expect("untyped view");
    assert_eq!(untyped.visible_story_ids, Vec::<String>::new());
}

#[test]
fn story_type_round_trip_keeps_legacy_and_omitted_values_untyped() {
    // Catches fabricating a type for historical JSON or dropping an explicitly selected type on persistence.
    let input = serde_json::json!({"planId":"p","title":"Old","criteria":["Done"],"priority":2,"origin":{"type":"native"},"fileScope":[]});
    let decoded: NewStory = serde_json::from_value(input).expect("legacy create input");
    assert_eq!(decoded.story_type, None);
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("scratch");
    let db = dir.path().join("stories.sqlite3");
    let store = StoryStore::open_at(&db).expect("store");
    let plan = store
        .create_plan(NewPlan {
            project: "/project".into(),
            title: "Kinds".into(),
            source: "plan.md".into(),
        })
        .expect("plan");
    let old = create(&store, &plan.id, "Untyped", None);
    let typed = create(&store, &plan.id, "Typed", Some(StoryType::Fix));
    let json = serde_json::to_value(&old).expect("old document");
    assert!(json.get("storyType").is_none());
    assert_eq!(
        serde_json::from_value::<Story>(json)
            .expect("legacy document")
            .story_type,
        None
    );
    drop(store);
    let store = StoryStore::open_at(&db).expect("reopen");
    assert_eq!(
        store
            .get_story(&typed.id)
            .expect("typed persisted")
            .story_type,
        Some(StoryType::Fix)
    );
    let untyped = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                untyped_only: true,
                ..Default::default()
            },
        )
        .expect("untyped");
    assert_eq!(untyped.visible_story_ids, vec![old.id]);
    let typed_view = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                story_type: Some(StoryType::Fix),
                ..Default::default()
            },
        )
        .expect("typed");
    assert_eq!(typed_view.visible_story_ids, vec![typed.id]);
}

#[test]
fn story_filters_keep_review_blocked_and_backlog_open_and_group_done_as_closed() {
    // Catches applying Wiz's status allowlist, losing empty columns, or filtering the plan summary.
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("scratch");
    let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
    let plan = store
        .create_plan(NewPlan {
            project: "/project".into(),
            title: "States".into(),
            source: "plan.md".into(),
        })
        .expect("plan");
    let ready = create(&store, &plan.id, "Ready", None);
    let backlog = create(&store, &plan.id, "Backlog", None);
    store
        .add_dependency(&backlog.id, &ready.id, 1)
        .expect("backlog");
    let blocked = create(&store, &plan.id, "Blocked", Some(StoryType::Chore));
    store
        .transition(&blocked.id, 1, StoryCommand::Block)
        .expect("block");
    let progress = create(&store, &plan.id, "In progress", None);
    store
        .transition(&progress.id, 1, StoryCommand::StartManual)
        .expect("start");
    let review = create(&store, &plan.id, "Review", None);
    let mut review = store
        .transition(&review.id, 1, StoryCommand::StartManual)
        .expect("start");
    review = store
        .transition(&review.id, review.revision, StoryCommand::CheckCriterion(0))
        .expect("criterion");
    store
        .transition(&review.id, review.revision, StoryCommand::SubmitReview)
        .expect("review");
    let done = create(&store, &plan.id, "Done", Some(StoryType::Task));
    let mut done = store
        .transition(&done.id, 1, StoryCommand::StartManual)
        .expect("start");
    done = store
        .transition(&done.id, done.revision, StoryCommand::CheckCriterion(0))
        .expect("criterion");
    done = store
        .transition(&done.id, done.revision, StoryCommand::SubmitReview)
        .expect("review");
    let done = store
        .transition(&done.id, done.revision, StoryCommand::Approve)
        .expect("approve");
    let view = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                visibility: StoryVisibility::Open,
                ..Default::default()
            },
        )
        .expect("open");
    for id in [
        &ready.id,
        &backlog.id,
        &blocked.id,
        &progress.id,
        &review.id,
    ] {
        assert!(
            view.visible_story_ids.contains(id),
            "open story {id} omitted"
        );
    }
    assert!(!view.visible_story_ids.contains(&done.id));
    assert_eq!(view.columns.len(), 8);
    assert_eq!(view.state, PlanState::Active);
    let closed = store
        .filtered_plan_view(
            &plan.id,
            &StoryFilter {
                visibility: StoryVisibility::Closed,
                ..Default::default()
            },
        )
        .expect("closed");
    assert_eq!(closed.visible_story_ids, vec![done.id]);
    assert_eq!(
        closed.state,
        PlanState::Active,
        "filters do not finish a plan"
    );
    assert!(
        store
            .filtered_plan_view(
                &plan.id,
                &StoryFilter {
                    untyped_only: true,
                    story_type: Some(StoryType::Fix),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
