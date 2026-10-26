mod api;
pub(super) mod check;
pub mod graph;
mod incidents;
mod model;
pub use incidents::RunIncident;
mod reducer;
mod runtime;
mod store;
pub(crate) use runtime::WorkflowRuntime;
#[cfg(test)]
mod runtime_tests;

pub use api::*;
pub(crate) use check::shutdown_checks;
#[cfg(test)]
pub use check::*;

pub use model::*;
pub use store::*;

#[cfg(test)]
mod critic_tests;
#[cfg(test)]
mod graph_critic_1446a_tests;
#[cfg(test)]
mod graph_tests;

#[cfg(test)]
mod critic_957_tests;

#[cfg(test)]
mod critic_960_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStore};
    use crate::workflows::{WorkflowKind, WorkflowStore};

    #[test]
    fn tuic_check_receipt_binds_a_clean_commit_and_reports_failures() {
        use std::process::Command;
        let repo = tempfile::tempdir().expect("repo");
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Workflow Test"],
            vec!["config", "user.email", "workflow@example.test"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(repo.path())
                    .status()
                    .expect("git setup")
                    .success()
            );
        }
        std::fs::write(repo.path().join("file.txt"), "first\n").expect("file");
        assert!(
            Command::new("git")
                .args(["add", "file.txt"])
                .current_dir(repo.path())
                .status()
                .expect("add")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-qm", "initial"])
                .current_dir(repo.path())
                .status()
                .expect("commit")
                .success()
        );
        let check = crate::workflows::CheckDefinition {
            id: "head".into(),
            argv: vec!["git".into(), "rev-parse".into(), "HEAD".into()],
            timeout_secs: 10,
        };
        let receipt = super::check::execute_run_check(
            &check,
            repo.path(),
            repo.path(),
            "receipt-test",
            false,
        )
        .expect("check");
        assert_eq!(receipt.exit_code, 0);
        assert_eq!(receipt.argv, check.argv);
        assert_eq!(receipt.commit.len(), 40);
        assert_eq!(receipt.tree.len(), 40);
        let failed = crate::workflows::CheckDefinition {
            id: "bad".into(),
            argv: vec!["git".into(), "rev-parse".into(), "missing-ref".into()],
            timeout_secs: 10,
        };
        assert_ne!(
            super::check::execute_run_check(
                &failed,
                repo.path(),
                repo.path(),
                "receipt-test",
                false
            )
            .expect("failed check receipt")
            .exit_code,
            0
        );
        std::fs::write(repo.path().join("file.txt"), "changed\n").expect("edit");
        assert!(
            super::check::execute_run_check(
                &check,
                repo.path(),
                repo.path(),
                "receipt-test",
                false
            )
            .expect_err("dirty worktree")
            .contains("clean")
        );
    }

    #[cfg(unix)]
    #[test]
    fn timed_out_pinned_check_stops_its_descendants() {
        assert_check_tree_stops(false);
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_pinned_check_stops_descendants_and_fences_late_workers() {
        // catches: cancellation stops the run projection but leaves checks alive,
        // or a worker finishes its Git setup and spawns after cancellation.
        assert_check_tree_stops(true);
    }

    #[cfg(unix)]
    fn assert_check_tree_stops(cancel: bool) {
        use std::process::Command;
        let repo = tempfile::tempdir().expect("repo");
        let pid_dir = tempfile::tempdir().expect("pid dir");
        let pid_file = pid_dir.path().join("worker.pid");
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Workflow Test"],
            vec!["config", "user.email", "workflow@example.test"],
            vec!["commit", "-q", "--allow-empty", "-m", "initial"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(repo.path())
                    .status()
                    .expect("git setup")
                    .success()
            );
        }
        // A runner that forks a long-lived worker and waits for it, like a
        // test runner with a child pool; killing only the shell leaves the worker.
        let check = crate::workflows::CheckDefinition {
            id: "pool".into(),
            argv: vec![
                "sh".into(),
                "-c".into(),
                format!("sleep 60 & echo $! > {}; wait", pid_file.display()),
            ],
            timeout_secs: if cancel { 60 } else { 1 },
        };
        if cancel {
            let repo_path = repo.path().to_owned();
            let db_path = pid_dir.path().join("runs.sqlite3");
            let worker_check = check.clone();
            let worker_db = db_path.clone();
            let worker = std::thread::spawn(move || {
                super::check::execute_run_check(
                    &worker_check,
                    &repo_path,
                    &worker_db,
                    "cancelled-run",
                    false,
                )
            });
            // Setup is not the behavior deadline; nextest bounds a stuck setup.
            while !pid_file.exists() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            super::check::cancel_checks(&db_path, "cancelled-run");
            assert!(
                worker
                    .join()
                    .expect("check worker")
                    .unwrap_err()
                    .contains("cancelled")
            );
            assert!(
                super::check::execute_run_check(
                    &check,
                    repo.path(),
                    &db_path,
                    "cancelled-run",
                    false
                )
                .unwrap_err()
                .contains("cancelled")
            );
        } else {
            let receipt = super::check::execute_run_check(
                &check,
                repo.path(),
                repo.path(),
                "receipt-test",
                false,
            )
            .expect("timed out receipt");
            assert_eq!(receipt.exit_code, -1);
        }
        let worker: libc::pid_t = std::fs::read_to_string(&pid_file)
            .expect("worker pid")
            .trim()
            .parse()
            .expect("pid");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // SAFETY: signal 0 only probes for existence.
        while unsafe { libc::kill(worker, 0) } == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "descendant {worker} outlived the timed-out check"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn failed_or_stale_pinned_check_cannot_authorize_integration() {
        let check = crate::workflows::CheckDefinition {
            id: "unit".into(),
            argv: vec!["cargo".into(), "test".into()],
            timeout_secs: 30,
        };
        let mut receipt = CheckReceipt {
            check_id: "unit".into(),
            argv: check.argv.clone(),
            exit_code: 1,
            ref_name: "refs/heads/story".into(),
            commit: "a".repeat(40),
            tree: "b".repeat(40),
            duration_ms: 25,
        };
        assert!(
            super::store::require_current_checks(
                std::slice::from_ref(&check),
                &[receipt.clone()],
                &receipt.ref_name,
                &receipt.commit,
                &receipt.tree
            )
            .is_err()
        );
        receipt.exit_code = 0;
        assert!(
            super::store::require_current_checks(
                std::slice::from_ref(&check),
                &[receipt.clone()],
                &receipt.ref_name,
                &"c".repeat(40),
                &receipt.tree
            )
            .is_err()
        );
        receipt.argv.push("--ignored".into());
        assert!(
            super::store::require_current_checks(
                std::slice::from_ref(&check),
                &[receipt.clone()],
                &receipt.ref_name,
                &receipt.commit,
                &receipt.tree
            )
            .is_err()
        );
        receipt.argv = check.argv.clone();
        assert!(
            super::store::require_current_checks(
                std::slice::from_ref(&check),
                &[receipt.clone()],
                "refs/heads/moved",
                &receipt.commit,
                &receipt.tree
            )
            .is_err()
        );
        assert!(
            super::store::require_current_checks(
                &[check],
                &[receipt.clone()],
                &receipt.ref_name,
                &receipt.commit,
                &receipt.tree
            )
            .is_ok()
        );
    }

    #[test]
    fn check_receipt_is_durable_and_replayed_after_restart() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let db = config.path().join("runs.sqlite3");
        let store = RunStore::open_at(&db).expect("run store");
        let run = store
            .start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("run");
        let stories = StoryStore::open().expect("stories");
        let story = stories
            .transition(&story_id, 1, StoryCommand::StartManual)
            .expect("start");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::CheckCriterion(0))
            .expect("criterion");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::SubmitReview)
            .expect("review");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::Approve)
            .expect("approve");
        let accepted = store
            .command(
                &run.id,
                "accept",
                RunCommand::AcceptStory {
                    story_id: story_id.clone(),
                },
            )
            .expect("accept");
        let receipt = CheckReceipt {
            check_id: "unit".into(),
            argv: vec!["git".into(), "status".into()],
            exit_code: 0,
            ref_name: "refs/heads/story".into(),
            commit: "a".repeat(40),
            tree: "b".repeat(40),
            duration_ms: 1,
        };
        assert_eq!(
            accepted.snapshot.stories[0].accepted_revision,
            Some(story.revision)
        );
        store
            .record_check_receipt(
                &run.id,
                &story_id,
                "check-1",
                accepted.sequence,
                receipt.clone(),
            )
            .expect("record");
        let reopened = RunStore::open_at(&db).expect("reopen");
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().stories[0].check_receipts,
            vec![receipt]
        );
        assert!(reopened.replay(&run.id).is_ok());
    }

    #[test]
    fn integration_receipt_is_durable_and_binds_the_accepted_revision() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let run = store
            .start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("run");
        let stories = StoryStore::open().expect("stories");
        let story = stories
            .transition(&story_id, 1, StoryCommand::StartManual)
            .unwrap();
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::CheckCriterion(0))
            .unwrap();
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::SubmitReview)
            .unwrap();
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::Approve)
            .unwrap();
        let accepted = store
            .command(
                &run.id,
                "accept",
                RunCommand::AcceptStory {
                    story_id: story_id.clone(),
                },
            )
            .unwrap();
        let receipt = IntegrationReceipt {
            story_revision: story.revision,
            canonical_ref: "refs/heads/main".into(),
            base_commit: "a".repeat(40),
            source_commit: "b".repeat(40),
            source_tree: "c".repeat(40),
            merge_commit: "d".repeat(40),
            merge_tree: "e".repeat(40),
            post_checks: vec![],
        };
        let stale = IntegrationReceipt {
            story_revision: story.revision - 1,
            ..receipt.clone()
        };
        assert!(
            store
                .record_integration_receipt(&run.id, &story_id, "stale", accepted.sequence, stale)
                .is_err()
        );
        store
            .record_integration_receipt(
                &run.id,
                &story_id,
                "integrate",
                accepted.sequence,
                receipt.clone(),
            )
            .expect("integrate");
        let reopened = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        assert_eq!(
            reopened
                .record_integration_receipt(
                    &run.id,
                    &story_id,
                    "integrate",
                    accepted.sequence,
                    receipt.clone()
                )
                .unwrap()
                .sequence,
            accepted.sequence + 1
        );
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().stories[0].integration_receipt,
            Some(receipt)
        );
        assert_eq!(
            reopened.replay(&run.id).unwrap(),
            reopened.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn recorded_merge_releases_dependents_and_ref_movement_invalidates_receipts() {
        assert_recorded_merge_receipts(true, false, false);
    }

    #[test]
    fn workflow_plan_done_requires_receipts_and_reopens_until_recertified() {
        // catches: approved stories make a workflow plan Done before integration,
        // or Done survives canonical ref movement which invalidates run completion.
        assert_recorded_merge_receipts(false, false, false);
    }

    #[test]
    fn preflight_git_does_not_hold_writers_and_post_probe_ref_movement_is_rejected_on_read() {
        assert_recorded_merge_receipts(false, true, false);
    }

    /// Catches: archive metadata invalidating accepted work and reopening a completed plan.
    #[test]
    fn archive_round_trip_preserves_workflow_integration_evidence() {
        assert_recorded_merge_receipts(false, false, true);
    }

    fn assert_recorded_merge_receipts(
        with_dependent: bool,
        preflight_races: bool,
        archive_round_trip: bool,
    ) {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let definitions = WorkflowStore::open().unwrap();
        let plan_draft = definitions.get_draft(&definition_id).unwrap();
        let story_template_id = plan_draft
            .graph
            .nodes
            .iter()
            .find_map(|node| {
                if let crate::workflows::NodeKind::StoryDispatch {
                    story_template_id, ..
                } = &node.kind
                {
                    Some(story_template_id.clone())
                } else {
                    None
                }
            })
            .unwrap();
        let story_draft = definitions.get_draft(&story_template_id).unwrap();
        let check = crate::workflows::CheckDefinition {
            id: "policy".into(),
            argv: vec![
                "git".into(),
                "config".into(),
                "--get".into(),
                "workflow.testpass".into(),
            ],
            timeout_secs: 10,
        };
        let story_draft = definitions
            .update_checks(&story_template_id, story_draft.draft_revision, vec![check])
            .unwrap();
        let story_published = definitions
            .publish(&story_template_id, story_draft.draft_revision)
            .unwrap();
        let mut graph = plan_draft.graph.clone();
        for node in &mut graph.nodes {
            if let crate::workflows::NodeKind::StoryDispatch { story_revision, .. } = &mut node.kind
            {
                *story_revision = story_published.revision;
            }
        }
        let plan_draft = definitions
            .update_draft(&definition_id, plan_draft.draft_revision, graph)
            .unwrap();
        let plan_published = definitions
            .publish(&definition_id, plan_draft.draft_revision)
            .unwrap();
        let repo = project.path();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Workflow Test"],
            vec!["config", "user.email", "workflow@example.invalid"],
            vec!["config", "workflow.testpass", "true"],
        ] {
            crate::git_cli::git_cmd(repo).args(args).run().unwrap();
        }
        std::fs::write(repo.join("README.md"), "base\n").unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["add", "README.md"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["commit", "-qm", "base"])
            .run()
            .unwrap();
        let worktree = config.path().join("story-worktree");
        crate::git_cli::git_cmd(repo)
            .args([
                "worktree",
                "add",
                "-q",
                "-b",
                "story",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        std::fs::write(worktree.join("story.txt"), "accepted\n").unwrap();
        crate::git_cli::git_cmd(&worktree)
            .args(["add", "story.txt"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&worktree)
            .args(["commit", "-qm", "story"])
            .run()
            .unwrap();

        let stories = StoryStore::open().unwrap();
        let dependent = if with_dependent {
            let dependent = stories
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan_id.clone(),
                    title: "Dependent".into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec!["dependent.txt".into()],
                })
                .unwrap();
            stories
                .add_dependency(&dependent.id, &story_id, dependent.revision)
                .unwrap();
            Some(dependent)
        } else {
            None
        };
        let assert_plan_state = |expected| {
            if !with_dependent {
                assert_eq!(stories.plan_state(&plan_id).unwrap(), expected);
                assert_eq!(stories.plan_view(&plan_id).unwrap().state, expected);
            }
        };
        let store = RunStore::open().unwrap();
        let run = store
            .start_plan(
                repo.to_str().unwrap(),
                &plan_id,
                &definition_id,
                plan_published.revision,
                RunLimits::default(),
            )
            .unwrap();
        let started = store
            .command(
                &run.id,
                "attempt",
                RunCommand::StartAttempt {
                    story_id: story_id.clone(),
                    node_id: "implement".into(),
                },
            )
            .unwrap();
        let attempt = &started.snapshot.attempts[0];
        store
            .command(
                &run.id,
                "worktree",
                RunCommand::AssignWorktree {
                    story_id: story_id.clone(),
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
        let initial_story = stories.get_story(&story_id).unwrap();
        let claimed = stories
            .claim(&story_id, "implementer", initial_story.revision)
            .unwrap();
        let checked = stories
            .transition_for_actor(
                &story_id,
                claimed.revision,
                StoryCommand::CheckCriterion(0),
                Some("implementer"),
            )
            .unwrap();
        let reviewed = stories
            .transition_for_actor(
                &story_id,
                checked.revision,
                StoryCommand::SubmitReview,
                Some("implementer"),
            )
            .unwrap();
        let story = stories
            .transition_for_actor(
                &story_id,
                reviewed.revision,
                StoryCommand::Approve,
                Some("reviewer"),
            )
            .unwrap();
        assert_eq!(
            stories
                .transition_history(&story_id)
                .unwrap()
                .last()
                .unwrap()
                .actor,
            crate::stories::StoryTransitionActor::ManagedSession {
                session_id: "reviewer".into(),
            }
        );
        let accepted = store
            .command(
                &run.id,
                "accept",
                RunCommand::AcceptStory {
                    story_id: story_id.clone(),
                },
            )
            .unwrap();
        if let Some(dependent) = &dependent {
            assert_eq!(
                stories.get_story(&dependent.id).unwrap().status,
                crate::stories::StoryStatus::Backlog
            );
        }
        assert_plan_state(crate::stories::PlanState::Active);
        let checked = store
            .execute_check(&run.id, &story_id, "policy", "check", accepted.sequence)
            .unwrap();
        assert!(matches!(
            checked.event.kind,
            RunEventKind::CheckRecorded { .. }
        ));
        assert!(
            store
                .record_integrated_story(&run.id, &story_id, "before-merge", checked.sequence)
                .is_err()
        );
        crate::git_cli::git_cmd(repo)
            .args(["merge", "--no-ff", "--no-edit", "story"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&worktree)
            .args(["switch", "-qc", "moved-source-ref"])
            .run()
            .unwrap();
        assert!(
            store
                .record_integrated_story(&run.id, &story_id, "wrong-source-ref", checked.sequence)
                .is_err()
        );
        crate::git_cli::git_cmd(&worktree)
            .args(["switch", "-q", "story"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["config", "--unset", "workflow.testpass"])
            .run()
            .unwrap();
        assert_eq!(
            store
                .record_integrated_story(&run.id, &story_id, "failed-post-check", checked.sequence)
                .unwrap_err(),
            "post-integration check policy failed"
        );
        if let Some(dependent) = &dependent {
            assert_eq!(
                stories.get_story(&dependent.id).unwrap().status,
                crate::stories::StoryStatus::Backlog
            );
        }
        crate::git_cli::git_cmd(repo)
            .args(["config", "workflow.testpass", "true"])
            .run()
            .unwrap();
        if preflight_races {
            let canonical = repo.to_path_buf();
            super::store::BEFORE_COMMIT.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || {
                    crate::git_cli::git_cmd(&canonical)
                        .args(["switch", "-qc", "probe-commit-race"])
                        .run()
                        .unwrap();
                }))
            });
        }
        let integrated = store
            .record_integrated_story(&run.id, &story_id, "integrate", checked.sequence)
            .unwrap();
        assert!(matches!(
            integrated.event.kind,
            RunEventKind::StoryIntegrated { .. }
        ));
        if preflight_races {
            assert!(!story_integrated_at_revision(&story_id, story.revision).unwrap());
            crate::git_cli::git_cmd(repo)
                .args(["switch", "-q", "main"])
                .run()
                .unwrap();
            let writer = store.clone();
            let run_id = run.id.clone();
            let project_path = run.project.clone();
            super::check::BEFORE_GIT.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || {
                    // catches: receipt Git probes holding story/run writer locks and timing out unrelated writes.
                    StoryStore::open()
                        .unwrap()
                        .create_plan(NewPlan {
                            project: project_path,
                            title: "Unrelated writer".into(),
                            source: "unrelated.md".into(),
                        })
                        .unwrap();
                    writer
                        .command(&run_id, "writer-during-git", RunCommand::Pause)
                        .unwrap();
                }))
            });
            let error = stories
                .reconcile_integrated_dependencies(&plan_id)
                .unwrap_err();
            assert!(error.contains("revisions changed"), "{error}");
            stories.reconcile_integrated_dependencies(&plan_id).unwrap();
        }
        assert_plan_state(crate::stories::PlanState::Done);
        let receipt = integrated
            .snapshot
            .stories
            .iter()
            .find(|item| item.story_id == story_id)
            .unwrap()
            .integration_receipt
            .as_ref()
            .unwrap();
        assert_eq!(receipt.post_checks.len(), 1);
        assert_eq!(receipt.post_checks[0].commit, receipt.merge_commit);
        assert_eq!(
            store
                .record_integrated_story(&run.id, &story_id, "integrate", checked.sequence)
                .unwrap()
                .sequence,
            integrated.sequence
        );
        assert!(story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Done);
        if archive_round_trip {
            store
                .command(&run.id, "release-archive-owner", RunCommand::Cancel)
                .unwrap();
            let actor = crate::stories::StoryTransitionActor::Human;
            let mut current = stories.get_story(&story_id).unwrap();
            for _ in 0..2 {
                let before = current.revision;
                current = stories
                    .set_story_archived(&story_id, before, true, actor.clone())
                    .unwrap();
                current = stories
                    .set_story_archived(&story_id, current.revision, false, actor.clone())
                    .unwrap();
                assert_eq!(current.revision, before + 2);
                assert!(
                    story_integrated_at_revision(&story_id, current.revision).unwrap(),
                    "archive metadata must preserve integration evidence"
                );
                assert_plan_state(crate::stories::PlanState::Done);
            }
            let dependent = stories
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan_id.clone(),
                    title: "After restore".into(),
                    criteria: vec!["Done".into()],
                    priority: 2,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                })
                .unwrap();
            let dependent = stories
                .add_dependency(&dependent.id, &story_id, dependent.revision)
                .unwrap();
            assert_eq!(dependent.status, crate::stories::StoryStatus::Ready);
            return;
        }
        // catches: a receipt with an empty post_checks list passing `all()` vacuously and releasing dependents.
        let mut vacuous = integrated.snapshot.clone();
        vacuous
            .stories
            .iter_mut()
            .find(|item| item.story_id == story_id)
            .unwrap()
            .integration_receipt
            .as_mut()
            .unwrap()
            .post_checks
            .clear();
        assert!(
            super::store::receipt_current(&integrated.snapshot, &story_id, story.revision).unwrap()
        );
        assert!(!super::store::receipt_current(&vacuous, &story_id, story.revision).unwrap());

        let mut verification_snapshot = integrated.snapshot.clone();
        verification_snapshot.planning_fingerprint = Some("closed".into());
        assert!(
            super::store::ready_to_verify(&verification_snapshot, std::slice::from_ref(&story))
                .is_ok()
        );
        if let Some(dependent) = &dependent {
            assert_eq!(
                stories.get_story(&dependent.id).unwrap().status,
                crate::stories::StoryStatus::Ready
            );
        }
        drop(store);
        let store = RunStore::open().unwrap();
        assert_eq!(store.reconcile_active_after_restart().unwrap(), 1);
        assert_eq!(store.snapshot(&run.id).unwrap().status, RunStatus::Paused);
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
        assert_eq!(
            store
                .record_integrated_story(&run.id, &story_id, "integrate", checked.sequence)
                .unwrap()
                .sequence,
            integrated.sequence
        );
        crate::git_cli::git_cmd(repo)
            .args(["switch", "-qc", "moved-ref"])
            .run()
            .unwrap();
        assert!(!story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Active);
        assert!(
            super::store::ready_to_verify(&verification_snapshot, std::slice::from_ref(&story))
                .is_err()
        );
        store.reconcile_active_after_restart().unwrap();
        if let Some(dependent) = &dependent {
            assert_eq!(
                stories.get_story(&dependent.id).unwrap().status,
                crate::stories::StoryStatus::Backlog
            );
        }
        crate::git_cli::git_cmd(repo)
            .args(["switch", "-q", "main"])
            .run()
            .unwrap();
        assert!(story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Done);
        store.reconcile_active_after_restart().unwrap();
        if let Some(dependent) = &dependent {
            assert_eq!(
                stories.get_story(&dependent.id).unwrap().status,
                crate::stories::StoryStatus::Ready
            );
        }
        std::fs::write(repo.join("later.txt"), "later\n").unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["add", "later.txt"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["commit", "-qm", "later"])
            .run()
            .unwrap();
        assert!(!story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Active);
        if let Some(dependent) = &dependent {
            let current = stories.get_story(&dependent.id).unwrap();
            assert!(
                stories
                    .transition(&dependent.id, current.revision, StoryCommand::StartManual)
                    .is_err()
            );
        }
        crate::git_cli::git_cmd(repo)
            .args(["config", "--unset", "workflow.testpass"])
            .run()
            .unwrap();
        assert_eq!(
            store
                .recertify_canonical(
                    &run.id,
                    "failed-recertification",
                    store.snapshot(&run.id).unwrap().sequence
                )
                .unwrap_err(),
            "post-integration check policy failed"
        );
        crate::git_cli::git_cmd(repo)
            .args(["config", "workflow.testpass", "true"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["switch", "-q", "moved-ref"])
            .run()
            .unwrap();
        assert!(
            store
                .recertify_canonical(
                    &run.id,
                    "wrong-recertification-ref",
                    store.snapshot(&run.id).unwrap().sequence
                )
                .is_err()
        );
        crate::git_cli::git_cmd(repo)
            .args(["switch", "-q", "main"])
            .run()
            .unwrap();
        std::fs::write(repo.join("dirty.txt"), "untracked\n").unwrap();
        assert!(
            store
                .recertify_canonical(
                    &run.id,
                    "dirty-recertification",
                    store.snapshot(&run.id).unwrap().sequence
                )
                .is_err()
        );
        std::fs::remove_file(repo.join("dirty.txt")).unwrap();
        let recertified = store
            .recertify_canonical(
                &run.id,
                "recertify-later-head",
                store.snapshot(&run.id).unwrap().sequence,
            )
            .expect("recheck the new canonical tip");
        assert!(matches!(
            recertified.event.kind,
            RunEventKind::CanonicalRecertified { .. }
        ));
        assert_eq!(
            store
                .recertify_canonical(&run.id, "recertify-later-head", recertified.sequence - 1)
                .unwrap()
                .sequence,
            recertified.sequence
        );
        assert!(story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Done);
        let mut recertified_snapshot = store.snapshot(&run.id).unwrap();
        recertified_snapshot.planning_fingerprint = Some("closed".into());
        assert!(
            super::store::ready_to_verify(&recertified_snapshot, std::slice::from_ref(&story))
                .is_ok()
        );
        if let Some(dependent) = &dependent {
            assert_eq!(
                stories.get_story(&dependent.id).unwrap().status,
                crate::stories::StoryStatus::Ready
            );
        }
        drop(store);
        let store = RunStore::open().unwrap();
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
        assert!(story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Done);
        let conflict_worktree = config.path().join("conflict-worktree");
        crate::git_cli::git_cmd(repo)
            .args([
                "worktree",
                "add",
                "-q",
                "-b",
                "conflict",
                conflict_worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        std::fs::write(conflict_worktree.join("README.md"), "branch\n").unwrap();
        crate::git_cli::git_cmd(&conflict_worktree)
            .args(["add", "README.md"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&conflict_worktree)
            .args(["commit", "-qm", "branch edit"])
            .run()
            .unwrap();
        std::fs::write(repo.join("README.md"), "canonical\n").unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["add", "README.md"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["commit", "-qm", "canonical edit"])
            .run()
            .unwrap();
        assert!(!story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Active);
        assert!(
            crate::git_cli::git_cmd(repo)
                .args(["merge", "--no-ff", "--no-edit", "conflict"])
                .run()
                .is_err()
        );
        assert!(
            store
                .record_integrated_story(
                    &run.id,
                    &story_id,
                    "conflicted",
                    store.snapshot(&run.id).unwrap().sequence
                )
                .is_err()
        );
        crate::git_cli::git_cmd(repo)
            .args(["merge", "--abort"])
            .run()
            .unwrap();
        assert!(!story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Active);
        let cancelled = store
            .command(&run.id, "cancel-after-integration", RunCommand::Cancel)
            .unwrap();
        assert_eq!(cancelled.snapshot.status, RunStatus::Cancelled);
        store
            .recertify_canonical(&run.id, "recertify-cancelled-run", cancelled.sequence)
            .expect("a terminal run can renew its integration evidence");
        assert!(story_integrated_at_revision(&story_id, story.revision).unwrap());
        assert_plan_state(crate::stories::PlanState::Done);
    }

    pub(super) fn fixture() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        String,
        String,
        String,
        impl Drop,
    ) {
        let config = tempfile::tempdir().expect("config");
        let project = tempfile::tempdir().expect("project");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project_path = project
            .path()
            .canonicalize()
            .expect("canonical project")
            .to_string_lossy()
            .to_string();
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
        let plan_template = definitions
            .seed_templates(&project_path)
            .expect("templates")
            .into_iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .expect("plan template");
        (config, project, plan.id, story.id, plan_template.id, _guard)
    }

    #[test]
    fn bound_agent_report_is_typed_owned_idempotent_and_replayable() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        let started = store
            .command(
                &run.id,
                "attempt",
                RunCommand::StartAttempt {
                    story_id: story_id.clone(),
                    node_id: "implement".into(),
                },
            )
            .expect("attempt");
        let attempt = &started.snapshot.attempts[0];
        let reserved = store
            .command(
                &run.id,
                "reserve",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("reserve spawn");
        let binding = AgentBinding {
            session_id: "managed-session".into(),
            task_id: Some("task-1".into()),
            effect_id: reserved.snapshot.effects[0].id.clone(),
            prompt_contract_version: 1,
            prompt_sha256: "a".repeat(64),
            audit_preview: "redacted prompt".into(),
        };
        let bound = store
            .bind_agent(&run.id, &attempt.id, binding.clone())
            .expect("bind");
        assert_eq!(bound.snapshot.effects[0].state, EffectState::Succeeded);
        assert_eq!(bound.snapshot.attempts[0].agent.as_ref(), Some(&binding));
        assert_eq!(
            store
                .bind_agent(&run.id, &attempt.id, binding)
                .unwrap()
                .sequence,
            bound.sequence
        );
        assert!(
            store
                .command(
                    &run.id,
                    "untyped",
                    RunCommand::ReportAttempt {
                        attempt_id: attempt.id.clone(),
                        generation: attempt.generation,
                        outcome: AttemptOutcome::Completed,
                    }
                )
                .is_err()
        );

        let story = StoryStore::open().unwrap().get_story(&story_id).unwrap();
        let report = AttemptReport {
            contract_version: 1,
            run_id: run.id.clone(),
            story_id,
            story_revision: story.revision,
            attempt_id: attempt.id.clone(),
            generation: attempt.generation,
            outcome: AttemptOutcome::Completed,
            summary: "Implemented and checked".into(),
            criterion_results: vec![CriterionResult {
                index: 0,
                satisfied: true,
                evidence: "focused test passed".into(),
            }],
            evidence: vec!["test receipt 123".into()],
            input_request: None,
            review: None,
        };
        assert!(
            store
                .report_bound_agent(report.clone(), "other-session")
                .is_err()
        );
        let accepted = store
            .report_bound_agent(report.clone(), "managed-session")
            .expect("report");
        assert_eq!(accepted.snapshot.attempts[0].report.as_ref(), Some(&report));
        assert_eq!(
            store
                .report_bound_agent(report, "managed-session")
                .unwrap()
                .sequence,
            accepted.sequence
        );
        assert_eq!(
            store.interrupt_agent_session("managed-session").unwrap(),
            [] as [crate::workflows::run::model::RunSnapshot; 0]
        );

        let next = store
            .command(
                &run.id,
                "attempt-after-report",
                RunCommand::StartAttempt {
                    story_id: story.id.clone(),
                    node_id: "implement".into(),
                },
            )
            .expect("next attempt");
        let next_attempt = next.snapshot.attempts.last().unwrap();
        let reserved = store
            .command(
                &run.id,
                "reserve-after-report",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", next_attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("reserve next spawn");
        store
            .bind_agent(
                &run.id,
                &next_attempt.id,
                AgentBinding {
                    session_id: "managed-exit".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "b".repeat(64),
                    audit_preview: "redacted prompt".into(),
                },
            )
            .expect("bind next");
        let interrupted = store
            .interrupt_agent_session("managed-exit")
            .expect("observe exit");
        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0].status, RunStatus::Paused);
        assert_eq!(
            interrupted[0].attempts.last().unwrap().state,
            AttemptState::Interrupted
        );
        assert!(interrupted[0].attempts.last().unwrap().report.is_none());
        assert_eq!(
            store.interrupt_agent_session("managed-exit").unwrap(),
            [] as [crate::workflows::run::model::RunSnapshot; 0]
        );
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn bound_input_request_pauses_until_a_durable_answer_and_resume() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let started = store
            .command(
                &run.id,
                "input-attempt",
                RunCommand::StartAttempt {
                    story_id: story_id.clone(),
                    node_id: "implement".into(),
                },
            )
            .unwrap();
        let attempt = started.snapshot.attempts.last().unwrap();
        let reserved = store
            .command(
                &run.id,
                "input-spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        store
            .bind_agent(
                &run.id,
                &attempt.id,
                AgentBinding {
                    session_id: "input-worker".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "a".repeat(64),
                    audit_preview: "request input".into(),
                },
            )
            .unwrap();
        let story = StoryStore::open().unwrap().get_story(&story_id).unwrap();
        let report: AttemptReport = serde_json::from_value(serde_json::json!({
            "contractVersion": 1, "runId": run.id, "storyId": story_id,
            "storyRevision": story.revision, "attemptId": attempt.id,
            "generation": attempt.generation, "outcome": "needs_input",
            "summary": "Need product decision", "criterionResults": [], "evidence": [],
            "inputRequest": {"question": "Use A or B?", "options": ["A", "B"]}
        }))
        .unwrap();
        let receipt = store.report_bound_agent(report, "input-worker").unwrap();
        assert_eq!(receipt.snapshot.status, RunStatus::Paused);
        assert_eq!(receipt.snapshot.attempts[0].input_answer, None);
        assert_eq!(
            receipt.snapshot.attempts[0]
                .report
                .as_ref()
                .unwrap()
                .input_request
                .as_ref()
                .unwrap()
                .question,
            "Use A or B?"
        );
        assert!(
            store
                .command(&run.id, "early-resume", RunCommand::Resume)
                .is_err()
        );
        let answered = store
            .command(
                &run.id,
                "human-answer",
                RunCommand::AnswerInput {
                    attempt_id: attempt.id.clone(),
                    answer: "A".into(),
                },
            )
            .unwrap();
        assert_eq!(answered.snapshot.status, RunStatus::Paused);
        assert_eq!(
            answered.snapshot.attempts[0].input_answer.as_deref(),
            Some("A")
        );
        assert_eq!(
            store
                .command(
                    &run.id,
                    "human-answer",
                    RunCommand::AnswerInput {
                        attempt_id: attempt.id.clone(),
                        answer: "A".into(),
                    }
                )
                .unwrap()
                .sequence,
            answered.sequence
        );
        assert!(
            store
                .command(
                    &run.id,
                    "different-answer",
                    RunCommand::AnswerInput {
                        attempt_id: attempt.id.clone(),
                        answer: "B".into(),
                    }
                )
                .is_err()
        );
        let resumed = store
            .command(&run.id, "human-resume", RunCommand::Resume)
            .unwrap();
        assert_eq!(resumed.snapshot.status, RunStatus::Running);
        assert_eq!(store.replay(&run.id).unwrap(), resumed.snapshot);
    }

    #[test]
    fn parallel_report_survives_another_story_pausing_for_input() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let stories = StoryStore::open().unwrap();
        let make_story = |title: &str, scope: &str| {
            stories
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan_id.clone(),
                    title: title.into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![scope.into()],
                })
                .unwrap()
        };
        let needs_input = make_story("Needs input", "src/first");
        let completed = make_story("Completes independently", "src/second");
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = project.path().canonicalize().unwrap();
        let run = store
            .start_plan(
                project_path.to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let bind = |story_id: &str, session_id: &str| {
            let started = store
                .command(
                    &run.id,
                    &format!("parallel-attempt:{story_id}"),
                    RunCommand::StartAttempt {
                        story_id: story_id.into(),
                        node_id: "implement".into(),
                    },
                )
                .unwrap();
            let attempt = started.snapshot.attempts.last().unwrap().clone();
            let reserved = store
                .command(
                    &run.id,
                    &format!("spawn:{story_id}"),
                    RunCommand::ReserveEffect {
                        key: format!("spawn:{}", attempt.id),
                        kind: EffectKind::SpawnAgent,
                    },
                )
                .unwrap();
            store
                .bind_agent(
                    &run.id,
                    &attempt.id,
                    AgentBinding {
                        session_id: session_id.into(),
                        task_id: None,
                        effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                        prompt_contract_version: 1,
                        prompt_sha256: "a".repeat(64),
                        audit_preview: "parallel test".into(),
                    },
                )
                .unwrap();
            attempt
        };
        let waiting = bind(&needs_input.id, "worker-waiting");
        let finishing = bind(&completed.id, "worker-finishing");
        let report = |story: &crate::stories::Story,
                      attempt: &NodeAttempt,
                      outcome: AttemptOutcome| AttemptReport {
            contract_version: 1,
            run_id: run.id.clone(),
            story_id: story.id.clone(),
            story_revision: story.revision,
            attempt_id: attempt.id.clone(),
            generation: attempt.generation,
            outcome,
            summary: "Worker result".into(),
            criterion_results: vec![],
            evidence: vec![],
            input_request: (outcome == AttemptOutcome::NeedsInput).then(|| InputRequest {
                question: "Which option?".into(),
                options: vec!["A".into()],
            }),
            review: None,
        };
        store
            .report_bound_agent(
                report(&needs_input, &waiting, AttemptOutcome::NeedsInput),
                "worker-waiting",
            )
            .unwrap();
        assert_eq!(store.snapshot(&run.id).unwrap().status, RunStatus::Paused);
        let completed_report = report(&completed, &finishing, AttemptOutcome::Completed);
        let receipt = store
            .report_bound_agent(completed_report.clone(), "worker-finishing")
            .unwrap();
        assert!(matches!(
            receipt.event.kind,
            RunEventKind::AttemptReported { .. }
        ));
        assert_eq!(receipt.snapshot.status, RunStatus::Paused);
        let saved = receipt
            .snapshot
            .attempts
            .iter()
            .find(|attempt| attempt.id == finishing.id)
            .unwrap();
        assert_eq!(saved.state, AttemptState::Reported);
        assert_eq!(saved.report.as_ref(), Some(&completed_report));
        assert_eq!(
            store
                .report_bound_agent(completed_report, "worker-finishing")
                .unwrap()
                .sequence,
            receipt.sequence
        );
        assert_eq!(store.replay(&run.id).unwrap(), receipt.snapshot);
        store
            .command(
                &run.id,
                "answer-parallel-input",
                RunCommand::AnswerInput {
                    attempt_id: waiting.id,
                    answer: "A".into(),
                },
            )
            .unwrap();
        let resumed = store
            .command(&run.id, "resume-parallel-run", RunCommand::Resume)
            .unwrap();
        assert_eq!(resumed.snapshot.status, RunStatus::Running);
        assert_eq!(
            resumed
                .snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == finishing.id)
                .unwrap()
                .report
                .as_ref(),
            saved.report.as_ref()
        );
        assert_eq!(store.replay(&run.id).unwrap(), resumed.snapshot);
    }

    #[test]
    fn reviewer_report_records_advisory_findings_without_changing_story_status() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let started = store
            .command(
                &run.id,
                "review-attempt",
                RunCommand::StartAttempt {
                    story_id: story_id.clone(),
                    node_id: "review".into(),
                },
            )
            .unwrap();
        let attempt = started.snapshot.attempts.last().unwrap();
        let reserved = store
            .command(
                &run.id,
                "review-spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        store
            .bind_agent(
                &run.id,
                &attempt.id,
                AgentBinding {
                    session_id: "review-worker".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "a".repeat(64),
                    audit_preview: "review".into(),
                },
            )
            .unwrap();
        let story = StoryStore::open().unwrap().get_story(&story_id).unwrap();
        let report: AttemptReport = serde_json::from_value(serde_json::json!({
            "contractVersion": 1, "runId": run.id, "storyId": story_id,
            "storyRevision": story.revision, "attemptId": attempt.id,
            "generation": attempt.generation, "outcome": "completed",
            "summary": "Criterion needs repair", "criterionResults": [], "evidence": [],
            "review": {"decision": "changes_requested", "artifactDigest": "a".repeat(64),
                "findings": [{"criterionIndex": 0, "severity": "major",
                    "summary": "Missing failure path", "evidence": "test did not cover error"}]}
        }))
        .unwrap();
        assert_eq!(report.review.as_ref().unwrap().findings.len(), 1);
        let mut invalid = report.clone();
        invalid.review.as_mut().unwrap().findings[0].criterion_index = story.criteria.len();
        assert!(store.report_bound_agent(invalid, "review-worker").is_err());
        assert!(
            store
                .report_bound_agent(report.clone(), "wrong-worker")
                .is_err()
        );
        let mut missing_review = report.clone();
        missing_review.review = None;
        assert!(
            store
                .report_bound_agent(missing_review, "review-worker")
                .is_err()
        );
        let receipt = store
            .report_bound_agent(report.clone(), "review-worker")
            .unwrap();
        assert_eq!(receipt.snapshot.attempts[0].report.as_ref(), Some(&report));
        assert_eq!(
            StoryStore::open()
                .unwrap()
                .get_story(&story.id)
                .unwrap()
                .status,
            story.status
        );
        assert_eq!(store.replay(&run.id).unwrap(), receipt.snapshot);
    }

    #[test]
    fn coordinator_wake_target_is_the_active_bound_plan_coordinator() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        assert_eq!(active_coordinator_session(&run).unwrap(), None);
        let started = store
            .command(
                &run.id,
                "coordinator-attempt",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .unwrap();
        let attempt = started.snapshot.attempts.last().unwrap();
        let reserved = store
            .command(
                &run.id,
                "coordinator-spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{}", attempt.id),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        let bound = store
            .bind_agent(
                &run.id,
                &attempt.id,
                AgentBinding {
                    session_id: "coordinator-pty".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "a".repeat(64),
                    audit_preview: "coordinate".into(),
                },
            )
            .unwrap();
        assert_eq!(
            active_coordinator_session(&bound.snapshot)
                .unwrap()
                .as_deref(),
            Some("coordinator-pty")
        );
        let interrupted = store.interrupt_agent_session("coordinator-pty").unwrap();
        assert_eq!(interrupted.len(), 1);
        assert_eq!(active_coordinator_session(&interrupted[0]).unwrap(), None);
    }

    #[test]
    fn workflow_dispatch_serializes_unknown_or_overlapping_scopes_and_limits_parallelism() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let stories = StoryStore::open().unwrap();
        let make = |title: &str, scope: Vec<&str>| {
            stories
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan_id.clone(),
                    title: title.into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: scope.into_iter().map(str::to_owned).collect(),
                })
                .unwrap()
        };
        let first = make("First", vec!["src/alpha"]);
        let separate = make("Separate", vec!["src/beta"]);
        let overlap = make("Overlap", vec!["src/alpha/nested"]);
        let unknown = make("Unknown", vec![]);
        let glob = make("Glob", vec!["src/{alpha,beta}"]);
        let third = make("Third", vec!["src/gamma"]);
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let start = |story_id: String, key: &str| {
            store.command(
                &run.id,
                key,
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
        };
        start(first.id, "first").unwrap();
        assert!(start(overlap.id, "overlap").is_err());
        assert!(start(unknown.id, "unknown").is_err());
        assert!(start(glob.id, "glob").is_err());
        start(separate.id, "separate").unwrap();
        assert!(start(third.id, "third").is_err());
    }

    #[test]
    fn workflow_dispatch_holds_dependents_after_manual_done_without_integration_receipt() {
        let (_config, project, plan_id, prerequisite_id, definition_id, _guard) = fixture();
        let store = RunStore::open().unwrap();
        let stories = StoryStore::open().unwrap();
        let dependent = stories
            .create_story(NewStory {
                story_type: None,
                plan_id: plan_id.clone(),
                title: "Dependent".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec!["src/dependent".into()],
            })
            .unwrap();
        stories
            .add_dependency(&dependent.id, &prerequisite_id, dependent.revision)
            .unwrap();
        let mut prerequisite = stories.get_story(&prerequisite_id).unwrap();
        for command in [
            StoryCommand::StartManual,
            StoryCommand::CheckCriterion(0),
            StoryCommand::SubmitReview,
            StoryCommand::Approve,
        ] {
            prerequisite = stories
                .transition(&prerequisite_id, prerequisite.revision, command)
                .unwrap();
        }
        assert_eq!(prerequisite.status, crate::stories::StoryStatus::Done);
        assert_eq!(
            stories.get_story(&dependent.id).unwrap().status,
            crate::stories::StoryStatus::Ready
        );
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        assert_eq!(
            stories.get_story(&dependent.id).unwrap().status,
            crate::stories::StoryStatus::Backlog
        );
        let error = store
            .command(
                &run.id,
                "dependent",
                RunCommand::StartAttempt {
                    story_id: dependent.id,
                    node_id: "implement".into(),
                },
            )
            .unwrap_err();
        assert!(error.contains("integration receipt"), "{error}");
    }

    /// Catches: archiving a run-owned story or its parent plan while dispatch can still use it.
    #[test]
    fn archive_refuses_active_run_ownership_and_allows_after_cancel() {
        let (_config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let runs = RunStore::open().unwrap();
        let run = runs
            .start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let stories = StoryStore::open().unwrap();
        let story = stories.get_story(&story_id).unwrap();
        let plan = stories.get_plan(&plan_id).unwrap();
        let actor = crate::stories::StoryTransitionActor::Human;
        for archived in [true, false] {
            assert!(
                stories
                    .set_story_archived(&story_id, story.revision, archived, actor.clone())
                    .unwrap_err()
                    .contains("active workflow")
            );
            assert!(
                stories
                    .set_plan_archived(&plan_id, plan.revision, archived, actor.clone())
                    .unwrap_err()
                    .contains("active workflow")
            );
        }
        runs.command(&run.id, "cancel-for-archive", RunCommand::Cancel)
            .unwrap();
        stories
            .set_story_archived(&story_id, story.revision, true, actor.clone())
            .unwrap();
        stories
            .set_plan_archived(&plan_id, plan.revision, true, actor)
            .unwrap();
        assert!(
            runs.start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default()
            )
            .unwrap_err()
            .to_string()
            .contains("archived")
        );
    }

    #[test]
    fn another_plans_workflow_run_does_not_hold_manual_dependents() {
        let (_config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let runs = RunStore::open().unwrap();
        runs.start_plan(
            project.path().canonicalize().unwrap().to_str().unwrap(),
            &plan_id,
            &definition_id,
            1,
            RunLimits::default(),
        )
        .unwrap();

        let stories = StoryStore::open().unwrap();
        let manual_plan = stories
            .create_plan(NewPlan {
                project: project
                    .path()
                    .canonicalize()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
                title: "Manual".into(),
                source: "manual.md".into(),
            })
            .unwrap();
        let make_story = |title: &str| NewStory {
            story_type: None,
            plan_id: manual_plan.id.clone(),
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let prerequisite = stories.create_story(make_story("Prerequisite")).unwrap();
        let dependent = stories.create_story(make_story("Dependent")).unwrap();
        stories
            .add_dependency(&dependent.id, &prerequisite.id, dependent.revision)
            .unwrap();
        let mut prerequisite = prerequisite;
        for command in [
            StoryCommand::StartManual,
            StoryCommand::CheckCriterion(0),
            StoryCommand::SubmitReview,
            StoryCommand::Approve,
        ] {
            prerequisite = stories
                .transition(&prerequisite.id, prerequisite.revision, command)
                .unwrap();
        }
        assert_eq!(prerequisite.status, crate::stories::StoryStatus::Done);
        assert_eq!(
            stories.get_story(&dependent.id).unwrap().status,
            crate::stories::StoryStatus::Ready
        );
    }

    fn stored_schema_version(db: &std::path::Path) -> i64 {
        rusqlite::Connection::open(db)
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn run_store_records_its_schema_version_and_keeps_a_pre_version_store() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let db = config.path().join("runs-versioned.sqlite3");
        let store = RunStore::open_at(&db).unwrap();
        // 4: paused_for events; the previous build (3) must refuse the store, not fail to replay it.
        assert_eq!(stored_schema_version(&db), 4, "fresh store records version");
        let run = store
            .start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        // A store written before the version existed reports 0 with data in it.
        rusqlite::Connection::open(&db)
            .unwrap()
            .pragma_update(None, "user_version", 0)
            .unwrap();
        let upgraded = RunStore::open_at(&db).unwrap();
        assert_eq!(stored_schema_version(&db), 4);
        assert_eq!(
            upgraded.snapshot(&run.id).unwrap().sequence,
            run.sequence,
            "the pre-version run survives the upgrade"
        );
    }

    #[test]
    fn run_store_refuses_a_newer_schema_version_without_touching_it() {
        let config = tempfile::tempdir().unwrap();
        let db = config.path().join("runs-future.sqlite3");
        RunStore::open_at(&db).unwrap();
        rusqlite::Connection::open(&db)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        let err = RunStore::open_at(&db).unwrap_err();
        assert!(err.contains("99") && err.contains("newer"), "got: {err}");
        assert_eq!(stored_schema_version(&db), 99);
    }

    #[test]
    fn runtime_reconciliation_preserves_live_attempts_and_in_flight_effects() {
        // catches: invoking runtime reconciliation interrupting a healthy worker
        // and marking an in-progress external effect uncertain.
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
        let run = store
            .start_plan(
                project.path().canonicalize().unwrap().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        store
            .command(
                &run.id,
                "live-attempt",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .unwrap();
        let live = store
            .command(
                &run.id,
                "live-effect",
                RunCommand::ReserveEffect {
                    key: "spawn-live-worker".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap()
            .snapshot;
        let observed = store.reconcile(&run.id).unwrap();
        assert_eq!(observed, live);
        assert_eq!(observed.attempts[0].state, AttemptState::Running);
        assert_eq!(observed.effects[0].state, EffectState::Intended);
        assert_eq!(observed.status, RunStatus::Running);
        assert_eq!(store.replay(&run.id).unwrap(), live);
    }

    #[test]
    fn daemon_owner_startup_recovers_healthy_runs_past_a_corrupt_snapshot() {
        // catches: one unreadable run aborting owner-startup recovery of every other run.
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let db = config.path().join("workflow_runs.sqlite3");
        let store = RunStore::open_at(&db).unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let corrupt = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let stories = StoryStore::open().unwrap();
        let healthy_plan = stories
            .create_plan(NewPlan {
                project: project_path.clone(),
                title: "Healthy plan".into(),
                source: "healthy.md".into(),
            })
            .unwrap();
        let healthy = store
            .start_plan(
                &project_path,
                &healthy_plan.id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE workflow_runs SET snapshot_json='broken JSON' WHERE id=?1",
            [&corrupt.id],
        )
        .unwrap();
        drop(conn);
        let owner = super::runtime::RuntimeOwner::acquire(&db)
            .expect("bad run must not prevent owner-startup recovery of healthy runs");
        let reopened = &owner.store;
        let recovered = reopened.snapshot(&healthy.id).unwrap();
        assert_eq!(recovered.status, RunStatus::Paused);
        assert_eq!(reopened.replay(&healthy.id).unwrap(), recovered);
        assert!(reopened.snapshot(&corrupt.id).is_err());
        assert_eq!(
            RunStore::open()
                .unwrap()
                .snapshot(&healthy.id)
                .unwrap()
                .sequence,
            recovered.sequence
        );
    }

    #[test]
    fn daemon_owner_startup_reconciles_existing_active_run_once() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let db = config.path().join("workflow_runs.sqlite3");
        let stored = RunStore::open_at(&db).unwrap();
        let run = stored
            .start_plan(
                project.path().canonicalize().unwrap().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        assert_eq!(stored.snapshot(&run.id).unwrap().status, RunStatus::Running);

        let opened = RunStore::open().unwrap();
        assert_eq!(
            opened.snapshot(&run.id).unwrap().status,
            RunStatus::Running,
            "a read must not recover a live run"
        );
        let owner = super::runtime::RuntimeOwner::acquire(&db).unwrap();
        let recovered = owner.store.snapshot(&run.id).unwrap();
        assert_eq!(recovered.status, RunStatus::Paused);
        let reopened = RunStore::open().unwrap();
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().sequence,
            recovered.sequence
        );
    }

    #[test]
    fn workflow_assigns_distinct_registered_worktrees_before_worker_spawn() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let repo = project.path();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Workflow Test"],
            vec!["config", "user.email", "workflow@example.invalid"],
        ] {
            crate::git_cli::git_cmd(repo).args(args).run().unwrap();
        }
        std::fs::write(repo.join("README.md"), "initial\n").unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["add", "README.md"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["commit", "-qm", "initial"])
            .run()
            .unwrap();
        let path_a = config.path().join("worktree-a");
        let path_b = config.path().join("worktree-b");
        for (name, path) in [("story-a", &path_a), ("story-b", &path_b)] {
            crate::git_cli::git_cmd(repo)
                .args(["worktree", "add", "-q", "-b", name, path.to_str().unwrap()])
                .run()
                .unwrap();
        }
        let stories = StoryStore::open().unwrap();
        let create = |title: &str, scope: &str| {
            stories
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan_id.clone(),
                    title: title.into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![scope.into()],
                })
                .unwrap()
        };
        let first = create("First", "src/a");
        let second = create("Second", "src/b");
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = repo.canonicalize().unwrap().to_string_lossy().to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        for (key, story_id) in [("first", &first.id), ("second", &second.id)] {
            store
                .command(
                    &run.id,
                    key,
                    RunCommand::StartAttempt {
                        story_id: story_id.clone(),
                        node_id: "implement".into(),
                    },
                )
                .unwrap();
        }
        let assign = |key: &str, story_id: String, path: &std::path::Path| {
            store.command(
                &run.id,
                key,
                RunCommand::AssignWorktree {
                    story_id,
                    path: path.canonicalize().unwrap().to_string_lossy().to_string(),
                },
            )
        };
        let first_assignment = assign("assign-first", first.id.clone(), &path_a).unwrap();
        assert_eq!(
            assign("assign-first", first.id.clone(), &path_a).unwrap(),
            first_assignment
        );
        assert!(assign("assign-first", first.id.clone(), &path_b).is_err());
        assert!(assign("assign-duplicate", second.id.clone(), &path_a).is_err());
        let unrelated = config.path().join("unrelated");
        std::fs::create_dir(&unrelated).unwrap();
        assert!(assign("assign-unregistered", second.id.clone(), &unrelated).is_err());
        let result = assign("assign-second", second.id.clone(), &path_b).unwrap();
        assert_eq!(
            result
                .snapshot
                .stories
                .iter()
                .find(|entry| entry.story_id == first.id)
                .unwrap()
                .worktree_path
                .as_deref(),
            path_a.canonicalize().unwrap().to_str()
        );
        assert_eq!(
            result
                .snapshot
                .stories
                .iter()
                .find(|entry| entry.story_id == second.id)
                .unwrap()
                .worktree_path
                .as_deref(),
            path_b.canonicalize().unwrap().to_str()
        );
        assert_eq!(store.replay(&run.id).unwrap(), result.snapshot);
    }

    #[test]
    fn cancellation_records_an_unfinished_spawn_as_uncertain() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let started = store
            .command(
                &run.id,
                "attempt",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .unwrap();
        let attempt_id = started.snapshot.attempts[0].id.clone();
        let reserved = store
            .command(
                &run.id,
                "spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{attempt_id}"),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        let cancelled = store
            .command(&run.id, "cancel", RunCommand::Cancel)
            .unwrap();
        assert_eq!(cancelled.snapshot.status, RunStatus::Cancelled);
        assert_eq!(cancelled.snapshot.effects[0].state, EffectState::Uncertain);
        assert_eq!(
            cancelled.snapshot.attempts[0].state,
            AttemptState::Interrupted
        );
        assert!(
            store
                .bind_agent(
                    &run.id,
                    &attempt_id,
                    AgentBinding {
                        session_id: "late-agent".into(),
                        task_id: None,
                        effect_id: reserved.snapshot.effects[0].id.clone(),
                        prompt_contract_version: 1,
                        prompt_sha256: "a".repeat(64),
                        audit_preview: "redacted".into(),
                    }
                )
                .is_err()
        );
        assert_eq!(store.replay(&run.id).unwrap(), cancelled.snapshot);
    }

    #[test]
    fn plan_coordinator_attempt_does_not_create_a_story_execution() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        let started = store
            .command(
                &run.id,
                "coordinate",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .expect("plan coordinator");
        assert_eq!(started.snapshot.attempts[0].story_id, plan_id);
        assert_eq!(
            started.snapshot.stories,
            [] as [crate::workflows::run::model::StoryExecution; 0]
        );
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn only_the_bound_coordinator_can_create_a_story_once() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let started = store
            .command(
                &run.id,
                "coordinate",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .unwrap();
        let attempt_id = started.snapshot.attempts[0].id.clone();
        let reserved = store
            .command(
                &run.id,
                "spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{attempt_id}"),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        store
            .bind_agent(
                &run.id,
                &attempt_id,
                AgentBinding {
                    session_id: "coordinator".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects[0].id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "a".repeat(64),
                    audit_preview: "redacted".into(),
                },
            )
            .unwrap();
        let proposed = NewStory {
            story_type: None,
            plan_id: plan_id.clone(),
            title: "Follow up".into(),
            criteria: vec!["Covered".into()],
            priority: 1,
            origin: StoryOrigin::PlanStep {
                step: "follow-up".into(),
            },
            file_scope: vec![],
        };
        assert!(
            store
                .create_story_from_coordinator(&run.id, "stranger", "proposal-1", proposed.clone())
                .is_err()
        );
        let first = store
            .create_story_from_coordinator(&run.id, "coordinator", "proposal-1", proposed.clone())
            .unwrap();
        let second = store
            .create_story_from_coordinator(&run.id, "coordinator", "proposal-1", proposed.clone())
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            StoryStore::open()
                .unwrap()
                .list_stories(&plan_id)
                .unwrap()
                .len(),
            2
        );
        assert!(
            store
                .snapshot(&run.id)
                .unwrap()
                .effects
                .iter()
                .any(|effect| {
                    effect.key == "create-story:proposal-1"
                        && effect.state == EffectState::Succeeded
                })
        );
        let mut changed = proposed;
        changed.title = "Different".into();
        assert!(
            store
                .create_story_from_coordinator(&run.id, "coordinator", "proposal-1", changed)
                .is_err()
        );
    }

    #[test]
    fn effect_outcomes_remain_recordable_when_story_shape_reopens_planning() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .expect("close planning");
        let first = store
            .command(
                &run.id,
                "effect-1",
                RunCommand::ReserveEffect {
                    key: "spawn-first".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("first effect")
            .snapshot
            .effects[0]
            .id
            .clone();
        let second = store
            .command(
                &run.id,
                "effect-2",
                RunCommand::ReserveEffect {
                    key: "spawn-second".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("second effect")
            .snapshot
            .effects[1]
            .id
            .clone();
        StoryStore::open()
            .unwrap()
            .create_story(NewStory {
                plan_id,
                title: "New evidence".into(),
                criteria: vec!["Reviewed".into()],
                priority: 2,
                story_type: None,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("change planning fingerprint");
        let marked = store
            .command(
                &run.id,
                "mark-first",
                RunCommand::MarkEffect {
                    effect_id: first,
                    succeeded: true,
                },
            )
            .expect("mark observed success");
        assert!(matches!(
            marked.event.kind,
            RunEventKind::EffectChanged {
                state: EffectState::Succeeded,
                ..
            }
        ));
        let uncertain = store
            .reconcile_after_restart(&run.id)
            .expect("reconcile remaining intent after restart");
        assert_eq!(uncertain.effects[1].state, EffectState::Uncertain);
        let resolved = store
            .command(
                &run.id,
                "resolve-second",
                RunCommand::ResolveUncertainEffect {
                    effect_id: second,
                    succeeded: false,
                },
            )
            .expect("resolve observed failure");
        assert!(matches!(
            resolved.event.kind,
            RunEventKind::EffectChanged {
                state: EffectState::Failed,
                ..
            }
        ));
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    /// Catches: a bound coordinator continuing to propose work from a replaced source document.
    #[test]
    fn update_plan_source_refuses_an_active_bound_coordinator() {
        let (_config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open().unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let started = store
            .command(
                &run.id,
                "coordinate",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .unwrap();
        let attempt_id = started.snapshot.attempts[0].id.clone();
        let reserved = store
            .command(
                &run.id,
                "spawn",
                RunCommand::ReserveEffect {
                    key: format!("spawn:{attempt_id}"),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .unwrap();
        store
            .bind_agent(
                &run.id,
                &attempt_id,
                AgentBinding {
                    session_id: "coordinator".into(),
                    task_id: None,
                    effect_id: reserved.snapshot.effects[0].id.clone(),
                    prompt_contract_version: 1,
                    prompt_sha256: "a".repeat(64),
                    audit_preview: "redacted".into(),
                },
            )
            .unwrap();
        let stories = StoryStore::open().unwrap();
        let before = stories.get_plan(&plan_id).unwrap();
        let error = stories
            .update_plan(
                &plan_id,
                before.revision,
                crate::stories::PlanPatch {
                    title: Some("Must not partially apply".into()),
                    source: Some("moved.md".into()),
                },
            )
            .expect_err("source correction must not leave an old-source coordinator active");
        assert!(error.contains("active plan coordinator"), "{error}");
        assert_eq!(stories.get_plan(&plan_id).unwrap(), before);
        assert!(crate::stories::is_user_error(&error));
        // Title-only and unchanged-source patches do not replace the coordinator's document.
        let renamed = stories
            .update_plan(
                &plan_id,
                before.revision,
                crate::stories::PlanPatch {
                    title: Some("Corrected title".into()),
                    source: None,
                },
            )
            .unwrap();
        assert_eq!(renamed.source, before.source);
        let unchanged = stories
            .update_plan(
                &plan_id,
                renamed.revision,
                crate::stories::PlanPatch {
                    title: None,
                    source: Some(format!(" {} ", before.source)),
                },
            )
            .unwrap();
        assert_eq!(unchanged.source, before.source);
        let other = stories
            .create_plan(NewPlan {
                project: project_path,
                title: "Other plan".into(),
                source: "other.md".into(),
            })
            .unwrap();
        assert_eq!(
            stories
                .update_plan(
                    &other.id,
                    other.revision,
                    crate::stories::PlanPatch {
                        title: None,
                        source: Some("unrelated.md".into()),
                    }
                )
                .unwrap()
                .source,
            "unrelated.md"
        );
        store
            .command(&run.id, "cancel", RunCommand::Cancel)
            .unwrap();
        assert_eq!(
            stories
                .update_plan(
                    &plan_id,
                    unchanged.revision,
                    crate::stories::PlanPatch {
                        title: None,
                        source: Some("moved.md".into()),
                    }
                )
                .unwrap()
                .source,
            "moved.md"
        );
    }

    /// Catches: a not-yet-bound or paused coordinator being missed by a session-only guard.
    #[test]
    fn update_plan_source_refuses_unbound_and_paused_coordinators() {
        let (_config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open().unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        store
            .command(
                &run.id,
                "coordinate",
                RunCommand::StartPlanAgent {
                    node_id: "coordinate".into(),
                },
            )
            .unwrap();
        let stories = StoryStore::open().unwrap();
        let before = stories.get_plan(&plan_id).unwrap();
        for paused in [false, true] {
            if paused {
                store.command(&run.id, "pause", RunCommand::Pause).unwrap();
            }
            let error = stories
                .update_plan(
                    &plan_id,
                    before.revision,
                    crate::stories::PlanPatch {
                        title: None,
                        source: Some("moved.md".into()),
                    },
                )
                .unwrap_err();
            assert!(error.contains("active plan coordinator"), "{error}");
            assert_eq!(stories.get_plan(&plan_id).unwrap(), before);
        }
    }

    /// Catches: a corrected plan source retaining closed planning and verified subjects.
    #[test]
    fn update_plan_source_reopens_planning_and_changes_verification_subject() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let closed = store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .unwrap();
        assert!(closed.snapshot.planning_fingerprint.is_some());
        let stories = StoryStore::open().unwrap();
        let original = stories.get_plan(&plan_id).unwrap();
        let before =
            store::fingerprint(&plan_id, &stories.list_stories(&plan_id).unwrap(), true).unwrap();
        let empty_before = store::fingerprint(&plan_id, &[], false).unwrap();
        stories
            .update_plan(
                &plan_id,
                original.revision,
                crate::stories::PlanPatch {
                    title: None,
                    source: Some("moved.md".into()),
                },
            )
            .unwrap();
        assert_ne!(
            before,
            store::fingerprint(&plan_id, &stories.list_stories(&plan_id).unwrap(), true).unwrap()
        );
        assert_ne!(
            empty_before,
            store::fingerprint(&plan_id, &[], false).unwrap()
        );
        let reopened = store
            .command(&run.id, "after-source-edit", RunCommand::ClosePlanning)
            .unwrap();
        assert!(matches!(
            reopened.event.kind,
            RunEventKind::PlanningReopened
        ));
        assert!(reopened.snapshot.planning_fingerprint.is_none());
        assert!(reopened.snapshot.verification_fingerprint.is_none());
        let reclosed = store
            .command(&run.id, "reclose", RunCommand::ClosePlanning)
            .unwrap();
        assert_ne!(
            closed.snapshot.planning_fingerprint,
            reclosed.snapshot.planning_fingerprint
        );
    }

    #[test]
    fn command_events_and_projection_replay_after_reopen() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        let closed = store
            .command(&run.id, "close-plan", RunCommand::ClosePlanning)
            .expect("close planning");
        assert!(closed.snapshot.planning_fingerprint.is_some());
        let same = store
            .command(&run.id, "close-plan", RunCommand::ClosePlanning)
            .expect("duplicate");
        assert_eq!(same.sequence, closed.sequence);
        let attempt = store
            .command(
                &run.id,
                "attempt-1",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .expect("attempt");
        assert_eq!(attempt.snapshot.attempts.len(), 1);
        drop(store);
        let reopened = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("reopen");
        assert_eq!(
            reopened.replay(&run.id).expect("replay"),
            reopened.snapshot(&run.id).expect("snapshot")
        );
        assert_eq!(
            reopened
                .events_after(&run.id, 0, 100)
                .expect("events")
                .len(),
            3
        );
        assert_eq!(
            reopened
                .reconcile_active_after_restart()
                .expect("restart reconciliation"),
            1
        );
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().status,
            RunStatus::Paused
        );
        let late = reopened
            .command(
                &run.id,
                "late-after-restart",
                RunCommand::ReportAttempt {
                    attempt_id: attempt.snapshot.attempts[0].id.clone(),
                    generation: attempt.snapshot.attempts[0].generation,
                    outcome: AttemptOutcome::Completed,
                },
            )
            .expect("audit late report");
        assert!(matches!(
            late.event.kind,
            RunEventKind::LateReportIgnored { .. }
        ));
        assert_eq!(late.snapshot.attempts[0].state, AttemptState::Interrupted);
    }

    #[test]
    fn stale_attempt_report_is_audited_without_advancing() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        let started = store
            .command(
                &run.id,
                "attempt-1",
                RunCommand::StartAttempt {
                    story_id,
                    node_id: "implement".into(),
                },
            )
            .expect("attempt");
        let attempt = &started.snapshot.attempts[0];
        let stale = store
            .command(
                &run.id,
                "late",
                RunCommand::ReportAttempt {
                    attempt_id: attempt.id.clone(),
                    generation: attempt.generation + 1,
                    outcome: AttemptOutcome::Completed,
                },
            )
            .expect("audited late report");
        assert!(matches!(
            stale.event.kind,
            RunEventKind::LateReportIgnored { .. }
        ));
        assert_eq!(stale.snapshot.attempts[0].state, AttemptState::Running);
        let report = store
            .command(
                &run.id,
                "report",
                RunCommand::ReportAttempt {
                    attempt_id: attempt.id.clone(),
                    generation: attempt.generation,
                    outcome: AttemptOutcome::Completed,
                },
            )
            .expect("report");
        assert_eq!(report.snapshot.attempts[0].state, AttemptState::Reported);
    }

    #[test]
    fn uncertain_effect_is_not_replayed_after_restart_and_limits_hold() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let limits = RunLimits {
            max_loops: 1,
            max_story_creations: 1,
            max_spawns: 1,
            max_parallel_stories: 1,
            max_duration_secs: 3600,
        };
        let run = store
            .start_plan(&project_path, &plan_id, &definition_id, 1, limits)
            .expect("start");
        store
            .command(
                &run.id,
                "spawn-1",
                RunCommand::ReserveEffect {
                    key: "spawn-one".into(),
                    kind: EffectKind::SpawnAgent,
                },
            )
            .expect("spawn intent");
        assert!(
            store
                .command(
                    &run.id,
                    "spawn-2",
                    RunCommand::ReserveEffect {
                        key: "spawn-two".into(),
                        kind: EffectKind::SpawnAgent
                    }
                )
                .is_err()
        );
        store
            .command(&run.id, "loop-1", RunCommand::AdvanceLoop)
            .expect("loop");
        assert!(
            store
                .command(&run.id, "loop-2", RunCommand::AdvanceLoop)
                .is_err()
        );
        drop(store);
        let reopened = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("reopen");
        assert_eq!(
            reopened
                .reconcile_active_after_restart()
                .expect("reconcile active runs"),
            1
        );
        let snapshot = reopened.snapshot(&run.id).expect("snapshot");
        assert_eq!(snapshot.effects[0].state, EffectState::Uncertain);
        assert_eq!(snapshot.status, RunStatus::Paused);
        assert!(
            reopened
                .command(&run.id, "resume-too-soon", RunCommand::Resume)
                .is_err()
        );
        reopened
            .command(
                &run.id,
                "resolve",
                RunCommand::ResolveUncertainEffect {
                    effect_id: snapshot.effects[0].id.clone(),
                    succeeded: false,
                },
            )
            .expect("resolve uncertain effect");
        reopened
            .command(&run.id, "resume", RunCommand::Resume)
            .expect("resume");
        assert_eq!(
            reopened.snapshot(&run.id).unwrap().status,
            RunStatus::Running
        );
    }

    /// Catches: legacy dispatched executions being editable because they lack a graph reservation.
    #[test]
    fn update_story_refuses_legacy_run_owned_stories() {
        let (_config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let runs = RunStore::open().unwrap();
        let run = runs
            .start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        runs.command(
            &run.id,
            "attempt",
            RunCommand::StartAttempt {
                story_id: story_id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
        let stories = StoryStore::open().unwrap();
        let current = stories.get_story(&story_id).unwrap();
        let error = stories
            .update_story(
                &story_id,
                current.revision,
                crate::stories::StoryPatch {
                    priority: Some(3),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(error.contains("owned by an active workflow"), "{error}");
        assert_eq!(stories.get_story(&story_id).unwrap(), current);
    }

    /// Catches: amended requirements/metadata reusing planning or final verification evidence.
    #[test]
    fn update_story_reopens_planning_through_existing_fingerprints() {
        let (_config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open().unwrap();
        let stories = StoryStore::open().unwrap();
        let run = store
            .start_plan(
                project.path().to_str().unwrap(),
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .unwrap();
        for (index, patch) in [
            serde_json::json!({"criteria": ["Revised acceptance"]}),
            serde_json::json!({"problem": "New requirement context"}),
            serde_json::json!({"risk": "high"}),
            serde_json::json!({"storyType": "fix"}),
            serde_json::json!({"priority": 3}),
        ]
        .into_iter()
        .enumerate()
        {
            store
                .command(
                    &run.id,
                    &format!("close-{index}"),
                    RunCommand::ClosePlanning,
                )
                .unwrap();
            let current = stories.get_story(&story_id).unwrap();
            let before =
                super::store::fingerprint(&plan_id, std::slice::from_ref(&current), true).unwrap();
            let updated = stories
                .update_story(
                    &story_id,
                    current.revision,
                    serde_json::from_value(patch).unwrap(),
                )
                .unwrap();
            assert_ne!(
                before,
                super::store::fingerprint(&plan_id, &[updated], true).unwrap()
            );
            let receipt = store
                .command(
                    &run.id,
                    &format!("verify-{index}"),
                    RunCommand::FinalVerificationPassed,
                )
                .unwrap();
            assert!(matches!(receipt.event.kind, RunEventKind::PlanningReopened));
            assert!(receipt.snapshot.planning_fingerprint.is_none());
            assert!(receipt.snapshot.verification_fingerprint.is_none());
        }
    }

    #[test]
    fn adding_a_story_reopens_planning_before_the_next_command() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .expect("close");
        StoryStore::open()
            .unwrap()
            .create_story(NewStory {
                plan_id,
                title: "New work".into(),
                criteria: vec!["New criterion".into()],
                priority: 1,
                story_type: None,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("add story");
        let receipt = store
            .command(&run.id, "verify", RunCommand::FinalVerificationPassed)
            .expect("reopen planning");
        assert!(matches!(receipt.event.kind, RunEventKind::PlanningReopened));
        assert!(receipt.snapshot.planning_fingerprint.is_none());
        assert!(receipt.snapshot.verification_fingerprint.is_none());
    }

    #[test]
    fn command_identity_is_bound_to_the_payload_and_only_one_run_can_be_active() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        assert!(
            store
                .start_plan(
                    &project_path,
                    &plan_id,
                    &definition_id,
                    1,
                    RunLimits::default()
                )
                .is_err()
        );
        store
            .command(&run.id, "same", RunCommand::ClosePlanning)
            .expect("first command");
        assert!(store.command(&run.id, "same", RunCommand::Pause).is_err());
    }

    #[test]
    fn concurrent_retry_commits_one_event_and_one_projection() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        std::thread::scope(|scope| {
            let first = scope.spawn(|| store.command(&run.id, "same-pause", RunCommand::Pause));
            let second = scope.spawn(|| store.command(&run.id, "same-pause", RunCommand::Pause));
            assert_eq!(first.join().unwrap().unwrap().sequence, 2);
            assert_eq!(second.join().unwrap().unwrap().sequence, 2);
        });
        assert_eq!(store.snapshot(&run.id).unwrap().sequence, 2);
        assert_eq!(store.events_after(&run.id, 0, 10).unwrap().len(), 2);
        assert_eq!(
            store.replay(&run.id).unwrap(),
            store.snapshot(&run.id).unwrap()
        );
    }

    #[test]
    fn duration_budget_pauses_before_processing_an_expired_command() {
        let (config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits {
                    max_duration_secs: 1,
                    ..RunLimits::default()
                },
            )
            .expect("start");
        let receipt = store
            .command_at(
                &run.id,
                "late",
                RunCommand::ClosePlanning,
                run.started_ms + 1001,
            )
            .expect("pause");
        assert!(matches!(receipt.event.kind, RunEventKind::Paused));
        assert_eq!(receipt.snapshot.status, RunStatus::Paused);
        assert!(receipt.snapshot.planning_fingerprint.is_none());
    }

    #[test]
    fn run_api_scopes_reads_and_commands_to_the_canonical_project() {
        let (_config, project, plan_id, _story_id, definition_id, _guard) = fixture();
        let other = tempfile::tempdir().expect("other project");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let RunReply::Snapshot(run) = run_action(
            &project_path,
            RunAction::StartPlan {
                plan_id: plan_id.clone(),
                definition_id: definition_id.clone(),
                definition_revision: 1,
                limits: RunLimits::default(),
            },
        )
        .expect("start") else {
            panic!("snapshot reply");
        };
        let RunReply::Runs(runs) = run_action(
            &project_path,
            RunAction::ListPlanRuns {
                plan_id: plan_id.clone(),
                limit: 20,
            },
        )
        .expect("list runs") else {
            panic!("runs reply")
        };
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run.id);
        // Catches: an incident read bypasses project scope or advances the run.
        let state = crate::state::tests_support::make_test_app_state();
        let RunReply::Incidents(incidents) = run_action_with_events(
            &state,
            &project_path,
            RunAction::Incidents {
                run_id: run.id.clone(),
            },
        )
        .expect("incidents") else {
            panic!("incident reply")
        };
        assert_eq!(
            incidents,
            [] as [crate::workflows::run::incidents::RunIncident; 0]
        );
        assert_eq!(
            RunStore::open()
                .unwrap()
                .snapshot(&run.id)
                .unwrap()
                .sequence,
            run.sequence
        );
        assert!(
            run_action_with_events(
                &state,
                other.path().to_str().unwrap(),
                RunAction::Incidents {
                    run_id: run.id.clone()
                },
            )
            .is_err()
        );

        assert!(
            run_action(
                other.path().to_str().unwrap(),
                RunAction::ListPlanRuns {
                    plan_id: plan_id.clone(),
                    limit: 20,
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                &project_path,
                RunAction::ListPlanRuns {
                    plan_id: plan_id.clone(),
                    limit: 0,
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                other.path().to_str().unwrap(),
                RunAction::Get {
                    run_id: run.id.clone()
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                other.path().to_str().unwrap(),
                RunAction::Command {
                    run_id: run.id.clone(),
                    command_id: "other-project".into(),
                    expected_sequence: run.sequence,
                    command: Box::new(RunCommand::Pause),
                }
            )
            .is_err()
        );
        let RunReply::Events(events) = run_action(
            std::path::Path::new(&project_path)
                .join(".")
                .to_str()
                .unwrap(),
            RunAction::Events {
                run_id: run.id.clone(),
                after_sequence: 0,
                limit: 10,
            },
        )
        .expect("events") else {
            panic!("events reply");
        };
        assert_eq!(events.len(), 1);
        let RunReply::Receipt(paused) = run_action(
            &project_path,
            RunAction::Command {
                run_id: run.id.clone(),
                command_id: "pause".into(),
                expected_sequence: run.sequence,
                command: Box::new(RunCommand::Pause),
            },
        )
        .expect("pause") else {
            panic!("receipt reply");
        };
        assert_eq!(paused.sequence, run.sequence + 1);
        assert!(
            run_action(
                &project_path,
                RunAction::Command {
                    run_id: run.id.clone(),
                    command_id: "forged-recertification".into(),
                    expected_sequence: paused.sequence,
                    command: Box::new(RunCommand::RecordRecertification {
                        receipt: CanonicalReceipt {
                            canonical_ref: "refs/heads/main".into(),
                            commit: "a".repeat(40),
                            tree: "b".repeat(40),
                            post_checks: vec![],
                        },
                    }),
                }
            )
            .is_err()
        );
        assert!(
            run_action(
                &project_path,
                RunAction::Command {
                    run_id: run.id,
                    command_id: "stale-cancel".into(),
                    expected_sequence: run.sequence,
                    command: Box::new(RunCommand::Cancel),
                }
            )
            .is_err()
        );
        let RunReply::Receipt(cancelled) = run_action(
            &project_path,
            RunAction::Command {
                run_id: paused.snapshot.id,
                command_id: "cancel-after-pause".into(),
                expected_sequence: paused.sequence,
                command: Box::new(RunCommand::Cancel),
            },
        )
        .expect("cancel") else {
            panic!("cancel receipt")
        };
        assert_eq!(cancelled.snapshot.status, RunStatus::Cancelled);
        let RunReply::Snapshot(newest) = run_action(
            &project_path,
            RunAction::StartPlan {
                plan_id: plan_id.clone(),
                definition_id,
                definition_revision: 1,
                limits: RunLimits::default(),
            },
        )
        .expect("restart plan") else {
            panic!("new run")
        };
        let RunReply::Runs(latest) =
            run_action(&project_path, RunAction::ListPlanRuns { plan_id, limit: 1 })
                .expect("latest run")
        else {
            panic!("latest runs")
        };
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].id, newest.id);
    }

    #[test]
    fn resolve_plan_requires_closure_accepted_stories_and_integration_receipts() {
        let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).expect("run store");
        let project_path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let run = store
            .start_plan(
                &project_path,
                &plan_id,
                &definition_id,
                1,
                RunLimits::default(),
            )
            .expect("start");
        assert!(
            store
                .command(&run.id, "too-early", RunCommand::Complete)
                .is_err()
        );
        store
            .command(&run.id, "close", RunCommand::ClosePlanning)
            .expect("close");
        assert!(
            store
                .command(&run.id, "still-early", RunCommand::Complete)
                .is_err()
        );
        let stories = StoryStore::open().expect("stories");
        let story = stories.get_story(&story_id).expect("story");
        let story = stories
            .claim(&story_id, "tab", story.revision)
            .expect("claim");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::CheckCriterion(0))
            .expect("check");
        let story = stories
            .transition(&story_id, story.revision, StoryCommand::SubmitReview)
            .expect("review");
        stories
            .transition(&story_id, story.revision, StoryCommand::Approve)
            .expect("approve");
        store
            .command(
                &run.id,
                "accept",
                RunCommand::AcceptStory {
                    story_id: story_id.clone(),
                },
            )
            .expect("accept");
        let mut revised = stories.get_story(&story_id).expect("accepted story");
        revised.revision += 1;
        assert!(
            super::store::ready_to_verify(&store.snapshot(&run.id).unwrap(), &[revised]).is_err()
        );
        assert!(
            store
                .command(&run.id, "verify", RunCommand::FinalVerificationPassed)
                .is_err()
        );
        assert!(
            store
                .command(&run.id, "complete", RunCommand::Complete)
                .is_err()
        );
    }
}
