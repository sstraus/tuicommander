//! Shared HTTP, IPC, MCP and CLI boundary; all state belongs to the addressed backend.
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
    Templates {},
    Profiles {},
    Template {
        template_id: String,
        repository: String,
        run_config: String,
        timezone: Option<String>,
    },
    Get {
        id: String,
    },
    Create {
        definition: Value,
        #[serde(skip)]
        idempotent: bool,
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
    Run {
        id: String,
    },
    ListRuns {
        id: Option<String>,
        #[serde(default = "history_limit")]
        limit: u32,
        #[serde(default)]
        offset: u32,
        #[serde(default)]
        group_failures: bool,
    },
    WorktreePreview {
        id: String,
    },
    CleanupWorktree {
        id: String,
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
    execute_as(input, state, Caller::Client).await
}

pub(crate) enum Caller {
    Client,
    Agent(Option<String>),
}

impl Caller {
    fn creator(&self, action: &AutomationAction) -> Result<Option<&str>, String> {
        match self {
            Self::Agent(None) if matches!(action, AutomationAction::Create { .. }) => {
                Err("Automation creation requires a bound agent session".into())
            }
            Self::Agent(session) => Ok(session.as_deref()),
            Self::Client => Ok(None),
        }
    }
}

pub(crate) async fn execute_as(
    input: Value,
    state: Arc<AppState>,
    caller: Caller,
) -> Result<Value, String> {
    let reply = execute_action(input, state, caller).await?;
    // Whether an automation exists decides if MCP lists `automations`.
    crate::mcp_http::tool_usage::note_definitions_changed();
    Ok(reply)
}

async fn execute_action(
    input: Value,
    state: Arc<AppState>,
    caller: Caller,
) -> Result<Value, String> {
    let mut action = parse(input)?;
    caller.creator(&action)?;
    if let AutomationAction::Create {
        definition,
        idempotent,
    } = &mut action
        && let Caller::Agent(Some(creator)) = &caller
        && definition.get("id").is_none()
    {
        super::create::assign_retry_id(definition, creator)?;
        *idempotent = true;
    }
    if let AutomationAction::RunNow { id } = action {
        return value(AutomationRuntime::run_now(&state, &id).await?);
    }
    if matches!(
        action,
        AutomationAction::WorktreePreview { .. } | AutomationAction::CleanupWorktree { .. }
    ) {
        return tokio::task::spawn_blocking(move || {
            let (id, remove) = match action {
                AutomationAction::WorktreePreview { id } => (id, false),
                AutomationAction::CleanupWorktree { id } => (id, true),
                _ => unreachable!(),
            };
            super::worktrees::execute(&state, &RunStore::open()?, &id, remove)
        })
        .await
        .map_err(|error| format!("Automation cleanup task failed: {error}"))?;
    }
    tokio::task::spawn_blocking(move || {
        let definitions = DefinitionStore::new();
        if let AutomationAction::Create {
            definition,
            idempotent,
        } = &mut action
        {
            if *idempotent {
                // A retry returns the original occurrence even if caller cwd or
                // launch settings are no longer available to resolve defaults.
                if let Some(saved) = definitions
                    .load()?
                    .definitions
                    .into_iter()
                    .find(|saved| definition["id"].as_str() == Some(saved.id.as_str()))
                {
                    if saved.created_by_session.as_deref()
                        != match &caller {
                            Caller::Agent(session) => session.as_deref(),
                            Caller::Client => None,
                        }
                    {
                        return Err("Automation id already exists".into());
                    }
                    return create_reply(saved, Utc::now());
                }
            }
            super::create::apply_caller_defaults(definition, &state, &caller)?;
        }
        execute_stored_configured(
            &definitions,
            &RunStore::open()?,
            action,
            Utc::now(),
            caller,
            Some(&crate::config::load_agents_config()),
        )
    })
    .await
    .map_err(|error| format!("Automation task failed: {error}"))?
}

pub(crate) fn parse(mut input: Value) -> Result<AutomationAction, String> {
    // Create replies include a read-only next occurrence; round-tripping a saved
    // definition into update/preview must not persist that computed metadata.
    if let Some(definition) = input.get_mut("definition").and_then(Value::as_object_mut) {
        definition.remove("next_run_ms");
    }
    serde_json::from_value(input).map_err(|error| format!("Invalid automation action: {error}"))
}

#[cfg(test)]
pub(crate) fn execute_stored(
    definitions: &DefinitionStore,
    runs: &RunStore,
    action: AutomationAction,
    now: DateTime<Utc>,
) -> Result<Value, String> {
    execute_stored_as(definitions, runs, action, now, Caller::Client)
}

#[cfg(test)]
pub(crate) fn execute_stored_as(
    definitions: &DefinitionStore,
    runs: &RunStore,
    action: AutomationAction,
    now: DateTime<Utc>,
    caller: Caller,
) -> Result<Value, String> {
    execute_stored_configured(definitions, runs, action, now, caller, None)
}

/// Pure snapshot tests omit ambient agent settings; every live transport supplies them.
fn execute_stored_configured(
    definitions: &DefinitionStore,
    runs: &RunStore,
    action: AutomationAction,
    now: DateTime<Utc>,
    caller: Caller,
    profiles: Option<&crate::config::AgentsConfig>,
) -> Result<Value, String> {
    let creator = caller.creator(&action)?;
    let validate_profile = |definition: &AutomationDefinition| -> Result<(), String> {
        if let Some(profiles) = profiles {
            profiles.require_named_run_config(&definition.run_config)?;
        }
        Ok(())
    };
    match action {
        AutomationAction::List {} => {
            let mut items = Vec::new();
            let config = definitions.load()?;
            for definition in &config.definitions {
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
                let profile_error = profiles.and_then(|profiles| {
                    profiles
                        .require_named_run_config(&definition.run_config)
                        .err()
                });
                let next = next.filter(|_| profile_error.is_none());
                items.push(json!({"definition": definition, "next_run_ms": next.map(|t| t.timestamp_millis()), "last_status": last, "run_config_error": profile_error}));
            }
            Ok(Value::Array(items))
        }
        AutomationAction::Profiles {} => {
            let names: Vec<&str> = profiles
                .map(|config| {
                    config
                        .saved_run_configs()
                        .into_iter()
                        .map(|(_, profile)| profile.name.as_str())
                        .collect()
                })
                .unwrap_or_default();
            value(names)
        }
        AutomationAction::Templates {} => value(super::templates::TEMPLATES),
        AutomationAction::Template {
            template_id,
            repository,
            run_config,
            timezone,
        } => value(super::templates::draft(
            &template_id,
            repository,
            run_config,
            timezone.as_deref(),
        )?),
        AutomationAction::Get { id } => value(definitions.get(&id)?),
        AutomationAction::Create {
            definition,
            idempotent,
        } => {
            let definition = super::create::resolve(definition, now)?;
            validate_profile(&definition)?;
            let saved = actions::create_saved(definitions, definition, creator, idempotent)?;
            create_reply(saved, now)
        }
        AutomationAction::Update {
            id,
            definition,
            enabled,
        } => match (definition, enabled) {
            (Some(definition), None) => {
                validate_profile(&definition)?;
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
                if enabled {
                    validate_profile(&definitions.get(&id)?)?;
                }
                definitions.set_enabled(&id, enabled)?;
                value(definitions.get(&id)?)
            }
            _ => Err("Automation update requires exactly one of definition or enabled".into()),
        },
        AutomationAction::Pause { id } => {
            actions::execute(definitions, DefinitionAction::Pause { id }, None)
        }
        AutomationAction::Resume { id } => {
            validate_profile(&definitions.get(&id)?)?;
            actions::execute(definitions, DefinitionAction::Resume { id }, None)
        }
        AutomationAction::Delete { id } => {
            actions::execute(definitions, DefinitionAction::Delete { id }, None)
        }
        AutomationAction::Run { id } => value(runs.get(&id)?),
        AutomationAction::ListRuns {
            id,
            limit,
            offset,
            group_failures,
        } => {
            let mut listed = if group_failures {
                value(runs.grouped_history(id.as_deref(), limit, offset)?)?
            } else {
                value(runs.history(id.as_deref(), limit, offset)?)?
            };
            // Full output only through `run`; a page of six runs once weighed 1.1 MB.
            for run in listed.as_array_mut().into_iter().flatten() {
                for stream in ["stdout", "stderr"] {
                    excerpt(&mut run[stream]);
                }
            }
            Ok(listed)
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
        AutomationAction::WorktreePreview { .. } | AutomationAction::CleanupWorktree { .. } => {
            Err("Worktree cleanup requires local session state".into())
        }
        AutomationAction::RunNow { .. } => Err("Run Now requires the runtime owner".into()),
    }
}
fn create_reply(saved: AutomationDefinition, now: DateTime<Utc>) -> Result<Value, String> {
    let next = if saved.enabled {
        saved.schedule()?.next_after(now)?
    } else {
        None
    };
    let mut reply = value(&saved)?;
    reply["next_run_ms"] = json!(next.map(|time| time.timestamp_millis()));
    Ok(reply)
}

fn cursor(runs: &RunStore, id: &str) -> Result<Option<DateTime<Utc>>, String> {
    runs.scheduled_cursor(id)?
        .map(|millis| {
            DateTime::from_timestamp_millis(millis)
                .ok_or_else(|| "Invalid automation cursor timestamp".into())
        })
        .transpose()
}
/// Keep the tail of a saved output: the final report and error sit at the end.
fn excerpt(output: &mut Value) {
    let Some(text) = output["text"].as_str() else {
        return;
    };
    if text.len() <= LIST_OUTPUT_EXCERPT {
        return;
    }
    let mut start = text.len() - LIST_OUTPUT_EXCERPT;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    output["text"] = text[start..].into();
    output["truncated"] = true.into();
}
const LIST_OUTPUT_EXCERPT: usize = 2048;
fn value(input: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(input).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
