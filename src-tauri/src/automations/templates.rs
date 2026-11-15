//! Built-in read-only draft recipes. No persistence or launch effects.
use super::{
    model::{AutomationDefinition, Overlap, Workspace},
    schedule,
};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Template {
    pub id: &'static str,
    pub name: &'static str,
    cron: &'static str,
    prompt: &'static str,
}

pub(crate) const TEMPLATES: &[Template] = &[
    Template {
        id: "weekday_audit",
        name: "Weekday repository audit",
        cron: "0 9 * * 1-5",
        prompt: "Audit this repository for concrete correctness, security and reliability issues. Read the project instructions first. Report findings with file references, impact and suggested fixes. Do not modify files, commit, push or publish anything.",
    },
    Template {
        id: "release_readiness",
        name: "Release readiness",
        cron: "0 9 * * 1",
        prompt: "Review this repository for release readiness. Read the project instructions, inspect the working tree, recent changes and release documentation. Report blockers, missing validation and documentation gaps. Do not modify files, commit, push, tag or publish a release.",
    },
    Template {
        id: "daily_review",
        name: "Daily change review",
        cron: "0 9 * * *",
        prompt: "Review this repository's changes from the last 24 hours and its current working tree. Read the project instructions first. Summarize changes, flag concrete risks with file references, and state when there are no changes. Do not modify files, commit, push or publish anything.",
    },
    Template {
        id: "hourly_maintenance",
        name: "Hourly maintenance review",
        cron: "0 * * * *",
        prompt: "Inspect this repository for maintenance needs. Read the project instructions first. Report actionable build, dependency and documentation concerns, or state that no action is needed. Do not install dependencies, modify files, commit, push or publish anything.",
    },
];

pub(crate) fn draft(
    template_id: &str,
    repository: String,
    run_config: String,
    timezone: Option<&str>,
) -> Result<AutomationDefinition, String> {
    let template = TEMPLATES
        .iter()
        .find(|item| item.id == template_id)
        .ok_or("Unknown automation template")?;
    let definition = AutomationDefinition {
        id: uuid::Uuid::new_v4().to_string(),
        name: template.name.into(),
        prompt: template.prompt.into(),
        repository,
        run_config,
        timezone: schedule::timezone_for_creation(timezone)?,
        workspace: Workspace::Existing,
        cron: template.cron.into(),
        once_local: None,
        once_at: None,
        enabled: false,
        grace_secs: 43200,
        overlap: Overlap::Skip,
        max_duration_secs: 3600,
        close_on_finish: false,
        precheck: None,
        created_by_session: None,
    };
    definition.validate()?;
    Ok(definition)
}
