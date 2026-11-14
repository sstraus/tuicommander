//! Trigger configuration uses the domain store and existing runtime owner.
use super::model::{NeutralAction, NeutralReply};
use crate::state::AppState;
use crate::workflows::{
    WorkflowStore,
    catalog::{HandlerRef, WorkflowCatalog},
    trigger::{NeutralTriggerBinding, ReplayAuthority, TriggerSource},
};
use std::{collections::BTreeSet, sync::Arc};

/// A saved or toggled GitHub binding moves the machine-owned intake
/// subscription at once, through the same refresh as runtime start (#3056-4ff3).
/// The binding change is already committed, so a refused subscription is logged
/// exactly as at runtime start instead of failing the committed change.
fn refresh_github_intake(
    state: &Arc<AppState>,
    mut binding: NeutralTriggerBinding,
) -> NeutralTriggerBinding {
    if binding.draft.source == TriggerSource::Github {
        if let Err(error) = crate::workflows::github_poll::refresh(state) {
            tracing::warn!(source = "workflows", %error, "Neutral GitHub polling subscription refused");
        }
        crate::workflows::github_poll::report_intake_refusals(
            state,
            std::slice::from_mut(&mut binding),
        );
    }
    binding
}

pub(super) fn action(
    state: &Arc<AppState>,
    store: &WorkflowStore,
    project: &str,
    action: NeutralAction,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
) -> Result<NeutralReply, String> {
    match action {
        NeutralAction::SaveTriggerBinding {
            expected_revision,
            draft,
        } => store
            .save_neutral_trigger_binding(project, expected_revision, draft, catalog, handlers)
            .map(|binding| refresh_github_intake(state, binding))
            .map(NeutralReply::TriggerBinding),
        NeutralAction::EnableTriggerBinding {
            id,
            expected_revision,
            enabled,
            acknowledged_overlaps,
        } => store
            .enable_neutral_trigger_binding(
                project,
                &id,
                expected_revision,
                enabled,
                &acknowledged_overlaps,
                catalog,
                handlers,
            )
            .map(|binding| refresh_github_intake(state, binding))
            .map(NeutralReply::TriggerBinding),
        NeutralAction::PreviewTriggerMapping { draft, snapshot } => store
            .preview_neutral_trigger_mapping(project, &draft, &snapshot, catalog, handlers)
            .map(NeutralReply::TriggerPreview),
        NeutralAction::ReplayTrigger {
            binding_id,
            resource,
            generation,
            replay_id,
        } => {
            let authority = ReplayAuthority::authenticated_owner(project);
            state
                .workflow_runtime
                .replay_neutral_trigger(
                    store,
                    catalog,
                    handlers,
                    project,
                    &binding_id,
                    &resource,
                    &generation,
                    &replay_id,
                    &authority,
                )
                .map(NeutralReply::TriggerAdmission)
        }
        _ => Err("not a neutral trigger action".into()),
    }
}
