//! A common occurrence API for recurring and one-shot definitions.

use super::{Schedule, parse_zone};
use chrono::{DateTime, NaiveDateTime, TimeZone, Timelike, Utc};
use serde::Serialize;

/// Validated recurring pattern or one resolved UTC occurrence.
pub enum AutomationSchedule {
    Cron(Box<Schedule>),
    Once(DateTime<Utc>),
}

impl AutomationSchedule {
    /// Resolve Once folds to the earlier instant; reject gaps and mixed schedules.
    pub fn parse(
        cron: &str,
        timezone: &str,
        once_local: Option<NaiveDateTime>,
    ) -> Result<Self, String> {
        match once_local {
            None => Schedule::parse(cron, timezone).map(|schedule| Self::Cron(Box::new(schedule))),
            Some(local) => {
                if !cron.is_empty() {
                    return Err("Automation Once and cron are mutually exclusive".into());
                }
                if local.nanosecond() >= 1_000_000_000 {
                    return Err("Automation Once does not support leap seconds".into());
                }
                let instant = parse_zone(timezone)?
                    .from_local_datetime(&local)
                    .earliest()
                    .ok_or("Automation Once local time does not exist in its timezone")?
                    .with_timezone(&Utc);
                Ok(Self::Once(instant))
            }
        }
    }

    /// Existing elapsed definitions remain readable; creation must be in the future.
    pub fn validate_creation(&self, now: DateTime<Utc>) -> Result<(), String> {
        if matches!(self, Self::Once(instant) if *instant <= now) {
            return Err("Automation Once must be in the future".into());
        }
        Ok(())
    }

    /// Find the next occurrence, or none when a Once schedule is exhausted.
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
        match self {
            Self::Cron(schedule) => schedule.next_after(after).map(Some),
            Self::Once(instant) => Ok((*instant > after).then_some(*instant)),
        }
    }

    /// Include the current instant, excluding occurrences consumed by the durable cursor.
    pub fn latest_due(
        &self,
        now: DateTime<Utc>,
        after: Option<DateTime<Utc>>,
    ) -> Result<Option<DateTime<Utc>>, String> {
        match self {
            Self::Cron(schedule) => schedule.latest_due(now, after),
            Self::Once(instant) => Ok((*instant <= now
                && after.is_none_or(|cursor| *instant > cursor))
            .then_some(*instant)),
        }
    }

    /// A consumed Once schedule is complete without removing its definition.
    pub fn is_completed(&self, scheduled_cursor: Option<DateTime<Utc>>) -> bool {
        matches!(self, Self::Once(instant) if scheduled_cursor.is_some_and(|cursor| cursor >= *instant))
    }
}

/// Definition preview keeps Once local time and returns UTC transport instants.
#[derive(Debug, Serialize)]
pub struct DefinitionSchedulePreview {
    pub cron: String,
    pub timezone: String,
    pub once_local: Option<NaiveDateTime>,
    pub occurrences: Vec<DateTime<Utc>>,
    pub completed: bool,
}

/// Preview at most twenty future instants, omitting consumed Once occurrences.
pub fn preview_definition(
    definition: &crate::automations::model::AutomationDefinition,
    after: DateTime<Utc>,
    count: usize,
    scheduled_cursor: Option<DateTime<Utc>>,
) -> Result<DefinitionSchedulePreview, String> {
    if !(1..=20).contains(&count) {
        return Err("Automation preview count must be between 1 and 20".into());
    }
    let schedule = definition.schedule()?;
    let completed = schedule.is_completed(scheduled_cursor);
    let mut occurrences = Vec::with_capacity(count);
    let mut cursor = after;
    if !completed {
        for _ in 0..count {
            let Some(next) = schedule.next_after(cursor)? else {
                break;
            };
            occurrences.push(next);
            cursor = next;
        }
    }
    Ok(DefinitionSchedulePreview {
        cron: definition.cron.clone(),
        timezone: definition.timezone.clone(),
        once_local: definition.once_local,
        occurrences,
        completed,
    })
}

#[cfg(test)]
mod tests;
