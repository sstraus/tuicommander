use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::types::{PeerStatus, RelayMessage};

/// Per-session slot holding up to two peer senders.
pub struct SessionSlot {
    /// Bounded channel senders for each connected peer.
    peers: Vec<mpsc::Sender<Message>>,
}

/// Application state shared across all handlers.
pub struct AppState {
    pub sessions: DashMap<String, SessionSlot>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
        })
    }
}

/// Maximum peers allowed per session (desktop + mobile).
const MAX_PEERS_PER_SESSION: usize = 2;

/// Bounded channel capacity per peer — backpressure on slow clients.
const PEER_CHANNEL_CAPACITY: usize = 32;

/// Handle a new WebSocket connection for the given session.
pub async fn handle_ws(state: Arc<AppState>, session_id: String, mut socket: WebSocket) {
    // Check capacity before splitting — allows sending close frame on rejection
    {
        let session_full = state
            .sessions
            .get(&session_id)
            .map(|s| s.peers.len() >= MAX_PEERS_PER_SESSION)
            .unwrap_or(false);

        if session_full {
            warn!(session_id, "session full, rejecting peer");
            let _ = socket
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 4001,
                    reason: "session full".into(),
                })))
                .await;
            return;
        }
    }

    let (ws_sender, mut ws_receiver) = socket.split();

    // Create bounded channel for this peer
    let (tx, rx) = mpsc::channel::<Message>(PEER_CHANNEL_CAPACITY);

    // Join the session
    let peer_index = {
        let mut entry = state
            .sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionSlot { peers: Vec::new() });
        let slot = entry.value_mut();
        slot.peers.push(tx.clone());
        slot.peers.len() - 1
    };

    info!(session_id, peer_index, "peer joined");

    // Notify this peer of current state
    let peer_count = state
        .sessions
        .get(&session_id)
        .map(|s| s.peers.len())
        .unwrap_or(0);

    let status = if peer_count == 2 {
        PeerStatus::Connected
    } else {
        PeerStatus::Waiting
    };

    let status_msg = serde_json::to_string(&RelayMessage::Status {
        peer: status.clone(),
    })
    .expect("status serialization is infallible");
    let _ = tx.send(Message::Text(status_msg.into())).await;

    // If we're the second peer, notify the first peer too
    if peer_count == 2 {
        notify_other_peers(&state, &session_id, peer_index, PeerStatus::Connected).await;
    }

    // Spawn writer task: reads from channel, writes to WebSocket
    let writer_handle = tokio::spawn(write_loop(ws_sender, rx));

    // Reader loop: reads from WebSocket, forwards to other peer(s)
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match &msg {
            Message::Binary(_) => {
                // Opaque E2E data — forward to other peers
                forward_to_others(&state, &session_id, peer_index, msg).await;
            }
            Message::Text(text) => {
                // Could be relay:push hint or other plaintext
                // Forward as-is to other peers
                forward_to_others(
                    &state,
                    &session_id,
                    peer_index,
                    Message::Text(text.clone()),
                )
                .await;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {
                // Axum handles ping/pong automatically
            }
        }
    }

    // Peer disconnected — clean up
    writer_handle.abort();
    remove_peer(&state, &session_id, peer_index).await;
    info!(session_id, peer_index, "peer left");
}

/// Forward a message to all peers in the session except the sender.
async fn forward_to_others(
    state: &AppState,
    session_id: &str,
    sender_index: usize,
    msg: Message,
) {
    if let Some(slot) = state.sessions.get(session_id) {
        for (i, peer_tx) in slot.peers.iter().enumerate() {
            if i != sender_index {
                // Drop message if peer is slow (bounded channel)
                let _ = peer_tx.try_send(msg.clone());
            }
        }
    }
}

/// Notify all peers except the given one of a status change.
async fn notify_other_peers(
    state: &AppState,
    session_id: &str,
    except_index: usize,
    status: PeerStatus,
) {
    let msg = serde_json::to_string(&RelayMessage::Status { peer: status })
        .expect("status serialization is infallible");

    if let Some(slot) = state.sessions.get(session_id) {
        for (i, peer_tx) in slot.peers.iter().enumerate() {
            if i != except_index {
                let _ = peer_tx.try_send(Message::Text(msg.clone().into()));
            }
        }
    }
}

/// Remove a peer from the session and notify remaining peers.
async fn remove_peer(state: &AppState, session_id: &str, peer_index: usize) {
    let remaining = {
        let mut entry = match state.sessions.get_mut(session_id) {
            Some(e) => e,
            None => return,
        };
        let slot = entry.value_mut();

        // Remove the peer (shift indices)
        if peer_index < slot.peers.len() {
            slot.peers.remove(peer_index);
        }
        slot.peers.len()
    };

    if remaining == 0 {
        state.sessions.remove(session_id);
    } else {
        // Notify remaining peers
        notify_all_peers(state, session_id, PeerStatus::Disconnected).await;
    }
}

/// Notify all peers in a session of a status change.
async fn notify_all_peers(state: &AppState, session_id: &str, status: PeerStatus) {
    let msg = serde_json::to_string(&RelayMessage::Status { peer: status })
        .expect("status serialization is infallible");

    if let Some(slot) = state.sessions.get(session_id) {
        for peer_tx in slot.peers.iter() {
            let _ = peer_tx.try_send(Message::Text(msg.clone().into()));
        }
    }
}

/// Writer loop: drains the channel and writes to the WebSocket sink.
async fn write_loop(
    mut sender: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<Message>,
) {
    while let Some(msg) = rx.recv().await {
        if sender.send(msg).await.is_err() {
            break;
        }
    }
}
