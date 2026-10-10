//! Shared HTTP and IPC boundary; all state belongs to the addressed backend.
use super::{
    actions::{self, DefinitionAction},
    definitions::DefinitionStore,
    model::AutomationDefinition,
    run::SummaryWindow,
    runtime::AutomationRuntime,
    schedule::{self, SchedulePreset},
    store::RunStore,
};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AutomationAction {
    List {},
    Get {
        id: String,
    },
    Create {
        definition: AutomationDefinition,
    },
    Update {
        id: Option<String>,
        definition: Option<AutomationDefinition>,
        enabled: Option<bool>,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Delete {
        id: String,
    },
    RunNow {
        id: String,
    },
    ListRuns {
        id: Option<String>,
        #[serde(default = "history_limit")]
        limit: u32,
        #[serde(default)]
        offset: u32,
    },
    Summary {
        window: SummaryWindow,
    },
    Preview {
        cron: String,
        timezone: Option<String>,
        #[serde(default = "preview_count")]
        count: usize,
    },
    PreviewDefinition {
        definition: AutomationDefinition,
        #[serde(default = "preview_count")]
        count: usize,
    },
    Preset {
        preset: SchedulePreset,
    },
}
fn history_limit() -> u32 {
    50
}
fn preview_count() -> usize {
    4
}

/// Parse the same input and return the same errors on either transport.
pub(crate) async fn execute(input: Value, state: Arc<AppState>) -> Result<Value, String> {
    let action = parse(input)?;
    if let AutomationAction::RunNow { id } = action {
        return value(AutomationRuntime::run_now(&state, &id).await?);
    }
    tokio::task::spawn_blocking(move || {
        execute_stored(
            &DefinitionStore::new(),
            &RunStore::open()?,
            action,
            Utc::now(),
        )
    })
    .await
    .map_err(|error| format!("Automation task failed: {error}"))?
}

pub(crate) fn parse(input: Value) -> Result<AutomationAction, String> {
    serde_json::from_value(input).map_err(|error| format!("Invalid automation action: {error}"))
}

pub(crate) fn execute_stored(
    definitions: &DefinitionStore,
    runs: &RunStore,
    action: AutomationAction,
    now: DateTime<Utc>,
) -> Result<Value, String> {
    match action {
        AutomationAction::List {} => {
            let mut items = Vec::new();
            for definition in definitions.load()?.definitions {
                let cursor = cursor(runs, &definition.id)?;
                let schedule = definition.schedule()?;
                let next = if definition.enabled && !schedule.is_completed(cursor) {
                    schedule.next_after(now.max(cursor.unwrap_or(now)))?
                } else {
                    None
                };
                let last = runs
                    .history(Some(&definition.id), 1, 0)?
                    .first()
                    .map(|run| run.status);
                items.push(json!({"definition": definition, "next_run_ms": next.map(|t| t.timestamp_millis()), "last_status": last}));
            }
            Ok(Value::Array(items))
        }
        AutomationAction::Get { id } => value(definitions.get(&id)?),
        AutomationAction::Create { definition } => {
            let id = definition.id.clone();
            actions::execute(definitions, DefinitionAction::Create { definition }, None)?;
            value(definitions.get(&id)?)
        }
        AutomationAction::Update {
            id,
            definition,
            enabled,
        } => match (definition, enabled) {
            (Some(definition), None) => {
                let id = id.unwrap_or_else(|| definition.id.clone());
                actions::execute(
                    definitions,
                    DefinitionAction::Update {
                        id: id.clone(),
                        definition,
                    },
                    None,
                )?;
                value(definitions.get(&id)?)
            }
            (None, Some(enabled)) => {
                let id = id.ok_or("Automation update requires id")?;
                definitions.set_enabled(&id, enabled)?;
                value(definitions.get(&id)?)
            }
            _ => Err("Automation update requires exactly one of definition or enabled".into()),
        },
        AutomationAction::Pause { id } => {
            actions::execute(definitions, DefinitionAction::Pause { id }, None)
        }
        AutomationAction::Resume { id } => {
            actions::execute(definitions, DefinitionAction::Resume { id }, None)
        }
        AutomationAction::Delete { id } => {
            actions::execute(definitions, DefinitionAction::Delete { id }, None)
        }
        AutomationAction::ListRuns { id, limit, offset } => {
            value(runs.history(id.as_deref(), limit, offset)?)
        }
        AutomationAction::Summary { window } => {
            value(runs.summary(window, now.timestamp_millis())?)
        }
        AutomationAction::Preview {
            cron,
            timezone,
            count,
        } => {
            let zone = schedule::timezone_for_creation(timezone.as_deref())?;
            value(schedule::preview(&cron, &zone, now, count)?)
        }
        AutomationAction::PreviewDefinition {
            mut definition,
            count,
        } => {
            if definition.timezone.is_empty() {
                definition.timezone = schedule::timezone_for_creation(None)?;
            }
            value(schedule::once::preview_definition(
                &definition,
                now,
                count,
                cursor(runs, &definition.id)?,
            )?)
        }
        AutomationAction::Preset { preset } => Ok(json!({"cron": preset.cron()?})),
        AutomationAction::RunNow { .. } => Err("Run Now requires the runtime owner".into()),
    }
}
fn cursor(runs: &RunStore, id: &str) -> Result<Option<DateTime<Utc>>, String> {
    runs.scheduled_cursor(id)?
        .map(|millis| {
            DateTime::from_timestamp_millis(millis)
                .ok_or_else(|| "Invalid automation cursor timestamp".into())
        })
        .transpose()
}
fn value(input: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(input).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
