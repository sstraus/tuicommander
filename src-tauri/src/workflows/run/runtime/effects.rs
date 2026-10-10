//! External graph effects reuse the managed launch and durable intent fences.
use super::super::graph::{Activation, GraphExecution};
use super::super::{EffectKind, EffectState, NodeAttempt};
use super::*;
use crate::workflows::AgentRole;

pub(in crate::workflows::run) fn activation_attempt<'a>(
    run: &'a RunSnapshot,
    graph: &GraphExecution,
    activation: &Activation,
) -> Option<&'a NodeAttempt> {
    let node_id = agent_node_id(graph, activation).ok()?;
    let ordinal = graph
        .activations
        .iter()
        .take_while(|a| a.id != activation.id)
        .filter(|a| a.node_id == activation.node_id)
        .count();
    run.attempts
        .iter()
        .filter(|a| a.story_id == graph.target_id && a.node_id == node_id)
        .nth(ordinal)
}

/// CreateStories consumes the first coordinator report; later visits request a fresh replan.
pub(in crate::workflows::run) fn agent_node_id<'a>(
    graph: &'a GraphExecution,
    activation: &Activation,
) -> Result<&'a str, String> {
    let node = graph
        .definition
        .graph
        .nodes
        .iter()
        .find(|n| n.id == activation.node_id)
        .ok_or("activation node missing")?;
    if matches!(node.kind, NodeKind::CreateStories) {
        return graph
            .definition
            .graph
            .nodes
            .iter()
            .find(|n| {
                matches!(
                    n.kind,
                    NodeKind::Agent {
                        role: AgentRole::Coordinator,
                        ..
                    }
                )
            })
            .map(|n| n.id.as_str())
            .ok_or_else(|| "Create Stories requires a coordinator node".to_owned());
    }
    Ok(node.id.as_str())
}

/// Return true only after durable progress, so waiting agents never spin.
pub(in crate::workflows::run) async fn drive_effect(
    state: &Arc<AppState>,
    store: &RunStore,
    run: &RunSnapshot,
) -> Result<bool, String> {
    if run.status != RunStatus::Running {
        return Ok(false);
    }
    for graph in &run.graph_executions {
        let Some(activation) = graph
            .activations
            .iter()
            .find(|a| a.state == ActivationState::Running)
        else {
            continue;
        };
        let node = graph
            .definition
            .graph
            .nodes
            .iter()
            .find(|n| n.id == activation.node_id)
            .ok_or("activation node missing")?;
        match &node.kind {
            NodeKind::Agent { .. } | NodeKind::CreateStories => {
                let agent_id = agent_node_id(graph, activation)?;
                let NodeKind::Agent { role, .. } = &graph
                    .definition
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.id == agent_id)
                    .ok_or("agent node missing")?
                    .kind
                else {
                    return Err("agent node missing".into());
                };
                let Some(attempt) = activation_attempt(run, graph, activation) else {
                    continue;
                };
                if attempt.state != super::super::AttemptState::Running || attempt.agent.is_some() {
                    continue;
                }
                if *role == AgentRole::Implementer {
                    store.begin_graph_story(
                        run,
                        &graph.target_id,
                        &format!("workflow:{}", attempt.id),
                    )?;
                }
                let key = format!("spawn:{}", attempt.id);
                if run.effects.iter().any(|e| e.key == key) {
                    return Err(
                        "Agent spawn intent needs reconciliation; it will not be retried".into(),
                    );
                }
                // Named profiles are required; do not silently choose another model.
                let profile = match role {
                    AgentRole::Reviewer | AgentRole::Validator => "sonnet",
                    AgentRole::Implementer => "sol",
                    AgentRole::Coordinator | AgentRole::Planner => "sol",
                };
                let settings = crate::config::load_agents_config();
                if !settings
                    .agents
                    .values()
                    .any(|s| s.run_configs.iter().any(|c| c.name == profile))
                {
                    return Err(format!(
                        "workflow requires the configured '{profile}' run profile"
                    ));
                }
                let receipt = store.command_expected(
                    &run.id,
                    &format!("spawn-intent:{}", attempt.id),
                    run.sequence,
                    RunCommand::ReserveEffect {
                        key,
                        kind: EffectKind::SpawnAgent,
                    },
                )?;
                let super::super::RunEventKind::EffectReserved { effect } = receipt.event.kind
                else {
                    return Err("workflow changed before effect reservation".into());
                };
                emit_run_changed(state, &run.project, &run.id, receipt.sequence);
                let existing = run
                    .stories
                    .iter()
                    .find(|s| s.story_id == graph.target_id)
                    .and_then(|s| s.worktree_path.clone());
                let worktree = match existing {
                    Some(path) => path,
                    None => {
                        let branch = if graph.target_id == run.plan_id {
                            format!("workflow/{}-{}", run.id, attempt.id)
                        } else {
                            format!("workflow/{}-{}", run.id, graph.target_id)
                        };
                        match crate::mcp_http::mcp_transport::create_daemon_workflow_worktree(
                            state,
                            &run.project,
                            &branch,
                            None,
                        )
                        .await
                        {
                            Ok(workspace) => workspace.path,
                            Err(error) => {
                                fail_effect(store, &run.id, &effect.id)?;
                                return Err(error);
                            }
                        }
                    }
                };
                let feedback = run
                    .attempts
                    .iter()
                    .rev()
                    .filter(|a| a.story_id == graph.target_id)
                    .find_map(|a| a.report.as_ref())
                    .map(|r| {
                        format!(
                            "{}\nReview: {:?}\nEvidence: {:?}",
                            r.summary, r.review, r.evidence
                        )
                    });
                let state_bg = state.clone();
                let id = run.id.clone();
                let attempt_id = attempt.id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    crate::mcp_http::mcp_transport::launch_daemon_workflow_agent(
                        &state_bg,
                        &id,
                        &attempt_id,
                        &worktree,
                        profile,
                        feedback,
                    )
                })
                .await
                .map_err(|e| format!("workflow launch task: {e}"))?;
                if let Err(error) = result {
                    // Cancellation may have already fenced this intent.
                    if store.snapshot(&run.id)?.status == RunStatus::Running {
                        fail_effect(store, &run.id, &effect.id)?;
                    }
                    return Err(error);
                }
                return Ok(true);
            }
            NodeKind::Judge | NodeKind::Gate
                if graph.definition.kind == crate::workflows::WorkflowKind::Plan =>
            {
                let store = store.clone();
                let run = run.clone();
                let graph = graph.clone();
                let activation = activation.clone();
                let progressed = tokio::task::spawn_blocking(move || {
                    super::plan::drive_checks(&store, &run, &graph, &activation)
                })
                .await
                .map_err(|e| format!("plan check task: {e}"))??;
                if progressed {
                    return Ok(true);
                }
            }
            NodeKind::Judge | NodeKind::Gate => {
                let store = store.clone();
                let run = run.clone();
                let graph = graph.clone();
                let activation = activation.clone();
                let progressed = tokio::task::spawn_blocking(move || {
                    super::policy::drive_policy(&store, &run, &graph, &activation)
                })
                .await
                .map_err(|e| format!("workflow policy task: {e}"))??;
                if progressed {
                    return Ok(true);
                }
            }
            NodeKind::Notify => {
                let key = format!("notify:{}:{}", graph.id, activation.id);
                if let Some(effect) = run.effects.iter().find(|e| e.key == key) {
                    return match effect.state {
                        EffectState::Succeeded => Ok(false),
                        _ => Err(
                            "Notify intent needs explicit reconciliation; delivery is not replayed"
                                .into(),
                        ),
                    };
                }
                let receipt = store.command_expected(
                    &run.id,
                    &format!("daemon:{key}:intent"),
                    run.sequence,
                    RunCommand::ReserveEffect {
                        key: key.clone(),
                        kind: EffectKind::Notify,
                    },
                )?;
                let super::super::RunEventKind::EffectReserved { effect } = receipt.event.kind
                else {
                    return Err("workflow changed before notification".into());
                };
                if store.snapshot(&run.id)?.status != RunStatus::Running {
                    return Ok(true);
                }
                let notice = crate::mcp_http::mcp_transport::report_progress(
                    state,
                    Some(&run.project),
                    crate::progress::ProgressReportInput {
                        kind: crate::progress::ProgressKind::Done,
                        text: format!(
                            "Workflow {} visited {} for {}",
                            run.id, node.id, graph.target_id
                        ),
                        step: Some(key),
                    },
                    Some("Workflow daemon".into()),
                    None,
                    None,
                    None,
                );
                let marked = store.command(
                    &run.id,
                    &format!("daemon:notify-receipt:{}", effect.id),
                    RunCommand::MarkEffect {
                        effect_id: effect.id,
                        succeeded: notice.is_ok(),
                    },
                )?;
                emit_run_changed(state, &run.project, &run.id, marked.sequence);
                notice?;
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

fn fail_effect(store: &RunStore, run_id: &str, effect_id: &str) -> Result<(), String> {
    store
        .command(
            run_id,
            &format!("daemon:effect-failed:{effect_id}"),
            RunCommand::MarkEffect {
                effect_id: effect_id.into(),
                succeeded: false,
            },
        )
        .map(|_| ())
}
