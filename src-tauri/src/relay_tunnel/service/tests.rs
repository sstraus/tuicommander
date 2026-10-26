use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

fn bearer() -> RelayBearer {
    RelayBearer::new("R".into()).unwrap()
}

// Catches a pairing that is written but not found again by the daemon start,
// or whose stored secret differs from the bundle handed to the desktop.
#[test]
fn the_stored_pairing_is_the_one_handed_to_the_desktop() {
    let upstream: SocketAddr = "127.0.0.1:9877".parse().unwrap();
    let text = pair("wss://relay.example", bearer()).unwrap();
    let agent = load_agent(upstream, "D").unwrap().expect("stored pairing");
    let handed_out = PairingBundle::parse(&text).unwrap();
    assert_eq!(agent.bundle.room, handed_out.room);
    assert_eq!(agent.bundle.pairing_id, handed_out.pairing_id);
    assert_eq!(agent.bundle.secret.as_bytes(), handed_out.secret.as_bytes());
    assert_eq!(agent.upstream, upstream);
}

// Catches a second pairing that leaves the first desktop's secret valid.
#[test]
fn pairing_again_replaces_the_secret() {
    let first = pair("wss://relay.example", bearer()).unwrap();
    let second = pair("wss://relay.example", bearer()).unwrap();
    let first = PairingBundle::parse(&first).unwrap();
    let second = PairingBundle::parse(&second).unwrap();
    assert_ne!(first.secret.as_bytes(), second.secret.as_bytes());
    assert_ne!(first.room, second.room);
}

#[tokio::test]
async fn registration_reads_the_token_from_the_relay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        assert!(request.starts_with("POST /register "));
        let body = r#"{"token":"issued-token"}"#;
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
    });
    let got = register_bearer(&format!("ws://127.0.0.1:{port}"))
        .await
        .unwrap();
    assert_eq!(got.expose(), "issued-token");
}

#[tokio::test]
async fn registration_refused_by_the_relay_is_an_error_without_secrets() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        socket
            .write_all(
                b"HTTP/1.1 429 Too Many Requests\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            )
            .await
            .unwrap();
    });
    let err = register_bearer(&format!("ws://127.0.0.1:{port}"))
        .await
        .unwrap_err();
    assert!(err.contains("relay registration failed"), "{err}");
}

// Catches a bundle file readable by other users, or one that silently
// overwrites an existing file.
#[cfg(unix)]
#[test]
fn the_bundle_file_is_private_and_never_overwrites() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bundle");
    write_private_new(&path, b"secret").unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(write_private_new(&path, b"other").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"secret");
}
