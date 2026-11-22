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
    Ok(admit_each(
        bindings
            .into_iter()
            .filter(|b| b.enabled && b.draft.source == TriggerSource::Scheduled),
        |binding| admit_binding(state, &store, binding, now),
        |binding, error| {
            tracing::warn!(
                source = "automations",
                project = %binding.draft.publication.project,
                binding = %binding.draft.id,
                %error,
                "Neutral scheduled binding admission failed; other bindings continue"
            )
        },
    ))
}

/// One binding's failure (for example a deleted project directory) must not
/// stop the admission of every other binding on the same tick.
fn admit_each<T, R>(
    items: impl IntoIterator<Item = T>,
    mut admit: impl FnMut(&T) -> Result<Vec<R>, String>,
    mut report: impl FnMut(&T, String),
) -> Vec<R> {
    let mut result = Vec::new();
    for item in items {
        match admit(&item) {
            Ok(admitted) => result.extend(admitted),
            Err(error) => report(&item, error),
        }
    }
    result
}

fn admit_binding(
    state: &Arc<AppState>,
    store: &WorkflowStore,
    binding: &NeutralTriggerBinding,
    now: DateTime<Utc>,
) -> Result<Vec<TriggerAdmission>, String> {
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
        return Ok(Vec::new());
    };
    let project = &binding.draft.publication.project;
    let host = state.workflow_runtime.neutral_catalog_host()?;
    let installed = host.catalog(project)?;
    // The registry is read after catalog(), which loads the installed handler
    // families; read first, a fresh host refuses every handler as unavailable.
    let registry = host.registry();
    let catalog = store.build_project_catalog(project, &installed, &registry)?;
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
    state.workflow_runtime.admit_neutral_trigger(
        store,
        &catalog,
        &registry.available_handlers(),
        project,
        event,
    )
}

#[cfg(test)]
mod admit_each_tests {
    use super::admit_each;

    // Catches: a tick that returns on the first failing binding, so a stale
    // binding over a deleted project silences every healthy schedule.
    #[test]
    fn a_failing_binding_does_not_stop_the_bindings_after_it() {
        let mut failures = Vec::new();
        let admitted = admit_each(
            ["deleted-project", "healthy", "also-healthy"],
            |item| match *item {
                "deleted-project" => Err("No such file or directory".to_string()),
                name => Ok(vec![name.to_string()]),
            },
            |item, error| failures.push(format!("{item}: {error}")),
        );
        assert_eq!(admitted, ["healthy", "also-healthy"]);
        assert_eq!(failures, ["deleted-project: No such file or directory"]);
    }
}
