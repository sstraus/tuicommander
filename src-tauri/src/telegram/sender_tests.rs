use super::tests::{FakeServer, PEER, outbound_tests, setup};
use super::{mail::Update, tool::Input};
use serde_json::{Value, json};

const SENDER: &str = "33333333-3333-4333-8333-333333333333";

async fn add_sender(runtime: &super::runtime::Runtime) {
    let result = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"register","tuic_session":SENDER,"name":"coordinator"}),
        Some("sender-mcp"),
        None,
    )
    .await;
    assert!(result.get("error").is_none(), "{result}");
}

// Catches: outbound permission remains coupled to inbound ownership.
#[tokio::test]
async fn non_owner_send_preserves_owner_and_prefixes_server_identity() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    let result = runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Send {
                text: "[target-mcp] release ready".into(),
                buttons: vec![],
            },
        )
        .await;
    assert!(result.is_ok(), "non-owner send refused: {result:?}");
    assert_eq!(runtime.registered_peer(), Some(PEER));
    assert_eq!(
        server.requests()[0].1["text"],
        "[coordinator] [target-mcp] release ready"
    );
    assert_eq!(server.requests()[0].1["chat_id"], 1111111);
    // Complete the loopback boundary: reply to the bytes the fake Bot API received,
    // not a second independently chosen attribution string.
    let mut reply: Value =
        serde_json::from_str(include_str!("fixtures/reply-to-sender.json")).unwrap();
    reply["message"]["reply_to_message"]["text"] = server.requests()[0].1["text"].clone();
    reply["message"]["reply_to_message"]["message_id"] = result.unwrap()["message_ids"][0].clone();
    let mail = Update::parse(
        &reply,
        &runtime.outbound.paths.allowlist().unwrap(),
        "test-bot",
        "",
    )
    .unwrap()
    .mail
    .unwrap();
    runtime.deliver(mail).await.unwrap();
    for (sid, expected) in [("sender-mcp", 1), ("target-mcp", 0)] {
        let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &runtime.state,
            &json!({"action":"inbox","since":0}),
            Some(sid),
            None,
        )
        .await;
        assert_eq!(inbox["count"], expected, "{sid}: {inbox}");
    }
    assert_eq!(runtime.registered_peer(), Some(PEER));
}

// Catches: one sender can flood, a different sender is charged, or expiry never recovers.
#[tokio::test]
async fn sender_limit_is_per_peer_and_expires_without_registration() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    let send = || Input::Send {
        text: "notice".into(),
        buttons: vec![],
    };
    runtime.tool(SENDER, "sender-mcp", send()).await.unwrap();
    assert_eq!(
        runtime
            .tool(SENDER, "sender-mcp", send())
            .await
            .unwrap_err(),
        super::Error::SenderRateLimited
    );
    assert!(
        super::Error::SenderRateLimited
            .to_string()
            .contains("30-second")
    );
    let other = "55555555-5555-4555-8555-555555555555";
    let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"register","name":"other sender","tuic_session":other}),
        Some("other-mcp"),
        None,
    )
    .await;
    assert!(registered.get("error").is_none(), "{registered}");
    runtime.tool(other, "other-mcp", send()).await.unwrap();
    // Owner remains unrestricted and does not acquire a sender prefix.
    runtime.tool(PEER, "target-mcp", send()).await.unwrap();
    assert_eq!(server.requests()[2].1["text"], "notice");
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(30)).await;
    tokio::time::resume();
    runtime.tool(SENDER, "sender-mcp", send()).await.unwrap();
    assert_eq!(server.requests().len(), 4);
}

// Catches: non-owner button sends mutate owner routing or create callback handles.
#[tokio::test]
async fn non_owner_buttons_are_refused() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    assert_eq!(
        runtime
            .tool(
                SENDER,
                "sender-mcp",
                Input::Send {
                    text: "choice".into(),
                    buttons: vec![vec![super::tool::Button {
                        label: "yes".into(),
                        data: "yes".into()
                    }]],
                }
            )
            .await
            .unwrap_err(),
        super::Error::NonOwnerButtons
    );
    assert_eq!(server.requests(), [] as [(String, serde_json::Value); 0]);
}

// Catches: continuation messages lose attribution or Unicode splitting exceeds Telegram's bound.
#[tokio::test]
async fn every_non_owner_chunk_has_the_server_prefix() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    let text = "😀".repeat(2500);
    runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Send {
                text: text.clone(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let parts: Vec<_> = requests
        .iter()
        .map(|(_, body)| {
            let text = body["text"].as_str().unwrap();
            assert!(text.encode_utf16().count() <= 4096);
            text.strip_prefix("[coordinator] ")
                .expect("missing sender attribution")
        })
        .collect();
    assert_eq!(parts.concat(), text);
}

// Catches: stale or ambiguous names select an arbitrary peer instead of the owner.
#[tokio::test]
async fn unprefixed_unknown_dead_and_ambiguous_replies_fall_back_to_owner() {
    for scenario in ["new", "plain", "missing", "dead", "ambiguous"] {
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![]).await;
        let mut runtime = outbound_tests::runtime(paths, server.address).await;
        add_sender(&runtime).await;
        let mut update: Value =
            serde_json::from_str(include_str!("fixtures/reply-to-sender.json")).unwrap();
        match scenario {
            "new" => {
                update["message"]
                    .as_object_mut()
                    .unwrap()
                    .remove("reply_to_message");
            }
            "plain" => update["message"]["reply_to_message"]["text"] = json!("owner message"),
            "missing" => update["message"]["reply_to_message"]["text"] = json!("[not here] notice"),
            "dead" => {
                runtime.state.mcp.to_session.remove("sender-mcp");
            }
            "ambiguous" => {
                let result = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(&runtime.state,
                    &json!({"action":"register","name":"coordinator","tuic_session":"44444444-4444-4444-8444-444444444444"}), Some("duplicate-mcp"), None).await;
                assert!(result.get("error").is_none(), "{result}");
            }
            _ => unreachable!(),
        }
        let mail = Update::parse(
            &update,
            &runtime.outbound.paths.allowlist().unwrap(),
            "test-bot",
            "",
        )
        .unwrap()
        .mail
        .unwrap();
        runtime.deliver(mail).await.unwrap();
        let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &runtime.state,
            &json!({"action":"inbox","since":0}),
            Some("target-mcp"),
            None,
        )
        .await;
        assert_eq!(inbox["count"], 1, "{scenario}: {inbox}");
    }
}

// Catches: sends revoke open owner requests or make their reply authority follow a new recipient.
#[tokio::test]
async fn sender_notice_preserves_pending_request_and_reply_finish() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    let update: Value =
        serde_json::from_str(include_str!("fixtures/reply-to-sender.json")).unwrap();
    let mail = Update::parse(
        &update,
        &runtime.outbound.paths.allowlist().unwrap(),
        "test-bot",
        "",
    )
    .unwrap()
    .mail
    .unwrap();
    let id = mail.id.clone();
    runtime.deliver(mail).await.unwrap();
    runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Send {
                text: "notice".into(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(runtime.pending.len(), 1);
    assert_eq!(runtime.registered_peer(), Some(PEER));
    runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Finish {
                request_id: id,
                text: "answer".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        server
            .requests()
            .iter()
            .rev()
            .find(|(method, _)| method.ends_with("sendMessage"))
            .unwrap()
            .1["text"],
        "answer"
    );
    assert!(runtime.pending.is_empty());
}

// Catches: a non-owner notice silently revokes the inbound owner's open buttons.
#[tokio::test]
async fn non_owner_notice_preserves_owner_callback_handles() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    runtime
        .tool(
            PEER,
            "target-mcp",
            Input::Send {
                text: "owner choice".into(),
                buttons: vec![vec![super::tool::Button {
                    label: "yes".into(),
                    data: "owner answer".into(),
                }]],
            },
        )
        .await
        .unwrap();
    let sent = server.requests()[0].1.clone();
    runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Send {
                text: "notice".into(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    assert!(
        runtime
            .callback(
                &json!({"callback_query":{"id":"query","from":{"id":1111111},
        "message":{"message_id":101,"date":1,"chat":{"id":1111111,"type":"private"}},
        "data":sent["reply_markup"]["inline_keyboard"][0][0]["callback_data"]}})
            )
            .await
            .unwrap()
    );
    let inbox = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
        &runtime.state,
        &json!({"action":"inbox","since":0}),
        Some("target-mcp"),
        None,
    )
    .await;
    assert_eq!(inbox["count"], 1, "{inbox}");
    let body: Value =
        serde_json::from_str(inbox["messages"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(body["data"], "owner answer");
}

// Catches: outbound send still depends on having any inbound owner at all.
#[tokio::test]
async fn sender_can_notify_without_any_inbound_owner() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    add_sender(&runtime).await;
    runtime
        .tool(PEER, "target-mcp", Input::Unregister {})
        .await
        .unwrap();
    runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Send {
                text: "notice".into(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(server.requests()[0].1["text"], "[coordinator] notice");
    assert!(runtime.recipient_peer().is_none());
}

// Catches: an unparseable peer name forges attribution or consumes a send interval.
#[tokio::test]
async fn invalid_sender_names_are_refused_before_outbound_delivery() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let mut runtime = outbound_tests::runtime(paths, server.address).await;
    for name in [
        " ".to_string(),
        "[owner]".to_string(),
        "line\nbreak".to_string(),
        "😀".repeat(65),
    ] {
        let registered = crate::mcp_http::mcp_transport::local_peer_call_with_message_id(
            &runtime.state,
            &json!({"action":"register","name":name,"tuic_session":SENDER}),
            Some("sender-mcp"),
            None,
        )
        .await;
        assert!(registered.get("error").is_none(), "{registered}");
        assert_eq!(
            runtime
                .tool(
                    SENDER,
                    "sender-mcp",
                    Input::Send {
                        text: "notice".into(),
                        buttons: vec![]
                    }
                )
                .await
                .unwrap_err(),
            super::Error::InvalidSenderName
        );
    }
    assert_eq!(server.requests(), [] as [(String, serde_json::Value); 0]);
    add_sender(&runtime).await;
    runtime
        .tool(
            SENDER,
            "sender-mcp",
            Input::Send {
                text: "notice".into(),
                buttons: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(server.requests()[0].1["text"], "[coordinator] notice");
}
