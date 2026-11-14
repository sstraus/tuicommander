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

/// Runs an agent forever and tells the test when it is dropped.
struct Running {
    pairing_id: String,
    log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Running {
    async fn forever(agent: Arc<DaemonAgent>, log: Arc<std::sync::Mutex<Vec<String>>>) {
        let guard = Self {
            pairing_id: agent.bundle.pairing_id.clone(),
            log,
        };
        guard
            .log
            .lock()
            .unwrap()
            .push(format!("start {}", guard.pairing_id));
        std::future::pending::<()>().await;
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.log
            .lock()
            .unwrap()
            .push(format!("stop {}", self.pairing_id));
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

// Catches: a pairing deleted while the daemon runs keeps its agent, so the
// revoked desktop still attaches until the next restart; and a pairing stored
// after startup is never loaded.
#[tokio::test]
async fn the_agent_follows_the_stored_pairing_without_a_restart() {
    let _ = credentials::delete(Credential::RelayPairing(DAEMON_PAIRING_ID));
    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let upstream: SocketAddr = "127.0.0.1:9877".parse().unwrap();
    let sink = log.clone();
    let supervisor = tokio::spawn(supervise(
        move || load_agent(upstream, "D"),
        Duration::from_millis(20),
        move |agent| Running::forever(agent, sink.clone()),
        std::future::pending::<()>(),
    ));
    settle().await;
    assert!(log.lock().unwrap().is_empty(), "no pairing, no agent");

    let first = PairingBundle::parse(&pair("wss://relay.example", bearer()).unwrap()).unwrap();
    settle().await;
    assert_eq!(
        *log.lock().unwrap(),
        [format!("start {}", first.pairing_id)]
    );

    credentials::delete(Credential::RelayPairing(DAEMON_PAIRING_ID)).unwrap();
    settle().await;
    assert_eq!(
        log.lock().unwrap().last().unwrap(),
        &format!("stop {}", first.pairing_id)
    );

    let second = PairingBundle::parse(&pair("wss://relay.example", bearer()).unwrap()).unwrap();
    settle().await;
    assert_eq!(
        log.lock().unwrap().last().unwrap(),
        &format!("start {}", second.pairing_id)
    );
    supervisor.abort();
    let _ = credentials::delete(Credential::RelayPairing(DAEMON_PAIRING_ID));
}

// Catches a daemon poll that reads only its own in-process vault cache.
// `tuic-remote --relay-pair` and an operator's delete run in another process
// and change the OS keyring; a poll that never reaches the keyring keeps a
// deleted pairing usable until restart.
#[test]
fn a_pairing_deleted_by_another_process_is_no_longer_loaded() {
    let upstream: SocketAddr = "127.0.0.1:9877".parse().unwrap();
    pair("wss://relay.example", bearer()).unwrap();
    assert!(load_agent(upstream, "D").unwrap().is_some());

    // Another process removes the pairing from the keyring entry.
    let instance = crate::app_instance::current_app_instance();
    let entry = keyring::Entry::new(instance.vault_service(), instance.vault_user()).unwrap();
    let mut vault: std::collections::HashMap<String, String> =
        serde_json::from_str(&entry.get_password().unwrap()).unwrap();
    vault.remove("remote/relay-pairing/daemon");
    entry
        .set_password(&serde_json::to_string(&vault).unwrap())
        .unwrap();

    assert!(
        load_agent(upstream, "D").unwrap().is_none(),
        "the daemon still loads a pairing deleted from the keyring"
    );
}
