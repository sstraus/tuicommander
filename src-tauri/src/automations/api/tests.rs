use super::*;
use crate::automations::{
    run::{RunStatus, RunTrigger},
    store::RunOwner,
};

fn definition(id: &str) -> Value {
    json!({"id":id,"name":"Review","prompt":"Review repository","run_config":"codex",
        "repository":"/project","workspace":{"mode":"existing"},"cron":"0 9 * * *",
        "timezone":"UTC","enabled":true,"grace_secs":43200,"overlap":"skip",
        "max_duration_secs":3600,"precheck":null})
}
fn now() -> DateTime<Utc> {
    "2026-10-10T08:00:00Z".parse().unwrap()
}
fn call(definitions: &DefinitionStore, runs: &RunStore, input: Value) -> Result<Value, String> {
    execute_stored(definitions, runs, parse(input)?, now())
}
fn setup() -> (tempfile::TempDir, DefinitionStore, RunOwner) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let definitions = DefinitionStore::at_path(dir.path().join("automations.json"));
    let owner =
        RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), now().timestamp_millis()).unwrap();
    (dir, definitions, owner)
}

// Catches: whole-array writes, stale pause overwrites, history deletion and mutable run snapshots.
#[test]
fn per_id_api_edits_preserve_other_definitions_and_deleted_history() {
    let (_dir, definitions, owner) = setup();
    let runs = owner.store();
    for id in ["a", "b"] {
        let saved = call(
            &definitions,
            runs,
            json!({"action":"create","definition":definition(id)}),
        )
        .unwrap();
        assert_eq!(saved["id"], id);
    }
    let original = definitions.get("a").unwrap();
    runs.reserve(&original, RunTrigger::Manual, now().timestamp_millis())
        .unwrap()
        .unwrap();
    let mut changed = definition("a");
    changed["prompt"] = json!("New prompt");
    call(
        &definitions,
        runs,
        json!({"action":"update","definition":changed}),
    )
    .unwrap();
    let paused = call(
        &definitions,
        runs,
        json!({"action":"update","id":"a","enabled":false}),
    )
    .unwrap();
    assert_eq!(paused["prompt"], "New prompt");
    assert_eq!(paused["enabled"], false);
    assert_eq!(
        call(&definitions, runs, json!({"action":"get","id":"b"})).unwrap()["prompt"],
        "Review repository"
    );
    let items = call(&definitions, runs, json!({"action":"list"})).unwrap();
    assert_eq!(items[0]["next_run_ms"], Value::Null);
    assert_eq!(items[0]["last_status"], "reserved");
    assert_eq!(items[1]["next_run_ms"], 1791622800000_i64);
    call(&definitions, runs, json!({"action":"delete","id":"a"})).unwrap();
    let history = call(
        &definitions,
        runs,
        json!({"action":"list_runs","id":"a","limit":1}),
    )
    .unwrap();
    assert_eq!(history[0]["definition"]["prompt"], "Review repository");
    assert!(call(&definitions, runs, json!({"action":"get","id":"a"})).is_err());
}

// Catches: preview inventing a second Once occurrence or discarding the durable consumption cursor.
#[test]
fn preview_and_list_respect_consumed_once_and_backend_presets() {
    let (_dir, definitions, owner) = setup();
    let runs = owner.store();
    let mut once = definition("once");
    once["cron"] = json!("");
    once["once_local"] = json!("2099-10-10T09:00:00");
    call(
        &definitions,
        runs,
        json!({"action":"create","definition":once}),
    )
    .unwrap();
    let input = json!({"action":"preview_definition","definition":once,"count":4});
    let preview = call(&definitions, runs, input.clone()).unwrap();
    assert_eq!(preview["occurrences"], json!(["2099-10-10T09:00:00Z"]));
    let instant: DateTime<Utc> = "2099-10-10T09:00:00Z".parse().unwrap();
    runs.admit(
        &definitions.get("once").unwrap(),
        RunTrigger::Scheduled {
            occurrence_ms: instant.timestamp_millis(),
        },
        2,
        instant.timestamp_millis(),
    )
    .unwrap();
    let consumed = call(&definitions, runs, input).unwrap();
    assert_eq!(consumed["completed"], true);
    assert_eq!(consumed["occurrences"], json!([]));
    assert_eq!(
        call(&definitions, runs, json!({"action":"list"})).unwrap()[0]["next_run_ms"],
        Value::Null
    );
    assert_eq!(
        call(
            &definitions,
            runs,
            json!({"action":"preset","preset":{"kind":"weekdays","hour":9,"minute":15}})
        )
        .unwrap(),
        json!({"cron":"15 9 * * 1-5"})
    );
    let cron = call(
        &definitions,
        runs,
        json!({"action":"preview","cron":"0 9 * * *","timezone":"UTC","count":2}),
    )
    .unwrap();
    assert_eq!(
        cron["occurrences"],
        json!(["2026-10-10T09:00:00Z", "2026-10-11T09:00:00Z"])
    );
}

// Catches: summary using paginated history or accepting invalid windows/counts/ambiguous edits.
#[test]
fn api_aggregates_elapsed_windows_and_rejects_invalid_inputs_without_writes() {
    let (dir, definitions, owner) = setup();
    let runs = owner.store();
    call(
        &definitions,
        runs,
        json!({"action":"create","definition":definition("a")}),
    )
    .unwrap();
    for age in [0, 86_400_001, 604_800_001] {
        runs.reserve(
            &definitions.get("a").unwrap(),
            RunTrigger::Manual,
            now().timestamp_millis() - age,
        )
        .unwrap();
    }
    assert_eq!(
        call(
            &definitions,
            runs,
            json!({"action":"summary","window":"24h"})
        )
        .unwrap(),
        json!({"total":1,"by_status":{"reserved":1}})
    );
    assert_eq!(
        call(
            &definitions,
            runs,
            json!({"action":"summary","window":"7d"})
        )
        .unwrap(),
        json!({"total":2,"by_status":{"reserved":2}})
    );
    let before = std::fs::read(dir.path().join("automations.json")).unwrap();
    for input in [
        json!({"action":"summary","window":"month"}),
        json!({"action":"list_runs","limit":0}),
        json!({"action":"list_runs","limit":101}),
        json!({"action":"preview","cron":"invalid","timezone":"UTC"}),
        json!({"action":"preview","cron":"0 9 * * *","timezone":"invalid"}),
        json!({"action":"preview","cron":"0 9 * * *","timezone":"UTC","count":21}),
        json!({"action":"update","id":"a","enabled":false,"definition":definition("a")}),
        json!({"action":"update","id":"b","definition":definition("a")}),
        json!({"action":"get","id":"absent"}),
        json!({"action":"create","definition":definition("a")}),
        json!({"action":"list","unexpected":true}),
    ] {
        assert!(call(&definitions, runs, input.clone()).is_err(), "{input}");
    }
    assert_eq!(
        std::fs::read(dir.path().join("automations.json")).unwrap(),
        before
    );
    assert_eq!(
        runs.history(None, 1, 1).unwrap()[0].status,
        RunStatus::Reserved
    );
}
