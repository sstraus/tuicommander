//! Durable execution records, independent of definition lifetime and delivery outcome.
use super::model::AutomationDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Each saved output stream retains at most the plan's 256 KiB budget.
pub const OUTPUT_LIMIT: usize = 256 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunTrigger {
    Scheduled { occurrence_ms: i64 },
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Reserved,
    Prechecking,
    Running,
    NeedsYou,
    Completed,
    Failed,
    Unknown,
    TimedOut,
    Interrupted,
    SkippedPrecheck,
    SkippedOverlap,
    SkippedConcurrency,
    SkippedExpired,
    SkippedMissed,
}

impl RunStatus {
    pub fn is_open(self) -> bool {
        matches!(
            self,
            Self::Reserved | Self::Prechecking | Self::Running | Self::NeedsYou
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SavedOutput {
    pub text: String,
    pub truncated: bool,
}

impl SavedOutput {
    pub fn bounded(text: &str) -> Self {
        let mut end = text.len().min(OUTPUT_LIMIT);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            text: text[..end].into(),
            truncated: end < text.len(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SavedPrecheck {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: SavedOutput,
    pub stderr: SavedOutput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AutomationRun {
    pub id: String,
    pub definition: AutomationDefinition,
    pub trigger: RunTrigger,
    pub status: RunStatus,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub finished_ms: Option<i64>,
    #[serde(default)]
    pub started_ms: Option<i64>,
    #[serde(default)]
    pub deadline_ms: Option<i64>,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub stdout: SavedOutput,
    pub stderr: SavedOutput,
    pub precheck: Option<SavedPrecheck>,
    /// Full pre-dispatch decision, including explicit bypass and absent checks.
    #[serde(default)]
    pub precheck_outcome: Option<super::precheck::PrecheckOutcome>,
    pub reason: Option<String>,
}

/// Fields learned after reservation; terminal records reject later mutations.
#[derive(Clone, Debug, Default)]
pub struct RunDetails {
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub precheck: Option<SavedPrecheck>,
    pub precheck_outcome: Option<super::precheck::PrecheckOutcome>,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Attempted,
    Confirmed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NotificationDelivery {
    pub run_id: String,
    pub transition: String,
    pub channel: String,
    pub status: DeliveryStatus,
    pub attempted_ms: i64,
    pub settled_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum SummaryWindow {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
}

impl SummaryWindow {
    pub fn millis(self) -> i64 {
        match self {
            Self::Day => 86_400_000,
            Self::Week => 604_800_000,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RunSummary {
    pub total: u64,
    pub by_status: BTreeMap<RunStatus, u64>,
}
