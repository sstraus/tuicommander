use super::*;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

fn bearer() -> RelayBearer {
    RelayBearer::new("relay-secret-R".into()).unwrap()
}

async fn server() -> (url::Url, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!(
        "ws://127.0.0.1:{}",
        listener.local_addr().unwrap().port()
    ))
    .unwrap();
    (url, listener)
}

#[tokio::test]
async fn first_frame_is_the_bearer_and_records_travel_both_ways() {
    let (url, listener) = server().await;
    let relay = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        let first = ws.next().await.unwrap().unwrap();
        ws.send(Message::Binary(vec![9, 9].into())).await.unwrap();
        let record = ws.next().await.unwrap().unwrap();
        (first, record)
    });
    let mut link = connect(&url, "room-1", &bearer()).await.unwrap();
    assert_eq!(link.rx.recv().await.unwrap(), LinkItem::Record(vec![9, 9]));
    link.tx.send(vec![1, 2, 3]).await.unwrap();
    let (first, record) = relay.await.unwrap();
    assert_eq!(first, Message::Text("Bearer relay-secret-R".into()));
    assert_eq!(record, Message::Binary(vec![1, 2, 3].into()));
}

#[tokio::test]
async fn relay_status_arrives_in_order_with_the_records() {
    let (url, listener) = server().await;
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        let _ = ws.next().await;
        ws.send(Message::Text(
            r#"{"type":"relay:status","peer":"waiting"}"#.into(),
        ))
        .await
        .unwrap();
        ws.send(Message::Text(
            r#"{"type":"relay:status","peer":"connected"}"#.into(),
        ))
        .await
        .unwrap();
        ws.send(Message::Binary(vec![7].into())).await.unwrap();
        ws.send(Message::Text(
            r#"{"type":"relay:status","peer":"disconnected"}"#.into(),
        ))
        .await
        .unwrap();
        let _ = ws.next().await;
    });
    let mut link = connect(&url, "room-1", &bearer()).await.unwrap();
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(link.rx.recv().await.unwrap());
    }
    assert_eq!(
        seen,
        vec![
            LinkItem::Peer(Presence::Waiting),
            LinkItem::Peer(Presence::Connected),
            LinkItem::Record(vec![7]),
            LinkItem::Peer(Presence::Gone),
        ]
    );
}

#[tokio::test]
async fn relay_close_ends_the_receive_side() {
    let (url, listener) = server().await;
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        let _ = ws.next().await;
        ws.close(None).await.unwrap();
    });
    let mut link = connect(&url, "room-1", &bearer()).await.unwrap();
    assert!(link.rx.recv().await.is_none());
}

#[tokio::test]
async fn oversized_message_ends_the_link() {
    let (url, listener) = server().await;
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        let _ = ws.next().await;
        let _ = ws
            .send(Message::Binary(vec![0; MAX_WS_MESSAGE + 1].into()))
            .await;
        let _ = ws.next().await;
    });
    let mut link = connect(&url, "room-1", &bearer()).await.unwrap();
    assert!(link.rx.recv().await.is_none());
}

#[tokio::test]
async fn unreachable_relay_is_a_connect_error_without_the_bearer() {
    let socket = crate::test_support::resetting_tcp_peer();
    let url = url::Url::parse(&format!("ws://{}", socket.local_addr().unwrap())).unwrap();
    let err = connect(&url, "room-1", &bearer()).await.err().unwrap();
    assert!(matches!(err, LinkError::Connect(_)));
    assert!(!err.to_string().contains("relay-secret-R"));
}
