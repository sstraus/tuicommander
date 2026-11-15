//! Source-neutral admission uses RuntimeOwner and its existing effect ledger.
use super::neutral::{NeutralRun, NeutralStartRequest};
use super::neutral_execution::{NeutralExecutionCommand, NeutralStatus};
use super::runtime::{RuntimeOwner, WorkflowRuntime};
use crate::workflows::{
    WorkflowStore,
    catalog::{HandlerRef, WorkflowCatalog},
    trigger::*,
};
use std::collections::BTreeSet;

impl RuntimeOwner {
    pub(super) fn admit_trigger(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        event: TriggerEvent,
    ) -> Result<Vec<TriggerAdmission>, String> {
        event.validate()?;
        let _serial = self.neutral_trigger_lock.lock();
        self.admit_trigger_locked(workflows, catalog, handlers, project, event)
    }
    pub(super) fn admit_trigger_locked(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        event: TriggerEvent,
    ) -> Result<Vec<TriggerAdmission>, String> {
        event.validate()?;
        self.flush_trigger_intents(workflows, catalog, handlers, project, false)?;
        let mut admitted = Vec::new();
        for binding in workflows
            .stored_neutral_trigger_bindings(project)?
            .into_iter()
            .filter(|b| b.enabled && matches(&b.draft, &event))
        {
            if binding.draft.source == TriggerSource::Github
                && !self.store.trigger_baseline_ready(
                    project,
                    &binding.draft.id,
                    binding.revision,
                )?
            {
                continue;
            }
            if self
                .store
                .trigger_suppressed(project, &binding.draft.id, &event)?
            {
                continue;
            }
            let request = trigger_request(workflows, catalog, handlers, project, &binding, &event)?;
            let intent = self.store.reserve_trigger(
                project,
                &binding.draft.id,
                event.clone(),
                request,
                "",
            )?;
            if intent.admission.disposition != "baseline" {
                admitted.push(intent);
            }
        }
        self.flush_trigger_intents(workflows, catalog, handlers, project, true)?;
        admitted
            .into_iter()
            .map(|i| {
                Ok(self
                    .store
                    .trigger_intent(&i.receipt)?
                    .ok_or("trigger receipt missing")?
                    .admission)
            })
            .collect()
    }
    #[allow(clippy::too_many_arguments)] // an exact replay names binding, resource, generation, identity and authority.
    pub(super) fn replay_trigger(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        binding: &str,
        resource: &str,
        generation: &str,
        replay_id: &str,
        authority: &ReplayAuthority,
    ) -> Result<Vec<TriggerAdmission>, String> {
        authority.require(project)?;
        if replay_id.is_empty() || replay_id.len() > 128 {
            return Err("replay requires bounded explicit identity".into());
        }
        let _serial = self.neutral_trigger_lock.lock();
        self.flush_trigger_intents(workflows, catalog, handlers, project, false)?;
        let original = self
            .store
            .trigger_original(project, binding, resource, generation)?;
        let intent = self.store.reserve_trigger(
            project,
            binding,
            original.event,
            original.request,
            replay_id,
        )?;
        self.flush_trigger_intents(workflows, catalog, handlers, project, true)?;
        Ok(vec![
            self.store
                .trigger_intent(&intent.receipt)?
                .ok_or("trigger receipt missing")?
                .admission,
        ])
    }
    fn flush_trigger_intents(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        supersede: bool,
    ) -> Result<Vec<NeutralRun>, String> {
        let mut started = Vec::new();
        for (active, intent) in self.store.trigger_pending(project)? {
            // A crash after run creation but before linking its receipt retries
            // the exact persisted start, never repins to today's binding.
            if let Some(active) = active {
                let mut snapshot = self.store.neutral_execution(&active)?;
                if snapshot.status == NeutralStatus::Running {
                    if !supersede {
                        continue;
                    }
                    loop {
                        match self.neutral_command(
                            &active,
                            &format!("trigger:superseded:{}", intent.receipt),
                            snapshot.sequence,
                            NeutralExecutionCommand::Cancel,
                        ) {
                            Err(e) if e.contains("sequence changed") => {
                                snapshot = self.store.neutral_execution(&active)?
                            }
                            result => {
                                snapshot = result?;
                                break;
                            }
                        }
                    }
                }
                // Effects are global across nested execution frames. Ended
                // frames cannot release a mutating resource before certainty+stop.
                if snapshot
                    .effects
                    .values()
                    .any(|e| e.dispatched && !e.settled())
                {
                    continue;
                }
            }
            let run = self.start_trigger(workflows, catalog, handlers, &intent.request)?;
            self.store.trigger_started(&intent, &run.id)?;
            started.push(run);
        }
        Ok(started)
    }
}

impl WorkflowRuntime {
    pub(crate) fn admit_neutral_trigger(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        event: TriggerEvent,
    ) -> Result<Vec<TriggerAdmission>, String> {
        let owner = self.neutral_owner()?;
        let result = owner.admit_trigger(workflows, catalog, handlers, project, event)?;
        self.wake_trigger_runs(&owner)?;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)] // mirrors the owner method it wraps.
    pub(crate) fn replay_neutral_trigger(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        binding: &str,
        resource: &str,
        generation: &str,
        replay_id: &str,
        authority: &ReplayAuthority,
    ) -> Result<Vec<TriggerAdmission>, String> {
        let owner = self.neutral_owner()?;
        let result = owner.replay_trigger(
            workflows, catalog, handlers, project, binding, resource, generation, replay_id,
            authority,
        )?;
        self.wake_trigger_runs(&owner)?;
        Ok(result)
    }
    pub(super) fn wake_trigger_runs(&self, owner: &RuntimeOwner) -> Result<(), String> {
        for run in owner.store.neutral_recovery_run_ids()? {
            self.wake(&run);
        }
        owner.wake_neutral_actors();
        Ok(())
    }
}

pub(super) fn trigger_request(
    workflows: &WorkflowStore,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
    project: &str,
    binding: &NeutralTriggerBinding,
    event: &TriggerEvent,
) -> Result<NeutralStartRequest, String> {
    let preview = workflows.preview_neutral_trigger_mapping(
        project,
        &binding.draft,
        event,
        catalog,
        handlers,
    )?;
    Ok(NeutralStartRequest {
        publication: binding.draft.publication.clone(),
        request_id: String::new(),
        inputs: preview.inputs,
        subject: preview.subject,
        limits: binding.draft.limits.clone(),
    })
}
