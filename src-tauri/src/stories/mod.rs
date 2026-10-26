mod api;
#[cfg(test)]
mod critic_tests;
#[cfg(test)]
mod filter_tests;
mod filters;
mod list;
#[cfg(test)]
mod list_tests;
mod model;
#[cfg(test)]
mod note_tests;
mod sources;
mod store;

pub use api::*;
pub use model::*;
pub use store::StoryStore;

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog reads these exact keys; a serde rename or a lost flatten must fail here.
    #[test]
    fn plan_view_reply_keeps_the_wire_shape_the_dialog_reads() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Wire plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id.clone(),
                title: "Wire story".into(),
                criteria: vec!["Shown".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let reply = serde_json::to_value(StoryReply::PlanView(
            store.plan_view(&plan.id).expect("plan view"),
        ))
        .expect("serialize");
        assert_eq!(reply["type"], "plan_view");
        let value = &reply["value"];
        assert_eq!(value["wontFixCount"], 0);
        assert_eq!(value["allCancelled"], false);
        assert_eq!(value["stories"][0]["title"], "Wire story");
        assert_eq!(value["stories"][0]["abandoned"], false);
        assert!(
            value["stories"][0].get("story").is_none(),
            "story fields are flattened"
        );
    }

    #[test]
    fn new_store_sets_schema_version_five() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");

        StoryStore::open_at(&db).expect("open store");

        let connection = rusqlite::Connection::open(db).expect("open database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version, 5);
    }

    #[test]
    fn store_rejects_a_newer_schema_version() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let connection = rusqlite::Connection::open(&db).expect("open database");
        connection
            .pragma_update(None, "user_version", 6)
            .expect("set future schema version");

        let error = StoryStore::open_at(&db).expect_err("future schema is rejected");
        assert!(error.contains("newer than supported"), "{error}");
    }

    #[test]
    fn version_zero_store_is_upgraded_in_place_with_its_data() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Pre-version plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        // Recreate the pre-archive plan layout, then mark it as an unversioned store.
        rusqlite::Connection::open(&db)
            .and_then(|conn| conn.execute_batch("ALTER TABLE plans DROP COLUMN revision; ALTER TABLE plans DROP COLUMN archived; PRAGMA user_version=0;"))
            .expect("mark store as version 0");

        let reopened = StoryStore::open_at(&db).expect("version 0 opens");
        assert_eq!(
            reopened.get_plan(&plan.id).expect("plan survives").title,
            "Pre-version plan"
        );
        let version: i64 = rusqlite::Connection::open(&db)
            .expect("open database")
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version, 5);
    }

    #[test]
    fn coordinator_story_proposal_is_idempotent_across_reopen() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let proposed = NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: "Implement".into(),
            criteria: vec!["Focused test passes".into()],
            priority: 1,
            origin: StoryOrigin::PlanStep {
                step: "implementation".into(),
            },
            file_scope: vec!["src/core.rs".into()],
        };
        assert!(
            store
                .existing_story_for_proposal("run-1", "proposal-1", &proposed)
                .unwrap()
                .is_none()
        );
        let first = store
            .create_story_once("run-1", "proposal-1", proposed.clone())
            .expect("create");
        drop(store);
        let reopened = StoryStore::open_at(&db).expect("reopen");
        let retry = reopened
            .create_story_once("run-1", "proposal-1", proposed.clone())
            .expect("retry");
        assert_eq!(retry.id, first.id);
        assert_eq!(
            reopened
                .existing_story_for_proposal("run-1", "proposal-1", &proposed)
                .unwrap()
                .unwrap()
                .id,
            first.id
        );
        assert_eq!(reopened.list_stories(&plan.id).unwrap().len(), 1);
        let mut changed = proposed.clone();
        changed.title = "Different".into();
        assert!(
            reopened
                .create_story_once("run-1", "proposal-1", changed)
                .is_err()
        );
        let mut invalid = proposed;
        invalid.origin = StoryOrigin::PlanStep { step: " ".into() };
        assert!(
            reopened
                .create_story_once("run-1", "proposal-2", invalid)
                .is_err()
        );
        assert_eq!(reopened.list_stories(&plan.id).unwrap().len(), 1);
    }

    #[test]
    fn operator_can_work_a_story_without_a_terminal_claim() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Manual plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Manual work".into(),
                criteria: vec!["Verified".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        // Catches: tracking metadata refusing a valid manual start without creating a claim.
        let started = store
            .transition_for_actor(
                &story.id,
                story.revision,
                StoryCommand::StartManual,
                Some("agent"),
            )
            .expect("trusted manual start");
        assert_eq!(
            store.transition_history(&story.id).expect("history")[0].actor,
            StoryTransitionActor::ManagedSession {
                session_id: "agent".into()
            }
        );
        assert_eq!(started.status, StoryStatus::InProgress);
        assert_eq!(started.claim_session, None);
        let checked = store
            .transition_for_actor(
                &story.id,
                started.revision,
                StoryCommand::CheckCriterion(0),
                None,
            )
            .expect("check");
        let review = store
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::SubmitReview,
                None,
            )
            .expect("review");
        let done = store
            .transition_for_actor(&story.id, review.revision, StoryCommand::Approve, None)
            .expect("approve");
        assert!(
            store
                .transition(&story.id, done.revision, StoryCommand::Approve)
                .is_err()
        );
        assert_eq!(done.status, StoryStatus::Done);
    }

    #[test]
    fn plan_and_story_survive_reopen_and_plan_state_is_derived() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("open store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Ship feature".into(),
                source: "plans/feature.md".into(),
            })
            .expect("create plan");
        assert_eq!(store.plan_state(&plan.id).expect("state"), PlanState::Draft);
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id.clone(),
                title: "Implement core".into(),
                criteria: vec!["Core behavior works".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec!["src/core.rs".into()],
            })
            .expect("create story");
        assert_eq!(
            store.plan_state(&plan.id).expect("state"),
            PlanState::Active
        );
        drop(store);

        let reopened = StoryStore::open_at(&db).expect("reopen store");
        assert_eq!(reopened.get_story(&story.id).expect("story"), story);
        assert_eq!(
            reopened.plan_state(&plan.id).expect("state"),
            PlanState::Active
        );
        let claimed = reopened
            .claim(&story.id, "tab", story.revision)
            .expect("claim");
        let checked = reopened
            .transition_for_actor(
                &story.id,
                claimed.revision,
                StoryCommand::CheckCriterion(0),
                None,
            )
            .expect("check criterion");
        let review = reopened
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::SubmitReview,
                None,
            )
            .expect("submit review");
        reopened
            .transition_for_actor(&story.id, review.revision, StoryCommand::Approve, None)
            .expect("approve story");
        assert_eq!(
            reopened.plan_state(&plan.id).expect("state"),
            PlanState::Done
        );
    }

    #[test]
    fn cancelled_stories_close_a_nonempty_plan_but_never_release_dependents() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        assert_eq!(
            store.plan_state(&plan.id).expect("empty state"),
            PlanState::Draft
        );
        let make_story = |title: &str| NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let prerequisite = store
            .create_story(make_story("Prerequisite"))
            .expect("prerequisite");
        let dependent = store
            .create_story(make_story("Dependent"))
            .expect("dependent");
        let dependent = store
            .add_dependency(&dependent.id, &prerequisite.id, dependent.revision)
            .expect("dependency");
        store
            .transition_for_actor(
                &prerequisite.id,
                prerequisite.revision,
                StoryCommand::WontFix,
                None,
            )
            .expect("cancel prerequisite");
        assert_eq!(
            store.get_story(&dependent.id).expect("dependent").status,
            StoryStatus::Backlog
        );
        assert!(
            store
                .claim(&dependent.id, "agent", dependent.revision)
                .is_err()
        );
        assert_eq!(
            store.plan_state(&plan.id).expect("unfinished state"),
            PlanState::Active
        );
        let dependent = store
            .remove_dependency(&dependent.id, &prerequisite.id, dependent.revision, None)
            .expect("remove cancelled edge");
        assert_eq!(dependent.status, StoryStatus::Ready);
        store
            .transition_for_actor(
                &dependent.id,
                dependent.revision,
                StoryCommand::WontFix,
                None,
            )
            .expect("cancel dependent");
        assert_eq!(
            store.plan_state(&plan.id).expect("cancelled state"),
            PlanState::Done
        );
    }

    #[test]
    fn removing_a_cancelled_dependency_respects_every_remaining_prerequisite() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let make_story = |title: &str| NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let cancelled = store
            .create_story(make_story("Cancelled"))
            .expect("cancelled");
        let outstanding = store
            .create_story(make_story("Outstanding"))
            .expect("outstanding");
        let dependent = store
            .create_story(make_story("Dependent"))
            .expect("dependent");
        let dependent = store
            .add_dependency(&dependent.id, &cancelled.id, dependent.revision)
            .expect("first dependency");
        let dependent = store
            .add_dependency(&dependent.id, &outstanding.id, dependent.revision)
            .expect("second dependency");
        assert!(
            store
                .remove_dependency(
                    &dependent.id,
                    &cancelled.id,
                    dependent.revision,
                    Some("agent")
                )
                .is_err()
        );
        assert!(
            store
                .remove_dependency(&dependent.id, &cancelled.id, dependent.revision, None)
                .is_err()
        );
        store
            .transition_for_actor(
                &cancelled.id,
                cancelled.revision,
                StoryCommand::WontFix,
                None,
            )
            .expect("cancel");
        assert!(
            store
                .remove_dependency(&dependent.id, &cancelled.id, dependent.revision - 1, None)
                .is_err()
        );
        assert!(
            store
                .remove_dependency(&dependent.id, "missing", dependent.revision, None)
                .is_err()
        );
        let dependent = store
            .remove_dependency(&dependent.id, &cancelled.id, dependent.revision, None)
            .expect("remove cancelled edge");
        assert_eq!(dependent.status, StoryStatus::Backlog);
        assert_eq!(dependent.dependencies, vec![outstanding.id.clone()]);
        assert!(
            store
                .remove_dependency(&dependent.id, &outstanding.id, dependent.revision, None)
                .is_err()
        );
        let outstanding = store
            .transition_for_actor(
                &outstanding.id,
                outstanding.revision,
                StoryCommand::StartManual,
                None,
            )
            .expect("start");
        let outstanding = store
            .transition_for_actor(
                &outstanding.id,
                outstanding.revision,
                StoryCommand::CheckCriterion(0),
                None,
            )
            .expect("check");
        let outstanding = store
            .transition_for_actor(
                &outstanding.id,
                outstanding.revision,
                StoryCommand::SubmitReview,
                None,
            )
            .expect("review");
        store
            .transition_for_actor(
                &outstanding.id,
                outstanding.revision,
                StoryCommand::Approve,
                None,
            )
            .expect("done");
        assert_eq!(
            store.get_story(&dependent.id).expect("promoted").status,
            StoryStatus::Ready
        );
        assert_eq!(
            store.plan_state(&plan.id).expect("state"),
            PlanState::Active
        );
    }

    #[test]
    fn completed_and_cancelled_stories_close_a_plan_but_done_cannot_be_cancelled() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let make_story = |title: &str| NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let done = store.create_story(make_story("Done")).expect("done");
        let cancelled = store
            .create_story(make_story("Cancelled"))
            .expect("cancelled");
        let done = store
            .transition_for_actor(&done.id, done.revision, StoryCommand::StartManual, None)
            .expect("start");
        let done = store
            .transition_for_actor(
                &done.id,
                done.revision,
                StoryCommand::CheckCriterion(0),
                None,
            )
            .expect("check");
        let done = store
            .transition_for_actor(&done.id, done.revision, StoryCommand::SubmitReview, None)
            .expect("review");
        let done = store
            .transition_for_actor(&done.id, done.revision, StoryCommand::Approve, None)
            .expect("done");
        assert!(
            store
                .transition_for_actor(&done.id, done.revision, StoryCommand::WontFix, None)
                .is_err()
        );
        store
            .transition_for_actor(
                &cancelled.id,
                cancelled.revision,
                StoryCommand::WontFix,
                None,
            )
            .expect("cancel");
        assert_eq!(store.plan_state(&plan.id).expect("state"), PlanState::Done);
    }

    #[test]
    fn dependency_cycle_and_stale_revision_are_rejected() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let make_story = |title: &str| NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let a = store.create_story(make_story("A")).expect("A");
        let b = store.create_story(make_story("B")).expect("B");
        store
            .add_dependency(&b.id, &a.id, b.revision)
            .expect("B depends on A");
        assert!(store.add_dependency(&a.id, &b.id, a.revision).is_err());
        assert!(
            store
                .transition_for_actor(&a.id, a.revision, StoryCommand::Approve, None)
                .is_err()
        );
        assert!(
            store
                .transition_for_actor(&b.id, b.revision, StoryCommand::SubmitReview, None)
                .is_err()
        );
        assert_eq!(
            store.get_story(&b.id).expect("B").status,
            StoryStatus::Backlog
        );
        let a = store.claim(&a.id, "tab", a.revision).expect("claim A");
        let a = store
            .transition_for_actor(&a.id, a.revision, StoryCommand::CheckCriterion(0), None)
            .expect("check A");
        let a = store
            .transition_for_actor(&a.id, a.revision, StoryCommand::SubmitReview, None)
            .expect("review A");
        store
            .transition_for_actor(&a.id, a.revision, StoryCommand::Approve, None)
            .expect("approve A");
        assert_eq!(
            store.get_story(&b.id).expect("B").status,
            StoryStatus::Ready
        );
    }

    #[test]
    fn desktop_approval_records_human_provenance() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Story".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check");
        let review = store
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .expect("review");
        let done = store
            .transition(&story.id, review.revision, StoryCommand::Approve)
            .expect("approve");
        drop(store);
        let history = StoryStore::open_at(&db)
            .expect("reopen")
            .transition_history(&story.id)
            .expect("history");
        assert_eq!(history.len(), 4);
        assert_eq!(history.last().expect("approval").revision, done.revision);
        assert_eq!(
            history.last().expect("approval").actor,
            StoryTransitionActor::Human
        );
        assert_eq!(
            history.last().expect("approval").command,
            StoryCommand::Approve
        );
    }

    #[test]
    fn claiming_session_approval_records_managed_provenance() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Story".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let claimed = store
            .claim(&story.id, "implementer", story.revision)
            .expect("claim");
        let checked = store
            .transition_for_actor(
                &story.id,
                claimed.revision,
                StoryCommand::CheckCriterion(0),
                Some("implementer"),
            )
            .expect("check criterion");
        let review = store
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::SubmitReview,
                Some("implementer"),
            )
            .expect("submit review");

        // Catches: actor tracking refusing a valid approval by the claiming session.
        let done = store
            .transition_for_actor(
                &story.id,
                review.revision,
                StoryCommand::Approve,
                Some("implementer"),
            )
            .expect("implementer approval");
        assert_eq!(done.status, StoryStatus::Done);
        assert_eq!(done.claim_session, None);
        assert_eq!(
            store
                .transition_history(&story.id)
                .expect("history")
                .last()
                .expect("approval")
                .actor,
            StoryTransitionActor::ManagedSession {
                session_id: "implementer".into()
            }
        );
    }

    #[test]
    fn local_http_approval_records_its_own_provenance() {
        let dir = tempfile::tempdir().expect("config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Story".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        // Catches: local administrative actions being refused or losing LocalApi provenance.
        let blocked = store
            .transition_from_local_api(&story.id, story.revision, StoryCommand::Block)
            .expect("local block");
        let story = store
            .transition_from_local_api(&story.id, blocked.revision, StoryCommand::Unblock)
            .expect("local unblock");
        assert!(
            store
                .transition_history(&story.id)
                .unwrap()
                .iter()
                .all(|entry| entry.actor == StoryTransitionActor::LocalApi)
        );
        let started = store
            .transition_from_local_api(&story.id, story.revision, StoryCommand::StartManual)
            .expect("local action");
        assert_eq!(
            store
                .transition_history(&story.id)
                .unwrap()
                .last()
                .unwrap()
                .actor,
            StoryTransitionActor::LocalApi
        );
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .unwrap();
        let review = store
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .unwrap();
        let done = store
            .transition_from_local_api(&story.id, review.revision, StoryCommand::Approve)
            .expect("local reviewer approves");
        assert_eq!(done.status, StoryStatus::Done);
        assert_eq!(
            store
                .transition_history(&story.id)
                .expect("history")
                .last()
                .expect("approval")
                .actor,
            StoryTransitionActor::LocalApi
        );
    }

    #[test]
    fn manual_claim_conflicts_and_is_released_with_session() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "A".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let claimed = store
            .claim(&story.id, "tab-one", story.revision)
            .expect("claim");
        // Catches: provenance restricting another caller's criterion update or stealing the claim.
        let claimed = store
            .transition_for_actor(
                &story.id,
                claimed.revision,
                StoryCommand::CheckCriterion(0),
                Some("tab-two"),
            )
            .expect("trusted update");
        assert_eq!(claimed.checked, vec![true]);
        assert_eq!(claimed.claim_session.as_deref(), Some("tab-one"));
        assert_eq!(
            store
                .transition_history(&story.id)
                .expect("history")
                .last()
                .expect("update")
                .actor,
            StoryTransitionActor::ManagedSession {
                session_id: "tab-two".into()
            }
        );
        assert!(store.claim(&story.id, "tab-two", claimed.revision).is_err());
        assert_eq!(store.release_session_claims("tab-one").expect("release"), 1);
        let released = store.get_story(&story.id).expect("story");
        assert!(released.claim_session.is_none());
        assert_eq!(released.status, StoryStatus::Ready);
        store
            .claim(&story.id, "tab-two", released.revision)
            .expect("second claim");
    }

    #[test]
    fn story_scope_cannot_escape_the_project_on_any_platform() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        for scope in [
            "../secret",
            "/tmp/secret",
            "C:\\secret",
            "\\\\server\\share",
        ] {
            let result = store.create_story(NewStory {
                story_type: None,
                plan_id: plan.id.clone(),
                title: "Unsafe scope".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![scope.into()],
            });
            assert!(result.is_err(), "accepted unsafe scope: {scope}");
        }
    }

    #[test]
    fn closed_session_releases_claim_without_creating_an_unused_database() {
        let dir = tempfile::tempdir().expect("temporary config");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let db = dir.path().join("stories.sqlite3");
        assert_eq!(
            StoryStore::release_closed_session("unused").expect("no store"),
            0
        );
        assert!(!db.exists());

        let store = StoryStore::open().expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Work".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        store
            .claim(&story.id, "tab-one", story.revision)
            .expect("claim");
        assert_eq!(
            StoryStore::release_closed_session("tab-one").expect("release"),
            1
        );
        assert_eq!(
            store.get_story(&story.id).expect("story").status,
            StoryStatus::Ready
        );
    }

    #[test]
    fn closed_unclaimed_session_does_not_wait_for_a_story_write_lock() {
        let dir = tempfile::tempdir().expect("temporary config");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let store = StoryStore::open().expect("store");
        let lock = rusqlite::Connection::open(dir.path().join("stories.sqlite3"))
            .expect("open locking connection");
        lock.execute_batch("BEGIN IMMEDIATE")
            .expect("acquire write lock");

        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            sent.send(StoryStore::release_closed_session("unclaimed"))
                .expect("send release result");
        });
        assert_eq!(
            received
                .recv_timeout(std::time::Duration::from_millis(250))
                .expect("unclaimed release must not wait for a write transaction")
                .expect("release"),
            0
        );
        lock.execute_batch("ROLLBACK").expect("release write lock");
        drop(store);
    }

    #[test]
    fn closed_session_release_survives_a_store_without_its_schema() {
        let dir = tempfile::tempdir().expect("temporary config");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        // A file whose schema never landed (for example a crash between open and DDL):
        // the read-only probe cannot see a `stories` table, and teardown must not fail on it.
        rusqlite::Connection::open(dir.path().join("stories.sqlite3"))
            .and_then(|conn| conn.execute_batch("CREATE TABLE unrelated(x INTEGER)"))
            .expect("create schema-less store");
        assert_eq!(
            StoryStore::release_closed_session("unclaimed").expect("release"),
            0
        );
    }

    #[test]
    fn plan_view_derives_transitive_abandonment_and_cancellation_summary() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let create = |title: &str| {
            store
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: title.into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .expect("story")
        };
        let cancelled = create("Cancelled");
        let middle = create("Middle");
        let last = create("Last");
        let middle = store
            .add_dependency(&middle.id, &cancelled.id, middle.revision)
            .expect("edge");
        store
            .add_dependency(&last.id, &middle.id, last.revision)
            .expect("edge");
        store
            .transition_for_actor(
                &cancelled.id,
                cancelled.revision,
                StoryCommand::WontFix,
                None,
            )
            .expect("cancel");
        let view = store.plan_view(&plan.id).expect("view");
        assert_eq!(view.state, PlanState::Active);
        assert_eq!(view.wont_fix_count, 1);
        assert!(!view.all_cancelled);
        assert!(view.stories.iter().all(|story| story.abandoned));
        let empty = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Empty".into(),
                source: "empty.md".into(),
            })
            .expect("empty plan");
        let empty_view = store.plan_view(&empty.id).expect("empty view");
        assert_eq!(empty_view.state, PlanState::Draft);
        assert!(!empty_view.all_cancelled);
        store
            .transition_for_actor(&middle.id, middle.revision, StoryCommand::WontFix, None)
            .expect("cancel middle");
        let last = store.get_story(&last.id).expect("last");
        store
            .transition_for_actor(&last.id, last.revision, StoryCommand::WontFix, None)
            .expect("cancel last");
        let cancelled_view = store.plan_view(&plan.id).expect("cancelled view");
        assert_eq!(cancelled_view.state, PlanState::Done);
        assert_eq!(cancelled_view.wont_fix_count, 3);
        assert!(cancelled_view.all_cancelled);
    }

    #[test]
    fn repeated_wontfix_is_rejected_without_revision_change() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                story_type: None,
                plan_id: plan.id,
                title: "Discard".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let cancelled = store
            .transition_for_actor(&story.id, story.revision, StoryCommand::WontFix, None)
            .expect("cancel");
        assert!(
            store
                .transition_for_actor(&story.id, cancelled.revision, StoryCommand::WontFix, None)
                .is_err()
        );
        assert_eq!(
            store.get_story(&story.id).expect("unchanged").revision,
            cancelled.revision
        );
    }

    #[test]
    fn removal_rejects_a_non_backlog_dependent_even_with_cancelled_edge() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let create = |title: &str| {
            store
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: title.into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .expect("story")
        };
        let target = create("Target");
        let dependent = create("Dependent");
        let dependent = store
            .add_dependency(&dependent.id, &target.id, dependent.revision)
            .expect("edge");
        store
            .transition_for_actor(&target.id, target.revision, StoryCommand::WontFix, None)
            .expect("cancel target");
        let dropped = store
            .transition_for_actor(
                &dependent.id,
                dependent.revision,
                StoryCommand::WontFix,
                None,
            )
            .expect("discard dependent");
        assert!(
            store
                .remove_dependency(&dropped.id, &target.id, dropped.revision, None)
                .is_err()
        );
        assert_eq!(
            store.get_story(&dropped.id).expect("unchanged").revision,
            dropped.revision
        );
        // A legacy record can be Ready while retaining an abandoned edge: the guard applies there too.
        let mut ready = dropped.clone();
        ready.status = StoryStatus::Ready;
        rusqlite::Connection::open(&db)
            .expect("connection")
            .execute(
                "UPDATE stories SET document=?1,status='ready' WHERE id=?2",
                rusqlite::params![serde_json::to_string(&ready).expect("document"), ready.id],
            )
            .expect("legacy ready record");
        assert!(
            store
                .remove_dependency(&ready.id, &target.id, ready.revision, None)
                .is_err()
        );
        assert_eq!(
            store.get_story(&ready.id).expect("unchanged").revision,
            ready.revision
        );
    }

    #[test]
    fn managed_caller_can_remove_an_otherwise_valid_cancelled_edge() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let create = |title: &str| {
            store
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: title.into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .expect("story")
        };
        let target = create("Target");
        let dependent = create("Dependent");
        let dependent = store
            .add_dependency(&dependent.id, &target.id, dependent.revision)
            .expect("edge");
        store
            .transition_for_actor(&target.id, target.revision, StoryCommand::WontFix, None)
            .expect("cancel target");
        // Catches: actor metadata preventing release after a cancelled dependency is removed.
        let released = store
            .remove_dependency(&dependent.id, &target.id, dependent.revision, Some("agent"))
            .expect("managed removal");
        assert_eq!(released.status, StoryStatus::Ready);
        assert_eq!(released.dependencies, [] as [std::string::String; 0]);
        assert_eq!(released.revision, dependent.revision + 1);
    }
}
