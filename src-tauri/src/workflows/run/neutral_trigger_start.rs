//! A backend source retains Owner provenance before any source-started actor wakes.
use super::{neutral::*, neutral_execution::NeutralStatus, runtime::RuntimeOwner};
use crate::workflows::{WorkflowStore, catalog::*};
use std::collections::BTreeSet;

impl RuntimeOwner {
    pub(super) fn start_trigger(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        request: &NeutralStartRequest,
    ) -> Result<NeutralRun, String> {
        let _start = self.neutral_start_lock.lock();
        let state = self.state.lock().upgrade();
        let context = if let Some(state) = &state {
            let host = state.workflow_runtime.neutral_catalog_host()?;
            let closure = if let Some(run) = self
                .store
                .neutral_start_receipt(request, &content_digest(request)?)?
            {
                run.closure
            } else {
                RetainedClosure::resolve(workflows, &request.publication, catalog, handlers)?
            };
            super::neutral_native::capture(
                state,
                host.as_ref(),
                Some(super::NativeCaller::Owner),
                &request.publication.project,
                std::iter::once(&closure.root).chain(&closure.children),
                &closure.descriptors,
            )?
        } else {
            None
        };
        let run = self.start_neutral(workflows, request, catalog, handlers)?;
        if let (Some(state), Some(context)) = (&state, context) {
            state.workflow_runtime.neutral_catalog_host()?.bind_run(
                &self.store,
                &run.id,
                context,
            )?;
        }
        Ok(run)
    }
}

impl RuntimeOwner {
    /// A restart drops the volatile binding of a source-started run. The source
    /// retains Owner provenance, so its next admission pass restores it from the
    /// run's retained closure, as a repeated manual start does.
    ///
    /// Host unavailability or a failed list of the active runs in the trigger
    /// store fails the pass. One run that cannot be
    /// rebound (unreadable row, refused capture or binding) is logged and stays
    /// unbound; it must not block the other runs or any admission of the
    /// project. The next pass retries it.
    pub(super) fn rebind_trigger_runs(&self, project: &str) -> Result<(), String> {
        let Some(state) = self.state.lock().upgrade() else {
            return Ok(());
        };
        let host = state.workflow_runtime.neutral_catalog_host()?;
        for id in self.store.trigger_active_runs(project)? {
            if let Err(error) = self.rebind_trigger_run(&state, host.as_ref(), project, &id) {
                tracing::warn!(
                    source = "workflows",
                    project,
                    run = %id,
                    %error,
                    "Trigger run rebind failed; the run stays unbound and other admissions continue"
                );
            }
        }
        Ok(())
    }

    fn rebind_trigger_run(
        &self,
        state: &std::sync::Arc<crate::state::AppState>,
        host: &dyn crate::workflows::neutral_host::NeutralCatalogHost,
        project: &str,
        id: &str,
    ) -> Result<(), String> {
        if host.bound(id) || self.store.neutral_execution(id)?.status != NeutralStatus::Running {
            return Ok(());
        }
        let run = self.store.neutral_run(id)?;
        if let Some(context) = super::neutral_native::capture(
            state,
            host,
            Some(super::NativeCaller::Owner),
            project,
            std::iter::once(&run.closure.root).chain(&run.closure.children),
            &run.closure.descriptors,
        )? {
            host.bind_run(&self.store, id, context)?;
        }
        Ok(())
    }
}
