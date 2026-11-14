//! One neutral workflow wire contract; transports do not own service authority.
pub(crate) mod drafts;
pub(crate) mod model;
pub(crate) mod templates;
mod triggers;
pub(crate) mod validation;
use super::WorkflowStore;
use super::catalog::WorkflowCatalog;
use super::neutral_admission::NeutralAdmissionService;
use crate::state::AppState;
use model::{CatalogView, NeutralAction, NeutralReply, PublicationView};
use std::sync::Arc;

use super::neutral_host::NeutralCatalogHost;

fn publication_list(
    store: &WorkflowStore,
    project: &str,
    catalog: &WorkflowCatalog,
    host: &dyn NeutralCatalogHost,
) -> Result<Vec<PublicationView>, String> {
    let conn = store.connect()?;
    let mut query=conn.prepare("SELECT id,revision FROM neutral_workflow_publications WHERE project=?1 ORDER BY id,revision").map_err(|e| e.to_string())?;
    let refs = query
        .query_map([project], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let handlers = host.registry().available_handlers();
    refs.into_iter()
        .map(|(id, revision)| {
            store
                .get_neutral_publication(project, &id, revision, catalog, &handlers)
                .and_then(|p| store.neutral_publication_view(p))
        })
        .collect()
}

pub(crate) fn catalog_reply(
    store: &WorkflowStore,
    project: &str,
    catalog: &WorkflowCatalog,
    host: &dyn NeutralCatalogHost,
) -> Result<NeutralReply, String> {
    Ok(NeutralReply::Catalog(CatalogView {
        activities: catalog.descriptors().cloned().collect(),
        metanodes: publication_list(store, project, catalog, host)?,
    }))
}

pub(crate) fn neutral_action(
    state: &Arc<AppState>,
    project: &str,
    action: NeutralAction,
    caller: Option<super::NativeCaller>,
) -> Result<NeutralReply, String> {
    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let project = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .into_owned();
    let store = WorkflowStore::open()?;
    if matches!(&action, NeutralAction::CreateDraft { .. }) {
        crate::mcp_http::tool_usage::note_definitions_changed();
    }
    match action {
        NeutralAction::TriggerSchema { source, kind } => {
            return super::trigger::trigger_schema(source, &kind).map(NeutralReply::Schema);
        }
        NeutralAction::Templates {} => {
            return templates::templates().map(NeutralReply::Templates);
        }
        NeutralAction::TriggerBindings {} => {
            let mut bindings = store.neutral_trigger_bindings(&project)?;
            super::github_poll::report_intake_refusals(state, &mut bindings);
            return Ok(NeutralReply::TriggerBindings(bindings));
        }
        NeutralAction::CreateDraft {
            name,
            definition,
            editor,
        } => {
            return store
                .create_neutral_draft(&project, name, definition, editor)
                .map(NeutralReply::Draft);
        }
        NeutralAction::ListDrafts {} => {
            return store
                .list_neutral_drafts(&project)
                .map(NeutralReply::Drafts);
        }
        NeutralAction::GetDraft { id } => {
            return store
                .get_neutral_draft(&project, &id)
                .map(NeutralReply::Draft);
        }
        NeutralAction::UpdateDraft {
            id,
            expected_revision,
            name,
            definition,
            editor,
        } => {
            return store
                .update_neutral_draft(&project, &id, expected_revision, name, definition, editor)
                .map(NeutralReply::Draft);
        }
        NeutralAction::Start { .. }
        | NeutralAction::StartDelivery { .. }
        | NeutralAction::DeliveryRuns { .. }
        | NeutralAction::Inspect { .. }
        | NeutralAction::Events { .. }
        | NeutralAction::Cancel { .. }
        | NeutralAction::Answer { .. }
        | NeutralAction::OperatorAnswer { .. } => {
            return super::run::neutral_api::run_action(state, &project, action, caller);
        }
        _ => {}
    }
    let host = state.workflow_runtime.neutral_catalog_host()?;
    let installed = host.catalog(&project)?;
    let registry = host.registry();
    let catalog = store.build_project_catalog(&project, &installed, &registry)?;
    let handlers = registry.available_handlers();
    match action {
        NeutralAction::CorrectionPolicyRegion { region } => {
            let definition = region.definition(&store, &project, &catalog, &handlers)?;
            Ok(NeutralReply::CorrectionRegion(
                model::CorrectionRegionView {
                    region: region.region,
                    definition,
                },
            ))
        }
        NeutralAction::CorrectionRegion { region } => correction_reply(region, &catalog),
        NeutralAction::SaveTriggerBinding { .. }
        | NeutralAction::EnableTriggerBinding { .. }
        | NeutralAction::PreviewTriggerMapping { .. }
        | NeutralAction::ReplayTrigger { .. } => {
            triggers::action(state, &store, &project, action, &catalog, &handlers)
        }
        NeutralAction::Catalog {} => catalog_reply(&store, &project, &catalog, host.as_ref()),
        NeutralAction::Validate { definition } => Ok(NeutralReply::Validation(
            validation::validate(&definition, &catalog, &handlers),
        )),
        NeutralAction::Preview {
            definition,
            inputs,
            subject,
        } => validation::preview(
            host.as_ref(),
            &project,
            definition,
            inputs,
            subject,
            &catalog,
            &handlers,
        )
        .map(NeutralReply::Preview),
        NeutralAction::Publish {
            id,
            expected_revision,
        } => {
            let draft = store.get_neutral_draft(&project, &id)?;
            if draft.revision != expected_revision {
                return Err("neutral draft revision changed".into());
            }
            let context = host.publication_context(&project)?;
            let publication = NeutralAdmissionService {
                store: &store,
                catalog: &catalog,
                registry: &registry,
            }
            .publish_draft(Some(context.as_ref()), &draft)?;
            crate::mcp_http::tool_usage::note_definitions_changed();
            store
                .neutral_publication_view(publication)
                .map(NeutralReply::Publication)
        }
        NeutralAction::GetPublication { id, revision } => store
            .get_neutral_publication(&project, &id, revision, &catalog, &handlers)
            .and_then(|p| store.neutral_publication_view(p))
            .map(NeutralReply::Publication),
        NeutralAction::ClonePublication { id, revision, name } => {
            let publication =
                store.get_neutral_publication(&project, &id, revision, &catalog, &handlers)?;
            let view = store.neutral_publication_view(publication)?;
            store
                .create_neutral_draft(&project, name, view.publication.definition, view.editor)
                .map(NeutralReply::Draft)
        }
        NeutralAction::SaveChildDispatch {
            cohort,
            expected_version,
            id,
            publication,
        } => {
            if publication.reference.project != project {
                return Err("cross-project child dispatch is not supported".into());
            }
            let child = store.get_neutral_publication(
                &project,
                &publication.reference.id,
                publication.reference.revision,
                &catalog,
                &handlers,
            )?;
            let descriptor =
                super::native_child::descriptor(id, publication, &child, &catalog, cohort)?;
            store.save_reusable_operation(
                &project,
                expected_version,
                descriptor.clone(),
                &catalog,
                &registry,
            )?;
            Ok(NeutralReply::Operation(descriptor))
        }
        NeutralAction::SaveOperation {
            expected_version,
            descriptor,
        } => {
            store.save_reusable_operation(
                &project,
                expected_version,
                descriptor.clone(),
                &catalog,
                &registry,
            )?;
            Ok(NeutralReply::Operation(descriptor))
        }
        NeutralAction::SavePreset {
            id,
            expected_revision,
            instance,
        } => store
            .save_activity_preset(
                &project,
                &id,
                expected_revision,
                instance,
                &catalog,
                &handlers,
            )
            .map(NeutralReply::Preset),
        NeutralAction::GetPreset { id, revision } => store
            .get_activity_preset(&project, &id, revision, &catalog, &handlers)
            .map(NeutralReply::Preset),
        _ => unreachable!("draft and run actions returned above"),
    }
}

pub(crate) fn correction_reply(
    region: super::correction_region::CorrectionRegion,
    catalog: &WorkflowCatalog,
) -> Result<NeutralReply, String> {
    let definition = region.definition(catalog)?;
    Ok(NeutralReply::CorrectionRegion(
        model::CorrectionRegionView { region, definition },
    ))
}
