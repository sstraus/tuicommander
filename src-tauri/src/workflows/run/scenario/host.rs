//! Counting wrapper over the real native host. It adds no behavior: it records
//! every adapter entry and can drop the reply of one chosen entry (a lost response).
use super::run::neutral_effects::NeutralEffect;
use super::run::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use crate::workflows::neutral_admission::{HostContext, HostRegistry};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Debug)]
pub(super) struct Call {
    pub digest: String,
    pub mutation: bool,
}

/// Shared across restarts so the invariants see every adapter entry of the scenario.
#[derive(Default)]
pub(super) struct Calls {
    pub entries: parking_lot::Mutex<Vec<Call>>,
    /// 1-based adapter entry whose reply is dropped; 0 disables the fault.
    pub lose: AtomicUsize,
}

pub(super) struct CountingHost {
    pub inner: Arc<dyn NeutralHost>,
    pub calls: Arc<Calls>,
}

impl NeutralHost for CountingHost {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        self.inner.context(invocation)
    }
    fn registry(&self) -> HostRegistry<'_> {
        self.inner.registry()
    }
    fn prepare(
        &self,
        invocation: &NeutralInvocation,
    ) -> Result<super::run::neutral_effects::EffectPreparation, String> {
        self.inner.prepare(invocation)
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let index = {
            let mut entries = self.calls.entries.lock();
            entries.push(Call {
                digest: effect.key.digest.clone(),
                mutation: effect.preparation.mutation,
            });
            entries.len()
        };
        let inner = self.inner.invoke_effect(effect);
        if self.calls.lose.load(Ordering::Acquire) != index {
            return inner;
        }
        Box::pin(async move {
            // The adapter did its work; only the reply never arrives.
            let _ = inner.await;
            Err("scenario: adapter response lost".into())
        })
    }
    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        self.inner.reconcile(effect)
    }
    fn cancel(&self, effect: NeutralEffect) -> ObservationFuture {
        self.inner.cancel(effect)
    }
    fn original_stopped(&self, effect: &NeutralEffect) -> Result<(), String> {
        self.inner.original_stopped(effect)
    }
    fn forget_original(&self, effect: &NeutralEffect) {
        self.inner.forget_original(effect)
    }
    fn bound(&self, run: &str) -> bool {
        self.inner.bound(run)
    }
}
