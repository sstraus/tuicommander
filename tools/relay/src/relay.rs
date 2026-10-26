use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// Bounded channel senders keyed by stable peer ID.
    peers: Vec<(u64, mpsc::Sender<Message>)>,
    /// Monotonic counter for assigning stable peer IDs.
    next_peer_id: u64,
    /// Token hash that owns this session (for stats tracking).
    pub token_hash: Option<String>,
    /// Last activity timestamp — updated on every incoming message.
    pub last_activity: Instant,
}

impl SessionSlot {
    fn contains_peer(&self, peer_id: u64, peer: &mpsc::WeakSender<Message>) -> bool {
        peer.upgrade().is_some_and(|sender| {
            self.peers
                .iter()
                .any(|(id, tx)| *id == peer_id && tx.same_channel(&sender))
        })
    }

    fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

/// Application state shared across all handlers.
pub struct AppState {
    pub sessions: DashMap<String, SessionSlot>,
    /// Serializes room admission and removal so cross-room token limits are atomic.
    pub admission: std::sync::Mutex<()>,
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
    pub timing: RelayTiming,
}

/// Waits the relay applies to slow or silent peers. Fields are public so a
/// test can shrink them; production uses the defaults.
#[derive(Clone, Copy, Debug)]
pub struct RelayTiming {
    /// How long a forward waits for room in a full peer queue before the room
    /// is closed. A terminal burst is absorbed; a stuck peer is not.
    pub peer_send_timeout: Duration,
    /// Interval between WebSocket pings to each peer.
    pub ping_interval: Duration,
    /// A peer that sent no frame (pong included) this long is dropped.
    pub dead_after: Duration,
}

impl Default for RelayTiming {
    fn default() -> Self {
        Self {
            peer_send_timeout: Duration::from_secs(5),
            ping_interval: Duration::from_secs(20),
            dead_after: Duration::from_secs(60),
        }
    }
}

impl AppState {
    /// Create state without database (for tests).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            admission: std::sync::Mutex::new(()),
            db: None,
            token_cache: DashMap::new(),
            vapid: None,
            http_client: reqwest::Client::new(),
            rate_limits: DashMap::new(),
            rate_limit_per_hour: 0,
            max_sessions_per_token: 0,
            timing: RelayTiming::default(),
        })
    }

    /// Create state with database connection.
    pub fn with_db(db: Connection) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            admission: std::sync::Mutex::new(()),
            db: Some(db),
            token_cache: DashMap::new(),
            vapid: None,
            http_client: reqwest::Client::new(),
            rate_limits: DashMap::new(),
            rate_limit_per_hour: 0,
            max_sessions_per_token: 0,
            timing: RelayTiming::default(),
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

        let mut entry = self.rate_limits.entry(ip).or_default();
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

/// Resolve a bearer token: check in-memory cache first, fall back to DB argon2 verification.
/// On cache miss + DB hit, populates the cache for subsequent lookups.
async fn resolve_token(state: &AppState, token: &str) -> Option<String> {
    // Fast path: cache hit
    if let Some(hash) = state.token_cache.get(token).map(|e| e.value().clone()) {
        return Some(hash);
    }

    // DEFERRED (2026-10-16) — public-service exhaustion controls (pre-auth WS
    // quotas and bounded/off-thread Argon2 cache misses) are outside this
    // trusted-relay admission fix; do not expose the prototype publicly.
    // Slow path: iterate DB hashes and verify with argon2
    let conn = state.db.as_ref()?;
    let all_hashes = match crate::db::list_token_hashes(conn).await {
        Ok(hashes) => hashes,
        Err(e) => {
            warn!(error = %e, "failed to list token hashes from DB");
            return None;
        }
    };

    for hash in all_hashes {
        if crate::auth::verify_token(token, &hash) {
            // Populate cache for future lookups
            state.token_cache.insert(token.to_owned(), hash.clone());
            return Some(hash);
        }
    }

    None
}

/// Maximum peers allowed per session (desktop + mobile).
const MAX_PEERS_PER_SESSION: usize = 2;

/// Largest WebSocket message the relay accepts. A tunnel record is 16 KiB plus
/// framing and tag; existing mobile event envelopes fit well under this.
pub const MAX_WS_MESSAGE_BYTES: usize = 1024 * 1024;

/// Bounded channel capacity per peer — backpressure on slow clients.
const PEER_CHANNEL_CAPACITY: usize = 32;

/// Timeout for receiving the first auth message from a connecting peer.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Handle a new WebSocket connection for the given session.
pub async fn handle_ws(state: Arc<AppState>, session_id: String, mut socket: WebSocket) {
    // Authentication is mandatory, including when no database is configured.
    let token = match tokio::time::timeout(AUTH_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => text
            .strip_prefix("Bearer ")
            .filter(|token| !token.is_empty())
            .map(str::to_owned),
        _ => None,
    };
    let token_hash = match token {
        Some(token) => resolve_token(&state, &token).await,
        None => None,
    };
    let Some(token_hash) = token_hash else {
        reject(&mut socket, 4003, "authentication required").await;
        return;
    };

    let (tx, rx) = mpsc::channel::<Message>(PEER_CHANNEL_CAPACITY);
    // No await under this lock. Ownership, capacity, token room count and
    // insertion are one transaction; a second peer does not create a room.
    let admission = {
        let _guard = state.admission.lock().expect("admission lock poisoned");
        if let Some(mut slot) = state.sessions.get_mut(&session_id) {
            if slot.token_hash.as_deref() != Some(token_hash.as_str()) {
                Err((4003, "room belongs to another token"))
            } else if slot.peer_count() >= MAX_PEERS_PER_SESSION {
                Err((4001, "session full"))
            } else {
                let id = slot.next_peer_id;
                slot.next_peer_id += 1;
                slot.peers.push((id, tx.clone()));
                Ok(id)
            }
        } else {
            let room_count = state
                .sessions
                .iter()
                .filter(|entry| entry.token_hash.as_deref() == Some(token_hash.as_str()))
                .count();
            if state.max_sessions_per_token > 0
                && room_count >= state.max_sessions_per_token as usize
            {
                Err((4002, "max sessions per token exceeded"))
            } else {
                state.sessions.insert(
                    session_id.clone(),
                    SessionSlot {
                        peers: vec![(0, tx.clone())],
                        next_peer_id: 1,
                        token_hash: Some(token_hash.clone()),
                        last_activity: Instant::now(),
                    },
                );
                Ok(0)
            }
        }
    };
    let peer = tx.downgrade();
    let peer_id = match admission {
        Ok(id) => id,
        Err((code, reason)) => {
            reject(&mut socket, code, reason).await;
            return;
        }
    };
    let (ws_sender, mut ws_receiver) = socket.split();

    info!(session_id, peer_id, "peer joined");

    // Record session start in DB for stats tracking
    if let Some(conn) = &state.db {
        if let Err(e) = crate::db::start_session(conn, &session_id, &token_hash).await {
            warn!(error = %e, "failed to record session start");
        }
    }

    // Notify this peer of current state
    let peer_count = state
        .sessions
        .get(&session_id)
        .filter(|s| s.contains_peer(peer_id, &peer))
        .map(|s| s.peer_count())
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
        notify_other_peers(&state, &session_id, peer_id, &peer, PeerStatus::Connected).await;
    }

    // Spawn writer task: reads from channel, writes to WebSocket
    // Only the room owns strong senders after setup. Eviction closes the writer
    // channel, and writer completion cancels the reader even on a silent socket.
    drop(tx);
    let mut writer_handle = tokio::spawn(write_loop(ws_sender, rx));

    // Reader loop: reads from WebSocket, forwards to other peer(s). A silent
    // TCP peer would otherwise pin its room until the idle reaper runs.
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + state.timing.ping_interval,
        state.timing.ping_interval,
    );
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_seen = Instant::now();
    loop {
        tokio::select! {
            _ = &mut writer_handle => break,
            _ = ping.tick() => {
                if last_seen.elapsed() > state.timing.dead_after {
                    warn!(session_id, peer_id, "peer silent, dropping");
                    break;
                }
                if let Some(tx) = peer.upgrade() {
                    let _ = tx.try_send(Message::Ping(Vec::new().into()));
                }
            }
            incoming = ws_receiver.next() => {
                let Some(Ok(msg)) = incoming else { break };
                last_seen = Instant::now();
                if matches!(&msg, Message::Close(_)) {
                    break;
                }
                process_message(&state, &session_id, peer_id, &peer, msg).await;
            }
        }
    }

    // Peer disconnected — clean up
    writer_handle.abort();

    // A stale handler cannot remove or account for a replacement room.
    let removed = remove_peer(&state, &session_id, peer_id, &peer).await;
    if let Some(conn) = state.db.as_ref().filter(|_| removed) {
        if let Err(e) = crate::db::end_session(conn, &session_id, 0).await {
            warn!(error = %e, "failed to record session end");
        }
    }

    info!(session_id, peer_id, "peer left");
}

async fn reject(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}

/// If the other peer is offline, send Web Push to all subscriptions for this session's token.
async fn maybe_send_push(
    state: &Arc<AppState>,
    session_id: &str,
    peer_id: u64,
    peer: &mpsc::WeakSender<Message>,
    reason: &str,
    session_name: &str,
) {
    // Resolve ownership and presence together, before any await. A stale handler
    // must never use a recreated room's token for push delivery.
    let token_hash = {
        let Some(slot) = state.sessions.get(session_id) else {
            return;
        };
        if !slot.contains_peer(peer_id, peer) || slot.peer_count() >= 2 {
            return;
        }
        let Some(hash) = slot.token_hash.clone() else {
            return;
        };
        hash
    };
    let (Some(vapid), Some(conn)) = (state.vapid.clone(), state.db.clone()) else {
        return;
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
    peer_id: u64,
    peer: &mpsc::WeakSender<Message>,
    msg: Message,
) {
    // Update activity timestamp
    if let Some(mut slot) = state.sessions.get_mut(session_id) {
        if !slot.contains_peer(peer_id, peer) {
            return;
        }
        slot.last_activity = Instant::now();
    } else {
        return;
    }

    match &msg {
        Message::Binary(_) => {
            // Opaque E2E data — forward to other peers
            forward_to_others(state, session_id, peer_id, peer, msg).await;
        }
        Message::Text(text) => {
            // Check if this is a relay:push hint
            if let Ok(RelayMessage::Push {
                reason,
                session_name,
            }) = serde_json::from_str::<RelayMessage>(text)
            {
                maybe_send_push(state, session_id, peer_id, peer, &reason, &session_name).await;
            }
            // Always forward text to other peers
            forward_to_others(
                state,
                session_id,
                peer_id,
                peer,
                Message::Text(text.clone()),
            )
            .await;
        }
        Message::Close(_) | Message::Ping(_) | Message::Pong(_) => {}
    }
}

/// Forward a message to all peers in the session except the sender.
///
/// A full peer queue ends the whole room. The frames carry an ordered encrypted
/// stream, so dropping one would corrupt it while both sides still looked
/// connected; closing both ends is the only outcome the endpoints can detect.
async fn forward_to_others(
    state: &AppState,
    session_id: &str,
    sender_id: u64,
    peer: &mpsc::WeakSender<Message>,
    msg: Message,
) {
    // Clone the senders so no map guard is held across the awaits below.
    let targets: Vec<mpsc::Sender<Message>> = {
        let Some(slot) = state.sessions.get(session_id) else {
            return;
        };
        if !slot.contains_peer(sender_id, peer) {
            return;
        }
        slot.peers
            .iter()
            .filter(|(id, _)| *id != sender_id)
            .map(|(_, tx)| tx.clone())
            .collect()
    };
    let mut overflowed = false;
    for tx in targets {
        if let Err(mpsc::error::SendTimeoutError::Timeout(_)) = tx
            .send_timeout(msg.clone(), state.timing.peer_send_timeout)
            .await
        {
            overflowed = true;
        }
    }
    if overflowed {
        evict_room(state, session_id, sender_id, peer);
    }
}

/// Drop the room's senders so each writer drains its queue, sends a close
/// frame and ends its handler. Only the room that overflowed is touched.
fn evict_room(
    state: &AppState,
    session_id: &str,
    sender_id: u64,
    peer: &mpsc::WeakSender<Message>,
) {
    let _guard = state.admission.lock().expect("admission lock poisoned");
    let still_ours = state
        .sessions
        .get(session_id)
        .is_some_and(|slot| slot.contains_peer(sender_id, peer));
    if still_ours {
        warn!(session_id, "peer queue full, closing room");
        state.sessions.remove(session_id);
    }
}

/// Notify all peers except the given one of a status change.
async fn notify_other_peers(
    state: &AppState,
    session_id: &str,
    except_id: u64,
    peer: &mpsc::WeakSender<Message>,
    status: PeerStatus,
) {
    let msg = serde_json::to_string(&RelayMessage::Status { peer: status })
        .expect("status serialization is infallible");

    if let Some(slot) = state.sessions.get(session_id) {
        if !slot.contains_peer(except_id, peer) {
            return;
        }
        for (id, peer_tx) in slot.peers.iter() {
            if *id != except_id {
                let _ = peer_tx.try_send(Message::Text(msg.clone().into()));
            }
        }
    }
}

/// Remove only the admitted channel, never a reused numeric ID in another room.
async fn remove_peer(
    state: &AppState,
    session_id: &str,
    peer_id: u64,
    peer: &mpsc::WeakSender<Message>,
) -> bool {
    let _guard = state.admission.lock().expect("admission lock poisoned");
    let Some(mut entry) = state.sessions.get_mut(session_id) else {
        return false;
    };
    let slot = entry.value_mut();
    if !slot.contains_peer(peer_id, peer) {
        return false;
    }
    slot.peers.retain(|(id, _)| *id != peer_id);
    let remaining = slot.peer_count();
    let msg = serde_json::to_string(&RelayMessage::Status {
        peer: PeerStatus::Disconnected,
    })
    .expect("status serialization is infallible");
    for (_, tx) in &slot.peers {
        let _ = tx.try_send(Message::Text(msg.clone().into()));
    }
    drop(entry);
    if remaining == 0 {
        state.sessions.remove(session_id);
    }
    true
}

/// Writer loop: drains the channel and writes to the WebSocket sink.
async fn write_loop(mut sender: SplitSink<WebSocket, Message>, mut rx: mpsc::Receiver<Message>) {
    while let Some(msg) = rx.recv().await {
        if sender.send(msg).await.is_err() {
            return;
        }
    }
    // The room dropped this peer's channel (overflow or idle reap): say so
    // explicitly rather than leaving the socket to look like a network fault.
    let _ = sender
        .send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code: 4004,
            reason: "room closed".into(),
        })))
        .await;
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

            // Check activity and evict under the same locks as admission. No
            // stale candidate list can remove a refreshed or recreated room.
            let _guard = state.admission.lock().expect("admission lock poisoned");
            state.sessions.retain(|session_id, slot| {
                if slot.last_activity.elapsed() <= timeout {
                    return true;
                }
                info!(session_id, "reaping idle session");
                let msg = serde_json::to_string(&RelayMessage::Status {
                    peer: PeerStatus::Timeout,
                })
                .expect("status serialization is infallible");
                for (_, tx) in &slot.peers {
                    let _ = tx.try_send(Message::Text(msg.clone().into()));
                }
                // Dropping the room's senders drains and closes each writer;
                // handle_ws then cancels its reader and checks channel identity.
                false
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches a stale handler acting on reused peer ID 0 while its old channel
    // is still alive during setup or an in-flight operation.
    #[tokio::test]
    async fn stale_channel_cannot_refresh_forward_or_remove_replacement_peer() {
        let state = AppState::new();
        let (old_tx, _old_rx) = mpsc::channel(2);
        let old_peer = old_tx.downgrade();
        let (new_tx, mut new_rx) = mpsc::channel(2);
        let previous_activity = Instant::now() - Duration::from_secs(1);
        state.sessions.insert(
            "reused".into(),
            SessionSlot {
                peers: vec![(0, new_tx)],
                next_peer_id: 1,
                token_hash: Some("replacement-owner".into()),
                last_activity: previous_activity,
            },
        );
        process_message(
            &state,
            "reused",
            0,
            &old_peer,
            Message::Binary(vec![1].into()),
        )
        .await;
        // Forwarding also revalidates after an await, independently of the
        // initial activity check (text push processing can yield in between).
        forward_to_others(
            &state,
            "reused",
            1,
            &old_peer,
            Message::Binary(vec![2].into()),
        )
        .await;
        assert!(!remove_peer(&state, "reused", 0, &old_peer).await);
        let slot = state.sessions.get("reused").unwrap();
        assert_eq!(slot.last_activity, previous_activity);
        assert_eq!(slot.peer_count(), 1);
        assert_eq!(slot.token_hash.as_deref(), Some("replacement-owner"));
        assert!(matches!(
            new_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }
}
