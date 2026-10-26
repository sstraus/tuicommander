//! The desktop end of the relay tunnel: attaches to the daemon's room, proves
//! knowledge of the pairing secret, learns the daemon API token D through the
//! encrypted channel, and exposes the daemon on a loopback port.
//!
//! The rest of the app then treats `http://127.0.0.1:<port>` exactly like the
//! local end of an SSH forward: the same HTTP, SSE and terminal WebSocket
//! contracts, and the same authentication with D as the session token. A
//! session is one attachment. After any loss it is dropped and a new one is
//! started; nothing a lost session had in flight is replayed.
//!
//! Like the SSH forward, the loopback port accepts any local process, but a
//! caller without D cannot authenticate to the daemon behind it.

use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use super::bridge::bridge;
use super::bundle::PairingBundle;
use super::mux::{self, CloseReason, Inbound, MuxConfig, MuxHandle, Role};
use super::secure::{ChannelBinding, Handshake};
use super::ws_link::{self, LinkItem, Presence, WsLink};

/// Why an attachment did not come up, or ended. `code` is the typed reason the
/// connection status carries; `Display` is the sentence for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RelayFailure {
    /// The relay refused or could not be reached (also: wrong relay bearer).
    RelayUnreachable(String),
    /// The room has no daemon in it.
    DaemonOffline,
    /// A daemon is there but did not complete the handshake: wrong pairing
    /// secret, or a pairing the daemon has since replaced.
    PairingRejected,
    /// The attachment was up and the link ended.
    LinkLost,
}

impl RelayFailure {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::RelayUnreachable(_) => "relay_unreachable",
            Self::DaemonOffline => "daemon_offline",
            Self::PairingRejected => "pairing_rejected",
            Self::LinkLost => "link_lost",
        }
    }
}

impl std::fmt::Display for RelayFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RelayUnreachable(why) => write!(f, "relay unreachable: {why}"),
            Self::DaemonOffline => f.write_str("the remote daemon is not connected to the relay"),
            Self::PairingRejected => f.write_str(
                "the remote daemon did not accept this pairing (wrong or replaced pairing secret)",
            ),
            Self::LinkLost => f.write_str("the relay connection to the remote daemon was lost"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub daemon_wait: Duration,
    pub handshake: Duration,
    pub credential: Duration,
    pub mux: MuxConfig,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            daemon_wait: Duration::from_secs(10),
            handshake: Duration::from_secs(10),
            credential: Duration::from_secs(10),
            mux: MuxConfig::default(),
        }
    }
}

pub(crate) struct RelaySession {
    port: u16,
    daemon_token: Zeroizing<String>,
    handle: MuxHandle,
    accept: JoinHandle<()>,
}

impl RelaySession {
    pub(crate) async fn start(bundle: &PairingBundle) -> Result<Self, RelayFailure> {
        let link = ws_link::connect(bundle.relay_url(), &bundle.room, &bundle.relay_token)
            .await
            .map_err(|e| RelayFailure::RelayUnreachable(e.to_string()))?;
        Self::attach(link, bundle, Timing::default()).await
    }

    pub(crate) async fn attach(
        mut link: WsLink,
        bundle: &PairingBundle,
        timing: Timing,
    ) -> Result<Self, RelayFailure> {
        wait_for_daemon(&mut link, timing.daemon_wait).await?;
        let channel = handshake(&mut link, bundle, timing.handshake).await?;
        let WsLink { tx, rx } = link;
        let records = records_until_peer_leaves(rx);
        let mux::Mux {
            handle,
            mut inbound,
        } = mux::start(Role::Initiator, channel, tx, records, timing.mux);
        let daemon_token = tokio::time::timeout(timing.credential, async {
            loop {
                match inbound.recv().await {
                    Some(Inbound::Credential(bytes)) => return Some(bytes),
                    // The daemon never opens streams.
                    Some(Inbound::Stream(rejected)) => {
                        rejected.reject(super::frames::OpenFailure::TooManyStreams)
                    }
                    None => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(Zeroizing::new);
        let Some(daemon_token) = daemon_token else {
            handle.shutdown();
            return Err(RelayFailure::PairingRejected);
        };
        drop(inbound);

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| RelayFailure::RelayUnreachable(format!("loopback listener: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| RelayFailure::RelayUnreachable(format!("loopback listener: {e}")))?
            .port();
        let opener = handle.clone();
        let accept = tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let opener = opener.clone();
                tokio::spawn(async move {
                    if let Ok(stream) = opener.open().await {
                        bridge(stream, tcp).await;
                    }
                });
            }
        });
        Ok(Self {
            port,
            daemon_token,
            handle,
            accept,
        })
    }

    pub(crate) fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub(crate) fn daemon_token(&self) -> &str {
        &self.daemon_token
    }

    /// Resolves when this attachment has ended, with the reason a person sees.
    pub(crate) fn lost(&self) -> impl std::future::Future<Output = RelayFailure> + Send + 'static {
        let mut handle = self.handle.clone();
        async move {
            let _: CloseReason = handle.closed().await;
            RelayFailure::LinkLost
        }
    }
}

impl Drop for RelaySession {
    fn drop(&mut self) {
        self.accept.abort();
        self.handle.shutdown();
    }
}

/// The relay announces who is in the room; a handshake sent to an empty room
/// goes nowhere, so wait for the daemon to be there.
async fn wait_for_daemon(link: &mut WsLink, within: Duration) -> Result<(), RelayFailure> {
    let wait = async {
        loop {
            match link.rx.recv().await {
                Some(LinkItem::Peer(Presence::Connected)) => return Ok(()),
                Some(_) => {}
                None => return Err(RelayFailure::RelayUnreachable("closed by the relay".into())),
            }
        }
    };
    tokio::time::timeout(within, wait)
        .await
        .map_err(|_| RelayFailure::DaemonOffline)?
}

async fn handshake(
    link: &mut WsLink,
    bundle: &PairingBundle,
    within: Duration,
) -> Result<super::secure::SecureChannel, RelayFailure> {
    let binding = ChannelBinding {
        pairing_id: &bundle.pairing_id,
        room: &bundle.room,
    };
    let mut noise = Handshake::initiator(&bundle.secret, &binding)
        .map_err(|_| RelayFailure::PairingRejected)?;
    let first = noise.write().map_err(|_| RelayFailure::PairingRejected)?;
    link.tx
        .send(first)
        .await
        .map_err(|_| RelayFailure::LinkLost)?;
    let reply = tokio::time::timeout(within, async {
        loop {
            match link.rx.recv().await {
                Some(LinkItem::Record(record)) => return Ok(record),
                Some(LinkItem::Peer(Presence::Waiting | Presence::Gone)) => {
                    return Err(RelayFailure::DaemonOffline);
                }
                Some(LinkItem::Peer(Presence::Connected)) => {}
                None => return Err(RelayFailure::LinkLost),
            }
        }
    })
    .await
    .map_err(|_| RelayFailure::PairingRejected)??;
    noise
        .read(&reply)
        .map_err(|_| RelayFailure::PairingRejected)?;
    noise.finish().map_err(|_| RelayFailure::PairingRejected)
}

/// Feed the multiplexer records only. When the daemon leaves the room the
/// feed ends, so the session ends at once instead of waiting for the heartbeat.
fn records_until_peer_leaves(mut rx: mpsc::Receiver<LinkItem>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, records) = mpsc::channel(32);
    tokio::spawn(async move {
        while let Some(item) = rx.recv().await {
            match item {
                LinkItem::Record(record) => {
                    if tx.send(record).await.is_err() {
                        return;
                    }
                }
                LinkItem::Peer(Presence::Waiting | Presence::Gone) => return,
                LinkItem::Peer(Presence::Connected) => {}
            }
        }
    });
    records
}

#[cfg(test)]
mod tests;
