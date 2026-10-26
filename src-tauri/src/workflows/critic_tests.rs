//! Critic tests for 955-a3fe: story publish policy and the pre-policy seed migration.
use super::*;

fn open(dir: &tempfile::TempDir) -> WorkflowStore {
    WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store")
}

fn pick(drafts: &[WorkflowDraft], kind: WorkflowKind) -> WorkflowDraft {
    drafts
        .iter()
        .find(|draft| draft.kind == kind)
        .expect("seed")
        .clone()
}

fn check(id: &str) -> CheckDefinition {
    CheckDefinition {
        id: id.into(),
        argv: vec!["git".into(), "status".into()],
        timeout_secs: 30,
    }
}

fn legacy(store: &WorkflowStore, project: &str) -> (WorkflowDraft, WorkflowDraft) {
    store.seed_pre_policy_templates(project).expect("legacy");
    let drafts = store.list_drafts(project).expect("list");
    (
        pick(&drafts, WorkflowKind::Story),
        pick(&drafts, WorkflowKind::Plan),
    )
}

fn edit_coordinator_prompt(plan: &WorkflowDraft) -> WorkflowGraph {
    let mut graph = plan.graph.clone();
    for node in &mut graph.nodes {
        if let NodeKind::Agent {
            prompt_template, ..
        } = &mut node.kind
        {
            *prompt_template = "User plan edit".into();
        }
    }
    graph
}

#[test]
fn edited_plan_with_untouched_story_keeps_the_users_plan() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let (_story, plan) = legacy(&store, "/project");
    let edited = store
        .update_draft(
            &plan.id,
            plan.draft_revision,
            edit_coordinator_prompt(&plan),
        )
        .unwrap();
    let after = pick(
        &store.seed_templates("/project").unwrap(),
        WorkflowKind::Plan,
    );
    // catches: the migration judging only the story definition and republishing over a plan
    // the user edited (the edit is lost and a rev 2 the user never wrote goes live).
    assert_eq!(after, edited);
    assert_eq!(after.latest_published_revision, 1);
    assert_eq!(
        store.get_published(&plan.id, 2).unwrap_err(),
        "published workflow revision not found"
    );
}

#[test]
fn migration_only_touches_the_project_being_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    legacy(&store, "/project-a");
    let (other_story, other_plan) = legacy(&store, "/project-b");
    let a = store.seed_templates("/project-a").unwrap();
    // catches: a built-in lookup without the project filter, migrating (or erroring on) another
    // project's seeds when only one project is loaded.
    assert_eq!(pick(&a, WorkflowKind::Story).latest_published_revision, 2);
    assert_eq!(
        store.get_draft(&other_story.id).unwrap(),
        other_story,
        "project-b story must stay at its legacy revision"
    );
    assert_eq!(store.get_draft(&other_plan.id).unwrap(), other_plan);
}

#[test]
fn migrated_seed_can_be_edited_and_republished_as_revision_three() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let (story, _plan) = legacy(&store, "/project");
    let migrated = pick(
        &store.seed_templates("/project").unwrap(),
        WorkflowKind::Story,
    );
    assert_eq!(migrated.id, story.id);
    // catches: the migration advancing draft/publish counters inconsistently, so the migrated
    // draft cannot be republished ("already published") or revision 3 collides with revision 2.
    assert_eq!(
        store
            .publish(&migrated.id, migrated.draft_revision)
            .unwrap_err(),
        "draft revision has already been published"
    );
    let mut checks = migrated.required_checks.clone();
    checks.push(check("unit"));
    let edited = store
        .update_checks(&migrated.id, migrated.draft_revision, checks.clone())
        .unwrap();
    let third = store.publish(&migrated.id, edited.draft_revision).unwrap();
    assert_eq!(third.revision, 3);
    assert_eq!(third.required_checks, checks);
    assert_eq!(
        store
            .get_published(&migrated.id, 2)
            .unwrap()
            .required_checks
            .len(),
        1,
        "revision 2 stays immutable"
    );
    // a later load must not re-run the migration over the user's revision 3.
    let after = pick(
        &store.seed_templates("/project").unwrap(),
        WorkflowKind::Story,
    );
    assert_eq!(after.latest_published_revision, 3);
    assert_eq!(after.required_checks, checks);
}

#[test]
fn a_draft_reports_the_draft_revision_it_last_published() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let (story, _plan) = legacy(&store, "/project");
    // catches: the designer cannot tell an unchanged draft from an edited one, so Publish
    // stays enabled and the click fails with "already been published".
    assert_eq!(story.last_published_draft_revision, story.draft_revision);
    let mut checks = story.required_checks.clone();
    checks.push(check("unit"));
    let edited = store
        .update_checks(&story.id, story.draft_revision, checks)
        .unwrap();
    assert_ne!(edited.last_published_draft_revision, edited.draft_revision);
    store.publish(&story.id, edited.draft_revision).unwrap();
    let published = store.get_draft(&story.id).unwrap();
    assert_eq!(
        published.last_published_draft_revision,
        published.draft_revision
    );
}

#[test]
fn concurrent_loads_migrate_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let (story, plan) = legacy(&store, "/project");
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || store.seed_templates("/project"))
        })
        .collect();
    for handle in handles {
        // catches: a read-then-write migration outside one immediate transaction, where a
        // second loader collides on the revision 2 primary key or publishes a revision 3.
        handle
            .join()
            .unwrap()
            .expect("every concurrent load succeeds");
    }
    assert_eq!(
        store
            .get_draft(&story.id)
            .unwrap()
            .latest_published_revision,
        2
    );
    assert_eq!(
        store.get_draft(&plan.id).unwrap().latest_published_revision,
        2
    );
    assert_eq!(
        store.get_published(&story.id, 3).unwrap_err(),
        "published workflow revision not found"
    );
}

#[test]
fn clearing_checks_after_a_publish_blocks_the_next_story_publish() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let draft = store
        .create_draft(
            "/project",
            "Delivery",
            WorkflowKind::Story,
            pick(
                &store.seed_templates("/project").unwrap(),
                WorkflowKind::Story,
            )
            .graph,
        )
        .unwrap();
    let draft = store
        .update_checks(&draft.id, draft.draft_revision, vec![check("unit")])
        .unwrap();
    let first = store.publish(&draft.id, draft.draft_revision).unwrap();
    assert_eq!(first.revision, 1);
    let cleared = store
        .update_checks(&draft.id, draft.draft_revision, vec![])
        .unwrap();
    // catches: the empty-policy guard applying only to a story's first publication, so a later
    // revision silently drops every check.
    assert!(
        store
            .publish(&draft.id, cleared.draft_revision)
            .unwrap_err()
            .contains("required check")
    );
    // the rejected publish leaves no partial state: adding a check publishes revision 2.
    assert_eq!(
        store
            .get_draft(&draft.id)
            .unwrap()
            .latest_published_revision,
        1
    );
    let fixed = store
        .update_checks(&draft.id, cleared.draft_revision, vec![check("unit")])
        .unwrap();
    assert_eq!(
        store
            .publish(&draft.id, fixed.draft_revision)
            .unwrap()
            .revision,
        2
    );
}
