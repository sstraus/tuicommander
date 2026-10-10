//! Zone-aware cron semantics for scheduled automations.

use chrono::{DateTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use croner::{
    Cron,
    parser::{CronParser, Seconds, Year},
};
use serde::{Deserialize, Serialize};

pub mod once;
pub use once::AutomationSchedule;

/// Validated five-field Vixie cron evaluated in its stored IANA zone.
pub struct Schedule {
    cron: Cron,
    zone: Tz,
}

impl Schedule {
    /// Reject malformed, extended and impossible cron patterns and unknown zones.
    pub fn parse(expression: &str, timezone: &str) -> Result<Self, String> {
        // Bound parser allocation; five fields need far less than this even with lists.
        if expression.len() > 256 || expression.split_whitespace().count() != 5 {
            return Err(
                "Automation cron must contain exactly five Vixie fields (maximum 256 bytes)".into(),
            );
        }
        // Croner also supports Quartz extensions. Admit only Vixie tokens and let
        // its parser own field ranges, lists and steps (including named aliases).
        for (index, field) in expression.split_whitespace().enumerate() {
            if field.split(',').any(str::is_empty) {
                return Err("Automation cron lists must not contain empty entries".into());
            }
            let mut field = field.to_ascii_uppercase();
            let aliases: &[&str] = match index {
                3 => &[
                    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV",
                    "DEC",
                ],
                4 => &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
                _ => &[],
            };
            if field
                .split(|c: char| !c.is_ascii_alphabetic())
                .filter(|word| !word.is_empty())
                .any(|word| !aliases.contains(&word))
            {
                return Err("Invalid Vixie cron alias".into());
            }
            for alias in aliases {
                field = field.replace(alias, "0");
            }
            if !field
                .bytes()
                .all(|c| c.is_ascii_digit() || b"*,-/".contains(&c))
            {
                return Err("Automation cron only supports five-field Vixie syntax".into());
            }
        }
        // Vixie intersects day fields when either starts with '*', including
        // steps. Croner only marks a literal '*' as a wildcard by itself.
        let intersect_days = expression
            .split_whitespace()
            .enumerate()
            .any(|(index, field)| matches!(index, 2 | 4) && field.starts_with('*'));
        let cron = CronParser::builder()
            .seconds(Seconds::Disallowed)
            .year(Year::Disallowed)
            .dom_and_dow(intersect_days)
            .build()
            .parse(expression)
            .map_err(|e| format!("Invalid automation cron: {e}"))?;
        // Five-field expressions have no year restriction. A representative leap
        // year detects impossible DOM/month combinations. Croner's search is
        // bounded by YEAR_UPPER_LIMIT=5000 and MAX_SEARCH_ITERATIONS.
        let anchor = Utc
            .with_ymd_and_hms(2000, 1, 1, 0, 0, 0)
            .single()
            .ok_or("Invalid cron validation anchor")?;
        cron.find_next_occurrence(&anchor, true)
            .map_err(|e| format!("Automation cron has no possible occurrence: {e}"))?;
        let zone = parse_zone(timezone)?;
        Ok(Self { cron, zone })
    }

    /// Find the first occurrence strictly after the UTC instant.
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
        self.occurrence(after, true)
    }

    /// Return the latest occurrence at or before now, after the optional cursor.
    pub fn latest_due(
        &self,
        now: DateTime<Utc>,
        after: Option<DateTime<Utc>>,
    ) -> Result<Option<DateTime<Utc>>, String> {
        let latest = self.occurrence(now, false)?;
        Ok(after.is_none_or(|cursor| latest > cursor).then_some(latest))
    }

    fn occurrence(&self, instant: DateTime<Utc>, forward: bool) -> Result<DateTime<Utc>, String> {
        // Normalize in UTC: chrono's local with_nanosecond re-resolves wall time
        // and returns None in a fold, even for an already unambiguous instant.
        let mut cursor = instant
            .with_nanosecond(0)
            .ok_or("Invalid automation search instant")?
            .with_timezone(&self.zone);
        let mut inclusive = !forward;
        // A fixed-time gap may produce a shifted nonmatching candidate in croner
        // 4.0.1. Skip it in either direction, with a finite policy-filter budget.
        // This comfortably spans consecutive annual gaps without an unbounded loop.
        for _ in 0..512 {
            let candidate = if forward {
                self.cron.find_next_occurrence(&cursor, inclusive)
            } else {
                self.cron.find_previous_occurrence(&cursor, inclusive)
            }
            .map_err(|e| format!("Cannot find automation occurrence: {e}"))?;
            let utc = candidate.with_timezone(&Utc);
            if self
                .cron
                .is_time_matching(&candidate)
                .map_err(|e| format!("Invalid automation occurrence: {e}"))?
                && if forward {
                    utc > instant
                } else {
                    utc <= instant
                }
            {
                return Ok(utc);
            }
            cursor = candidate;
            inclusive = false;
        }
        Err("Automation occurrence search exceeded its DST policy limit".into())
    }
}

fn parse_zone(timezone: &str) -> Result<Tz, String> {
    timezone
        .parse()
        .map_err(|_| format!("Invalid IANA automation timezone: {timezone}"))
}

/// Resolve an omitted zone at creation; never silently substitute UTC.
pub fn timezone_for_creation(timezone: Option<&str>) -> Result<String, String> {
    resolve_creation_timezone(timezone, || {
        iana_time_zone::get_timezone()
            .map_err(|e| format!("Local IANA timezone is unavailable: {e}"))
    })
}

fn resolve_creation_timezone(
    timezone: Option<&str>,
    local: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let timezone = match timezone {
        Some(timezone) => timezone.to_owned(),
        None => local()?,
    };
    parse_zone(&timezone)?;
    Ok(timezone)
}

/// Friendly cadence inputs converted to cron entirely in Rust.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchedulePreset {
    Hourly {
        minute: u8,
    },
    Daily {
        hour: u8,
        minute: u8,
    },
    Weekdays {
        hour: u8,
        minute: u8,
    },
    /// Sunday is 0 (7 is also accepted), Monday is 1.
    Weekly {
        weekday: u8,
        hour: u8,
        minute: u8,
    },
}

impl SchedulePreset {
    /// Produce a five-field Vixie pattern, rejecting invalid cadence controls.
    pub fn cron(self) -> Result<String, String> {
        let (hour, minute, weekday) = match self {
            Self::Hourly { minute } => (0, minute, 0),
            Self::Daily { hour, minute } | Self::Weekdays { hour, minute } => (hour, minute, 0),
            Self::Weekly {
                weekday,
                hour,
                minute,
            } => (hour, minute, weekday),
        };
        if hour > 23 || minute > 59 || weekday > 7 {
            return Err("Invalid automation preset hour, minute or weekday".into());
        }
        Ok(match self {
            Self::Hourly { .. } => format!("{minute} * * * *"),
            Self::Daily { .. } => format!("{minute} {hour} * * *"),
            Self::Weekdays { .. } => format!("{minute} {hour} * * 1-5"),
            Self::Weekly { .. } => format!("{minute} {hour} * * {weekday}"),
        })
    }
}

/// Backend preview result; occurrences are UTC instants for transport parity.
#[derive(Debug, Serialize)]
pub struct SchedulePreview {
    pub cron: String,
    pub timezone: String,
    pub occurrences: Vec<DateTime<Utc>>,
}

/// Preview up to twenty occurrences of either a preset or a custom pattern.
pub fn preview(
    expression: &str,
    timezone: &str,
    after: DateTime<Utc>,
    count: usize,
) -> Result<SchedulePreview, String> {
    if !(1..=20).contains(&count) {
        return Err("Automation preview count must be between 1 and 20".into());
    }
    let schedule = Schedule::parse(expression, timezone)?;
    let mut occurrences = Vec::with_capacity(count);
    let mut cursor = after;
    for _ in 0..count {
        cursor = schedule.next_after(cursor)?;
        occurrences.push(cursor);
    }
    Ok(SchedulePreview {
        cron: expression.into(),
        timezone: timezone.into(),
        occurrences,
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod recheck_tests;
