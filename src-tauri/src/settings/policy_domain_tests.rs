//! Domain execution/exposure classes, through their real file owners.
use super::{
    native::native_catalogue, policy::Caller, policy_baseline_tests::fixture, request::*,
    service::Service,
};
use serde_json::{Value, json};
use std::sync::Arc;

async fn read<S: super::service::Store>(service: &Service<S>, store: &str, path: &str) -> String {
    service
        .get(Target::Machine, store.into(), vec![path.into()])
        .await
        .unwrap()
        .revision
}

// Catches: stdio env/cwd/enable/transport changes executing code outside the command guard.
#[tokio::test]
#[serial_test::serial]
async fn upstream_stdio_class_requires_confirmation() {
    let (dir, _guard, state, _app) = fixture();
    let file = dir.path().join("mcp-upstreams.json");
    std::fs::write(&file, json!({"servers":[{"id":"policy-stdio", "name":"policy_stdio", "enabled":false,
        "transport":{"type":"stdio", "command":"verification-executable", "args":[], "env":{}, "cwd":null}}]}).to_string()).unwrap();
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(super::upstreams::UpstreamsSettingsStore::at_path(
            file.clone(),
            Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            3845,
        )),
    )
    .with_policy(state.settings_policy.clone());
    let bytes = std::fs::read(&file).unwrap();
    for (path, value) in [
        (
            "/servers/policy-stdio/transport/command",
            json!("/verification/new"),
        ),
        ("/servers/policy-stdio/transport/args", json!(["--review"])),
        (
            "/servers/policy-stdio/transport/env/PATH",
            json!("/verification/bin"),
        ),
        (
            "/servers/policy-stdio/transport/cwd",
            json!("/verification"),
        ),
        ("/servers/policy-stdio/enabled", json!(true)),
    ] {
        let error = service
            .patch(
                Target::Machine,
                "upstreams".into(),
                read(&service, "upstreams", "/servers/policy-stdio/enabled").await,
                vec![Edit::Set {
                    path: path.into(),
                    value,
                }],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "confirmation_required", "{path}");
        let preview = state
            .settings_policy
            .begin_confirmation(&Caller::native(), error.proposal_id.as_deref().unwrap())
            .unwrap();
        if path.ends_with("/command") {
            assert!(preview.message.contains("/verification/new"));
            assert!(!preview.message.contains("null → null"));
        }
        if path.contains("/env/") || path.ends_with("/args") {
            assert!(preview.message.contains("[redacted]"));
            assert!(!preview.message.contains("/verification/bin"));
            assert!(!preview.message.contains("--review"));
        }
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }
    // Replacing stdio execution with an outbound HTTP destination narrows authority.
    let reply = service
        .patch(
            Target::Machine,
            "upstreams".into(),
            read(&service, "upstreams", "/servers/policy-stdio/enabled").await,
            vec![
                Edit::Set {
                    path: "/servers/policy-stdio/transport/type".into(),
                    value: json!("http"),
                },
                Edit::Set {
                    path: "/servers/policy-stdio/transport/url".into(),
                    value: json!("https://verification.example/mcp"),
                },
            ],
        )
        .await
        .unwrap();
    assert!(reply.persisted);
}

// Catches: treating an outbound HTTP destination as local execution or inbound exposure.
#[tokio::test]
#[serial_test::serial]
async fn upstream_http_destination_is_immediate() {
    let (dir, _guard, state, _app) = fixture();
    let file = dir.path().join("mcp-upstreams.json");
    std::fs::write(
        &file,
        json!({"servers":[{"id":"policy-http", "name":"policy_http", "enabled":false,
        "transport":{"type":"http", "url":"https://old.example/mcp"}}]})
        .to_string(),
    )
    .unwrap();
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(super::upstreams::UpstreamsSettingsStore::at_path(
            file.clone(),
            Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            3845,
        )),
    )
    .with_policy(state.settings_policy.clone());
    let path = "/servers/policy-http/transport/url";
    assert!(
        service
            .patch(
                Target::Machine,
                "upstreams".into(),
                read(&service, "upstreams", path).await,
                vec![Edit::Set {
                    path: path.into(),
                    value: json!("https://new.example/mcp")
                }]
            )
            .await
            .unwrap()
            .persisted
    );
    let disk: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(
        disk["servers"][0]["transport"]["url"],
        json!("https://new.example/mcp")
    );
}

// Catches: tunnel auto-connect or port edits bypassing the exposure proposal.
#[tokio::test]
#[serial_test::serial]
async fn tunnel_class_requires_confirmation_after_owner_validation() {
    let (dir, _guard, state, _app) = fixture();
    let profile = crate::tunnels::profile::TunnelProfile::new(
        "Policy",
        "verification.example",
        "verification",
    );
    crate::tunnels::storage::ProfileStore::save(dir.path(), &profile).unwrap();
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(super::remote::TunnelsSettingsStore::new(
            dir.path().to_owned(),
            Arc::new(|_| {}),
        )),
    )
    .with_policy(state.settings_policy.clone());
    for (leaf, value) in [("port", json!(2222)), ("auto_connect", json!(true))] {
        let path = format!("/profiles/{}/{leaf}", profile.id);
        let error = service
            .patch(
                Target::Machine,
                "tunnels".into(),
                read(&service, "tunnels", &path).await,
                vec![Edit::Set { path, value }],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "confirmation_required");
        let saved = crate::tunnels::storage::ProfileStore::load_all(dir.path(), None).unwrap();
        assert_eq!(saved[0].port, 22);
        assert!(!saved[0].auto_connect);
    }
    let path = format!("/profiles/{}/port", profile.id);
    let error = service
        .patch(
            Target::Machine,
            "tunnels".into(),
            read(&service, "tunnels", &path).await,
            vec![Edit::Set {
                path,
                value: json!(0),
            }],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "out_of_range");
    assert_eq!(state.settings_policy.audit().len(), 2);
}

// Catches: remote MCP peer identity or a disabled tool gaining write authority.
#[tokio::test]
#[serial_test::serial]
async fn disabled_mcp_and_unauthenticated_http_cannot_write() {
    let (dir, _guard, _state, service) = fixture();
    for caller in [
        Caller::mcp("disabled".into(), false),
        Caller::http(String::new()),
    ] {
        let request = Request::Set {
            target: Target::Machine,
            store: "app".into(),
            expected_revision: read(&service, "app", "/font_size").await,
            path: "/font_size".into(),
            value: json!(21),
        };
        assert!(service.execute_as(request, &caller, None).await.is_err());
        assert!(!dir.path().join("config.json").exists());
    }
}
