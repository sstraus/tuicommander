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
    let shared_coordinator = graph
        .definition
        .graph
        .nodes
        .iter()
        .any(|n| n.id == activation.node_id && matches!(n.kind, NodeKind::CreateStories));
    if !shared_coordinator
        && graph
            .contract
            .as_ref()
            .is_some_and(|contract| contract.version >= 4)
    {
        return run.attempts.iter().find(|attempt| {
            attempt.graph_binding.as_ref().is_some_and(|binding| {
                binding.execution_id == graph.id && binding.activation_id == activation.id
            })
        });
    }
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

/// Interrupted work keeps its admission slot until its generation's stop commits.
pub(in crate::workflows::run) fn attempt_holds_slot(
    run: &RunSnapshot,
    attempt: &NodeAttempt,
) -> bool {
    attempt.state == super::super::AttemptState::Running
        || attempt.graph_binding.as_ref().is_some_and(|binding| {
            run.graph_executions
                .iter()
                .find(|g| g.id == binding.execution_id)
                .is_some_and(|g| {
                    g.activations
                        .iter()
                        .find(|a| a.id == binding.activation_id)
                        .is_some_and(|a| {
                            a.tokens.iter().any(|t| {
                                g.forks.iter().any(|f| {
                                    f.id == t.fork_id
                                        && f.generation == t.generation
                                        && f.state == super::super::parallel::ForkState::Cancelling
                                })
                            })
                        })
                })
        })
}

fn current_check_claim() -> &'static str {
    static CLAIM: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("check:{}", uuid::Uuid::new_v4()));
    &CLAIM
}

pub(in crate::workflows::run) fn require_known_check_owner(
    graph: &GraphExecution,
    fork_id: &str,
    generation: u32,
) -> Result<(), String> {
    // An empty process registry after another daemon's crash is not stop proof.
    if graph.activations.iter().any(|a| {
        a.tokens
            .iter()
            .any(|t| t.fork_id == fork_id && t.generation == generation)
            && a.resource_claim
                .as_deref()
                .is_some_and(|claim| claim != current_check_claim())
    }) {
        return Err("Superseded check belongs to another daemon; stop outcome is unknown and repair remains fenced".into());
    }
    Ok(())
}

/// Retained contract-2 CreateStories maps to its coordinator; contract 3+ uses typed input.
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
        if let Some(fork) = graph
            .forks
            .iter()
            .find(|f| f.state == super::super::parallel::ForkState::Cancelling)
        {
            require_known_check_owner(graph, &fork.id, fork.generation)?;
            // Cancellation is already durable and reports already fenced. Scope the
            // process registry to this generation, so a late old spawn cannot pass.
            let token = super::super::parallel::BranchToken {
                fork_id: fork.id.clone(),
                generation: fork.generation,
                branch_id: String::new(),
            };
            store.cancel_owned_checks(&super::super::parallel::check_owner(
                &run.id, &graph.id, &token,
            ));
            for nested in graph.forks.iter().filter(|f| {
                f.parent
                    .iter()
                    .any(|t| t.fork_id == fork.id && t.generation == fork.generation)
            }) {
                let token = super::super::parallel::BranchToken {
                    fork_id: nested.id.clone(),
                    generation: nested.generation,
                    branch_id: String::new(),
                };
                store.cancel_owned_checks(&super::super::parallel::check_owner(
                    &run.id, &graph.id, &token,
                ));
            }
            for activation in graph.activations.iter().filter(|a| {
                a.tokens
                    .iter()
                    .any(|t| t.fork_id == fork.id && t.generation == fork.generation)
            }) {
                let Some(attempt) = activation_attempt(run, graph, activation) else {
                    continue;
                };
                if run.effects.iter().any(|effect| {
                    effect.key == format!("spawn:{}", attempt.id)
                        && matches!(effect.state, EffectState::Intended | EffectState::Uncertain)
                        && attempt.agent.is_none()
                }) {
                    return Err(
                        "Superseded spawn outcome is unknown; reconcile before repair".into(),
                    );
                }
                if let Some(binding) = &attempt.agent {
                    if binding.acp.is_some() {
                        crate::mcp_http::launch_transaction::stop_superseded_workflow_agent(
                            state, &run.id, binding,
                        )
                        .await?;
                    } else {
                        crate::pty::kill_pty_core(state, &binding.session_id);
                        if !state
                            .session_maps
                            .exit_codes
                            .contains_key(&binding.session_id)
                        {
                            return Err(
                                "Superseded agent stop is unproven; repair remains fenced".into()
                            );
                        }
                    }
                }
            }
            let stopped = store.command_expected(
                &run.id,
                &format!("fork-stopped:{}:{}", fork.id, fork.generation),
                run.sequence,
                RunCommand::Graph {
                    transition: GraphTransition::ForkStopped {
                        execution_id: graph.id.clone(),
                        fork_id: fork.id.clone(),
                        generation: fork.generation,
                    },
                },
            )?;
            emit_run_changed(state, &run.project, &run.id, stopped.sequence);
            return Ok(true);
        }
        for activation in graph
            .activations
            .iter()
            .filter(|a| a.state == ActivationState::Running)
        {
            let node = graph
                .definition
                .graph
                .nodes
                .iter()
                .find(|n| n.id == activation.node_id)
                .ok_or("activation node missing")?;
            match &node.kind {
                NodeKind::CreateStories if run.event_contract_version >= 3 => {
                    let Some(batch) = super::super::proposals::batch_for(run, graph, activation)
                    else {
                        continue;
                    };
                    if super::super::proposals::drive(store, run, batch)? {
                        let current = store.snapshot(&run.id)?;
                        emit_run_changed(state, &run.project, &run.id, current.sequence);
                        return Ok(true);
                    }
                }
                NodeKind::Agent { .. } | NodeKind::CreateStories => {
                    let agent_id = agent_node_id(graph, activation)?;
                    let NodeKind::Agent {
                        role, execution, ..
                    } = &graph
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
                    if attempt.state != super::super::AttemptState::Running {
                        continue;
                    }
                    if let Some(binding) = &attempt.agent {
                        if attempt.continuation.is_some()
                            && continuation::deliver(state, store, run, attempt).await?
                        {
                            return Ok(true);
                        }
                        if binding.acp.is_some() {
                            let state = state.clone();
                            let run_id = run.id.clone();
                            let attempt_id = attempt.id.clone();
                            let progressed = tokio::task::spawn_blocking(move || {
                                crate::mcp_http::mcp_transport::resume_daemon_workflow_delivery(
                                    &state,
                                    &run_id,
                                    &attempt_id,
                                )
                            })
                            .await
                            .map_err(|e| format!("ACP delivery task: {e}"))??;
                            if progressed {
                                return Ok(true);
                            }
                        }
                        continue;
                    }
                    if *role == AgentRole::Implementer {
                        let status = store.begin_graph_story(
                            run,
                            &graph.target_id,
                            &format!("workflow:{}", attempt.id),
                        )?;
                        if status == crate::stories::StoryStatus::Done {
                            // Approval can win after the deterministic turn but before this effect.
                            let settled = super::drive_turn(store, &run.id)?;
                            emit_run_changed(state, &run.project, &run.id, settled.sequence);
                            return Ok(settled.sequence != run.sequence);
                        }
                        if status != crate::stories::StoryStatus::InProgress {
                            let cause = if status == crate::stories::StoryStatus::Qa {
                                format!("Story {} awaits manual QA", graph.target_id)
                            } else {
                                format!(
                                    "Story {} cannot start implementation in {status:?}",
                                    graph.target_id
                                )
                            };
                            let receipt = store.command_expected(
                                &run.id,
                                &format!("story-not-startable:{}:{}", attempt.id, run.sequence),
                                run.sequence,
                                RunCommand::PauseFor { cause },
                            )?;
                            emit_run_changed(state, &run.project, &run.id, receipt.sequence);
                            return Ok(true);
                        }
                    }
                    let key = format!("spawn:{}", attempt.id);
                    if run.effects.iter().any(|e| e.key == key) {
                        return Err(
                            "Agent spawn intent needs reconciliation; it will not be retried"
                                .into(),
                        );
                    }
                    let pinned_execution = if run.event_contract_version >= 3 {
                        Some(
                            crate::workflows::pinned_agent_route(run, attempt)?
                                .1
                                .route
                                .execution
                                .clone(),
                        )
                    } else {
                        execution.clone()
                    };
                    let profile = match &pinned_execution {
                        Some(crate::workflows::AgentExecution::Acp { .. }) => "ego".to_owned(),
                        Some(crate::workflows::AgentExecution::Pty { profile }) => {
                            crate::config::load_agents_config()
                                .require_named_run_config(profile)?;
                            profile.clone()
                        }
                        None => resolve_run_profile(&crate::config::load_agents_config(), role)?,
                    };
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
                    let mut feedback = run
                    .attempts
                    .iter()
                    .rev()
                    .filter(|a| a.story_id == graph.target_id)
                    .find_map(|a| a.report.as_ref())
                    .map(|r| {
                        if *role == AgentRole::Implementer
                            && let Some(typed) = r.review.as_ref().and_then(|r| r.evidence.as_ref()) {
                            let fixes: Vec<_> = typed.findings.iter().filter(|f| f.decision() == super::super::evidence::TriageDecision::Fix).collect();
                            return format!("Repair only these FIX findings; STORY proposals are separate work and ACCEPT risks are not repair scope: {fixes:?}");
                        }
                        if *role == AgentRole::Reviewer && graph.definition.graph.evidence_policy.is_some() {
                            let prior: Vec<_> = run.attempts.iter().filter(|a| a.story_id == graph.target_id)
                                .filter_map(|a| a.report.as_ref()?.review.as_ref()?.evidence.as_ref())
                                .collect();
                            return format!("Implementation: {}\nPrior typed reviews (resolve or repeat each FIX): {prior:?}", r.summary);
                        }
                        format!(
                            "{}\nReview: {:?}\nEvidence: {:?}",
                            r.summary, r.review, r.evidence
                        )
                    });
                    if let Some(item) = activation.tokens.last().and_then(|token| {
                        graph
                            .forks
                            .iter()
                            .find(|f| f.id == token.fork_id)
                            .and_then(|f| f.branches.iter().find(|b| b.id == token.branch_id))
                            .and_then(|b| b.item.as_ref())
                    }) {
                        let context = format!("Declared For each item: {item}");
                        feedback =
                            Some(feedback.map_or(context.clone(), |previous| {
                                format!("{previous}\n{context}")
                            }));
                    }
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
                    if !activation.tokens.is_empty() {
                        let used = run
                            .attempts
                            .iter()
                            .filter(|a| attempt_holds_slot(run, a))
                            .count()
                            + run
                                .graph_executions
                                .iter()
                                .flat_map(|g| &g.activations)
                                .filter(|a| a.resource_claim.is_some())
                                .count();
                        if activation.resource_claim.is_some() {
                            continue;
                        }
                        if used >= usize::from(run.limits.max_parallel_stories) {
                            if activation.resource_wait.is_none() {
                                store.command_expected(
                                    &run.id,
                                    &format!("check-wait:{}:{}", activation.id, run.sequence),
                                    run.sequence,
                                    RunCommand::Graph {
                                        transition: GraphTransition::ResourceWait {
                                            execution_id: graph.id.clone(),
                                            activation_id: activation.id.clone(),
                                            reason: Some(
                                                "Parallel activity capacity is occupied".into(),
                                            ),
                                        },
                                    },
                                )?;
                                return Ok(true);
                            }
                            continue;
                        }
                        let claimed = store.command_expected(
                            &run.id,
                            &format!("check-claim:{}:{}", activation.id, run.sequence),
                            run.sequence,
                            RunCommand::Graph {
                                transition: GraphTransition::ResourceClaim {
                                    execution_id: graph.id.clone(),
                                    activation_id: activation.id.clone(),
                                    key: Some(current_check_claim().into()),
                                },
                            },
                        )?;
                        let claim_sequence = claimed.sequence;
                        let graph_id = graph.id.clone();
                        let activation_id = activation.id.clone();
                        let state = state.clone();
                        let store = store.clone();
                        let run = claimed.snapshot;
                        let graph = run
                            .graph_executions
                            .iter()
                            .find(|g| g.id == graph_id)
                            .expect("claimed graph")
                            .clone();
                        let activation = graph
                            .activations
                            .iter()
                            .find(|a| a.id == activation_id)
                            .expect("claimed activation")
                            .clone();
                        tokio::spawn(async move {
                            let store_bg = store.clone();
                            let run_bg = run.clone();
                            let graph_bg = graph.clone();
                            let activation_bg = activation.clone();
                            let result = tokio::task::spawn_blocking(move || {
                                super::policy::drive_policy(
                                    &store_bg,
                                    &run_bg,
                                    &graph_bg,
                                    &activation_bg,
                                )
                            })
                            .await;
                            let current = store.snapshot(&run.id);
                            if let Ok(current) = current {
                                let current_generation = current
                                    .graph_executions
                                    .iter()
                                    .find(|g| g.id == graph.id)
                                    .and_then(|g| {
                                        g.activations.iter().find(|a| a.id == activation.id)
                                    })
                                    .is_some_and(|a| a.state == ActivationState::Running);
                                if current_generation
                                    && let Err(error) = store.command(
                                        &run.id,
                                        &format!(
                                            "check-release:{}:{claim_sequence}",
                                            activation.id
                                        ),
                                        RunCommand::Graph {
                                            transition: GraphTransition::ResourceClaim {
                                                execution_id: graph.id.clone(),
                                                activation_id: activation.id.clone(),
                                                key: None,
                                            },
                                        },
                                    )
                                {
                                    super::pause_failure(&state, &store, &run.id, &error);
                                }
                                match result {
                                    Ok(Ok(_)) => {}
                                    // Release the claim and wake the actor to re-evaluate
                                    // ordinary sibling/preflight contention, just as a sync turn does.
                                    Ok(Err(error)) if super::is_contention(&error) => {}
                                    Ok(Err(error)) if current_generation => {
                                        super::pause_failure(&state, &store, &run.id, &error)
                                    }
                                    Err(error) if current_generation => super::pause_failure(
                                        &state,
                                        &store,
                                        &run.id,
                                        &error.to_string(),
                                    ),
                                    _ => {}
                                }
                                emit_run_changed(&state, &run.project, &run.id, current.sequence);
                            }
                        });
                        return Ok(true);
                    }
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
    _config: &crate::config::AgentsConfig,
) -> Vec<String> {
    let policy =
        match crate::workflows::WorkflowStore::open().and_then(|store| store.agent_policy()) {
            Ok(policy) => policy,
            Err(error) => return vec![format!("host routing policy unavailable: {error}")],
        };
    let mut problems = vec![];
    for node in &definition.graph.nodes {
        if let NodeKind::Agent {
            role,
            execution,
            capabilities,
            ..
        } = &node.kind
        {
            let result = policy
                .as_ref()
                .ok_or("effective host routing policy is not imported".to_owned())
                .and_then(|policy| policy.select(*role, execution.as_ref(), capabilities))
                .and_then(|pinned| pinned.validate_live_settings());
            if let Err(problem) = result
                && !problems.contains(&problem)
            {
                problems.push(problem);
            }
        }
    }
    problems
}
