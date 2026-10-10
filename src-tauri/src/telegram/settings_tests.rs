use super::*;

// Catches: shared status hides the polling process or reports stale/disabled connectivity.
#[test]
fn polling_owner_snapshot_distinguishes_this_process_remote_and_stale_status() {
    let (_dir, paths) = crate::telegram::tests::setup();
    let state = crate::state::tests_support::make_test_app_state();
    status(&paths, true, None, false);
    let current = serde_json::to_value(snapshot(&paths, &state).unwrap()).unwrap();
    assert_eq!(current["polling_owner"], "this_app");
    assert_eq!(current["connected"], true);
    let mut shared: Value =
        serde_json::from_str(&std::fs::read_to_string(paths.file("status.json")).unwrap()).unwrap();
    shared["owner_pid"] = json!(u32::MAX);
    shared["owner_kind"] = json!("tuic_remote");
    write(&paths, "status.json", &serde_json::to_vec(&shared).unwrap()).unwrap();
    let remote = serde_json::to_value(snapshot(&paths, &state).unwrap()).unwrap();
    assert_eq!(remote["polling_owner"], "tuic_remote");
    shared["updated_at"] = json!(1);
    write(&paths, "status.json", &serde_json::to_vec(&shared).unwrap()).unwrap();
    let stale = serde_json::to_value(snapshot(&paths, &state).unwrap()).unwrap();
    assert_eq!(stale["polling_owner"], Value::Null);
    assert_eq!(stale["connected"], false);
    status(&paths, true, None, false);
    save_config(
        &paths,
        &Config {
            enabled: false,
            bot_alias: "test-bot".into(),
        },
    )
    .unwrap();
    assert!(!snapshot(&paths, &state).unwrap().connected);
}
// Catches: code-like messages, expired credentials or replay authorize a stranger.
#[tokio::test]
async fn pairing_code_expired_or_wrong_authorizes_nothing() {
    use crate::telegram::{Inbound, Poll, offset, tests::FakeServer};
    use axum::http::StatusCode;
    for scenario in ["start", "wrong", "expired", "group", "valid", "replay"] {
        let (_dir, paths) = crate::telegram::tests::setup();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        change_at(
            &state,
            paths.clone(),
            Change::RemoveChat {
                chat_id: "1111111".into(),
            },
        )
        .await
        .unwrap();
        let pairing = change_at(&state, paths.clone(), Change::Pair)
            .await
            .unwrap();
        let code = pairing["code"].as_str().unwrap();
        if scenario == "expired" {
            write(
                &paths,
                "pairing.json",
                &serde_json::to_vec(&json!({"code":code,"expires_at":1})).unwrap(),
            )
            .unwrap();
        }
        let mut credential: Value =
            serde_json::from_str(include_str!("fixtures/public-text-update.json")).unwrap();
        credential["message"]["chat"]["type"] = json!(if scenario == "group" {
            "group"
        } else {
            "private"
        });
        credential["message"]["text"] = json!(match scenario {
            "start" => "/start",
            "wrong" => "wrong",
            _ => code,
        });
        let mut message = credential.clone();
        message["update_id"] = json!(10_001);
        message["message"]["text"] = json!("First ordinary message");
        let mut values = vec![credential.clone(), message];
        if scenario == "replay" {
            credential["update_id"] = json!(10_002);
            credential["message"]["chat"]["id"] = json!(333);
            values.push(credential);
        }
        offset::write(&paths, 10_000).unwrap();
        let server =
            FakeServer::start(vec![(StatusCode::OK, json!({"ok":true,"result":values}))]).await;
        let mut inbound = Inbound::loopback(paths.clone(), server.address).unwrap();
        let authorized = matches!(scenario, "valid" | "replay");
        let expected = usize::from(authorized);
        assert!(
            matches!(inbound.poll().await.unwrap(), Poll::Accepted(n) if n == expected),
            "{scenario}"
        );
        assert_eq!(
            paths.allowlist_entries().unwrap().contains(&1_111_111),
            authorized,
            "{scenario}"
        );
        assert!(!paths.allowlist_entries().unwrap().contains(&333));
        let pending = inbound.pending().unwrap();
        assert_eq!(pending.len(), expected, "{scenario}");
        for mail in pending {
            let content: Value = serde_json::from_str(&mail.content).unwrap();
            assert_eq!(content["text"], "First ordinary message");
            assert!(!mail.content.contains(code));
        }
    }
}
// Catches: reading Settings echoes the secret or private file content.
#[test]
fn token_never_returned_by_settings_api() {
    let (dir, paths) = crate::telegram::tests::setup();
    write(&paths, "bot.token", b"123:secret-settings-token").unwrap();
    let state = AppState::new(
        dir.path().into(),
        dir.path().join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(10),
        )),
    );
    let snapshot = serde_json::to_value(snapshot(&paths, &state).unwrap()).unwrap();
    assert_eq!(snapshot["token_set"], true);
    assert!(snapshot.get("token").is_none());
    assert!(!snapshot.to_string().contains("secret-settings-token"));
}
// Catches: setup follows a secret-file symlink or grants malformed IDs.
#[tokio::test]
async fn setup_rejects_unsafe_private_files_and_non_private_ids() {
    let (_dir, paths) = crate::telegram::tests::setup();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let original = std::fs::read(paths.file("allowed_chat_ids")).unwrap();
    for id in ["0", "-123", "+123", "00123", "abc", "9223372036854775808"] {
        assert_eq!(
            change_at(
                &state,
                paths.clone(),
                Change::AddChat { chat_id: id.into() }
            )
            .await,
            Err(Error::Config),
            "{id}"
        );
        assert_eq!(
            std::fs::read(paths.file("allowed_chat_ids")).unwrap(),
            original
        );
    }
    change_at(
        &state,
        paths.clone(),
        Change::AddChat {
            chat_id: "123".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        snapshot(&paths, &state)
            .unwrap()
            .chats
            .contains(&"123".into())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let target = paths.file("saved-allowlist");
        std::fs::rename(paths.file("allowed_chat_ids"), &target).unwrap();
        let before = std::fs::read(&target).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        symlink(&target, paths.file("allowed_chat_ids")).unwrap();
        assert_eq!(
            change_at(
                &state,
                paths.clone(),
                Change::AddChat {
                    chat_id: "222".into()
                }
            )
            .await,
            Err(Error::PrivateFile)
        );
        assert_eq!(std::fs::read(&target).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode(),
            mode
        );
        assert!(
            std::fs::symlink_metadata(paths.file("allowed_chat_ids"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

// Catches: removing the final chat leaves it authorized, or disabling requires a vanished agent.
#[tokio::test]
async fn manual_chat_revocation_and_disable_use_persisted_state() {
    let (dir, paths) = crate::telegram::tests::setup();
    let state = Arc::new(AppState::new(
        dir.path().into(),
        dir.path().join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(10),
        )),
    ));
    change_at(
        &state,
        paths.clone(),
        Change::AddChat {
            chat_id: "222".into(),
        },
    )
    .await
    .unwrap();
    assert!(paths.allowlist().unwrap().contains(&222));
    change_at(
        &state,
        paths.clone(),
        Change::RemoveChat {
            chat_id: "222".into(),
        },
    )
    .await
    .unwrap();
    assert!(!paths.allowlist().unwrap().contains(&222));
    change_at(&state, paths.clone(), Change::Configure { enabled: false })
        .await
        .unwrap();
    assert!(!config(&paths).unwrap().enabled);
    assert!(
        !std::fs::read_to_string(paths.file("config.json"))
            .unwrap()
            .contains("target_tuic_session")
    );
    assert!(Config::load(&paths).unwrap().is_none());
    change_at(&state, paths.clone(), Change::Configure { enabled: true })
        .await
        .unwrap();
    assert!(config(&paths).unwrap().enabled);
}

// Catches: desktop reports an old registration after adapter restart or expiry.
#[cfg(unix)]
#[tokio::test]
async fn registered_agent_snapshot_uses_shared_status_and_retires_stale_names() {
    use crate::telegram::{
        tests::{FakeServer, PEER, outbound_tests},
        tool::Input,
    };
    let (_dir, paths) = crate::telegram::tests::setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    let state = runtime.state.clone();
    let mut agent =
        crate::test_support::ForegroundIdentityProbe::shell_parent(state.clone(), PEER, "claude");
    assert_eq!(
        crate::pty::refresh_session_agent(&state, PEER).as_deref(),
        Some("claude")
    );
    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    assert_eq!(
        snapshot(&paths, &state)
            .unwrap()
            .registered_agent_name
            .as_deref(),
        Some("target-mcp")
    );
    runtime
        .tool(PEER, "target-mcp", Input::Unregister {})
        .await
        .unwrap();
    assert!(
        snapshot(&paths, &state)
            .unwrap()
            .registered_agent_name
            .is_none()
    );
    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    agent.return_to_root();
    assert_eq!(crate::pty::refresh_session_agent(&state, PEER), None);
    runtime.tick().await;
    assert!(
        snapshot(&paths, &state)
            .unwrap()
            .registered_agent_name
            .is_none()
    );
    write(&paths, "status.json", br#"{"connected":true,"registered_agent_name":"stale","last_error":null,"last_message_time":null,"updated_at":1}"#).unwrap();
    assert!(
        snapshot(&paths, &state)
            .unwrap()
            .registered_agent_name
            .is_none()
    );
}
