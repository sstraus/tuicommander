//! The daemon end of the relay tunnel: registers in a relay room (outbound
//! connection only), authenticates each desktop attachment with the pairing
//! secret, and bridges every stream the desktop opens to the daemon's own
//! loopback listener.
//!
//! Trust rules:
//! - A stream is connected to exactly one address, `upstream`, fixed at start.
//!   The desktop names no host and no port, so the tunnel cannot be used to
//!   reach anything else on the daemon's network.
//! - No upstream socket exists before the Noise handshake succeeds, so a peer
//!   holding only the relay bearer R can reach nothing.
//! - D, the daemon API token, leaves this process only inside the encrypted
//!   channel.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use super::bridge::bridge;
use super::bundle::PairingBundle;
use super::frames::OpenFailure;
use super::mux::{self, Inbound, MuxConfig, Role};
use super::secure::{ChannelBinding, Handshake};
use super::ws_link::{self, LinkItem, Presence, WsLink};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_FIRST: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(30);
/// Wait added before the next handshake for every failed one in a row.
const PENALTY_STEP: Duration = Duration::from_millis(500);
const PENALTY_MAX: Duration = Duration::from_secs(8);
const SESSION_QUEUE: usize = 32;

pub(crate) struct DaemonAgent {
    pub bundle: PairingBundle,
    /// The daemon's own HTTP listener on loopback.
    pub upstream: SocketAddr,
    /// D: delivered to the desktop inside the encrypted channel.
    pub daemon_token: Zeroizing<String>,
    pub mux: MuxConfig,
}

/// Keep a registration in the room until `shutdown` resolves.
pub(crate) async fn run(agent: Arc<DaemonAgent>, shutdown: impl Future<Output = ()>) {
    tokio::pin!(shutdown);
    let mut delay = RECONNECT_FIRST;
    loop {
        let attempt = ws_link::connect(
            agent.bundle.relay_url(),
            &agent.bundle.room,
            &agent.bundle.relay_token,
        );
        let link = tokio::select! {
            _ = &mut shutdown => return,
            link = attempt => link,
        };
        match link {
            Ok(link) => {
                delay = RECONNECT_FIRST;
                tracing::info!(source = "relay_tunnel", "registered in the relay room");
                tokio::select! {
                    _ = &mut shutdown => return,
                    _ = serve(agent.clone(), link) => {}
                }
                tracing::warn!(source = "relay_tunnel", "relay connection lost");
            }
            Err(error) => {
                tracing::warn!(source = "relay_tunnel", %error, "relay connection failed");
            }
        }
        tokio::select! {
            _ = &mut shutdown => return,
            _ = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(RECONNECT_MAX);
    }
}

/// Serve one relay connection until it ends. The relay says when a desktop
/// joins or leaves; each join starts a fresh handshake and ends the previous
/// session, so the newest attachment always wins.
pub(crate) async fn serve(agent: Arc<DaemonAgent>, mut link: WsLink) {
    let failures = Arc::new(AtomicU32::new(0));
    let mut session: Option<Session> = None;
    while let Some(item) = link.rx.recv().await {
        match item {
            LinkItem::Peer(Presence::Connected) => {
                session = Some(Session::start(&agent, link.tx.clone(), failures.clone()));
            }
            LinkItem::Peer(Presence::Waiting | Presence::Gone) => session = None,
            LinkItem::Record(record) => {
                if let Some(current) = &session
                    && current.records.try_send(record).is_err()
                {
                    session = None;
                }
            }
        }
    }
}

struct Session {
    records: mpsc::Sender<Vec<u8>>,
    task: JoinHandle<()>,
}

impl Session {
    fn start(
        agent: &Arc<DaemonAgent>,
        link_tx: mpsc::Sender<Vec<u8>>,
        failures: Arc<AtomicU32>,
    ) -> Self {
        let (records, records_rx) = mpsc::channel(SESSION_QUEUE);
        let task = tokio::spawn(run_session(agent.clone(), link_tx, records_rx, failures));
        Self { records, task }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn penalty(failures: u32) -> Duration {
    (PENALTY_STEP * failures).min(PENALTY_MAX)
}

async fn run_session(
    agent: Arc<DaemonAgent>,
    link_tx: mpsc::Sender<Vec<u8>>,
    mut records: mpsc::Receiver<Vec<u8>>,
    failures: Arc<AtomicU32>,
) {
    // Slows a peer that keeps failing the handshake. Records that arrive in the
    // meantime wait in the queue.
    tokio::time::sleep(penalty(failures.load(Ordering::Relaxed))).await;
    let channel = match handshake(&agent, &link_tx, &mut records).await {
        Ok(channel) => channel,
        Err(why) => {
            failures.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(source = "relay_tunnel", why, "desktop handshake rejected");
            return;
        }
    };
    failures.store(0, Ordering::Relaxed);
    tracing::info!(source = "relay_tunnel", "desktop attached");
    let mux::Mux {
        handle,
        mut inbound,
    } = mux::start(Role::Responder, channel, link_tx, records, agent.mux);
    handle.send_credential(agent.daemon_token.as_bytes().to_vec());
    while let Some(item) = inbound.recv().await {
        match item {
            Inbound::Stream(incoming) => {
                let upstream = agent.upstream;
                tokio::spawn(async move {
                    match TcpStream::connect(upstream).await {
                        Ok(tcp) => bridge(incoming.accept(), tcp).await,
                        Err(_) => incoming.reject(OpenFailure::UpstreamUnreachable),
                    }
                });
            }
            // Only the daemon sends D; a desktop that sends one is ignored.
            Inbound::Credential(_) => {}
        }
    }
}

async fn handshake(
    agent: &DaemonAgent,
    link_tx: &mpsc::Sender<Vec<u8>>,
    records: &mut mpsc::Receiver<Vec<u8>>,
) -> Result<super::secure::SecureChannel, &'static str> {
    let bundle = &agent.bundle;
    let binding = ChannelBinding {
        pairing_id: &bundle.pairing_id,
        room: &bundle.room,
    };
    let mut noise = Handshake::responder(&bundle.secret, &binding).map_err(|_| "setup")?;
    let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, records.recv())
        .await
        .map_err(|_| "timeout")?
        .ok_or("link closed")?;
    noise.read(&first).map_err(|_| "wrong secret or binding")?;
    let reply = noise.write().map_err(|_| "reply")?;
    link_tx.send(reply).await.map_err(|_| "link closed")?;
    noise.finish().map_err(|_| "finish")
}

#[cfg(test)]
mod tests;
