//! Daemon-owned serial scheduling. Mailboxes wake actors; SQLite owns the work.
use super::graph::{ActivationState, DecisionEvidence, GraphTransition};
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
        let owner = self
            .owner
            .lock()
            .clone()
            .ok_or("workflow executor unavailable")?;
        let definition = crate::workflows::WorkflowStore::open()?
            .get_published(&request.definition_id, request.definition_revision)?;
        require_supported_nodes(&definition)?;
        let snapshot = owner.store.start_graph_run(request)?;
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
                if error.contains("sequence") || error.contains("preflight; retry") {
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
        if snapshot.status == RunStatus::Running {
            let Some(state) = state.upgrade() else {
                break;
            };
            let Some(current) = owner.upgrade() else {
                break;
            };
            let remaining = deadline_ms(&snapshot).saturating_sub(now_ms()).max(0) as u64;
            let result = tokio::select! {
                result = effects::drive_effect(&state, &current.store, &snapshot) => result,
                _ = tokio::time::sleep(std::time::Duration::from_millis(remaining)) => {
                    current.store.command(&run_id, &format!("daemon:effect-deadline:{}", snapshot.sequence), RunCommand::ExpireDeadline).map(|_| true)
                }
            };
            match result {
                Ok(true) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(source = "workflows", %run_id, %error, "Workflow effect paused");
                    pause_failure(&state, &current.store, &run_id, &error);
                    continue;
                }
            }
            // Yielding the deterministic turn never strands remaining runnable nodes.
            if deterministic_work_pending(&snapshot) {
                tokio::task::yield_now().await;
                continue;
            }
            let remaining = deadline_ms(&snapshot).saturating_sub(now_ms()).max(0) as u64;
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
        'graphs: for graph in snapshot.graph_executions.iter().filter(|g| !g.completed) {
            let Some(activation) = graph
                .activations
                .iter()
                .find(|a| matches!(a.state, ActivationState::Ready | ActivationState::Running))
            else {
                continue;
            };
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
                    NodeKind::Agent { .. } | NodeKind::CreateStories => {
                        let Some(attempt) =
                            effects::activation_attempt(&snapshot, graph, activation)
                        else {
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
                        if attempt.state == super::AttemptState::Running {
                            continue 'graphs;
                        }
                        if attempt.report.is_none()
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
                            continue 'graphs;
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
                            plan::Dispatch::Waiting => continue 'graphs,
                            plan::Dispatch::Decided(decision) => {
                                outcome = Some(decision.0);
                                evidence = Some(decision.1);
                            }
                        }
                    }
                    NodeKind::Judge => {
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
                                continue 'graphs;
                            }
                        } else {
                            outcome = Some(decision.0);
                            evidence = Some(decision.1);
                        }
                    }
                    NodeKind::Gate => {
                        let Some(decision) = policy::gate(&snapshot, graph)? else {
                            continue 'graphs;
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
                                    "Published graph pause requires an explicit resolution".into()
                                }),
                            references: vec![format!("activation:{}:{}", graph.id, activation.id)],
                        });
                    }
                    NodeKind::Notify => {
                        let key = format!("notify:{}:{}", graph.id, activation.id);
                        let Some(effect) = snapshot.effects.iter().find(|e| e.key == key) else {
                            continue 'graphs;
                        };
                        if effect.state != super::EffectState::Succeeded {
                            continue 'graphs;
                        }
                    }
                    // End marks graph completion only; approval/integration policy still owns story delivery.
                    NodeKind::Start | NodeKind::Loop { .. } | NodeKind::Join {} | NodeKind::End => {
                    }
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
