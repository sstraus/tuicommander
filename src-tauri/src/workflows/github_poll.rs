//! A consumer of the existing GitHub poller. No timer or separate execution loop.
use super::{
    WorkflowStore,
    github_trigger::{GithubResource, repository},
    trigger::{NeutralTriggerBinding, TriggerSource},
};
use crate::{github_pr_api::RepoApi, state::AppState};
use std::{collections::BTreeSet, sync::Arc};

fn binding_repository(binding: &NeutralTriggerBinding) -> Result<String, String> {
    let prefix = binding
        .draft
        .resource_prefix
        .strip_prefix("github:")
        .ok_or("GitHub resource prefix must start github:")?;
    let mut parts = prefix.split('/');
    repository(&format!(
        "{}/{}",
        parts.next().unwrap_or(""),
        parts.next().unwrap_or("")
    ))
}

fn enabled_github(binding: &NeutralTriggerBinding) -> bool {
    binding.enabled && binding.draft.source == TriggerSource::Github
}

/// The registered path that subscribes an enabled GitHub binding for intake,
/// or the reason it cannot be subscribed.
fn intake_path(
    binding: &NeutralTriggerBinding,
    resolved: &[(String, String)],
) -> Result<String, String> {
    let repository = binding_repository(binding)?;
    resolved
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&repository))
        .map(|(_, path)| path.clone())
        .ok_or_else(|| {
            format!("GitHub repository {repository} has no registered unambiguous account path")
        })
}

/// One binding that cannot be subscribed is skipped and logged; it never
/// removes intake from the other bindings (#3059-ffa3).
pub(crate) fn refresh(state: &Arc<AppState>) -> Result<(), String> {
    let store = WorkflowStore::open()?;
    let resolved = crate::automations::runtime::resolved_github_repositories(state);
    let mut paths = BTreeSet::new();
    for binding in store
        .all_neutral_trigger_bindings()?
        .into_iter()
        .filter(enabled_github)
    {
        match intake_path(&binding, &resolved) {
            Ok(path) => {
                paths.insert(path);
            }
            Err(error) => tracing::warn!(
                source = "workflows",
                project = %binding.draft.publication.project,
                binding = %binding.draft.id,
                %error,
                "Neutral GitHub binding not subscribed for intake"
            ),
        }
    }
    crate::github_poller::set_neutral_intake_paths(state, paths.into_iter().collect())
}

/// Report each enabled GitHub binding that `refresh` cannot subscribe in its
/// `refusal_reason`, with the same predicate (#3059-ffa3).
pub(crate) fn report_intake_refusals(state: &AppState, bindings: &mut [NeutralTriggerBinding]) {
    if !bindings.iter().any(enabled_github) {
        return;
    }
    let resolved = crate::automations::runtime::resolved_github_repositories(state);
    for binding in bindings.iter_mut().filter(|b| enabled_github(b)) {
        if binding.refusal_reason.is_none() {
            binding.refusal_reason = intake_path(binding, &resolved)
                .err()
                .map(|error| format!("GitHub intake unavailable: {error}"));
        }
    }
}

pub(crate) async fn observe(
    state: &AppState,
    path: &str,
    kind: GithubResource,
) -> Result<(), String> {
    let store = WorkflowStore::open()?;
    let kind_name = match kind {
        GithubResource::Issue => "issue",
        GithubResource::PullRequest => "pull_request",
    };
    let bindings = store
        .all_neutral_trigger_bindings()?
        .into_iter()
        .filter(|b| {
            b.enabled && b.draft.source == TriggerSource::Github && b.draft.kind == kind_name
        })
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        return Ok(());
    }
    let host = state.workflow_runtime.neutral_catalog_host()?;
    let api = RepoApi::resolve(state, path).await?;
    let repository = format!("{}/{}", api.owner, api.repo).to_ascii_lowercase();
    let projects = bindings
        .iter()
        .filter_map(|b| match binding_repository(b) {
            Ok(repo) if repo == repository => Some(Ok(b.draft.publication.project.clone())),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<BTreeSet<_>, String>>()?;
    if projects.is_empty() {
        return Ok(());
    }
    let known = state
        .workflow_runtime
        .known_github_numbers(&repository, kind)?;
    let polled = super::github_event::poll_snapshots(&api, &repository, kind, &known).await?;
    for number in polled.unavailable {
        state
            .workflow_runtime
            .retire_github_source(&repository, kind, number)?;
    }
    for project in &projects {
        let registry = host.registry();
        let catalog = store.build_project_catalog(project, &host.catalog(project)?, &registry)?;
        state.workflow_runtime.admit_github_batch(
            &store,
            &catalog,
            &registry.available_handlers(),
            project,
            &repository,
            kind,
            polled.snapshots.clone(),
        )?;
    }
    Ok(())
}
