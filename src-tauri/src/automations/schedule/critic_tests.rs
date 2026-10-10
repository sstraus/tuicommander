use super::Schedule;
use chrono::{DateTime, Utc};

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

// Catches: a stepped DOM wildcard becoming OR and scheduling unrequested agent runs.
// Vixie entry.c sets DOM_STAR when the field starts with '*'; cron.c then ANDs DOM/DOW.
#[test]
fn stepped_dom_wildcard_does_not_run_on_non_mondays() {
    let schedule = Schedule::parse("0 9 */2 * MON", "UTC").unwrap();
    assert_eq!(
        schedule.next_after(utc("2025-01-01T09:00:00Z")).unwrap(),
        utc("2025-01-13T09:00:00Z"),
        "stepped wildcard ran on an unrequested day"
    );
    assert_eq!(
        schedule
            .latest_due(utc("2025-01-14T09:00:00Z"), None)
            .unwrap(),
        Some(utc("2025-01-13T09:00:00Z"))
    );
}

// Catches: losing Vixie's leading-star distinction, or applying it only to DOM.
#[test]
fn adjacent_day_forms_preserve_vixie_intersection_and_restricted_or() {
    for (days, next, latest) in [
        ("*/2 * MON", "2025-01-13T09:00:00Z", "2025-01-13T09:00:00Z"),
        ("*/3 * MON", "2025-01-13T09:00:00Z", "2025-01-13T09:00:00Z"),
        (
            "*/2 * MON,WED",
            "2025-01-13T09:00:00Z",
            "2025-01-13T09:00:00Z",
        ),
        ("* * MON", "2025-01-06T09:00:00Z", "2025-01-13T09:00:00Z"),
        (
            "1-31/2 * MON",
            "2025-01-03T09:00:00Z",
            "2025-01-13T09:00:00Z",
        ),
        ("* * *", "2025-01-02T09:00:00Z", "2025-01-14T09:00:00Z"),
        ("4 * */2", "2025-01-04T09:00:00Z", "2025-01-04T09:00:00Z"),
    ] {
        let schedule = Schedule::parse(&format!("0 9 {days}"), "UTC").unwrap();
        assert_eq!(
            schedule.next_after(utc("2025-01-01T09:00:00Z")).unwrap(),
            utc(next),
            "wrong next occurrence for {days}"
        );
        assert_eq!(
            schedule
                .latest_due(utc("2025-01-14T09:00:00Z"), None)
                .unwrap(),
            Some(utc(latest)),
            "wrong latest occurrence for {days}"
        );
    }
    // The explicit range remains restricted: an even-numbered Monday also runs.
    let restricted = Schedule::parse("0 9 1-31/2 * MON", "UTC").unwrap();
    assert_eq!(
        restricted.next_after(utc("2025-01-05T09:00:00Z")).unwrap(),
        utc("2025-01-06T09:00:00Z")
    );
}
