use super::*;
use crate::automations::{definitions::DefinitionStore, model::AutomationDefinition};
use serde_json::json;

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn definition(local: &str, timezone: &str) -> AutomationDefinition {
    serde_json::from_value(json!({
        "id":"once", "name":"Revisit", "prompt":"Review changes", "run_config":"codex",
        "repository":"/project", "workspace":{"mode":"existing"}, "cron":"",
        "once_local":local, "timezone":timezone, "enabled":true, "grace_secs":43200,
        "overlap":"skip", "max_duration_secs":3600
    }))
    .unwrap()
}

// Catches: treating Once as a recurring cron, losing due-boundary equality, or replaying a consumed instant.
#[test]
fn once_has_one_occurrence_and_consumed_cursor_prevents_catch_up_replay() {
    let def = definition("2099-10-09T10:00:00", "Europe/Madrid");
    let schedule = def.schedule().unwrap();
    let at = utc("2099-10-09T08:00:00Z");
    let before = at - chrono::Duration::milliseconds(1);
    let later = at + chrono::Duration::days(365);
    assert_eq!(schedule.next_after(before).unwrap(), Some(at));
    assert_eq!(schedule.next_after(at).unwrap(), None);
    assert_eq!(schedule.next_after(later).unwrap(), None);
    assert_eq!(schedule.latest_due(before, None).unwrap(), None);
    assert_eq!(schedule.latest_due(at, None).unwrap(), Some(at));
    assert_eq!(schedule.latest_due(later, Some(before)).unwrap(), Some(at));
    assert_eq!(schedule.latest_due(later, Some(at)).unwrap(), None);
    assert_eq!(schedule.latest_due(later, Some(later)).unwrap(), None);
    assert!(!schedule.is_completed(None));
    assert!(!schedule.is_completed(Some(before)));
    assert!(schedule.is_completed(Some(at)));
}

// Catches: silently shifting nonexistent wall times or dispatching the later fold instant.
#[test]
fn once_rejects_spring_gaps_and_uses_the_earlier_fall_fold() {
    assert!(
        definition("2026-03-08T02:30:00", "America/New_York")
            .validate()
            .is_err()
    );
    let def = definition("2026-11-01T01:30:00", "America/New_York");
    let schedule = def.schedule().unwrap();
    let first = utc("2026-11-01T05:30:00Z");
    assert_eq!(
        schedule.next_after(utc("2026-11-01T04:00:00Z")).unwrap(),
        Some(first)
    );
    assert_eq!(
        schedule
            .latest_due(utc("2026-11-01T06:45:00Z"), None)
            .unwrap(),
        Some(first)
    );
    assert_eq!(
        schedule
            .latest_due(utc("2026-11-01T06:45:00Z"), Some(first))
            .unwrap(),
        None
    );
    assert!(schedule.validate_creation(first).is_err());
    assert!(
        schedule
            .validate_creation(first - chrono::Duration::seconds(1))
            .is_ok()
    );
}

// Catches: new elapsed Once definitions executing immediately, or expired persisted definitions breaking all loads/edits.
#[test]
fn creation_rejects_elapsed_once_but_loading_and_non_schedule_edits_preserve_it() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("automations.json");
    let store = DefinitionStore::at_path(path.clone());
    let mut def = definition("2000-10-09T10:00:00", "UTC");
    assert!(store.create(def.clone()).unwrap_err().contains("future"));
    assert!(!path.exists());
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"version":1, "max_concurrent_runs":2,
        "definitions":[def]}))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(store.load().unwrap().definitions, vec![def.clone()]);
    def.prompt = "Inspect saved result".into();
    store.update("once", def.clone()).unwrap();
    let before = std::fs::read(&path).unwrap();
    def.once_local = Some("2001-10-09T10:00:00".parse().unwrap());
    assert!(store.update("once", def).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

// Catches: ambiguous mixed schedules, invalid zones, leap seconds, or date-times with a misleading UTC offset.
#[test]
fn once_wire_and_schedule_validation_reject_ambiguous_or_invalid_inputs() {
    let mut def = definition("2099-10-09T10:00:00", "UTC");
    def.cron = "0 9 * * *".into();
    assert!(def.validate().is_err());
    def.cron = " ".into();
    assert!(def.validate().is_err());
    def.cron.clear();
    def.timezone = "Mars/Olympus".into();
    assert!(def.validate().is_err());
    def.timezone = "UTC".into();
    def.once_local = Some("2099-10-09T10:00:60".parse().unwrap());
    assert!(def.validate().is_err());
    for invalid in [
        "2099-02-30T10:00:00",
        "2099-10-09T10:00:00Z",
        "2099-10-09T10:00:00+02:00",
    ] {
        let mut value = serde_json::to_value(&def).unwrap();
        value["once_local"] = json!(invalid);
        assert!(serde_json::from_value::<AutomationDefinition>(value).is_err());
    }
}

// Catches: Once preview fabricating more runs, reporting completion just because time elapsed, or dropping the definition.
#[test]
fn preview_returns_only_one_instant_and_completion_comes_from_the_scheduled_cursor() {
    let def = definition("2099-10-09T10:00:00", "UTC");
    let at = utc("2099-10-09T10:00:00Z");
    let before = at - chrono::Duration::days(1);
    let result = preview_definition(&def, before, 20, None).unwrap();
    assert_eq!(result.occurrences, vec![at]);
    assert!(!result.completed);
    assert_eq!(result.once_local, def.once_local);
    let elapsed = preview_definition(&def, at, 20, None).unwrap();
    assert!(elapsed.occurrences.is_empty());
    assert!(!elapsed.completed);
    let consumed = preview_definition(&def, before, 20, Some(at)).unwrap();
    assert!(consumed.occurrences.is_empty());
    assert!(consumed.completed);
    assert_eq!(serde_json::to_value(consumed).unwrap()["completed"], true);
    assert!(preview_definition(&def, before, 0, None).is_err());
    assert!(preview_definition(&def, before, 21, None).is_err());
}

// Catches: the shared definition API changing existing cron boundaries or marking recurring schedules completed.
#[test]
fn cron_definitions_keep_their_existing_occurrence_and_preview_semantics() {
    let mut def = definition("2099-10-09T10:00:00", "UTC");
    def.once_local = None;
    def.cron = "0 9 * * *".into();
    let at = utc("2026-10-09T09:00:00Z");
    let schedule = def.schedule().unwrap();
    assert_eq!(
        schedule.next_after(at).unwrap(),
        Some(at + chrono::Duration::days(1))
    );
    assert_eq!(schedule.latest_due(at, None).unwrap(), Some(at));
    assert_eq!(schedule.latest_due(at, Some(at)).unwrap(), None);
    assert!(!schedule.is_completed(Some(at)));
    assert_eq!(
        preview_definition(&def, at, 2, Some(at))
            .unwrap()
            .occurrences
            .len(),
        2
    );
}
