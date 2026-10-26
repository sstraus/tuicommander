use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::*;
use crate::relay_tunnel::bundle::RelayBearer;
use crate::relay_tunnel::daemon::{self, DaemonAgent};
use crate::relay_tunnel::secure::PairingSecret;

const WAIT: Duration = Duration::from_secs(5);

fn bundle() -> PairingBundle {
    PairingBundle::generate("ws://127.0.0.1:1", RelayBearer::new("R".into()).unwrap()).unwrap()
}

fn timing() -> Timing {
    Timing {
        daemon_wait: Duration::from_millis(300),
        handshake: Duration::from_millis(500),
        credential: Duration::from_secs(2),
        mux: MuxConfig {
            ping_interval: Duration::from_millis(50),
            dead_after: Duration::from_millis(600),
            ..MuxConfig::default()
        },
    }
}

/// A two-peer room: records cross in both directions, presence is injected by
/// the test the way the relay would announce it.
struct Room {
    desktop: WsLink,
    daemon: WsLink,
    to_desktop: mpsc::Sender<LinkItem>,
    to_daemon: mpsc::Sender<LinkItem>,
}

fn room() -> Room {
    let (d_out, mut d_out_rx) = mpsc::channel::<Vec<u8>>(64);
    let (m_out, mut m_out_rx) = mpsc::channel::<Vec<u8>>(64);
    let (to_desktop, desktop_rx) = mpsc::channel::<LinkItem>(64);
    let (to_daemon, daemon_rx) = mpsc::channel::<LinkItem>(64);
    let into_daemon = to_daemon.clone();
    tokio::spawn(async move {
        while let Some(record) = d_out_rx.recv().await {
            if into_daemon.send(LinkItem::Record(record)).await.is_err() {
                break;
            }
        }
    });
    let into_desktop = to_desktop.clone();
    tokio::spawn(async move {
        while let Some(record) = m_out_rx.recv().await {
            if into_desktop.send(LinkItem::Record(record)).await.is_err() {
                break;
            }
        }
    });
    Room {
        desktop: WsLink {
            tx: d_out,
            rx: desktop_rx,
        },
        daemon: WsLink {
            tx: m_out,
            rx: daemon_rx,
        },
        to_desktop,
        to_daemon,
    }
}

async fn echo_upstream() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 || socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

fn start_daemon(room: &mut Room, bundle: &PairingBundle, upstream: SocketAddr) {
    let agent = Arc::new(DaemonAgent {
        bundle: bundle.clone(),
        upstream,
        daemon_token: Zeroizing::new("D-token".into()),
        mux: timing().mux,
    });
    let link = std::mem::replace(
        &mut room.daemon,
        WsLink {
            tx: mpsc::channel(1).0,
            rx: mpsc::channel(1).1,
        },
    );
    tokio::spawn(daemon::serve(agent, link));
}

async fn announce_both(room: &Room) {
    room.to_desktop
        .send(LinkItem::Peer(Presence::Connected))
        .await
        .unwrap();
    room.to_daemon
        .send(LinkItem::Peer(Presence::Connected))
        .await
        .unwrap();
}

async fn echo_through(session: &RelaySession, text: &[u8]) -> Vec<u8> {
    let mut tcp = TcpStream::connect(("127.0.0.1", session.port))
        .await
        .unwrap();
    tcp.write_all(text).await.unwrap();
    let mut got = vec![0u8; text.len()];
    tokio::time::timeout(WAIT, tcp.read_exact(&mut got))
        .await
        .unwrap()
        .unwrap();
    got
}

#[tokio::test]
async fn attached_session_carries_loopback_connections_and_hands_over_d() {
    let upstream = echo_upstream().await;
    let bundle = bundle();
    let mut room = room();
    start_daemon(&mut room, &bundle, upstream);
    announce_both(&room).await;
    let session = RelaySession::attach(room.desktop, &bundle, timing())
        .await
        .expect("attach");
    assert_eq!(session.daemon_token(), "D-token");
    assert!(session.base_url().starts_with("http://127.0.0.1:"));
    assert_eq!(
        echo_through(&session, b"over the relay").await,
        b"over the relay"
    );
    // A second loopback connection is a second stream, not a second session.
    assert_eq!(echo_through(&session, b"again").await, b"again");
}

// Catches an attach that reports success to a daemon holding a different secret.
#[tokio::test]
async fn wrong_secret_is_rejected_with_a_typed_reason() {
    let upstream = echo_upstream().await;
    let daemon_bundle = bundle();
    let mut desktop_bundle = daemon_bundle.clone();
    desktop_bundle.secret = PairingSecret::generate().unwrap();
    let mut room = room();
    start_daemon(&mut room, &daemon_bundle, upstream);
    announce_both(&room).await;
    let err = RelaySession::attach(room.desktop, &desktop_bundle, timing())
        .await
        .err()
        .expect("must fail");
    assert_eq!(err, RelayFailure::PairingRejected);
    assert_eq!(err.code(), "pairing_rejected");
}

#[tokio::test]
async fn empty_room_is_reported_as_daemon_offline() {
    let bundle = bundle();
    let room = room();
    room.to_desktop
        .send(LinkItem::Peer(Presence::Waiting))
        .await
        .unwrap();
    let err = RelaySession::attach(room.desktop, &bundle, timing())
        .await
        .err()
        .expect("must fail");
    assert_eq!(err, RelayFailure::DaemonOffline);
}

// Catches a session that keeps serving after the daemon left the room, which
// would hide the loss until the heartbeat expires.
#[tokio::test]
async fn daemon_leaving_ends_the_session_at_once() {
    let upstream = echo_upstream().await;
    let bundle = bundle();
    let mut room = room();
    start_daemon(&mut room, &bundle, upstream);
    announce_both(&room).await;
    let session = RelaySession::attach(room.desktop, &bundle, timing())
        .await
        .expect("attach");
    room.to_desktop
        .send(LinkItem::Peer(Presence::Gone))
        .await
        .unwrap();
    let lost = tokio::time::timeout(Duration::from_millis(400), session.lost())
        .await
        .expect("session must end before any heartbeat could expire");
    assert_eq!(lost, RelayFailure::LinkLost);
}

#[tokio::test]
async fn dropping_the_session_closes_the_loopback_port() {
    let upstream = echo_upstream().await;
    let bundle = bundle();
    let mut room = room();
    start_daemon(&mut room, &bundle, upstream);
    announce_both(&room).await;
    let session = RelaySession::attach(room.desktop, &bundle, timing())
        .await
        .expect("attach");
    let port = session.port;
    drop(session);
    let mut refused = false;
    for _ in 0..40 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            refused = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(refused, "the loopback port must not outlive the session");
}
