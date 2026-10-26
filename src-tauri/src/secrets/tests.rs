use super::*;
use std::collections::BTreeMap;

#[test]
fn guessed_or_replayed_nonce_cannot_replace_a_secret() {
    let store = SecretStore::default();
    let form = Form::request(
        vec![Field {
            name: "TOKEN".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "test".into(),
    )
    .unwrap();
    let opened = store.open(form).unwrap();
    assert!(
        store
            .submit(
                "guessed",
                Submission::stored(BTreeMap::from([("TOKEN".into(), "private".into())]))
            )
            .is_err()
    );
    assert!(
        store
            .submit(
                &opened.nonce,
                Submission::stored(BTreeMap::from([("EXTRA".into(), "private".into())]))
            )
            .is_err()
    );
    let result = store
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([("TOKEN".into(), "private".into())])),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        r#"{"names":["TOKEN"],"status":"stored"}"#
    );
    assert!(
        store
            .submit(
                &opened.nonce,
                Submission::stored(BTreeMap::from([("TOKEN".into(), "replacement".into())]))
            )
            .is_err()
    );
    assert_eq!(store.mask("private replacement"), "[REDACTED] replacement");
}

#[test]
fn declined_form_and_removed_secret_leave_no_retrievable_value() {
    let store = SecretStore::default();
    let opened = store
        .open(
            Form::request(
                vec![Field {
                    name: "OTP".into(),
                    kind: FieldKind::Otp,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let result = store.submit(&opened.nonce, Submission::declined()).unwrap();
    assert_eq!(result.status, "declined");
    assert!(store.environment(&["OTP".into()]).is_err());
    assert!(SecretStore::default().environment(&["OTP".into()]).is_err());
}

#[test]
fn encoded_and_line_wrapped_secret_cannot_escape_output_masking() {
    let store = SecretStore::default();
    let opened = store
        .open(
            Form::request(
                vec![Field {
                    name: "TOKEN".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([("TOKEN".into(), "ab +/".into())])),
        )
        .unwrap();
    for text in [
        "ab +/",
        "ab \r\n+/",
        "YWIgKy8=",
        "YWIg\nKy8=",
        "6162202b2f",
        "6162202B2F",
        "ab%20%2B%2F",
        "ab%20%2b%2f",
        "ab+%2B%2F",
        "%61%62%20%2b%2F",
        "6162202b2F",
        "ab \x1b[31m+/",
    ] {
        assert_eq!(
            store.mask(text),
            "[REDACTED]",
            "unmasked representation: {text:?}"
        );
    }
}

#[test]
fn approval_cannot_override_shell_or_environment_dumper_rejection() {
    for argv in [
        vec!["sh", "-c", "true"],
        vec!["/bin/bash", "-lc", "true"],
        vec!["zsh", "--command", "true"],
        vec!["env"],
        vec!["printenv"],
        vec!["node", "-e", "0"],
        vec!["python3", "-cpass"],
        vec!["perl", "-we", "0"],
        vec!["node", "--eval=0"],
        vec!["renamed-interpreter", "-c", "pass"],
    ] {
        assert!(
            policy::validate_argv(&argv.into_iter().map(String::from).collect::<Vec<_>>()).is_err()
        );
    }
}

#[tokio::test]
async fn child_environment_and_pipe_capture_do_not_echo_secret_to_result() {
    let exe = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
    let argv = vec![
        exe.to_string_lossy().into_owned(),
        "--exact".into(),
        "secrets::tests::secret_child_fixture".into(),
        "--ignored".into(),
        "--nocapture".into(),
    ];
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let cwd = std::fs::canonicalize(crate::test_support::test_temp_root()).unwrap();
    let opened = state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "TUIC_SECRET_FIXTURE".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    state
        .secrets
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([(
                "TUIC_SECRET_FIXTURE".into(),
                "ab +/".into(),
            )])),
        )
        .unwrap();
    let mut approval = Form::request(
        vec![Field {
            name: "TUIC_SECRET_FIXTURE".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "Approve".into(),
    )
    .unwrap();
    approval.argv = Some(argv.clone());
    approval.cwd = Some(cwd.to_string_lossy().into_owned());
    let approval = state.secrets.open(approval).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = Some(argv.clone());
    state.secrets.submit(&approval.nonce, submission).unwrap();
    let result = run::handle_secret(&state, &serde_json::json!({"action": "run", "names": ["TUIC_SECRET_FIXTURE"], "argv": argv, "cwd": cwd})).await;
    assert!(state.session_maps.output_buffers.is_empty());
    assert!(state.log_buffer.lock().get_entries(0).is_empty());
    let stdout = result["stdout"].as_str().unwrap();
    let stderr = result["stderr"].as_str().unwrap();
    assert_eq!(result["exit_code"], 0);
    assert!(
        stdout.contains("child-saw-environment [REDACTED]"),
        "{result}"
    );
    assert!(!serde_json::to_string(&result).unwrap().contains("YWIgKy8="));
    assert!(stderr.contains("[REDACTED]"));
    assert!(std::env::var("TUIC_SECRET_FIXTURE").is_err());
}

#[test]
#[ignore = "Child-process fixture: run only by child_environment_and_pipe_capture_do_not_echo_secret_to_result"]
fn secret_child_fixture() {
    let value = std::env::var("TUIC_SECRET_FIXTURE").unwrap();
    println!("child-saw-environment {value}");
    println!("YWIg\nKy8=");
    eprintln!("6162202B2F");
}

#[tokio::test]
async fn http_submission_rejects_guessed_and_replayed_nonce_without_echoing_values() {
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use tower::ServiceExt;
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let form = state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "PASS".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let app = crate::mcp_http::build_router(state.clone(), false, true).layer(axum::Extension(
        ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))),
    ));
    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/secrets/forms/wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), 404);
    for (nonce, expected) in [
        ("wrong", 400),
        (form.nonce.as_str(), 200),
        (form.nonce.as_str(), 400),
    ] {
        let body = serde_json::json!({"nonce": nonce, "status": "stored", "values": {"PASS": "synthetic-entry"}, "template": null});
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/secrets/forms/submit")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-entry"));
    }
}

// Catches: direct upstream MCP calls bypassing the private-form inspection gate.
#[tokio::test]
async fn open_form_blocks_native_and_proxied_inspection_tools() {
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use tower::ServiceExt;
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    state.config.write().mcp_capabilities.denied.clear();
    state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "PASS".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    for (tool, args) in [
        (
            "ui",
            serde_json::json!({"action": "screenshot", "id": "any"}),
        ),
        (
            "debug",
            serde_json::json!({"action": "invoke_js", "script": "return document.body.innerHTML"}),
        ),
        ("maccontrol__screenshot", serde_json::json!({})),
    ] {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": tool, "arguments": args}
        });
        let mut request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let response = crate::mcp_http::build_router(state.clone(), false, true)
            .oneshot(request)
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["result"]["isError"], true, "{tool}: {result}");
        assert!(
            result["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("inspection is disabled"),
            "{tool}: {result}"
        );
    }
}

#[test]
fn saved_template_cannot_approve_new_names_or_working_directory() {
    let store = SecretStore::default();
    let mut approval = Form::request(
        vec![Field {
            name: "PASS".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "Approve".into(),
    )
    .unwrap();
    approval.argv = Some(vec!["/usr/bin/gh".into(), "api".into(), "user".into()]);
    approval.cwd = Some("/trusted".into());
    let approval = store.open(approval).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = approval.argv.clone();
    store.submit(&approval.nonce, submission).unwrap();
    let argv = vec!["/usr/bin/gh".into(), "api".into(), "user".into()];
    assert!(store.allowed(&argv, &["PASS".into()], "/trusted"));
    assert!(!store.allowed(&argv, &["OTHER".into()], "/trusted"));
    assert!(!store.allowed(&argv, &["PASS".into()], "/attacker"));
}

#[test]
fn utf8_split_by_a_wrap_is_masked_before_lossy_decoding() {
    let needles = mask::representations("café");
    assert_eq!(mask::mask_bytes(b"caf\xc3\r\n\xa9", &needles), "[REDACTED]");
}

// Catches: exact argv consent accidentally authorising changed arguments or
// interpreting a literal former placeholder as a wildcard.
#[test]
fn exact_template_does_not_authorise_changed_arguments() {
    let argv = vec!["gh".into(), "api".into(), "user".into()];
    let template = policy::Template::new(argv.clone()).unwrap();
    assert!(template.matches(&argv));
    for changed in [
        vec!["gh".into(), "api".into(), "repos/org/repo".into()],
        vec![
            "gh".into(),
            "api".into(),
            "https://evil.example/collect".into(),
        ],
        vec!["gh".into(), "auth".into(), "user".into()],
        vec!["gh".into(), "api".into(), "user".into(), "extra".into()],
    ] {
        assert!(!template.matches(&changed));
    }
    let literal = policy::Template::new(vec!["gh".into(), "api".into(), "{arg}".into()]).unwrap();
    assert!(!literal.matches(&argv));
}

// Catches: remove retaining a previously stored value in the child environment.
#[test]
fn removed_secret_is_missing_from_the_child_environment() {
    let store = SecretStore::default();
    let form = store
        .open(
            Form::request(
                vec![Field {
                    name: "PASS".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .submit(
            &form.nonce,
            Submission::stored(BTreeMap::from([("PASS".into(), "synthetic-entry".into())])),
        )
        .unwrap();
    assert!(store.environment(&["PASS".into()]).is_ok());
    store.remove(&["PASS".into()]);
    assert!(store.environment(&["PASS".into()]).is_err());
}

// Catches: encoded values leaking when their enclosing payload starts at a
// different base64 alignment, or unrelated encoded output being over-redacted.
#[test]
fn base64_payload_redacts_secret_at_every_alignment_without_hiding_unrelated_output() {
    use base64::Engine;
    let secret = "s3cr3t-Pass!w0rd";
    let needles = mask::representations(secret);
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        for prefix in ["a:", "ab:", "abc:"] {
            let encoded = engine.encode(format!("{prefix}{secret}:suffix"));
            let wrapped = format!("Basic {}\r\n{}", &encoded[..8], &encoded[8..]);
            let masked = mask::mask(&wrapped, &needles);
            assert!(masked.contains("[REDACTED]"), "{wrapped}: {masked}");
            assert!(!masked.contains(&encoded[8..]), "{wrapped}: {masked}");
        }
    }
    let unrelated = "Basic dXNlcjpub3QtaXQ=";
    assert_eq!(mask::mask(unrelated, &needles), unrelated);
}

// Catches: successful deletion reported as a lookup failure, and MCP teardown
// clearing backend-owned values before a replacement connection can remove them.
#[tokio::test]
async fn secret_remove_reports_completion_across_mcp_reconnects() {
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use tower::ServiceExt;
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    state.config.write().mcp_capabilities.denied.clear();
    let app = crate::mcp_http::build_router(state.clone(), false, true).layer(axum::Extension(
        ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345))),
    ));
    let mut results = Vec::new();
    for reconnect in [false, true] {
        let initialize = || {
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "jsonrpc": "2.0", "id": 1, "method": "initialize",
                        "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                            "clientInfo": {"name": "secret-scope-test", "version": "1"}}
                    })
                    .to_string(),
                ))
                .unwrap()
        };
        let response = app.clone().oneshot(initialize()).await.unwrap();
        assert_eq!(response.status(), 200);
        let original = response.headers()["mcp-session-id"]
            .to_str()
            .unwrap()
            .to_owned();
        // Existing form/HTTP seam: native bootstrap is deliberately unavailable
        // to the main window. Submission still uses the real validation/store.
        let form = state
            .secrets
            .open(
                Form::request(
                    ["TARGET", "KEEP"]
                        .into_iter()
                        .map(|name| Field {
                            name: name.into(),
                            kind: FieldKind::Password,
                            display: None,
                        })
                        .collect(),
                    "Scope regression".into(),
                )
                .unwrap(),
            )
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/secrets/forms/submit")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "nonce": form.nonce, "status": "stored",
                            "values": {"TARGET": "synthetic-target", "KEEP": "synthetic-keep"}
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            stored,
            serde_json::json!({"names": ["TARGET", "KEEP"], "status": "stored"})
        );
        let session = if reconnect {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri("/mcp")
                        .header("mcp-session-id", &original)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(response.status().is_success());
            let response = app.clone().oneshot(initialize()).await.unwrap();
            assert_eq!(response.status(), 200);
            let replacement = response.headers()["mcp-session-id"]
                .to_str()
                .unwrap()
                .to_owned();
            assert_ne!(original, replacement);
            replacement
        } else {
            original
        };
        assert!(
            state.secrets.environment(&["TARGET".into()]).is_ok(),
            "connection teardown lost stored value"
        );
        for _ in 0..2 {
            let response = app.clone().oneshot(Request::builder()
                .method("POST").uri("/mcp")
                .header("content-type", "application/json").header("mcp-session-id", &session)
                .body(Body::from(serde_json::json!({
                    "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                    "params": {"name": "secret", "arguments": {"action": "remove", "names": ["TARGET", "ABSENT"]}}
                }).to_string())).unwrap()).await.unwrap();
            assert_eq!(response.status(), 200);
            let bytes = axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap();
            let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_ne!(reply["result"]["isError"], true);
            results.push(
                serde_json::from_str::<serde_json::Value>(
                    reply["result"]["content"][0]["text"].as_str().unwrap(),
                )
                .unwrap(),
            );
            assert!(state.secrets.environment(&["TARGET".into()]).is_err());
            assert!(state.secrets.environment(&["KEEP".into()]).is_ok());
        }
    }
    // Check all lifecycle/state assertions before the wording regression.
    assert_eq!(
        results,
        vec![serde_json::json!({"names": ["TARGET", "ABSENT"], "status": "removed"}); 4]
    );
}
