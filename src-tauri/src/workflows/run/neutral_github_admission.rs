//! Complete source batches establish activation baselines before any run starts.
use super::{
    neutral_triggers::trigger_request,
    runtime::{RuntimeOwner, WorkflowRuntime},
};
use crate::workflows::{
    WorkflowStore,
    catalog::{HandlerRef, WorkflowCatalog},
    github_trigger::{GithubResource, GithubSnapshot, repository},
    trigger::*,
};
use std::collections::BTreeSet;

impl RuntimeOwner {
    #[allow(clippy::too_many_arguments)] // one source batch: store, catalog, project, repository, kind and snapshots.
    pub(super) fn admit_github_batch(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        repo: &str,
        kind: GithubResource,
        snapshots: Vec<GithubSnapshot>,
    ) -> Result<Vec<TriggerAdmission>, String> {
        let repo = repository(repo)?;
        let _serial = self.neutral_trigger_lock.lock();
        for snapshot in snapshots {
            if repository(&snapshot.repository)? != repo || snapshot.kind != kind {
                return Err("GitHub source batch contains another repository/kind".into());
            }
            self.store.normalize_github(snapshot, None)?;
        }
        let events = self.store.retained_github_events(&repo, kind, true)?;
        let kind_name = match kind {
            GithubResource::Issue => "issue",
            GithubResource::PullRequest => "pull_request",
        };
        for binding in workflows
            .stored_neutral_trigger_bindings(project)?
            .into_iter()
            .filter(|b| {
                b.enabled
                    && b.draft.source == TriggerSource::Github
                    && b.draft.kind == kind_name
                    && b.draft.resource_prefix == format!("github:{repo}/")
            })
        {
            if self
                .store
                .trigger_baseline_ready(project, &binding.draft.id, binding.revision)?
            {
                continue;
            }
            // Baseline every resource, including selectors that currently do
            // not auto-admit. Owner replay may explicitly select that snapshot.
            for event in &events {
                let request =
                    trigger_request(workflows, catalog, handlers, project, &binding, event)?;
                self.store
                    .baseline_trigger(project, &binding.draft.id, event.clone(), request)?;
            }
            // Readiness is last: a crash while retaining intents fails closed and
            // retries the baseline, never starts a partially captured backlog.
            self.store
                .complete_trigger_baseline(project, &binding.draft.id, binding.revision)?;
        }
        let mut admitted = Vec::new();
        for event in self.store.retained_github_events(&repo, kind, false)? {
            admitted
                .extend(self.admit_trigger_locked(workflows, catalog, handlers, project, event)?);
        }
        Ok(admitted)
    }
}
impl WorkflowRuntime {
    #[allow(clippy::too_many_arguments)] // mirrors the owner method it wraps.
    pub(crate) fn admit_github_batch(
        &self,
        workflows: &WorkflowStore,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
        project: &str,
        repo: &str,
        kind: GithubResource,
        snapshots: Vec<GithubSnapshot>,
    ) -> Result<Vec<TriggerAdmission>, String> {
        let owner = self.neutral_owner()?;
        let admitted = owner
            .admit_github_batch(workflows, catalog, handlers, project, repo, kind, snapshots)?;
        self.wake_trigger_runs(&owner)?;
        Ok(admitted)
    }
}
