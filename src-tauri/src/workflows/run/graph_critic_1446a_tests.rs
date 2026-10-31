//! Critic tests for 1446 slice A: pause fences, Pause targets, legacy run migration.
use super::graph::*;
use super::reducer::apply_event;
use super::*;
use crate::workflows::{
    Edge, Node, NodeKind, PublishedWorkflow, WorkflowClosure, WorkflowGraph, WorkflowKind,
};

fn node(id: &str, kind: NodeKind) -> Node {
    Node {
        id: id.into(),
        kind,
    }
}

fn edge(from: &str, to: &str, outcome: Option<&str>) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        outcome: outcome.map(Into::into),
    }
}

fn evidence() -> DecisionEvidence {
    DecisionEvidence {
        actor: "s".into(),
        reason: "r".into(),
        references: vec!["x".into()],
    }
}

fn event(snapshot: &RunSnapshot, kind: RunEventKind) -> RunEvent {
    RunEvent {
        sequence: snapshot.sequence + 1,
        command_id: format!("c{}", snapshot.sequence + 1),
        command_hash: None,
        at_ms: 0,
        kind,
    }
}

fn step(snapshot: RunSnapshot, kind: RunEventKind) -> RunSnapshot {
    let e = event(&snapshot, kind);
    apply_event(Some(snapshot), &e).expect("event applies")
}

fn graph_step(snapshot: RunSnapshot, transition: GraphTransition) -> RunSnapshot {
    step(
        snapshot,
        RunEventKind::Graph {
            event: GraphEvent::Transition { transition },
        },
    )
}

fn pause_graph() -> WorkflowGraph {
    WorkflowGraph {
        evidence_policy: None,
        nodes: vec![
            node("start", NodeKind::Start),
            node("judge", NodeKind::Judge),
            node(
                "pause",
                NodeKind::Pause {
                    resume_to: Some("end".into()),
                },
            ),
            node("end", NodeKind::End),
        ],
        edges: vec![
            edge("start", "judge", None),
            edge("judge", "end", Some("yes")),
            edge("judge", "end", Some("no")),
            edge("judge", "pause", Some("uncertain")),
        ],
    }
}

/// Catches: ResolvePause forcing status Running although an unresolved uncertain
/// effect still holds the run paused (Resume refuses in that state).
#[test]
fn resolve_pause_keeps_the_uncertain_effect_fence() {
    let (config, project, plan_id, _story, definition_id, _guard) = super::tests::fixture();
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
    let definition = PublishedWorkflow {
        id: run.definition_id.clone(),
        project: run.project.clone(),
        name: "g".into(),
        kind: WorkflowKind::Story,
        closure: WorkflowClosure::Human,
        required_checks: vec![crate::workflows::CheckDefinition {
            id: "c".into(),
            argv: vec!["true".into()],
            timeout_secs: 5,
        }],
        graph: pause_graph(),
        revision: run.definition_revision,
    };
    let execution = GraphExecution::start("g1".into(), run.plan_id.clone(), definition).unwrap();
    let mut s = step(
        run,
        RunEventKind::Graph {
            event: GraphEvent::Started {
                execution: Box::new(execution),
            },
        },
    );
    let t = |a: &str| (("g1".to_string()), a.to_string());
    let (e, a) = t("a0");
    s = graph_step(
        s,
        GraphTransition::Activate {
            execution_id: e.clone(),
            activation_id: a.clone(),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Complete {
            execution_id: e.clone(),
            activation_id: a,
            outcome: None,
            evidence: None,
        },
    );
    s = graph_step(
        s,
        GraphTransition::Activate {
            execution_id: e.clone(),
            activation_id: "a1".into(),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Complete {
            execution_id: e.clone(),
            activation_id: "a1".into(),
            outcome: Some(EdgeOutcome::Uncertain),
            evidence: Some(evidence()),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Activate {
            execution_id: e.clone(),
            activation_id: "a2".into(),
        },
    );
    s = graph_step(
        s,
        GraphTransition::Complete {
            execution_id: e.clone(),
            activation_id: "a2".into(),
            outcome: None,
            evidence: Some(evidence()),
        },
    );
    assert_eq!(s.status, RunStatus::Paused);
    s = step(
        s,
        RunEventKind::EffectReserved {
            effect: EffectIntent {
                id: "e1".into(),
                key: "k".into(),
                kind: EffectKind::SpawnAgent,
                state: EffectState::Intended,
            },
        },
    );
    s = step(
        s,
        RunEventKind::EffectChanged {
            effect_id: "e1".into(),
            state: EffectState::Uncertain,
        },
    );
    let resolution = GraphTransition::ResolvePause {
        execution_id: e,
        activation_id: "a2".into(),
        resolution: "go".into(),
    };
    let rejected = event(
        &s,
        RunEventKind::Graph {
            event: GraphEvent::Transition {
                transition: resolution.clone(),
            },
        },
    );
    assert!(
        apply_event(Some(s.clone()), &rejected)
            .unwrap_err()
            .contains("uncertain effects")
    );
    // Answered input still cannot override an uncertain effect.
    s.attempts.push(NodeAttempt {
        continuation: None,
        graph_binding: None,
        id: "input".into(),
        story_id: s.plan_id.clone(),
        node_id: "judge".into(),
        generation: 1,
        state: AttemptState::Reported,
        outcome: Some(AttemptOutcome::NeedsInput),
        agent: None,
        report: None,
        input_answer: Some("answer".into()),
    });
    assert!(
        apply_event(Some(s.clone()), &rejected)
            .unwrap_err()
            .contains("uncertain effects")
    );
    // This phase asserts retained run-wide input blocking, not contract-5 branch input.
    // The uncertain-effect assertions above continue to use the current contract.
    s.event_contract_version = 4;
    s.graph_executions[0].contract.as_mut().unwrap().version = 4;
    s.effects[0].state = EffectState::Succeeded;
    s.attempts[0].input_answer = None;
    assert!(
        apply_event(Some(s.clone()), &rejected)
            .unwrap_err()
            .contains("human input")
    );
    s.attempts[0].input_answer = Some("answer".into());
    let resumed = apply_event(Some(s), &rejected).unwrap();
    assert_eq!(resumed.status, RunStatus::Running);
    assert!(resumed.graph_executions[0].pauses[0].resolution.is_some());
}

fn published_graph_run() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    RunStore,
    RunSnapshot,
    String,
    impl Drop,
) {
    use crate::workflows::{NodeKind as K, WorkflowStore};
    let (config, project, plan_id, story_id, definition_id, guard) = super::tests::fixture();
    let definitions = WorkflowStore::open().unwrap();
    let plan = definitions.get_draft(&definition_id).unwrap();
    let (story_definition_id, _) = plan
        .graph
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            K::StoryDispatch {
                story_template_id,
                story_revision,
            } => Some((story_template_id.clone(), *story_revision)),
            _ => None,
        })
        .unwrap();
    let mut story = definitions.get_draft(&story_definition_id).unwrap();
    for n in &mut story.graph.nodes {
        if let K::Pause { resume_to } = &mut n.kind {
            *resume_to = Some("implement".into());
        }
    }
    let story = definitions
        .update_draft(&story.id, story.draft_revision, story.graph)
        .unwrap();
    let published = definitions
        .publish(&story.id, story.draft_revision)
        .unwrap();
    let mut graph = plan.graph;
    for n in &mut graph.nodes {
        if let K::StoryDispatch { story_revision, .. } = &mut n.kind {
            *story_revision = published.revision;
        }
        if let K::Pause { resume_to } = &mut n.kind {
            *resume_to = Some("coordinate".into());
        }
    }
    let plan = definitions
        .update_draft(&plan.id, plan.draft_revision, graph)
        .unwrap();
    let plan = definitions
        .update_checks(
            &plan.id,
            plan.draft_revision,
            published.required_checks.clone(),
        )
        .unwrap();
    let published = definitions.publish(&plan.id, plan.draft_revision).unwrap();
    let store = RunStore::open_at(&config.path().join("critic-runs.sqlite3")).unwrap();
    let run = store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan_id,
            &definition_id,
            published.revision,
            RunLimits::default(),
        )
        .unwrap();
    (config, project, store, run, story_id, guard)
}

/// Catches: a manual Pause (or any non-graph pause) on a graph run that has no
/// pending graph Pause node can never be resumed: Resume is refused for every
/// graph run and ResolvePause needs a pending graph pause, so only Cancel is left.
/// Slice B provides an explicit activation-bound ResumeGraph recovery resolution.
#[test]
fn manual_pause_of_a_graph_run_without_graph_pause_can_be_resumed() {
    let (_config, _project, store, run, story_id, _guard) = published_graph_run();
    let seq = |s: &RunStore| s.snapshot(&run.id).unwrap().sequence;
    store
        .command_expected(
            &run.id,
            "g-start",
            seq(&store),
            RunCommand::Graph {
                transition: GraphTransition::Start {
                    execution_id: "x".into(),
                    target_id: story_id,
                },
            },
        )
        .unwrap();
    store
        .command_expected(&run.id, "pause", seq(&store), RunCommand::Pause)
        .unwrap();
    assert!(
        store
            .command_expected(
                &run.id,
                "status-only-resume",
                seq(&store),
                RunCommand::Resume
            )
            .is_err()
    );
    let resumed = store.command_expected(
        &run.id,
        "resume",
        seq(&store),
        RunCommand::ResumeGraph {
            execution_id: "x".into(),
            activation_id: "a0".into(),
            resolution: "Operator resolved the manual pause".into(),
        },
    );
    assert!(
        resumed.is_ok(),
        "graph run stuck Paused after a manual Pause: {:?}",
        resumed.err()
    );
}
