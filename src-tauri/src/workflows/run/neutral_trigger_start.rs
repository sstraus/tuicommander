//! A backend source retains Owner provenance before any source-started actor wakes.
use super::{neutral::*, runtime::RuntimeOwner};
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
