//! Add-time vault/config lifecycle regressions. Uses the existing mock vault.
use super::*;
use crate::mcp_upstream_credentials::{
    delete_upstream_credential, read_credential_for_url, save_credential_for_url,
};

fn server(id: &str, name: &str, url: &str) -> UpstreamMcpServer {
    UpstreamMcpServer {
        id: id.into(),
        name: name.into(),
        transport: UpstreamTransport::Http { url: url.into() },
        enabled: false,
        timeout_secs: 30,
        tool_filter: None,
        auth: Some(UpstreamAuth::Bearer {
            token: String::new(),
        }),
        headers: vec![],
    }
}

fn secret(id: &str, token: &str) -> UpstreamCredentialWrite {
    UpstreamCredentialWrite {
        server_id: id.into(),
        token: token.into(),
        header: None,
    }
}

// Catches: credential writes escaping validation of duplicate names, including
// a bearer add destroying an existing OAuth blob at the same origin.
#[test]
#[serial_test::serial]
fn rejected_duplicate_add_preserves_bearer_and_oauth_credentials() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    let url = "https://original.example/mcp";
    for original in [
        "original-bearer",
        r#"{"type":"oauth2","access_token":"original-oauth","token_endpoint":"https://original.example/token","client_id":"client"}"#,
    ] {
        for duplicate_url in [url, "https://other.example/mcp"] {
            let existing = server("existing", "duplicate-lifecycle", url);
            let base = UpstreamMcpConfig {
                servers: vec![existing.clone()],
            };
            ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
                .save(&base)
                .unwrap();
            save_credential_for_url(&existing.name, original, url).unwrap();
            let mut desired = base.clone();
            desired
                .servers
                .push(server("new", &existing.name, duplicate_url));
            let error = persist_upstream_delta_with_credentials(
                &base,
                &desired,
                9876,
                vec![secret("new", "replacement")],
            )
            .unwrap_err();
            assert!(error.contains("Duplicate server name"), "{error}");
            assert_eq!(load_mcp_upstreams(), base);
            assert_eq!(
                read_credential_for_url(&existing.name, url)
                    .unwrap()
                    .as_deref(),
                Some(original)
            );
            delete_upstream_credential(&existing.name).unwrap();
        }
    }
}

// Catches: two stale clients validating outside the file lock and replacing the
// winner's secret when only one same-name add can be persisted.
#[test]
#[serial_test::serial]
fn concurrent_duplicate_add_keeps_the_winning_config_and_secret() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let handles: Vec<_> = [("first", "first-secret"), ("second", "second-secret")]
            .into_iter()
            .map(|(id, token)| {
                let barrier = &barrier;
                scope.spawn(move || {
                    let desired = UpstreamMcpConfig {
                        servers: vec![server(
                            id,
                            "concurrent-add-lifecycle",
                            "https://example.com/mcp",
                        )],
                    };
                    barrier.wait();
                    (
                        id,
                        token,
                        persist_upstream_delta_with_credentials(
                            &UpstreamMcpConfig::default(),
                            &desired,
                            9876,
                            vec![secret(id, token)],
                        ),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|o| o.2.is_ok()).count(), 1);
    let winner = outcomes.iter().find(|o| o.2.is_ok()).unwrap();
    let saved = load_mcp_upstreams();
    assert_eq!(saved.servers.len(), 1);
    assert_eq!(saved.servers[0].id, winner.0);
    assert_eq!(
        read_credential_for_url("concurrent-add-lifecycle", "https://example.com/mcp")
            .unwrap()
            .as_deref(),
        Some(winner.1)
    );
    delete_upstream_credential("concurrent-add-lifecycle").unwrap();
}

// Catches: failed config persistence leaving newly created secrets behind, or
// modifying a pre-existing secret when the add could not be committed.
#[test]
#[cfg(unix)]
#[serial_test::serial]
fn failed_config_save_rolls_back_new_secret_and_preserves_existing_secret() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    let url = "https://example.com/mcp";
    let base = UpstreamMcpConfig {
        servers: vec![server("existing", "save-failure-existing", url)],
    };
    ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
        .save(&base)
        .unwrap();
    save_credential_for_url("save-failure-existing", "original", url).unwrap();
    let mut desired = base.clone();
    desired.servers.push(server("new", "save-failure-new", url));
    let permissions = std::fs::metadata(tmp.path()).unwrap().permissions();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = persist_upstream_delta_with_credentials(
        &base,
        &desired,
        9876,
        vec![secret("new", "new-secret")],
    );
    std::fs::set_permissions(tmp.path(), permissions).unwrap();
    let error = result.unwrap_err();
    assert!(error.contains("Failed to write temp file"), "{error}");
    assert_eq!(load_mcp_upstreams(), base);
    assert_eq!(
        read_credential_for_url("save-failure-new", url).unwrap(),
        None
    );
    assert_eq!(
        read_credential_for_url("save-failure-existing", url)
            .unwrap()
            .as_deref(),
        Some("original")
    );
    delete_upstream_credential("save-failure-existing").unwrap();
}

// Catches: a later credential failure leaking the earlier header write, and
// secrets or bearer/header bindings being lost in a successful compound add.
#[test]
#[serial_test::serial]
fn compound_add_rolls_back_partial_vault_writes_and_can_retry() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    let url = "https://example.com/mcp";
    let header = UpstreamHeader {
        name: "x-api-key".into(),
        credential_ref: uuid::Uuid::new_v4().to_string(),
    };
    let mut new = server("new", "compound-add-lifecycle", url);
    new.headers.push(header.clone());
    let header2 = UpstreamHeader {
        name: "x-other".into(),
        credential_ref: uuid::Uuid::new_v4().to_string(),
    };
    new.headers.push(header2.clone());
    let desired = UpstreamMcpConfig { servers: vec![new] };
    let writes = || {
        vec![
            UpstreamCredentialWrite {
                server_id: "new".into(),
                token: "header-secret".into(),
                header: Some(header.clone()),
            },
            secret("new", "bearer-secret"),
        ]
    };
    let mut failing = writes();
    failing[1] = UpstreamCredentialWrite {
        server_id: "new".into(),
        token: "bad\nvalue".into(),
        header: Some(header2.clone()),
    };
    let error = persist_upstream_delta_with_credentials(
        &UpstreamMcpConfig::default(),
        &desired,
        9876,
        failing,
    )
    .unwrap_err();
    assert!(error.contains("Invalid custom header value"), "{error}");
    assert_eq!(
        load_mcp_upstreams().servers,
        [] as [crate::mcp_upstream_config::UpstreamMcpServer; 0]
    );
    assert_eq!(
        read_credential_for_url(&header.credential_key("compound-add-lifecycle"), url).unwrap(),
        None
    );
    persist_upstream_delta_with_credentials(
        &UpstreamMcpConfig::default(),
        &desired,
        9876,
        writes(),
    )
    .unwrap();
    assert_eq!(load_mcp_upstreams(), desired);
    assert_eq!(
        read_credential_for_url("compound-add-lifecycle", url)
            .unwrap()
            .as_deref(),
        Some("bearer-secret")
    );
    assert_eq!(
        read_credential_for_url(&header.credential_key("compound-add-lifecycle"), url)
            .unwrap()
            .as_deref(),
        Some("header-secret")
    );
    let disk = std::fs::read_to_string(tmp.path().join(UPSTREAMS_FILE)).unwrap();
    assert!(!disk.contains("bearer-secret") && !disk.contains("header-secret"));
    delete_upstream_credential("compound-add-lifecycle").unwrap();
    delete_upstream_credential(&header.credential_key("compound-add-lifecycle")).unwrap();
}

// Catches: an old add client binding a new token to an existing upstream that
// has no vault entry, before its duplicate config is rejected.
#[test]
#[serial_test::serial]
fn existing_upstream_without_vault_entry_requires_explicit_credential_edit() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    let url = "https://example.com/mcp";
    let config = UpstreamMcpConfig {
        servers: vec![server("existing", "existing-empty-vault", url)],
    };
    ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
        .save(&config)
        .unwrap();
    let result = crate::mcp_upstream_credentials::save_mcp_upstream_credential(
        "existing-empty-vault".into(),
        "unrelated-add-token".into(),
        None,
        url.into(),
    );
    assert!(result.unwrap_err().contains("explicit edit"));
    assert_eq!(
        read_credential_for_url("existing-empty-vault", url).unwrap(),
        None
    );
    assert_eq!(load_mcp_upstreams(), config);
}

// Catches: an orphan vault entry (no config entry) blocking a legitimate add of
// that name, or the add keeping the stale orphan secret.
#[test]
#[serial_test::serial]
fn add_replaces_orphan_vault_entry_for_unconfigured_name() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    let url = "https://example.com/mcp";
    save_credential_for_url("orphan-add-lifecycle", "orphan-original", url).unwrap();
    let desired = UpstreamMcpConfig {
        servers: vec![server("new", "orphan-add-lifecycle", url)],
    };
    persist_upstream_delta_with_credentials(
        &UpstreamMcpConfig::default(),
        &desired,
        9876,
        vec![secret("new", "fresh-secret")],
    )
    .unwrap();
    assert_eq!(load_mcp_upstreams(), desired);
    assert_eq!(
        read_credential_for_url("orphan-add-lifecycle", url)
            .unwrap()
            .as_deref(),
        Some("fresh-secret")
    );
    delete_upstream_credential("orphan-add-lifecycle").unwrap();
}

// Catches: durable vault writes followed by failed config persistence losing their refresh hint.
#[cfg(unix)]
#[tokio::test]
#[serial_test::serial]
async fn legacy_upstream_partial_vault_persistence_publishes_unapplied_hint() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
    crate::credentials::reset_test_faults();
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let base = UpstreamMcpConfig::default();
    ConfigFile::<UpstreamMcpConfig>::new(UPSTREAMS_FILE)
        .save(&base)
        .unwrap();
    let desired = UpstreamMcpConfig {
        servers: vec![server(
            "partial-new",
            "partial-legacy-publisher",
            "https://example.com/mcp",
        )],
    };
    let mut events = state.event_bus.subscribe();
    let permissions = std::fs::metadata(tmp.path()).unwrap().permissions();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = save_mcp_upstreams_with_credentials_inner(
        base.clone(),
        desired,
        vec![secret("partial-new", "partial-secret-canary")],
        &state,
    )
    .await;
    std::fs::set_permissions(tmp.path(), permissions).unwrap();
    assert!(result.unwrap_err().contains("Failed to write temp file"));
    assert_eq!(load_mcp_upstreams(), base);
    assert!(
        read_credential_for_url("partial-legacy-publisher", "https://example.com/mcp")
            .unwrap()
            .is_none()
    );
    let notice = std::iter::from_fn(|| events.try_recv().ok())
        .find_map(|event| match event {
            crate::state::AppEvent::SettingsChanged(notice) => Some(notice),
            _ => None,
        })
        .expect("partial vault persistence must invalidate readers even after rollback");
    assert_eq!(notice.section, "upstreams");
    assert!(notice.persisted);
    assert!(!notice.applied);
    assert!(
        !serde_json::to_string(&notice)
            .unwrap()
            .contains("partial-secret-canary")
    );
}
