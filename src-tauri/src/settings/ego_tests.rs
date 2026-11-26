use super::{ego_fixture::*, request::*};
use serde_json::json;

// Catches: the model owner ignores the configured profile and edits the base profile.
#[tokio::test]
async fn ego_profile_edit_cannot_target_other_profile() {
    let f = fixture();
    let base = disk(&f, "base");
    let beta = disk(&f, "beta");
    let reply = crate::ego_cli::set_default_model(&f.state, "ollama/changed".into())
        .await
        .unwrap();
    assert_eq!(disk(&f, "alpha")["values"]["model"], expected_model());
    assert_eq!(reply.default_model, Some("ollama/changed".into()));
    assert_eq!(disk(&f, "base"), base);
    assert_eq!(disk(&f, "beta"), beta);
    assert!(
        calls(&f)
            .iter()
            .all(|call| call["cwd"] == f.dir.path().to_string_lossy().as_ref())
    );
}

// Catches: an adapter edit clobbers sibling fields, another profile, or another store.
#[tokio::test]
async fn adapter_preserves_other_fields() {
    let f = fixture();
    let before = disk(&f, "alpha");
    let beta = disk(&f, "beta");
    let neighbour = f.dir.path().join("notifications.json");
    std::fs::write(&neighbour, "sentinel").unwrap();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let reply = set(&f, snapshot.revision, "/model", expected_model())
        .await
        .unwrap();
    assert!(reply.persisted && reply.applied);
    let mut expected = before;
    expected["values"]["model"] = expected_model();
    assert_eq!(disk(&f, "alpha"), expected);
    assert_eq!(disk(&f, "beta"), beta);
    assert_eq!(std::fs::read_to_string(neighbour).unwrap(), "sentinel");
    let after = get(&f, &["/model"]).await.unwrap();
    assert_eq!(after.values["/model"].stored, expected_model());
    assert_eq!(after.revision, reply.revision);
}

// Catches: a caller-selected executable is probed or a foreign profile is accepted.
#[tokio::test]
async fn ego_uses_trusted_binary_and_profile() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let before = calls(&f).len();
    let error = f
        .registry
        .execute(Request::Set {
            target: Target::Profile { id: "beta".into() },
            store: "ego".into(),
            path: "/model".into(),
            value: expected_model(),
            expected_revision: snapshot.revision.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "not_found");
    assert_eq!(
        calls(&f).len(),
        before,
        "foreign target must not launch a probe"
    );
    let error = set(
        &f,
        snapshot.revision,
        "/executable",
        json!("/untrusted/program"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "unknown_path");
    assert!(
        !calls(&f)
            .iter()
            .any(|c| c.to_string().contains("/untrusted/program"))
    );
}

// Catches: secret diagnostics or unknown owner fields escaping through reads or failures.
#[tokio::test]
async fn adapter_secret_fields_presence_only() {
    let f = fixture();
    let mut config = disk(&f, "alpha");
    config["values"]["unknown_secret"] = json!("canary-ego-secret-in-diagnostic");
    std::fs::write(f.dir.path().join("alpha.json"), config.to_string()).unwrap();
    let snapshot = get(
        &f,
        &[
            "/model",
            "/credentials/anthropic",
            "/credentials/connection:key_absent",
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        snapshot.values["/credentials/anthropic"].stored,
        json!({"status":"set"})
    );
    assert_eq!(
        snapshot.values["/credentials/connection:key_absent"].stored,
        json!({"status":"unset"})
    );
    let rejected = set(
        &f,
        snapshot.revision.clone(),
        "/credentials/anthropic",
        json!("canary-ego-secret-in-diagnostic"),
    )
    .await
    .unwrap_err();
    assert_eq!(rejected.code, "unavailable");
    assert!(
        !serde_json::to_string(&rejected)
            .unwrap()
            .contains("canary-ego-secret")
    );
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("canary-ego-secret")
    );
    std::fs::write(f.dir.path().join("fail"), "").unwrap();
    let error = get(&f, &["/model"]).await.unwrap_err();
    assert_eq!(error.code, "store_failed");
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("canary-ego-secret")
    );
}

// Catches: stale revisions write, or valid multi-key batches fail to publish together.
#[tokio::test]
async fn stale_revision_rejects_and_multi_key_batch_publishes() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let before = disk(&f, "alpha");
    let error = set(&f, "stale".into(), "/model", expected_model())
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    let reply = f
        .registry
        .execute(Request::Patch {
            target: Target::Profile { id: "alpha".into() },
            store: "ego".into(),
            expected_revision: snapshot.revision,
            operations: vec![
                Edit::Set {
                    path: "/model".into(),
                    value: expected_model(),
                },
                Edit::Set {
                    path: "/network".into(),
                    value: json!("on"),
                },
            ],
        })
        .await
        .unwrap();
    assert!(matches!(reply, Reply::Commit(_)));
    assert_ne!(disk(&f, "alpha"), before);
    assert_eq!(disk(&f, "alpha")["values"]["network"], json!("on"));
}

// Catches: unset is stored as a literal null, or explicit [] becomes absent/inherited roots.
#[tokio::test]
async fn reset_unsets_while_empty_roots_remain_explicit() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let reply = f
        .registry
        .execute(Request::Reset {
            target: Target::Profile { id: "alpha".into() },
            store: "ego".into(),
            path: "/roots".into(),
            expected_revision: snapshot.revision,
        })
        .await
        .unwrap();
    assert!(
        !disk(&f, "alpha")["values"]
            .as_object()
            .unwrap()
            .contains_key("roots")
    );
    let snapshot = get(&f, &["/roots"]).await.unwrap();
    assert_eq!(snapshot.values["/roots"].stored, json!(null));
    set(&f, snapshot.revision, "/roots", json!([]))
        .await
        .unwrap();
    assert_eq!(disk(&f, "alpha")["values"]["roots"], json!([]));
    assert!(
        !serde_json::to_string(&reply)
            .unwrap()
            .contains("canary-ego-secret")
    );
}

// Catches: malformed model/enum values reach an owner write.
#[tokio::test]
async fn invalid_values_and_wrong_scope_are_rejected_before_write() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    for (path, value) in [
        ("/model", json!("-flag")),
        ("/model", json!("missing-provider")),
        ("/network", json!("offline")),
    ] {
        assert!(
            set(&f, snapshot.revision.clone(), path, value)
                .await
                .is_err()
        );
    }
    assert!(!calls(&f).iter().any(|c| c["args"][1] == "set"));
    f.state.config.write().ego_profile = "beta".into();
    let before = calls(&f).len();
    let error = get(&f, &["/model"]).await.unwrap_err();
    assert_eq!(error.code, "not_found");
    assert_eq!(calls(&f).len(), before);
}

// Catches: a changed workspace/process authority is accepted under an earlier revision.
#[tokio::test]
async fn authority_change_invalidates_revision_and_missing_binary_does_not_probe() {
    let f = fixture();
    let snapshot = get(&f, &["/model"]).await.unwrap();
    let workspace = f.dir.path().join("new-workspace");
    std::fs::create_dir(&workspace).unwrap();
    f.state.config.write().ai_chat_workspace = workspace.to_string_lossy().into_owned();
    let error = set(&f, snapshot.revision, "/model", expected_model())
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    f.state.config.write().ego_executable.clear();
    let before = calls(&f).len();
    assert_eq!(get(&f, &["/model"]).await.unwrap_err().code, "unavailable");
    assert_eq!(calls(&f).len(), before);
}

// Catches: managed keys are misrouted, root paths become TOML injection, or unset stops working.
#[tokio::test]
async fn managed_keys_roundtrip_as_closed_assignments() {
    let f = fixture();
    for (path, value) in [
        ("/network", json!("on")),
        (
            "/roots",
            json!([{"path":"/data/a\"},network=\"on", "access":"read"}]),
        ),
    ] {
        let before = get(&f, &[path]).await.unwrap();
        let reply = set(&f, before.revision, path, value.clone()).await.unwrap();
        let after = get(&f, &[path]).await.unwrap();
        assert_eq!(after.values[path].stored, value);
        assert_eq!(after.revision, reply.revision);
    }
    let disk = disk(&f, "alpha");
    assert_eq!(disk["values"]["network"], json!("on"));
    assert_eq!(
        disk["values"]["roots"][0]["path"],
        json!("/data/a\"},network=\"on")
    );
}

// Catches: the owner writes successfully but a failed readback invites replay or hides durability.
#[tokio::test]
async fn readback_failure_reports_durable_write() {
    let f = fixture();
    let before = get(&f, &["/model"]).await.unwrap();
    std::fs::write(f.dir.path().join("fail-after-write"), "").unwrap();
    let error = set(&f, before.revision, "/model", expected_model())
        .await
        .unwrap_err();
    assert!(error.persisted);
    assert_eq!(error.next_action, "get");
    assert_eq!(disk(&f, "alpha")["values"]["model"], expected_model());
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("canary-ego-secret")
    );
}

// Catches: the Providers tab shows no default model under a profile that inherits the
// base model (Ego resolves a missing profile model from the same layer's base value).
#[tokio::test]
async fn providers_default_model_falls_back_to_base_under_a_profile() {
    let f = fixture();
    let mut alpha = disk(&f, "alpha");
    alpha["values"].as_object_mut().unwrap().remove("model");
    std::fs::write(f.dir.path().join("alpha.json"), alpha.to_string()).unwrap();
    let mut base = disk(&f, "base");
    base["values"]["model"] = json!("ollama/base");
    std::fs::write(f.dir.path().join("base.json"), base.to_string()).unwrap();
    let providers = crate::ego_cli::providers(&f.state, false).await.unwrap();
    assert_eq!(providers.default_model, Some("ollama/base".into()));
}

// Catches: the Providers tab refuses a configured AI Chat workspace that ACP has not yet
// created, although ACP creates it on first connect.
#[tokio::test]
async fn providers_accept_a_workspace_not_yet_created() {
    let f = fixture();
    let workspace = f.dir.path().join("not-yet-created");
    f.state.config.write().ai_chat_workspace = workspace.to_string_lossy().into_owned();
    let providers = crate::ego_cli::providers(&f.state, false).await.unwrap();
    assert_eq!(providers.default_model, Some("ollama/qwen3".into()));
    assert!(workspace.is_dir());
}

// Catches: construction-time profile binding rejects a valid launch profile change.
#[tokio::test]
async fn ego_settings_follow_launch_profile_changes() {
    let f = fixture();
    get(&f, &["/model"]).await.unwrap();
    f.state.config.write().ego_profile = "beta".into();
    let reply = f
        .registry
        .execute(Request::Get {
            target: Target::Profile { id: "beta".into() },
            store: "ego".into(),
            paths: vec!["/model".into()],
        })
        .await
        .expect("current launch profile must resolve without rebuilding registry");
    assert!(matches!(reply, Reply::Snapshot(_)));
    assert_eq!(get(&f, &["/model"]).await.unwrap_err().code, "not_found");
}

// Catches: initial unconfigured Ego prevents registration and remains absent after setup.
#[tokio::test]
async fn ego_settings_register_before_first_configuration() {
    let f = fixture();
    let executable = f.state.config.read().ego_executable.clone();
    f.state.config.write().ego_executable.clear();
    let owner = super::ego::EgoSettingsStore::new(f.state.clone())
        .expect("registration must perform no configuration I/O");
    let service = super::service::Service::new(
        std::sync::Arc::new(super::native::native_catalogue().unwrap()),
        std::sync::Arc::new(owner),
    );
    assert_eq!(
        service
            .get(
                Target::Profile { id: "alpha".into() },
                "ego".into(),
                vec!["/model".into()]
            )
            .await
            .unwrap_err()
            .code,
        "unavailable"
    );
    f.state.config.write().ego_executable = executable;
    service
        .get(
            Target::Profile { id: "alpha".into() },
            "ego".into(),
            vec!["/model".into()],
        )
        .await
        .unwrap();
}

// Catches: new managed keys are missing or sent to another owner/profile.
#[tokio::test]
async fn ego_gap_keys_use_owner_target() {
    let f = fixture();
    let before = get(&f, &["/model"]).await.unwrap();
    let reply = set(&f, before.revision, "/exec_paths", json!(["/fixture/bin"]))
        .await
        .unwrap();
    assert!(reply.persisted);
    assert_eq!(
        disk(&f, "alpha")["values"]["exec_paths"],
        json!(["/fixture/bin"])
    );
    assert!(disk(&f, "base")["values"].get("exec_paths").is_none());
}

// Catches: an external owner write between snapshot and batch is overwritten.
#[tokio::test]
async fn ego_external_revision_conflict_writes_nothing() {
    let f = fixture();
    let before = get(&f, &["/model"]).await.unwrap();
    std::fs::write(f.dir.path().join("race-before-batch"), "").unwrap();
    let error = set(&f, before.revision, "/model", expected_model())
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    assert_eq!(
        disk(&f, "alpha")["values"]["model"],
        json!("ollama/external")
    );
}

// Catches: auth secrets leak in reads or profile selection redirects base-only edits.
#[tokio::test]
async fn ego_auth_actions_are_base_only_and_write_only() {
    let f = fixture();
    let owner = super::ego::EgoSettingsStore::new(f.state.clone()).unwrap();
    let snapshot = f
        .registry
        .execute(Request::Get {
            target: Target::Machine,
            store: "ego".into(),
            paths: vec!["/model".into()],
        })
        .await
        .unwrap();
    let Reply::Snapshot(snapshot) = snapshot else {
        panic!("snapshot")
    };
    let alpha = disk(&f, "alpha");
    let reply = tokio::task::spawn_blocking(move || {
        owner.auth_edit(
            &snapshot.revision,
            super::ego::EgoAuthAction::SetHeaders {
                provider: "openrouter".into(),
                headers: [("X-Identity".into(), "fixture-canary".into())].into(),
            },
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(reply.persisted);
    assert_eq!(disk(&f, "alpha"), alpha);
    assert_eq!(
        disk(&f, "base")["values"]["auth.openrouter.headers"],
        "<redacted>"
    );
    let reply = f
        .registry
        .execute(Request::Get {
            target: Target::Machine,
            store: "ego".into(),
            paths: vec!["/auth/openrouter/headers".into()],
        })
        .await
        .unwrap();
    let wire = serde_json::to_string(&reply).unwrap();
    assert!(!wire.contains("fixture-canary"));
    assert!(wire.contains("set"));
}

// Catches: captured owner redaction is mistaken for a writable secret value.
#[test]
fn ego_captured_owner_auth_is_redacted_and_capabilities_are_conditional() {
    let snapshot: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ego/snapshot-base-auth.json")).unwrap();
    assert_eq!(snapshot["values"]["auth.openai.headers"], "<redacted>");
    assert_eq!(
        snapshot["values"]["auth.openai.credential_command"],
        "<redacted>"
    );
    let schema = super::ego::EgoSettingsStore::auth_action_schema();
    let wire = schema.to_string();
    assert!(wire.contains("writeOnly"));
    let capabilities: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ego/capabilities.json")).unwrap();
    assert_eq!(capabilities["atomic_batch"], true);
    assert_eq!(capabilities["compare_and_swap"], true);
}

// Catches: a profile snapshot reports base-only provider authentication as unset.
#[tokio::test]
async fn ego_profile_read_must_not_report_base_auth_unset() {
    let f = fixture();
    let mut base = disk(&f, "base");
    base["values"]["auth.openai.headers"] = json!("<redacted>");
    std::fs::write(f.dir.path().join("base.json"), base.to_string()).unwrap();
    let result = get(&f, &["/auth/openai/headers"]).await;
    match result {
        Ok(snapshot) => assert_eq!(
            snapshot.values["/auth/openai/headers"].stored,
            json!({"status":"set"}),
            "base-only headers exist, but the profile read reports them unset"
        ),
        Err(error) => assert!(
            matches!(error.code.as_str(), "not_found" | "unavailable"),
            "an owner read failure does not prove profile scope rejection: {error}"
        ),
    }
}

// Catches: human headings, truncated IDs, or vendor flattening lose real inventory rows.
#[tokio::test]
async fn ego_inventory_recorded_named_legacy_and_source_presence() {
    let f = fixture();
    let long_path = format!("/credentials/connection:long_{}", "x".repeat(80));
    let snapshot = get(
        &f,
        &[
            "/credentials/anthropic",
            "/credentials/connection:anthropic",
            "/credentials/connection:account_a",
            "/credentials/connection:account_b",
            "/credentials/connection:key_absent",
            "/credentials/connection:key_blank",
            "/credentials/connection:key_empty",
            "/credentials/connection:key_present",
            "/credentials/connection:orphan",
            &long_path,
        ],
    )
    .await
    .unwrap();
    for path in [
        "/credentials/anthropic",
        "/credentials/connection:anthropic",
        "/credentials/connection:account_a",
        "/credentials/connection:key_present",
        "/credentials/connection:orphan",
        &long_path,
    ] {
        assert_eq!(
            snapshot.values[path].stored,
            json!({"status":"set"}),
            "{path}"
        );
    }
    for path in [
        "/credentials/connection:account_b",
        "/credentials/connection:key_absent",
        "/credentials/connection:key_blank",
        "/credentials/connection:key_empty",
    ] {
        assert_eq!(
            snapshot.values[path].stored,
            json!({"status":"unset"}),
            "{path}"
        );
    }
    let calls = calls(&f);
    assert!(
        calls
            .iter()
            .filter(|c| c["args"][0] == "auth")
            .all(|c| c["args"][1] == "status" && c["args"][2] == "--json")
    );
    assert!(!calls.iter().any(|c| c["args"][0] == "models"));
}

// Catches: equal-spelling named and legacy identities overwrite one another.
#[tokio::test]
async fn ego_inventory_named_presence_does_not_overwrite_legacy() {
    let f = fixture();
    // Deliberately divergent presence derived from the actual producer capture.
    let mut inventory: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ego/auth-inventory.json")).unwrap();
    let named = inventory["credentials"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["connection"] == "anthropic")
        .unwrap();
    named["present"] = json!(false);
    std::fs::write(f.dir.path().join("auth.json"), inventory.to_string()).unwrap();
    let snapshot = get(
        &f,
        &[
            "/credentials/anthropic",
            "/credentials/connection:anthropic",
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        snapshot.values["/credentials/anthropic"].stored,
        json!({"status":"set"})
    );
    assert_eq!(
        snapshot.values["/credentials/connection:anthropic"].stored,
        json!({"status":"unset"})
    );
}

// Catches: named vendors, account IDs, orphan rows, or model sources invent legacy edit targets.
#[tokio::test]
async fn ego_inventory_only_legacy_rows_project_auth_targets() {
    use super::service::Store;
    let f = fixture();
    let owner = std::sync::Arc::new(super::ego::EgoSettingsStore::new(f.state.clone()).unwrap())
        .resolve(&Target::Machine)
        .unwrap();
    let document = tokio::task::spawn_blocking(move || owner.read())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        document["auth"]["anthropic"]["headers"],
        serde_json::Value::Null
    );
    assert!(document["auth"].get("anthropic").is_some());
    assert!(document["auth"].get("openai-codex").is_some());
    for key in [
        "account_a",
        "account_b",
        "openai",
        "orphan",
        "connection:anthropic",
        "key_absent",
    ] {
        assert!(
            document["auth"].get(key).is_none(),
            "fabricated auth target {key}"
        );
    }
    assert_eq!(document["auth"].as_object().unwrap().len(), 2);
}

// Catches: invalid machine output silently becomes empty inventory or falls back to human TSV.
#[tokio::test]
async fn ego_inventory_invalid_machine_output_fails_sanitized() {
    let f = fixture();
    let capture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ego/auth-inventory.json")).unwrap();
    let mut cases = Vec::new();
    for (label, field, value) in [
        ("version", "v", json!(2)),
        ("missing rows", "credentials", serde_json::Value::Null),
    ] {
        let mut invalid = capture.clone();
        invalid[field] = value;
        cases.push((label, invalid.to_string()));
    }
    for (label, field, value) in [
        ("empty vendor", "provider", json!("")),
        ("null vendor", "provider", serde_json::Value::Null),
        ("null connection", "connection", serde_json::Value::Null),
        ("human label", "provider", json!("legacy anthropic")),
        ("bad connection type", "connection", json!(42)),
        ("empty connection", "connection", json!("")),
        ("path connection", "connection", json!("../canary-secret")),
        ("control connection", "connection", json!("canary-secret\n")),
        ("bad presence", "present", json!("canary-secret")),
    ] {
        let mut invalid = capture.clone();
        invalid["credentials"][0][field] = value;
        cases.push((label, invalid.to_string()));
    }
    let mut missing = capture.clone();
    missing["credentials"][0]
        .as_object_mut()
        .unwrap()
        .remove("provider");
    cases.push(("missing identity", missing.to_string()));
    let mut duplicate = capture.clone();
    let row = duplicate["credentials"][0].clone();
    duplicate["credentials"].as_array_mut().unwrap().push(row);
    cases.push(("duplicate identity", duplicate.to_string()));
    cases.extend([
        ("human TSV", "anthropic\toauth\n".into()),
        ("invalid JSON", "canary-secret".into()),
        ("empty stdout", String::new()),
    ]);
    let mut failures = Vec::new();
    for (label, raw) in cases {
        std::fs::write(f.dir.path().join("auth.json"), raw).unwrap();
        match get(&f, &["/model"]).await {
            Err(error) if error.code == "store_failed" => {
                assert!(
                    !serde_json::to_string(&error)
                        .unwrap()
                        .contains("canary-secret")
                );
            }
            result => failures.push(format!("{label}: {result:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        calls(&f)
            .iter()
            .filter(|c| c["args"][0] == "auth")
            .all(|c| c["args"][2] == "--json")
    );
}

// Catches: a valid empty inventory fails or model sources fabricate credential rows.
#[tokio::test]
async fn ego_inventory_valid_empty_keeps_managed_settings_available() {
    let f = fixture();
    let mut inventory: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ego/auth-inventory.json")).unwrap();
    inventory["credentials"].as_array_mut().unwrap().clear();
    std::fs::write(f.dir.path().join("auth.json"), inventory.to_string()).unwrap();
    assert!(get(&f, &["/model"]).await.is_ok());
    let error = get(&f, &["/credentials/anthropic"]).await.unwrap_err();
    assert_eq!(error.code, "not_found");
}
