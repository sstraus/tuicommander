use super::super::super::runtime::plan;
use super::*;
use crate::stories::{NewStory, StoryOrigin, StoryStatus};

pub(in super::super) fn plan_definition(
    project: &str,
    template: &str,
    story: &crate::workflows::PublishedWorkflow,
) -> crate::workflows::PublishedWorkflow {
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(template).unwrap();
    let mut graph = draft.graph;
    for node in &mut graph.nodes {
        match &mut node.kind {
            NodeKind::StoryDispatch {
                story_template_id,
                story_revision,
            } => {
                *story_template_id = story.id.clone();
                *story_revision = story.revision;
            }
            NodeKind::Pause { resume_to } => *resume_to = Some("create".into()),
            _ => {}
        }
    }
    let draft = definitions
        .update_draft(&draft.id, draft.draft_revision, graph)
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
        panic!("plan publication");
    };
    published
}

fn plan_report(store: &RunStore, run: &RunSnapshot, attempt: &NodeAttempt, session: &str) {
    store
        .report_bound_agent(
            AttemptReport {
                contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
                run_id: run.id.clone(),
                story_id: run.plan_id.clone(),
                story_revision: 0,
                attempt_id: attempt.id.clone(),
                generation: attempt.generation,
                outcome: AttemptOutcome::Completed,
                summary: "Native proposals complete".into(),
                criterion_results: vec![],
                evidence: vec!["native proposal service".into()],
                input_request: None,
                review: None,
            },
            session,
        )
        .unwrap();
}

fn start_plan(
    store: &RunStore,
    project: &str,
    plan_id: &str,
    definition: &crate::workflows::PublishedWorkflow,
    key: &str,
) -> RunSnapshot {
    store
        .start_graph_run(&GraphStartRequest {
            project: project.into(),
            target: RunTarget::Plan(plan_id.into()),
            expected_revision: None,
            definition_id: definition.id.clone(),
            definition_revision: definition.revision,
            request_id: key.into(),
            limits: RunLimits::default(),
        })
        .unwrap()
}

fn current_attempt(store: &RunStore, id: &str, story: &str) -> NodeAttempt {
    drive_turn(store, id)
        .unwrap()
        .attempts
        .iter()
        .rev()
        .find(|a| a.story_id == story && a.state == AttemptState::Running)
        .unwrap()
        .clone()
}

fn finish_child(store: &RunStore, run_id: &str, story_id: &str, worktree: &str, file: &str) {
    let attempt = current_attempt(store, run_id, story_id);
    assert_eq!(attempt.node_id, "implement");
    store
        .begin_graph_story(
            &store.snapshot(run_id).unwrap(),
            story_id,
            "native workflow start",
        )
        .unwrap();
    store
        .command(
            run_id,
            &format!("assign:{story_id}"),
            RunCommand::AssignWorktree {
                story_id: story_id.into(),
                path: worktree.into(),
            },
        )
        .unwrap();
    std::fs::write(
        std::path::Path::new(worktree).join(file),
        format!("{story_id}\n"),
    )
    .unwrap();
    crate::git_cli::git_cmd(std::path::Path::new(worktree))
        .args(["add", file])
        .run()
        .unwrap();
    crate::git_cli::git_cmd(std::path::Path::new(worktree))
        .args(["commit", "-qm", "story artifact"])
        .run()
        .unwrap();
    let worker = format!("implementer:{story_id}");
    bind(store, run_id, &attempt, &worker);
    let stories = StoryStore::open().unwrap();
    let mut story = stories.get_story(story_id).unwrap();
    for command in [StoryCommand::CheckCriterion(0), StoryCommand::SubmitReview] {
        story = stories
            .transition_for_actor(story_id, story.revision, command, Some(&worker))
            .unwrap();
    }
    report(store, run_id, &attempt, &worker, None);
    let review = current_attempt(store, run_id, story_id);
    assert_eq!(review.node_id, "review");
    let reviewer = format!("reviewer:{story_id}");
    bind(store, run_id, &review, &reviewer);
    let (commit, tree) =
        super::super::super::check::clean_artifact(std::path::Path::new(worktree)).unwrap();
    report(
        store,
        run_id,
        &review,
        &reviewer,
        Some(ReviewAssessment {
            decision: ReviewDecision::Approved,
            artifact_digest: artifact_digest(&commit, &tree),
            findings: vec![],
        }),
    );
    for _ in 0..2 {
        let run = drive_turn(store, run_id).unwrap();
        let graph = run
            .graph_executions
            .iter()
            .find(|g| g.target_id == story_id)
            .unwrap();
        assert!(
            super::super::super::runtime::policy::drive_policy(
                store,
                &run,
                graph,
                graph.activations.last().unwrap()
            )
            .unwrap()
        );
    }
    let run = drive_turn(store, run_id).unwrap();
    assert!(
        run.graph_executions
            .iter()
            .find(|g| g.target_id == story_id)
            .unwrap()
            .completed
    );
    assert_eq!(
        stories.get_story(story_id).unwrap().status,
        StoryStatus::Done
    );
    assert!(
        run.stories
            .iter()
            .find(|s| s.story_id == story_id)
            .unwrap()
            .integration_receipt
            .is_none()
    );
}

#[test]
fn resolve_plan_reaches_verified_fixed_point_after_explicit_dependency_integration() {
    // catches: seeded plan checks missing at final verification, proposal duplication, Done releasing dependents or automatic merge.
    let (config, project, plan_id, first, template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let first_tree = init_worktree(project.path(), config.path());
    let definitions = WorkflowStore::open().unwrap();
    let seed = definitions
        .seed_templates(&canonical_owner(project_path))
        .unwrap()
        .into_iter()
        .find(|d| d.kind == WorkflowKind::Story)
        .unwrap();
    let mut graph = seed.graph;
    for node in &mut graph.nodes {
        if let NodeKind::Pause { resume_to } = &mut node.kind {
            *resume_to = Some("implement".into());
        }
    }
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
    let story_definition = publish(project_path, graph);
    let definition = plan_definition(project_path, &template, &story_definition);
    let store = RunStore::open().unwrap();
    let run = start_plan(&store, project_path, &plan_id, &definition, "fixed-point");
    let coordinator = current_attempt(&store, &run.id, &plan_id);
    bind(&store, &run.id, &coordinator, "coordinator");
    let proposal = NewStory {
        story_type: None,
        plan_id: plan_id.clone(),
        title: "Dependent".into(),
        criteria: vec!["Delivered".into()],
        priority: 2,
        origin: StoryOrigin::PlanStep {
            step: "second".into(),
        },
        file_scope: vec!["second.txt".into()],
    };
    let second = store
        .create_story_from_coordinator(&run.id, "coordinator", "second", proposal.clone())
        .unwrap();
    assert_eq!(
        store
            .create_story_from_coordinator(&run.id, "coordinator", "second", proposal)
            .unwrap()
            .id,
        second.id
    );
    StoryStore::open()
        .unwrap()
        .add_dependency(&second.id, &first, second.revision)
        .unwrap();
    plan_report(&store, &run, &coordinator, "coordinator");
    let active = drive_turn(&store, &run.id).unwrap();
    assert!(active.graph_executions.iter().any(|g| g.target_id == first));
    assert!(
        !active
            .graph_executions
            .iter()
            .any(|g| g.target_id == second.id)
    );
    finish_child(&store, &run.id, &first, &first_tree, "first.txt");
    let waiting = drive_turn(&store, &run.id).unwrap();
    assert_eq!(waiting.status, RunStatus::Running);
    assert!(
        !waiting
            .graph_executions
            .iter()
            .any(|g| g.target_id == second.id)
    );
    crate::git_cli::git_cmd(project.path())
        .args([
            "merge",
            "--no-ff",
            "-m",
            "Integrate first story",
            "artifact",
        ])
        .run()
        .unwrap();
    store
        .record_integrated_story(&run.id, &first, "integrate:first", waiting.sequence)
        .unwrap();
    let released = drive_turn(&store, &run.id).unwrap();
    assert!(
        released
            .graph_executions
            .iter()
            .any(|g| g.target_id == second.id)
    );
    let second_tree = config.path().join("second-artifact");
    crate::git_cli::git_cmd(project.path())
        .args([
            "worktree",
            "add",
            "-q",
            "-b",
            "second-artifact",
            second_tree.to_str().unwrap(),
        ])
        .run()
        .unwrap();
    let second_tree = second_tree
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    finish_child(&store, &run.id, &second.id, &second_tree, "second.txt");
    let waiting = drive_turn(&store, &run.id).unwrap();
    crate::git_cli::git_cmd(project.path())
        .args([
            "merge",
            "--no-ff",
            "-m",
            "Integrate second story",
            "second-artifact",
        ])
        .run()
        .unwrap();
    store
        .record_integrated_story(&run.id, &second.id, "integrate:second", waiting.sequence)
        .unwrap();
    let final_gate = drive_turn(&store, &run.id).unwrap();
    assert_eq!(final_gate.status, RunStatus::Running);
    assert!(
        store
            .command(
                &run.id,
                "fabricated-final",
                RunCommand::FinalVerificationPassed
            )
            .is_err()
    );
    for _ in 0..2 {
        let current = store.snapshot(&run.id).unwrap();
        let graph = &current.graph_executions[0];
        assert!(
            plan::drive_checks(&store, &current, graph, graph.activations.last().unwrap()).unwrap()
        );
    }
    let completed = drive_turn(&store, &run.id).unwrap();
    assert_eq!(completed.status, RunStatus::Completed);
    assert!(completed.graph_executions.iter().all(|g| g.completed));
    assert_eq!(store.replay(&run.id).unwrap(), completed);
}

#[test]
fn two_plan_roots_share_dispatch_reservations_and_wontfix_never_releases_dependency() {
    // catches: another plan bypassing an overlapping wave, or WontFix acting as integrated delivery.
    let (config, project, first_plan, first_story, template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    init_worktree(project.path(), config.path());
    let story_definition = definition(project_path, false, true);
    let definition = plan_definition(project_path, &template, &story_definition);
    let stories = StoryStore::open().unwrap();
    let second_plan = stories
        .create_plan(crate::stories::NewPlan {
            project: canonical_owner(project_path),
            title: "Second plan".into(),
            source: "second-plan.md".into(),
        })
        .unwrap();
    let second_story = stories
        .create_story(NewStory {
            story_type: None,
            plan_id: second_plan.id.clone(),
            title: "Second story".into(),
            criteria: vec!["Delivered".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec!["second.txt".into()],
        })
        .unwrap();
    let store = RunStore::open().unwrap();
    let first_run = start_plan(&store, project_path, &first_plan, &definition, "first-plan");
    let first_agent = current_attempt(&store, &first_run.id, &first_plan);
    bind(&store, &first_run.id, &first_agent, "first-coordinator");
    plan_report(&store, &first_run, &first_agent, "first-coordinator");
    let first_active = drive_turn(&store, &first_run.id).unwrap();
    assert!(
        first_active
            .graph_executions
            .iter()
            .any(|g| g.target_id == first_story)
    );
    let second_run = start_plan(
        &store,
        project_path,
        &second_plan.id,
        &definition,
        "second-plan",
    );
    let second_agent = current_attempt(&store, &second_run.id, &second_plan.id);
    bind(&store, &second_run.id, &second_agent, "second-coordinator");
    plan_report(&store, &second_run, &second_agent, "second-coordinator");
    let second_waiting = drive_turn(&store, &second_run.id).unwrap();
    assert_eq!(second_waiting.status, RunStatus::Running);
    assert!(
        !second_waiting
            .graph_executions
            .iter()
            .any(|g| g.target_id == second_story.id)
    );
    store
        .command(&first_run.id, "cancel-first-wave", RunCommand::Cancel)
        .unwrap();
    let second_active = drive_turn(&store, &second_run.id).unwrap();
    assert!(
        second_active
            .graph_executions
            .iter()
            .any(|g| g.target_id == second_story.id)
    );
    store
        .command(&second_run.id, "cancel-second-wave", RunCommand::Cancel)
        .unwrap();

    let cancelled = stories
        .transition(
            &first_story,
            stories.get_story(&first_story).unwrap().revision,
            StoryCommand::WontFix,
        )
        .unwrap();
    let dependent = stories
        .create_story(NewStory {
            story_type: None,
            plan_id: first_plan.clone(),
            title: "Blocked dependent".into(),
            criteria: vec!["Delivered".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec!["dependent.txt".into()],
        })
        .unwrap();
    stories
        .add_dependency(&dependent.id, &cancelled.id, dependent.revision)
        .unwrap();
    let blocked_run = start_plan(
        &store,
        project_path,
        &first_plan,
        &definition,
        "wontfix-plan",
    );
    let agent = current_attempt(&store, &blocked_run.id, &first_plan);
    bind(&store, &blocked_run.id, &agent, "blocked-coordinator");
    plan_report(&store, &blocked_run, &agent, "blocked-coordinator");
    let blocked = drive_turn(&store, &blocked_run.id).unwrap();
    assert_eq!(blocked.status, RunStatus::Paused);
    assert!(
        !blocked
            .graph_executions
            .iter()
            .any(|g| g.target_id == dependent.id)
    );
    let decision = &blocked.graph_executions[0].decisions[0];
    assert_eq!(
        blocked.graph_executions[0].definition.graph.edges[decision.edge_index]
            .outcome
            .as_deref(),
        Some("blocked")
    );
}

#[test]
fn failed_final_plan_gate_replans_through_a_fresh_bound_coordinator() {
    // catches: replan reusing an exited coordinator or failed final checks certifying completion.
    let (config, project, _plan, _story, template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    init_worktree(project.path(), config.path());
    let story_definition = definition(project_path, false, true);
    let published = plan_definition(project_path, &template, &story_definition);
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(&published.id).unwrap();
    let draft = definitions
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "fails".into(),
                argv: vec!["git".into(), "rev-parse".into(), "missing-ref".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    let published = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    let stories = StoryStore::open().unwrap();
    let empty_plan = stories
        .create_plan(crate::stories::NewPlan {
            project: canonical_owner(project_path),
            title: "Empty plan".into(),
            source: "empty-plan.md".into(),
        })
        .unwrap();
    let store = RunStore::open().unwrap();
    let run = start_plan(&store, project_path, &empty_plan.id, &published, "replan");
    let coordinator = current_attempt(&store, &run.id, &empty_plan.id);
    bind(&store, &run.id, &coordinator, "original-coordinator");
    plan_report(&store, &run, &coordinator, "original-coordinator");
    let final_gate = drive_turn(&store, &run.id).unwrap();
    let graph = &final_gate.graph_executions[0];
    assert!(
        plan::drive_checks(
            &store,
            &final_gate,
            graph,
            graph.activations.last().unwrap()
        )
        .unwrap()
    );
    let replanning = drive_turn(&store, &run.id).unwrap();
    assert_eq!(replanning.status, RunStatus::Running);
    assert!(replanning.verification_fingerprint.is_none());
    let fresh = replanning.attempts.last().unwrap();
    assert_eq!(fresh.node_id, "coordinate");
    assert_eq!(fresh.generation, 2);
    assert_ne!(fresh.id, coordinator.id);
    bind(&store, &run.id, fresh, "new-coordinator");
    let created = store
        .create_story_from_coordinator(
            &run.id,
            "new-coordinator",
            "repair",
            NewStory {
                story_type: None,
                plan_id: empty_plan.id.clone(),
                title: "Repair final check".into(),
                criteria: vec!["Fixed".into()],
                priority: 1,
                origin: StoryOrigin::PlanStep {
                    step: "repair".into(),
                },
                file_scope: vec!["repair.txt".into()],
            },
        )
        .unwrap();
    plan_report(&store, &run, fresh, "new-coordinator");
    let dispatched = drive_turn(&store, &run.id).unwrap();
    assert!(
        dispatched
            .graph_executions
            .iter()
            .any(|g| g.target_id == created.id)
    );
    assert_eq!(store.replay(&run.id).unwrap(), dispatched);
}

#[test]
fn disjoint_story_children_share_one_bounded_dispatch_wave() {
    // catches: unconditional serialization of disjoint stories or dispatch beyond the run cap.
    let (config, project, _plan, _story, template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    init_worktree(project.path(), config.path());
    let story_definition = definition(project_path, false, true);
    let definition = plan_definition(project_path, &template, &story_definition);
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(crate::stories::NewPlan {
            project: canonical_owner(project_path),
            title: "Wave".into(),
            source: "wave.md".into(),
        })
        .unwrap();
    let mut ids = Vec::new();
    for path in ["a.txt", "b.txt", "c.txt"] {
        ids.push(
            stories
                .create_story(NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: path.into(),
                    criteria: vec!["Delivered".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![path.into()],
                })
                .unwrap()
                .id,
        );
    }
    let store = RunStore::open().unwrap();
    let run = start_plan(&store, project_path, &plan.id, &definition, "wave");
    let coordinator = current_attempt(&store, &run.id, &plan.id);
    bind(&store, &run.id, &coordinator, "wave-coordinator");
    plan_report(&store, &run, &coordinator, "wave-coordinator");
    let active = drive_turn(&store, &run.id).unwrap();
    assert_eq!(
        active
            .graph_executions
            .iter()
            .filter(|g| ids.contains(&g.target_id))
            .count(),
        2
    );
    assert_eq!(
        active
            .attempts
            .iter()
            .filter(|a| ids.contains(&a.story_id) && a.state == AttemptState::Running)
            .count(),
        2
    );
    assert_eq!(store.replay(&run.id).unwrap(), active);
}
