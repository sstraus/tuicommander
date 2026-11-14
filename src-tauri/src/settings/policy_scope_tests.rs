//! Class coverage uses real native settings, owner validation and isolated durable files.
use super::{
    policy_baseline_tests::{fixture, revision},
    request::*,
};
use serde_json::{Value, json};

async fn refuses(paths: &[(&str, Value)]) {
    let (dir, _guard, _state, service) = fixture();
    let rev = revision(&service).await;
    for (path, value) in paths {
        let error = service
            .patch(
                Target::Machine,
                "app".into(),
                rev.clone(),
                vec![Edit::Set {
                    path: (*path).into(),
                    value: value.clone(),
                }],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "confirmation_required", "{path}");
        assert!(error.proposal_id.is_some());
        assert!(!dir.path().join("config.json").exists());
    }
}

// Catches: the listener gaining exposure through a port/IPv6 switch rather than enabled.
#[tokio::test]
#[serial_test::serial]
async fn listener_class_requires_confirmation() {
    refuses(&[
        ("/services/server/enabled", json!(true)),
        ("/services/server/port", json!(9981)),
        ("/services/server/ipv6_enabled", json!(true)),
    ])
    .await;
}

// Catches: protection weakening through bypass or session lifetime avoiding confirmation.
#[tokio::test]
#[serial_test::serial]
async fn auth_class_requires_confirmation() {
    refuses(&[
        ("/services/auth/lan_auth_bypass", json!(true)),
        (
            "/services/auth/session_token_duration_secs",
            json!(2_600_000),
        ),
        ("/services/auth/auth_rate_limit_max", json!(100)),
    ])
    .await;
}

// Catches: the TLS discriminator remaining ordinary while only certificate paths are gated.
#[tokio::test]
#[serial_test::serial]
async fn tls_class_requires_confirmation() {
    let (dir, _guard, state, service) = fixture();
    state.config.write().services.tls = serde_json::from_value(json!({"mode":"manual", "cert_path":"/verification/old-cert", "key_path":"/verification/old-key"})).unwrap();
    for path in ["/services/tls/cert_path", "/services/tls/key_path"] {
        let error = service
            .patch(
                Target::Machine,
                "app".into(),
                revision(&service).await,
                vec![Edit::Set {
                    path: path.into(),
                    value: json!("/verification/new-file"),
                }],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "confirmation_required");
        assert!(!dir.path().join("config.json").exists());
    }
}

// Catches: relay enablement or a replacement destination silently opening access.
#[tokio::test]
#[serial_test::serial]
async fn relay_class_requires_confirmation() {
    refuses(&[
        ("/services/relay/enabled", json!(true)),
        ("/services/relay/url", json!("https://relay.example")),
    ])
    .await;
}

// Catches: an executable, argument or enable flag changing who runs code without review.
#[tokio::test]
#[serial_test::serial]
async fn execution_class_requires_confirmation() {
    let executable = crate::test_support::test_temp_root().join("verification-ego");
    refuses(&[("/ego_executable", json!(executable))]).await;
    let (dir, _guard, state, service) = fixture();
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            revision(&service).await,
            vec![Edit::Set {
                path: "/ego_executable".into(),
                value: json!("relative-executable"),
            }],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "out_of_range");
    assert!(!dir.path().join("config.json").exists());
    state.config.write().custom_launchers = serde_json::from_value(json!([{"id":"policy-launcher","name":"Policy","executable":"/verification/old", "args":[], "enabled":false, "platform":null}])).unwrap();
    for (path, value) in [
        (
            "/custom_launchers/policy-launcher/executable",
            json!("/verification/new"),
        ),
        (
            "/custom_launchers/policy-launcher/args",
            json!(["--review"]),
        ),
        ("/custom_launchers/policy-launcher/enabled", json!(true)),
    ] {
        let error = service
            .patch(
                Target::Machine,
                "app".into(),
                revision(&service).await,
                vec![Edit::Set {
                    path: path.into(),
                    value,
                }],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "confirmation_required");
        assert!(!dir.path().join("config.json").exists());
    }
}

// Catches: a model-requested push provider becoming an outbound destination without the owner.
#[tokio::test]
#[serial_test::serial]
async fn push_provider_enrollment_requires_owner_confirmation() {
    refuses(&[(
        "/services/push/added_providers",
        json!(["push.example.net"]),
    )])
    .await;
}

// Catches: a URL, wildcard, bare TLD or empty entry being saved as a provider host.
#[tokio::test]
#[serial_test::serial]
async fn push_provider_enrollment_rejects_entries_that_are_not_hosts() {
    let (dir, _guard, _state, service) = fixture();
    for entry in [
        "https://push.example.net",
        "push.example.net/path",
        "*.example.net",
        ".example.net",
        "net",
        "",
        "push..example.net",
        "-push.example.net",
    ] {
        let error = service
            .patch(
                Target::Machine,
                "app".into(),
                revision(&service).await,
                vec![Edit::Set {
                    path: "/services/push/added_providers".into(),
                    value: json!([entry]),
                }],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "out_of_range", "{entry:?}");
        assert!(!dir.path().join("config.json").exists());
    }
}

// Catches: broad catalogue classes imposing confirmation on push or local MCP controls.
#[tokio::test]
#[serial_test::serial]
async fn push_and_local_mcp_controls_are_immediate() {
    let (dir, _guard, _state, service) = fixture();
    for (path, value) in [
        ("/services/push/enabled", json!(true)),
        (
            "/services/push/vapid_subject",
            json!("mailto:verification@example.test"),
        ),
        ("/mcp_port", json!(9982)),
        ("/mcp_server_enabled", json!(false)),
        ("/mcp_work_item_writes", json!(true)),
    ] {
        let result = service
            .patch(
                Target::Machine,
                "app".into(),
                revision(&service).await,
                vec![Edit::Set {
                    path: path.into(),
                    value: value.clone(),
                }],
            )
            .await;
        match result {
            Ok(_) => {}
            Err(error) => assert!(error.persisted, "{error}"),
        }
        let disk: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("config.json")).unwrap())
                .unwrap();
        assert_eq!(disk.pointer(path), Some(&value));
    }
}

// Catches: a model-reachable settings write widening where plugins may send
// HTTP (LAN included) without the owner approving it (#2659-b083).
#[tokio::test]
#[serial_test::serial]
async fn plugin_http_destinations_require_confirmation() {
    refuses(&[(
        "/plugin_http_destinations",
        json!(["http://192.168.1.20:3000/*"]),
    )])
    .await;
}

mod authority {
    use super::super::{
        policy::Caller,
        policy_baseline_tests::{fixture, revision},
        policy_tests::answer,
        request::*,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

    fn seed(state: &Arc<crate::AppState>) {
        let mut config = state.config.write();
        config.services.auth.session_token_duration_secs = 3600;
        config.services.auth.auth_rate_limit_max = 5;
        config.services.tls = serde_json::from_value(json!({"mode":"manual", "cert_path":"/verification/cert", "key_path":"/verification/key"})).unwrap();
        config.custom_launchers = serde_json::from_value(json!([{"id":"policy-launcher","name":"Policy","executable":"/verification/old", "args":[], "enabled":false, "platform":null}])).unwrap();
    }
    fn set(rev: String, path: &str, value: Value) -> Request {
        Request::Set {
            target: Target::Machine,
            store: "app".into(),
            path: path.into(),
            value,
            expected_revision: rev,
        }
    }

    // Catches: the owner getting an extra prompt for narrowing exposure, a no-op, or editing in their own window.
    #[tokio::test]
    #[serial_test::serial]
    async fn settings_safe_reduction_and_owner_ui_edit_apply_directly() {
        let agent = Caller::http("agent-connection".into());
        let owner = Caller::owner_ui();
        let cases = [
            (
                &agent,
                "/services/auth/session_token_duration_secs",
                json!(600),
            ),
            (&agent, "/services/auth/auth_rate_limit_max", json!(3)),
            (
                &agent,
                "/services/auth/auth_rate_limit_window_secs",
                json!(600),
            ),
            (&owner, "/services/auth/lan_auth_bypass", json!(true)),
            (
                &owner,
                "/services/auth/session_token_duration_secs",
                json!(86400),
            ),
            (&owner, "/services/tls/mode", json!("off")),
            (
                &owner,
                "/services/relay/url",
                json!("https://relay.example"),
            ),
            (
                &owner,
                "/custom_launchers/policy-launcher/enabled",
                json!(true),
            ),
            (
                &owner,
                "/custom_launchers/policy-launcher/executable",
                json!("/verification/new"),
            ),
        ];
        for (caller, path, value) in cases {
            let (dir, _guard, state, service) = fixture();
            seed(&state);
            let request = set(revision(&service).await, path, value.clone());
            match service.execute_as(request, caller, None).await {
                Ok(_) => {}
                Err(error) => assert!(error.persisted, "{path}: {error}"),
            }
            let disk: Value =
                serde_json::from_slice(&std::fs::read(dir.path().join("config.json")).expect(path))
                    .unwrap();
            let on_disk = path.replace("/policy-launcher/", "/0/");
            assert_eq!(disk.pointer(&on_disk), Some(&value), "{path}");
        }
        // A no-op edit is applied without a proposal and without a write.
        let (dir, _guard, state, service) = fixture();
        seed(&state);
        let request = set(
            revision(&service).await,
            "/services/auth/session_token_duration_secs",
            json!(3600),
        );
        service.execute_as(request, &agent, None).await.unwrap();
        assert!(!dir.path().join("config.json").exists());
    }

    // Catches: prompt-injected settings adding execution or network authority without a bound approval.
    #[tokio::test]
    #[serial_test::serial]
    async fn agent_authority_widening_requires_bound_confirmation() {
        let callers = [
            Caller::http("agent-connection".into()),
            Caller::mcp("agent-mcp".into(), true),
            Caller::native(),
        ];
        let cases = [
            ("/services/server/enabled", json!(true)),
            ("/services/server/ipv6_enabled", json!(true)),
            ("/services/server/port", json!(9981)),
            ("/services/auth/lan_auth_bypass", json!(true)),
            ("/services/auth/session_token_duration_secs", json!(86400)),
            ("/services/auth/auth_rate_limit_max", json!(100)),
            ("/services/tls/mode", json!("off")),
            ("/services/relay/enabled", json!(true)),
            ("/services/relay/url", json!("https://relay.example")),
            ("/custom_launchers/policy-launcher/enabled", json!(true)),
            (
                "/custom_launchers/policy-launcher/args",
                json!(["--review"]),
            ),
            (
                "/custom_launchers/policy-launcher/executable",
                json!("/verification/new"),
            ),
        ];
        for caller in &callers {
            for (path, value) in &cases {
                let (dir, _guard, state, service) = fixture();
                seed(&state);
                let error = service
                    .execute_as(
                        set(revision(&service).await, path, value.clone()),
                        caller,
                        None,
                    )
                    .await
                    .unwrap_err();
                assert_eq!(error.code, "confirmation_required", "{path}");
                assert!(error.proposal_id.is_some(), "{path}");
                assert!(!dir.path().join("config.json").exists(), "{path}");
            }
        }
        // The grant is bound to the asking connection and to the exact edit.
        let (dir, _guard, state, service) = fixture();
        seed(&state);
        let agent = &callers[0];
        let request = set(
            revision(&service).await,
            "/services/auth/lan_auth_bypass",
            json!(true),
        );
        let id = service
            .execute_as(request.clone(), agent, None)
            .await
            .unwrap_err()
            .proposal_id
            .unwrap();
        assert_eq!(
            answer(state.clone(), agent.clone(), id.clone(), true).await["confirmed"],
            json!(true)
        );
        let other = Caller::http("other-connection".into());
        assert!(
            service
                .execute_as(request.clone(), &other, Some(&id))
                .await
                .is_err()
        );
        assert!(!dir.path().join("config.json").exists());
    }
}

// Catches: an agent setting the auth rate limit to 0, which disables the brute-force
// bound (`rate_max > 0` in admit_auth_attempt), passing as a "lowered" number.
#[tokio::test]
#[serial_test::serial]
async fn disabling_the_auth_rate_limit_requires_confirmation() {
    refuses(&[("/services/auth/auth_rate_limit_max", json!(0))]).await;
}

// Catches: an agent setting the session lifetime to 0, which drops the cookie Max-Age
// (session_cookie_value) and removes the idle bound, passing as a "lowered" number.
#[tokio::test]
#[serial_test::serial]
async fn unbounded_session_lifetime_requires_confirmation() {
    refuses(&[("/services/auth/session_token_duration_secs", json!(0))]).await;
}
