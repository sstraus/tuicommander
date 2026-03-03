use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// Start a relay server on a random port and return the bound address.
async fn start_relay() -> SocketAddr {
    let state = tuic_relay::relay::AppState::new();
    let router = tuic_relay::routes::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    addr
}

/// Connect a WebSocket client to the relay for the given session.
async fn connect_ws(
    addr: SocketAddr,
    session_id: &str,
) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let url = format!("ws://{addr}/ws/{session_id}");
    let (ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
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
    let resp = reqwest::get(format!("http://{addr}/health"))
        .await
        .unwrap();
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
    assert!(status.contains("waiting"), "expected waiting, got: {status}");

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

    let mut _peer_a = connect_ws(addr, session).await;
    let mut _peer_b = connect_ws(addr, session).await;

    // Third peer connects — should be closed with code 4001
    let mut peer_c = connect_ws(addr, session).await;

    // The third peer should receive a close frame or connection reset
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), peer_c.next()).await;

    match result {
        // Close frame received (clean rejection)
        Ok(Some(Ok(Message::Close(Some(frame))))) => {
            assert_eq!(frame.code, 4001.into(), "expected close code 4001");
        }
        // Connection closed without frame, or close frame without payload
        Ok(Some(Ok(Message::Close(None)))) | Ok(None) | Err(_) => {}
        // Connection reset (server dropped before close handshake completed)
        Ok(Some(Err(_))) => {}
        other => panic!("expected rejection for third peer, got: {other:?}"),
    }
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
    peer_a
        .send(Message::Text(push_msg.into()))
        .await
        .unwrap();

    // Peer B should receive the push hint forwarded
    let received = read_text(&mut peer_b).await;
    assert!(received.contains("relay:push"), "expected relay:push, got: {received}");
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
    peer_a
        .send(Message::Text(push_msg.into()))
        .await
        .unwrap();

    // Send a regular text message after to verify the connection is still alive
    peer_a
        .send(Message::Text("ping".into()))
        .await
        .unwrap();

    // Give the relay a moment to process
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connection should still be alive — close cleanly
    peer_a.close(None).await.unwrap();
}

#[tokio::test]
async fn invalid_session_id_rejected() {
    let addr = start_relay().await;

    // Empty session ID
    let resp = reqwest::get(format!("http://{addr}/ws/"))
        .await
        .unwrap();
    // Should get 404 (no route match) or 400
    assert!(resp.status().is_client_error());
}
