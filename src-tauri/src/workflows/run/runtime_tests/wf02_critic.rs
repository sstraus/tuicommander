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
