//! Critic regressions for the settings transaction foundation (1676-1230).
use super::{native::native_catalogue, request::*, service::Service};
use crate::config::{self, settings_app::AppSettingsStore};
use serde_json::json;
use std::sync::Arc;

const DURATION: &str = "/services/auth/session_token_duration_secs";

/// Catches: the adapter reports a committed value and revision that its own load
/// normalization rewrites, so a 24h phone-session lifetime silently becomes 30 days and
/// the returned revision conflicts with the next authoritative read.
#[tokio::test]
#[serial_test::serial]
async fn committed_24h_session_duration_reads_back_unchanged() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(AppSettingsStore::new(state.clone())),
    )
    .with_policy(state.settings_policy.clone());
    let get = || service.get(Target::Machine, "app".into(), vec![DURATION.into()]);
    let before = get().await.unwrap();
    let request = Request::Patch {
        target: Target::Machine,
        store: "app".into(),
        expected_revision: before.revision,
        operations: vec![Edit::Set {
            path: DURATION.into(),
            value: json!(86_400),
        }],
    };
    // 24h is below the 30-day default, so lowering the lifetime needs no confirmation.
    let result = service.execute(request).await;
    let revision = match result {
        Ok(Reply::Commit(reply)) => reply.revision,
        Ok(Reply::Snapshot(_)) => panic!("patch returned a snapshot"),
        Err(error) if error.persisted => error.revision.unwrap(),
        Err(error) => panic!("explicit 24h lifetime must be accepted: {error}"),
    };
    let after = get().await.unwrap();
    assert_eq!(
        after.values[DURATION].stored,
        json!(86_400),
        "a persisted 24h session lifetime must not read back as another value"
    );
    assert_eq!(
        after.revision, revision,
        "the committed revision must match immediate readback"
    );
}

// Catches: ordinary UI saves omit migration state and make explicit 24h values legacy again.
#[test]
#[serial_test::serial]
fn ordinary_save_marks_explicit_24h_duration_as_current() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let mut value = config::AppConfig {
        session_duration_migrated: false,
        ..config::AppConfig::default()
    };
    value.services.auth.session_token_duration_secs = 86_400;
    config::save_app_config(value).unwrap();
    assert_eq!(
        config::load_app_config()
            .services
            .auth
            .session_token_duration_secs,
        86_400
    );
}
