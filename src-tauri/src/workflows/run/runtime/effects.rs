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
                    let status = store.begin_graph_story(
                        run,
                        &graph.target_id,
                        &format!("workflow:{}", attempt.id),
                    )?;
                    if status == crate::stories::StoryStatus::Qa {
                        let receipt = store.command_expected(
                            &run.id,
                            &format!("manual-qa:{}:{}", attempt.id, run.sequence),
                            run.sequence,
                            RunCommand::PauseFor {
                                cause: format!("Story {} awaits manual QA", graph.target_id),
                            },
                        )?;
                        emit_run_changed(state, &run.project, &run.id, receipt.sequence);
                        return Ok(true);
                    }
                }
                let key = format!("spawn:{}", attempt.id);
                if run.effects.iter().any(|e| e.key == key) {
                    return Err(
                        "Agent spawn intent needs reconciliation; it will not be retried".into(),
                    );
                }
                let profile = resolve_run_profile(&crate::config::load_agents_config(), role)?;
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
                        &profile,
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
    // The launch path may already have closed this intent; marking twice would hide the real error.
    if store
        .snapshot(run_id)?
        .effects
        .iter()
        .any(|e| e.id == effect_id && e.state != EffectState::Intended)
    {
        return Ok(());
    }
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

/// A run configuration named `sol` (builders) or `sonnet` (reviewers) still wins when saved;
/// otherwise the user's default run configuration serves every role.
pub(in crate::workflows) fn resolve_run_profile(
    config: &crate::config::AgentsConfig,
    role: &AgentRole,
) -> Result<String, String> {
    let preferred = match role {
        AgentRole::Reviewer | AgentRole::Validator => "sonnet",
        AgentRole::Implementer | AgentRole::Coordinator | AgentRole::Planner => "sol",
    };
    if let Some((_, profile)) = config.named_run_config(preferred) {
        return Ok(profile.name.clone());
    }
    let saved = config.saved_run_configs();
    let no_default = || {
        format!(
            "workflow has no run profile for the {role:?} role: {}",
            if saved.is_empty() {
                "no run configuration is saved; save one in Settings > Agents".to_owned()
            } else {
                "several run configurations are saved and none is the default; mark one as default in Settings > Agents".to_owned()
            }
        )
    };
    let defaults: Vec<_> = saved
        .iter()
        .filter(|(_, profile)| profile.is_default)
        .collect();
    let candidates: Vec<_> = match (defaults.as_slice(), saved.as_slice()) {
        ([], [only]) => vec![only],
        ([], _) => return Err(no_default()),
        _ => defaults,
    };
    // A default is chosen for interactive use; a sandbox bypass must be chosen for workflows by name.
    let (safe, bypassing): (Vec<_>, Vec<_>) =
        candidates.into_iter().partition(|(agent, profile)| {
            crate::agent_hook_launch::permission_bypass_flag(config, agent, &profile.args).is_none()
        });
    match (safe.as_slice(), bypassing.first()) {
        ([(_, profile)], _) => Ok(profile.name.clone()),
        ([], Some((agent, profile))) => {
            let flag =
                crate::agent_hook_launch::permission_bypass_flag(config, agent, &profile.args)
                    .unwrap_or_default();
            Err(format!(
                "workflow will not use the default run profile '{}': it passes {flag}. Save a run profile named '{preferred}' to choose it for workflows, or use another default",
                profile.name
            ))
        }
        (several, _) => Err(format!(
            "several agents have a default run profile ({}); save a run profile named '{preferred}' to choose one for workflows",
            several
                .iter()
                .map(|(_, profile)| profile.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// What stops a published workflow from reaching its Agent steps, checked before a run starts.
pub(in crate::workflows) fn profile_problems(
    definition: &crate::workflows::PublishedWorkflow,
    config: &crate::config::AgentsConfig,
) -> Vec<String> {
    let mut problems: Vec<String> = Vec::new();
    for node in &definition.graph.nodes {
        if let NodeKind::Agent { role, .. } = &node.kind
            && let Err(problem) = resolve_run_profile(config, role)
            && !problems.contains(&problem)
        {
            problems.push(problem);
        }
    }
    problems
}
