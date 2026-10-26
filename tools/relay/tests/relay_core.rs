use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tuic_relay::relay::AppState;

/// Start a relay server on a random port and return the bound address.
async fn start_relay() -> SocketAddr {
    start_relay_with_state(AppState::new()).await
}

/// Start a relay server with the given state and return the bound address.
async fn start_relay_with_state(state: Arc<AppState>) -> SocketAddr {
    state
        .token_cache
        .insert("test-owner".into(), "owner-hash".into());
    state
        .token_cache
        .insert("other-owner".into(), "other-hash".into());
    let router = tuic_relay::routes::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    addr
}

/// Connect a WebSocket client to the relay for the given session.
async fn connect_ws(
    addr: SocketAddr,
    session_id: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{addr}/ws/{session_id}");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.send(Message::Text("Bearer test-owner".into()))
        .await
        .unwrap();
    ws
}

/// Read the next text message, skipping pings.
async fn read_text(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> String {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => return t.to_string(),
            Some(Ok(Message::Ping(_))) => continue,
            other => panic!("unexpected message: {other:?}"),
        }
    }
}

/// Read the next binary message, skipping pings and text.
async fn read_binary(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Vec<u8> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Binary(b))) => return b.to_vec(),
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Text(_))) => continue,
            other => panic!("unexpected message: {other:?}"),
        }
    }
}

#[tokio::test]
async fn health_check() {
    let addr = start_relay().await;
    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test]
async fn two_peers_exchange_binary_messages() {
    let addr = start_relay().await;
    let session = "test-session-1";

    // Peer A connects — should get "waiting" status
    let mut peer_a = connect_ws(addr, session).await;
    let status = read_text(&mut peer_a).await;
    assert!(
        status.contains("waiting"),
        "expected waiting, got: {status}"
    );

    // Peer B connects — both should get "connected" status
    let mut peer_b = connect_ws(addr, session).await;
    let status_b = read_text(&mut peer_b).await;
    assert!(
        status_b.contains("connected"),
        "expected connected, got: {status_b}"
    );
    let status_a = read_text(&mut peer_a).await;
    assert!(
        status_a.contains("connected"),
        "expected connected, got: {status_a}"
    );

    // Peer A sends binary (simulating E2E encrypted blob)
    let payload_a = b"encrypted-data-from-a".to_vec();
    peer_a
        .send(Message::Binary(payload_a.clone().into()))
        .await
        .unwrap();

    // Peer B receives it
    let received = read_binary(&mut peer_b).await;
    assert_eq!(received, payload_a);

    // Peer B sends binary back
    let payload_b = b"encrypted-data-from-b".to_vec();
    peer_b
        .send(Message::Binary(payload_b.clone().into()))
        .await
        .unwrap();

    // Peer A receives it
    let received = read_binary(&mut peer_a).await;
    assert_eq!(received, payload_b);
}

#[tokio::test]
async fn third_peer_rejected() {
    let addr = start_relay().await;
    let session = "test-session-full";

    let mut peer_a = connect_ws(addr, session).await;
    read_text(&mut peer_a).await;
    let mut peer_b = connect_ws(addr, session).await;
    read_text(&mut peer_b).await;
    read_text(&mut peer_a).await;
    let mut peer_c = connect_ws(addr, session).await;
    assert_close(&mut peer_c, 4001).await;
}

#[tokio::test]
async fn peer_disconnect_notifies_remaining() {
    let addr = start_relay().await;
    let session = "test-session-disconnect";

    let mut peer_a = connect_ws(addr, session).await;
    let _status = read_text(&mut peer_a).await; // waiting

    let mut peer_b = connect_ws(addr, session).await;
    let _status = read_text(&mut peer_b).await; // connected
    let _status = read_text(&mut peer_a).await; // connected

    // Peer B disconnects
    peer_b.close(None).await.unwrap();

    // Peer A should get "disconnected" status
    let status = read_text(&mut peer_a).await;
    assert!(
        status.contains("disconnected"),
        "expected disconnected, got: {status}"
    );
}

#[tokio::test]
async fn relay_push_hint_forwarded_to_connected_peer() {
    let addr = start_relay().await;
    let session = "test-session-push";

    let mut peer_a = connect_ws(addr, session).await;
    let _status = read_text(&mut peer_a).await; // waiting

    let mut peer_b = connect_ws(addr, session).await;
    let _status = read_text(&mut peer_b).await; // connected
    let _status = read_text(&mut peer_a).await; // connected

    // Peer A sends a relay:push hint
    let push_msg = r#"{"type":"relay:push","reason":"awaiting_input","session_name":"dev-server"}"#;
    peer_a.send(Message::Text(push_msg.into())).await.unwrap();

    // Peer B should receive the push hint forwarded
    let received = read_text(&mut peer_b).await;
    assert!(
        received.contains("relay:push"),
        "expected relay:push, got: {received}"
    );
    assert!(received.contains("awaiting_input"));
}

#[tokio::test]
async fn relay_push_hint_with_single_peer_does_not_crash() {
    let addr = start_relay().await;
    let session = "test-session-push-solo";

    let mut peer_a = connect_ws(addr, session).await;
    let _status = read_text(&mut peer_a).await; // waiting

    // Peer A sends relay:push — no other peer connected, no VAPID configured
    // Should not crash or hang
    let push_msg = r#"{"type":"relay:push","reason":"awaiting_input","session_name":"dev-server"}"#;
    peer_a.send(Message::Text(push_msg.into())).await.unwrap();

    // Send a regular text message after to verify the connection is still alive
    peer_a.send(Message::Text("ping".into())).await.unwrap();

    // Give the relay a moment to process
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connection should still be alive — close cleanly
    peer_a.close(None).await.unwrap();
}

#[tokio::test]
async fn invalid_session_id_rejected() {
    let addr = start_relay().await;

    // Empty session ID
    let resp = reqwest::get(format!("http://{addr}/ws/")).await.unwrap();
    // Should get 404 (no route match) or 400
    assert!(resp.status().is_client_error());
}

#[tokio::test]
async fn idle_session_receives_timeout_and_is_cleaned_up() {
    let state = AppState::new();
    // Spawn reaper with 1-second timeout, checking every 200ms
    tuic_relay::relay::spawn_session_reaper(
        state.clone(),
        Duration::from_secs(1),
        Duration::from_millis(200),
    );
    let addr = start_relay_with_state(state.clone()).await;

    let mut peer = connect_ws(addr, "timeout-test").await;
    let status = read_text(&mut peer).await;
    assert!(
        status.contains("waiting"),
        "expected waiting, got: {status}"
    );

    // Wait for the session to be reaped (1s idle + 200ms check interval + margin)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Should receive timeout status
    let msg = tokio::time::timeout(Duration::from_secs(2), read_text(&mut peer)).await;
    match msg {
        Ok(text) => assert!(text.contains("timeout"), "expected timeout, got: {text}"),
        Err(_) => {
            // Connection may have been closed — that's also acceptable
        }
    }

    // Session should be removed from state
    assert!(
        state.sessions.is_empty(),
        "expected sessions to be empty after timeout"
    );
}

// Catches the prototype replaying a non-Bearer first frame into an admitted room.
#[tokio::test]
async fn relay_admission_missing_auth_is_rejected() {
    let state = AppState::new();
    let addr = start_relay_with_state(state.clone()).await;
    let (mut peer, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/missing-auth"))
        .await
        .unwrap();
    peer.send(Message::Text("not-authentication".into()))
        .await
        .unwrap();
    match peer.next().await {
        Some(Ok(Message::Close(Some(frame)))) => assert_eq!(frame.code, 4003.into()),
        other => panic!("expected authentication rejection, got {other:?}"),
    }
    assert!(state.sessions.is_empty());
}

type Peer =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn assert_close(peer: &mut Peer, code: u16) {
    match peer.next().await {
        Some(Ok(Message::Close(Some(frame)))) => assert_eq!(frame.code, code.into()),
        other => panic!("expected close {code}, got {other:?}"),
    }
}

// Catches invalid or binary first frames entering a room without verified ownership.
#[tokio::test]
async fn relay_admission_invalid_first_frames_are_rejected() {
    let state = AppState::new();
    let addr = start_relay_with_state(state.clone()).await;
    for frame in [
        Message::Text("Bearer invalid".into()),
        Message::Binary(vec![1].into()),
    ] {
        let (mut peer, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/ws/invalid-auth"))
                .await
                .unwrap();
        peer.send(frame).await.unwrap();
        assert_close(&mut peer, 4003).await;
        assert!(state.sessions.is_empty());
    }
}

// Catches another valid token replacing the room owner's hash.
#[tokio::test]
async fn relay_admission_other_token_cannot_join_or_replace_owner() {
    let addr = start_relay().await;
    let mut owner = connect_ws(addr, "owned").await;
    read_text(&mut owner).await;
    let (mut other, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/owned"))
        .await
        .unwrap();
    other
        .send(Message::Text("Bearer other-owner".into()))
        .await
        .unwrap();
    assert_close(&mut other, 4003).await;
    let mut phone = connect_ws(addr, "owned").await;
    assert!(read_text(&mut phone).await.contains("connected"));
    read_text(&mut owner).await;
    phone.send(Message::Binary(vec![42].into())).await.unwrap();
    assert_eq!(read_binary(&mut owner).await, vec![42]);
}

// Catches counting a second peer as a new room at the token's room cap.
#[tokio::test]
async fn relay_admission_second_peer_succeeds_at_room_cap() {
    let mut state = AppState::new();
    Arc::get_mut(&mut state).unwrap().max_sessions_per_token = 1;
    let addr = start_relay_with_state(state).await;
    let mut desktop = connect_ws(addr, "one-room").await;
    read_text(&mut desktop).await;
    let mut phone = connect_ws(addr, "one-room").await;
    assert!(read_text(&mut phone).await.contains("connected"));
    let mut extra = connect_ws(addr, "extra-room").await;
    assert_close(&mut extra, 4002).await;
}

// Catches capacity checks occurring before an auth await instead of at admission.
#[tokio::test]
async fn relay_admission_concurrent_peers_cannot_exceed_capacity() {
    let addr = start_relay().await;
    let url = format!("ws://{addr}/ws/concurrent");
    let ((a, _), (b, _), (c, _)) = tokio::join!(
        async { tokio_tungstenite::connect_async(&url).await.unwrap() },
        async { tokio_tungstenite::connect_async(&url).await.unwrap() },
        async { tokio_tungstenite::connect_async(&url).await.unwrap() },
    );
    let authenticate = |mut peer: Peer| async move {
        peer.send(Message::Text("Bearer test-owner".into()))
            .await
            .unwrap();
        let first = peer.next().await.unwrap().unwrap();
        (peer, first)
    };
    let (a, b, c) = tokio::join!(authenticate(a), authenticate(b), authenticate(c));
    let frames = [&a.1, &b.1, &c.1];
    assert_eq!(
        frames
            .iter()
            .filter(|frame| matches!(frame, Message::Text(_)))
            .count(),
        2
    );
    assert_eq!(
        frames
            .iter()
            .filter(
                |frame| matches!(frame, Message::Close(Some(close)) if close.code == 4001.into())
            )
            .count(),
        1
    );
}

// Catches concurrent new rooms both passing a per-token count before insertion.
#[tokio::test]
async fn relay_admission_concurrent_rooms_cannot_exceed_token_cap() {
    let mut state = AppState::new();
    Arc::get_mut(&mut state).unwrap().max_sessions_per_token = 1;
    let addr = start_relay_with_state(state).await;
    let (mut a, mut b) = tokio::join!(connect_ws(addr, "room-a"), connect_ws(addr, "room-b"));
    let (a, b) = tokio::join!(a.next(), b.next());
    let frames = [a.unwrap().unwrap(), b.unwrap().unwrap()];
    assert_eq!(
        frames
            .iter()
            .filter(|frame| matches!(frame, Message::Text(_)))
            .count(),
        1
    );
    assert_eq!(
        frames
            .iter()
            .filter(
                |frame| matches!(frame, Message::Close(Some(close)) if close.code == 4002.into())
            )
            .count(),
        1
    );
}

// Catches auth timeout falling through to anonymous room admission.
#[tokio::test]
async fn relay_admission_silent_peer_is_rejected_after_auth_timeout() {
    let state = AppState::new();
    let addr = start_relay_with_state(state.clone()).await;
    let (mut peer, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/silent"))
        .await
        .unwrap();
    assert_close(&mut peer, 4003).await;
    assert!(state.sessions.is_empty());
}

/// An expired socket must not send into a room recreated under a different owner.
#[tokio::test]
async fn expired_peer_cannot_forward_into_recreated_room_owned_by_another_token() {
    expired_peer_cannot_access_recreated_room("other-owner").await;
}

// Catches stale cleanup deleting a new peer when the same owner reuses a room.
#[tokio::test]
async fn expired_peer_cannot_remove_recreated_room_owned_by_same_token() {
    expired_peer_cannot_access_recreated_room("test-owner").await;
}

async fn expired_peer_cannot_access_recreated_room(token: &str) {
    let state = AppState::new();
    let addr = start_relay_with_state(state.clone()).await;
    let mut expired = connect_ws(addr, "reused-room").await;
    assert!(read_text(&mut expired).await.contains("waiting"));
    state.sessions.get_mut("reused-room").unwrap().last_activity =
        std::time::Instant::now() - Duration::from_secs(60);
    tuic_relay::relay::spawn_session_reaper(
        state.clone(),
        Duration::from_secs(30),
        Duration::from_millis(10),
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(5), read_text(&mut expired))
            .await
            .unwrap()
            .contains("timeout")
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !state.sessions.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("reaper did not remove expired room");

    let url = format!("ws://{addr}/ws/reused-room");
    let (mut owner_a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    owner_a
        .send(Message::Text(format!("Bearer {token}").into()))
        .await
        .unwrap();
    assert!(read_text(&mut owner_a).await.contains("waiting"));
    let (mut owner_b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    owner_b
        .send(Message::Text(format!("Bearer {token}").into()))
        .await
        .unwrap();
    assert!(read_text(&mut owner_b).await.contains("connected"));
    assert!(read_text(&mut owner_a).await.contains("connected"));

    // The old owner's socket is no longer authorized for this room incarnation.
    let payload = b"old-owner-data".to_vec();
    let _ = expired.send(Message::Binary(payload.clone().into())).await;
    if let Ok(Some(Ok(Message::Binary(received)))) =
        tokio::time::timeout(Duration::from_millis(200), owner_b.next()).await
    {
        assert_ne!(
            received.as_ref(),
            payload.as_slice(),
            "expired peer forwarded data into another token's recreated room"
        );
    }
    // Eviction must terminate the old socket, not just hide its room entry.
    let closed = tokio::time::timeout(Duration::from_secs(5), expired.next())
        .await
        .expect("expired socket reader remained alive");
    assert!(
        matches!(closed, None | Some(Err(_)) | Some(Ok(Message::Close(_)))),
        "expired socket remained open: {closed:?}"
    );

    // A positive delivery barrier proves both replacement peers survive stale
    // handler cleanup and that the room remains usable at its two-peer limit.
    let fresh = b"replacement-owner-data".to_vec();
    owner_a
        .send(Message::Binary(fresh.clone().into()))
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), read_binary(&mut owner_b))
            .await
            .expect("replacement room lost a peer"),
        fresh
    );
    assert_eq!(
        state
            .sessions
            .get("reused-room")
            .unwrap()
            .token_hash
            .as_deref(),
        Some(if token == "test-owner" {
            "owner-hash"
        } else {
            "other-hash"
        })
    );
}

type TestWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Catches a silent `try_send` drop: a tunnel rides this room, so a peer that
/// stops reading must end the room, never receive a stream with a hole in it.
#[tokio::test]
async fn slow_peer_overflow_closes_the_room_instead_of_dropping_frames() {
    let mut state = AppState::new();
    Arc::get_mut(&mut state).unwrap().timing.peer_send_timeout = Duration::from_millis(300);
    let addr = start_relay_with_state(state).await;
    let mut sender = connect_ws(addr, "overflow").await;
    let _ = read_text(&mut sender).await;
    let mut slow = connect_ws(addr, "overflow").await;
    let _ = read_text(&mut slow).await;
    let _ = read_text(&mut sender).await;

    // The slow peer reads nothing while the sender fills every buffer on the
    // path: its TCP window, the relay's write loop and the 32-slot channel.
    let sender_task = tokio::spawn(async move {
        for i in 0u32..1500 {
            let mut payload = vec![0u8; 64 * 1024];
            payload[..4].copy_from_slice(&i.to_be_bytes());
            if sender.send(Message::Binary(payload.into())).await.is_err() {
                return;
            }
        }
        // Still connected after 1500 frames: keep probing for the close.
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    tokio::time::sleep(Duration::from_secs(3)).await;

    let mut next = 0u32;
    let ended = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match slow.next().await {
                Some(Ok(Message::Binary(b))) => {
                    let seq = u32::from_be_bytes(b[..4].try_into().unwrap());
                    assert_eq!(seq, next, "a frame was dropped before the room closed");
                    next += 1;
                }
                Some(Ok(_)) => continue,
                _ => return,
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the room stayed open after an overflow");
    assert!(next < 1500, "no overflow happened, the test proves nothing");
    let _ = sender_task.await;
}

/// Catches an unbounded WS message: one frame must not be able to allocate
/// more than the tunnel record limit allows.
#[tokio::test]
async fn oversized_message_ends_the_connection_and_is_not_forwarded() {
    let addr = start_relay().await;
    let mut a = connect_ws(addr, "oversize").await;
    let _ = read_text(&mut a).await;
    let mut b: TestWs = connect_ws(addr, "oversize").await;
    let _ = read_text(&mut b).await;
    let _ = read_text(&mut a).await;

    let _ = a
        .send(Message::Binary(vec![7u8; 2 * 1024 * 1024].into()))
        .await;
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match b.next().await {
                Some(Ok(Message::Binary(_))) => return "forwarded",
                Some(Ok(Message::Text(t))) if t.contains("disconnected") => return "closed",
                Some(Ok(_)) => continue,
                _ => return "closed",
            }
        }
    })
    .await;
    assert_eq!(outcome, Ok("closed"));
}

/// Catches a burst closing the room: a peer that is only briefly slow must
/// receive every frame in order, because the relay waits before it gives up.
#[tokio::test]
async fn brief_stall_of_a_peer_delays_frames_but_loses_none() {
    let addr = start_relay().await;
    let mut sender = connect_ws(addr, "burst").await;
    let _ = read_text(&mut sender).await;
    let mut slow = connect_ws(addr, "burst").await;
    let _ = read_text(&mut slow).await;
    let _ = read_text(&mut sender).await;

    // 100 frames overrun the 32-slot channel while the reader sleeps 1 s.
    let sender_task = tokio::spawn(async move {
        for i in 0u32..100 {
            let mut payload = vec![0u8; 64 * 1024];
            payload[..4].copy_from_slice(&i.to_be_bytes());
            sender.send(Message::Binary(payload.into())).await.unwrap();
        }
        sender
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    for i in 0u32..100 {
        let b = read_binary(&mut slow).await;
        assert_eq!(u32::from_be_bytes(b[..4].try_into().unwrap()), i);
    }
    let _ = sender_task.await;
}

/// Catches a dead TCP peer pinning a room: a peer that answers nothing is
/// reaped and the remaining peer is told.
#[tokio::test]
async fn silent_peer_is_reaped_and_the_other_peer_is_told() {
    let mut state = AppState::new();
    {
        let timing = &mut Arc::get_mut(&mut state).unwrap().timing;
        timing.ping_interval = Duration::from_millis(100);
        timing.dead_after = Duration::from_millis(500);
    }
    let addr = start_relay_with_state(state).await;
    let mut live = connect_ws(addr, "dead").await;
    let _ = read_text(&mut live).await;
    // Never polled again: tungstenite answers pings only while it is read.
    let mut silent = connect_ws(addr, "dead").await;
    let _ = read_text(&mut silent).await;
    let _ = read_text(&mut live).await;

    let told = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Ok(Message::Text(t))) = live.next().await {
                if t.contains("disconnected") {
                    return;
                }
            }
        }
    })
    .await;
    assert!(told.is_ok(), "a silent peer was never reaped");
    drop(silent);
}
