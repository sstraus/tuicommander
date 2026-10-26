use super::*;
use crate::telegram::{runtime::Runtime, tool::Input};

const OTHER: &str = "33333333-3333-4333-8333-333333333333";

async fn inbox(runtime: &Runtime, sid: &str) -> Value {
    crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"inbox","since":0}),
        Some(sid),
        None,
    )
    .await
}

async fn assert_no_agent_reply(runtime: &mut Runtime, server: &FakeServer) {
    let value = inbound::text_update(7, 1111111, "after exit");
    let parsed = crate::telegram::mail::Update::parse(
        &value,
        &runtime.outbound.paths.allowlist().unwrap(),
        "test-bot",
        "",
    )
    .unwrap();
    let result = runtime.deliver(parsed.mail.unwrap()).await.unwrap();
    assert_eq!(result["accepted"], false);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].0.ends_with("sendMessage"));
    assert_eq!(
        requests[1].1["text"],
        "No agent is listening. Ask an agent in TUICommander to register for Telegram."
    );
    assert_eq!(requests[1].1["chat_id"], 1111111);
}

// Catches: registration is still restricted to a configured peer, replacement
// sends repeated notices, or old buttons/requests migrate to the new agent.
#[tokio::test]
async fn second_register_replaces_first_and_notifies_it_once() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(
        StatusCode::OK,
        json!({"ok":true,"result":{"message_id":7}}),
    )])
    .await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"register","tuic_session":OTHER,"name":"second"}),
        Some("second-mcp"),
        None,
    )
    .await;
    assert!(registered.get("error").is_none());
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Send {
                text: "Old choice".into(),
                buttons: vec![vec![crate::telegram::tool::Button {
                    label: "Yes".into(),
                    data: "old".into(),
                }]],
            },
        )
        .await
        .unwrap();
    let old_handle =
        server.requests()[0].1["reply_markup"]["inline_keyboard"][0][0]["callback_data"].clone();
    runtime
        .track(
            PendingMail {
                id: "old".into(),
                recipient: PEER.into(),
                content: json!({"chat_id":"1111111"}).to_string(),
            },
            super::streaming::Recipient::capture(&runtime.state, PEER),
        )
        .unwrap();
    runtime
        .tool(OTHER, "second-mcp", Input::Register {})
        .await
        .unwrap();
    runtime
        .tool(OTHER, "second-mcp", Input::Register {})
        .await
        .unwrap();
    runtime.update(json!({"callback_query":{"id":"old-query","from":{"id":1111111},
        "message":{"message_id":7,"date":1,"chat":{"id":1111111,"type":"private"}},"data":old_handle}})).await.unwrap();
    let mail = inbox(&runtime, "target-mcp").await;
    assert_eq!(mail["count"], 1, "{mail}");
    assert!(
        mail["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("replaced")
    );
    assert!(runtime.pending.is_empty());
    assert_eq!(
        runtime
            .tool(PEER, "target-mcp", Input::Unregister {})
            .await
            .unwrap_err(),
        Error::NotRegistered
    );
    assert_eq!(inbox(&runtime, "second-mcp").await["count"], 0);
    assert_eq!(server.requests().len(), 1);
}

// Catches: unregistered request actions succeed, unregister is a no-op, or a
// daemon restart reloads the registration from config.
#[tokio::test]
async fn unregistered_caller_cannot_answer_requests_and_restart_has_no_registration() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister {})
        .await
        .unwrap();
    for input in [
        Input::Begin {
            request_id: "r".into(),
        },
        Input::Activity {
            request_id: "r".into(),
            text: "working".into(),
        },
        Input::Finish {
            request_id: "r".into(),
            text: "done".into(),
        },
    ] {
        let error = runtime.tool(PEER, "target-mcp", input).await.unwrap_err();
        assert_eq!(error, Error::NotRegistered);
        assert!(error.to_string().contains("register"));
    }
    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    let fresh = Runtime::new(
        runtime.state.clone(),
        Config::load(&paths).unwrap().unwrap(),
        paths,
        "adapter-mcp".into(),
    )
    .unwrap();
    assert!(fresh.registered_peer().is_none());
    assert_eq!(
        server.requests(),
        [] as [(std::string::String, serde_json::Value); 0]
    );
}

// Catches: a registered agent retains phone authority after its MCP transport
// ends, after PTY close, or after agent exit while its shell remains alive.
#[cfg(unix)]
#[tokio::test]
async fn exited_agent_mcp_session_and_closed_pty_unregister() {
    for end in ["mcp", "pty"] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![(
            StatusCode::OK,
            json!({"ok":true,"result":{"message_id":9}}),
        )])
        .await;
        let mut runtime = outbound_tests::runtime(paths, server.address).await;
        crate::state::tests_support::insert_dummy_session(&runtime.state, PEER);
        crate::test_support::agent_session(&runtime.state, PEER, crate::pty::SHELL_IDLE);
        runtime
            .tool(PEER, "target-mcp", Input::Register {})
            .await
            .unwrap();
        let mut removed = None;
        match end {
            "mcp" => {
                runtime.state.mcp.to_session.remove("target-mcp");
            }
            "pty" => {
                removed = runtime.state.session_maps.sessions.remove(PEER);
            }
            _ => unreachable!(),
        }
        runtime.tick().await;
        assert!(runtime.registered_peer().is_none(), "{end}");
        assert_eq!(
            runtime
                .tool(
                    PEER,
                    "target-mcp",
                    Input::Send {
                        text: "late".into(),
                        buttons: vec![],
                    }
                )
                .await
                .unwrap_err(),
            Error::NotRegistered
        );
        if let Some((_, session)) =
            removed.or_else(|| runtime.state.session_maps.sessions.remove(PEER))
        {
            let _ = session.lock()._child.kill();
        }
        assert_no_agent_reply(&mut runtime, &server).await;
    }
}

// Catches: a carried configured agent keeps its registration after the native
// foreground detector observes that the agent has returned to its live shell.
#[cfg(unix)]
#[tokio::test]
async fn foreground_agent_exit_unregisters_even_when_shell_stays() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(
        StatusCode::OK,
        json!({"ok":true,"result":{"message_id":9}}),
    )])
    .await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    let state = runtime.state.clone();
    let agent =
        crate::test_support::ForegroundIdentityProbe::shell_parent(state.clone(), PEER, "claude");
    state
        .session_maps
        .session_states
        .get_mut(PEER)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(
        crate::pty::refresh_session_agent(&state, PEER).as_deref(),
        Some("claude")
    );
    runtime
        .tool(PEER, "target-mcp", Input::Register {})
        .await
        .unwrap();
    let seen = state.session_maps.session_states.get(PEER).unwrap().clone();
    drop(agent);
    let _shell =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), PEER, "bash");
    state.session_maps.session_states.insert(PEER.into(), seen);
    assert_eq!(crate::pty::refresh_session_agent(&state, PEER), None);
    assert!(state.session_maps.sessions.contains_key(PEER));
    runtime.tick().await;
    assert_eq!(
        runtime
            .tool(
                PEER,
                "target-mcp",
                Input::Send {
                    text: "late".into(),
                    buttons: vec![],
                }
            )
            .await
            .unwrap_err(),
        Error::NotRegistered
    );
    assert_no_agent_reply(&mut runtime, &server).await;
}

// Catches: no-agent inbound is retained or silently lost, stranger text gets a
// reply, or a drop prevents committing the polling cursor.
#[tokio::test]
async fn inbound_without_registration_replies_and_drops_but_strangers_stay_silent() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    let server = FakeServer::start(vec![
        inbound::updates(vec![
            inbound::text_update(7, 1111111, "allowed"),
            inbound::text_update(8, 3333333, "stranger"),
        ]),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":9}})),
    ])
    .await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister {})
        .await
        .unwrap();
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let port = crate::telegram::native::NativeMail::new(commands);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller =
        crate::telegram::inbound::Inbound::with_loopback(paths.clone(), server.address, port)
            .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(0)));
    assert_eq!(
        std::fs::read_to_string(paths.file("next_offset"))
            .unwrap()
            .trim(),
        "9"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[2].0.ends_with("sendMessage"));
    assert_eq!(requests[2].1["chat_id"], 1111111);
    assert_eq!(
        requests[2].1["text"],
        "No agent is listening. Ask an agent in TUICommander to register for Telegram."
    );
    assert!(
        state
            .agent_inbox
            .iter()
            .all(|entry| entry.value().is_empty())
    );
    let recent = serde_json::to_value(state.telegram_recent.lock().snapshot()).unwrap();
    assert_eq!(recent[0]["outcome"], "rejected: not authorized");
    assert_eq!(recent[1]["direction"], "out");
    assert_eq!(recent[2]["outcome"], "dropped: no agent");

    worker.abort();
}

// Catches: inbound parsing still addresses the removed configured UUID rather
// than letting the serialized runtime resolve its current registered agent.
#[tokio::test]
async fn inbound_registered_agent_receives_native_mail_without_phone_error() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    let server = FakeServer::start(vec![inbound::updates(vec![inbound::text_update(
        7,
        1111111,
        "exact inbound",
    )])])
    .await;
    let runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let port = crate::telegram::native::NativeMail::new(commands);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller =
        crate::telegram::inbound::Inbound::with_loopback(paths.clone(), server.address, port)
            .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(1)));
    let mail = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    assert_eq!(mail["count"], 1, "{mail}");
    let envelope: Value =
        serde_json::from_str(mail["messages"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["text"], "exact inbound");
    assert_eq!(envelope["request_id"], "tg:test-bot:7");
    assert_eq!(server.requests().len(), 3);
    worker.abort();
}

// Recorded public-update replay through the poller, native command channel and outbound fake.
async fn pairing_reply_1955(
    text: &str,
    expiry: u64,
    allowed: bool,
) -> (Vec<(String, Value)>, bool) {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    write_private(
        &paths.file("pairing.json"),
        &json!({"code":"A1B2C3","expires_at":expiry}).to_string(),
    );
    let chat = if allowed { 1111111 } else { 3333333 };
    let server = FakeServer::start(vec![
        inbound::updates(vec![inbound::text_update(7, chat, text)]),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":9}})),
    ])
    .await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister {})
        .await
        .unwrap();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller = crate::telegram::inbound::Inbound::with_loopback(
        paths.clone(),
        server.address,
        crate::telegram::native::NativeMail::new(commands),
    )
    .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(0)));
    let linked = paths.allowlist_entries().unwrap().contains(&chat);
    worker.abort();
    (server.requests(), linked)
}

fn assert_pairing_reply_1955(requests: &[(String, Value)], chat: i64, text: &str) {
    assert_eq!(requests.len(), 2, "poll must send exactly one reply");
    assert!(requests[1].0.ends_with("sendMessage"));
    assert_eq!(requests[1].1, json!({"chat_id":chat,"text":text}));
    assert!(!requests[1].1.to_string().contains("A1B2C3"));
    assert!(!requests[1].1.to_string().contains("fake-one"));
}

// Catches: authorization succeeds but the phone receives no confirmation.
#[tokio::test]
async fn pairing_confirms_link_1955() {
    let (requests, linked) = pairing_reply_1955("A1B2C3", u64::MAX, false).await;
    assert!(linked);
    assert_pairing_reply_1955(&requests, 3333333, "Chat linked.");
}

// Catches: bare /start is swallowed rather than explaining the link code.
#[tokio::test]
async fn pairing_start_instructs_1955() {
    let (requests, linked) = pairing_reply_1955("/start", u64::MAX, false).await;
    assert!(!linked);
    assert_pairing_reply_1955(
        &requests,
        3333333,
        "Use Link chat in TUICommander Settings, then send the code here.",
    );
}

// Catches: wrong, expired or consumed codes fail silently or authorize strangers.
#[tokio::test]
async fn pairing_bad_code_explains_1955() {
    for (text, expiry) in [("FFFFFF", u64::MAX), ("A1B2C3", 1)] {
        let (requests, linked) = pairing_reply_1955(text, expiry, false).await;
        assert!(!linked);
        assert_pairing_reply_1955(
            &requests,
            3333333,
            "Invalid or expired code. Use Link chat in TUICommander Settings for a new code.",
        );
    }
}

// Catches: arbitrary unauthorized text becomes a reply oracle.
#[tokio::test]
async fn pairing_random_strangers_stay_silent_1955() {
    for text in ["hello", "not-a-code", "ABCDE", "/start other", "GHIJKL"] {
        let (requests, linked) = pairing_reply_1955(text, u64::MAX, false).await;
        assert!(!linked);
        assert_eq!(requests.len(), 1, "{text}");
    }
}

// Catches: setup replies require a nonempty allowlist, making first-time setup silent.
#[tokio::test]
async fn pairing_start_with_empty_allowlist_1955() {
    let (_dir, paths) = setup();
    write_private(&paths.file("allowed_chat_ids"), "");
    write_private(&paths.file("next_offset"), "7\n");
    let server = FakeServer::start(vec![
        inbound::updates(vec![inbound::text_update(7, 3333333, "/start")]),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":9}})),
    ])
    .await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister {})
        .await
        .unwrap();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller = crate::telegram::inbound::Inbound::with_loopback(
        paths,
        server.address,
        crate::telegram::native::NativeMail::new(commands),
    )
    .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(0)));
    worker.abort();
    assert_pairing_reply_1955(
        &server.requests(),
        3333333,
        "Use Link chat in TUICommander Settings, then send the code here.",
    );
}

// Catches: the Settings log is not fed by the actual poll/mail/outbound boundaries,
// or stores pairing/stranger text before redaction.
#[tokio::test]
async fn recent_poll_and_outbound_events_1956() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    write_private(
        &paths.file("pairing.json"),
        &json!({"code":"A1B2C3","expires_at":u64::MAX}).to_string(),
    );
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":8}})),
        inbound::updates(vec![
            inbound::text_update(7, 3333333, "A1B2C3"),
            inbound::text_update(8, 3333333, "Hello from linked chat"),
            inbound::text_update(9, 4444444, "private stranger text"),
        ]),
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":9}})),
    ])
    .await;
    let mut runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Send {
                text: "Outbound probe".into(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller = crate::telegram::inbound::Inbound::with_loopback(
        paths,
        server.address,
        crate::telegram::native::NativeMail::new(commands),
    )
    .unwrap();
    assert!(matches!(poller.poll().await.unwrap(), Poll::Accepted(1)));
    let value = serde_json::to_value(state.telegram_recent.lock().snapshot()).unwrap();
    let value = Value::Array(
        value
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] != "status")
            .cloned()
            .collect(),
    );
    worker.abort();
    assert_eq!(value.as_array().unwrap().len(), 5);
    assert_eq!(value[0]["outcome"], "rejected: not authorized");
    assert_eq!(value[0]["preview"], "[redacted]");
    assert_eq!(value[1]["kind"], "message");
    assert_eq!(value[1]["outcome"], "delivered");
    assert_eq!(value[1]["preview"], "Hello from linked chat");
    assert_eq!(value[2]["direction"], "out");
    assert_eq!(value[2]["preview"], "Chat linked.");
    assert_eq!(value[3]["kind"], "pairing");
    assert_eq!(value[3]["preview"], "[redacted]");
    assert_eq!(value[4]["preview"], "Outbound probe");
    let serialized = value.to_string();
    assert!(!serialized.contains("A1B2C3"));
    assert!(!serialized.contains("private stranger text"));
    assert!(!serialized.contains("fake-one"));
    assert!(
        value
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["time"].as_u64().unwrap() > 0)
    );
}

// Catches: rejected outbound operations have no diagnostic or retain raw secrets.
#[tokio::test]
async fn recent_outbound_failure_is_safe_1956() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![(
        StatusCode::FORBIDDEN,
        json!({"ok":false,"description":"111:fake-one"}),
    )])
    .await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    assert_eq!(
        runtime
            .tool(
                PEER,
                "target-mcp",
                Input::Send {
                    text: "token 111:fake-one and A1B2C3".into(),
                    buttons: vec![]
                }
            )
            .await,
        Err(Error::Rejected(403))
    );
    let value = serde_json::to_value(runtime.state.telegram_recent.lock().snapshot()).unwrap();
    assert_eq!(value[0]["outcome"], "failed: telegram_request_rejected");
    assert_eq!(value[0]["preview"], "token [redacted] and [redacted]");
}

// Catches: button handling is invisible in the log or retains opaque callback data.
#[tokio::test]
async fn recent_button_events_are_redacted_1956() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
        (StatusCode::OK, json!({"ok":true,"result":true})),
    ])
    .await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Send {
                text: "Choose".into(),
                buttons: vec![vec![crate::telegram::tool::Button {
                    label: "Yes".into(),
                    data: "private callback data".into(),
                }]],
            },
        )
        .await
        .unwrap();
    let handle =
        server.requests()[0].1["reply_markup"]["inline_keyboard"][0][0]["callback_data"].clone();
    runtime.update(json!({"callback_query":{"id":"query","from":{"id":1111111},"message":{"message_id":7,"date":1,"chat":{"id":1111111,"type":"private"}},"data":handle}})).await.unwrap();
    let value = serde_json::to_value(runtime.state.telegram_recent.lock().snapshot()).unwrap();
    assert_eq!(value[0]["direction"], "in");
    assert_eq!(value[0]["kind"], "button");
    assert_eq!(value[0]["outcome"], "delivered");
    assert_eq!(value[0]["preview"], "[redacted]");
    assert_eq!(value[1]["direction"], "out");
    assert_eq!(value[1]["kind"], "button");
    assert_eq!(value[1]["outcome"], "sent");
    assert!(!value.to_string().contains("private callback data"));
}
// Catches: a transient link-confirmation failure replays the consumed pairing
// credential as ordinary agent mail after the chat has become authorized.
#[tokio::test]
async fn failed_link_confirmation_never_replays_pairing_code_into_agent_inbox() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    write_private(
        &paths.file("pairing.json"),
        &json!({"code":"A1B2C3","expires_at":super::super::settings::now_ms() + 60_000})
            .to_string(),
    );
    let update = inbound::text_update(7, 1111111, "A1B2C3");
    let server = FakeServer::start(vec![
        inbound::updates(vec![update.clone()]),
        (StatusCode::BAD_GATEWAY, json!({})),
        inbound::updates(vec![update]),
    ])
    .await;
    let runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller = crate::telegram::inbound::Inbound::with_loopback(
        paths.clone(),
        server.address,
        crate::telegram::native::NativeMail::new(commands.clone()),
    )
    .unwrap();
    let _ = poller.poll().await;
    // Restart the polling owner as after a connection interruption; the server
    // returns the unacknowledged update again, as getUpdates specifies.
    drop(poller);
    let mut poller = crate::telegram::inbound::Inbound::with_loopback(
        paths,
        server.address,
        crate::telegram::native::NativeMail::new(commands),
    )
    .unwrap();
    let _ = poller.poll().await;
    let mail = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    worker.abort();
    assert_eq!(
        mail["count"], 0,
        "pairing credential became agent mail: {mail}"
    );
}

// Catches: committing a pairing cursor skips earlier mail, or failed feedback
// prevents later mail in the same sorted batch from reaching the native inbox.
#[tokio::test]
async fn failed_link_confirmation_preserves_same_batch_mail_order() {
    let (_dir, paths) = setup();
    write_private(&paths.file("next_offset"), "7\n");
    write_private(
        &paths.file("pairing.json"),
        &json!({"code":"A1B2C3","expires_at":super::super::settings::now_ms() + 60_000})
            .to_string(),
    );
    let before = inbound::text_update(7, 1111111, "before link");
    let pairing = inbound::text_update(8, 3333333, "A1B2C3");
    let after = inbound::text_update(9, 3333333, "after link");
    let server = FakeServer::start(vec![inbound::updates(vec![after, pairing, before])]).await;
    // The failure belongs to link confirmation, not to the receipt draft that
    // now precedes it. Keep the fault tied to its HTTP method rather than order.
    server
        .state
        .lock()
        .unwrap()
        .send_message_responses
        .push_back((StatusCode::BAD_GATEWAY, json!({})));
    let runtime = outbound_tests::runtime(paths.clone(), server.address).await;
    let state = runtime.state.clone();
    let (commands, receive) = tokio::sync::mpsc::channel(10);
    let worker = tokio::spawn(runtime.run(receive));
    let mut poller = crate::telegram::inbound::Inbound::with_loopback(
        paths.clone(),
        server.address,
        crate::telegram::native::NativeMail::new(commands),
    )
    .unwrap();
    let result = poller.poll().await;
    let mail = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    worker.abort();
    assert!(matches!(
        result,
        Ok(crate::telegram::inbound::Poll::Accepted(2))
    ));
    assert_eq!(super::super::offset::read(&paths), Some(10));
    assert_eq!(mail["count"], 2, "{mail}");
    let texts: Vec<String> = mail["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let content: Value = serde_json::from_str(entry["content"].as_str().unwrap()).unwrap();
            content["text"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(texts, ["before link", "after link"]);
    let events = serde_json::to_value(state.telegram_recent.lock().snapshot()).unwrap();
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["direction"] == "out"
                && event["outcome"] == "failed: telegram_transport_unavailable"),
        "{events}"
    );
}
