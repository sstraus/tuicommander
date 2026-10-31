//! Daemon-owned serial scheduling. Mailboxes wake actors; SQLite owns the work.
use super::graph::{ActivationState, DecisionEvidence, GraphTransition};
mod continuation;
pub(super) mod effects;
pub(super) mod judge;
pub(super) mod plan;
pub(super) mod policy;
use super::store::{GraphStartRequest, now_ms};
use super::{RunCommand, RunSnapshot, RunStatus, RunStore, emit_run_changed};
use crate::state::AppState;
use crate::workflows::NodeKind;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Notify;

/// Bound synchronous graph transitions before giving other actors a turn.
const TRANSITIONS_PER_TURN: usize = 32;

#[derive(Default)]
pub(crate) struct WorkflowRuntime {
    started: AtomicBool,
    state: parking_lot::Mutex<Weak<AppState>>,
    owner: parking_lot::Mutex<Option<Arc<RuntimeOwner>>>,
}

pub(super) struct RuntimeOwner {
    // Never unlink this file: another process must lock the same inode.
    _lock: File,
    pub(super) store: RunStore,
    actors: parking_lot::Mutex<HashMap<String, Arc<Notify>>>,
}

impl RuntimeOwner {
    pub(super) fn acquire(path: &Path) -> Result<Arc<Self>, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create executor directory: {e}"))?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("owner.lock"))
            .map_err(|e| format!("open executor owner lock: {e}"))?;
        lock.try_lock()
            .map_err(|e| format!("workflow executor unavailable: {e}"))?;
        // Ownership precedes even opening/recovering the store. Reads never recover.
        let store = RunStore::open_at(path)?;
        store.reconcile_runs_after_restart(&store.active_run_ids()?);
        Ok(Arc::new(Self {
            _lock: lock,
            store,
            actors: Default::default(),
        }))
    }
}

impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        for notify in self.actors.get_mut().values() {
            notify.notify_one();
        }
    }
}

impl WorkflowRuntime {
    /// Start once on both daemon boot paths, outside the WebView lifecycle.
    pub(crate) fn spawn(state: &Arc<AppState>) {
        if state.workflow_runtime.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(state);
        *state.workflow_runtime.state.lock() = weak.clone();
        let path = crate::config::config_dir().join("workflow_runs.sqlite3");
        tokio::spawn(async move {
            let owner = match tokio::task::spawn_blocking(move || RuntimeOwner::acquire(&path))
                .await
            {
                Ok(Ok(owner)) => owner,
                Ok(Err(error)) => {
                    tracing::warn!(source = "workflows", %error, "Workflow executor unavailable");
                    return;
                }
                Err(error) => {
                    tracing::error!(source = "workflows", %error, "Workflow executor startup failed");
                    return;
                }
            };
            let Some(state) = weak.upgrade() else {
                return;
            };
            let ids = match owner.store.active_run_ids() {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::error!(source = "workflows", %error, "Workflow recovery scan failed");
                    return;
                }
            };
            *state.workflow_runtime.owner.lock() = Some(owner);
            for id in ids {
                state.workflow_runtime.wake(&id);
            }
        });
    }

    pub(crate) fn require_owner(&self) -> Result<(), String> {
        self.owner.lock().as_ref().map(|_| ()).ok_or_else(|| {
            "workflow executor unavailable: this daemon does not own the run database".into()
        })
    }

    /// Root-start boundary shared by public run transports.
    pub(crate) fn start_graph(
        &self,
        state: &AppState,
        request: &GraphStartRequest,
    ) -> Result<RunSnapshot, String> {
        self.start_graph_in_worktree(state, request, None)
    }

    /// Event intake binds its already-created isolated artifact before any actor wakes.
    pub(crate) fn start_graph_in_worktree(
        &self,
        state: &AppState,
        request: &GraphStartRequest,
        worktree: Option<(&str, &str)>,
    ) -> Result<RunSnapshot, String> {
        let owner = self
            .owner
            .lock()
            .clone()
            .ok_or("workflow executor unavailable")?;
        let definition = crate::workflows::WorkflowStore::open()?
            .get_published(&request.definition_id, request.definition_revision)?;
        require_supported_nodes(&definition)?;
        let mut snapshot = owner.store.start_graph_run(request)?;
        if let Some((story_id, path)) = worktree {
            let bound = (|| {
                drive_turn(&owner.store, &snapshot.id)?;
                owner
                    .store
                    .command(
                        &snapshot.id,
                        &format!("event-worktree:{}", request.request_id),
                        RunCommand::AssignWorktree {
                            story_id: story_id.into(),
                            path: path.into(),
                        },
                    )
                    .map(|receipt| receipt.snapshot)
            })();
            match bound {
                Ok(bound) => snapshot = bound,
                Err(error) => {
                    let receipt = owner.store.command(
                        &snapshot.id,
                        &format!("event-start-failed:{}", request.request_id),
                        RunCommand::Cancel,
                    )?;
                    emit_run_changed(state, &snapshot.project, &snapshot.id, receipt.sequence);
                    return Err(error);
                }
            }
        }
        emit_run_changed(state, &snapshot.project, &snapshot.id, snapshot.sequence);
        Ok(snapshot)
    }

    pub(crate) fn wake(&self, run_id: &str) {
        let Some(owner) = self.owner.lock().clone() else {
            return;
        };
        let mut actors = owner.actors.lock();
        if let Some(notify) = actors.get(run_id) {
            notify.notify_one();
            return;
        }
        let notify = Arc::new(Notify::new());
        actors.insert(run_id.to_owned(), notify.clone());
        tokio::spawn(run_actor(
            self.state.lock().clone(),
            Arc::downgrade(&owner),
            run_id.to_owned(),
            notify,
        ));
    }
}

fn require_supported_nodes(definition: &crate::workflows::PublishedWorkflow) -> Result<(), String> {
    crate::workflows::definition::validate_runtime_nodes(&definition.graph, definition.kind)
}

/// Cursor and preflight movement asks the actor to re-evaluate current state.
fn is_contention(error: &str) -> bool {
    error.contains("sequence") || error.contains("preflight; retry")
}

async fn run_actor(
    state: Weak<AppState>,
    owner: Weak<RuntimeOwner>,
    run_id: String,
    notify: Arc<Notify>,
) {
    loop {
        let Some(current) = owner.upgrade() else {
            return;
        };
        let store = current.store.clone();
        let id = run_id.clone();
        // Keep ownership alive until the blocking transaction completes.
        let result = tokio::task::spawn_blocking(move || {
            let _owner = current;
            drive_turn(&store, &id)
        })
        .await;
        let snapshot = match result {
            Ok(Ok(snapshot)) => snapshot,
            Ok(Err(error)) => {
                tracing::warn!(source = "workflows", %run_id, %error, "Workflow scheduling turn failed");
                if is_contention(&error) {
                    tokio::task::yield_now().await;
                    continue;
                }
                if let (Some(state), Some(owner)) = (state.upgrade(), owner.upgrade()) {
                    pause_failure(&state, &owner.store, &run_id, &error);
                }
                notify.notified().await;
                continue;
            }
            Err(error) => {
                tracing::error!(source = "workflows", %run_id, %error, "Workflow actor failed");
                break;
            }
        };
        if let Some(state) = state.upgrade() {
            // Do not wake our own actor for an unchanged snapshot.
            super::api::emit_run_cursor(&state, &snapshot.project, &snapshot.id, snapshot.sequence);
        } else {
            break;
        }
        if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
            break;
        }
        if snapshot.status == RunStatus::Running || effects::has_pending_command_recovery(&snapshot)
        {
            let Some(state) = state.upgrade() else {
                break;
            };
            let Some(current) = owner.upgrade() else {
                break;
            };
            // A timer wake must not drop an in-flight adapter future. Keep it pinned
            // while independently committing due timer receipts.
            let effect = effects::drive_effect(&state, &current.store, &snapshot);
            tokio::pin!(effect);
            let mut timer_snapshot = snapshot.clone();
            let mut timer_changed = false;
            let result = loop {
                let next_wake = if timer_snapshot.status == RunStatus::Running {
                    Some(next_wake_ms(&timer_snapshot))
                } else {
                    super::timer::next_due(&timer_snapshot)
                };
                let remaining =
                    next_wake.map_or(0, |due| due.saturating_sub(now_ms()).max(0) as u64);
                tokio::select! {
                    result = &mut effect => break result.map(|changed| changed || timer_changed),
                    _ = tokio::time::sleep(std::time::Duration::from_millis(remaining)),
                        if next_wake.is_some() => {},
                    _ = notify.notified() => {},
                }
                // New schedule/cancel hints also recompute the earliest deadline,
                // without restarting the already-running adapter.
                let store = current.store.clone();
                let id = run_id.clone();
                let fired = tokio::task::spawn_blocking(move || {
                    let latest = store.snapshot(&id)?;
                    if latest.status == RunStatus::Running && now_ms() >= deadline_ms(&latest) {
                        return store
                            .command(
                                &id,
                                &format!("daemon:effect-deadline:{}", latest.sequence),
                                RunCommand::ExpireDeadline,
                            )
                            .map(|receipt| receipt.snapshot);
                    }
                    super::timer::fire_due(&store, &latest, now_ms())?;
                    store.snapshot(&id)
                })
                .await;
                match fired {
                    Ok(Ok(latest)) => {
                        timer_changed |= latest.sequence != timer_snapshot.sequence;
                        super::api::emit_run_cursor(
                            &state,
                            &latest.project,
                            &latest.id,
                            latest.sequence,
                        );
                        timer_snapshot = latest;
                    }
                    Ok(Err(error)) => break Err(error),
                    Err(error) => break Err(format!("timer task failed: {error}")),
                }
            };
            match result {
                Ok(true) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Ok(false) => {}
                Err(error) if is_contention(&error) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(source = "workflows", %run_id, %error, "Workflow effect paused");
                    pause_failure(&state, &current.store, &run_id, &error);
                    if snapshot.status != RunStatus::Running {
                        tokio::select! {
                            _ = notify.notified() => {},
                            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {},
                        }
                    }
                    continue;
                }
            }
            // Keep pending ownership inspectable without replaying a paused command.
            if snapshot.status != RunStatus::Running {
                tokio::select! {
                    _ = notify.notified() => {},
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {},
                }
                continue;
            }
            // Yielding the deterministic turn never strands remaining runnable nodes.
            if deterministic_work_pending(&snapshot) {
                tokio::task::yield_now().await;
                continue;
            }
            let remaining = next_wake_ms(&snapshot).saturating_sub(now_ms()).max(0) as u64;
            tokio::select! {
                _ = notify.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(remaining)) => {},
            }
        } else if let Some(due) = super::timer::next_due(&snapshot) {
            let remaining = due.saturating_sub(now_ms()).max(0) as u64;
            tokio::select! {
                _ = notify.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(remaining)) => {},
            }
        } else {
            notify.notified().await;
        }
    }
    if let Some(owner) = owner.upgrade() {
        owner.actors.lock().remove(&run_id);
    }
}

fn next_wake_ms(snapshot: &RunSnapshot) -> i64 {
    super::timer::next_due(snapshot).map_or_else(
        || deadline_ms(snapshot),
        |due| due.min(deadline_ms(snapshot)),
    )
}

pub(super) fn deadline_ms(snapshot: &RunSnapshot) -> i64 {
    deadline_at(snapshot, now_ms())
}

pub(super) fn deadline_at(snapshot: &RunSnapshot, at_ms: i64) -> i64 {
    let pending = snapshot
        .paused_since_ms
        .map_or(0, |since| at_ms.saturating_sub(since).max(0));
    snapshot
        .started_ms
        .saturating_add(i64::from(snapshot.limits.max_duration_secs) * 1000)
        .saturating_add(snapshot.paused_duration_ms)
        .saturating_add(pending)
}

fn deterministic_work_pending(snapshot: &RunSnapshot) -> bool {
    snapshot
        .graph_executions
        .iter()
        .filter(|g| !g.completed)
        .any(|g| {
            g.activations
                .iter()
                .any(|a| a.state == ActivationState::Ready)
        })
}

/// Re-read after each expected-sequence commit; external effects stay outside transactions.
pub(super) fn drive_turn(store: &RunStore, run_id: &str) -> Result<RunSnapshot, String> {
    'turn: for _ in 0..TRANSITIONS_PER_TURN {
        let snapshot = store.snapshot(run_id)?;
        if super::timer::fire_due(store, &snapshot, now_ms())? {
            continue;
        }
        if snapshot.status != RunStatus::Running {
            return Ok(snapshot);
        }
        if now_ms() >= deadline_ms(&snapshot) {
            return Ok(store
                .command_expected(
                    run_id,
                    &format!("daemon:deadline:{}", snapshot.sequence),
                    snapshot.sequence,
                    RunCommand::ExpireDeadline,
                )?
                .snapshot);
        }
        if matches!(snapshot.root_target, Some(super::RunTarget::Plan(_)))
            && snapshot.graph_executions.iter().all(|g| g.completed)
        {
            return Ok(store
                .command_expected(
                    run_id,
                    "daemon:complete-plan",
                    snapshot.sequence,
                    RunCommand::Complete,
                )?
                .snapshot);
        }
        for graph in snapshot.graph_executions.iter().filter(|g| !g.completed) {
            'activations: for activation in graph
                .activations
                .iter()
                .filter(|a| matches!(a.state, ActivationState::Ready | ActivationState::Running))
            {
                let node = graph
                    .definition
                    .graph
                    .nodes
                    .iter()
                    .find(|node| node.id == activation.node_id)
                    .ok_or("activation node missing")?;
                let transition = if activation.state == ActivationState::Ready {
                    GraphTransition::Activate {
                        execution_id: graph.id.clone(),
                        activation_id: activation.id.clone(),
                    }
                } else {
                    let mut outcome = None;
                    let mut evidence = None;
                    match &node.kind {
                        NodeKind::CreateStories if snapshot.event_contract_version >= 3 => {
                            let Some(batch) =
                                super::proposals::batch_for(&snapshot, graph, activation)
                            else {
                                continue 'activations;
                            };
                            if crate::stories::StoryStore::open()?
                                .get_plan(&snapshot.plan_id)?
                                .revision
                                != batch.plan_revision
                            {
                                return Err("story proposal input has a stale plan revision".into());
                            }
                            if !super::proposals::complete(&snapshot, batch)? {
                                continue 'activations;
                            }
                            if snapshot.planning_fingerprint.is_none() {
                                store.command_expected(
                                    run_id,
                                    &format!(
                                        "daemon:close-plan:{}:{}",
                                        activation.id, snapshot.sequence
                                    ),
                                    snapshot.sequence,
                                    RunCommand::ClosePlanning,
                                )?;
                                continue 'turn;
                            }
                        }
                        NodeKind::ForEach {
                            receipt_evidence: Some(source),
                            ..
                        } => {
                            let source = graph
                                .activations
                                .iter()
                                .rev()
                                .find(|a| {
                                    a.node_id == *source
                                        && a.state == ActivationState::Completed
                                        && activation.tokens.starts_with(&a.tokens)
                                })
                                .ok_or("For each source receipt is not complete in this branch")?;
                            let report = effects::activation_attempt(&snapshot, graph, source)
                                .and_then(|a| a.report.as_ref())
                                .filter(|r| r.outcome == super::AttemptOutcome::Completed)
                                .ok_or("For each requires a completed typed source report")?;
                            store.command_expected(
                                run_id,
                                &format!("expand:{}:{}", activation.id, snapshot.sequence),
                                snapshot.sequence,
                                RunCommand::Graph {
                                    transition: GraphTransition::Expand {
                                        execution_id: graph.id.clone(),
                                        activation_id: activation.id.clone(),
                                        source_activation_id: source.id.clone(),
                                        items: report.evidence.clone(),
                                    },
                                },
                            )?;
                            continue 'turn;
                        }
                        NodeKind::Agent { .. } | NodeKind::CreateStories => {
                            let Some(attempt) =
                                effects::activation_attempt(&snapshot, graph, activation)
                            else {
                                if !activation.tokens.is_empty() {
                                    let waiting = snapshot
                                        .attempts
                                        .iter()
                                        .filter(|a| effects::attempt_holds_slot(&snapshot, a))
                                        .count()
                                        + snapshot
                                            .graph_executions
                                            .iter()
                                            .flat_map(|g| &g.activations)
                                            .filter(|a| a.resource_claim.is_some())
                                            .count()
                                        >= usize::from(snapshot.limits.max_parallel_stories);
                                    if waiting {
                                        if activation.resource_wait.is_none() {
                                            store.command_expected(run_id, &format!("resource-wait:{}:{}", activation.id, snapshot.sequence), snapshot.sequence,
                                            RunCommand::Graph { transition: GraphTransition::ResourceWait { execution_id: graph.id.clone(), activation_id: activation.id.clone(), reason: Some("Parallel activity capacity is occupied".into()) } })?;
                                            continue 'turn;
                                        }
                                        continue 'activations;
                                    }
                                    if activation.resource_wait.is_some() {
                                        store.command_expected(
                                            run_id,
                                            &format!(
                                                "resource-admitted:{}:{}",
                                                activation.id, snapshot.sequence
                                            ),
                                            snapshot.sequence,
                                            RunCommand::Graph {
                                                transition: GraphTransition::ResourceWait {
                                                    execution_id: graph.id.clone(),
                                                    activation_id: activation.id.clone(),
                                                    reason: None,
                                                },
                                            },
                                        )?;
                                        continue 'turn;
                                    }
                                }
                                store.command_expected(
                                    run_id,
                                    &format!(
                                        "daemon:{}:{}:attempt:{}",
                                        graph.id, activation.id, snapshot.sequence
                                    ),
                                    snapshot.sequence,
                                    RunCommand::StartGraphAgent {
                                        execution_id: graph.id.clone(),
                                        activation_id: activation.id.clone(),
                                    },
                                )?;
                                continue 'turn;
                            };
                            let native_approval = if matches!(
                                node.kind,
                                NodeKind::Agent {
                                    role: crate::workflows::AgentRole::Implementer,
                                    ..
                                }
                            ) && attempt.agent.is_none()
                                && attempt.report.is_none()
                                && !snapshot
                                    .effects
                                    .iter()
                                    .any(|effect| effect.key == format!("spawn:{}", attempt.id))
                            {
                                let story = crate::stories::StoryStore::open()?
                                    .get_story(&graph.target_id)?;
                                (story.status == crate::stories::StoryStatus::Done)
                                    .then_some(story.revision)
                            } else {
                                None
                            };
                            if let Some(revision) = native_approval {
                                if attempt.state != super::AttemptState::Reported {
                                    store.command_expected(
                                        run_id,
                                        &format!(
                                            "daemon:native-approval:{}:{revision}",
                                            attempt.id
                                        ),
                                        snapshot.sequence,
                                        RunCommand::AcceptUnspawnedStory {
                                            attempt_id: attempt.id.clone(),
                                        },
                                    )?;
                                    continue 'turn;
                                }
                                if !snapshot.stories.iter().any(|story| {
                                    story.story_id == graph.target_id
                                        && story.accepted
                                        && story.accepted_revision == Some(revision)
                                }) {
                                    return Err(
                                        "unspawned completion lacks native approval receipt".into(),
                                    );
                                }
                            }
                            if snapshot.event_contract_version >= 5
                                && attempt.outcome == Some(super::AttemptOutcome::NeedsInput)
                            {
                                continue 'activations;
                            }
                            if attempt.state == super::AttemptState::Running {
                                continue 'activations;
                            }
                            if let Some(token) = activation.tokens.last() {
                                if matches!(
                                    node.kind,
                                    NodeKind::Agent {
                                        role: crate::workflows::AgentRole::Reviewer,
                                        ..
                                    }
                                ) && judge::judge(&snapshot, graph, activation)?.0
                                    == super::graph::EdgeOutcome::No
                                {
                                    let token = activation
                                        .tokens
                                        .iter()
                                        .rev()
                                        .find(|token| {
                                            graph.forks.iter().any(|f| {
                                                f.id == token.fork_id && f.repair_to.is_some()
                                            })
                                        })
                                        .ok_or(
                                            "critic FIX has no published owning repair region",
                                        )?;
                                    store.command_expected(
                                        run_id,
                                        &format!(
                                            "supersede:{}:{}",
                                            token.fork_id, token.generation
                                        ),
                                        snapshot.sequence,
                                        RunCommand::Graph {
                                            transition: GraphTransition::SupersedeFork {
                                                execution_id: graph.id.clone(),
                                                fork_id: token.fork_id.clone(),
                                                generation: token.generation,
                                            },
                                        },
                                    )?;
                                    continue 'turn;
                                }
                                if matches!(
                                    attempt.outcome,
                                    Some(
                                        super::AttemptOutcome::Failed
                                            | super::AttemptOutcome::Interrupted
                                    )
                                ) {
                                    store.command_expected(
                                        run_id,
                                        &format!(
                                            "branch-failed:{}:{}",
                                            activation.id, snapshot.sequence
                                        ),
                                        snapshot.sequence,
                                        RunCommand::Graph {
                                            transition: GraphTransition::Arrive {
                                                execution_id: graph.id.clone(),
                                                activation_id: activation.id.clone(),
                                                token: token.clone(),
                                                outcome: super::parallel::ArrivalOutcome::Failed,
                                            },
                                        },
                                    )?;
                                    continue 'turn;
                                }
                            }
                            if (attempt.report.is_none() && native_approval.is_none())
                                || attempt.outcome != Some(super::AttemptOutcome::Completed)
                            {
                                return Ok(store
                                    .command_expected(
                                        run_id,
                                        &format!(
                                            "daemon:{}:{}:incomplete-report:{}",
                                            graph.id, activation.id, snapshot.sequence
                                        ),
                                        snapshot.sequence,
                                        RunCommand::Pause,
                                    )?
                                    .snapshot);
                            }
                            if matches!(node.kind, NodeKind::CreateStories)
                                && snapshot.planning_fingerprint.is_none()
                            {
                                store.command_expected(
                                    run_id,
                                    &format!(
                                        "daemon:close-plan:{}:{}",
                                        activation.id, snapshot.sequence
                                    ),
                                    snapshot.sequence,
                                    RunCommand::ClosePlanning,
                                )?;
                                continue 'turn;
                            }
                        }
                        NodeKind::Judge | NodeKind::Gate
                            if graph.definition.kind == crate::workflows::WorkflowKind::Plan =>
                        {
                            let decision = plan::judge(&snapshot, graph)?;
                            let Some(decision) = decision else {
                                continue 'activations;
                            };
                            outcome = Some(if matches!(node.kind, NodeKind::Gate) {
                                if decision.0 == super::graph::EdgeOutcome::Yes {
                                    super::graph::EdgeOutcome::Pass
                                } else {
                                    super::graph::EdgeOutcome::Fail
                                }
                            } else {
                                decision.0
                            });
                            evidence = Some(decision.1);
                        }
                        NodeKind::StoryDispatch { .. } => {
                            match plan::dispatch(store, &snapshot, graph, activation)? {
                                plan::Dispatch::Advanced => continue 'turn,
                                plan::Dispatch::Waiting => continue 'activations,
                                plan::Dispatch::Decided(decision) => {
                                    outcome = Some(decision.0);
                                    evidence = Some(decision.1);
                                }
                            }
                        }
                        NodeKind::Judge => {
                            if graph.definition.graph.evidence_policy.is_some()
                                && policy::gate_for_activation(&snapshot, graph, Some(activation))?
                                    .is_none()
                            {
                                continue 'activations;
                            }
                            let decision = judge::judge(&snapshot, graph, activation)?;
                            if decision.0 == super::graph::EdgeOutcome::Yes
                                && !policy::approved(&snapshot, graph)?
                            {
                                if policy::gate(&snapshot, graph)?
                                    .is_some_and(|d| d.0 == super::graph::EdgeOutcome::Fail)
                                {
                                    outcome = Some(super::graph::EdgeOutcome::Uncertain);
                                    evidence = Some(DecisionEvidence {
                                        actor: "daemon".into(),
                                        reason: "Pre-approval checks failed".into(),
                                        references: decision.1.references,
                                    });
                                } else {
                                    continue 'activations;
                                }
                            } else {
                                outcome = Some(decision.0);
                                evidence = Some(decision.1);
                            }
                        }
                        NodeKind::Gate => {
                            let Some(decision) =
                                policy::gate_for_activation(&snapshot, graph, Some(activation))?
                            else {
                                continue 'activations;
                            };
                            outcome = Some(decision.0);
                            evidence = Some(decision.1);
                        }
                        NodeKind::Pause { .. } => {
                            evidence = Some(DecisionEvidence {
                                actor: "daemon".into(),
                                reason: graph
                                    .decisions
                                    .last()
                                    .map(|d| d.evidence.reason.clone())
                                    .unwrap_or_else(|| {
                                        "Published graph pause requires an explicit resolution"
                                            .into()
                                    }),
                                references: vec![format!(
                                    "activation:{}:{}",
                                    graph.id, activation.id
                                )],
                            });
                        }
                        NodeKind::RunCommand { .. } | NodeKind::Tool { .. } => {
                            let key =
                                effects::command_identity(&snapshot, graph, activation).effect;
                            if !snapshot.effects.iter().any(|effect| {
                                effect.key == key
                                    && matches!(
                                        effect.state,
                                        super::EffectState::Succeeded | super::EffectState::Failed
                                    )
                            }) {
                                continue 'activations;
                            }
                            let Some(receipt) = store.command_journal().receipt(&key)? else {
                                continue 'activations;
                            };
                            outcome = Some(if receipt.succeeded {
                                super::graph::EdgeOutcome::Pass
                            } else {
                                super::graph::EdgeOutcome::Fail
                            });
                            evidence = Some(DecisionEvidence {
                                actor: "daemon:typed-command".into(),
                                reason: receipt.reason.clone(),
                                references: vec![format!(
                                    "command:{}:{}",
                                    key, receipt.output_sha256
                                )],
                            });
                        }
                        NodeKind::Notify => {
                            let key = format!("notify:{}:{}", graph.id, activation.id);
                            let Some(effect) = snapshot.effects.iter().find(|e| e.key == key)
                            else {
                                continue 'activations;
                            };
                            if effect.state != super::EffectState::Succeeded {
                                continue 'activations;
                            }
                        }
                        // End marks graph completion only; approval/integration policy still owns story delivery.
                        NodeKind::Start
                        | NodeKind::Loop { .. }
                        | NodeKind::Join {}
                        | NodeKind::End
                        | NodeKind::Fork { .. }
                        | NodeKind::ForEach { .. }
                        | NodeKind::AllJoin { .. }
                        | NodeKind::IfElse { .. } => {}
                    }
                    GraphTransition::Complete {
                        execution_id: graph.id.clone(),
                        activation_id: activation.id.clone(),
                        outcome,
                        evidence,
                    }
                };
                let key = format!(
                    "daemon:{}:{}:{:?}:{}",
                    graph.id, activation.id, activation.state, snapshot.sequence
                );
                store.command_expected(
                    run_id,
                    &key,
                    snapshot.sequence,
                    RunCommand::Graph { transition },
                )?;
                continue 'turn;
            }
        }
        return Ok(snapshot);
    }
    // Yield with a durable pending activation; the next turn is explicitly woken.
    store.snapshot(run_id)
}

fn pause_failure(state: &Arc<AppState>, store: &RunStore, run_id: &str, error: &str) {
    let Ok(snapshot) = store.snapshot(run_id) else {
        return;
    };
    if snapshot.status != RunStatus::Running {
        return;
    }
    match store.command(
        run_id,
        &format!("daemon:failure:{}", snapshot.sequence),
        RunCommand::PauseFor {
            cause: error.into(),
        },
    ) {
        Ok(receipt) => {
            emit_run_changed(state, &snapshot.project, run_id, receipt.sequence);
            if let Err(notice_error) = crate::mcp_http::mcp_transport::report_progress(
                state,
                Some(&snapshot.project),
                crate::progress::ProgressReportInput {
                    kind: crate::progress::ProgressKind::Blocked,
                    text: format!("Workflow {run_id} paused: {error}"),
                    step: Some("Workflow execution".into()),
                },
                Some("Workflow daemon".into()),
                None,
                None,
                None,
            ) {
                tracing::warn!(source = "workflows", %notice_error, "Workflow pause notice failed");
            }
        }
        Err(pause_error) => {
            tracing::warn!(source = "workflows", %pause_error, "Workflow failure pause failed")
        }
    }
}
