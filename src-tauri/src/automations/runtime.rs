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
    disabled: AtomicBool,
    owner: parking_lot::Mutex<Option<RunStore>>,
}

impl AutomationRuntime {
    /// Daemon option `--no-automations`: no schedule or event may start work.
    /// Must be called before `spawn`.
    pub(crate) fn disable(&self) {
        self.disabled.store(true, Ordering::Release);
    }

    pub(crate) fn spawn(state: &Arc<AppState>) {
        if state.automation_runtime.disabled.load(Ordering::Acquire) {
            tracing::info!(
                source = "automations",
                "Automations disabled by daemon option"
            );
            if !state
                .automation_runtime
                .started
                .swap(true, Ordering::AcqRel)
            {
                Self::settle_left_over_runs();
            }
            return;
        }
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
            let mut timer = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let scheduled_wake = tokio::select! {
                    _ = timer.tick() => true,
                    event = events.recv() => {
                        match event {
                            Ok(crate::state::AppEvent::AutomationRunChanged { .. }
                                | crate::state::AppEvent::ProgressRecorded { .. }
                                | crate::state::AppEvent::PtyExit { .. }
                                | crate::state::AppEvent::SessionClosed { .. }
                                | crate::state::AppEvent::SessionStateChanged { .. })
                            | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => false,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                            _ => continue,
                        }
                    }
                };
                let Some(state) = weak.upgrade() else {
                    return;
                };
                let reconcile_state = state.clone();
                let reconcile_store = owner.store().clone();
                match tokio::task::spawn_blocking(move || {
                    super::completion::reconcile(
                        &reconcile_store,
                        &super::completion::NativeCompletion(&reconcile_state),
                        chrono::Utc::now().timestamp_millis(),
                    )
                })
                .await
                {
                    Ok(Ok(())) => {}
                    error => tracing::warn!(
                        source = "automations",
                        ?error,
                        "Automation reconciliation failed"
                    ),
                }
                if !scheduled_wake {
                    continue;
                }
                {
                    let scheduler_state = state.clone();
                    match tokio::task::spawn_blocking(move || {
                        crate::workflows::scheduled_trigger::tick(
                            &scheduler_state,
                            chrono::Utc::now(),
                        )
                    })
                    .await
                    {
                        Ok(Ok(_)) => {}
                        error => tracing::warn!(
                            source = "automations",
                            ?error,
                            "Neutral scheduled trigger admission failed"
                        ),
                    }
                }
                let dispatcher = Arc::new(Dispatcher {
                    definitions: DefinitionStore::at_path(definitions.clone()),
                    runs: owner.store().clone(),
                    effects: NativeEffects(state),
                });
                let blocking = dispatcher.clone();
                let runs = match tokio::task::spawn_blocking(move || {
                    blocking.reserve_due(chrono::Utc::now())
                })
                .await
                {
                    Ok(Ok(runs)) => runs,
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

    /// A daemon restarted with `--no-automations` still owes the runs the previous
    /// process left open: owner acquisition settles Reserved/Running runs. Without a ledger there is
    /// nothing to settle, and none is created. No scheduler or intake starts.
    fn settle_left_over_runs() {
        let path = crate::config::config_dir().join("automation_runs.sqlite3");
        if !path.exists() {
            return;
        }
        tokio::spawn(async move {
            let now = chrono::Utc::now().timestamp_millis();
            // Acquisition settles the runs; the owner drops at once, so a disabled
            // worker never starves an enabled instance sharing this config dir.
            match tokio::task::spawn_blocking(move || RunOwner::acquire_at(&path, now)).await {
                Ok(Ok(_owner)) => {}
                error => {
                    tracing::warn!(source="automations", error=?error.map(|r| r.map(|_| ())), "Left-over automation runs not settled");
                }
            }
        });
    }

    pub(crate) fn notification_store(&self) -> Option<RunStore> {
        self.owner.lock().clone()
    }

    /// Step 8 exposes this same owner-guarded boundary through HTTP and IPC.
    pub(crate) async fn run_now(state: &Arc<AppState>, id: &str) -> Result<AutomationRun, String> {
        if state.automation_runtime.disabled.load(Ordering::Acquire) {
            return Err("Automations are disabled: this daemon runs with --no-automations".into());
        }
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

/// Resolve only registered, unambiguous github.com repositories through the same
/// account/binding rules as the poller; an unmatched allowlist must never be silent.
/// Registered checkouts bound to a GitHub cloud account, as `(owner/name, path)`.
pub(crate) fn resolved_github_repositories(state: &AppState) -> Vec<(String, String)> {
    use crate::github_account::{
        GitHubAccountRegistry, RepoBindingStore, RepoResolution, resolve_repo_account,
    };
    let registry = GitHubAccountRegistry::load();
    let bindings = RepoBindingStore::load();
    let default = crate::github::github_com_account(state);
    let mut resolved = Vec::new();
    for path in crate::config::registered_repo_paths() {
        if let RepoResolution::Bound {
            account,
            owner,
            repo,
        } = resolve_repo_account(
            std::path::Path::new(&path),
            &registry,
            &bindings,
            Some(&default),
        ) && account.host.is_cloud()
        {
            resolved.push((format!("{owner}/{repo}"), path));
        }
    }
    resolved
}
