//! The existing run actor's neutral branch. Retained preparation and approval
//! never replace current host admission. Reconciliation never invokes again.
use super::neutral_effects::{
    EffectCommand, EffectKey, EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect,
};
use super::neutral_execution::{
    NeutralAttemptFence, NeutralExecution, NeutralExecutionCommand as Command, NeutralStatus,
};
use super::runtime::RuntimeOwner;
use super::store::now_ms;
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance, content_digest};
use crate::workflows::neutral_admission::{HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::Value;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use tokio::sync::Notify;

pub(crate) type ObservationFuture =
    Pin<Box<dyn Future<Output = Result<EffectObservation, String>> + Send>>;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralInvocation {
    pub fence: NeutralAttemptFence,
    pub publication: NeutralPublication,
    pub descriptor: ActivityDescriptor,
    pub instance: ActivityInstance,
    pub inputs: BTreeMap<String, Value>,
    pub subject: Option<BTreeMap<String, Value>>,
}

/// Backend-installed service boundary. prepare resolves actual mutation and
/// resource semantics without performing the operation. Native adapters keep
/// their own permission/resource invariants in addition to this ledger.
pub(crate) trait NeutralHost: Send + Sync {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String>;
    fn registry(&self) -> HostRegistry<'_>;
    /// Authoring intent only. The returned value is never a dispatch permit.
    fn preview(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let context = self.context(invocation)?;
        let mut invocation = invocation.clone();
        invocation.instance = self.registry().admit_resolved(
            context.as_ref(),
            &invocation.publication,
            &invocation.descriptor,
            &invocation.instance,
            &invocation.inputs,
            invocation.subject.as_ref(),
        )?;
        self.prepare(&invocation)
    }
    /// Deterministic for the same resolved invocation, including its absolute
    /// approval deadline. Re-admission compares the retained preparation digest;
    /// do not regenerate a relative-to-now deadline when an approval resumes.
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String>;
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture;
    /// This may observe by retained handle or precommitted correlation. It must
    /// not dispatch or synthesize a new operation when the handle is missing.
    fn reconcile(&self, _effect: NeutralEffect) -> ObservationFuture {
        Box::pin(async { Err("neutral host cannot reconcile the original effect".into()) })
    }
    fn cancel(&self, _effect: NeutralEffect) -> ObservationFuture {
        Box::pin(async { Err("neutral host has no proven stop for the original effect".into()) })
    }
    /// False while the host lacks the caller binding of this run, as after a
    /// restart: the run then waits for a resume instead of failing its next step.
    fn bound(&self, _run: &str) -> bool {
        true
    }
}

pub(super) fn admitted(
    host: &dyn NeutralHost,
    snapshot: &NeutralExecution,
    invocation: &mut NeutralInvocation,
    parent: Option<&NeutralInvocation>,
) -> Result<(), String> {
    let registry = host.registry();
    let ancestors = snapshot.ancestor_invocations()?;
    let root = ancestors.first().unwrap_or(parent.unwrap_or(invocation));
    let mut context = host.context(root)?;
    for parent in &ancestors {
        context = registry.subflow_context(context.as_ref(), &parent.descriptor, &parent.inputs)?;
    }
    invocation.instance = registry.admit_resolved(
        context.as_ref(),
        &invocation.publication,
        &invocation.descriptor,
        &invocation.instance,
        &invocation.inputs,
        invocation.subject.as_ref(),
    )?;
    if matches!(
        invocation.descriptor.target,
        Some(crate::workflows::catalog::ReusableTarget::Subflow { .. })
    ) {
        registry.subflow_context(context.as_ref(), &invocation.descriptor, &invocation.inputs)?;
    }
    Ok(())
}

impl RuntimeOwner {
    // DEFERRED (2026-11-10) — native provider installation belongs to 008;
    // owned-protocol tests install the invocation port directly.
    #[cfg(test)]
    pub(super) fn neutral_host(&self, host: Arc<dyn NeutralHost>) {
        *self.neutral_host.lock() = Some(host);
    }

    /// Callback and approval entry point: exact command identity and sequence
    /// converge in the same IMMEDIATE transaction as graph decisions.
    pub(super) fn neutral_effect_command(
        &self,
        id: &str,
        command: EffectCommand,
    ) -> Result<NeutralExecution, String> {
        let command_id = format!("effect:{}", content_digest(&command)?);
        loop {
            let snapshot = self.store.neutral_execution(id)?;
            match self.neutral_command(
                id,
                &command_id,
                snapshot.sequence,
                Command::Effect {
                    command: command.clone(),
                },
            ) {
                Err(error) if error.contains("sequence changed") => continue,
                result => {
                    if result.is_ok() {
                        // Resource release can unblock a different run, while
                        // approval/callback wakes its branch in the same owner.
                        self.wake_neutral_actors();
                        self.neutral_effect_updates.notify_waiters();
                    }
                    return result;
                }
            }
        }
    }

    pub(super) fn drive_neutral_turn(&self, id: &str) -> Result<(NeutralExecution, bool), String> {
        for _ in 0..32 {
            let snapshot = self.store.neutral_execution(id)?;
            if snapshot.status != NeutralStatus::Running {
                return Ok((snapshot, false));
            }
            let result = self.neutral_command(
                id,
                &format!("daemon:neutral-step:{}", snapshot.sequence),
                snapshot.sequence,
                Command::Step,
            );
            let next = match result {
                Ok(next) => next,
                Err(error) if error.contains("sequence changed") => continue,
                Err(error) => return Err(error),
            };
            if next.sequence == snapshot.sequence {
                return Ok((next, false));
            }
        }
        self.store
            .neutral_execution(id)
            .map(|snapshot| (snapshot, true))
    }

    fn neutral_run_unbound(&self, id: &str) -> bool {
        self.neutral_host
            .lock()
            .as_ref()
            .is_some_and(|host| !host.bound(id))
    }

    fn fail_neutral_preparation(
        &self,
        snapshot: &NeutralExecution,
        fence: &NeutralAttemptFence,
        diagnostic: String,
    ) -> Result<bool, String> {
        self.neutral_command(
            &snapshot.run.id,
            &format!("daemon:neutral-preparation-refused:{}", fence.activation),
            snapshot.sequence,
            Command::Fail {
                fence: fence.clone(),
                diagnostic,
            },
        )?;
        Ok(true)
    }

    pub(super) fn dispatch_neutral(
        self: &Arc<Self>,
        snapshot: &NeutralExecution,
        notify: &Arc<Notify>,
    ) -> Result<bool, String> {
        let host = self.neutral_host.lock().clone();
        for (activity, attempt) in snapshot.pending_invocations() {
            let mut invocation = NeutralInvocation {
                fence: attempt.fence.clone(),
                publication: snapshot.publication.clone(),
                descriptor: snapshot.descriptor(activity)?.clone(),
                instance: snapshot.publication.definition.instances[activity].clone(),
                inputs: attempt.inputs.clone(),
                subject: attempt.subject.clone(),
            };
            let existing = snapshot.effect_for(&attempt.fence);
            // A question belongs only to this activation. Try siblings rather
            // than letting the first waiting slot starve independent diagnosis.
            if existing.is_some_and(|effect| !effect.approved()) {
                continue;
            }
            let admission = host
                .as_ref()
                .ok_or_else(|| "neutral host provider unavailable".to_owned())
                .and_then(|host| admitted(host.as_ref(), snapshot, &mut invocation, None));
            if let Err(diagnostic) = admission {
                // No adapter call took place. An existing approved effect still
                // has no claim and must never use its old approval as authority.
                if existing.is_some() {
                    continue;
                }
                self.neutral_command(
                    &snapshot.run.id,
                    &format!("daemon:neutral-refused:{}", attempt.fence.activation),
                    snapshot.sequence,
                    Command::Fail {
                        fence: attempt.fence.clone(),
                        diagnostic,
                    },
                )?;
                return Ok(true);
            }
            if matches!(
                invocation.descriptor.target,
                Some(crate::workflows::catalog::ReusableTarget::Subflow { .. })
            ) {
                self.neutral_command(
                    &snapshot.run.id,
                    &format!("daemon:neutral-subflow:{}", attempt.fence.activation),
                    snapshot.sequence,
                    Command::EnterSubflow {
                        fence: attempt.fence.clone(),
                    },
                )?;
                return Ok(true);
            }
            if !snapshot.dispatch_capacity() {
                continue;
            }
            let host = host.as_ref().expect("admitted provider");
            let prepared = host
                .prepare(&invocation)
                .and_then(|preparation| NeutralEffect::prepare(invocation, preparation));
            let prepared = match prepared {
                Ok(prepared) => prepared,
                Err(diagnostic) if existing.is_none() => {
                    return self.fail_neutral_preparation(snapshot, &attempt.fence, diagnostic);
                }
                // A changed or unavailable re-preparation never grants authority,
                // and cannot prevent independent candidates from being examined.
                Err(_) => continue,
            };
            let (effect, sequence, newly_prepared) = if let Some(effect) = existing {
                if effect.key != prepared.key {
                    continue;
                }
                (effect.clone(), snapshot.sequence, false)
            } else {
                let retained = match self.neutral_effect_command(
                    &snapshot.run.id,
                    EffectCommand::Prepare {
                        effect: Box::new(prepared.clone()),
                    },
                ) {
                    Ok(retained) => retained,
                    Err(diagnostic) => {
                        return self.fail_neutral_preparation(snapshot, &attempt.fence, diagnostic);
                    }
                };
                if !prepared.approved() {
                    return Ok(true);
                }
                (prepared, retained.sequence, true)
            };
            // Current permission and current resolved resource/approval policy
            // are checked above on every dispatch, including resumed approval.
            let claimed = match self.neutral_command(
                &snapshot.run.id,
                &format!("effect-dispatch:{}", effect.key.digest),
                sequence,
                Command::Effect {
                    command: EffectCommand::Dispatch {
                        key: effect.key.clone(),
                    },
                },
            ) {
                Err(error) if error.contains("resource conflict") => {
                    if newly_prepared {
                        return Ok(true);
                    }
                    continue;
                }
                Err(error) => return Err(error),
                Ok(claimed) => claimed,
            };
            let effect = claimed.effect(&effect.key)?.clone();
            if !effect.dispatched {
                // Expired approval settled this branch without adapter entry.
                return Ok(true);
            }
            let call = format!("invoke:{}", effect.key.digest);
            self.neutral_effect_calls.lock().insert(call.clone());
            let future = host.invoke_effect(effect.clone());
            self.observe_neutral_future(
                snapshot.run.id.clone(),
                effect.key.clone(),
                call,
                future,
                notify.clone(),
            );
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn observe_neutral_future(
        self: &Arc<Self>,
        run: String,
        key: EffectKey,
        call: String,
        future: ObservationFuture,
        notify: Arc<Notify>,
    ) {
        let owner = Arc::downgrade(self);
        tokio::spawn(async move {
            let observation = future.await.unwrap_or_else(|diagnostic| EffectObservation {
                outcome: EffectOutcome::Uncertain { diagnostic },
                stopped: false,
            });
            if let Some(owner) = owner.upgrade() {
                if let Err(error) = owner.neutral_effect_command(
                    &run,
                    EffectCommand::Observe {
                        key: key.clone(),
                        observation,
                    },
                ) {
                    // Malformed/lost completion is not proof that a write did
                    // not occur. Keep ownership and an explicit uncertainty.
                    let uncertain = EffectObservation {
                        outcome: EffectOutcome::Uncertain {
                            diagnostic: format!("neutral effect observation refused: {error}"),
                        },
                        stopped: false,
                    };
                    if let Err(error) = owner.neutral_effect_command(
                        &run,
                        EffectCommand::Observe {
                            key,
                            observation: uncertain,
                        },
                    ) {
                        tracing::warn!(source = "workflows", %run, %error, "Neutral observation unavailable");
                    }
                }
                owner.neutral_effect_calls.lock().remove(&call);
            }
            notify.notify_one();
        });
    }

    /// One observation per effect per actor lifetime, plus an independently
    /// claimed cancellation. This is not a retry loop or a second scheduler.
    fn recover_neutral_effects(
        self: &Arc<Self>,
        snapshot: &NeutralExecution,
        notify: &Arc<Notify>,
        seen: &mut Vec<EffectKey>,
    ) -> Result<bool, String> {
        let Some(host) = self.neutral_host.lock().clone() else {
            return Ok(false);
        };
        for effect in snapshot
            .effects
            .values()
            .filter(|effect| effect.dispatched && !effect.settled())
        {
            if effect.invocation.descriptor.handler.name == super::native_issue_publication::HANDLER
                && matches!(
                    &effect.observation,
                    Some(EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Uncertain { .. }
                    })
                )
            {
                // The graph retains a separate disposition; the original unknown
                // is neither retried nor relabelled by generic recovery.
                continue;
            }
            // Parent recovery may issue a new model prompt only after original
            // tool effects reconcile. Do not mark it seen before those settle.
            if effect.preparation.model_tools.is_some()
                && !snapshot.owned_tools_settled(&effect.key, None)
                && snapshot.status == NeutralStatus::Running
            {
                continue;
            }
            let ended = snapshot.status != NeutralStatus::Running;
            if ended && !effect.cancel_started {
                self.neutral_effect_command(
                    &snapshot.run.id,
                    EffectCommand::Cancel {
                        key: effect.key.clone(),
                    },
                )?;
                let call = format!("cancel:{}", effect.key.digest);
                self.neutral_effect_calls.lock().insert(call.clone());
                let mut effect = effect.clone();
                effect.cancel_started = true;
                let future = host.cancel(effect.clone());
                self.observe_neutral_future(
                    snapshot.run.id.clone(),
                    effect.key,
                    call,
                    future,
                    notify.clone(),
                );
                return Ok(true);
            }
            if self
                .neutral_effect_calls
                .lock()
                .iter()
                .any(|call| call.ends_with(&effect.key.digest))
            {
                continue;
            }
            if !seen.contains(&effect.key) {
                seen.push(effect.key.clone());
                let call = format!("reconcile:{}", effect.key.digest);
                self.neutral_effect_calls.lock().insert(call.clone());
                let future = host.reconcile(effect.clone());
                self.observe_neutral_future(
                    snapshot.run.id.clone(),
                    effect.key.clone(),
                    call,
                    future,
                    notify.clone(),
                );
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub(super) async fn actor(owner: Weak<RuntimeOwner>, id: &str, notify: &Arc<Notify>) {
    let mut observed = Vec::new();
    loop {
        let Some(current) = owner.upgrade() else {
            return;
        };
        let (snapshot, yielded) = match current.drive_neutral_turn(id) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!(source = "workflows", %id, %error, "Neutral history unavailable");
                drop(current);
                notify.notified().await;
                continue;
            }
        };
        if snapshot.status == NeutralStatus::Running && current.neutral_run_unbound(id) {
            // Caller provenance is volatile and a restart drops it. A resume
            // (the same start request) rebinds it and wakes this actor; until
            // then nothing is admitted, reconciled or failed.
            drop(current);
            notify.notified().await;
            continue;
        }
        match current.recover_neutral_effects(&snapshot, notify, &mut observed) {
            Ok(true) => {
                tokio::task::yield_now().await;
                continue;
            }
            Err(error) => {
                tracing::warn!(source = "workflows", %id, %error, "Neutral reconciliation unavailable");
            }
            _ => {}
        }
        if snapshot.status != NeutralStatus::Running {
            // Retained claims survive actor exit and restart. A later callback
            // may settle them but can never reopen this graph.
            return;
        }
        if yielded {
            tokio::task::yield_now().await;
            continue;
        }
        match current.dispatch_neutral(&snapshot, notify) {
            Ok(true) => {
                match current.store.neutral_execution(id) {
                    Ok(retained) => {
                        for (_, attempt) in snapshot.pending_invocations() {
                            if let Some(effect) = retained.effect_for(&attempt.fence)
                                && effect.dispatched
                                && !observed.contains(&effect.key)
                            {
                                observed.push(effect.key.clone());
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(source = "workflows", %id, %error, "Neutral dispatch observation unavailable")
                    }
                }
                tokio::task::yield_now().await;
                continue;
            }
            Err(error) if error.contains("sequence changed") => continue,
            Err(error) => {
                tracing::warn!(source = "workflows", %id, %error, "Neutral dispatch failed");
            }
            Ok(false) => {}
        }
        let due = snapshot
            .wait_due_ms
            .unwrap_or(snapshot.deadline_ms)
            .min(snapshot.deadline_ms);
        let remaining = due.saturating_sub(now_ms()).max(0) as u64;
        drop(current);
        tokio::select! {
            _ = notify.notified() => {},
            _ = tokio::time::sleep(std::time::Duration::from_millis(remaining)) => {},
        }
    }
}
