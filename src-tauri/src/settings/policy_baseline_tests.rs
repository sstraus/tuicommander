//! Baseline-callable contracts; production owners and isolated config files only.
use super::{native::native_catalogue, request::*, service::Service};
use crate::config::{self, settings_app::AppSettingsStore};
use serde_json::json;
use std::sync::Arc;

pub(super) fn fixture() -> (
    tempfile::TempDir,
    impl Drop,
    Arc<crate::AppState>,
    Arc<Service<AppSettingsStore>>,
) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = config::set_config_dir_override(dir.path().to_path_buf());
    crate::credentials::reset_test_faults();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(AppSettingsStore::new(state.clone())),
    )
    .with_policy(state.settings_policy.clone());
    (dir, guard, state, Arc::new(service))
}

pub(super) async fn revision(service: &Service<AppSettingsStore>) -> String {
    service
        .get(Target::Machine, "app".into(), vec!["/font_size".into()])
        .await
        .unwrap()
        .revision
}

// Catches: exposure/execution edits persisted without a human approval.
#[tokio::test]
#[serial_test::serial]
async fn exposure_setting_requires_confirm() {
    let (dir, _guard, _state, service) = fixture();
    let rev = revision(&service).await;
    let result = service
        .patch(
            Target::Machine,
            "app".into(),
            rev,
            vec![Edit::Set {
                path: "/services/auth/username".into(),
                value: json!("verification-user"),
            }],
        )
        .await;
    assert!(
        result.is_err(),
        "exposure edit must be refused until approved"
    );
    assert_eq!(result.unwrap_err().code, "confirmation_required");
    assert!(!dir.path().join("config.json").exists());
}

// Catches: an ordinary preference incorrectly requiring a human approval.
#[tokio::test]
#[serial_test::serial]
async fn ordinary_setting_commits_without_confirm() {
    let (dir, _guard, _state, service) = fixture();
    let rev = revision(&service).await;
    let reply = service
        .patch(
            Target::Machine,
            "app".into(),
            rev,
            vec![Edit::Set {
                path: "/font_size".into(),
                value: json!(21),
            }],
        )
        .await
        .unwrap();
    assert!(reply.persisted);
    let disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(disk["font_size"], json!(21));
}

// Catches: a caller-supplied boolean bypassing the human confirmation path.
#[tokio::test]
#[serial_test::serial]
async fn model_supplied_confirmed_flag_ignored() {
    let (dir, _guard, _state, service) = fixture();
    let request = json!({"action":"set", "target":{"kind":"machine"}, "store":"app",
        "path":"/services/auth/username", "value":"verification-user", "expectedRevision":revision(&service).await,
        "confirmed":true});
    assert!(serde_json::from_value::<Request>(request).is_err());
    assert!(!dir.path().join("config.json").exists());
}

// Catches: a reveal flag exposing a write-only credential.
#[tokio::test]
#[serial_test::serial]
async fn write_only_reveal_denied() {
    let (_dir, _guard, _state, service) = fixture();
    let request = json!({"action":"get", "target":{"kind":"machine"}, "store":"app",
        "paths":["/services/auth/session_token"], "revealSecrets":true});
    assert!(serde_json::from_value::<Request>(request).is_err());
    let snapshot = service
        .get(
            Target::Machine,
            "app".into(),
            vec!["/services/auth/session_token".into()],
        )
        .await
        .unwrap();
    assert_eq!(
        snapshot.values["/services/auth/session_token"].stored,
        json!({"status":"unset"})
    );
}

// Catches: downloaded configuration or model request silently enables settings/diagnostics.
#[tokio::test]
#[serial_test::serial]
async fn settings_migration_preserves_default_off_and_explicit_deny() {
    let (dir, _guard, _state, service) = fixture();
    let path = dir.path().join("config.json");
    // Saved policy fields -> (one visible opt-in, remaining denials) after load.
    for (fields, enabled, denied) in [
        // Without a saved policy the old default denied settings.
        (
            json!({"settings_mcp_enabled": true}),
            false,
            vec!["diagnostics"],
        ),
        (json!({}), false, vec!["diagnostics"]),
        (
            json!({"settings_mcp_enabled": true, "mcp_capabilities": {"denied": ["settings", "diagnostics"]}}),
            false,
            vec!["diagnostics"],
        ),
        (
            json!({"settings_mcp_enabled": false, "mcp_capabilities": {"denied": ["settings"]}}),
            false,
            vec![],
        ),
        (
            json!({"settings_mcp_enabled": true, "mcp_capabilities": {"denied": ["diagnostics"]}}),
            true,
            vec!["diagnostics"],
        ),
        (
            json!({"settings_mcp_enabled": false, "mcp_capabilities": {"denied": []}}),
            false,
            vec![],
        ),
        (
            json!({"settings_mcp_enabled": true, "disabled_native_tools": ["settings", "debug"]}),
            false,
            vec!["diagnostics"],
        ),
        (
            json!({"settings_mcp_enabled": true, "disabled_native_tools": ["debug"]}),
            true,
            vec!["diagnostics"],
        ),
    ] {
        let mut doc = json!({"font_family": "Menlo", "font_size": 14, "theme": "dark"});
        for (key, value) in fields.as_object().unwrap() {
            doc[key] = value.clone();
        }
        std::fs::write(&path, doc.to_string()).unwrap();
        let loaded = config::load_app_config();
        assert_eq!(loaded.settings_mcp_enabled, enabled, "{fields}");
        assert_eq!(loaded.mcp_capabilities.denied, denied, "{fields}");
        // Deterministic: a second load reads the same policy and rewrites nothing.
        let saved = std::fs::read(&path).unwrap();
        let again = config::load_app_config();
        assert_eq!(again.settings_mcp_enabled, enabled, "{fields}");
        assert_eq!(again.mcp_capabilities, loaded.mcp_capabilities, "{fields}");
        assert_eq!(std::fs::read(&path).unwrap(), saved, "{fields}");
    }
    // A settings request cannot turn the opt-in on.
    std::fs::write(
        &path,
        json!({"font_family": "Menlo", "font_size": 14, "theme": "dark"}).to_string(),
    )
    .unwrap();
    let rev = revision(&service).await;
    let result = service
        .patch(
            Target::Machine,
            "app".into(),
            rev,
            vec![Edit::Set {
                path: "/settings_mcp_enabled".into(),
                value: json!(true),
            }],
        )
        .await;
    assert!(result.is_err(), "settings opt-in must stay host-only");
    assert!(!config::load_app_config().settings_mcp_enabled);
}
