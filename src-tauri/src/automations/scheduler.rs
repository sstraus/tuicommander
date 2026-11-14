//! Deterministic admission only. The runtime supplies one tick every 30 seconds.
//! Dispatch is intentionally separate: only newly Reserved rows may launch work.
use super::{
    model::AutomationsConfig,
    run::{AutomationRun, RunTrigger},
    store::RunStore,
};
use chrono::{DateTime, Utc};

pub const TICK_INTERVAL_SECS: u64 = 30;

/// Reconcile a wake without replaying intervening occurrences. Expired latest
/// occurrences become skipped_missed; earlier occurrences are covered by the cursor.
pub fn tick(
    store: &RunStore,
    config: &AutomationsConfig,
    now: DateTime<Utc>,
) -> Result<Vec<AutomationRun>, String> {
    config.validate()?;
    let mut candidates = Vec::new();
    for definition in config
        .definitions
        .iter()
        .filter(|definition| definition.enabled)
    {
        let schedule = definition.schedule()?;
        if let Some(occurrence) = schedule.latest_due(now, None)? {
            candidates.push((
                definition,
                RunTrigger::Scheduled {
                    occurrence_ms: occurrence.timestamp_millis(),
                },
            ));
        }
    }
    store.admit_batch(
        &candidates,
        config.max_concurrent_runs,
        now.timestamp_millis(),
    )
}

/// Run Now ignores enabled state and prechecks (the dispatcher branches on
/// Manual), but shares scheduled admission's atomic overlap and capacity checks.
pub fn run_now(
    store: &RunStore,
    config: &AutomationsConfig,
    automation_id: &str,
    now: DateTime<Utc>,
) -> Result<AutomationRun, String> {
    config.validate()?;
    let definition = config
        .definitions
        .iter()
        .find(|definition| definition.id == automation_id)
        .ok_or("Automation definition not found")?;
    store
        .admit(
            definition,
            RunTrigger::Manual,
            config.max_concurrent_runs,
            now.timestamp_millis(),
        )?
        .ok_or_else(|| "Manual automation reservation was not recorded".into())
}

#[cfg(test)]
mod tests;
