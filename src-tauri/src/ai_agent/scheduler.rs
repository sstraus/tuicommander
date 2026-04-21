//! Cron scheduler for recurring agent tasks.
//!
//! Persists jobs to `ai-cron.json` in the app config dir. A tokio interval
//! task checks cron expressions every 30 s. Triggered jobs always run with
//! `TrustLevel::Standard` regardless of any global unsafe-mode toggle.

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Notify;

use crate::state::AppState;

const CONFIG_FILE: &str = "ai-cron.json";
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
const DEFAULT_MAX_DURATION_SECS: u64 = 300;

// ── Job definition ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScheduledJob {
    pub id: String,
    pub cron_expr: String,
    pub goal: String,
    pub target_session: Option<String>,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_max_duration() -> u64 {
    DEFAULT_MAX_DURATION_SECS
}
fn default_enabled() -> bool {
    true
}

impl ScheduledJob {
    pub fn parse_schedule(&self) -> Result<Schedule, String> {
        Schedule::from_str(&self.cron_expr)
            .map_err(|e| format!("Invalid cron expression '{}': {e}", self.cron_expr))
    }
}

// ── Scheduler state ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SchedulerConfig {
    #[serde(default)]
    pub jobs: Vec<ScheduledJob>,
}

pub(crate) struct Scheduler {
    state: Arc<AppState>,
    last_fire: parking_lot::Mutex<HashMap<String, DateTime<Utc>>>,
    stop: Arc<Notify>,
}

impl Scheduler {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            last_fire: parking_lot::Mutex::new(HashMap::new()),
            stop: Arc::new(Notify::new()),
        }
    }

    pub async fn run(&self) {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => self.tick().await,
                _ = self.stop.notified() => {
                    tracing::info!("Scheduler stopped");
                    break;
                }
            }
        }
    }

    async fn tick(&self) {
        let config: SchedulerConfig = crate::config::load_json_config(CONFIG_FILE);
        let now = Utc::now();

        for job in &config.jobs {
            if !job.enabled {
                continue;
            }
            let schedule = match job.parse_schedule() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(job_id = %job.id, "Skipping job: {e}");
                    continue;
                }
            };

            if !should_fire(&schedule, &job.id, now, &self.last_fire) {
                continue;
            }

            self.last_fire.lock().insert(job.id.clone(), now);

            if super::engine::ACTIVE_AGENTS.contains_key(
                job.target_session.as_deref().unwrap_or(""),
            ) {
                tracing::info!(job_id = %job.id, "Skipping: target session busy");
                continue;
            }

            tracing::info!(job_id = %job.id, goal = %job.goal, "Firing scheduled job");
            self.fire_job(job).await;
        }
    }

    async fn fire_job(&self, job: &ScheduledJob) {
        let session_id = match &job.target_session {
            Some(sid) if self.state.sessions.contains_key(sid) => sid.clone(),
            _ => {
                match crate::pty::spawn_session_for_agent(
                    &self.state,
                    None,
                    Some(format!("cron:{}", job.id)),
                )
                .await
                {
                    Ok(sid) => sid,
                    Err(e) => {
                        tracing::error!(job_id = %job.id, "Failed to spawn session: {e}");
                        return;
                    }
                }
            }
        };

        let runtime = match super::commands::build_llm_runtime_for_scheduler() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(job_id = %job.id, "LLM runtime unavailable: {e}");
                return;
            }
        };

        match super::engine::start_agent_loop(
            self.state.clone(),
            session_id.clone(),
            job.goal.clone(),
            runtime,
            super::engine::TrustLevel::Standard,
        )
        .await
        {
            Ok(_rx) => {
                tracing::info!(job_id = %job.id, session_id, "Scheduled agent started");
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, "Failed to start agent: {e}");
            }
        }
    }
}

fn should_fire(
    schedule: &Schedule,
    job_id: &str,
    now: DateTime<Utc>,
    last_fire: &parking_lot::Mutex<HashMap<String, DateTime<Utc>>>,
) -> bool {
    let guard = last_fire.lock();
    let after = guard
        .get(job_id)
        .copied()
        .unwrap_or(now - chrono::Duration::seconds(TICK_INTERVAL.as_secs() as i64 + 1));
    drop(guard);

    schedule
        .after(&after)
        .take(1)
        .any(|next| next <= now)
}

// ── Config persistence commands ──────────────────────────────────

pub(crate) fn load_config() -> SchedulerConfig {
    crate::config::load_json_config(CONFIG_FILE)
}

pub(crate) fn save_config(config: &SchedulerConfig) -> Result<(), String> {
    for job in &config.jobs {
        job.parse_schedule()?;
    }
    crate::config::save_json_config(CONFIG_FILE, config)
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_cron() {
        let job = ScheduledJob {
            id: "test".into(),
            cron_expr: "0 0 * * * *".into(),
            goal: "run tests".into(),
            target_session: None,
            max_duration_secs: 300,
            enabled: true,
        };
        assert!(job.parse_schedule().is_ok());
    }

    #[test]
    fn parse_invalid_cron() {
        let job = ScheduledJob {
            id: "bad".into(),
            cron_expr: "not a cron".into(),
            goal: "".into(),
            target_session: None,
            max_duration_secs: 300,
            enabled: true,
        };
        assert!(job.parse_schedule().is_err());
    }

    #[test]
    fn should_fire_when_due() {
        let schedule = Schedule::from_str("* * * * * *").unwrap(); // every second
        let last = parking_lot::Mutex::new(HashMap::new());
        let now = Utc::now();
        assert!(should_fire(&schedule, "j1", now, &last));
    }

    #[test]
    fn should_not_fire_when_recently_fired() {
        let schedule = Schedule::from_str("0 0 * * * *").unwrap(); // top of every hour
        let last = parking_lot::Mutex::new(HashMap::new());
        let now = Utc::now();
        last.lock().insert("j1".into(), now);
        // Just fired — next occurrence is ~1h away, so shouldn't fire
        assert!(!should_fire(&schedule, "j1", now, &last));
    }

    #[test]
    fn disabled_job_skipped_in_config() {
        let config = SchedulerConfig {
            jobs: vec![ScheduledJob {
                id: "off".into(),
                cron_expr: "* * * * * *".into(),
                goal: "noop".into(),
                target_session: None,
                max_duration_secs: 300,
                enabled: false,
            }],
        };
        assert!(!config.jobs[0].enabled);
    }

    #[test]
    fn serde_round_trip() {
        let config = SchedulerConfig {
            jobs: vec![ScheduledJob {
                id: "build".into(),
                cron_expr: "0 0 * * * *".into(),
                goal: "cargo build".into(),
                target_session: Some("sess-1".into()),
                max_duration_secs: 600,
                enabled: true,
            }],
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: SchedulerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.jobs.len(), 1);
        assert_eq!(loaded.jobs[0].id, "build");
        assert_eq!(loaded.jobs[0].max_duration_secs, 600);
    }

    #[test]
    fn defaults_on_deserialize() {
        let json = r#"{"jobs":[{"id":"x","cron_expr":"0 0 * * * *","goal":"test"}]}"#;
        let config: SchedulerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.jobs[0].max_duration_secs, DEFAULT_MAX_DURATION_SECS);
        assert!(config.jobs[0].enabled);
        assert!(config.jobs[0].target_session.is_none());
    }

    #[test]
    fn save_rejects_invalid_cron() {
        let config = SchedulerConfig {
            jobs: vec![ScheduledJob {
                id: "bad".into(),
                cron_expr: "invalid".into(),
                goal: "test".into(),
                target_session: None,
                max_duration_secs: 300,
                enabled: true,
            }],
        };
        assert!(save_config(&config).is_err());
    }
}
