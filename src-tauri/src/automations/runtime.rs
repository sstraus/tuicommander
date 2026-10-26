//! Machine-owned Once scheduling, independent of the WebView lifecycle.
use super::{
    definitions::DefinitionStore,
    dispatcher::{Dispatcher, native::NativeEffects},
    run::AutomationRun,
    scheduler::TICK_INTERVAL_SECS,
    store::{RunOwner, RunStore},
};
use crate::state::AppState;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
pub(crate) struct AutomationRuntime {
    started: AtomicBool,
    owner: parking_lot::Mutex<Option<RunStore>>,
}

impl AutomationRuntime {
    pub(crate) fn spawn(state: &Arc<AppState>) {
        if state
            .automation_runtime
            .started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let weak = Arc::downgrade(state);
        let path = crate::config::config_dir().join("automation_runs.sqlite3");
        let definitions = crate::config::config_dir().join("automations.json");
        tokio::spawn(async move {
            let owner = match tokio::task::spawn_blocking(move || {
                RunOwner::acquire_at(&path, chrono::Utc::now().timestamp_millis())
            })
            .await
            {
                Ok(Ok(owner)) => owner,
                error => {
                    // Do not expose prompts or output in runtime diagnostics.
                    tracing::warn!(source="automations", error=?error.map(|r| r.map(|_| ())), "Automation executor unavailable");
                    return;
                }
            };
            if let Some(state) = weak.upgrade() {
                *state.automation_runtime.owner.lock() = Some(owner.store().clone());
            } else {
                return;
            }
            let Some(state) = weak.upgrade() else { return };
            let mut events = state.event_bus.subscribe();
            drop(state);
            // The poller emits only on change; an issue refused for capacity is retried from here.
            let mut issues = super::event_intake::IssueSnapshot::default();
            let mut timer = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let (scheduled_wake, intake_wake) = tokio::select! {
                    _ = timer.tick() => (true, true),
                    event = events.recv() => {
                        match event {
                            Ok(crate::state::AppEvent::GitHubIssuesUpdate { repo_path, issues: update }) => {
                                issues.replace(repo_path, update);
                                (false, true)
                            }
                            Ok(crate::state::AppEvent::ProgressRecorded { .. }
                                | crate::state::AppEvent::PtyExit { .. }
                                | crate::state::AppEvent::SessionClosed { .. }
                                | crate::state::AppEvent::SessionStateChanged { .. }) => (false, false),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                // Issue updates may be among the lost events: drop the
                                // snapshot and ask the poller to emit everything again.
                                issues.lost_updates();
                                if let Some(state) = weak.upgrade()
                                    && let Err(error) = crate::github_poller::send_poller_cmd(&state, crate::github_poller::PollerCmd::ForceResync)
                                {
                                    tracing::warn!(source="automations", %error, "Issue resync after bus lag not requested");
                                }
                                (false, false)
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                            _ => continue,
                        }
                    }
                };
                let Some(state) = weak.upgrade() else {
                    return;
                };
                if let Err(error) = super::completion::reconcile(
                    owner.store(),
                    &super::completion::NativeCompletion(&state),
                    chrono::Utc::now().timestamp_millis(),
                ) {
                    tracing::warn!(source="automations", %error, "Automation reconciliation failed");
                }
                if !scheduled_wake && !intake_wake {
                    continue;
                }
                let dispatcher = Arc::new(Dispatcher {
                    definitions: DefinitionStore::at_path(definitions.clone()),
                    runs: owner.store().clone(),
                    effects: NativeEffects(state),
                });
                let blocking = dispatcher.clone();
                let snapshot = issues.issues();
                let runs = match tokio::task::spawn_blocking(move || {
                    // A failed half must not strand the other half's Reserved runs.
                    let now = chrono::Utc::now();
                    let mut runs = Vec::new();
                    let mut errors = Vec::new();
                    if intake_wake {
                        match blocking.admit_events(&snapshot, now) {
                            Ok(admitted) => runs.extend(admitted),
                            Err(error) => errors.push(error),
                        }
                    }
                    if scheduled_wake {
                        match blocking.reserve_due(now) {
                            Ok(due) => runs.extend(due),
                            Err(error) => errors.push(error),
                        }
                    }
                    (runs, errors)
                })
                .await
                {
                    Ok((runs, errors)) => {
                        for error in errors {
                            tracing::warn!(source="automations", %error, "Automation wake failed");
                        }
                        runs
                    }
                    error => {
                        tracing::warn!(source="automations", error=?error, "Automation wake failed");
                        continue;
                    }
                };
                for run in runs
                    .into_iter()
                    .filter(|run| run.status == super::run::RunStatus::Reserved)
                {
                    let dispatcher = dispatcher.clone();
                    tokio::spawn(async move {
                        if let Err(error) = dispatcher.dispatch(&run.id).await {
                            tracing::warn!(source="automations", %error, run_id=%run.id, "Automation dispatch persistence failed");
                        }
                    });
                }
            }
        });
    }

    pub(crate) fn notification_store(&self) -> Option<RunStore> {
        self.owner.lock().clone()
    }

    /// Step 8 exposes this same owner-guarded boundary through HTTP and IPC.
    pub(crate) async fn run_now(state: &Arc<AppState>, id: &str) -> Result<AutomationRun, String> {
        let runs = state
            .automation_runtime
            .owner
            .lock()
            .clone()
            .ok_or("Automation executor unavailable: this process is not the owner")?;
        Arc::new(Dispatcher {
            definitions: DefinitionStore::new(),
            runs,
            effects: NativeEffects(state.clone()),
        })
        .run_now(id)
        .await
    }
}
