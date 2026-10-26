//! Critic tests for 955-a3fe: the check policy at integration, recertification and run start.
use super::*;
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStore};
use crate::workflows::{WorkflowKind, WorkflowStore};
use std::path::PathBuf;

pub(super) struct Flow {
    _config: tempfile::TempDir,
    _project: tempfile::TempDir,
    repo: PathBuf,
    pub(super) story_id: String,
    revision: i64,
    pub(super) store: RunStore,
    pub(super) run_id: String,
    pub(super) sequence: i64,
}

fn git(path: &std::path::Path, args: &[&str]) {
    crate::git_cli::git_cmd(path).args(args).run().unwrap();
}

/// A project repo, a story worktree on branch `story`, and a run whose story is accepted.
/// `legacy` seeds the pre-policy built-ins (story_delivery rev 1 without checks).
pub(super) fn accepted_flow(legacy: bool) -> (Flow, impl Drop) {
    let config = tempfile::tempdir().expect("config");
    let project = tempfile::tempdir().expect("project");
    let guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let repo = project.path().canonicalize().expect("canonical project");
    let project_path = repo.to_string_lossy().to_string();
    let stories = StoryStore::open().expect("stories");
    let plan = stories
        .create_plan(NewPlan {
            project: project_path.clone(),
            title: "Resolve".into(),
            source: "plan.md".into(),
        })
        .expect("plan");
    let story = stories
        .create_story(NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: "Work".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .expect("story");
    let definitions = WorkflowStore::open().expect("definitions");
    let templates = if legacy {
        definitions
            .seed_pre_policy_templates(&project_path)
            .expect("legacy seed");
        definitions.list_drafts(&project_path).expect("list")
    } else {
        definitions.seed_templates(&project_path).expect("seed")
    };
    let definition_id = templates
        .into_iter()
        .find(|draft| draft.kind == WorkflowKind::Plan)
        .expect("plan template")
        .id;
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.name", "Workflow Test"],
        vec!["config", "user.email", "workflow@example.invalid"],
    ] {
        git(&repo, &args);
    }
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-qm", "base"]);
    let worktree = config.path().join("story-worktree");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "story",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("story.txt"), "accepted\n").unwrap();
    git(&worktree, &["add", "story.txt"]);
    git(&worktree, &["commit", "-qm", "story"]);

    let store = RunStore::open().expect("run store");
    // A legacy run was started before the start-time guard existed.
    let run = if legacy {
        store.start_plan_pre_policy(
            &project_path,
            &plan.id,
            &definition_id,
            1,
            RunLimits::default(),
        )
    } else {
        store.start_plan(
            &project_path,
            &plan.id,
            &definition_id,
            1,
            RunLimits::default(),
        )
    }
    .expect("run");
    let started = store
        .command(
            &run.id,
            "attempt",
            RunCommand::StartAttempt {
                story_id: story.id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
    let attempt = started.snapshot.attempts[0].clone();
    store
        .command(
            &run.id,
            "worktree",
            RunCommand::AssignWorktree {
                story_id: story.id.clone(),
                path: worktree
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            },
        )
        .unwrap();
    store
        .command(
            &run.id,
            "report",
            RunCommand::ReportAttempt {
                attempt_id: attempt.id.clone(),
                generation: attempt.generation,
                outcome: AttemptOutcome::Completed,
            },
        )
        .unwrap();
    let current = stories.get_story(&story.id).unwrap();
    let claimed = stories
        .claim(&story.id, "implementer", current.revision)
        .unwrap();
    let checked = stories
        .transition_for_actor(
            &story.id,
            claimed.revision,
            StoryCommand::CheckCriterion(0),
            Some("implementer"),
        )
        .unwrap();
    let reviewed = stories
        .transition_for_actor(
            &story.id,
            checked.revision,
            StoryCommand::SubmitReview,
            Some("implementer"),
        )
        .unwrap();
    let done = stories
        .transition_for_actor(
            &story.id,
            reviewed.revision,
            StoryCommand::Approve,
            Some("reviewer"),
        )
        .unwrap();
    let accepted = store
        .command(
            &run.id,
            "accept",
            RunCommand::AcceptStory {
                story_id: story.id.clone(),
            },
        )
        .unwrap();
    (
        Flow {
            _config: config,
            _project: project,
            repo,
            story_id: story.id,
            revision: done.revision,
            store,
            run_id: run.id,
            sequence: accepted.sequence,
        },
        guard,
    )
}

#[test]
fn seeded_default_policy_passes_a_real_integration() {
    let (flow, _guard) = accepted_flow(false);
    // catches: a default seeded check that always fails (or cannot run in a linked worktree), so
    // no seeded workflow can ever integrate even though every policy-shape assertion passes.
    let checked = flow
        .store
        .execute_check(
            &flow.run_id,
            &flow.story_id,
            "repository-integrity",
            "check",
            flow.sequence,
        )
        .expect("default check runs in the story worktree");
    git(&flow.repo, &["merge", "--no-ff", "--no-edit", "story"]);
    let integrated = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", checked.sequence)
        .expect("default policy integrates a clean merge");
    let receipt = integrated
        .snapshot
        .stories
        .iter()
        .find(|item| item.story_id == flow.story_id)
        .unwrap()
        .integration_receipt
        .clone()
        .unwrap();
    assert_eq!(receipt.post_checks.len(), 1);
    assert_eq!(receipt.post_checks[0].check_id, "repository-integrity");
    assert_eq!(receipt.post_checks[0].exit_code, 0);
    assert!(story_integrated_at_revision(&flow.story_id, flow.revision).unwrap());
}

#[test]
fn unchecked_definition_records_no_receipt_and_names_the_definition() {
    let (flow, _guard) = accepted_flow(true);
    git(&flow.repo, &["merge", "--no-ff", "--no-edit", "story"]);
    let error = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", flow.sequence)
        .unwrap_err();
    // catches: the guard missing from the service path, so a run pinned to an unchecked rev 1
    // records an integration receipt with zero post-checks.
    assert!(
        error.contains("Story delivery") && error.contains("revision 1"),
        "{error}"
    );
    let snapshot = flow.store.snapshot(&flow.run_id).unwrap();
    assert_eq!(snapshot.sequence, flow.sequence);
    assert!(
        snapshot
            .stories
            .iter()
            .all(|item| item.integration_receipt.is_none())
    );
    assert!(!story_integrated_at_revision(&flow.story_id, flow.revision).unwrap());
}

#[test]
fn unchecked_definition_is_reported_before_the_user_must_merge() {
    let (flow, _guard) = accepted_flow(true);
    // canonical HEAD is not yet a merge of the story: nothing has been merged.
    let error = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "early", flow.sequence)
        .unwrap_err();
    // catches: the policy error surfacing only after the merge, so the user merges into the
    // canonical branch first and only then learns the definition can never integrate.
    assert!(error.contains("required checks"), "{error}");
}

#[test]
fn empty_recertification_cannot_certify_an_advanced_head() {
    let (flow, _guard) = accepted_flow(false);
    let checked = flow
        .store
        .execute_check(
            &flow.run_id,
            &flow.story_id,
            "repository-integrity",
            "check",
            flow.sequence,
        )
        .unwrap();
    git(&flow.repo, &["merge", "--no-ff", "--no-edit", "story"]);
    let integrated = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", checked.sequence)
        .unwrap();
    std::fs::write(flow.repo.join("later.txt"), "later\n").unwrap();
    git(&flow.repo, &["add", "later.txt"]);
    git(&flow.repo, &["commit", "-qm", "later"]);
    let head = super::check::git_output(&flow.repo, &["rev-parse", "HEAD"]).unwrap();
    let tree = super::check::git_output(&flow.repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let mut snapshot = integrated.snapshot.clone();
    snapshot.canonical_recertification = Some(CanonicalReceipt {
        canonical_ref: "refs/heads/main".into(),
        commit: head.clone(),
        tree: tree.clone(),
        post_checks: vec![],
    });
    // catches: the second `!post_checks.is_empty()` guard being dropped, so an empty
    // recertification vacuously certifies an advanced head and releases dependents.
    assert!(!super::store::receipt_current(&snapshot, &flow.story_id, flow.revision).unwrap());
    snapshot
        .canonical_recertification
        .as_mut()
        .unwrap()
        .post_checks
        .push(CheckReceipt {
            check_id: "repository-integrity".into(),
            argv: vec!["git".into(), "fsck".into()],
            exit_code: 0,
            ref_name: "refs/heads/main".into(),
            commit: head,
            tree,
            duration_ms: 1,
        });
    // control: the same snapshot with one passing post-check is current.
    assert!(super::store::receipt_current(&snapshot, &flow.story_id, flow.revision).unwrap());
}

#[test]
fn start_plan_rejects_a_pinned_story_definition_without_checks() {
    let config = tempfile::tempdir().expect("config");
    let project = tempfile::tempdir().expect("project");
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project_path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(NewPlan {
            project: project_path.clone(),
            title: "Resolve".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let definitions = WorkflowStore::open().unwrap();
    definitions
        .seed_pre_policy_templates(&project_path)
        .unwrap();
    let plan_definition = definitions
        .list_drafts(&project_path)
        .unwrap()
        .into_iter()
        .find(|draft| draft.kind == WorkflowKind::Plan)
        .unwrap();
    let store = RunStore::open().unwrap();
    // catches: a run launching agents against a story definition that is guaranteed to fail at
    // integration, so the whole implementation cycle is spent before the missing policy surfaces.
    let error = store
        .start_plan(
            &project_path,
            &plan.id,
            &plan_definition.id,
            1,
            RunLimits::default(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("required checks"), "{error}");
}

fn migrated_start_fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    impl Drop,
    String,
    String,
    String,
) {
    let config = tempfile::tempdir().expect("config");
    let project = tempfile::tempdir().expect("project");
    let guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project_path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let plan = StoryStore::open()
        .unwrap()
        .create_plan(NewPlan {
            project: project_path.clone(),
            title: "Resolve".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let definitions = WorkflowStore::open().unwrap();
    definitions
        .seed_pre_policy_templates(&project_path)
        .unwrap();
    // The upgrade path: opening the project lists drafts, which seeds and migrates.
    let plan_definition = definitions
        .seed_templates(&project_path)
        .unwrap()
        .into_iter()
        .find(|draft| draft.kind == WorkflowKind::Plan)
        .unwrap();
    (
        config,
        project,
        guard,
        project_path,
        plan.id,
        plan_definition.id,
    )
}

#[test]
fn start_plan_checks_the_pinned_story_revision_not_the_latest_one() {
    let (_config, _project, _guard, project_path, plan_id, definition_id) =
        migrated_start_fixture();
    let store = RunStore::open().unwrap();
    // catches: the policy being read from the latest published story revision (or the draft)
    // instead of the revision the plan pins, so plan revision 1 (pinned to the unchecked story
    // revision 1) starts a run that can never integrate just because story revision 2 has checks.
    let error = store
        .start_plan(
            &project_path,
            &plan_id,
            &definition_id,
            1,
            RunLimits::default(),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("Story delivery") && error.to_string().contains("revision 1"),
        "{error}"
    );
    // catches: the rejection happening after the run row is written, leaving a Running run that
    // blocks the plan and dispatches nothing.
    assert_eq!(
        store.list_plan_runs(&project_path, &plan_id, 10).unwrap(),
        [] as [crate::workflows::run::model::RunSnapshot; 0]
    );
    // control: the migrated plan revision pins the checked story revision and starts.
    let run = store
        .start_plan(
            &project_path,
            &plan_id,
            &definition_id,
            2,
            RunLimits::default(),
        )
        .unwrap();
    assert_eq!(run.story_definition_revision, 2);
}

#[test]
fn run_started_before_the_upgrade_stays_pinned_to_its_unchecked_revision() {
    let (flow, _guard) = accepted_flow(true);
    let project = flow.repo.to_string_lossy().to_string();
    // The user opens the project after upgrading: story revision 2 now carries the default policy.
    let definitions = WorkflowStore::open()
        .unwrap()
        .seed_templates(&project)
        .unwrap();
    let story = definitions
        .iter()
        .find(|draft| draft.kind == WorkflowKind::Story)
        .unwrap();
    assert_eq!(story.latest_published_revision, 2);
    git(&flow.repo, &["merge", "--no-ff", "--no-edit", "story"]);
    // catches: integration re-reading the latest story revision, so an in-flight run silently
    // changes policy mid-run; the run's pin (revision 1) decides, and it fails by name.
    let error = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", flow.sequence)
        .unwrap_err();
    assert!(error.contains("revision 1"), "{error}");
    assert!(!story_integrated_at_revision(&flow.story_id, flow.revision).unwrap());
}

// Run the actual published check, retaining the same inputs execute_check holds
// while its subprocess runs. The completion phase is exercised against real stores.
fn completed_check(flow: &Flow) -> (StoryExecution, CheckReceipt) {
    let snapshot = flow.store.snapshot(&flow.run_id).unwrap();
    let execution = snapshot
        .stories
        .iter()
        .find(|item| item.story_id == flow.story_id)
        .unwrap()
        .clone();
    let definition = WorkflowStore::open()
        .unwrap()
        .get_published(
            &snapshot.story_definition_id,
            snapshot.story_definition_revision,
        )
        .unwrap();
    let path = std::path::Path::new(execution.worktree_path.as_deref().unwrap());
    let receipt = super::check::execute_run_check(
        &definition.required_checks[0],
        path,
        path,
        &flow.run_id,
        false,
    )
    .unwrap();
    (execution, receipt)
}

#[test]
fn completed_check_survives_an_unrelated_event_and_replays_at_its_original_cursor() {
    // catches: a different worker's event discarding valid check output, or making retries
    // fail because the receipt sequence is no longer expected_sequence + 1.
    let (flow, _guard) = accepted_flow(false);
    let (execution, receipt) = completed_check(&flow);
    let unrelated = flow
        .store
        .command(
            &flow.run_id,
            "notification",
            RunCommand::ReserveEffect {
                key: "notify-parent".into(),
                kind: EffectKind::Notify,
            },
        )
        .unwrap();
    let recorded = flow
        .store
        .commit_completed_check(&flow.run_id, &execution, "check", flow.sequence, receipt)
        .expect("unrelated event must not invalidate the check");
    let mut duplicate = recorded.snapshot.stories[0].check_receipts[0].clone();
    duplicate.duration_ms += 1;
    assert_eq!(
        flow.store
            .commit_completed_check(&flow.run_id, &execution, "check", flow.sequence, duplicate,)
            .expect("duplicate in-flight completion returns the first receipt"),
        recorded
    );
    assert_eq!(recorded.sequence, unrelated.sequence + 1);
    assert_eq!(recorded.snapshot.stories[0].check_receipts.len(), 1);
    assert_eq!(
        flow.store
            .execute_check(
                &flow.run_id,
                &flow.story_id,
                "repository-integrity",
                "check",
                flow.sequence,
            )
            .unwrap(),
        recorded
    );
    assert!(
        flow.store
            .execute_check(
                &flow.run_id,
                &flow.story_id,
                "repository-integrity",
                "check",
                recorded.sequence,
            )
            .unwrap_err()
            .contains("different payload")
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().sequence,
        recorded.sequence
    );
}

#[test]
fn completed_check_rejects_a_changed_git_artifact_without_persisting() {
    // catches: accepting a successful result after someone edits the checked worktree.
    let (flow, _guard) = accepted_flow(false);
    let (execution, receipt) = completed_check(&flow);
    std::fs::write(
        std::path::Path::new(execution.worktree_path.as_deref().unwrap()).join("story.txt"),
        "changed after check\n",
    )
    .unwrap();
    assert!(
        flow.store
            .commit_completed_check(&flow.run_id, &execution, "check", flow.sequence, receipt,)
            .unwrap_err()
            .contains("clean worktree")
    );
    let snapshot = flow.store.snapshot(&flow.run_id).unwrap();
    assert_eq!(snapshot.sequence, flow.sequence);
    assert_eq!(
        snapshot.stories[0].check_receipts,
        [] as [crate::workflows::run::check::CheckReceipt; 0]
    );
}

#[test]
fn completed_check_cannot_reuse_an_unrelated_command_id() {
    // catches: an existing notification receipt being returned as the completed check.
    let (flow, _guard) = accepted_flow(false);
    let (execution, receipt) = completed_check(&flow);
    let unrelated = flow
        .store
        .command(
            &flow.run_id,
            "check",
            RunCommand::ReserveEffect {
                key: "notify-parent".into(),
                kind: EffectKind::Notify,
            },
        )
        .unwrap();
    assert!(
        flow.store
            .commit_completed_check(&flow.run_id, &execution, "check", flow.sequence, receipt,)
            .unwrap_err()
            .contains("different payload")
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().sequence,
        unrelated.sequence
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().stories[0].check_receipts,
        [] as [crate::workflows::run::check::CheckReceipt; 0]
    );
}

#[test]
fn completed_check_cannot_advance_a_cancelled_run() {
    // catches: a check still running at cancellation appending usable evidence afterwards.
    let (flow, _guard) = accepted_flow(false);
    let (execution, receipt) = completed_check(&flow);
    let cancelled = flow
        .store
        .command(&flow.run_id, "cancel", RunCommand::Cancel)
        .unwrap();
    assert!(
        flow.store
            .commit_completed_check(&flow.run_id, &execution, "check", flow.sequence, receipt,)
            .unwrap_err()
            .contains("terminal workflow")
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().sequence,
        cancelled.sequence
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().stories[0].check_receipts,
        [] as [crate::workflows::run::check::CheckReceipt; 0]
    );
}

fn check_story_before_merge(flow: &Flow) -> RunReceipt {
    flow.store
        .execute_check(
            &flow.run_id,
            &flow.story_id,
            "repository-integrity",
            "check",
            flow.sequence,
        )
        .unwrap()
}

#[test]
fn integration_rejects_an_evil_merge_with_an_unreviewed_extra_file() {
    // catches: valid merge parents certifying an unrelated file inserted by the integrator.
    let (flow, _guard) = accepted_flow(false);
    let checked = check_story_before_merge(&flow);
    git(&flow.repo, &["merge", "--no-ff", "--no-commit", "story"]);
    std::fs::write(flow.repo.join("unreviewed.txt"), "unreviewed\n").unwrap();
    git(&flow.repo, &["add", "unreviewed.txt"]);
    git(&flow.repo, &["commit", "-qm", "evil merge"]);
    assert!(
        flow.store
            .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", checked.sequence,)
            .unwrap_err()
            .contains("differs from the verified clean merge")
    );
    let snapshot = flow.store.snapshot(&flow.run_id).unwrap();
    assert_eq!(snapshot.sequence, checked.sequence);
    assert!(snapshot.stories[0].integration_receipt.is_none());
    assert!(!story_integrated_at_revision(&flow.story_id, flow.revision).unwrap());
}

#[test]
fn integration_rejects_an_evil_merge_that_drops_the_checked_story_change() {
    // catches: correct-parent merges that silently remove the accepted story's content.
    let (flow, _guard) = accepted_flow(false);
    let checked = check_story_before_merge(&flow);
    git(&flow.repo, &["merge", "--no-ff", "--no-commit", "story"]);
    git(&flow.repo, &["rm", "-f", "story.txt"]);
    git(&flow.repo, &["commit", "-qm", "drop reviewed content"]);
    assert!(
        flow.store
            .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", checked.sequence,)
            .unwrap_err()
            .contains("differs from the verified clean merge")
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().sequence,
        checked.sequence
    );
    assert!(!story_integrated_at_revision(&flow.story_id, flow.revision).unwrap());
}

#[test]
fn integration_rejects_manual_conflict_resolution_without_separate_review() {
    // catches: a conflict resolution receiving the source story's approval by ancestry alone.
    let (flow, _guard) = accepted_flow(false);
    let worktree = flow.store.snapshot(&flow.run_id).unwrap().stories[0]
        .worktree_path
        .clone()
        .unwrap();
    let worktree = std::path::Path::new(&worktree);
    std::fs::write(flow.repo.join("README.md"), "canonical edit\n").unwrap();
    git(&flow.repo, &["add", "README.md"]);
    git(&flow.repo, &["commit", "-qm", "canonical edit"]);
    std::fs::write(worktree.join("README.md"), "source edit\n").unwrap();
    git(worktree, &["add", "README.md"]);
    git(worktree, &["commit", "-qm", "source edit"]);
    let checked = check_story_before_merge(&flow);
    crate::git_cli::git_cmd(&flow.repo)
        .args(["merge", "--no-ff", "--no-edit", "story"])
        .run()
        .expect_err("real merge must conflict");
    std::fs::write(flow.repo.join("README.md"), "manually resolved\n").unwrap();
    git(&flow.repo, &["add", "README.md"]);
    git(&flow.repo, &["commit", "-qm", "manual resolution"]);
    let error = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", checked.sequence)
        .unwrap_err();
    assert!(
        error.contains("conflict resolution requires explicit human review"),
        "{error}"
    );
    assert_eq!(
        flow.store.snapshot(&flow.run_id).unwrap().sequence,
        checked.sequence
    );
    assert!(!story_integrated_at_revision(&flow.story_id, flow.revision).unwrap());
}
