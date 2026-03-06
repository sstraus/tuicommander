use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_rusqlite::Connection;
use tracing::{info, warn};

use crate::push::VapidConfig;
use crate::types::{PeerStatus, RelayMessage};

/// Per-session slot holding up to two peer senders.
pub struct SessionSlot {
    /// Bounded channel senders for each connected peer.
    peers: Vec<mpsc::Sender<Message>>,
    /// Token hash that owns this session (for stats tracking).
    pub token_hash: Option<String>,
    /// Last activity timestamp — updated on every incoming message.
    pub last_activity: Instant,
}

/// Application state shared across all handlers.
pub struct AppState {
    pub sessions: DashMap<String, SessionSlot>,
    /// SQLite connection for stats. None in test mode.
    pub db: Option<Connection>,
    /// Registered token hashes (in-memory cache for fast WS auth verification).
    /// Maps plaintext token → argon2 hash.
    pub token_cache: DashMap<String, String>,
    /// VAPID config for Web Push. None if push is disabled.
    pub vapid: Option<VapidConfig>,
    /// Shared HTTP client for sending push notifications.
    pub http_client: reqwest::Client,
    /// Registration rate limit: sliding window per IP address.
    pub rate_limits: DashMap<std::net::IpAddr, Vec<Instant>>,
    /// Max registrations per IP per hour. 0 = no limit.
    pub rate_limit_per_hour: u32,
    /// Max concurrent sessions per token. 0 = no limit.
    pub max_sessions_per_token: u32,
}

impl AppState {
    /// Create state without database (for tests).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            db: None,
            token_cache: DashMap::new(),
            vapid: None,
            http_client: reqwest::Client::new(),
            rate_limits: DashMap::new(),
            rate_limit_per_hour: 0,
            max_sessions_per_token: 0,
        })
    }

    /// Create state with database connection.
    pub fn with_db(db: Connection) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            db: Some(db),
            token_cache: DashMap::new(),
            vapid: None,
            http_client: reqwest::Client::new(),
            rate_limits: DashMap::new(),
            rate_limit_per_hour: 0,
            max_sessions_per_token: 0,
        })
    }

    /// Check if a registration is allowed for the given IP under rate limit.
    /// Returns true if allowed, false if rate limited.
    pub fn check_rate_limit(&self, ip: std::net::IpAddr) -> bool {
        if self.rate_limit_per_hour == 0 {
            return true;
        }

        let now = Instant::now();
        let window = std::time::Duration::from_secs(3600);

        let mut entry = self.rate_limits.entry(ip).or_insert_with(Vec::new);
        let timestamps = entry.value_mut();

        // Remove expired entries
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() >= self.rate_limit_per_hour as usize {
            return false;
        }

        timestamps.push(now);
        true
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

    // Try to authenticate: read first message, check if it's a Bearer token
    let mut token_hash: Option<String> = None;
    let mut first_msg_to_replay: Option<Message> = None;

    if let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(ref text) = msg {
            if let Some(token) = text.strip_prefix("Bearer ") {
                // Verify token against cache
                if let Some(hash) = state.token_cache.get(token).map(|e| e.value().clone()) {
                    token_hash = Some(hash.clone());

                    // Check max concurrent sessions per token
                    if state.max_sessions_per_token > 0 {
                        let session_count = state
                            .sessions
                            .iter()
                            .filter(|e| e.value().token_hash.as_deref() == Some(&hash))
                            .count();

                        if session_count >= state.max_sessions_per_token as usize {
                            warn!(session_id, "max sessions per token exceeded, rejecting");
                            let _ = socket
                                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                    code: 4002,
                                    reason: "max sessions per token exceeded".into(),
                                })))
                                .await;
                            return;
                        }
                    }
                } else {
                    warn!(session_id, "invalid bearer token, rejecting");
                    let _ = socket
                        .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                            code: 4003,
                            reason: "invalid token".into(),
                        })))
                        .await;
                    return;
                }
            } else {
                // Not a bearer token — treat as regular message
                first_msg_to_replay = Some(msg);
            }
        } else {
            // Not a text message — treat as regular message
            first_msg_to_replay = Some(msg);
        }
    } else {
        // Connection closed before any message
        return;
    }

    let (ws_sender, mut ws_receiver) = socket.split();

    // Create bounded channel for this peer
    let (tx, rx) = mpsc::channel::<Message>(PEER_CHANNEL_CAPACITY);

    // Join the session
    let peer_index = {
        let mut entry = state
            .sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionSlot { peers: Vec::new(), token_hash: None, last_activity: Instant::now() });
        // Set token_hash if authenticated
        if token_hash.is_some() {
            entry.value_mut().token_hash = token_hash;
        }
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

    // If the first message wasn't a bearer token, process it now
    if let Some(msg) = first_msg_to_replay {
        process_message(&state, &session_id, peer_index, msg).await;
    }

    // Reader loop: reads from WebSocket, forwards to other peer(s)
    while let Some(Ok(msg)) = ws_receiver.next().await {
        if matches!(&msg, Message::Close(_)) {
            break;
        }
        process_message(&state, &session_id, peer_index, msg).await;
    }

    // Peer disconnected — clean up
    writer_handle.abort();
    remove_peer(&state, &session_id, peer_index).await;
    info!(session_id, peer_index, "peer left");
}

/// If the other peer is offline, send Web Push to all subscriptions for this session's token.
async fn maybe_send_push(state: &Arc<AppState>, session_id: &str, reason: &str, session_name: &str) {
    // Check if the other peer is connected (session has 2 peers = both online)
    let peer_count = state
        .sessions
        .get(session_id)
        .map(|s| s.peers.len())
        .unwrap_or(0);

    if peer_count >= 2 {
        // Other peer is connected, no push needed
        return;
    }

    let (Some(vapid), Some(conn)) = (state.vapid.clone(), state.db.clone()) else {
        // Push not configured or no DB
        return;
    };

    // Find the token_hash for this session
    let token_hash = match state.sessions.get(session_id).and_then(|s| s.token_hash.clone()) {
        Some(h) => h,
        None => {
            warn!(session_id, "no token_hash for session, cannot send push");
            return;
        }
    };

    // Get all push subscriptions for this token
    let subs = match crate::db::list_push_subs(&conn, &token_hash).await {
        Ok(subs) => subs,
        Err(e) => {
            warn!(error = %e, "failed to list push subscriptions");
            return;
        }
    };

    if subs.is_empty() {
        return;
    }

    info!(
        session_id,
        reason,
        sub_count = subs.len(),
        "sending push notifications (mobile peer offline)"
    );

    let payload = serde_json::to_vec(&serde_json::json!({
        "title": session_name,
        "body": reason,
    }))
    .expect("json serialization is infallible");

    // Clone everything needed before spawning to satisfy 'static requirement
    let client = state.http_client.clone();
    tokio::spawn(async move {
        for sub in &subs {
            match crate::push::send_push(&client, &vapid, sub, &payload).await {
                Ok(false) => {
                    // Endpoint gone — remove subscription
                    let _ = crate::db::delete_push_sub(&conn, &token_hash, &sub.endpoint).await;
                }
                Err(e) => {
                    warn!(endpoint = %sub.endpoint, error = %e, "push send error");
                }
                Ok(true) => {}
            }
        }
    });
}

/// Process a single incoming message: update activity, forward to peers, handle push hints.
async fn process_message(
    state: &Arc<AppState>,
    session_id: &str,
    peer_index: usize,
    msg: Message,
) {
    // Update activity timestamp
    if let Some(mut slot) = state.sessions.get_mut(session_id) {
        slot.last_activity = Instant::now();
    }

    match &msg {
        Message::Binary(_) => {
            // Opaque E2E data — forward to other peers
            forward_to_others(state, session_id, peer_index, msg).await;
        }
        Message::Text(text) => {
            // Check if this is a relay:push hint
            if let Ok(RelayMessage::Push { reason, session_name }) =
                serde_json::from_str::<RelayMessage>(text)
            {
                maybe_send_push(state, session_id, &reason, &session_name).await;
            }
            // Always forward text to other peers
            forward_to_others(state, session_id, peer_index, Message::Text(text.clone())).await;
        }
        Message::Close(_) | Message::Ping(_) | Message::Pong(_) => {}
    }
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

/// Spawn a background task that reaps idle sessions.
///
/// - `timeout`: sessions idle longer than this are cleaned up.
/// - `check_interval`: how often the reaper runs.
pub fn spawn_session_reaper(
    state: Arc<AppState>,
    timeout: std::time::Duration,
    check_interval: std::time::Duration,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(check_interval).await;

            let mut to_remove = Vec::new();
            for entry in state.sessions.iter() {
                if entry.value().last_activity.elapsed() > timeout {
                    to_remove.push(entry.key().clone());
                }
            }

            for session_id in to_remove {
                info!(session_id, "reaping idle session");

                // Send timeout status to all connected peers before dropping
                notify_all_peers(&state, &session_id, PeerStatus::Timeout).await;

                // Remove the session — dropping senders closes the channels,
                // which terminates the writer tasks and disconnects the peers.
                state.sessions.remove(&session_id);
            }
        }
    });
}
