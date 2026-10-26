use super::*;

fn setup(criteria: &[&str]) -> (tempfile::TempDir, StoryStore, Story) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    let plan = store
        .create_plan(NewPlan {
            project: "/project".into(),
            title: "QA".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let story = store
        .create_story(NewStory {
            story_type: None,
            plan_id: plan.id,
            title: "Verify".into(),
            criteria: criteria.iter().map(|s| (*s).into()).collect(),
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    (dir, store, story)
}

fn apply(store: &StoryStore, story: &Story, command: StoryCommand) -> Story {
    store
        .transition(&story.id, story.revision, command)
        .unwrap()
}

/// Catches: QA bypassing unfinished development, stale revisions, or unchecked manual checks.
#[test]
fn qa_entry_and_approval_enforce_current_criteria_and_revision() {
    let (_dir, store, story) = setup(&["Development", "[MANUAL] Hardware"]);
    let started = apply(&store, &story, StoryCommand::StartManual);
    assert!(
        store
            .transition(&story.id, started.revision, StoryCommand::SubmitQa)
            .is_err()
    );
    let qa = apply(&store, &started, StoryCommand::CheckCriterion(0));
    assert_eq!(qa.status, StoryStatus::Qa);
    assert_eq!(
        store.plan_view(&qa.plan_id).unwrap().state,
        PlanState::Active
    );
    assert!(
        store
            .transition(&qa.id, started.revision, StoryCommand::CheckCriterion(1))
            .unwrap_err()
            .contains("stale")
    );
    assert!(
        store
            .transition(&qa.id, qa.revision, StoryCommand::Approve)
            .unwrap_err()
            .contains(CRITERIA_INCOMPLETE)
    );
    let passed = apply(&store, &qa, StoryCommand::CheckCriterion(1));
    assert_eq!(passed.status, StoryStatus::Qa, "checking does not approve");
    let done = apply(&store, &passed, StoryCommand::Approve);
    assert_eq!(done.status, StoryStatus::Done);
    assert_eq!(
        store.plan_view(&done.plan_id).unwrap().state,
        PlanState::Done
    );
    let history = store.transition_history(&done.id).unwrap();
    assert_eq!(history.len(), 4);
    assert_eq!(history[1].command, StoryCommand::CheckCriterion(0));
}

/// Catches: manual tagging moving Ready work early, or failing to reclassify Review and QA.
#[test]
fn qa_manual_marker_reclassifies_only_active_work() {
    let (_dir, store, story) = setup(&["Development", "Hardware"]);
    let marked = apply(
        &store,
        &story,
        StoryCommand::SetCriterionManual {
            index: 1,
            manual: true,
        },
    );
    assert_eq!(marked.status, StoryStatus::Ready);
    assert_eq!(marked.criteria[1], "[MANUAL] Hardware");
    let started = apply(&store, &marked, StoryCommand::StartManual);
    let qa = apply(&store, &started, StoryCommand::CheckCriterion(0));
    assert_eq!(qa.status, StoryStatus::Qa);
    let failed = apply(&store, &qa, StoryCommand::RejectQa);
    assert_eq!(
        failed.status,
        StoryStatus::InProgress,
        "failure must not auto-reenter QA"
    );
    let qa = apply(&store, &failed, StoryCommand::SubmitReview);
    assert_eq!(qa.status, StoryStatus::Qa);
    let dev = apply(
        &store,
        &qa,
        StoryCommand::SetCriterionManual {
            index: 1,
            manual: false,
        },
    );
    assert_eq!(dev.status, StoryStatus::InProgress);
    assert_eq!(dev.criteria[1], "Hardware");
    let checked = apply(&store, &dev, StoryCommand::CheckCriterion(1));
    let review = apply(&store, &checked, StoryCommand::SubmitReview);
    assert_eq!(review.status, StoryStatus::Review);
    let marked = apply(
        &store,
        &review,
        StoryCommand::SetCriterionManual {
            index: 1,
            manual: true,
        },
    );
    let qa = apply(&store, &marked, StoryCommand::SubmitQa);
    assert_eq!(qa.status, StoryStatus::Qa);
    let reopened = apply(&store, &qa, StoryCommand::UncheckCriterion(0));
    assert_eq!(reopened.status, StoryStatus::InProgress);
}

/// Catches: an all-manual story taking a development claim or completing a prerequisite early.
#[test]
fn qa_all_manual_claim_releases_ownership_but_not_dependencies() {
    let (_dir, store, story) = setup(&["[MANUAL] Device"]);
    let dependent = store
        .create_story(NewStory {
            story_type: None,
            plan_id: story.plan_id.clone(),
            title: "Next".into(),
            criteria: vec!["Next step".into()],
            priority: 2,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let dependent = store
        .add_dependency(&dependent.id, &story.id, dependent.revision)
        .unwrap();
    let qa = store.claim(&story.id, "session", story.revision).unwrap();
    assert_eq!(qa.status, StoryStatus::Qa);
    assert_eq!(qa.claim_session, None);
    assert_eq!(store.release_session_claims("session").unwrap(), 0);
    assert_eq!(
        store.get_story(&dependent.id).unwrap().status,
        StoryStatus::Backlog
    );
    let qa = apply(&store, &qa, StoryCommand::CheckCriterion(0));
    apply(&store, &qa, StoryCommand::Approve);
    assert_eq!(
        store.get_story(&dependent.id).unwrap().status,
        StoryStatus::Ready
    );
}

/// Catches: schema expansion rewriting old JSON/revisions or breaking transition foreign keys.
#[test]
fn qa_schema_upgrade_preserves_existing_documents_and_history() {
    let (dir, store, story) = setup(&["Development", "[MANUAL] Hardware"]);
    let started = apply(&store, &story, StoryCommand::StartManual);
    let db = dir.path().join("stories.sqlite3");
    drop(store);
    let conn = rusqlite::Connection::open(&db).unwrap();
    let original: String = conn
        .query_row(
            "SELECT document FROM stories WHERE id=?1",
            [&story.id],
            |r| r.get(0),
        )
        .unwrap();
    // Model the old CHECK constraint, using the actual stored document unchanged.
    conn.execute_batch("PRAGMA foreign_keys=OFF;
        BEGIN IMMEDIATE;
        CREATE TABLE stories_v1 (
            id TEXT PRIMARY KEY, plan_id TEXT NOT NULL REFERENCES plans(id), document TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('backlog','ready','in_progress','review','done','blocked','wontfix')),
            revision INTEGER NOT NULL, claim_session TEXT);
        INSERT INTO stories_v1 SELECT * FROM stories;
        DROP TABLE stories;
        ALTER TABLE stories_v1 RENAME TO stories;
        CREATE INDEX stories_by_plan ON stories(plan_id);
        CREATE UNIQUE INDEX one_story_per_session ON stories(claim_session) WHERE claim_session IS NOT NULL;
        ALTER TABLE plans DROP COLUMN revision;
        ALTER TABLE plans DROP COLUMN archived;
        PRAGMA user_version=1;
        COMMIT;").unwrap();
    drop(conn);
    let store = StoryStore::open_at(&db).unwrap();
    assert_eq!(store.get_story(&story.id).unwrap(), started);
    let conn = rusqlite::Connection::open(&db).unwrap();
    let after: String = conn
        .query_row(
            "SELECT document FROM stories WHERE id=?1",
            [&story.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, original);
    assert_eq!(store.transition_history(&story.id).unwrap().len(), 1);
    let qa = apply(&store, &started, StoryCommand::CheckCriterion(0));
    assert_eq!(qa.status, StoryStatus::Qa);
    let violations: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
    drop(store);
    assert_eq!(
        StoryStore::open_at(&db)
            .unwrap()
            .get_story(&story.id)
            .unwrap(),
        qa
    );
}

/// Catches: QA disappearing from Open/Kanban or being included in Closed/other-type projections.
#[test]
fn qa_filters_and_kanban_keep_human_work_visible_and_unfinished() {
    use crate::stories::{StoryFilter, StoryVisibility};
    let (_dir, store, story) = setup(&["[MANUAL] Device"]);
    let qa = apply(&store, &story, StoryCommand::StartManual);
    let qa_filter: StoryFilter =
        serde_json::from_value(serde_json::json!({"visibility":"qa"})).unwrap();
    for filter in [
        StoryFilter::default(),
        StoryFilter {
            visibility: StoryVisibility::Open,
            ..Default::default()
        },
        qa_filter,
    ] {
        let view = store.filtered_plan_view(&qa.plan_id, &filter).unwrap();
        assert_eq!(view.visible_story_ids, vec![qa.id.clone()]);
        assert_eq!(
            view.columns
                .iter()
                .find(|column| column.status == StoryStatus::Qa)
                .unwrap()
                .story_ids,
            vec![qa.id.clone()]
        );
        assert_eq!(view.state, PlanState::Active);
    }
    let closed = store
        .filtered_plan_view(
            &qa.plan_id,
            &StoryFilter {
                visibility: StoryVisibility::Closed,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(closed.visible_story_ids, Vec::<String>::new());
    assert_eq!(closed.state, PlanState::Active);
}

/// Catches: QA being amended as if it were unstarted work, invalidating manual evidence.
#[test]
fn update_story_refuses_qa_without_writing_metadata() {
    let (_dir, store, story) = setup(&["[MANUAL] Hardware"]);
    let qa = apply(&store, &story, StoryCommand::StartManual);
    assert_eq!(qa.status, StoryStatus::Qa);
    let error = store
        .update_story(
            &qa.id,
            qa.revision,
            StoryPatch {
                priority: Some(3),
                ..StoryPatch::default()
            },
        )
        .unwrap_err();
    assert!(error.contains("only backlog or ready"), "{error}");
    assert_eq!(store.get_story(&qa.id).unwrap(), qa);
}
