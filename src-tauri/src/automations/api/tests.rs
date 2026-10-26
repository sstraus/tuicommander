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

// Catches: transport callers losing the opt-in on create/update, accepting a
// non-boolean consent value, or changing the literal prompt during persistence.
#[test]
fn shared_api_persists_explicit_context_consent_and_rejects_invalid_flags() {
    let (dir, definitions, owner) = setup();
    let mut value = definition("context");
    value["prompt"] = json!("  Literal {{files}}\né  \n");
    value["precheck"] = json!({"command":"git status --short","timeout_secs":30});
    let created = call(
        &definitions,
        owner.store(),
        json!({"action":"create","definition":value}),
    )
    .unwrap();
    assert_eq!(created["precheck"]["inject_output"], false);
    for enabled in [true, false] {
        value["precheck"]["inject_output"] = json!(enabled);
        call(
            &definitions,
            owner.store(),
            json!({"action":"update","definition":value}),
        )
        .unwrap();
        let saved = call(
            &definitions,
            owner.store(),
            json!({"action":"get","id":"context"}),
        )
        .unwrap();
        assert_eq!(saved["precheck"]["inject_output"], enabled);
        assert_eq!(saved["prompt"], "  Literal {{files}}\né  \n");
    }
    let before = std::fs::read(dir.path().join("automations.json")).unwrap();
    for invalid in [json!("true"), json!(1), Value::Null] {
        value["precheck"]["inject_output"] = invalid;
        assert!(
            call(
                &definitions,
                owner.store(),
                json!({"action":"update","definition":value})
            )
            .is_err()
        );
    }
    assert_eq!(
        std::fs::read(dir.path().join("automations.json")).unwrap(),
        before
    );
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

// Catches a notification link depending on a definition that has since been deleted.
#[test]
fn canonical_run_lookup_survives_definition_deletion() {
    let (_dir, definitions, owner) = setup();
    call(
        &definitions,
        owner.store(),
        json!({"action":"create","definition":definition("notice")}),
    )
    .unwrap();
    let run = owner
        .store()
        .reserve(
            &definitions.get("notice").unwrap(),
            RunTrigger::Manual,
            now().timestamp_millis(),
        )
        .unwrap()
        .unwrap();
    call(
        &definitions,
        owner.store(),
        json!({"action":"delete","id":"notice"}),
    )
    .unwrap();
    let reply = call(
        &definitions,
        owner.store(),
        json!({"action":"run","id":run.id}),
    )
    .unwrap();
    assert_eq!(reply["id"], run.id);
    assert_eq!(reply["definition"]["id"], "notice");
}

mod templates;

// Catches: relative requests requiring a caller-computed wall clock or rejecting in_secs.
#[test]
fn relative_create_uses_host_clock_not_caller_wall_time() {
    let (_dir, definitions, owner) = setup();
    let host_now: DateTime<Utc> = "2099-10-10T08:00:00Z".parse().unwrap();
    let mut input = definition("relative");
    input["cron"] = json!("");
    input["in_secs"] = json!(3600);
    let saved = execute_stored_as(
        &definitions,
        owner.store(),
        parse(json!({"action":"create","definition":input})).unwrap(),
        host_now,
        Caller::Agent(Some("caller".into())),
    )
    .unwrap();
    assert_eq!(saved["once_local"], "2099-10-10T09:00:00");
    assert_eq!(
        saved["next_run_ms"],
        host_now.timestamp_millis() + 3_600_000
    );
}

fn saved_profile_config() -> crate::config::AgentsConfig {
    serde_json::from_value(json!({"agents": {
        "claude": {"run_configs": [{"name":"Review profile", "command":"claude"}]},
        "retired-harness": {"run_configs": [{"name":"Retired profile", "command":"retired"}]}
    }}))
    .unwrap()
}

// Catches: accepting create/update configurations that fail the runner's named-profile lookup.
#[tokio::test]
async fn automation_create_rejects_unresolvable_run_config() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_owned());
    std::fs::write(
        dir.path().join("agents.json"),
        serde_json::to_vec(&saved_profile_config()).unwrap(),
    )
    .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    for (index, profile) in ["claude", "missing", "Retired profile"]
        .into_iter()
        .enumerate()
    {
        let mut input = definition(&format!("invalid-{index}"));
        input["run_config"] = json!(profile);
        let result = execute(
            json!({"action":"create", "definition":input}),
            state.clone(),
        )
        .await;
        let error = result.expect_err("create accepted a configuration the runner cannot resolve");
        assert!(error.contains(profile), "{error}");
        assert!(
            error.contains("Review profile"),
            "missing repair choices: {error}"
        );
    }
    assert_eq!(
        DefinitionStore::new().load().unwrap().definitions,
        [] as [crate::automations::model::AutomationDefinition; 0]
    );
    let mut valid = definition("valid");
    valid["run_config"] = json!("rEvIeW PrOfIlE");
    execute(
        json!({"action":"create", "definition":valid}),
        state.clone(),
    )
    .await
    .unwrap();
    let before = std::fs::read(dir.path().join("automations.json")).unwrap();
    valid["run_config"] = json!("claude");
    assert!(
        execute(json!({"action":"update", "definition":valid}), state)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(dir.path().join("automations.json")).unwrap(),
        before
    );
    // A removed profile cannot be re-enabled, but pausing remains a repair path.
    std::fs::write(dir.path().join("agents.json"), b"{}").unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    execute(json!({"action":"pause", "id":"valid"}), state.clone())
        .await
        .unwrap();
    let error = execute(json!({"action":"resume", "id":"valid"}), state.clone())
        .await
        .unwrap_err();
    assert!(error.contains("Save a run configuration"), "{error}");
    assert!(
        execute(
            json!({"action":"update", "id":"valid", "enabled":true}),
            state
        )
        .await
        .is_err()
    );
    assert!(!DefinitionStore::new().get("valid").unwrap().enabled);
}

// Catches: list advertising an executable occurrence for a configuration rejected by launch.
#[tokio::test]
async fn automation_list_and_runner_resolve_run_configs_the_same_way() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_owned());
    std::fs::write(
        dir.path().join("agents.json"),
        serde_json::to_vec(&saved_profile_config()).unwrap(),
    )
    .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    for (id, profile) in [
        ("valid", "review PROFILE"),
        ("legacy", "claude"),
        ("retired", "Retired profile"),
    ] {
        let mut input = definition(id);
        input["run_config"] = json!(profile);
        DefinitionStore::new()
            .create(serde_json::from_value(input).unwrap())
            .unwrap();
    }
    let listed = execute(json!({"action":"list"}), state.clone())
        .await
        .unwrap();
    let rows = listed.as_array().unwrap();
    let valid = rows
        .iter()
        .find(|row| row["definition"]["id"] == "valid")
        .unwrap();
    assert_eq!(valid["run_config_error"], Value::Null);
    assert!(valid["next_run_ms"].is_number());
    for (id, profile) in [("legacy", "claude"), ("retired", "Retired profile")] {
        let error = crate::mcp_http::managed_launch::launch_named(
            &state,
            profile,
            "unused",
            "unused",
            "/unused",
            "unlaunched-profile-test",
        )
        .unwrap_err();
        let row = rows
            .iter()
            .find(|row| row["definition"]["id"] == id)
            .unwrap();
        assert_eq!(
            row["run_config_error"].as_str(),
            Some(error.as_str()),
            "list hid the runner refusal: {row}"
        );
        assert_eq!(row["next_run_ms"], Value::Null);
        assert!(error.contains("Review profile"));
    }
}

// Catches: validating a create before the store resolves omitted/empty host-local timezone.
#[tokio::test]
async fn create_cron_with_omitted_or_empty_timezone_keeps_host_default() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_owned());
    std::fs::write(
        dir.path().join("agents.json"),
        serde_json::to_vec(&saved_profile_config()).unwrap(),
    )
    .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    let host_zone = iana_time_zone::get_timezone()
        .expect("host must expose an IANA timezone for the local-default contract");
    for (index, (once, omitted)) in [(false, true), (false, false), (true, true), (true, false)]
        .into_iter()
        .enumerate()
    {
        let id = format!("local-zone-{index}");
        let mut input = definition(&id);
        input["run_config"] = json!("Review profile");
        if omitted {
            input.as_object_mut().unwrap().remove("timezone");
        } else {
            input["timezone"] = json!("");
        }
        if once {
            input["cron"] = json!("");
            input["once_local"] = json!("2099-06-15T12:00:00");
        }
        let saved = execute(
            json!({"action":"create", "definition":input}),
            state.clone(),
        )
        .await
        .expect("create rejected the host-local timezone default before store resolution");
        assert_eq!(saved["timezone"], host_zone);
        assert_eq!(DefinitionStore::new().get(&id).unwrap().timezone, host_zone);
    }
    let mut explicit = definition("explicit-zone");
    explicit["run_config"] = json!("Review profile");
    explicit["timezone"] = json!("Pacific/Auckland");
    let saved = execute(
        json!({"action":"create", "definition":explicit}),
        state.clone(),
    )
    .await
    .unwrap();
    assert_eq!(saved["timezone"], "Pacific/Auckland");
    let before = std::fs::read(dir.path().join("automations.json")).unwrap();
    explicit["id"] = json!("invalid-zone");
    explicit["timezone"] = json!("Not/AZone");
    assert!(
        execute(json!({"action":"create", "definition":explicit}), state)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(dir.path().join("automations.json")).unwrap(),
        before
    );
}
