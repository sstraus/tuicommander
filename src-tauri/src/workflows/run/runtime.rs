//! Daemon-owned neutral run ownership. Mailboxes wake actors; SQLite owns the work.
use super::RunStore;
use crate::state::AppState;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
#[cfg(test)]
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Notify;

/// Standby retry of the owner lock: first wait, then doubling to the cap.
const OWNER_RETRY_FIRST: std::time::Duration = std::time::Duration::from_millis(100);
const OWNER_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Default)]
pub(crate) struct WorkflowRuntime {
    started: AtomicBool,
    state: parking_lot::Mutex<Weak<AppState>>,
    owner: parking_lot::Mutex<Option<Arc<RuntimeOwner>>>,
    pub(super) neutral_catalog:
        parking_lot::Mutex<Option<Arc<dyn crate::workflows::neutral_host::NeutralCatalogHost>>>,
}

pub(super) struct RuntimeOwner {
    // Never unlink this file: another process must lock the same inode.
    _lock: File,
    pub(super) state: parking_lot::Mutex<Weak<AppState>>,
    pub(super) store: RunStore,
    actors: parking_lot::Mutex<HashMap<String, Arc<Notify>>>,
    pub(super) neutral_host:
        parking_lot::Mutex<Option<Arc<dyn super::neutral_runtime::NeutralHost>>>,
    pub(super) neutral_effect_calls: parking_lot::Mutex<std::collections::BTreeSet<String>>,
    pub(super) neutral_effect_updates: Notify,
    pub(super) neutral_trigger_lock: parking_lot::Mutex<()>,
    pub(super) neutral_start_lock: parking_lot::Mutex<()>,
}

impl RuntimeOwner {
    pub(super) fn wake_neutral_actors(&self) {
        for notify in self.actors.lock().values() {
            notify.notify_one();
        }
    }

    /// Tests take ownership directly; the daemon waits as a standby instead.
    #[cfg(test)]
    pub(super) fn acquire(path: &Path) -> Result<Arc<Self>, String> {
        Self::try_acquire(path)?.ok_or_else(|| {
            "workflow executor unavailable: another process owns the run database".into()
        })
    }

    /// `Ok(None)` while another process holds the owner lock.
    fn try_acquire(path: &std::path::Path) -> Result<Option<Arc<Self>>, String> {
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
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(format!("workflow executor unavailable: {e}"));
            }
        }
        // Ownership precedes even opening/recovering the store. Reads never recover.
        let store = RunStore::open_at(path)?;
        Ok(Some(Arc::new(Self {
            _lock: lock,
            state: Default::default(),
            store,
            actors: Default::default(),
            neutral_host: Default::default(),
            neutral_effect_calls: Default::default(),
            neutral_effect_updates: Notify::new(),
            neutral_trigger_lock: Default::default(),
            neutral_start_lock: Default::default(),
        })))
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
        if state.workflow_runtime.neutral_catalog_host().is_err()
            && let Err(error) = super::native_host::NativeHost::install(state)
        {
            tracing::warn!(source = "workflows", %error, "Native operation catalog unavailable");
        }
        crate::story_adapter::binding::spawn_poller(state.event_bus.clone());
        let weak = Arc::downgrade(state);
        *state.workflow_runtime.state.lock() = weak.clone();
        let path = crate::config::config_dir().join("workflow_runs.sqlite3");
        tokio::spawn(async move {
            // A restarted daemon can start while the previous process still
            // releases the owner lock, and a second daemon may own it for as
            // long as it runs: wait as a standby instead of giving up (#2831-954e).
            let mut delay = OWNER_RETRY_FIRST;
            let mut waiting = false;
            let owner = loop {
                let path = path.clone();
                match tokio::task::spawn_blocking(move || RuntimeOwner::try_acquire(&path)).await {
                    Ok(Ok(Some(owner))) => break owner,
                    Ok(Ok(None)) => {
                        if weak.strong_count() == 0 {
                            return;
                        }
                        if !std::mem::replace(&mut waiting, true) {
                            tracing::info!(
                                source = "workflows",
                                "Workflow executor standby: another process owns the run database"
                            );
                        }
                        tokio::time::sleep(delay).await;
                        delay = (delay * 2).min(OWNER_RETRY_MAX);
                    }
                    Ok(Err(error)) => {
                        tracing::warn!(source = "workflows", %error, "Workflow executor unavailable");
                        return;
                    }
                    Err(error) => {
                        tracing::error!(source = "workflows", %error, "Workflow executor startup failed");
                        return;
                    }
                }
            };
            let Some(state) = weak.upgrade() else {
                return;
            };
            let ids = match owner.store.neutral_recovery_run_ids() {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::error!(source = "workflows", %error, "Workflow recovery scan failed");
                    return;
                }
            };
            *owner.state.lock() = Arc::downgrade(&state);
            {
                let mut slot = state.workflow_runtime.owner.lock();
                *owner.neutral_host.lock() = state
                    .workflow_runtime
                    .neutral_catalog
                    .lock()
                    .clone()
                    .map(|host| host as Arc<dyn super::neutral_runtime::NeutralHost>);
                *slot = Some(owner);
            }
            if let Err(error) = crate::workflows::github_poll::refresh(&state) {
                tracing::warn!(source="workflows",%error,"Neutral GitHub polling subscription refused");
            }
            for id in ids {
                state.workflow_runtime.wake(&id);
            }
        });
    }

    /// Native services install one host for both transport authoring and execution.
    pub(crate) fn install_neutral_catalog_host(
        &self,
        host: Arc<dyn crate::workflows::neutral_host::NeutralCatalogHost>,
    ) {
        let owner = self.owner.lock();
        *self.neutral_catalog.lock() = Some(host.clone());
        if let Some(owner) = owner.as_ref() {
            *owner.neutral_host.lock() = Some(host);
            owner.wake_neutral_actors();
        }
    }

    pub(crate) fn neutral_catalog_host(
        &self,
    ) -> Result<Arc<dyn crate::workflows::neutral_host::NeutralCatalogHost>, String> {
        self.neutral_catalog
            .lock()
            .clone()
            .ok_or_else(|| "native catalog not installed".into())
    }

    #[cfg(test)]
    pub(super) fn attach_owner(&self, owner: Arc<RuntimeOwner>) {
        *self.owner.lock() = Some(owner);
    }

    pub(super) fn neutral_owner(&self) -> Result<Arc<RuntimeOwner>, String> {
        self.owner.lock().clone().ok_or_else(|| {
            "workflow executor unavailable: this daemon does not own the run database".into()
        })
    }

    /// Telegram callback access to the same owner used by daemon run commands.
    pub(crate) fn telegram_answer(
        &self,
        bot: &str,
        chat: i64,
        message: i64,
        token: &str,
    ) -> Result<bool, String> {
        self.neutral_owner()?
            .telegram_answer(bot, chat, message, token)
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
        tokio::spawn(run_actor(Arc::downgrade(&owner), run_id.to_owned(), notify));
    }
}

pub(super) async fn run_actor(owner: Weak<RuntimeOwner>, run_id: String, notify: Arc<Notify>) {
    super::neutral_runtime::actor(owner.clone(), &run_id, &notify).await;
    if let Some(current) = owner.upgrade() {
        current.actors.lock().remove(&run_id);
    }
}

#[cfg(test)]
#[path = "neutral_transport_tests.rs"]
mod neutral_transport_tests;

#[cfg(test)]
#[path = "neutral_transport_fixture.rs"]
mod neutral_transport_fixture;

#[cfg(test)]
#[path = "neutral_trigger_transport_tests.rs"]
mod neutral_trigger_transport_tests;

#[cfg(test)]
#[path = "native_public_tests.rs"]
mod native_public_tests;

#[cfg(test)]
#[path = "native_public_checks_tests.rs"]
mod native_public_checks_tests;
#[cfg(test)]
#[path = "native_public_http_tests.rs"]
mod native_public_http_tests;
#[cfg(test)]
#[path = "native_public_support.rs"]
mod native_public_support;

#[cfg(test)]
#[path = "native_agent_tests.rs"]
mod native_agent_tests;
#[cfg(test)]
#[path = "native_batch_tests.rs"]
mod native_batch_tests;
#[cfg(test)]
#[path = "native_stage_tests.rs"]
mod native_stage_tests;

#[cfg(test)]
#[path = "native_agent_tool_tests.rs"]
mod native_agent_tool_tests;

#[cfg(test)]
#[path = "parity_support.rs"]
mod parity_support;
#[cfg(test)]
#[path = "parity_tests.rs"]
mod parity_tests;

#[cfg(test)]
#[path = "correction_policy_tests.rs"]
mod correction_policy_tests;
#[cfg(test)]
#[path = "native_role_budget_tests.rs"]
mod native_role_budget_tests;

#[cfg(test)]
#[path = "parity_schedule_tests.rs"]
mod parity_schedule_tests;

#[cfg(test)]
#[path = "parity_github_live.rs"]
mod parity_github_live;
