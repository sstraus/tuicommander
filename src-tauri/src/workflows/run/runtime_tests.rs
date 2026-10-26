use super::graph::{ActivationState, DecisionEvidence, EdgeOutcome, GraphTransition};
use super::runtime::{RuntimeOwner, WorkflowRuntime, drive_turn};
use super::store::GraphStartRequest;
use super::*;
use crate::stories::StoryStore;
use crate::workflows::{
    AgentRole, CheckDefinition, Edge, Node, NodeKind, WorkflowGraph, WorkflowKind, WorkflowStore,
};
use std::sync::Arc;

mod critic_c;
mod default_install;
mod parity;

fn fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    String,
    String,
    String,
    impl Drop,
) {
    let result = super::tests::fixture();
    let root = crate::test_support::test_temp_root();
    assert!(result.0.path().starts_with(&root) && result.1.path().starts_with(&root));
    result
}

fn canonical_owner(project: &str) -> String {
    // Match definition_action: Windows canonicalization adds the verbatim prefix.
    crate::progress::resolve_owning_project(Some(project))
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

fn definition(project: &str, pause: bool, agent: bool) -> crate::workflows::PublishedWorkflow {
    let project = canonical_owner(project);
    let project = project.as_str();
    let mut nodes = vec![
        Node {
            id: "start".into(),
            kind: NodeKind::Start,
        },
        Node {
            id: "end".into(),
            kind: NodeKind::End,
        },
    ];
    let mut edges = vec![Edge {
        from: "start".into(),
        to: "end".into(),
        outcome: None,
    }];
    if pause {
        nodes.push(Node {
            id: "gate".into(),
            kind: NodeKind::Gate,
        });
        nodes.push(Node {
            id: "pause".into(),
            kind: NodeKind::Pause {
                resume_to: Some("end".into()),
            },
        });
        edges[0].to = "gate".into();
        edges.extend([
            Edge {
                from: "gate".into(),
                to: "end".into(),
                outcome: Some("pass".into()),
            },
            Edge {
                from: "gate".into(),
                to: "pause".into(),
                outcome: Some("fail".into()),
            },
        ]);
    } else if agent {
        nodes.push(Node {
            id: "implement".into(),
            kind: NodeKind::Agent {
                role: AgentRole::Implementer,
                capabilities: vec!["story_read".into(), "story_report".into()],
                prompt_template: "Implement the native criteria".into(),
            },
        });
        edges[0].to = "implement".into();
        edges.push(Edge {
            from: "implement".into(),
            to: "end".into(),
            outcome: None,
        });
    }
    let store = WorkflowStore::open().unwrap();
    let draft = store
        .create_draft(
            project,
            "Runtime contract",
            WorkflowKind::Story,
            WorkflowGraph { nodes, edges },
        )
        .unwrap();
    let draft = store
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "status".into(),
                argv: vec!["git".into(), "status".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    store.publish(&draft.id, draft.draft_revision).unwrap()
}

fn request(
    project: &str,
    story: &str,
    definition: &crate::workflows::PublishedWorkflow,
    key: &str,
) -> GraphStartRequest {
    GraphStartRequest {
        project: project.into(),
        target: RunTarget::Story(story.into()),
        expected_revision: Some(
            StoryStore::open()
                .unwrap()
                .get_story(story)
                .unwrap()
                .revision,
        ),
        definition_id: definition.id.clone(),
        definition_revision: definition.revision,
        request_id: key.into(),
        limits: RunLimits::default(),
    }
}

fn state(config: &std::path::Path) -> Arc<crate::state::AppState> {
    Arc::new(crate::state::AppState::new(
        config.to_path_buf(),
        config.join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY),
        )),
    ))
}

async fn owner_ready(state: &Arc<crate::state::AppState>) {
    // Setup is not the behavior deadline. No process execution is timed here.
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while state.workflow_runtime.require_owner().is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("daemon setup did not acquire ownership");
}

/// Catches: raw fixture paths publishing definitions outside the canonical root identity.
#[test]
fn runtime_definition_uses_the_api_canonical_owner() {
    let (config, project, plan, story, _template, _guard) = fixture();
    let raw = project.path().join(".");
    let published = definition(raw.to_str().unwrap(), false, false);
    let plan = StoryStore::open().unwrap().get_plan(&plan).unwrap();
    assert_eq!(published.project, plan.project);
    let semantics_definition = semantics::publish(raw.to_str().unwrap(), published.graph.clone());
    assert_eq!(semantics_definition.project, plan.project);
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(
            raw.to_str().unwrap(),
            &story,
            &published,
            "raw-path",
        ))
        .unwrap();
    assert_eq!(run.project, plan.project);
    assert_eq!(run.status, RunStatus::Running);
}

/// Catches: all-manual Ready work entering QA but still reserving an implementer spawn.
#[tokio::test]
async fn manual_only_workflow_pauses_before_implementer_spawn() {
    let (config, project, _plan, story_id, _template, _guard) = fixture();
    let stories = StoryStore::open().unwrap();
    let story = stories.get_story(&story_id).unwrap();
    stories
        .transition(
            &story.id,
            story.revision,
            crate::stories::StoryCommand::SetCriterionManual {
                index: 0,
                manual: true,
            },
        )
        .unwrap();
    // Remove the default-install migration's profile in this isolated config so
    // even the pre-fix regression cannot launch an external agent.
    let base = crate::config::load_agents_config();
    let mut agents = base.clone();
    for settings in agents.agents.values_mut() {
        settings.run_configs.clear();
    }
    crate::config::save_agents_config(base, agents).unwrap();
    assert_eq!(
        crate::config::load_agents_config()
            .saved_run_configs()
            .len(),
        0
    );
    let published = definition(project.path().to_str().unwrap(), false, true);
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story_id,
            &published,
            "manual-only",
        ))
        .unwrap();
    let current = drive_turn(&store, &run.id).unwrap();
    assert_eq!(current.attempts.len(), 1);
    let result = runtime::effects::drive_effect(&state(config.path()), &store, &current).await;
    assert_eq!(
        stories.get_story(&story_id).unwrap().status,
        crate::stories::StoryStatus::Qa
    );
    let paused = store.snapshot(&run.id).unwrap();
    assert_eq!(paused.status, RunStatus::Paused);
    assert_eq!(paused.effects.len(), 0);
    assert_eq!(paused.attempts[0].agent, None);
    assert_eq!(paused.stories[0].worktree_path, None);
    assert_eq!(store.replay(&run.id).unwrap(), paused);
    assert!(result.unwrap());
}

/// Catches: a story-root run fencing unrelated siblings as if it owned the whole plan.
#[test]
fn archive_story_root_protects_target_but_allows_unrelated_sibling() {
    let (config, project, plan, story, _template, _guard) = fixture();
    let owner = RuntimeOwner::acquire(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let run = owner
        .store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story,
            &published,
            "archive-root",
        ))
        .unwrap();
    let stories = StoryStore::open().unwrap();
    let sibling = stories
        .create_story(crate::stories::NewStory {
            plan_id: plan.clone(),
            title: "Unrelated duplicate".into(),
            criteria: vec!["Unused".into()],
            priority: 2,
            story_type: None,
            origin: crate::stories::StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let actor = crate::stories::StoryTransitionActor::Human;
    let archived = stories
        .set_story_archived(&sibling.id, sibling.revision, true, actor.clone())
        .expect("unrelated sibling is not owned by story root");
    stories
        .set_story_archived(&sibling.id, archived.revision, false, actor.clone())
        .unwrap();
    let target = stories.get_story(&story).unwrap();
    assert!(
        stories
            .set_story_archived(&story, target.revision, true, actor.clone())
            .is_err()
    );
    let plan_record = stories.get_plan(&plan).unwrap();
    assert!(
        stories
            .set_plan_archived(&plan, plan_record.revision, true, actor.clone())
            .is_err()
    );
    owner
        .store
        .command(&run.id, "pause-archive-root", RunCommand::Pause)
        .unwrap();
    let sibling = stories.get_story(&sibling.id).unwrap();
    stories
        .set_story_archived(&sibling.id, sibling.revision, true, actor.clone())
        .unwrap();
    assert!(
        stories
            .set_story_archived(&story, target.revision, true, actor)
            .is_err()
    );
}

#[test]
fn start_story_activates_first_successor() {
    // catches: a Running snapshot with no runnable node (01), changed-key retries or skipped predecessors.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let owner = RuntimeOwner::acquire(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let start = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "first",
    );
    let run = owner.store.start_graph_run(&start).unwrap();
    assert_eq!(run.root_target, Some(RunTarget::Story(story.clone())));
    assert_eq!(
        run.plan_id,
        StoryStore::open()
            .unwrap()
            .get_story(&story)
            .unwrap()
            .plan_id
    );
    let graph = &run.graph_executions[0];
    assert_eq!(graph.activations[0].state, ActivationState::Completed);
    assert_eq!(graph.activations[1].node_id, "implement");
    assert_eq!(graph.activations[1].state, ActivationState::Ready);
    assert_eq!(owner.store.replay(&run.id).unwrap(), run);
    assert_eq!(owner.store.start_graph_run(&start).unwrap().id, run.id);
    let mut changed = start.clone();
    changed.limits.max_spawns += 1;
    assert!(
        owner
            .store
            .start_graph_run(&changed)
            .unwrap_err()
            .to_string()
            .contains("different payload")
    );
    assert!(
        owner
            .store
            .command(
                &run.id,
                "skip",
                RunCommand::StartAttempt {
                    story_id: story,
                    node_id: "end".into()
                }
            )
            .is_err()
    );
    assert!(
        owner
            .store
            .command(
                &run.id,
                "unreached",
                RunCommand::Graph {
                    transition: GraphTransition::Activate {
                        execution_id: "root".into(),
                        activation_id: "a2".into()
                    }
                }
            )
            .is_err()
    );
    let prepared = drive_turn(&owner.store, &run.id).unwrap();
    assert_eq!(prepared.attempts.len(), 1);
    assert!(prepared.attempts[0].agent.is_none());
    assert!(
        prepared.effects.is_empty(),
        "deterministic turns do not spawn agents"
    );
    assert_eq!(owner.store.replay(&run.id).unwrap(), prepared);
}

fn paused(store: &RunStore, run: &RunSnapshot) -> RunSnapshot {
    store
        .command(
            &run.id,
            "gate-enter",
            RunCommand::Graph {
                transition: GraphTransition::Activate {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    store
        .command(
            &run.id,
            "gate-fail",
            RunCommand::Graph {
                transition: GraphTransition::Complete {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                    outcome: Some(EdgeOutcome::Fail),
                    evidence: Some(DecisionEvidence {
                        actor: "operator".into(),
                        reason: "Domain transition fixture; no external check is asserted".into(),
                        references: vec!["native:test-decision".into()],
                    }),
                },
            },
        )
        .unwrap();
    drive_turn(store, &run.id).unwrap()
}

fn resume() -> RunCommand {
    RunCommand::ResumeGraph {
        execution_id: "root".into(),
        activation_id: "a2".into(),
        resolution: "Operator resolved the recorded pause".into(),
    }
}

#[test]
fn resume_requires_resolution_and_reactivates_target() {
    // catches: status-only resume or immediate re-pause (06), and uncertain effects bypassing resolution.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let published = definition(project.path().to_str().unwrap(), true, false);
    let run = store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story,
            &published,
            "pause",
        ))
        .unwrap();
    let effect = store
        .command(
            &run.id,
            "intent",
            RunCommand::ReserveEffect {
                key: "no-effect-executed".into(),
                kind: EffectKind::SpawnAgent,
            },
        )
        .unwrap()
        .snapshot
        .effects[0]
        .id
        .clone();
    let run = paused(&store, &run);
    assert_eq!(run.status, RunStatus::Paused);
    assert_eq!(run.graph_executions[0].pauses[0].resume_to, "end");
    assert!(
        store
            .command(&run.id, "status-only", RunCommand::Resume)
            .is_err()
    );
    store.reconcile_after_restart(&run.id).unwrap();
    assert!(
        store
            .command(&run.id, "unsafe-resolution", resume())
            .unwrap_err()
            .contains("uncertain")
    );
    store
        .command(
            &run.id,
            "effect-resolved",
            RunCommand::ResolveUncertainEffect {
                effect_id: effect,
                succeeded: false,
            },
        )
        .unwrap();
    let resumed = store
        .command(&run.id, "resolved", resume())
        .unwrap()
        .snapshot;
    assert_eq!(resumed.status, RunStatus::Running);
    assert_eq!(
        resumed.graph_executions[0]
            .activations
            .last()
            .unwrap()
            .node_id,
        "end"
    );
    assert!(resumed.graph_executions[0].pauses[0].resolution.is_some());
    assert_eq!(
        drive_turn(&store, &run.id).unwrap().status,
        RunStatus::Running
    );
    assert_eq!(
        store.replay(&run.id).unwrap(),
        store.snapshot(&run.id).unwrap()
    );
}

#[test]
fn cancel_paused_run_fences_pending_work() {
    // catches: resumed/stale reports advancing cancellation (07).
    let (config, project, _plan, story, _template, _guard) = fixture();
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let published = definition(project.path().to_str().unwrap(), true, false);
    let start = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "cancel",
    );
    let run = store.start_graph_run(&start).unwrap();
    let run = paused(&store, &run);
    let cancelled = store
        .command_expected(&run.id, "cancel", run.sequence, RunCommand::Cancel)
        .unwrap()
        .snapshot;
    assert_eq!(cancelled.status, RunStatus::Cancelled);
    assert!(store.command(&run.id, "late-resume", resume()).is_err());
    assert!(
        store
            .command(
                &run.id,
                "late-report",
                RunCommand::ReportAttempt {
                    attempt_id: "a2".into(),
                    generation: 1,
                    outcome: AttemptOutcome::Completed
                }
            )
            .is_err()
    );
    assert_eq!(drive_turn(&store, &run.id).unwrap(), cancelled);
    assert_eq!(store.replay(&run.id).unwrap(), cancelled);
    let mut restart = start;
    restart.request_id = "after-cancel".into();
    assert_ne!(store.start_graph_run(&restart).unwrap().id, run.id);
}

#[test]
fn restart_recovers_graph_without_respawning() {
    // catches: duplicate spawn/lost position (25), or a second owner/read recovering healthy work.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let path = config.path().join("workflow_runs.sqlite3");
    let owner = RuntimeOwner::acquire(&path).unwrap();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let run = owner
        .store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story,
            &published,
            "restart",
        ))
        .unwrap();
    let prepared = drive_turn(&owner.store, &run.id).unwrap();
    let attempt = prepared.attempts.last().unwrap();
    owner
        .store
        .command(
            &run.id,
            "intent",
            RunCommand::ReserveEffect {
                key: format!("spawn:{}", attempt.id),
                kind: EffectKind::SpawnAgent,
            },
        )
        .unwrap();
    let before = owner.store.snapshot(&run.id).unwrap();
    assert!(RuntimeOwner::acquire(&path).is_err(), "two executor owners");
    assert_eq!(
        RunStore::open().unwrap().snapshot(&run.id).unwrap(),
        before,
        "read recovered healthy work"
    );
    drop(owner);
    let restarted = RuntimeOwner::acquire(&path).unwrap();
    let recovered = restarted.store.snapshot(&run.id).unwrap();
    assert_eq!(recovered.status, RunStatus::Paused);
    assert_eq!(recovered.effects[0].state, EffectState::Uncertain);
    assert_eq!(recovered.graph_executions, before.graph_executions);
    let recovery = RunCommand::ResumeGraph {
        execution_id: "root".into(),
        activation_id: "a1".into(),
        resolution: "Inspected crash boundary".into(),
    };
    assert!(
        restarted
            .store
            .command(&run.id, "unsafe-resume", recovery.clone())
            .is_err()
    );
    restarted
        .store
        .command(
            &run.id,
            "resolve-effect",
            RunCommand::ResolveUncertainEffect {
                effect_id: recovered.effects[0].id.clone(),
                succeeded: false,
            },
        )
        .unwrap();
    restarted
        .store
        .command(&run.id, "safe-resume", recovery)
        .unwrap();
    let current = drive_turn(&restarted.store, &run.id).unwrap();
    assert_eq!(current.status, RunStatus::Paused);
    assert_eq!(current.attempts.len(), prepared.attempts.len());
    assert_eq!(current.spawns, 1);
    assert_eq!(current.effects.len(), 1);
    assert_eq!(current.graph_executions, before.graph_executions);
    assert_eq!(restarted.store.replay(&run.id).unwrap(), current);
}

#[tokio::test]
async fn deadline_pauses_idle_run_and_bounds_check() {
    // catches: deadlines enforced only on the next operator command (26).
    // Check-process timeout coverage remains in check.rs; no check is started by B.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let published = definition(project.path().to_str().unwrap(), false, false);
    let mut request = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "deadline",
    );
    request.limits.max_duration_secs = 1;
    let mut hints = state.event_bus.subscribe();
    let run = state
        .workflow_runtime
        .start_graph(&state, &request)
        .unwrap();
    let store = RunStore::open().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let event = hints.recv().await.unwrap();
            if matches!(event, crate::state::AppEvent::WorkflowRunChanged { .. })
                && store.snapshot(&run.id).unwrap().status == RunStatus::Paused
            {
                break;
            }
        }
    })
    .await
    .expect("daemon did not pause at the duration deadline");
    let expired = store.snapshot(&run.id).unwrap();
    let events = store.events_after(&run.id, run.sequence, 20).unwrap();
    assert!(events.iter().any(|event| matches!(event.kind, RunEventKind::DeadlineExpired { deadline_ms } if deadline_ms == run.started_ms + 1000)));
    assert!(
        expired.graph_executions[0].pauses.is_empty(),
        "deadline must not invent a graph Pause"
    );
    assert!(
        store
            .command(
                &run.id,
                "past-deadline",
                RunCommand::ResumeGraph {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                    resolution: "Cannot reset duration budget".into()
                }
            )
            .unwrap_err()
            .contains("deadline")
    );
    assert_eq!(
        store.replay(&run.id).unwrap(),
        store.snapshot(&run.id).unwrap()
    );
}

mod ownership;

mod semantics;
