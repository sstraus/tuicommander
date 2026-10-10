use super::*;
use crate::automations::model::AutomationDefinition;
use serde_json::json;

// Catches: malformed cron being persisted as an executable definition.
#[test]
fn malformed_cron_is_rejected_before_storage() {
    let definition: AutomationDefinition = serde_json::from_value(json!({
        "id":"invalid", "name":"Invalid cron", "prompt":"Review",
        "run_config":"codex", "repository":"/project",
        "workspace":{"mode":"existing"}, "cron":"not a cron",
        "timezone":"America/New_York", "enabled":true,
        "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600
    }))
    .unwrap();
    assert!(
        definition.validate().is_err(),
        "malformed cron was accepted"
    );
}

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

// Catches: accepting extended/extra-field cron, impossible dates, or invalid zones.
#[test]
fn strict_vixie_rejects_extensions_and_impossible_dates() {
    for cron in [
        "",
        "@daily",
        "* * * *",
        "0 * * * * *",
        "0 0 * * * 2026",
        "0 24 * * *",
        "60 * * * *",
        "0 0 31 2 *",
        "0 0 30 FEB *",
        "0 0 L * *",
        "0 0 1W * *",
        "0 0 ? * MON",
        "0 0 * * MON#2",
        "0 0 1 * +MON",
        "*/0 * * * *",
        "0 0 * * 8",
        "0 0 * JANJAN *",
        "0 0 * * MONMON",
    ] {
        assert!(Schedule::parse(cron, "UTC").is_err(), "accepted {cron}");
    }
    for zone in ["", "Mars/Olympus", "+02:00", "local"] {
        assert!(
            Schedule::parse("0 9 * * *", zone).is_err(),
            "accepted {zone}"
        );
    }
    assert!(Schedule::parse(&"*".repeat(257), "UTC").is_err());
    assert!(
        Schedule::parse("0 0 31 FEB MON", "UTC").is_ok(),
        "DOM/DOW OR makes Mondays in February valid"
    );
}

// Catches: combining DOM/DOW with AND, or interpreting Sunday 7 differently from 0.
#[test]
fn dom_dow_or_and_named_fields_keep_vixie_meaning() {
    let s = Schedule::parse("0 9 1 * MON", "UTC").unwrap();
    assert_eq!(
        s.next_after(utc("2026-10-01T09:00:00Z")).unwrap(),
        utc("2026-10-05T09:00:00Z")
    );
    assert_eq!(
        s.next_after(utc("2026-10-31T23:59:59Z")).unwrap(),
        utc("2026-11-01T09:00:00Z")
    );
    for dow in ["0", "7", "SUN"] {
        assert_eq!(
            Schedule::parse(&format!("0 9 * * {dow}"), "UTC")
                .unwrap()
                .next_after(utc("2026-10-09T00:00:00Z"))
                .unwrap(),
            utc("2026-10-11T09:00:00Z")
        );
    }
}

// Catches: host-zone dependence, inclusive-next duplicates, or lost fractional boundaries.
#[test]
fn stored_zone_and_exclusive_next_inclusive_latest_are_respected() {
    let ny = Schedule::parse("0 9 * * *", "America/New_York").unwrap();
    let z = Schedule::parse("0 9 * * *", "UTC").unwrap();
    let now = utc("2026-01-15T08:00:00Z");
    assert_eq!(ny.next_after(now).unwrap(), utc("2026-01-15T14:00:00Z"));
    assert_eq!(z.next_after(now).unwrap(), utc("2026-01-15T09:00:00Z"));
    let at = utc("2026-01-15T14:00:00Z");
    assert_eq!(ny.latest_due(at, None).unwrap(), Some(at));
    assert_eq!(ny.latest_due(at, Some(at)).unwrap(), None);
    assert_eq!(
        ny.latest_due(at, Some(utc("2026-01-14T14:00:00Z")))
            .unwrap(),
        Some(at)
    );
    assert_eq!(ny.next_after(at).unwrap(), utc("2026-01-16T14:00:00Z"));
    assert_eq!(ny.next_after(utc("2026-01-15T13:59:59.500Z")).unwrap(), at);
    assert_eq!(
        ny.latest_due(utc("2026-01-15T13:59:59.500Z"), None)
            .unwrap(),
        Some(utc("2026-01-14T14:00:00Z"))
    );
}

// Catches: croner shifting a nonexistent fixed wall time to the end of the gap.
#[test]
fn spring_gap_skips_the_missing_fixed_time_in_both_directions() {
    let s = Schedule::parse("30 2 * * *", "America/New_York").unwrap();
    assert_eq!(
        s.next_after(utc("2026-03-08T06:59:00Z")).unwrap(),
        utc("2026-03-09T06:30:00Z")
    );
    assert_eq!(
        s.latest_due(utc("2026-03-08T07:15:00Z"), None).unwrap(),
        Some(utc("2026-03-07T07:30:00Z"))
    );
}

// Catches: duplicate fixed-time dispatch at the second fold or latest_due selecting it.
#[test]
fn fall_fold_runs_fixed_time_once_at_the_earlier_instant() {
    let s = Schedule::parse("30 1 * * *", "America/New_York").unwrap();
    let first = utc("2026-11-01T05:30:00Z");
    assert_eq!(s.next_after(utc("2026-11-01T04:00:00Z")).unwrap(), first);
    assert_eq!(s.next_after(first).unwrap(), utc("2026-11-02T06:30:00Z"));
    assert_eq!(
        s.next_after(utc("2026-11-01T06:00:00Z")).unwrap(),
        utc("2026-11-02T06:30:00Z")
    );
    assert_eq!(
        s.latest_due(utc("2026-11-01T06:45:00Z"), None).unwrap(),
        Some(first)
    );
    assert_eq!(
        s.latest_due(utc("2026-11-01T06:45:00Z"), Some(first))
            .unwrap(),
        None
    );
}

// Catches: treating an hourly interval as a once-only fixed wall time during a fold.
#[test]
fn hourly_interval_retains_both_real_fold_occurrences() {
    let s = Schedule::parse("0 * * * *", "America/New_York").unwrap();
    assert_eq!(
        s.next_after(utc("2026-11-01T05:00:00Z")).unwrap(),
        utc("2026-11-01T06:00:00Z")
    );
}

// Catches: rejecting valid leap days or assuming a maximum four-year leap interval.
#[test]
fn leap_days_cross_the_non_leap_century() {
    let s = Schedule::parse("0 9 29 FEB *", "UTC").unwrap();
    assert_eq!(
        s.next_after(utc("2096-02-29T09:00:00Z")).unwrap(),
        utc("2104-02-29T09:00:00Z")
    );
    assert_eq!(
        s.latest_due(utc("2103-03-01T00:00:00Z"), None).unwrap(),
        Some(utc("2096-02-29T09:00:00Z"))
    );
}

// Catches: frontend-only preset arithmetic, invalid controls and unbounded preview counts.
#[test]
fn backend_presets_and_custom_preview_return_real_occurrences() {
    let now = utc("2026-10-09T10:35:00Z");
    for (preset, cron, expected) in [
        (
            SchedulePreset::Hourly { minute: 15 },
            "15 * * * *",
            "2026-10-09T11:15:00Z",
        ),
        (
            SchedulePreset::Daily { hour: 9, minute: 0 },
            "0 9 * * *",
            "2026-10-10T09:00:00Z",
        ),
        (
            SchedulePreset::Weekdays { hour: 9, minute: 0 },
            "0 9 * * 1-5",
            "2026-10-12T09:00:00Z",
        ),
        (
            SchedulePreset::Weekly {
                weekday: 1,
                hour: 9,
                minute: 0,
            },
            "0 9 * * 1",
            "2026-10-12T09:00:00Z",
        ),
    ] {
        assert_eq!(preset.cron().unwrap(), cron);
        let preview = preview(cron, "UTC", now, 3).unwrap();
        assert_eq!(preview.occurrences.len(), 3);
        assert_eq!(preview.occurrences[0], utc(expected));
        assert!(preview.occurrences.windows(2).all(|w| w[0] < w[1]));
    }
    let custom = preview("*/20 8-10 * * MON-FRI", "UTC", now, 1).unwrap();
    assert_eq!(custom.occurrences, vec![utc("2026-10-09T10:40:00Z")]);
    assert_eq!(custom.timezone, "UTC");
    assert!(preview("* * * * *", "UTC", now, 0).is_err());
    assert!(preview("* * * * *", "UTC", now, 21).is_err());
    assert!(SchedulePreset::Hourly { minute: 60 }.cron().is_err());
    assert!(
        SchedulePreset::Daily {
            hour: 24,
            minute: 0
        }
        .cron()
        .is_err()
    );
    assert!(
        SchedulePreset::Weekly {
            weekday: 8,
            hour: 9,
            minute: 0
        }
        .cron()
        .is_err()
    );
}

// Catches: silently falling back to UTC or resolving local zone despite an explicit zone.
#[test]
fn omitted_creation_zone_resolves_once_and_unavailable_zone_is_visible() {
    assert_eq!(
        resolve_creation_timezone(Some("America/New_York"), || panic!(
            "explicit zone queried host"
        ))
        .unwrap(),
        "America/New_York"
    );
    assert_eq!(
        resolve_creation_timezone(None, || Ok("Europe/Madrid".into())).unwrap(),
        "Europe/Madrid"
    );
    assert!(
        resolve_creation_timezone(None, || Err("OS zone unavailable".into()))
            .unwrap_err()
            .contains("OS zone unavailable")
    );
    assert!(resolve_creation_timezone(None, || Ok("unknown".into())).is_err());
    assert!(resolve_creation_timezone(Some(""), || Ok("UTC".into())).is_err());
    let actual = timezone_for_creation(None).unwrap();
    assert!(actual.parse::<chrono_tz::Tz>().is_ok());
}

// Catches: creation leaving a missing timezone on disk or subsequent reads changing it.
#[test]
fn creation_persists_the_local_zone_but_disk_and_edits_require_a_zone() {
    use crate::automations::definitions::DefinitionStore;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = DefinitionStore::at_path(dir.path().join("automations.json"));
    let input = json!({
        "id":"local", "name":"Local", "prompt":"Review", "run_config":"codex",
        "repository":"/project", "workspace":{"mode":"existing"}, "cron":"0 9 * * *",
        "enabled":true, "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600
    });
    let definition: AutomationDefinition = serde_json::from_value(input).unwrap();
    assert!(
        definition.validate().is_err(),
        "unresolved disk zone was accepted"
    );
    store.create(definition.clone()).unwrap();
    let saved = store.load().unwrap();
    assert_eq!(
        saved.definitions[0].timezone,
        timezone_for_creation(None).unwrap()
    );
    let path = dir.path().join("automations.json");
    let original = std::fs::read(&path).unwrap();
    assert!(store.update("local", definition.clone()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"version":1, "max_concurrent_runs":2, "definitions":[definition]}),
        )
        .unwrap(),
    )
    .unwrap();
    let unresolved = std::fs::read(&path).unwrap();
    assert!(store.load().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), unresolved);
}

#[path = "critic_tests.rs"]
mod critic_tests;
