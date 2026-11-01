use super::*;
use serde_json::json;

fn fork_graph(reviewers: bool) -> WorkflowGraph {
    let branch = |id: &str| {
        if reviewers {
            json!({"id":id,"kind":{"type":"agent","role":"reviewer","capabilities":[],"prompt_template":"Review independently"}})
        } else {
            json!({"id":id,"kind":{"type":"gate"}})
        }
    };
    let mut edges = vec![
        json!({"from":"start","to":"implement","outcome":null}),
        json!({"from":"implement","to":"fork","outcome":null}),
        json!({"from":"join","to":"post","outcome":"completed"}),
        json!({"from":"join","to":"end","outcome":"failed"}),
        json!({"from":"join","to":"end","outcome":"cancelled"}),
        json!({"from":"join","to":"end","outcome":"skipped"}),
        json!({"from":"post","to":"end","outcome":"pass"}),
        json!({"from":"post","to":"end","outcome":"fail"}),
    ];
    for id in ["critic", "advisor"] {
        for outcome in if reviewers {
            vec![None]
        } else {
            vec![Some("pass"), Some("fail")]
        } {
            edges.push(json!({"from":id,"to":"join","outcome":outcome}));
        }
    }
    serde_json::from_value(json!({"nodes":[
        {"id":"start","kind":{"type":"start"}},
        {"id":"implement","kind":{"type":"agent","role":"implementer","capabilities":[],"prompt_template":"Implement criteria"}},
        {"id":"fork","kind":{"type":"fork","join":"join","repair_to":"implement","branches":[
            {"id":"critic","target":"critic","required":true},{"id":"advisor","target":"advisor","required":false}]}},
        branch("critic"),branch("advisor"),
        {"id":"join","kind":{"type":"all_join","fork":"fork"}},
        {"id":"post","kind":{"type":"gate"}},
        {"id":"end","kind":{"type":"end"}}
    ],"edges":edges})).unwrap()
}

fn prepare_fork(
    config: &std::path::Path,
    project: &std::path::Path,
    story: &str,
    reviewers: bool,
) -> (RunStore, RunSnapshot, String) {
    prepare_graph(config, project, story, fork_graph(reviewers))
}

fn prepare_graph(
    config: &std::path::Path,
    project: &std::path::Path,
    story: &str,
    graph: WorkflowGraph,
) -> (RunStore, RunSnapshot, String) {
    let worktree = init_worktree(project, config);
    let published = publish(project.to_str().unwrap(), graph);
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&request(
            project.to_str().unwrap(),
            story,
            &published,
            "wf02-critic",
        ))
        .unwrap();
    let current = drive_turn(&store, &run.id).unwrap();
    let implementer = current.attempts.last().unwrap();
    store
        .command(
            &run.id,
            "artifact",
            RunCommand::AssignWorktree {
                story_id: story.into(),
                path: worktree.clone(),
            },
        )
        .unwrap();
    bind(&store, &run.id, implementer, "implementer");
    report(&store, &run.id, implementer, "implementer", None);
    let current = drive_turn(&store, &run.id).unwrap();
    (store, current, worktree)
}

#[test]
fn wf02_post_join_gate_consumes_current_generation_checks() {
    // catches F1: a post-join Gate asks for nonexistent activation-local receipts.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let (store, run, _worktree) = prepare_fork(config.path(), project.path(), &story, false);
    for node in ["critic", "advisor"] {
        let current = store.snapshot(&run.id).unwrap();
        let graph = &current.graph_executions[0];
        let activation = graph
            .activations
            .iter()
            .find(|a| a.node_id == node)
            .unwrap();
        assert!(
            super::super::super::runtime::policy::drive_policy(&store, &current, graph, activation)
                .unwrap()
        );
    }
    let current = drive_turn(&store, &run.id).unwrap();
    assert!(
        current.graph_executions[0].completed,
        "post-join Gate stranded despite both current checks passing"
    );
    assert_eq!(current.stories[0].check_receipts.len(), 2);
}

#[test]
fn wf02_primary_critic_fix_is_not_masked_by_later_reviewer() {
    // catches F2: the advisor's missing/approving report masks a bound primary critic FIX.
    for advisor_decision in [
        None,
        Some(ReviewDecision::Approved),
        Some(ReviewDecision::ChangesRequested),
    ] {
        let (config, project, _plan, story, _template, _guard) = fixture();
        let (store, run, worktree) = prepare_fork(config.path(), project.path(), &story, true);
        let (commit, tree) =
            super::super::super::check::clean_artifact(std::path::Path::new(&worktree)).unwrap();
        for node in ["critic", "advisor"] {
            let attempt = run.attempts.iter().find(|a| a.node_id == node).unwrap();
            bind(&store, &run.id, attempt, node);
            let decision = if node == "critic" {
                Some(ReviewDecision::ChangesRequested)
            } else {
                advisor_decision
            };
            if let Some(decision) = decision {
                report(
                    &store,
                    &run.id,
                    attempt,
                    node,
                    Some(ReviewAssessment {
                        evidence: None,
                        decision,
                        artifact_digest: artifact_digest(&commit, &tree),
                        findings: if decision == ReviewDecision::ChangesRequested {
                            vec![ReviewFinding {
                                criterion_index: 0,
                                severity: ReviewSeverity::Major,
                                summary: "Repair required".into(),
                                evidence: "Current artifact defect".into(),
                            }]
                        } else {
                            vec![]
                        },
                    }),
                );
            }
        }
        let current = drive_turn(&store, &run.id).unwrap();
        assert_eq!(
            current.graph_executions[0].forks[0].state,
            super::super::super::parallel::ForkState::Cancelling,
            "advisor {advisor_decision:?} masked FIX"
        );
        assert!(
            !current.graph_executions[0]
                .activations
                .iter()
                .any(|a| a.node_id == "join")
        );
    }
}

#[test]
fn wf02_typed_fix_supersedes_even_when_legacy_label_approves() {
    // catches: the legacy review label masks the pinned policy's authoritative FIX.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let mut graph = fork_graph(true);
    graph.evidence_policy = Some(Default::default());
    let (store, run, worktree) = prepare_graph(config.path(), project.path(), &story, graph);
    let critic = run.attempts.iter().find(|a| a.node_id == "critic").unwrap();
    bind(&store, &run.id, critic, "critic");
    let (commit, tree) =
        super::super::super::check::clean_artifact(std::path::Path::new(&worktree)).unwrap();
    let native_story = StoryStore::open().unwrap().get_story(&story).unwrap();
    let assessment = serde_json::from_value(json!({
        "decision":"approved", "artifactDigest":artifact_digest(&commit, &tree), "findings":[],
        "evidence":{
            "subject":{"type":"story","id":story,"commit":commit,"tree":tree,
                "requirements":super::super::super::evidence::requirements_digest(&native_story)},
            "findings":[{"id":"required-repair","risk":4,"value":4,"effort":1,
                "addsMechanism":false,"deletesOnly":false,"scope":[],"acceptedRisk":null,"proposal":null}],
            "resolvedFixes":[],
            "proof":{"realityEffect":"repair the defect","realityState":"native source",
                "realityAssumptions":"none","realityRadius":"story","symptom":null,"red":null,
                "testChanges":[],"checks":[],"omissions":[]}
        }
    })).unwrap();
    report(&store, &run.id, critic, "critic", Some(assessment));
    let current = drive_turn(&store, &run.id).unwrap();
    assert_eq!(
        current.graph_executions[0].forks[0].state,
        super::super::super::parallel::ForkState::Cancelling,
        "typed FIX was masked by the legacy approval label"
    );
    assert_eq!(store.replay(&run.id).unwrap(), current);
}

#[tokio::test]
async fn wf02_sibling_progress_does_not_pause_async_check() {
    // catches F4: ordinary sibling cursor movement is escalated to policy failure.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let (store, run, _worktree) = prepare_fork(config.path(), project.path(), &story, false);
    let app = state(config.path());
    assert!(drive_effect(&app, &store, &run).await.unwrap());
    let claimed = store.snapshot(&run.id).unwrap();
    let graph = &claimed.graph_executions[0];
    let checked = graph
        .activations
        .iter()
        .find(|a| a.resource_claim.is_some())
        .unwrap()
        .id
        .clone();
    let sibling = graph
        .activations
        .iter()
        .find(|a| a.node_id == "advisor")
        .unwrap();
    // The spawned async worker cannot start on this current-thread runtime until we yield.
    store
        .command(
            &run.id,
            "sibling-progress",
            RunCommand::Graph {
                transition: GraphTransition::ResourceWait {
                    execution_id: graph.id.clone(),
                    activation_id: sibling.id.clone(),
                    reason: Some("Sibling progress".into()),
                },
            },
        )
        .unwrap();
    let settled = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let current = store.snapshot(&run.id).unwrap();
            if current.graph_executions[0]
                .activations
                .iter()
                .find(|a| a.id == checked)
                .unwrap()
                .resource_claim
                .is_none()
            {
                break current;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("check worker did not release its claim");
    assert_eq!(
        settled.status,
        RunStatus::Running,
        "sibling progress paused the run"
    );
    assert_eq!(store.replay(&run.id).unwrap(), settled);
    // The same actor path can re-evaluate the released claim without retry state.
    let graph = &settled.graph_executions[0];
    let activation = graph.activations.iter().find(|a| a.id == checked).unwrap();
    assert!(
        super::super::super::runtime::policy::drive_policy(&store, &settled, graph, activation)
            .unwrap()
    );
    assert_eq!(
        store.snapshot(&run.id).unwrap().stories[0]
            .check_receipts
            .len(),
        1
    );
}

#[test]
fn wf02_empty_foreach_post_gate_records_check_once() {
    // catches: an empty successful region hides checks recorded by its post-region Gate.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let mut graph = fork_graph(false);
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "fork")
        .unwrap()
        .kind = NodeKind::ForEach {
        join: "join".into(),
        body: "critic".into(),
        items: vec![],
        receipt_evidence: None,
        max_items: 2,
        empty: crate::workflows::definition::EmptyFanOut::Complete,
    };
    graph.nodes.retain(|n| n.id != "advisor");
    graph.edges.retain(|e| e.from != "advisor");
    let (store, current, _worktree) = prepare_graph(config.path(), project.path(), &story, graph);
    let graph = &current.graph_executions[0];
    let post = graph
        .activations
        .iter()
        .find(|a| a.node_id == "post")
        .unwrap();
    assert!(
        super::super::super::runtime::policy::drive_policy(&store, &current, graph, post).unwrap()
    );
    let checked = store.snapshot(&current.id).unwrap();
    assert!(
        !super::super::super::runtime::policy::drive_policy(
            &store,
            &checked,
            &checked.graph_executions[0],
            post
        )
        .unwrap()
    );
    let completed = drive_turn(&store, &current.id).unwrap();
    assert!(completed.graph_executions[0].completed);
    assert_eq!(completed.stories[0].check_receipts.len(), 1);
}

#[tokio::test]
async fn wf02_unknown_old_daemon_check_keeps_its_claim_until_stop_proof() {
    // catches N2: a reopened daemon starts repair without proof of an old check's stop.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let (store, run, _worktree) = prepare_fork(config.path(), project.path(), &story, false);
    let graph = &run.graph_executions[0];
    let critic = graph
        .activations
        .iter()
        .find(|a| a.node_id == "critic")
        .unwrap();
    store
        .command(
            &run.id,
            "old-daemon-claim",
            RunCommand::Graph {
                transition: GraphTransition::ResourceClaim {
                    execution_id: graph.id.clone(),
                    activation_id: critic.id.clone(),
                    key: Some("check:previous-daemon".into()),
                },
            },
        )
        .unwrap();
    let fork = &graph.forks[0];
    store
        .command(
            &run.id,
            "old-daemon-supersede",
            RunCommand::Graph {
                transition: GraphTransition::SupersedeFork {
                    execution_id: graph.id.clone(),
                    fork_id: fork.id.clone(),
                    generation: fork.generation,
                },
            },
        )
        .unwrap();
    let reopened = RunStore::open().unwrap();
    let cancelling = reopened.snapshot(&run.id).unwrap();
    let error = drive_effect(&state(config.path()), &reopened, &cancelling)
        .await
        .unwrap_err();
    assert!(error.contains("stop outcome is unknown"), "{error}");
    let fenced = drive_turn(&reopened, &run.id).unwrap();
    assert_eq!(
        fenced.graph_executions[0].forks[0].state,
        super::super::super::parallel::ForkState::Cancelling
    );
    assert_eq!(
        fenced.graph_executions[0]
            .activations
            .iter()
            .filter(|a| a.node_id == "implement")
            .count(),
        1
    );
    assert_eq!(
        fenced.graph_executions[0]
            .activations
            .iter()
            .find(|a| a.id == critic.id)
            .unwrap()
            .resource_claim
            .as_deref(),
        Some("check:previous-daemon")
    );
    assert_eq!(reopened.replay(&run.id).unwrap(), fenced);
}

// Catches M1: FIX kills only PTYs, leaving genuine ACP reviewers alive and repair fenced.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wf04_acp_fork_fix_stops_processes_before_one_repair() {
    let (config, project, _plan, story, _template, _guard) = fixture();
    super::super::acp_close::recorded_worker(config.path(), project.path(), AgentRole::Reviewer);
    let app = state(config.path());
    WorkflowRuntime::spawn(&app);
    owner_ready(&app).await;
    let (store, run, worktree) = prepare_fork(config.path(), project.path(), &story, true);
    let mut hints = app.event_bus.subscribe();
    app.workflow_runtime.wake(&run.id);
    let bound = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let current = store.snapshot(&run.id).unwrap();
            if current
                .attempts
                .iter()
                .filter(|a| a.agent.as_ref().is_some_and(|b| b.acp.is_some()))
                .count()
                == 2
            {
                break current;
            }
            assert_eq!(
                current.status,
                RunStatus::Running,
                "{:?}",
                current.pause_cause
            );
            hints.recv().await.unwrap();
        }
    })
    .await
    .expect("reviewer launch setup did not finish");
    let bindings: Vec<_> = bound
        .attempts
        .iter()
        .filter_map(|a| a.agent.as_ref())
        .filter(|b| b.acp.is_some())
        .cloned()
        .collect();
    let processes = crate::pty::process_tree_snapshot().unwrap();
    let pids: Vec<_> = bindings
        .iter()
        .map(|b| crate::pty::workflow_acp_process(&b.session_id, &processes).unwrap())
        .collect();
    for binding in &bindings {
        let mut stream = app
            .acp
            .subscribe(binding.acp.as_ref().unwrap().connection_id, 1)
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            while let Some(frame) = stream.recv().await {
                if matches!(
                    frame.unwrap().event,
                    crate::acp::AcpClientEvent::TurnSettled { .. }
                ) {
                    return;
                }
            }
            panic!("recorded reviewer turn did not settle");
        })
        .await
        .expect("reviewer turn setup did not finish");
    }
    // Catches: a stale binding stopping a different generation, or a restarted
    // manager treating missing connection inventory as proof of a stopped child.
    let mut stale = bindings[0].clone();
    stale.acp.as_mut().unwrap().generation += 1;
    let stale_result =
        crate::mcp_http::launch_transaction::stop_superseded_workflow_agent(&app, &run.id, &stale)
            .await;
    assert!(
        stale_result
            .unwrap_err()
            .contains("differs from the owned managed launch")
    );
    let restarted = state(config.path());
    let unknown = crate::mcp_http::launch_transaction::stop_superseded_workflow_agent(
        &restarted,
        &run.id,
        &bindings[0],
    )
    .await;
    assert!(unknown.unwrap_err().contains("inventory is unknown"));
    assert!(bindings.iter().all(|b| {
        app.acp
            .snapshot(b.acp.as_ref().unwrap().connection_id)
            .unwrap()
            .settlement
            .is_none()
    }));
    let critic = bound
        .attempts
        .iter()
        .find(|a| a.node_id == "critic")
        .unwrap();
    let binding = critic.agent.as_ref().unwrap();
    let (commit, tree) =
        super::super::super::check::clean_artifact(std::path::Path::new(&worktree)).unwrap();
    store
        .report_acp_agent(
            AttemptReport {
                contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
                run_id: run.id.clone(),
                story_id: story.clone(),
                story_revision: StoryStore::open()
                    .unwrap()
                    .get_story(&story)
                    .unwrap()
                    .revision,
                attempt_id: critic.id.clone(),
                generation: critic.generation,
                outcome: AttemptOutcome::Completed,
                summary: "Recorded ACP reviewer requests repair".into(),
                criterion_results: vec![],
                evidence: vec![],
                input_request: None,
                review: Some(ReviewAssessment {
                    evidence: None,
                    decision: ReviewDecision::ChangesRequested,
                    artifact_digest: artifact_digest(&commit, &tree),
                    findings: vec![ReviewFinding {
                        criterion_index: 0,
                        severity: ReviewSeverity::Major,
                        summary: "Repair required".into(),
                        evidence: "Current artifact defect".into(),
                    }],
                }),
            },
            &binding.session_id,
            binding.acp.clone().unwrap(),
        )
        .unwrap();
    // Catches: direct turn/effect calls racing the spawned actor's sequence.
    // The actor alone drives FIX, stop and repair; observe its durable binding.
    app.workflow_runtime.wake(&run.id);
    let repair_bound = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let current = store.snapshot(&run.id).unwrap();
            if current.attempts.iter().rev().any(|attempt| {
                attempt.node_id == "implement"
                    && attempt
                        .agent
                        .as_ref()
                        .is_some_and(|binding| binding.acp.is_some())
            }) {
                break current;
            }
            assert_eq!(
                current.status,
                RunStatus::Running,
                "{:?}",
                current.pause_cause
            );
            hints.recv().await.unwrap();
        }
    })
    .await
    .expect("repair launch setup did not finish");
    let observed = crate::pty::process_tree_snapshot().unwrap();
    assert!(
        pids.iter()
            .all(|pid| !crate::pty::workflow_process_observed(&observed, *pid)),
        "repair was released while a superseded ACP process remained live"
    );
    assert_eq!(
        repair_bound.graph_executions[0]
            .activations
            .iter()
            .filter(|a| a.node_id == "implement")
            .count(),
        2
    );
    // Catches: a retry requiring the already-stopped child to remain live.
    for binding in &bindings {
        crate::mcp_http::launch_transaction::stop_superseded_workflow_agent(&app, &run.id, binding)
            .await
            .unwrap();
    }
    let repair = repair_bound
        .attempts
        .iter()
        .rev()
        .find(|a| a.node_id == "implement")
        .unwrap();
    let binding = repair.agent.as_ref().unwrap();
    let mut stream = app
        .acp
        .subscribe(binding.acp.as_ref().unwrap().connection_id, 1)
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        while let Some(frame) = stream.recv().await {
            if matches!(
                frame.unwrap().event,
                crate::acp::AcpClientEvent::TurnSettled { .. }
            ) {
                return;
            }
        }
        panic!("repair fixture ended before its recorded turn settled");
    })
    .await
    .expect("repair turn setup did not finish");
    let cancelled = store
        .command(&run.id, "fixture-cleanup", RunCommand::Cancel)
        .unwrap();
    app.acp.shutdown_all().await;
    // A running actor may advance between read and replay; terminal Cancel cannot.
    assert_eq!(store.replay(&run.id).unwrap(), cancelled.snapshot);
    assert!(
        app.session_maps.sessions.is_empty(),
        "ACP must not fabricate a terminal"
    );
    assert!(
        app.session_maps.exit_codes.is_empty(),
        "ACP must not fabricate a PTY tombstone"
    );
}

// Catches: fixture cleanup assuming Cancel can commit a stale preflight snapshot,
// or a conflict partially cancelling the run instead of preserving the other writer.
#[test]
fn wf04_fixture_cancel_preserves_preflight_revision_fences() {
    for competing_writer in ["none", "run", "story"] {
        let (config, project, plan, story, definition, _guard) = fixture();
        let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
        let run = store
            .start_plan(
                &project.path().canonicalize().unwrap().to_string_lossy(),
                &plan,
                &definition,
                1,
                RunLimits::default(),
            )
            .unwrap();
        let racing_store = store.clone();
        let racing_id = run.id.clone();
        super::super::store::BEFORE_COMMIT.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || match competing_writer {
                "run" => {
                    racing_store
                        .command(&racing_id, "other-writer", RunCommand::Pause)
                        .unwrap();
                }
                "story" => {
                    let stories = StoryStore::open().unwrap();
                    let current = stories.get_story(&story).unwrap();
                    stories
                        .transition(&story, current.revision, StoryCommand::StartManual)
                        .unwrap();
                }
                _ => {}
            }));
        });
        let result = store.command(&run.id, "fixture-cleanup", RunCommand::Cancel);
        let current = store.snapshot(&run.id).unwrap();
        if competing_writer == "none" {
            assert_eq!(result.unwrap().snapshot.status, RunStatus::Cancelled);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .contains("revisions changed during command preflight; retry")
            );
            assert_ne!(current.status, RunStatus::Cancelled);
            assert_eq!(
                current.sequence,
                run.sequence + i64::from(competing_writer == "run")
            );
        }
        assert_eq!(store.replay(&run.id).unwrap(), current);
    }
}
