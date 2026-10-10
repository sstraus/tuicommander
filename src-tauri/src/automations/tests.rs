use super::definitions::DefinitionStore;
use super::model::{AutomationDefinition, AutomationsConfig};
use serde_json::{Value, json};

fn store() -> (tempfile::TempDir, DefinitionStore) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = DefinitionStore::at_path(dir.path().join("automations.json"));
    (dir, store)
}

fn definition(id: &str) -> AutomationDefinition {
    serde_json::from_value(json!({
        "id": id, "name": "Daily review", "prompt": "Review the repository",
        "run_config": "codex", "repository": "/project",
        "workspace": { "mode": "new_per_run", "base_branch": "main" },
        "cron": "0 9 * * 1-5", "timezone": "America/New_York",
        "enabled": true, "grace_secs": 43200, "overlap": "skip",
        "max_duration_secs": 3600,
        "precheck": { "command": "git diff --quiet", "timeout_secs": 30 }
    }))
    .unwrap()
}

// Catches: a Once definition becoming undispatchable or firing twice after restart/catch-up.
#[test]
fn once_occurrence_is_persisted_and_cannot_dispatch_twice_after_restart() {
    use super::run::RunTrigger;
    use super::store::RunOwner;
    let mut value = serde_json::to_value(definition("once")).unwrap();
    value["cron"] = json!("");
    value["once_local"] = json!("2099-10-09T10:00:00");
    value["timezone"] = json!("UTC");
    let once: AutomationDefinition = serde_json::from_value(value).unwrap();
    let (dir, definitions) = store();
    definitions.create(once.clone()).unwrap();
    assert_eq!(definitions.load().unwrap().definitions, vec![once.clone()]);
    let path = dir.path().join("runs.sqlite3");
    let occurrence_ms = chrono::DateTime::parse_from_rfc3339("2099-10-09T10:00:00Z")
        .unwrap()
        .timestamp_millis();
    let owner = RunOwner::acquire_at(&path, occurrence_ms).unwrap();
    assert!(
        owner
            .store()
            .reserve(
                &once,
                RunTrigger::Scheduled { occurrence_ms },
                occurrence_ms
            )
            .unwrap()
            .is_some()
    );
    drop(owner);
    let owner = RunOwner::acquire_at(&path, occurrence_ms + 1).unwrap();
    assert!(
        owner
            .store()
            .reserve(
                &once,
                RunTrigger::Scheduled { occurrence_ms },
                occurrence_ms + 1
            )
            .unwrap()
            .is_none()
    );
}

// Catches: deriving an unbounded/zero concurrency default or writing on read.
#[test]
fn absent_file_defaults_to_two_without_creating_a_definition_document() {
    let (dir, store) = store();
    let config = store.load().unwrap();
    assert_eq!(config.max_concurrent_runs, 2);
    assert!(config.definitions.is_empty());
    assert!(!dir.path().join("automations.json").exists());
}

// Catches: stale whole-document saves losing a second writer's definition or limit.
#[test]
fn independent_writers_keep_unrelated_definitions_and_global_limit() {
    let (dir, first) = store();
    let second = DefinitionStore::at_path(dir.path().join("automations.json"));
    first.create(definition("alpha")).unwrap();
    let _stale = first.load().unwrap();
    second.create(definition("beta")).unwrap();
    second.set_concurrency(4).unwrap();
    let mut edited = definition("alpha");
    edited.prompt = "Review only tests".into();
    first.update("alpha", edited.clone()).unwrap();
    let reloaded = second.load().unwrap();
    assert_eq!(reloaded.max_concurrent_runs, 4);
    assert_eq!(reloaded.definitions, vec![edited, definition("beta")]);
    first.remove("alpha").unwrap();
    assert_eq!(second.load().unwrap().definitions, vec![definition("beta")]);
    let disk: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("automations.json")).unwrap())
            .unwrap();
    assert_eq!(disk["definitions"][0]["workspace"]["base_branch"], "main");
    assert_eq!(disk["definitions"][0]["timezone"], "America/New_York");
    assert_eq!(disk["definitions"][0]["precheck"]["timeout_secs"], 30);
}

// Catches: check-then-save races admitting duplicate ids in simultaneous writers.
#[test]
fn concurrent_creates_claim_one_id_without_losing_other_ids() {
    let (dir, store) = store();
    let path = dir.path().join("automations.json");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let store = DefinitionStore::at_path(path.clone());
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.create(definition("same"))
            })
        })
        .collect();
    barrier.wait();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(store.load().unwrap().definitions, vec![definition("same")]);
}

// Catches: invalid edits silently replacing valid definitions or changing identity.
#[test]
fn invalid_edits_leave_the_existing_document_byte_identical() {
    let (dir, store) = store();
    store.create(definition("alpha")).unwrap();
    let path = dir.path().join("automations.json");
    let original = std::fs::read(&path).unwrap();
    for field in [
        "id",
        "name",
        "prompt",
        "run_config",
        "repository",
        "cron",
        "timezone",
    ] {
        let mut value = serde_json::to_value(definition("alpha")).unwrap();
        value[field] = json!("  ");
        let invalid = serde_json::from_value(value).unwrap();
        assert!(
            store.update("alpha", invalid).is_err(),
            "accepted blank {field}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    for field in ["grace_secs", "max_duration_secs"] {
        let mut value = serde_json::to_value(definition("alpha")).unwrap();
        value[field] = json!(0);
        assert!(
            store
                .update("alpha", serde_json::from_value(value).unwrap())
                .is_err()
        );
    }
    let mut invalid = definition("alpha");
    invalid.precheck.as_mut().unwrap().timeout_secs = 0;
    assert!(store.update("alpha", invalid).is_err());
    let mut invalid = definition("alpha");
    invalid.precheck.as_mut().unwrap().command = " ".into();
    assert!(store.update("alpha", invalid).is_err());
    let mut invalid = definition("alpha");
    invalid.workspace =
        serde_json::from_value(json!({"mode":"new_per_run", "base_branch":" "})).unwrap();
    assert!(store.update("alpha", invalid).is_err());
    assert!(store.create(definition("alpha")).is_err());
    assert!(store.update("alpha", definition("beta")).is_err());
    assert!(store.update("missing", definition("missing")).is_err());
    assert!(store.remove("missing").is_err());
    assert!(store.set_concurrency(0).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

// Catches: a failed corrupt load followed by a save overwriting recovery bytes.
#[test]
fn malformed_json_is_preserved_and_the_failing_mutation_does_not_write_defaults() {
    let (dir, store) = store();
    let path = dir.path().join("automations.json");
    std::fs::write(&path, b"{broken automation data").unwrap();
    assert!(store.create(definition("alpha")).is_err());
    assert!(!path.exists());
    let recovery: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".corrupt-")
        })
        .collect();
    assert_eq!(recovery.len(), 1);
    assert_eq!(
        std::fs::read(&recovery[0]).unwrap(),
        b"{broken automation data"
    );
}

// Catches: schema downgrade, duplicate identities or invalid disk data being overwritten.
#[test]
fn unsupported_or_invalid_documents_abort_without_replacing_disk_state() {
    for value in [
        json!({"version": 2, "max_concurrent_runs": 2, "definitions": []}),
        json!({"version": 1, "max_concurrent_runs": 0, "definitions": []}),
        json!({"version": 1, "max_concurrent_runs": 2, "definitions": [definition("a"), definition("a")]}),
    ] {
        let (dir, store) = store();
        let path = dir.path().join("automations.json");
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(store.load().is_err());
        assert!(store.create(definition("beta")).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

// Catches: existing workspace or optional precheck definitions not surviving persistence.
#[test]
fn existing_workspace_and_optional_fields_round_trip() {
    let (_dir, store) = store();
    let mut value = serde_json::to_value(definition("existing")).unwrap();
    value["workspace"] = json!({"mode": "existing"});
    value.as_object_mut().unwrap().remove("precheck");
    let def: AutomationDefinition = serde_json::from_value(value).unwrap();
    store.create(def.clone()).unwrap();
    assert_eq!(store.load().unwrap().definitions, vec![def]);
    let config: AutomationsConfig = serde_json::from_value(json!({})).unwrap();
    assert_eq!(config.max_concurrent_runs, 2);
}

// Catches: read or same-value update rewriting a document just to reformat it.
#[test]
fn no_op_edits_preserve_document_bytes() {
    let (dir, store) = store();
    let value = json!({ "version": 1, "max_concurrent_runs": 2, "definitions": [definition("a")] });
    let bytes = serde_json::to_vec(&value).unwrap();
    let path = dir.path().join("automations.json");
    std::fs::write(&path, &bytes).unwrap();
    store.load().unwrap();
    store.update("a", definition("a")).unwrap();
    store.set_concurrency(2).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

// Catches: missing required fields or unknown execution modes silently becoming executable.
#[test]
fn invalid_wire_definitions_do_not_deserialize_into_runnable_defaults() {
    for field in [
        "id",
        "prompt",
        "run_config",
        "repository",
        "workspace",
        "cron",
    ] {
        let mut value = serde_json::to_value(definition("a")).unwrap();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<AutomationDefinition>(value).is_err(),
            "accepted missing {field}"
        );
    }
    for (field, value) in [
        ("overlap", json!("queue")),
        ("workspace", json!({"mode":"reuse"})),
        ("unattended_permissions", json!(true)),
    ] {
        let mut input = serde_json::to_value(definition("a")).unwrap();
        input[field] = value;
        assert!(serde_json::from_value::<AutomationDefinition>(input).is_err());
    }
}
