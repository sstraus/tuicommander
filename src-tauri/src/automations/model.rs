//! Stored scheduler definitions. Schedule interpretation belongs to `schedule`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutomationsConfig {
    pub version: u32,
    pub max_concurrent_runs: u32,
    pub definitions: Vec<AutomationDefinition>,
}

impl Default for AutomationsConfig {
    fn default() -> Self {
        Self {
            version: 1,
            max_concurrent_runs: 2,
            definitions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationDefinition {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub run_config: String,
    pub repository: String,
    pub workspace: Workspace,
    pub cron: String,
    /// One wall-clock occurrence in `timezone`; cron must be empty when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once_local: Option<chrono::NaiveDateTime>,
    #[serde(default)]
    pub timezone: String,
    pub enabled: bool,
    pub grace_secs: u64,
    pub overlap: Overlap,
    pub max_duration_secs: u64,
    pub precheck: Option<Precheck>,
    /// Host-issued identity of the agent that created this definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by_session: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Workspace {
    Existing,
    NewPerRun { base_branch: String },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Overlap {
    Skip,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Precheck {
    pub command: String,
    pub timeout_secs: u64,
}

impl AutomationsConfig {
    /// Reject invalid disk state before any caller can overwrite it.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "Unsupported automations schema version: {}",
                self.version
            ));
        }
        if self.max_concurrent_runs == 0 {
            return Err("Automation concurrency must be greater than zero".into());
        }
        let mut ids = HashSet::new();
        for definition in &self.definitions {
            definition.validate()?;
            if !ids.insert(&definition.id) {
                return Err("Duplicate automation id in definitions".into());
            }
        }
        Ok(())
    }
}

impl AutomationDefinition {
    /// Validate storage and scheduling invariants without launching agents.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("id", &self.id),
            ("name", &self.name),
            ("prompt", &self.prompt),
            ("run_config", &self.run_config),
            ("repository", &self.repository),
            ("timezone", &self.timezone),
        ] {
            if value.trim().is_empty() {
                return Err(format!("Automation {name} must not be blank"));
            }
        }
        if self.grace_secs == 0 || self.max_duration_secs == 0 {
            return Err("Automation grace and maximum duration must be greater than zero".into());
        }
        if let Workspace::NewPerRun { base_branch } = &self.workspace
            && base_branch.trim().is_empty()
        {
            return Err("Automation base branch must not be blank".into());
        }
        if let Some(precheck) = &self.precheck
            && (precheck.command.trim().is_empty() || precheck.timeout_secs == 0)
        {
            return Err("Automation precheck needs a command and positive timeout".into());
        }
        self.schedule()?;
        Ok(())
    }

    /// Resolve either cron or Once using the stored timezone.
    pub fn schedule(&self) -> Result<super::schedule::AutomationSchedule, String> {
        super::schedule::AutomationSchedule::parse(&self.cron, &self.timezone, self.once_local)
    }

    /// Reject elapsed Once schedules only when creating or changing their occurrence.
    pub fn validate_schedule_at(&self, now: chrono::DateTime<chrono::Utc>) -> Result<(), String> {
        self.schedule()?.validate_creation(now)
    }
}
