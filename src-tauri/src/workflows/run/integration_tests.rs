//! Real-Git crash regressions: no shell/Git outputs are mocked.
use super::store::integration::{IntegrateRequest, IntegrationFault, IntegrationOutcome};
use super::*;
use std::path::{Path, PathBuf};

fn git(path: &Path, args: &[&str]) -> String {
    super::check::git_output(path, args).unwrap()
}

struct Trial {
    store: RunStore,
    request: IntegrateRequest,
    project: PathBuf,
    workspace: PathBuf,
    _config: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _guard: Box<dyn std::any::Any>,
}

fn trial() -> Trial {
    let (config, repo, plan, story, definition, guard) = super::tests::fixture();
    assert!(
        config
            .path()
            .starts_with(crate::test_support::test_temp_root())
    );
    let project = repo.path().canonicalize().unwrap();
    let seed = config.path().join("seed");
    std::fs::create_dir(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    git(&seed, &["config", "user.name", "WF07 Trial"]);
    git(&seed, &["config", "user.email", "wf07@example.invalid"]);
    git(&seed, &["commit", "--allow-empty", "-qm", "base"]);
    git(
        config.path(),
        &[
            "clone",
            "-q",
            "--no-local",
            seed.to_str().unwrap(),
            // Git cannot clone into the verbatim path returned by Windows canonicalize.
            &tuic_core::path_spelling::portable_spelling(project.to_str().unwrap()),
        ],
    );
    git(&project, &["config", "user.name", "WF07 Trial"]);
    git(&project, &["config", "user.email", "wf07@example.invalid"]);
    let base = git(&project, &["rev-parse", "HEAD"]);
    let workspace = config.path().join("source");
    git(
        &project,
        &[
            "worktree",
            "add",
            "-qb",
            "story",
            workspace.to_str().unwrap(),
        ],
    );
    std::fs::write(workspace.join("work.txt"), "accepted work\n").unwrap();
    git(&workspace, &["add", "work.txt"]);
    git(&workspace, &["commit", "-qm", "source"]);
    let source = git(&workspace, &["rev-parse", "HEAD"]);
    let definitions = crate::workflows::WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(&definition).unwrap();
    let template = draft
        .graph
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            crate::workflows::NodeKind::StoryDispatch {
                story_template_id, ..
            } => Some(story_template_id.clone()),
            _ => None,
        })
        .unwrap();
    let checks = definitions.get_draft(&template).unwrap();
    let checks = definitions
        .update_checks(
            &template,
            checks.draft_revision,
            vec![crate::workflows::CheckDefinition {
                id: "git".into(),
                argv: vec!["git".into(), "rev-parse".into(), "HEAD".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    let published = definitions
        .publish(&template, checks.draft_revision)
        .unwrap();
    let mut graph = draft.graph.clone();
    for n in &mut graph.nodes {
        if let crate::workflows::NodeKind::StoryDispatch { story_revision, .. } = &mut n.kind {
            *story_revision = published.revision;
        }
    }
    let draft = definitions
        .update_draft(&definition, draft.draft_revision, graph)
        .unwrap();
    let published = definitions
        .publish(&definition, draft.draft_revision)
        .unwrap();
    let store = RunStore::open().unwrap();
    let run = store
        .start_plan(
            project.to_str().unwrap(),
            &plan,
            &definition,
            published.revision,
            RunLimits::default(),
        )
        .unwrap();
    let started = store
        .command(
            &run.id,
            "attempt",
            RunCommand::StartAttempt {
                story_id: story.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
    let attempt = started.snapshot.attempts[0].clone();
    store
        .command(
            &run.id,
            "workspace",
            RunCommand::AssignWorktree {
                story_id: story.clone(),
                path: workspace.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
    store
        .command(
            &run.id,
            "report",
            RunCommand::ReportAttempt {
                attempt_id: attempt.id,
                generation: attempt.generation,
                outcome: AttemptOutcome::Completed,
            },
        )
        .unwrap();
    let stories = crate::stories::StoryStore::open().unwrap();
    let mut s = stories.get_story(&story).unwrap();
    for command in [
        crate::stories::StoryCommand::StartManual,
        crate::stories::StoryCommand::CheckCriterion(0),
        crate::stories::StoryCommand::SubmitReview,
        crate::stories::StoryCommand::Approve,
    ] {
        s = stories.transition(&story, s.revision, command).unwrap();
    }
    let accepted = store
        .command(
            &run.id,
            "accept",
            RunCommand::AcceptStory {
                story_id: story.clone(),
            },
        )
        .unwrap();
    let checked = store
        .execute_check(&run.id, &story, "git", "check", accepted.sequence)
        .unwrap();
    Trial {
        store,
        request: IntegrateRequest {
            run_id: run.id,
            story_id: story,
            command_id: "land".into(),
            expected_sequence: checked.sequence,
            expected_base: base,
            expected_source: source,
        },
        project,
        workspace,
        _config: config,
        _repo: repo,
        _guard: Box::new(guard),
    }
}

#[test]
fn disabled_repository_policy_refuses_native_merge() {
    // catches: default-on integration and rollback erasing existing debt.
    let t = trial();
    assert!(
        t.store
            .integrate(&t.request)
            .unwrap_err()
            .contains("native_integration_enabled")
    );
    assert_eq!(
        git(&t.project, &["rev-parse", "HEAD"]),
        t.request.expected_base
    );
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    t.store.integrate(&t.request).unwrap();
    t.store
        .set_native_integration_policy(&t.project, false)
        .unwrap();
    assert_eq!(
        t.store.integration_obligations(&t.project).unwrap().len(),
        1
    );
    assert!(t.store.integrate(&t.request).unwrap().receipt.is_some());
}

#[test]
fn crash_after_merge_never_merges_twice() {
    // catches: a lost publication receipt replays the merge.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    super::store::integration::fault(IntegrationFault::AfterMerge);
    assert!(
        t.store
            .integrate(&t.request)
            .unwrap_err()
            .contains("injected crash")
    );
    let merged = git(&t.project, &["rev-parse", "HEAD"]);
    let reopened = RunStore::open().unwrap();
    let recovered = reopened.integrate(&t.request).unwrap();
    assert_eq!(recovered.receipt.unwrap().merge_commit, merged);
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
}

#[test]
fn obligation_outbox_recovers_merge_commit_crash() {
    // catches: the ref is durable before proof debt is visible.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    super::store::integration::fault(IntegrationFault::AfterMerge);
    assert!(t.store.integrate(&t.request).is_err());
    let reopened = RunStore::open().unwrap();
    reopened.recover_integration_outbox().unwrap();
    let outbox = reopened.integration_obligations(&t.project).unwrap();
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].base_commit, t.request.expected_base);
    assert_eq!(outbox[0].source_commit, t.request.expected_source);
    assert_eq!(outbox[0].files, vec!["work.txt"]);
    reopened.integrate(&t.request).unwrap();
    assert_eq!(
        reopened.integration_obligations(&t.project).unwrap(),
        outbox
    );
}

#[test]
fn moving_base_requires_new_merge_checks() {
    // catches: checked merge publication after an external writer advances trunk.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    let path = t.project.clone();
    super::store::integration::before_publish(move || {
        git(&path, &["commit", "--allow-empty", "-qm", "outside writer"]);
    });
    assert!(t.store.integrate(&t.request).unwrap_err().contains("moved"));
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "0"
    );
    assert!(t.store.integrate(&t.request).unwrap_err().contains("moved"));
}

#[test]
fn cleanup_failure_keeps_integrated_state() {
    // catches: cleanup refusal turns a successful integration into a second merge.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    let integrated = t.store.integrate(&t.request).unwrap();
    std::fs::write(t.workspace.join("unknown.txt"), "must survive\n").unwrap();
    let pending = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(
        pending.outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    assert_eq!(pending.receipt, integrated.receipt);
    assert_eq!(
        std::fs::read_to_string(t.workspace.join("unknown.txt")).unwrap(),
        "must survive\n"
    );
    t.store.integrate(&t.request).unwrap();
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
}

#[test]
fn merge_crash_preserves_obligation_and_unattributed_work() {
    // catches: crash/outbox recovery followed by cleanup destroys unclaimed work.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    super::store::integration::fault(IntegrationFault::AfterMerge);
    assert!(t.store.integrate(&t.request).is_err());
    std::fs::write(t.workspace.join("unattributed.rs"), "valuable work\n").unwrap();
    let reopened = RunStore::open().unwrap();
    let recovered = reopened.integrate(&t.request).unwrap();
    assert!(recovered.receipt.is_some());
    let pending = reopened
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(
        pending.outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    assert!(t.workspace.join("unattributed.rs").exists());
    assert_eq!(
        reopened.integration_obligations(&t.project).unwrap().len(),
        1
    );
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
}

#[test]
fn integration_dependency_and_outbox_failures_keep_distinct_recoverable_outcomes() {
    // catches: exit-3/4 partial success being retried as a failed merge.
    for (fault, expected) in [
        (
            IntegrationFault::Outbox,
            IntegrationOutcome::IntegratedObligationPending,
        ),
        (
            IntegrationFault::DependencyRefresh,
            IntegrationOutcome::IntegratedDependencyRefreshFailed,
        ),
    ] {
        let t = trial();
        t.store
            .set_native_integration_policy(&t.project, true)
            .unwrap();
        super::store::integration::fault(fault);
        let pending = t.store.integrate(&t.request).unwrap();
        assert_eq!(pending.outcome, expected);
        assert!(pending.merged);
        t.store.integrate(&t.request).unwrap();
        assert_eq!(
            git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
            "1"
        );
        assert_eq!(
            t.store.integration_obligations(&t.project).unwrap().len(),
            1
        );
    }
}

#[test]
fn protected_and_managed_dirty_workspaces_are_preserved_and_clean_cleanup_archives_tip() {
    // catches: a lock or managed-looking dirt being treated as disposal permission.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    t.store.integrate(&t.request).unwrap();
    git(
        &t.project,
        &["worktree", "lock", t.workspace.to_str().unwrap()],
    );
    assert_eq!(
        t.store
            .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
            .unwrap()
            .outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    git(
        &t.project,
        &["worktree", "unlock", t.workspace.to_str().unwrap()],
    );
    std::fs::create_dir(t.workspace.join(".wiz")).unwrap();
    std::fs::write(
        t.workspace.join(".wiz/generated.md"),
        "managed dirt is not disposable\n",
    )
    .unwrap();
    let pending = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(
        pending.outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    assert!(t.workspace.join(".wiz/generated.md").exists());
    // The test owns this fixture and explicitly retires its managed scratch.
    std::fs::remove_file(t.workspace.join(".wiz/generated.md")).unwrap();
    std::fs::remove_dir(t.workspace.join(".wiz")).unwrap();
    let excludes = t._config.path().join("ignored-patterns");
    std::fs::write(&excludes, ".local-work\n").unwrap();
    git(
        &t.workspace,
        &["config", "core.excludesFile", excludes.to_str().unwrap()],
    );
    std::fs::write(t.workspace.join(".local-work"), "ignored valuable work\n").unwrap();
    let ignored = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(
        ignored.outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    assert_eq!(
        std::fs::read_to_string(t.workspace.join(".local-work")).unwrap(),
        "ignored valuable work\n"
    );
    std::fs::remove_file(t.workspace.join(".local-work")).unwrap();
    let complete = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(complete.outcome, IntegrationOutcome::Integrated);
    assert!(!t.workspace.exists());
    assert_eq!(
        git(&t.project, &["rev-parse", "refs/archive/story"]),
        t.request.expected_source
    );
    assert_eq!(
        git(
            &t.project,
            &["for-each-ref", "--format=%(refname)", "refs/heads/story"]
        ),
        ""
    );
    assert_eq!(
        t.store
            .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
            .unwrap(),
        complete
    );
}

#[test]
fn two_runs_serialize_one_canonical_repository_without_stale_publication() {
    // catches: two run IDs bypassing a run-local lock and both merging a stale base.
    let t = trial();
    let stories = crate::stories::StoryStore::open().unwrap();
    let plan = stories
        .create_plan(crate::stories::NewPlan {
            project: t.project.to_string_lossy().into_owned(),
            title: "Second delivery".into(),
            source: "second.md".into(),
        })
        .unwrap();
    let story = stories
        .create_story(crate::stories::NewStory {
            plan_id: plan.id.clone(),
            title: "Second work".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            story_type: None,
            origin: crate::stories::StoryOrigin::Native,
            file_scope: vec!["second.txt".into()],
        })
        .unwrap();
    let second_workspace = t._config.path().join("second");
    git(
        &t.project,
        &[
            "worktree",
            "add",
            "-qb",
            "second",
            second_workspace.to_str().unwrap(),
            &t.request.expected_base,
        ],
    );
    std::fs::write(second_workspace.join("second.txt"), "second work\n").unwrap();
    git(&second_workspace, &["add", "second.txt"]);
    git(&second_workspace, &["commit", "-qm", "second"]);
    let first = t.store.snapshot(&t.request.run_id).unwrap();
    let run = t
        .store
        .start_plan(
            t.project.to_str().unwrap(),
            &plan.id,
            &first.definition_id,
            first.definition_revision,
            RunLimits::default(),
        )
        .unwrap();
    let attempt = t
        .store
        .command(
            &run.id,
            "attempt",
            RunCommand::StartAttempt {
                story_id: story.id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap()
        .snapshot
        .attempts[0]
        .clone();
    t.store
        .command(
            &run.id,
            "workspace",
            RunCommand::AssignWorktree {
                story_id: story.id.clone(),
                path: second_workspace.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
    t.store
        .command(
            &run.id,
            "report",
            RunCommand::ReportAttempt {
                attempt_id: attempt.id,
                generation: attempt.generation,
                outcome: AttemptOutcome::Completed,
            },
        )
        .unwrap();
    let mut current = story;
    for command in [
        crate::stories::StoryCommand::StartManual,
        crate::stories::StoryCommand::CheckCriterion(0),
        crate::stories::StoryCommand::SubmitReview,
        crate::stories::StoryCommand::Approve,
    ] {
        current = stories
            .transition(&current.id, current.revision, command)
            .unwrap();
    }
    let accepted = t
        .store
        .command(
            &run.id,
            "accept",
            RunCommand::AcceptStory {
                story_id: current.id.clone(),
            },
        )
        .unwrap();
    let checked = t
        .store
        .execute_check(&run.id, &current.id, "git", "check", accepted.sequence)
        .unwrap();
    let second = IntegrateRequest {
        run_id: run.id,
        story_id: current.id,
        command_id: "land-second".into(),
        expected_sequence: checked.sequence,
        expected_base: t.request.expected_base.clone(),
        expected_source: git(&second_workspace, &["rev-parse", "HEAD"]),
    };
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| t.store.integrate(&t.request));
        let b = scope.spawn(|| t.store.integrate(&second));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_ne!(
        a.is_ok(),
        b.is_ok(),
        "one stale base must refuse: {a:?}, {b:?}"
    );
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
    assert_eq!(
        t.store.integration_obligations(&t.project).unwrap().len(),
        1
    );
    assert!(t.workspace.exists() && second_workspace.exists());
}

#[test]
fn submodule_pointer_is_attributed_and_dirty_nested_work_is_not_removed() {
    // catches: a gitlink delta disappearing from proof scope or cleanup erasing nested dirt.
    let mut t = trial();
    let dependency = t._config.path().join("dependency");
    std::fs::create_dir(&dependency).unwrap();
    git(&dependency, &["init", "-q"]);
    git(&dependency, &["config", "user.name", "WF07"]);
    git(
        &dependency,
        &["config", "user.email", "wf07@example.invalid"],
    );
    std::fs::write(dependency.join("dep.txt"), "dependency\n").unwrap();
    git(&dependency, &["add", "dep.txt"]);
    git(&dependency, &["commit", "-qm", "dependency"]);
    git(
        &t.workspace,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            dependency.to_str().unwrap(),
            "dependency",
        ],
    );
    git(&t.workspace, &["commit", "-qam", "pin dependency"]);
    t.request.expected_source = git(&t.workspace, &["rev-parse", "HEAD"]);
    let sequence = t.store.snapshot(&t.request.run_id).unwrap().sequence;
    t.request.expected_sequence = t
        .store
        .execute_check(
            &t.request.run_id,
            &t.request.story_id,
            "git",
            "check-submodule",
            sequence,
        )
        .unwrap()
        .sequence;
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    t.store.integrate(&t.request).unwrap();
    let debt = t.store.integration_obligations(&t.project).unwrap();
    assert!(debt[0].files.contains(&"dependency".into()));
    std::fs::write(
        t.workspace.join("dependency/dep.txt"),
        "unattributed nested work\n",
    )
    .unwrap();
    let pending = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(
        pending.outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    assert_eq!(
        std::fs::read_to_string(t.workspace.join("dependency/dep.txt")).unwrap(),
        "unattributed nested work\n"
    );
}

#[test]
fn only_declared_append_only_documents_union_and_malformed_or_code_conflicts_require_repair() {
    // catches: broad union silently keeping incompatible code or malformed conflict text.
    for case in ["declared", "undeclared", "malformed", "code"] {
        let mut t = trial();
        let name = if case == "code" {
            "conflict.rs"
        } else {
            "CHANGELOG.md"
        };
        std::fs::write(t.project.join(name), "canonical addition\n").unwrap();
        git(&t.project, &["add", name]);
        git(&t.project, &["commit", "-qm", "canonical addition"]);
        let source = if case == "malformed" {
            "source addition\n<<<<<<< retained malformed marker\n"
        } else {
            "source addition\n"
        };
        std::fs::write(t.workspace.join(name), source).unwrap();
        git(&t.workspace, &["add", name]);
        git(&t.workspace, &["commit", "-qm", "source addition"]);
        t.request.expected_base = git(&t.project, &["rev-parse", "HEAD"]);
        t.request.expected_source = git(&t.workspace, &["rev-parse", "HEAD"]);
        let sequence = t.store.snapshot(&t.request.run_id).unwrap().sequence;
        t.request.expected_sequence = t
            .store
            .execute_check(
                &t.request.run_id,
                &t.request.story_id,
                "git",
                "check-document",
                sequence,
            )
            .unwrap()
            .sequence;
        let documents = if case == "undeclared" {
            vec![]
        } else {
            vec!["CHANGELOG.md".into()]
        };
        t.store
            .set_integration_policy(&t.project, true, &documents)
            .unwrap();
        let outcome = t.store.integrate(&t.request).unwrap();
        if case == "declared" {
            assert!(outcome.merged);
            assert_eq!(
                std::fs::read_to_string(t.project.join(name)).unwrap(),
                "canonical addition\n\nsource addition\n"
            );
        } else {
            assert_eq!(
                outcome.outcome,
                IntegrationOutcome::RepairRequired,
                "{case}"
            );
            assert!(!outcome.merged);
            assert_eq!(
                git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
                "0"
            );
            assert!(t.workspace.exists());
        }
    }
}

#[test]
fn integration_cleanup_preserves_a_successor_workspace_even_without_a_live_pty() {
    // catches: using only PTY cwd to decide that a successor no longer owns an artifact.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    t.store.integrate(&t.request).unwrap();
    let stories = crate::stories::StoryStore::open().unwrap();
    let plan = stories
        .create_plan(crate::stories::NewPlan {
            project: t.project.to_string_lossy().into_owned(),
            title: "Successor".into(),
            source: "successor.md".into(),
        })
        .unwrap();
    let story = stories
        .create_story(crate::stories::NewStory {
            plan_id: plan.id.clone(),
            title: "Successor work".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            story_type: None,
            origin: crate::stories::StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let original = t.store.snapshot(&t.request.run_id).unwrap();
    let successor = t
        .store
        .start_plan(
            t.project.to_str().unwrap(),
            &plan.id,
            &original.definition_id,
            original.definition_revision,
            RunLimits::default(),
        )
        .unwrap();
    t.store
        .command(
            &successor.id,
            "successor-attempt",
            RunCommand::StartAttempt {
                story_id: story.id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
    t.store
        .command(
            &successor.id,
            "successor-workspace",
            RunCommand::AssignWorktree {
                story_id: story.id,
                path: t.workspace.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
    let pending = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert_eq!(
        pending.outcome,
        IntegrationOutcome::IntegratedCleanupPending
    );
    assert!(pending.reason.unwrap().contains("successor"));
    assert!(t.workspace.join("work.txt").exists());
}

fn enable(t: &Trial) {
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
}

fn save_crash_progress(t: &Trial, progress: &super::store::integration::IntegrationProgress) {
    // Model a process stopping between durable saga writes; refs are produced by real Git.
    rusqlite::Connection::open(t.store.database_path()).unwrap().execute(
        "INSERT OR REPLACE INTO workflow_integrations(run_id,story_id,repository,document) VALUES (?1,?2,?3,?4)",
        rusqlite::params![progress.request.run_id, progress.request.story_id, progress.repository, serde_json::to_string(progress).unwrap()],
    ).unwrap();
}

fn prepared_before_publication(t: &Trial) -> super::store::integration::IntegrationProgress {
    enable(t);
    let workspace = t.workspace.clone();
    super::store::integration::before_publish(move || {
        std::fs::write(workspace.join("interruption.txt"), "stop publication").unwrap();
    });
    assert!(t.store.integrate(&t.request).is_err());
    std::fs::remove_file(t.workspace.join("interruption.txt")).unwrap();
    t.store
        .integration_progress(&t.request.run_id, &t.request.story_id)
        .unwrap()
        .unwrap()
}

#[test]
fn canonical_ignored_work_is_not_overwritten_by_native_publication() {
    let mut t = trial();
    enable(&t);
    std::fs::write(t.project.join(".git/info/exclude"), "*.local\n").unwrap();
    std::fs::write(
        t.project.join("valuable.local"),
        "unattributed local work\n",
    )
    .unwrap();
    std::fs::write(
        t.workspace.join("valuable.local"),
        "incoming tracked artifact\n",
    )
    .unwrap();
    git(&t.workspace, &["add", "-f", "valuable.local"]);
    git(&t.workspace, &["commit", "-qm", "track ignored path"]);
    t.request.expected_source = git(&t.workspace, &["rev-parse", "HEAD"]);
    let checked = t
        .store
        .execute_check(
            &t.request.run_id,
            &t.request.story_id,
            "git",
            "changed-source",
            t.request.expected_sequence,
        )
        .unwrap();
    t.request.expected_sequence = checked.sequence;
    let result = t.store.integrate(&t.request);
    assert_eq!(
        std::fs::read_to_string(t.project.join("valuable.local")).unwrap(),
        "unattributed local work\n",
        "ignored local bytes must survive: {result:?}"
    );
    assert!(result.is_err());
    assert_eq!(
        git(&t.project, &["rev-parse", "HEAD"]),
        t.request.expected_base
    );
}

#[test]
fn receipt_recovery_after_saved_cursor_and_unrelated_run_event() {
    let t = trial();
    enable(&t);
    super::store::integration::fault(IntegrationFault::Outbox);
    t.store.integrate(&t.request).unwrap();
    let mut progress = t
        .store
        .integration_progress(&t.request.run_id, &t.request.story_id)
        .unwrap()
        .unwrap();
    progress.receipt_sequence = Some(t.store.snapshot(&t.request.run_id).unwrap().sequence);
    save_crash_progress(&t, &progress);
    t.store
        .command(&t.request.run_id, "operator-pause", RunCommand::Pause)
        .unwrap();
    // Pause blocks receipts by design; resume leaves only the stale-cursor hazard.
    t.store
        .command(&t.request.run_id, "operator-resume", RunCommand::Resume)
        .unwrap();
    let recovered = t.store.integrate(&t.request).unwrap();
    assert!(recovered.receipt_recorded, "{recovered:?}");
    assert!(t.store.integrate(&t.request).unwrap().receipt_recorded);
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
}

#[test]
fn cleanup_resumes_after_worktree_removal_before_branch_retirement() {
    let t = trial();
    enable(&t);
    t.store.integrate(&t.request).unwrap();
    git(
        &t.project,
        &[
            "update-ref",
            "refs/archive/story",
            &t.request.expected_source,
        ],
    );
    git(
        &t.project,
        &["worktree", "remove", t.workspace.to_str().unwrap()],
    );
    let cleaned = t
        .store
        .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
        .unwrap();
    assert!(cleaned.cleanup_complete, "{cleaned:?}");
    assert_eq!(git(&t.project, &["for-each-ref", "refs/heads/story"]), "");
    assert_eq!(
        git(&t.project, &["rev-parse", "refs/archive/story"]),
        t.request.expected_source
    );
    assert!(
        t.store
            .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
            .unwrap()
            .cleanup_complete
    );
}

#[test]
fn synthetic_validation_retries_after_checkout_removed_before_ref_deleted() {
    let t = trial();
    let progress = prepared_before_publication(&t);
    git(
        &t.project,
        &["update-ref", &progress.validation_ref, &progress.candidate],
    );
    let recovered = t.store.integrate(&t.request).unwrap();
    assert!(recovered.receipt_recorded);
    assert!(!Path::new(&progress.validation_workspace).exists());
    assert_eq!(
        git(&t.project, &["for-each-ref", &progress.validation_ref]),
        ""
    );
}

#[test]
fn repair_only_record_does_not_reserve_canonical_repository() {
    let t = trial();
    let mut progress = prepared_before_publication(&t);
    rusqlite::Connection::open(t.store.database_path())
        .unwrap()
        .execute("DELETE FROM workflow_integrations", [])
        .unwrap();
    progress.request.run_id = "repair-only-run".into();
    progress.candidate.clear();
    progress.receipt = None;
    progress.outcome = IntegrationOutcome::RepairRequired;
    save_crash_progress(&t, &progress);
    let integrated = t.store.integrate(&t.request).unwrap();
    assert!(integrated.receipt_recorded);
}

#[test]
fn unavailable_trial_repository_does_not_block_other_outbox_recovery() {
    let t = trial();
    enable(&t);
    super::store::integration::fault(IntegrationFault::AfterMerge);
    assert!(t.store.integrate(&t.request).is_err());
    let progress = t
        .store
        .integration_progress(&t.request.run_id, &t.request.story_id)
        .unwrap()
        .unwrap();
    rusqlite::Connection::open(t.store.database_path())
        .unwrap()
        .execute("DELETE FROM workflow_integrations", [])
        .unwrap();
    let mut missing = progress.clone();
    missing.request.run_id = "missing-trial".into();
    missing.project = t
        ._config
        .path()
        .join("removed-trial")
        .to_string_lossy()
        .into_owned();
    save_crash_progress(&t, &missing);
    save_crash_progress(&t, &progress);
    assert_eq!(t.store.recover_integration_outbox().unwrap(), 1);
    assert_eq!(
        t.store.integration_obligations(&t.project).unwrap().len(),
        1
    );
    assert!(
        t.store
            .integration_progress(&t.request.run_id, &t.request.story_id)
            .unwrap()
            .unwrap()
            .receipt_recorded
    );
}

#[test]
fn post_merge_check_setup_error_reports_integrated_obligation_pending() {
    let t = trial();
    enable(&t);
    super::store::integration::fault(IntegrationFault::AfterMerge);
    assert!(t.store.integrate(&t.request).is_err());
    std::fs::write(t.project.join("operator.txt"), "canonical local work\n").unwrap();
    let pending = t.store.integrate(&t.request).unwrap();
    assert_eq!(
        pending.outcome,
        IntegrationOutcome::IntegratedObligationPending
    );
    assert!(pending.merged && pending.obligation_published);
    assert!(pending.reason.is_some());
    assert_eq!(
        std::fs::read_to_string(t.project.join("operator.txt")).unwrap(),
        "canonical local work\n"
    );
    std::fs::remove_file(t.project.join("operator.txt")).unwrap();
    assert!(t.store.integrate(&t.request).unwrap().receipt_recorded);
    assert_eq!(
        git(&t.project, &["rev-list", "--count", "--merges", "HEAD"]),
        "1"
    );
}

#[test]
fn interrupted_cleanup_refuses_changed_tip_or_missing_exact_archive() {
    // catches: branch-only replay deleting work without its exact pinned archive.
    for case in ["missing-archive", "wrong-archive", "changed-tip"] {
        let t = trial();
        enable(&t);
        t.store.integrate(&t.request).unwrap();
        git(
            &t.project,
            &["worktree", "remove", t.workspace.to_str().unwrap()],
        );
        if case != "missing-archive" {
            let archived = if case == "wrong-archive" {
                &t.request.expected_base
            } else {
                &t.request.expected_source
            };
            git(&t.project, &["update-ref", "refs/archive/story", archived]);
        }
        if case == "changed-tip" {
            git(
                &t.project,
                &["update-ref", "refs/heads/story", &t.request.expected_base],
            );
        }
        let before = git(&t.project, &["rev-parse", "refs/heads/story"]);
        let pending = t
            .store
            .cleanup_integration(&t.request.run_id, &t.request.story_id, None)
            .unwrap();
        assert!(!pending.cleanup_complete, "{case}: {pending:?}");
        assert_eq!(
            git(&t.project, &["rev-parse", "refs/heads/story"]),
            before,
            "{case}"
        );
    }
}

#[test]
fn synthetic_validation_preserves_a_mismatching_surviving_ref() {
    // catches: recreating scratch by resetting an unrelated ref to the candidate.
    let t = trial();
    let progress = prepared_before_publication(&t);
    git(
        &t.project,
        &[
            "update-ref",
            &progress.validation_ref,
            &t.request.expected_base,
        ],
    );
    assert!(t.store.integrate(&t.request).is_err());
    assert_eq!(
        git(&t.project, &["rev-parse", &progress.validation_ref]),
        t.request.expected_base
    );
    assert_eq!(
        git(&t.project, &["rev-parse", "HEAD"]),
        t.request.expected_base
    );
}

#[test]
fn parent_cancel_does_not_erase_shared_proof_debt() {
    // catches: cancelling a delivery run deletes independently owned post-merge obligations.
    let t = trial();
    t.store
        .set_native_integration_policy(&t.project, true)
        .unwrap();
    t.store.integrate(&t.request).unwrap();
    let debt = t.store.integration_obligations(&t.project).unwrap();
    assert_eq!(debt.len(), 1);
    t.store
        .command(&t.request.run_id, "cancel-parent", RunCommand::Cancel)
        .unwrap();
    let reopened = RunStore::open().unwrap();
    assert_eq!(reopened.integration_obligations(&t.project).unwrap(), debt);
    assert_eq!(
        reopened.replay(&t.request.run_id).unwrap().status,
        RunStatus::Cancelled
    );
}
