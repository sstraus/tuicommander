use super::*;

fn submit(store: &SecretStore, kind: FieldKind, value: &str) {
    let form = store
        .open(
            Form::request(
                vec![Field {
                    name: "SYNTHETIC_FIELD".into(),
                    kind,
                    display: None,
                }],
                "synthetic test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .submit(
            &form.nonce,
            Submission::stored(BTreeMap::from([("SYNTHETIC_FIELD".into(), value.into())])),
        )
        .unwrap();
}

// Catches: a username submitted through the form masks unrelated terminal output.
#[test]
fn username_form_does_not_mask_existing_or_new_terminal_output() {
    let store = SecretStore::default();
    let prompt = "/home/synthetic-account/project $ ";
    assert_eq!(store.mask(prompt), prompt);
    submit(&store, FieldKind::Username, "synthetic-account");
    assert_eq!(store.mask(prompt), prompt);
    assert_eq!(
        &*store.mask_preserving_offsets(prompt.as_bytes()),
        prompt.as_bytes()
    );
    for session in ["existing-synthetic-session", "new-synthetic-session"] {
        let mut stream = StreamMask::default();
        assert_eq!(
            stream
                .push(prompt, store.pty_needles(session))
                .as_deref()
                .unwrap_or(prompt),
            prompt
        );
    }
    assert_eq!(
        store.environment(&["SYNTHETIC_FIELD".into()]).unwrap()["SYNTHETIC_FIELD"].as_str(),
        "synthetic-account"
    );
}

// Catches: replacing a password with a username leaves a stale output mask,
// or replacing a username with a password disables protection.
#[test]
fn replacement_and_removal_update_form_masking() {
    let store = SecretStore::default();
    for kind in [FieldKind::Password, FieldKind::Otp] {
        submit(&store, kind, "synthetic-sensitive-value");
        assert_eq!(store.mask("synthetic-sensitive-value"), "[REDACTED]");
        submit(&store, FieldKind::Username, "synthetic-sensitive-value");
        assert_eq!(
            store.mask("synthetic-sensitive-value"),
            "synthetic-sensitive-value"
        );
    }
    submit(&store, FieldKind::Password, "synthetic-sensitive-value");
    store.remove(&["SYNTHETIC_FIELD".into()]);
    assert_eq!(
        store.mask("synthetic-sensitive-value"),
        "synthetic-sensitive-value"
    );
    assert_eq!(
        store.pty_needles("existing-synthetic-session"),
        Vec::<Zeroizing<String>>::new()
    );
}

// Catches: child capture re-registers every injected environment value as a secret.
#[tokio::test]
async fn username_child_environment_remains_injected_without_output_masking() {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let argv = vec![
        std::fs::canonicalize(std::env::current_exe().unwrap())
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        "--exact".into(),
        "secrets::tests::secret_child_fixture".into(),
        "--ignored".into(),
        "--nocapture".into(),
    ];
    let cwd = std::fs::canonicalize(crate::test_support::test_temp_root()).unwrap();
    let form = state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "TUIC_SECRET_FIXTURE".into(),
                    kind: FieldKind::Username,
                    display: None,
                }],
                "synthetic child".into(),
            )
            .unwrap(),
        )
        .unwrap();
    state
        .secrets
        .submit(
            &form.nonce,
            Submission::stored(BTreeMap::from([(
                "TUIC_SECRET_FIXTURE".into(),
                "synthetic-account".into(),
            )])),
        )
        .unwrap();
    let mut approval = Form::request(
        vec![Field {
            name: "TUIC_SECRET_FIXTURE".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "synthetic approval".into(),
    )
    .unwrap();
    approval.argv = Some(argv.clone());
    approval.cwd = Some(cwd.to_string_lossy().into_owned());
    let approval = state.secrets.open(approval).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = Some(argv.clone());
    state.secrets.submit(&approval.nonce, submission).unwrap();
    let result = run::handle_secret(
        &state,
        &serde_json::json!({
            "action": "run", "names": ["TUIC_SECRET_FIXTURE"], "argv": argv, "cwd": cwd,
        }),
    )
    .await;
    assert_eq!(result["exit_code"], 0);
    assert!(
        result["stdout"]
            .as_str()
            .unwrap()
            .contains("child-saw-environment synthetic-account")
    );
}

// Catches: real PTY output paths still masking usernames after form submission.
#[cfg(unix)]
#[tokio::test]
async fn username_form_preserves_real_shell_prompts_in_existing_and_new_terminals() {
    use futures_util::StreamExt;
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = crate::mcp_http::build_router(state.clone(), false, true);
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    struct Cleanup {
        state: std::sync::Arc<crate::AppState>,
        sessions: Vec<String>,
        server: tokio::task::JoinHandle<()>,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for id in &self.sessions {
                if let Some(entry) = self.state.session_maps.sessions.get(id) {
                    let _ = entry.lock()._child.kill();
                }
                crate::pty::cleanup_session(id, &self.state);
            }
            self.server.abort();
        }
    }
    let mut cleanup = Cleanup {
        state: state.clone(),
        sessions: Vec::new(),
        server,
    };
    let client = reqwest::Client::new();
    let base = format!("http://{address}");
    for index in 0..2 {
        if index == 1 {
            let form = state
                .secrets
                .open(
                    Form::request(
                        vec![Field {
                            name: "SYNTHETIC_FIELD".into(),
                            kind: FieldKind::Username,
                            display: None,
                        }],
                        "synthetic PTY form".into(),
                    )
                    .unwrap(),
                )
                .unwrap();
            client
                .post(format!("{base}/secrets/forms/submit"))
                .json(&serde_json::json!({
                    "nonce": form.nonce, "status": "stored",
                    "values": {"SYNTHETIC_FIELD": "synthetic-account"}
                }))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
        }
        let response: serde_json::Value = client
            .post(format!("{base}/sessions/agent"))
            .json(&serde_json::json!({
                "agent_type": "shell", "binary_path": "/bin/sh", "args": ["-i"],
                "env": {"PS1": "/home/synthetic-account/project $ "}, "prompt": "true"
            }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        cleanup.sessions.push(
            response["session_id"]
                .as_str()
                .expect("shell spawned")
                .into(),
        );
    }
    for id in &cleanup.sessions {
        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("ws://{address}/sessions/{id}/stream"))
                .await
                .unwrap();
        // Both shells emit a fresh real PS1 after the username has been stored.
        client
            .post(format!("{base}/sessions/{id}/write"))
            .json(&serde_json::json!({"data": "PS1='/home/synthetic-account/project $ '\ntrue\n"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let mut wire = String::new();
        while !wire.contains("\r\n/home/synthetic-account/project $ ") {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(30), ws.next())
                .await
                .unwrap_or_else(|_| panic!("shell prompt missing; received {wire:?}"))
                .expect("shell stream remains open")
                .unwrap();
            if let tokio_tungstenite::tungstenite::Message::Text(text) = frame {
                let json: serde_json::Value = serde_json::from_str(&text).unwrap();
                if let Some(data) = json["data"].as_str() {
                    wire.push_str(data);
                }
            }
        }
        assert!(
            wire.contains("/home/synthetic-account/project $ "),
            "{wire:?}"
        );
    }
}
