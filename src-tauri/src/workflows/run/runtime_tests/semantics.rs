use super::super::runtime::{effects::drive_effect, judge::artifact_digest};
use super::*;
use crate::stories::StoryCommand;

fn edge(from: &str, to: &str, outcome: Option<&str>) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        outcome: outcome.map(str::to_owned),
    }
}

pub(super) fn publish(project: &str, graph: WorkflowGraph) -> crate::workflows::PublishedWorkflow {
    let project = canonical_owner(project);
    let project = project.as_str();
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions
        .create_draft(project, "Executor semantics", WorkflowKind::Story, graph)
        .unwrap();
    let draft = definitions
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "status".into(),
                argv: vec!["git".into(), "status".into(), "--porcelain".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    let crate::workflows::WorkflowReply::Published(published) =
        crate::workflows::definition_action(
            project,
            crate::workflows::WorkflowAction::Publish {
                id: draft.id,
                expected_revision: draft.draft_revision,
            },
        )
        .unwrap()
    else {
        panic!("published definition")
    };
    published
}

fn init_worktree(repo: &std::path::Path, config: &std::path::Path) -> String {
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
    let path = config.join("artifact");
    crate::git_cli::git_cmd(repo)
        .args([
            "worktree",
            "add",
            "-q",
            "-b",
            "artifact",
            path.to_str().unwrap(),
        ])
        .run()
        .unwrap();
    path.canonicalize().unwrap().to_string_lossy().into_owned()
}

fn bind(store: &RunStore, run_id: &str, attempt: &NodeAttempt, session: &str) {
    let reserved = store
        .command(
            run_id,
            &format!("test:spawn:{}", attempt.id),
            RunCommand::ReserveEffect {
                key: format!("spawn:{}", attempt.id),
                kind: EffectKind::SpawnAgent,
            },
        )
        .unwrap();
    let RunEventKind::EffectReserved { effect } = reserved.event.kind else {
        panic!("effect")
    };
    store
        .bind_agent(
            run_id,
            &attempt.id,
            AgentBinding {
                acp: None,
                launch_id: None,
                session_id: session.into(),
                task_id: None,
                effect_id: effect.id,
                prompt_contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
                prompt_sha256: "a".repeat(64),
                audit_preview: "Native service-boundary test".into(),
            },
        )
        .unwrap();
}

fn report(
    store: &RunStore,
    run_id: &str,
    attempt: &NodeAttempt,
    session: &str,
    review: Option<ReviewAssessment>,
) {
    let story = StoryStore::open()
        .unwrap()
        .get_story(&attempt.story_id)
        .unwrap();
    store
        .report_bound_agent(
            AttemptReport {
                contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
                run_id: run_id.into(),
                story_id: story.id,
                story_revision: story.revision,
                attempt_id: attempt.id.clone(),
                generation: attempt.generation,
                outcome: AttemptOutcome::Completed,
                summary: "Native current evidence".into(),
                criterion_results: vec![CriterionResult {
                    index: 0,
                    satisfied: true,
                    evidence: "Verified criterion".into(),
                }],
                evidence: vec!["native-boundary".into()],
                input_request: None,
                review,
            },
            session,
        )
        .unwrap();
}

#[test]
fn published_story_delivery_routes_all_judge_outcomes() {
    // catches: the daemon never traversing published yes/no/uncertain edges.
    // Native reports exercise the owned report service; they are not recorded model fixtures.
    for expected in [EdgeOutcome::Yes, EdgeOutcome::No, EdgeOutcome::Uncertain] {
        assert_delivery(expected, false, false);
    }
}

fn assert_delivery(expected: EdgeOutcome, automatic: bool, fail_checks: bool) {
    let (config, project, plan, story, plan_template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let worktree = init_worktree(project.path(), config.path());
    let definitions = WorkflowStore::open().unwrap();
    let story_template = definitions
        .seed_templates(&canonical_owner(project_path))
        .unwrap()
        .into_iter()
        .find(|d| d.kind == WorkflowKind::Story)
        .unwrap();
    let mut graph = story_template.graph;
    for node in &mut graph.nodes {
        if let NodeKind::Pause { resume_to } = &mut node.kind {
            *resume_to = Some("implement".into());
        }
    }
    if fail_checks {
        graph.nodes.push(Node {
            id: "preapproval".into(),
            kind: NodeKind::Gate,
        });
        graph
            .edges
            .iter_mut()
            .find(|e| e.from == "review")
            .unwrap()
            .to = "preapproval".into();
        graph.edges.push(edge("preapproval", "judge", Some("pass")));
        graph.edges.push(edge("preapproval", "pause", Some("fail")));
    }
    let mut published = publish(project_path, graph);
    if fail_checks {
        let draft = definitions.get_draft(&published.id).unwrap();
        let draft = definitions
            .update_checks(
                &draft.id,
                draft.draft_revision,
                vec![CheckDefinition {
                    id: "status".into(),
                    argv: vec!["git".into(), "rev-parse".into(), "missing-ref".into()],
                    timeout_secs: 30,
                }],
            )
            .unwrap();
        published = definitions
            .publish(&draft.id, draft.draft_revision)
            .unwrap();
    }
    // Existing command-driven runs can pin a graph for native runtime policy tests.
    // Root-start controls and automatic approval are intentionally later slices.
    let plan_draft = definitions.get_draft(&plan_template).unwrap();
    let mut graph = plan_draft.graph;
    for node in &mut graph.nodes {
        if let NodeKind::StoryDispatch {
            story_template_id,
            story_revision,
        } = &mut node.kind
        {
            *story_template_id = published.id.clone();
            *story_revision = published.revision;
        }
    }
    let draft = definitions
        .update_draft(&plan_draft.id, plan_draft.draft_revision, graph)
        .unwrap();
    let plan_def = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    if expected == EdgeOutcome::Yes {
        let stories = StoryStore::open().unwrap();
        let native = stories.get_story(&story).unwrap();
        stories
            .transition(&story, native.revision, StoryCommand::StartManual)
            .unwrap();
    }
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let run = store
        .start_plan(
            project_path,
            &plan,
            &plan_template,
            plan_def.revision,
            RunLimits::default(),
        )
        .unwrap();
    store
        .command(
            &run.id,
            "test:graph",
            RunCommand::Graph {
                transition: GraphTransition::Start {
                    execution_id: "story".into(),
                    target_id: story.clone(),
                },
            },
        )
        .unwrap();
    let current = drive_turn(&store, &run.id).unwrap();
    let implement = current.attempts.last().unwrap().clone();
    assert_eq!(implement.node_id, "implement");
    assert_eq!(drive_turn(&store, &run.id).unwrap().attempts.len(), 1);
    store
        .command(
            &run.id,
            "test:worktree",
            RunCommand::AssignWorktree {
                story_id: story.clone(),
                path: worktree.clone(),
            },
        )
        .unwrap();
    bind(&store, &run.id, &implement, "implementer");
    assert!(
        store
            .command(
                &run.id,
                "test:untyped",
                RunCommand::ReportAttempt {
                    attempt_id: implement.id.clone(),
                    generation: implement.generation,
                    outcome: AttemptOutcome::Completed,
                }
            )
            .is_err()
    );
    report(&store, &run.id, &implement, "implementer", None);
    let current = drive_turn(&store, &run.id).unwrap();
    let reviewer = current.attempts.last().unwrap().clone();
    assert_eq!(reviewer.node_id, "review");
    bind(&store, &run.id, &reviewer, "reviewer");
    if expected == EdgeOutcome::Yes {
        // Exercise real operator approval transitions, never hand-set a Done snapshot.
        let stories = StoryStore::open().unwrap();
        let mut native = stories.get_story(&story).unwrap();
        for command in [
            StoryCommand::CheckCriterion(0),
            StoryCommand::SubmitReview,
            StoryCommand::Approve,
        ] {
            if automatic && command == StoryCommand::Approve {
                continue;
            }
            native = stories
                .transition(&story, native.revision, command)
                .unwrap();
        }
    }
    let (commit, tree) =
        super::super::check::clean_artifact(std::path::Path::new(&worktree)).unwrap();
    let current_run = store.snapshot(&run.id).unwrap();
    let current_story = StoryStore::open().unwrap().get_story(&story).unwrap();
    let package = crate::workflows::render_story_prompt(
        &current_run,
        &current_story,
        current_run
            .attempts
            .iter()
            .find(|attempt| attempt.id == reviewer.id)
            .unwrap(),
        &published,
        &[],
        None,
    )
    .unwrap();
    assert!(
        package.prompt.contains(&artifact_digest(&commit, &tree)),
        "review prompt must carry the exact artifact subject"
    );
    let assessment = ReviewAssessment {
        decision: if expected == EdgeOutcome::No {
            ReviewDecision::ChangesRequested
        } else {
            ReviewDecision::Approved
        },
        artifact_digest: if matches!(expected, EdgeOutcome::Uncertain | EdgeOutcome::Fail) {
            "0".repeat(64)
        } else {
            artifact_digest(&commit, &tree)
        },
        findings: if expected == EdgeOutcome::No {
            vec![ReviewFinding {
                criterion_index: 0,
                severity: ReviewSeverity::Major,
                summary: "Repair required".into(),
                evidence: "Current artifact finding".into(),
            }]
        } else {
            vec![]
        },
    };
    report(&store, &run.id, &reviewer, "reviewer", Some(assessment));
    if automatic {
        let current = drive_turn(&store, &run.id).unwrap();
        assert!(!current.graph_executions[0].completed);
        let native = StoryStore::open().unwrap().get_story(&story).unwrap();
        assert_eq!(native.status, crate::stories::StoryStatus::Review);
        store.interrupt_agent_session("implementer").unwrap();
        let error = StoryStore::open()
            .unwrap()
            .transition_for_actor(
                &story,
                native.revision,
                StoryCommand::Approve,
                Some("implementer"),
            )
            .unwrap_err();
        assert!(error.contains("workflow implementer"), "{error}");
        let graph = &current.graph_executions[0];
        let activation = graph.activations.last().unwrap();
        assert!(
            super::super::runtime::policy::drive_policy(&store, &current, graph, activation)
                .unwrap()
        );
        let checked = store.snapshot(&run.id).unwrap();
        assert_eq!(
            StoryStore::open()
                .unwrap()
                .get_story(&story)
                .unwrap()
                .status,
            crate::stories::StoryStatus::Review
        );
        assert_eq!(checked.stories[0].check_receipts.len(), 1);
        if !fail_checks {
            assert!(
                super::super::runtime::policy::drive_policy(
                    &store,
                    &checked,
                    &checked.graph_executions[0],
                    activation
                )
                .unwrap()
            );
            let history = StoryStore::open()
                .unwrap()
                .transition_history(&story)
                .unwrap();
            assert_eq!(
                history.last().unwrap().actor,
                crate::stories::StoryTransitionActor::ManagedSession {
                    session_id: "reviewer".into()
                }
            );
            assert!(
                store.snapshot(&run.id).unwrap().stories[0]
                    .integration_receipt
                    .is_none()
            );
        }
    } else if expected == EdgeOutcome::Yes {
        store
            .command(
                &run.id,
                "test:accept",
                RunCommand::AcceptStory {
                    story_id: story.clone(),
                },
            )
            .unwrap();
        let current = store.snapshot(&run.id).unwrap();
        store
            .execute_check(&run.id, &story, "status", "test:check", current.sequence)
            .unwrap();
    }
    let current = drive_turn(&store, &run.id).unwrap();
    let graph = &current.graph_executions[0];
    let decision = &graph.decisions[0];
    let expected = if fail_checks {
        EdgeOutcome::Fail
    } else {
        expected
    };
    let selected = &graph.definition.graph.edges[decision.edge_index];
    assert_eq!(
        selected.outcome.as_deref(),
        Some(match expected {
            EdgeOutcome::Yes => "yes",
            EdgeOutcome::No => "no",
            EdgeOutcome::Fail => "fail",
            _ => "uncertain",
        })
    );
    assert_eq!(
        graph
            .activations
            .iter()
            .filter(|a| a.node_id == "judge")
            .count(),
        if fail_checks { 0 } else { 1 }
    );
    assert_eq!(
        decision.evidence.actor,
        if matches!(expected, EdgeOutcome::Uncertain | EdgeOutcome::Fail) {
            "daemon"
        } else {
            "reviewer"
        }
    );
    match expected {
        EdgeOutcome::Yes => assert!(graph.completed),
        EdgeOutcome::No => {
            assert_eq!(current.loops, 1);
            assert_eq!(current.attempts.last().unwrap().node_id, "implement");
        }
        _ => {
            assert_eq!(current.status, RunStatus::Paused);
            assert_eq!(graph.pauses[0].resume_to, "implement");
        }
    }
    assert_eq!(store.replay(&run.id).unwrap(), current);
}

#[test]
fn workflow_approval_keeps_report_actor_and_subject_and_denies_exited_implementer() {
    // catches: daemon impersonation, approval before checks, self-approval after exit, or automatic merge.
    assert_delivery(EdgeOutcome::Yes, true, false);
}

#[test]
fn preapproval_failed_check_never_closes_the_story() {
    // catches: approval despite a deterministic failed check receipt.
    assert_delivery(EdgeOutcome::Yes, true, true);
}

#[path = "semantics_controls.rs"]
mod controls;

#[path = "plan_dispatch.rs"]
pub(super) mod plan_dispatch;

#[path = "critic_de.rs"]
mod critic_de;

// Catches: configured event dispatch spawning a plain implementer, losing the isolated
// artifact, or permitting publication with missing/stale native review/check evidence.
#[tokio::test]
async fn event_workflow_creates_native_story_and_reuses_current_review_and_check_authority() {
    let (config, project, _plan, _story, _template, _guard) = fixture();
    let worktree = init_worktree(project.path(), config.path());
    let project_path = canonical_owner(project.path().to_str().unwrap());
    let definitions = WorkflowStore::open().unwrap();
    let template = definitions
        .seed_templates(&project_path)
        .unwrap()
        .into_iter()
        .find(|d| d.kind == WorkflowKind::Story)
        .unwrap();
    let mut graph = template.graph;
    for node in &mut graph.nodes {
        if let NodeKind::Pause { resume_to } = &mut node.kind {
            *resume_to = Some("implement".into());
        }
    }
    let published = publish(&project_path, graph);
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let definition = serde_json::from_value(serde_json::json!({
        "id":"event", "name":"Issue", "prompt":"Resolve the issue", "run_config":"unused-legacy-profile",
        "repository":project_path, "workspace":{"mode":"new_per_run","base_branch":"main"},
        "cron":"0 * * * *", "timezone":"UTC", "enabled":true, "grace_secs":60,
        "overlap":"skip", "max_duration_secs":3600, "precheck":null
    })).unwrap();
    let automation_owner = crate::automations::store::RunOwner::acquire_at(
        &config.path().join("automation.sqlite3"),
        0,
    )
    .unwrap();
    let mut automation = automation_owner
        .store()
        .reserve(&definition, crate::automations::run::RunTrigger::Manual, 1)
        .unwrap()
        .unwrap();
    automation.trigger = crate::automations::run::RunTrigger::Event {
        repository: "owner/repo".into(),
        issue_number: 3,
        label: "run".into(),
    };
    automation.event_workflow = Some(crate::automations::model::EventWorkflow {
        definition_id: published.id.clone(),
        definition_revision: published.revision,
    });
    let binding = crate::automations::event_workflow::start(
        &state,
        &automation,
        &worktree,
        "Resolve issue #3 and preserve the interface",
    )
    .unwrap();
    automation.workflow_binding = Some(binding.clone());
    let store = RunStore::open().unwrap();
    let current = store.snapshot(&binding.run_id).unwrap();
    let native = StoryStore::open()
        .unwrap()
        .get_story(&binding.story_id)
        .unwrap();
    assert_eq!(
        native.origin,
        crate::stories::StoryOrigin::GitHubIssue { number: 3 }
    );
    assert_eq!(
        native.problem.as_deref(),
        Some("Resolve issue #3 and preserve the interface")
    );
    assert!(native.criteria[0].contains("https://github.com/owner/repo/issues/3"));
    assert_eq!(
        current.stories[0].worktree_path.as_deref(),
        Some(worktree.as_str())
    );
    assert_eq!(current.definition_id, published.id);
    assert!(crate::automations::event_workflow::review(&automation).is_err());
    let implement = current.attempts.last().unwrap().clone();
    store
        .begin_graph_story(&current, &binding.story_id, "workflow-implementer")
        .unwrap();
    bind(&store, &current.id, &implement, "implementer");
    report(&store, &current.id, &implement, "implementer", None);
    let current = drive_turn(&store, &current.id).unwrap();
    let reviewer = current.attempts.last().unwrap().clone();
    bind(&store, &current.id, &reviewer, "reviewer");
    let stories = StoryStore::open().unwrap();
    let native = stories.get_story(&binding.story_id).unwrap();
    let native = stories
        .transition(&native.id, native.revision, StoryCommand::CheckCriterion(0))
        .unwrap();
    stories
        .transition(&native.id, native.revision, StoryCommand::SubmitReview)
        .unwrap();
    let (commit, tree) =
        super::super::check::clean_artifact(std::path::Path::new(&worktree)).unwrap();
    report(
        &store,
        &current.id,
        &reviewer,
        "reviewer",
        Some(ReviewAssessment {
            decision: ReviewDecision::Approved,
            artifact_digest: artifact_digest(&commit, &tree),
            findings: vec![],
        }),
    );
    let current = drive_turn(&store, &current.id).unwrap();
    let graph = &current.graph_executions[0];
    let activation = graph.activations.last().unwrap();
    super::super::runtime::policy::drive_policy(&store, &current, graph, activation).unwrap();
    let checked = store.snapshot(&current.id).unwrap();
    super::super::runtime::policy::drive_policy(
        &store,
        &checked,
        &checked.graph_executions[0],
        activation,
    )
    .unwrap();
    let completed = drive_turn(&store, &current.id).unwrap();
    assert!(completed.graph_executions[0].completed);
    assert_eq!(
        completed.status,
        RunStatus::Running,
        "ordinary standalone graph lifecycle is unchanged"
    );
    let observation = crate::automations::event_workflow::observe(&state, &automation);
    assert_eq!(observation.task, Some(crate::tasks::TaskStatus::Completed));
    let completed = store.snapshot(&current.id).unwrap();
    assert_eq!(completed.status, RunStatus::Completed);
    let receipt = crate::automations::event_workflow::review(&automation).unwrap();
    assert_eq!(receipt.commit, commit);
    assert!(receipt.findings.contains("Native current evidence"));
    assert!(receipt.findings.contains("No review findings."));
    assert!(receipt.findings.contains("reviewer"));
    // Missing checks and self-review fail closed at the publication boundary.
    let mut missing = completed.clone();
    missing.stories[0].check_receipts.clear();
    assert!(super::super::publication_review(&missing, &binding.story_id).is_err());
    let mut same_agent = completed.clone();
    same_agent
        .attempts
        .iter_mut()
        .find(|a| a.id == reviewer.id)
        .unwrap()
        .agent
        .as_mut()
        .unwrap()
        .session_id = "implementer".into();
    assert!(super::super::publication_review(&same_agent, &binding.story_id).is_err());
    crate::git_cli::git_cmd(std::path::Path::new(&worktree))
        .args(["commit", "--allow-empty", "-qm", "newer unreviewed commit"])
        .run()
        .unwrap();
    assert!(
        crate::automations::event_workflow::review(&automation)
            .err()
            .unwrap()
            .contains("artifact moved")
    );
}
