//! Neutral schedules consume the existing automation tick, never another timer.
use super::{WorkflowStore, catalog::*, trigger::*};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriggerSchedule {
    pub cron: String,
    pub timezone: String,
    pub text: String,
}

pub(super) fn schema(kind: &str) -> Result<ObjectSchema, String> {
    if kind != "schedule" {
        return Err("unknown scheduled trigger kind".into());
    }
    Ok(ObjectSchema {
        fields: BTreeMap::from([
            (
                "text".into(),
                FieldSchema {
                    value_type: ValueType::String,
                    required: true,
                    default: None,
                },
            ),
            (
                "occurrence_ms".into(),
                FieldSchema {
                    value_type: ValueType::Integer,
                    required: true,
                    default: None,
                },
            ),
        ]),
    })
}

pub(super) fn validate(draft: &TriggerBindingDraft) -> Result<(), String> {
    if draft.source != TriggerSource::Scheduled {
        return if draft.schedule.is_none() {
            Ok(())
        } else {
            Err("schedule requires scheduled trigger source".into())
        };
    }
    let schedule = draft
        .schedule
        .as_ref()
        .ok_or("scheduled binding requires schedule")?;
    if draft.kind != "schedule"
        || draft.resource_prefix != format!("schedule:{}", draft.id)
        || draft.events != ["due".into()].into()
        || schedule.text.len() > 65536
    {
        return Err("invalid scheduled binding resource/event/text".into());
    }
    crate::automations::schedule::AutomationSchedule::parse(
        &schedule.cron,
        &schedule.timezone,
        None,
    )?;
    Ok(())
}

/// Deterministic wake entry shared by the automation runtime and acceptance.
/// The normal trigger ledger persists the occurrence, pinned start and run id.
pub(crate) fn tick(
    state: &Arc<AppState>,
    now: DateTime<Utc>,
) -> Result<Vec<TriggerAdmission>, String> {
    let store = WorkflowStore::open()?;
    let bindings = store.all_neutral_trigger_bindings()?;
    let mut result = Vec::new();
    for binding in bindings
        .into_iter()
        .filter(|b| b.enabled && b.draft.source == TriggerSource::Scheduled)
    {
        validate(&binding.draft)?;
        let schedule = binding
            .draft
            .schedule
            .as_ref()
            .ok_or("scheduled binding requires schedule")?;
        let parsed = crate::automations::schedule::AutomationSchedule::parse(
            &schedule.cron,
            &schedule.timezone,
            None,
        )?;
        let Some(occurrence) = parsed.latest_due(now, None)? else {
            continue;
        };
        let project = &binding.draft.publication.project;
        let host = state.workflow_runtime.neutral_catalog_host()?;
        let registry = host.registry();
        let catalog = store.build_project_catalog(project, &host.catalog(project)?, &registry)?;
        let event = TriggerEvent {
            source: TriggerSource::Scheduled,
            kind: "schedule".into(),
            resource: binding.draft.resource_prefix.clone(),
            generation: format!("{}:{}", binding.revision, occurrence.timestamp_millis()),
            event: "due".into(),
            delivery_id: None,
            schema: schema("schedule")?,
            values: BTreeMap::from([
                ("text".into(), json!(schedule.text)),
                ("occurrence_ms".into(), json!(occurrence.timestamp_millis())),
            ]),
            actor: None,
        };
        result.extend(state.workflow_runtime.admit_neutral_trigger(
            &store,
            &catalog,
            &registry.available_handlers(),
            project,
            event,
        )?);
    }
    Ok(result)
}
