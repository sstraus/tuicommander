//! Critic tests for 1169-b790: dependency receipts, approval rule, migration, proposals.
use super::*;
use std::path::Path;

fn open(dir: &Path) -> StoryStore {
    StoryStore::open_at(&dir.join("stories.sqlite3")).expect("store")
}

fn plan(store: &StoryStore, title: &str) -> Plan {
    store
        .create_plan(NewPlan {
            project: crate::test_support::test_temp_root()
                .canonicalize()
                .expect("project")
                .to_string_lossy()
                .into_owned(),
            title: title.into(),
            source: format!("{title}.md"),
        })
        .expect("plan")
}

fn new_story(plan_id: &str, title: &str) -> NewStory {
    NewStory {
        story_type: None,
        plan_id: plan_id.into(),
        title: title.into(),
        criteria: vec!["Done".into()],
        priority: 1,
        origin: StoryOrigin::Native,
        file_scope: vec![],
    }
}

fn story(store: &StoryStore, plan_id: &str, title: &str) -> Story {
    store
        .create_story(new_story(plan_id, title))
        .expect("story")
}

/// Human path to Review.
fn to_review(store: &StoryStore, id: &str) -> Story {
    let s = store.get_story(id).expect("story");
    let s = store
        .transition(id, s.revision, StoryCommand::StartManual)
        .expect("start");
    let s = store
        .transition(id, s.revision, StoryCommand::CheckCriterion(0))
        .expect("check");
    store
        .transition(id, s.revision, StoryCommand::SubmitReview)
        .expect("review")
}

fn to_done(store: &StoryStore, id: &str) -> Story {
    let s = to_review(store, id);
    store
        .transition(id, s.revision, StoryCommand::Approve)
        .expect("approve")
}

/// Registers a workflow run row for `plan_id`, as a started workflow would.
fn own_plan_by_run(dir: &Path, plan_id: &str) {
    let _guard = crate::config::set_config_dir_override(dir.to_path_buf());
    let project = open(dir).get_plan(plan_id).expect("plan").project;
    let definitions = crate::workflows::WorkflowStore::open().expect("definitions");
    let template = definitions
        .seed_templates(&project)
        .expect("templates")
        .into_iter()
        .find(|draft| draft.kind == crate::workflows::WorkflowKind::Plan)
        .expect("plan template");
    crate::workflows::RunStore::open()
        .expect("runs")
        .start_plan(
            &project,
            plan_id,
            &template.id,
            template.latest_published_revision,
            crate::workflows::RunLimits::default(),
        )
        .expect("valid persisted run");
}

/// Catches: an empty or schema-less workflow_runs.sqlite3 beside the story DB making every
/// dependency check fail with "no such table" instead of meaning "no workflow owns the plan".
#[test]
fn a_run_store_file_without_schema_does_not_block_dependency_checks() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("workflow_runs.sqlite3"), b"").expect("empty file");
    let store = open(dir.path());
    let p = plan(&store, "p");
    let a = story(&store, &p.id, "A");
    let b = story(&store, &p.id, "B");
    let a = to_done(&store, &a.id);
    let b = store
        .add_dependency(&b.id, &a.id, b.revision)
        .expect("add dependency");
    assert_eq!(b.status, StoryStatus::Ready);
    store.claim(&b.id, "s1", b.revision).expect("claim");
}

/// Catches: a Done dependency satisfying a story in a workflow-owned plan without any
/// integration receipt (add_dependency keeps it Ready, claim and start_manual succeed).
#[test]
fn done_dependency_without_receipt_blocks_in_a_workflow_owned_plan() {
    let dir = tempfile::tempdir().expect("dir");
    let store = open(dir.path());
    let p = plan(&store, "owned");
    let a = story(&store, &p.id, "A");
    let b = story(&store, &p.id, "B");
    let a = to_done(&store, &a.id);
    own_plan_by_run(dir.path(), &p.id);

    let b = store
        .add_dependency(&b.id, &a.id, b.revision)
        .expect("add dependency");
    assert_eq!(b.status, StoryStatus::Backlog, "no receipt: demoted");

    let claim = store
        .claim(&b.id, "s1", b.revision)
        .expect_err("claim refused");
    assert!(
        claim.contains(&a.id),
        "the refusal names the unmet dependency: {claim}"
    );
    store
        .reconcile_integrated_dependencies(&p.id)
        .expect("reconcile");
    assert_eq!(
        store.get_story(&b.id).expect("b").status,
        StoryStatus::Backlog
    );
    let start = store.transition(&b.id, b.revision, StoryCommand::StartManual);
    assert!(start.is_err(), "start_manual needs a Ready story");
}

/// Catches: the receipt requirement leaking to plans no workflow run owns.
#[test]
fn receipt_is_not_required_in_a_plan_without_a_run() {
    let dir = tempfile::tempdir().expect("dir");
    let store = open(dir.path());
    let owned = plan(&store, "owned");
    let free = plan(&store, "free");
    own_plan_by_run(dir.path(), &owned.id);
    let a = story(&store, &free.id, "A");
    let b = story(&store, &free.id, "B");
    let a = to_done(&store, &a.id);
    let b = store
        .add_dependency(&b.id, &a.id, b.revision)
        .expect("dependency");
    assert_eq!(b.status, StoryStatus::Ready);
}

/// Catches: WontFix counting as satisfied (promote on an unrelated approval, or add_dependency
/// leaving the dependent Ready), with and without a workflow run (Boss 2026-09-28).
#[test]
fn wont_fix_never_satisfies_a_dependency() {
    for owned in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let store = open(dir.path());
        let p = plan(&store, "p");
        if owned {
            own_plan_by_run(dir.path(), &p.id);
        }
        let a = story(&store, &p.id, "A");
        let b = story(&store, &p.id, "B");
        let c = story(&store, &p.id, "C");
        let a = store
            .transition(&a.id, a.revision, StoryCommand::WontFix)
            .expect("wontfix");
        let b = store
            .add_dependency(&b.id, &a.id, b.revision)
            .expect("dependency");
        assert_eq!(b.status, StoryStatus::Backlog, "owned={owned}");
        to_done(&store, &c.id);
        let b = store.get_story(&b.id).expect("b");
        assert_eq!(
            b.status,
            StoryStatus::Backlog,
            "owned={owned}: unrelated approval promoted it"
        );
        store
            .reconcile_integrated_dependencies(&p.id)
            .expect("reconcile");
        assert_eq!(
            store.get_story(&b.id).expect("b").status,
            StoryStatus::Backlog,
            "owned={owned}"
        );
        let err = store.claim(&b.id, "s", b.revision).expect_err("claim");
        assert!(err.contains(&a.id), "{err}");
        let b = store
            .remove_dependency(&b.id, &a.id, b.revision, None)
            .expect("user removes it");
        assert_eq!(b.status, StoryStatus::Ready, "owned={owned}");
    }
}

/// Catches: a not-Done (InProgress/Review) dependency leaving the dependent claimable.
#[test]
fn unfinished_dependency_demotes_and_is_named_in_claim_refusal() {
    let dir = tempfile::tempdir().expect("dir");
    let store = open(dir.path());
    let p = plan(&store, "p");
    let a = story(&store, &p.id, "A");
    let b = story(&store, &p.id, "B");
    let a = to_review(&store, &a.id);
    let b = store
        .add_dependency(&b.id, &a.id, b.revision)
        .expect("dependency");
    assert_eq!(b.status, StoryStatus::Backlog);
    let err = store.claim(&b.id, "s", b.revision).expect_err("claim");
    assert!(err.contains(&a.id), "{err}");
    store
        .transition(&a.id, a.revision, StoryCommand::Approve)
        .expect("approve");
    assert_eq!(
        store.get_story(&b.id).expect("b").status,
        StoryStatus::Ready
    );
}

/// Catches: a story DB written before story_transitions/workflow_story_proposals existed
/// failing to open, failing history reads, or losing its rows.
#[test]
fn a_story_database_from_before_this_change_opens_and_keeps_working() {
    let dir = tempfile::tempdir().expect("dir");
    let (p, a, b) = {
        let store = open(dir.path());
        let p = plan(&store, "legacy");
        let a = story(&store, &p.id, "A");
        let b = story(&store, &p.id, "B");
        to_done(&store, &a.id);
        (p, a, b)
    };
    rusqlite::Connection::open(dir.path().join("stories.sqlite3"))
        .expect("raw")
        .execute_batch("DROP TABLE story_transitions; DROP TABLE workflow_story_proposals;")
        .expect("drop new tables");

    let store = open(dir.path());
    assert_eq!(
        store
            .transition_history(&a.id)
            .expect("history of old story"),
        [] as [crate::stories::model::StoryTransition; 0]
    );
    assert_eq!(store.get_story(&a.id).expect("a").status, StoryStatus::Done);
    let b = store
        .add_dependency(&b.id, &a.id, b.revision)
        .expect("dependency");
    assert_eq!(
        b.status,
        StoryStatus::Ready,
        "old Done row satisfies a plan with no run"
    );
    to_done(&store, &b.id);
    let history = store.transition_history(&b.id).expect("history");
    assert_eq!(
        history.last().expect("approval").command,
        StoryCommand::Approve
    );
    assert_eq!(store.list_stories(&p.id).expect("list").len(), 2);
}

/// Catches: actor metadata refusing administrative transitions or dropping managed provenance.
#[test]
fn managed_administrative_transitions_record_each_actor() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("dir");
    let store = open(dir.path());
    let p = plan(&store, "p");
    let mut a = story(&store, &p.id, "A");
    for (command, actor) in [
        (StoryCommand::Block, "impl"),
        (StoryCommand::Unblock, "reviewer"),
        (StoryCommand::StartManual, "impl"),
        (StoryCommand::CheckCriterion(0), "reviewer"),
        (StoryCommand::SubmitReview, "reviewer"),
        (StoryCommand::RejectReview, "impl"),
        (StoryCommand::WontFix, "reviewer"),
    ] {
        a = store
            .transition_for_actor(&a.id, a.revision, command.clone(), Some(actor))
            .expect("trusted actor action");
        let history = store.transition_history(&a.id).expect("history");
        let last = history.last().expect("transition");
        assert_eq!(last.command, command);
        assert_eq!(
            last.actor,
            StoryTransitionActor::ManagedSession {
                session_id: actor.into()
            }
        );
        assert_eq!(last.revision, a.revision);
    }
    assert_eq!(a.status, StoryStatus::WontFix);
}

/// Catches: an agent approving a story that is not in review, or approving with a stale
/// revision, or a second approval being recorded.
#[test]
fn agent_approval_needs_review_status_and_current_revision() {
    let dir = tempfile::tempdir().expect("dir");
    let store = open(dir.path());
    let p = plan(&store, "p");
    let a = story(&store, &p.id, "A");
    let err = store
        .transition_for_actor(&a.id, a.revision, StoryCommand::Approve, Some("reviewer"))
        .expect_err("ready story");
    assert!(err.contains("review"), "{err}");
    let review = to_review(&store, &a.id);
    let err = store
        .transition_for_actor(
            &a.id,
            review.revision - 1,
            StoryCommand::Approve,
            Some("reviewer"),
        )
        .expect_err("stale");
    assert!(
        err.to_lowercase().contains("revision") || err.to_lowercase().contains("stale"),
        "{err}"
    );
    let done = store
        .transition_for_actor(
            &a.id,
            review.revision,
            StoryCommand::Approve,
            Some("reviewer"),
        )
        .expect("approve");
    let err = store
        .transition_for_actor(
            &a.id,
            done.revision,
            StoryCommand::Approve,
            Some("reviewer2"),
        )
        .expect_err("second approval");
    assert!(err.contains("review"), "{err}");
    let approvals = store
        .transition_history(&a.id)
        .expect("history")
        .into_iter()
        .filter(|t| t.command == StoryCommand::Approve)
        .count();
    assert_eq!(approvals, 1);
}

/// Catches: a retried proposal creating a second story, a reused key with another payload
/// being accepted, or a missing plan leaving a proposal row behind.
#[test]
fn story_proposal_keys_are_idempotent_and_payload_bound() {
    let dir = tempfile::tempdir().expect("dir");
    let store = open(dir.path());
    let p = plan(&store, "p");
    let input = new_story(&p.id, "Proposed");
    let first = store
        .create_story_once("run-1", "k1", input.clone())
        .expect("first");
    let again = store
        .create_story_once("run-1", "k1", input.clone())
        .expect("retry");
    assert_eq!(first.id, again.id);
    let mut other = input.clone();
    other.criteria = vec!["Different".into()];
    assert!(
        store
            .create_story_once("run-1", "k1", other.clone())
            .is_err()
    );
    assert!(
        store
            .existing_story_for_proposal("run-1", "k1", &other)
            .is_err()
    );
    assert_eq!(
        store
            .existing_story_for_proposal("run-1", "k1", &input)
            .expect("observe")
            .map(|s| s.id),
        Some(first.id.clone())
    );
    assert!(
        store
            .existing_story_for_proposal("run-1", "unknown", &input)
            .expect("none")
            .is_none()
    );
    let other_run = store
        .create_story_once("run-2", "k1", input.clone())
        .expect("other run");
    assert_ne!(other_run.id, first.id);
    assert_eq!(store.list_stories(&p.id).expect("list").len(), 2);

    let missing = new_story("no-such-plan", "X");
    assert!(
        store
            .create_story_once("run-1", "k2", missing.clone())
            .is_err()
    );
    assert!(
        store
            .existing_story_for_proposal("run-1", "k2", &missing)
            .expect("probe")
            .is_none()
    );
    assert!(store.create_story_once("", "k3", input.clone()).is_err());
    assert!(store.create_story_once("run-1", "", input).is_err());
}

/// Catches: two store handles (host plus CLI/HTTP) racing the same proposal key and creating
/// two stories because the lookup and the insert are not one transaction.
#[test]
fn concurrent_proposals_with_one_key_create_one_story() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("stories.sqlite3");
    let store = StoryStore::open_at(&path).expect("store");
    let p = plan(&store, "p");
    let input = new_story(&p.id, "Raced");
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let input = input.clone();
            std::thread::spawn(move || {
                StoryStore::open_at(&path)
                    .expect("open")
                    .create_story_once("run-1", "k", input)
                    .expect("proposal")
                    .id
            })
        })
        .collect();
    let ids: Vec<String> = handles
        .into_iter()
        .map(|h| h.join().expect("thread"))
        .collect();
    assert!(ids.iter().all(|id| *id == ids[0]), "{ids:?}");
    assert_eq!(store.list_stories(&p.id).expect("list").len(), 1);
}

/// Catches: tracking-only local requests bypassing revision or criterion bounds,
/// or a refused update leaving persisted state or a transition history row behind.
#[test]
fn refused_local_updates_preserve_story_and_transition_history() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("dir");
    let store = open(dir.path());
    let p = plan(&store, "local refusals");
    let initial = story(&store, &p.id, "A");
    let started = store
        .transition_from_local_api(&initial.id, initial.revision, StoryCommand::StartManual)
        .expect("trusted local start");
    let before = store.transition_history(&initial.id).expect("history");

    for (revision, command, cause) in [
        (
            started.revision,
            StoryCommand::CheckCriterion(1),
            "out of range",
        ),
        (initial.revision, StoryCommand::WontFix, "revision"),
    ] {
        let error = store
            .transition_from_local_api(&initial.id, revision, command)
            .expect_err("invalid local update");
        assert!(error.contains(cause), "{error}");
        let persisted = store.get_story(&initial.id).expect("persisted story");
        assert_eq!(persisted.status, StoryStatus::InProgress);
        assert_eq!(persisted.revision, started.revision);
        assert_eq!(persisted.checked, vec![false]);
        assert_eq!(persisted.claim_session, None);
        assert_eq!(
            store.transition_history(&initial.id).expect("history"),
            before
        );
    }
}
