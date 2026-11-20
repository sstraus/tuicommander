//! One isolated world for one scenario execution: durable store, real owner,
//! real native host. `restart` replaces owner and host like a daemon restart.
use super::host::{Calls, CountingHost};
use super::run::native_host::NativeHost;
use super::run::neutral_execution::{NeutralExecution, NeutralStatus};
use super::runtime::RuntimeOwner;
use crate::state::AppState;
use std::any::Any;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Longest total wait for a run whose adapter calls keep running.
const IN_FLIGHT_LIMIT: Duration = Duration::from_secs(60);

/// Starts (or, after a restart, resumes) the run and returns its id. It must be idempotent.
pub(super) type StartFn = Box<dyn Fn(&Rig) -> String>;

pub(super) struct Rig {
    pub state: Arc<AppState>,
    pub db: PathBuf,
    pub project: String,
    pub owner: Arc<RuntimeOwner>,
    pub host: Arc<NativeHost>,
    pub calls: Arc<Calls>,
    pub start: StartFn,
    /// Keeps temp dirs and config guards alive for the whole execution.
    pub _keep: Vec<Box<dyn Any>>,
}

impl Rig {
    pub(super) fn attach(mut rig: Self) -> Self {
        rig.wire();
        rig
    }

    fn wire(&mut self) {
        self.owner.neutral_host(Arc::new(CountingHost {
            inner: self.host.clone(),
            calls: self.calls.clone(),
        }));
        *self.owner.state.lock() = Arc::downgrade(&self.state);
        *self.state.workflow_runtime.owner.lock() = Some(self.owner.clone());
    }

    /// Kill the owner, then boot a new one on the same database with a fresh host
    /// (a restart drops every in-memory caller binding).
    pub(super) async fn restart(&mut self) {
        let old: Weak<RuntimeOwner> = Arc::downgrade(&self.owner);
        *self.state.workflow_runtime.owner.lock() = None;
        self.release_old(old).await;
        self.host = NativeHost::install(&self.state).unwrap();
        self.owner = RuntimeOwner::acquire(&self.db).unwrap();
        self.wire();
    }

    async fn release_old(&mut self, old: Weak<RuntimeOwner>) {
        // Swap in a throwaway owner on a scratch database so the old one can drop.
        let scratch = self.db.with_file_name("scratch.sqlite3");
        self.owner = RuntimeOwner::acquire(&scratch).unwrap();
        for _ in 0..200 {
            if old.strong_count() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the old owner never released the run database");
    }

    pub(super) fn bound(&self, run: &str) -> bool {
        super::run::neutral_runtime::NeutralHost::bound(&*self.host, run)
    }

    /// What a resume by the trusted owner would do. Used only after a restart left the run unbound.
    pub(super) fn rebind(&self, run: &str) -> bool {
        super::run::NativeContext::capture(
            &self.state,
            super::run::NativeCaller::Owner,
            std::path::Path::new(&self.project),
            vec![],
        )
        .and_then(|context| {
            crate::workflows::neutral_host::NeutralCatalogHost::bind_run(
                &*self.host,
                &self.owner.store,
                run,
                context,
            )
        })
        .is_ok()
    }

    pub(super) fn wake(&self, run: &str) {
        self.state.workflow_runtime.wake(run);
    }

    /// The scripted operator answers every open approval; it never invents one.
    /// Returns the snapshot once the run is terminal or idle for `patience`.
    /// An adapter call still in flight is work, not idleness: only `patience` without an
    /// update and without a running call ends the wait (bounded by `IN_FLIGHT_LIMIT`).
    pub(super) async fn settle(&self, run: &str, patience: Duration) -> NeutralExecution {
        self.wait(run, patience, true).await
    }

    /// Like `settle`, but the operator stays silent: an open question remains open.
    pub(super) async fn settle_unanswered(
        &self,
        run: &str,
        patience: Duration,
    ) -> NeutralExecution {
        self.wait(run, patience, false).await
    }

    /// The operator answers the one open question of the run.
    pub(super) fn answer_open(&self, run: &str, approved: bool) {
        let snapshot = self.owner.store.neutral_execution(run).unwrap();
        let effect = snapshot
            .effects
            .values()
            .find(|e| e.question.is_some() && e.answer.is_none())
            .expect("an open question");
        let action = crate::workflows::neutral_api::model::NeutralAction::Answer {
            run_id: run.into(),
            command_id: format!("operator-test:{}:{approved}", effect.key.digest),
            expected_sequence: snapshot.sequence,
            key: effect.key.clone(),
            revision: effect.question.as_ref().unwrap().revision,
            approved,
        };
        crate::workflows::neutral_api::neutral_action(
            &self.state,
            &self.project,
            action,
            Some(super::run::NativeCaller::Owner),
        )
        .unwrap();
    }

    async fn wait(&self, run: &str, patience: Duration, answer: bool) -> NeutralExecution {
        let started = std::time::Instant::now();
        loop {
            let owner = self.owner.clone();
            let changed = owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let snapshot = owner.store.neutral_execution(run).unwrap();
            if snapshot.status != NeutralStatus::Running {
                return snapshot;
            }
            if let Some(effect) = snapshot
                .effects
                .values()
                .find(|e| e.question.is_some() && e.answer.is_none())
                .filter(|_| answer)
            {
                let action = crate::workflows::neutral_api::model::NeutralAction::Answer {
                    run_id: run.into(),
                    command_id: format!("operator:{}", effect.key.digest),
                    expected_sequence: snapshot.sequence,
                    key: effect.key.clone(),
                    revision: effect.question.as_ref().unwrap().revision,
                    approved: true,
                };
                // A frozen or racing commit fails here; the wait below retries.
                let _ = crate::workflows::neutral_api::neutral_action(
                    &self.state,
                    &self.project,
                    action,
                    Some(super::run::NativeCaller::Owner),
                );
            }
            if tokio::time::timeout(patience, changed).await.is_err() {
                let in_flight = !owner.neutral_effect_calls.lock().is_empty();
                if !in_flight || started.elapsed() > IN_FLIGHT_LIMIT {
                    return owner.store.neutral_execution(run).unwrap();
                }
            }
        }
    }
}
