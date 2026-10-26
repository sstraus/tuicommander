use std::sync::Mutex;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::relay_tunnel::bundle::RelayBearer;
use crate::relay_tunnel::mux::{Mux, OpenError};
use crate::relay_tunnel::secure::PairingSecret;

const WAIT: Duration = Duration::from_secs(5);

fn bundle() -> PairingBundle {
    PairingBundle::generate("ws://127.0.0.1:1", RelayBearer::new("R".into()).unwrap()).unwrap()
}

fn mux_cfg() -> MuxConfig {
    MuxConfig {
        ping_interval: Duration::from_millis(50),
        dead_after: Duration::from_millis(600),
        ..MuxConfig::default()
    }
}

/// The relay as the daemon sees it: items in, records out. Records go to
/// whichever desktop attached last, as a relay room with one desktop does.
struct Relay {
    to_daemon: mpsc::Sender<LinkItem>,
    current: Arc<Mutex<mpsc::Sender<Vec<u8>>>>,
    /// Counts attachments; a relay forwards nothing from a desktop that left.
    epoch: Arc<AtomicU32>,
}

fn start_daemon(bundle: PairingBundle, upstream: SocketAddr) -> Relay {
    let (to_daemon, rx) = mpsc::channel(64);
    let (tx, mut from_daemon) = mpsc::channel::<Vec<u8>>(64);
    let (sink, _unused) = mpsc::channel(1);
    let current = Arc::new(Mutex::new(sink));
    let route = current.clone();
    tokio::spawn(async move {
        while let Some(record) = from_daemon.recv().await {
            let sink = route.lock().unwrap().clone();
            let _ = sink.send(record).await;
        }
    });
    let agent = Arc::new(DaemonAgent {
        bundle,
        upstream,
        daemon_token: Zeroizing::new("D-token".into()),
        mux: mux_cfg(),
    });
    tokio::spawn(serve(agent, WsLink { tx, rx }));
    Relay {
        to_daemon,
        current,
        epoch: Arc::new(AtomicU32::new(0)),
    }
}

impl Relay {
    /// A desktop joins the room and runs the handshake with `secret`. `None` when
    /// the daemon never answers.
    async fn attach(&self, bundle: &PairingBundle, secret: &PairingSecret) -> Option<Mux> {
        let (sink, mut from_daemon) = mpsc::channel::<Vec<u8>>(64);
        *self.current.lock().unwrap() = sink;
        let mine = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.to_daemon
            .send(LinkItem::Peer(Presence::Connected))
            .await
            .unwrap();
        let binding = ChannelBinding {
            pairing_id: &bundle.pairing_id,
            room: &bundle.room,
        };
        let mut noise = Handshake::initiator(secret, &binding).unwrap();
        self.to_daemon
            .send(LinkItem::Record(noise.write().unwrap()))
            .await
            .unwrap();
        let reply = tokio::time::timeout(Duration::from_millis(1500), from_daemon.recv())
            .await
            .ok()??;
        noise.read(&reply).ok()?;
        let channel = noise.finish().ok()?;
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
        let to_daemon = self.to_daemon.clone();
        let epoch = self.epoch.clone();
        tokio::spawn(async move {
            while let Some(record) = rx.recv().await {
                if epoch.load(Ordering::SeqCst) != mine {
                    continue;
                }
                if to_daemon.send(LinkItem::Record(record)).await.is_err() {
                    break;
                }
            }
        });
        Some(mux::start(
            Role::Initiator,
            channel,
            tx,
            from_daemon,
            mux_cfg(),
        ))
    }
}

async fn echo_upstream() -> (SocketAddr, Arc<AtomicU32>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicU32::new(0));
    let count = accepted.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
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
    (addr, accepted)
}

async fn echo_once(mux: &Mux, text: &[u8]) -> Vec<u8> {
    let stream = tokio::time::timeout(WAIT, mux.handle.open())
        .await
        .unwrap()
        .unwrap_or_else(|e| panic!("open for {text:?}: {e:?}"));
    stream.write_all(text).await.unwrap();
    let mut got = Vec::new();
    while got.len() < text.len() {
        got.extend(
            tokio::time::timeout(WAIT, stream.read())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
        );
    }
    got
}

#[tokio::test]
async fn attached_desktop_gets_d_and_reaches_only_the_fixed_upstream() {
    let (upstream, accepted) = echo_upstream().await;
    let bundle = bundle();
    let relay = start_daemon(bundle.clone(), upstream);
    let mut desktop = relay.attach(&bundle, &bundle.secret).await.expect("attach");
    match tokio::time::timeout(WAIT, desktop.inbound.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Inbound::Credential(d) => assert_eq!(d, b"D-token"),
        Inbound::Stream(_) => panic!("daemon must not open streams"),
    }
    assert_eq!(
        echo_once(&desktop, b"hello through the relay").await,
        b"hello through the relay"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn wrong_secret_gets_no_answer_and_no_upstream_connection() {
    let (upstream, accepted) = echo_upstream().await;
    let bundle = bundle();
    let relay = start_daemon(bundle.clone(), upstream);
    let wrong = PairingSecret::generate().unwrap();
    assert!(relay.attach(&bundle, &wrong).await.is_none());
    // Garbage after the failed handshake must not open anything either.
    let _ = relay.to_daemon.send(LinkItem::Record(vec![1; 64])).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 0);
    // The failure does not poison the room for the real desktop.
    let desktop = relay.attach(&bundle, &bundle.secret).await.expect("attach");
    echo_once(&desktop, b"x").await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn newest_attachment_replaces_the_previous_session() {
    let (upstream, _) = echo_upstream().await;
    let bundle = bundle();
    let relay = start_daemon(bundle.clone(), upstream);
    let mut first = relay
        .attach(&bundle, &bundle.secret)
        .await
        .expect("first attach");
    echo_once(&first, b"one").await;
    relay
        .to_daemon
        .send(LinkItem::Peer(Presence::Gone))
        .await
        .unwrap();
    let second = relay
        .attach(&bundle, &bundle.secret)
        .await
        .expect("second attach");
    assert_eq!(echo_once(&second, b"two").await, b"two");
    // The daemon no longer serves the first session: the harness has already
    // detached it from the link, and a still-attached peer would time out.
    let reason = tokio::time::timeout(WAIT, first.handle.closed())
        .await
        .unwrap();
    assert!(
        matches!(
            reason,
            mux::CloseReason::LinkClosed | mux::CloseReason::HeartbeatTimeout
        ),
        "first session ended with {reason:?}"
    );
}

#[tokio::test]
async fn unreachable_upstream_refuses_the_stream() {
    // Catches: a failed upstream connect being acknowledged as an open stream.
    // Port 0 cannot name a listener: bind(0) allocates a nonzero service port.
    // Winsock reports WSAEADDRNOTAVAIL for a zero destination port:
    // https://learn.microsoft.com/en-us/windows/win32/winsock/windows-sockets-error-codes-2
    // Assert the mux failure, not the platform-specific connect errno.
    let upstream = ([127, 0, 0, 1], 0).into();
    let bundle = bundle();
    let relay = start_daemon(bundle.clone(), upstream);
    let desktop = relay.attach(&bundle, &bundle.secret).await.expect("attach");
    let err = tokio::time::timeout(WAIT, desktop.handle.open())
        .await
        .unwrap()
        .err()
        .unwrap();
    assert_eq!(err, OpenError::Refused(OpenFailure::UpstreamUnreachable));
}
