//! Authenticated star-topology mail. The desktop opens the only duplex link;
//! daemons keep local delivery independent of that link. No process-control
//! actions cross this protocol.

#[cfg(test)]
use super::{build_remote_router, mcp_transport, remote_mcp_sessions};

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::AppState;

/// Bound outstanding calls and frame size independently of the PTY buffers.
const MAX_CALLS: usize = 64;
// One 64 KiB message may expand sixfold when JSON escapes control bytes.
const MAX_FRAME: usize = 512 * 1024;
/// Three missed 15-second pongs mean the authenticated connection is gone.
const HEARTBEAT: Duration = Duration::from_secs(15);
const LINK_IDLE: Duration = Duration::from_secs(45);
/// Network give-up budget, separate from native agent wait's own deadline.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub(crate) struct RemoteMail {
    connections: DashMap<String, Arc<Link>>,
    hub: Mutex<Option<(String, Arc<Link>)>>,
    // Role transitions are short; network handshakes serialize only per host.
    role_lock: Mutex<()>,
    connect_locks: Mutex<HashMap<String, Weak<HostLock>>>,
    forwarded_history: Mutex<ForwardedHistory>,
    own_host: Mutex<Option<String>>,
    notice_notify: Arc<tokio::sync::Notify>,
    notice_started: AtomicBool,
    supervisors: DashMap<String, tokio::task::AbortHandle>,
}

const FORWARDED_WINDOW: usize = 100;
const MAX_SHADOWS: usize = 1024;

/// Recipient retirement releases its ring; inbox reads preserve replay history.
#[derive(Default)]
struct ForwardedHistory {
    recipients: HashMap<String, VecDeque<(String, [u8; 32])>>,
}

struct HostLock {
    mutex: tokio::sync::Mutex<()>,
    generation: AtomicU64,
}

/// Pure replay bookkeeping is also exercised directly by the critic tests.
#[cfg(test)]
pub(super) fn record_forwarded(
    state: &AppState,
    recipient: &str,
    message: &crate::state::AgentMessage,
) -> Result<bool, String> {
    remember_forwarded(
        &mut state.remote_mail.forwarded_history.lock(),
        recipient,
        message,
    )
}

/// Registration, replay recording and enqueue share the retirement lock. A
/// concurrent unregister cannot leave replay state for an unregistered peer.
pub(super) fn enqueue_forwarded(
    state: &AppState,
    recipient: &str,
    message: crate::state::AgentMessage,
) -> Result<Option<u64>, String> {
    let mut history = state.remote_mail.forwarded_history.lock();
    if !state.peer_agents.contains_key(recipient) {
        return Err("Forwarded recipient is no longer registered".into());
    }
    if !remember_forwarded(&mut history, recipient, &message)? {
        return Ok(None);
    }
    Ok(Some(state.push_agent_inbox(recipient, message)))
}

/// A retry beyond the recipient's last 100 IDs may be delivered again.
fn remember_forwarded(
    history: &mut ForwardedHistory,
    recipient: &str,
    message: &crate::state::AgentMessage,
) -> Result<bool, String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update((message.from_tuic_session.len() as u64).to_le_bytes());
    hasher.update(message.from_tuic_session.as_bytes());
    hasher.update(message.content.as_bytes());
    let fingerprint: [u8; 32] = hasher.finalize().into();
    let ring = history.recipients.entry(recipient.to_string()).or_default();
    if let Some((_, previous)) = ring.iter().find(|(id, _)| id == &message.id) {
        return if *previous == fingerprint {
            Ok(false)
        } else {
            Err("Forwarded message identity collision".into())
        };
    }
    if ring.len() == FORWARDED_WINDOW {
        ring.pop_front();
    }
    ring.push_back((message.id.clone(), fingerprint));
    Ok(true)
}

/// Every peer retirement uses this path so replay history cannot outlive the
/// registered recipient. The history lock serializes removal with recording.
pub(crate) fn unregister_peer(state: &AppState, recipient: &str) {
    let mut history = state.remote_mail.forwarded_history.lock();
    state.peer_agents.remove(recipient);
    history.recipients.remove(recipient);
}

fn host_lock(state: &AppState, id: &str) -> Arc<HostLock> {
    let mut locks = state.remote_mail.connect_locks.lock();
    locks.retain(|_, lock| lock.strong_count() > 0);
    let slot = locks.entry(id.to_string()).or_default();
    if let Some(lock) = slot.upgrade() {
        lock
    } else {
        let lock = Arc::new(HostLock {
            mutex: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
        });
        *slot = Arc::downgrade(&lock);
        lock
    }
}

struct Link {
    outbound: mpsc::Sender<String>,
    pending: DashMap<String, oneshot::Sender<Value>>,
    slots: tokio::sync::Semaphore,
    shutdown: tokio::sync::Notify,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Sender {
    pub host: String,
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Frame {
    Call {
        id: String,
        sender: Option<Sender>,
        arguments: Value,
        message_id: Option<String>,
    },
    Reply {
        id: String,
        result: Value,
    },
}

#[derive(Deserialize)]
pub(super) struct PeerQuery {
    connection_id: String,
}

fn error(connection: &str, detail: impl std::fmt::Display) -> Value {
    json!({"error": format!("Remote connection '{connection}': {detail}"), "connection_id": connection})
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 128
        && host != "local"
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

fn allowed(args: &Value) -> bool {
    matches!(
        args["action"].as_str(),
        Some("register" | "list_peers" | "send" | "inbox" | "wait")
    )
}

impl Link {
    async fn call(&self, sender: Option<Sender>, arguments: Value) -> Value {
        self.call_with_id(sender, arguments, None).await
    }

    async fn call_with_id(
        &self,
        sender: Option<Sender>,
        arguments: Value,
        message_id: Option<String>,
    ) -> Value {
        let Ok(_slot) = self.slots.try_acquire() else {
            return json!({"error": "Remote mail link has too many outstanding calls"});
        };
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id.clone(), tx);
        let frame = Frame::Call {
            id: id.clone(),
            sender,
            arguments: arguments.clone(),
            message_id,
        };
        let budget = if arguments["action"] == "wait" {
            Duration::from_millis(
                arguments["timeout_ms"]
                    .as_u64()
                    .unwrap_or(60_000)
                    .min(300_000)
                    + 20_000,
            )
        } else {
            CALL_TIMEOUT
        };
        let result = tokio::time::timeout(budget, async {
            let text = serde_json::to_string(&frame).map_err(|e| e.to_string())?;
            if text.len() > MAX_FRAME {
                return Err("Remote mail frame exceeds size limit".into());
            }
            self.outbound
                .send(text)
                .await
                .map_err(|_| "Remote mail link is disconnected".to_string())?;
            rx.await
                .map_err(|_| "Remote mail link closed before acknowledgement".to_string())
        })
        .await;
        self.pending.remove(&id);
        match result {
            Ok(Ok(value)) => value,
            Ok(Err(detail)) => json!({"error": detail}),
            Err(_) => {
                json!({"error": "Remote mail acknowledgement timed out; delivery is uncertain, do not resend blindly"})
            }
        }
    }
}

/// Require the real daemon token even on loopback or with LAN auth bypass.
pub(super) async fn endpoint(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PeerQuery>,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Response {
    let token = state.session_token.read().clone();
    if token.is_empty() || !super::auth::has_valid_token_query(&uri, &token) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": "Peer mail requires the daemon connection token"})),
        )
            .into_response();
    }
    if !valid_host(&query.connection_id) {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "Invalid peer connection qualifier"})),
        )
            .into_response();
    }
    if state
        .remote
        .snapshot()
        .iter()
        .any(|connection| connection.base_url.is_some())
    {
        return (
            StatusCode::CONFLICT,
            axum::Json(json!({"error":"A mail hub cannot also become a daemon spoke"})),
        )
            .into_response();
    }
    if state.remote_mail.hub.lock().is_some() {
        return (
            StatusCode::CONFLICT,
            axum::Json(json!({"error": "This daemon already has a mail hub"})),
        )
            .into_response();
    }
    ws.max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| async move {
            let (tx, rx) = mpsc::channel(MAX_CALLS);
            let link = Arc::new(Link {
                outbound: tx,
                pending: DashMap::new(),
                slots: tokio::sync::Semaphore::new(MAX_CALLS),
                shutdown: tokio::sync::Notify::new(),
            });
            // Recheck after upgrade: concurrent authenticated upgrades cannot replace
            // a live hub and strand its outstanding calls.
            {
                let _role_guard = state.remote_mail.role_lock.lock();
                if state
                    .remote
                    .snapshot()
                    .iter()
                    .any(|connection| connection.base_url.is_some())
                {
                    return;
                }
                let mut hub = state.remote_mail.hub.lock();
                if hub.is_some() {
                    return;
                }
                *hub = Some((query.connection_id.clone(), link.clone()));
                *state.remote_mail.own_host.lock() = Some(query.connection_id.clone());
            }
            start_notices(&state);
            state.remote_mail.notice_notify.notify_one();
            drive(
                socket,
                state.clone(),
                link.clone(),
                rx,
                Role::Daemon(query.connection_id),
            )
            .await;
            let mut hub = state.remote_mail.hub.lock();
            if hub
                .as_ref()
                .is_some_and(|(_, current)| Arc::ptr_eq(current, &link))
            {
                *hub = None;
            }
            drop(hub);
            cleanup_shadows(&state, None);
        })
        .into_response()
}

enum Role {
    Hub(String),
    Daemon(String),
}

trait MailFrame: Sized {
    fn text(value: String) -> Self;
    fn ping() -> Self;
    fn payload(&self) -> Option<&str>;
    fn closed(&self) -> bool;
}

impl MailFrame for Message {
    fn text(value: String) -> Self {
        Self::Text(value.into())
    }
    fn ping() -> Self {
        Self::Ping(Vec::new().into())
    }
    fn payload(&self) -> Option<&str> {
        if let Self::Text(text) = self {
            Some(text.as_str())
        } else {
            None
        }
    }
    fn closed(&self) -> bool {
        matches!(self, Self::Close(_))
    }
}

impl MailFrame for tokio_tungstenite::tungstenite::Message {
    fn text(value: String) -> Self {
        Self::Text(value.into())
    }
    fn ping() -> Self {
        Self::Ping(Vec::new().into())
    }
    fn payload(&self) -> Option<&str> {
        if let Self::Text(text) = self {
            Some(text.as_str())
        } else {
            None
        }
    }
    fn closed(&self) -> bool {
        matches!(self, Self::Close(_))
    }
}

async fn drive<S, M, E>(
    mut socket: S,
    state: Arc<AppState>,
    link: Arc<Link>,
    mut rx: mpsc::Receiver<String>,
    role: Role,
) where
    S: Stream<Item = Result<M, E>> + Sink<M> + Unpin,
    M: MailFrame,
    E: std::fmt::Display,
{
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    let mut last_frame = tokio::time::Instant::now();
    let mut calls = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            received = socket.next() => {
                let Some(Ok(message)) = received else { break; };
                last_frame = tokio::time::Instant::now();
                if message.closed() { break; }
                let Some(text) = message.payload() else { continue; };
                if text.len() > MAX_FRAME { break; }
                let frame = match serde_json::from_str::<Frame>(text) {
                    Ok(frame) => frame,
                    Err(_) => break,
                };
                match frame {
                    Frame::Reply { id, result } => {
                        if let Some((_, response)) = link.pending.remove(&id) { let _ = response.send(result); }
                    }
                    Frame::Call { id, sender, arguments, message_id } => {
                        if calls.len() >= MAX_CALLS { break; }
                        let request = process(state.clone(), &role, sender, arguments, message_id);
                        let tx = link.outbound.clone();
                        calls.spawn(async move {
                            let result = request.await;
                            match serde_json::to_string(&Frame::Reply { id: id.clone(), result }) {
                                Ok(text) if text.len() <= MAX_FRAME => { let _ = tx.send(text).await; }
                                _ => {
                                    let fallback = Frame::Reply { id, result: json!({"error":"Peer reply exceeds 512 KiB; read the inbox with a smaller limit"}) };
                                    if let Ok(text) = serde_json::to_string(&fallback) { let _ = tx.send(text).await; }
                                }
                            }
                        });
                    }
                }
            }
            outbound = rx.recv() => {
                let Some(text) = outbound else { break; };
                if !matches!(tokio::time::timeout(HEARTBEAT, socket.send(M::text(text))).await, Ok(Ok(()))) { break; }
            }
            _ = heartbeat.tick() => {
                if last_frame.elapsed() >= LINK_IDLE || !matches!(tokio::time::timeout(HEARTBEAT, socket.send(M::ping())).await, Ok(Ok(()))) { break; }
            }
            _ = calls.join_next(), if !calls.is_empty() => {}
            _ = link.shutdown.notified() => break,
        }
    }
    calls.abort_all();
    link.pending.clear();
    tracing::info!(source = "remote_mail", "Peer mail link closed");
}

// The boxed future breaks the duplex dispatch/connect task's recursive Send
// type while keeping each request owned by its link's JoinSet.
fn process(
    state: Arc<AppState>,
    role: &Role,
    mut sender: Option<Sender>,
    arguments: Value,
    message_id: Option<String>,
) -> futures_util::future::BoxFuture<'static, Value> {
    let (is_hub, host) = match role {
        Role::Hub(id) => (true, id.clone()),
        Role::Daemon(id) => (false, id.clone()),
    };
    Box::pin(async move {
        if !allowed(&arguments) {
            return json!({"error": "Peer mail endpoint permits register/list_peers/send/inbox/wait only"});
        }
        if message_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 160)
        {
            return json!({"error":"Invalid forwarded message identity"});
        }
        if let Some(sender) = sender.as_mut() {
            if sender.id.is_empty()
                || sender.id.len() > 128
                || sender.id.contains('/')
                || sender.name.len() > 256
            {
                return json!({"error": "Invalid peer sender identity"});
            }
            if is_hub {
                // The connection, never the remote body, owns sender provenance.
                sender.host = host.clone();
            } else if sender.host == host || (sender.host != "local" && !valid_host(&sender.host)) {
                return json!({"error": "The hub cannot impersonate a daemon-local peer"});
            }
        }
        if is_hub {
            // A daemon may send as its own peer or discover the hub's directory;
            // it may not wait/read/register as a peer on another host.
            match arguments["action"].as_str() {
                Some("send") => route_send(&state, sender, arguments, message_id).await,
                Some("list_peers") => directory(&state, arguments).await,
                _ => json!({"error": "Daemon-to-hub calls permit send/list_peers only"}),
            }
        } else {
            native_call(&state, sender, arguments, message_id).await
        }
    })
}

async fn connection(state: &Arc<AppState>, id: &str) -> Result<Arc<Link>, Value> {
    // Weak locks disappear after the handshake callers finish, so unknown
    // host probes cannot accumulate permanent lock entries.
    let host_lock = host_lock(state, id);
    let _guard = host_lock.mutex.lock().await;
    let generation = {
        let _role_guard = state.remote_mail.role_lock.lock();
        if state.remote_mail.own_host.lock().is_some() {
            return Err(error(
                id,
                "a daemon spoke routes remote mail through its desktop hub",
            ));
        }
        host_lock.generation.load(Ordering::Acquire)
    };
    if let Some(link) = state.remote_mail.connections.get(id)
        && !link.outbound.is_closed()
    {
        return Ok(link.clone());
    }
    let base = state
        .remote
        .base_url(id)
        .ok_or_else(|| error(id, "connection is unavailable"))?;
    let token = state
        .remote
        .token(id)
        .ok_or_else(|| error(id, "connection has no authenticated daemon token"))?;
    let mut url = reqwest::Url::parse(&base).map_err(|e| error(id, e))?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme)
        .map_err(|_| error(id, "unsupported connection URL scheme"))?;
    url.set_path("/mcp/peer");
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("token", &token)
        .append_pair("connection_id", id);
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME))
        .max_frame_size(Some(MAX_FRAME));
    let (socket, _) = match tokio::time::timeout(
        CALL_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(url.as_str(), Some(config), false),
    )
    .await
    {
        Ok(Ok(connected)) => connected,
        Ok(Err(_)) => {
            return Err(error(
                id,
                "authenticated peer mail link could not open; the daemon must support /mcp/peer",
            ));
        }
        Err(_) => return Err(error(id, "peer mail link handshake timed out")),
    };
    let (tx, rx) = mpsc::channel(MAX_CALLS);
    let link = Arc::new(Link {
        outbound: tx,
        pending: DashMap::new(),
        slots: tokio::sync::Semaphore::new(MAX_CALLS),
        shutdown: tokio::sync::Notify::new(),
    });
    {
        let _role_guard = state.remote_mail.role_lock.lock();
        if host_lock.generation.load(Ordering::Acquire) != generation
            || state.remote_mail.own_host.lock().is_some()
            || state.remote.base_url(id).as_deref() != Some(base.as_str())
            || state.remote.token(id).as_deref() != Some(token.as_str())
        {
            return Err(error(id, "connection changed during peer handshake"));
        }
        state
            .remote_mail
            .connections
            .insert(id.to_string(), link.clone());
    }
    start_notices(state);
    state.remote_mail.notice_notify.notify_one();
    let task_state = state.clone();
    let task_link = link.clone();
    let id = id.to_string();
    let task_host_lock = host_lock.clone();
    tokio::spawn(async move {
        drive(
            socket,
            task_state.clone(),
            task_link.clone(),
            rx,
            Role::Hub(id.clone()),
        )
        .await;
        let _guard = task_host_lock.mutex.lock().await;
        let removed = task_state
            .remote_mail
            .connections
            .remove_if(&id, |_, current| Arc::ptr_eq(current, &task_link))
            .is_some();
        // DEFERRED (2026-10-03): a late old-link frame can recreate a shadow
        // after disconnect; a replacement link skips this cleanup. No
        // deterministic reproduction yet; keep link-generation behavior unchanged.
        if removed || !task_state.remote_mail.connections.contains_key(&id) {
            cleanup_shadows(&task_state, Some(&id));
        }
    });
    Ok(link)
}

fn qualify_rows(value: &mut Value, host: &str) {
    if let Some(rows) = value["peers"].as_array_mut() {
        rows.retain(|row| {
            row["tuic_session"]
                .as_str()
                .is_some_and(|id| !id.contains('/'))
        });
        for row in rows {
            if let Some(id) = row["tuic_session"].as_str() {
                row["address"] = json!(format!("{host}/{id}"));
                row["connection_id"] = json!(host);
            }
        }
    }
}

async fn directory(state: &Arc<AppState>, args: Value) -> Value {
    let mut local = super::mcp_transport::local_peer_call(state, &args, None).await;
    qualify_rows(&mut local, "local");
    let mut peers = local["peers"].as_array().cloned().unwrap_or_default();
    let ids: Vec<_> = if let Some(id) = args["connection_id"].as_str() {
        vec![id.to_string()]
    } else {
        state
            .remote
            .snapshot()
            .into_iter()
            .map(|status| status.id)
            .collect()
    };
    let mut failures = Vec::new();
    if args["connection_id"] == "local" {
        return json!({"peers": peers});
    }
    if args.get("connection_id").is_some() {
        peers.clear();
    }
    for id in ids {
        let link = match connection(state, &id).await {
            Ok(link) => link,
            Err(detail) => {
                failures.push(detail);
                continue;
            }
        };
        let mut remote = link
            .call(
                None,
                json!({"action":"list_peers", "path":args.get("path")}),
            )
            .await;
        qualify_rows(&mut remote, &id);
        if let Some(rows) = remote["peers"].as_array() {
            peers.extend(rows.iter().cloned());
        } else {
            failures.push(error(&id, remote));
        }
    }
    if args.get("connection_id").is_some() && !failures.is_empty() {
        return failures.remove(0);
    }
    let mut result = json!({"peers":peers});
    if !failures.is_empty() {
        result["connection_errors"] = json!(failures);
    }
    result
}

struct SenderBinding {
    state: Arc<AppState>,
    sid: Option<String>,
}

impl Drop for SenderBinding {
    fn drop(&mut self) {
        if let Some(sid) = self.sid.as_deref() {
            self.state.mcp.to_session.remove(sid);
        }
    }
}

async fn native_call(
    state: &Arc<AppState>,
    sender: Option<Sender>,
    mut args: Value,
    message_id: Option<String>,
) -> Value {
    let sid = if let Some(sender) = sender {
        let identity = format!("{}/{}", sender.host, sender.id);
        let sid = format!("remote-mail:{}:{identity}", uuid::Uuid::new_v4());
        let _role_guard = state.remote_mail.role_lock.lock();
        if state.peer_agents.len() >= MAX_SHADOWS && !state.peer_agents.contains_key(&identity) {
            return json!({"error":"Too many remote peer identities"});
        }
        // Qualified shadow peers can never bind or impersonate a local PTY.
        state
            .peer_agents
            .entry(identity.clone())
            .or_insert_with(|| crate::state::PeerAgent {
                tuic_session: identity.clone(),
                mcp_session_id: sid.clone(),
                name: sender.name,
                project: None,
                registered_at: 0,
            });
        state.mcp.to_session.insert(sid.clone(), identity);
        Some(sid)
    } else {
        None
    };
    let binding = SenderBinding {
        state: state.clone(),
        sid,
    };
    let sid = binding.sid.as_deref();
    if args["action"] == "register" {
        let identity = sid.and_then(|sid| state.mcp.to_session.get(sid).map(|p| p.value().clone()));
        if let Some(sid) = sid {
            state.mcp.to_session.remove(sid);
        }
        return json!({"tuic_session":identity});
    }
    if args["action"] != "list_peers" && sid.is_none() {
        return json!({"error":"Peer mail requires a bound sender identity"});
    }
    if let Some(object) = args.as_object_mut() {
        object.remove("connection_id");
    }
    let result =
        super::mcp_transport::local_peer_call_with_message_id(state, &args, sid, message_id).await;
    if let Some(sid) = sid {
        state.mcp.to_session.remove(sid);
    }
    result
}

async fn route_send(
    state: &Arc<AppState>,
    sender: Option<Sender>,
    mut args: Value,
    message_id: Option<String>,
) -> Value {
    let Some(sender) = sender else {
        return json!({"error":"Register before sending cross-host mail"});
    };
    let Some(address) = args["to"].as_str() else {
        return json!({"error":"Peer send requires to"});
    };
    let Some((host, recipient)) = address.split_once('/') else {
        return json!({"error":"Cross-host mail requires connection-qualified recipient: connection/id"});
    };
    if recipient.is_empty() || recipient.contains('/') || (host != "local" && !valid_host(host)) {
        return json!({"error":"Invalid connection-qualified recipient"});
    }
    let host = host.to_string();
    args["to"] = json!(recipient);
    let mut result = if host == "local" {
        native_call(state, Some(sender), args, message_id).await
    } else {
        match connection(state, &host).await {
            Ok(link) => link.call_with_id(Some(sender), args, message_id).await,
            Err(detail) => return detail,
        }
    };
    result["connection_id"] = json!(host);
    result
}

/// Called only after the public MCP dispatcher enforces its loopback boundary.
pub(super) async fn dispatch(
    state: &Arc<AppState>,
    args: &Value,
    sid: Option<&str>,
) -> Option<Value> {
    let hub = state.remote_mail.hub.lock().clone();
    let own_host = state.remote_mail.own_host.lock().clone();
    let action = args["action"].as_str()?;
    if action == "list_peers" {
        return Some(if let Some((_, link)) = hub {
            link.call(None, args.clone()).await
        } else if let Some(host) = own_host {
            if args["connection_id"]
                .as_str()
                .is_some_and(|requested| requested != host)
            {
                return Some(error(
                    &host,
                    "mail hub is disconnected; only daemon-local peers are available",
                ));
            }
            let mut rows = super::mcp_transport::local_peer_call(state, args, sid).await;
            qualify_rows(&mut rows, &host);
            rows
        } else {
            directory(state, args.clone()).await
        });
    }
    if action != "send" {
        return None;
    }
    let to = args["to"].as_str()?;
    let qualified = if let Some(host) = args["connection_id"].as_str() {
        if let Some((qualified_host, _)) = to.split_once('/') {
            if qualified_host != host {
                return Some(json!({"error":"connection_id conflicts with qualified recipient"}));
            }
            to.to_string()
        } else {
            format!("{host}/{to}")
        }
    } else if to.contains('/') {
        to.to_string()
    } else {
        return None;
    };
    let identity = sid.and_then(|sid| state.mcp.to_session.get(sid).map(|id| id.value().clone()));
    let sender = identity.and_then(|identity| {
        state.peer_agents.get(&identity).map(|peer| Sender {
            host: own_host.clone().unwrap_or_else(|| "local".into()),
            id: identity,
            name: peer.name.clone(),
        })
    });
    let mut args = args.clone();
    args["to"] = json!(qualified);
    if let Some(own_host) = own_host {
        if let Some(target) = qualified.strip_prefix(&format!("{own_host}/")) {
            args["to"] = json!(target);
            return Some(super::mcp_transport::local_peer_call(state, &args, sid).await);
        }
        Some(if let Some((_, link)) = hub {
            link.call(sender, args).await
        } else {
            error(
                &own_host,
                "mail hub is disconnected; intra-host mail remains available",
            )
        })
    } else {
        Some(route_send(state, sender, args, None).await)
    }
}

fn cleanup_shadows(state: &AppState, host: Option<&str>) {
    let identities: Vec<_> = state
        .peer_agents
        .iter()
        .filter(|peer| {
            peer.value().mcp_session_id.starts_with("remote-mail:")
                && host.is_none_or(|host| peer.key().starts_with(&format!("{host}/")))
        })
        .map(|peer| peer.key().clone())
        .collect();
    for identity in identities {
        unregister_peer(state, &identity);
        // Do not drop retained cross-host lifecycle mail: it is the outbox
        // until the owning machine acknowledges its durable inbox copy.
    }
}

/// Retire the mail link when its configured connection is disconnected.
pub(crate) fn disconnect(state: &Arc<AppState>, host: &str) {
    let gate = state
        .remote_mail
        .connect_locks
        .lock()
        .get(host)
        .and_then(Weak::upgrade);
    let _role_guard = state.remote_mail.role_lock.lock();
    if let Some(gate) = gate {
        gate.generation.fetch_add(1, Ordering::AcqRel);
    }
    if let Some((_, supervisor)) = state.remote_mail.supervisors.remove(host) {
        supervisor.abort();
    }
    if let Some((_, link)) = state.remote_mail.connections.remove(host) {
        link.shutdown.notify_one();
    }
    // Retirement does not await the handshake lock. The role lock serializes
    // it with publication and shadow creation, so a later generation cannot
    // cancel cleanup or have its fresh shadows removed by a deferred task.
    cleanup_shadows(state, Some(host));
}

/// Open the reverse path when the configured connection becomes ready, so a
/// daemon can mail the hub before the desktop makes its first mail call.
pub(crate) fn connect_configured(state: &Arc<AppState>, host: String) {
    {
        let _role_guard = state.remote_mail.role_lock.lock();
        if let Some(gate) = state
            .remote_mail
            .connect_locks
            .lock()
            .get(&host)
            .and_then(Weak::upgrade)
        {
            gate.generation.fetch_add(1, Ordering::AcqRel);
        }
    }
    if let Some((_, previous)) = state.remote_mail.supervisors.remove(&host) {
        previous.abort();
    }
    let task_state = state.clone();
    let task_host = host.clone();
    let handle = tokio::spawn(async move {
        let mut warned = false;
        while task_state.remote.base_url(&task_host).is_some() {
            match connection(&task_state, &task_host).await {
                Ok(link) => {
                    warned = false;
                    link.outbound.closed().await;
                }
                Err(detail) if !warned => {
                    warned = true;
                    tracing::warn!(
                        source = "remote_mail",
                        connection = task_host,
                        "Peer mail unavailable: {detail}"
                    );
                }
                Err(_) => {}
            }
            // Retry the existing connection only; this never deploys or starts
            // a daemon and never substitutes SSH for the configured transport.
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
    state
        .remote_mail
        .supervisors
        .insert(host, handle.abort_handle());
}

/// Lifecycle producers already write one durable bounded inbox. Qualified
/// recipients use that same FIFO as an outbox; reconnect resumes delivery.
pub(crate) fn notice_stored(state: &AppState, recipient: &str) {
    if recipient.contains('/') {
        state.remote_mail.notice_notify.notify_one();
    }
}

fn start_notices(state: &Arc<AppState>) {
    if state
        .remote_mail
        .notice_started
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let weak = Arc::downgrade(state);
    let notify = state.remote_mail.notice_notify.clone();
    tokio::spawn(async move {
        loop {
            notify.notified().await;
            let Some(state) = weak.upgrade() else {
                break;
            };
            let messages: Vec<_> = state
                .agent_inbox
                .iter()
                .filter(|entry| entry.key().contains('/'))
                .flat_map(|entry| {
                    entry
                        .value()
                        .iter()
                        .cloned()
                        .map(|message| (entry.key().clone(), message))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (recipient, message) in messages {
                let own_host = state.remote_mail.own_host.lock().clone();
                let sender = Sender {
                    host: own_host.clone().unwrap_or_else(|| "local".into()),
                    id: message.from_tuic_session.clone(),
                    name: message.from_name.clone(),
                };
                let args = json!({"action":"send", "to":recipient, "message":message.content});
                let hub = state.remote_mail.hub.lock().clone();
                let result = if let Some((_, link)) = hub {
                    link.call_with_id(Some(sender), args, Some(message.id.clone()))
                        .await
                } else if own_host.is_none() {
                    route_send(&state, Some(sender), args, Some(message.id.clone())).await
                } else {
                    continue;
                };
                if result.get("message_id").is_some() && result.get("error").is_none() {
                    if let Some(mut inbox) = state.agent_inbox.get_mut(&recipient) {
                        inbox.retain(|entry| entry.id != message.id);
                    }
                } else {
                    tracing::warn!(
                        source = "remote_mail",
                        recipient,
                        "Lifecycle mail remains in outbox: {result}"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_state;
    use super::*;

    async fn peer(state: &Arc<AppState>, name: &str) -> (String, String) {
        let sid = uuid::Uuid::new_v4().to_string();
        let result = super::super::mcp_transport::local_peer_call(
            state,
            &json!({"action":"register","name":name}),
            Some(&sid),
        )
        .await;
        (sid, result["tuic_session"].as_str().unwrap().to_string())
    }

    async fn daemon(
        hub: &Arc<AppState>,
        id: &str,
        state: Arc<AppState>,
    ) -> tokio::task::JoinHandle<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        hub.remote
            .force_connected_for_test(id, &url, Some(&state.session_token.read().clone()));
        tokio::spawn(async move {
            axum::serve(
                listener,
                super::super::build_remote_router(state)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        })
    }

    // Catches: /mcp/peer bypasses authentication on loopback or grants arbitrary
    // process tools rather than the approved mail-only boundary.
    #[tokio::test]
    async fn remote_peer_requires_the_connection_token_and_rejects_spawn() {
        let hub = test_state();
        let remote = test_state();
        let server = daemon(&hub, "mint", remote.clone()).await;
        let url = hub.remote.base_url("mint").unwrap().replace("http:", "ws:");
        let rejected = tokio_tungstenite::connect_async(format!(
            "{url}/mcp/peer?connection_id=mint&token=wrong"
        ))
        .await;
        assert!(rejected.is_err(), "loopback must not bypass the peer token");
        let link = connection(&hub, "mint")
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let rejected = link
            .call(None, json!({"action":"spawn","prompt":"do not spawn"}))
            .await;
        assert!(rejected["error"].as_str().unwrap().contains("only"));
        assert!(remote.session_maps.sessions.is_empty());
        let impersonation = link
            .call(
                Some(Sender {
                    host: "mint".into(),
                    id: "victim".into(),
                    name: "forged".into(),
                }),
                json!({"action":"inbox"}),
            )
            .await;
        assert!(
            impersonation["error"]
                .as_str()
                .unwrap()
                .contains("impersonate")
        );
        assert!(!remote.peer_agents.contains_key("victim"));
        disconnect(&hub, "mint");
        server.abort();
    }

    // Catches: remote peer enumeration remains local-only, remote sends do not
    // wake the owning daemon's waiter, or replies stop in a shadow inbox.
    #[tokio::test]
    async fn remote_peer_star_delivers_remote_to_remote_and_reply_to_the_mac() {
        let hub = test_state();
        let mint = test_state();
        let other = test_state();
        let (mac_sid, mac_id) = peer(&hub, "mac").await;
        let (mint_sid, mint_id) = peer(&mint, "mint-agent").await;
        let (other_sid, other_id) = peer(&other, "other-agent").await;
        let first = daemon(&hub, "mint", mint.clone()).await;
        let second = daemon(&hub, "other", other.clone()).await;
        let listed = dispatch(&hub, &json!({"action":"list_peers"}), Some(&mac_sid))
            .await
            .unwrap();
        assert!(
            listed["peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["address"] == format!("mint/{mint_id}"))
        );
        assert!(
            listed["peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["address"] == format!("other/{other_id}"))
        );
        let wait_args = json!({"action":"wait","timeout_ms":60_000});
        let mut remote_wait = Box::pin(super::super::mcp_transport::local_peer_call(
            &other,
            &wait_args,
            Some(&other_sid),
        ));
        assert!(matches!(
            futures_util::poll!(&mut remote_wait),
            std::task::Poll::Pending
        ));
        assert!(other.has_active_agent_waiter(&other_id));
        let sent = dispatch(&mint,&json!({"action":"send","to":format!("other/{other_id}"),"message":"across both spokes"}),Some(&mint_sid)).await.unwrap();
        assert_eq!(
            sent["delivered"], true,
            "owner must deliver through its real waiter: {sent}"
        );
        assert_eq!(sent["delivery_path"], "waiter_and_inbox");
        let received = remote_wait.await;
        assert_eq!(received["messages"][0]["content"], "across both spokes");
        assert_eq!(
            received["messages"][0]["from_tuic_session"],
            format!("mint/{mint_id}")
        );
        let mut mac_wait = Box::pin(super::super::mcp_transport::local_peer_call(
            &hub,
            &wait_args,
            Some(&mac_sid),
        ));
        assert!(matches!(
            futures_util::poll!(&mut mac_wait),
            std::task::Poll::Pending
        ));
        let reply = dispatch(
            &other,
            &json!({"action":"send","to":format!("local/{mac_id}"),"message":"reply to Mac"}),
            Some(&other_sid),
        )
        .await
        .unwrap();
        assert_eq!(
            reply["delivered"], true,
            "reply must reach the hub: {reply}"
        );
        let received = mac_wait.await;
        assert_eq!(
            received["messages"][0]["from_tuic_session"],
            format!("other/{other_id}")
        );
        assert_eq!(received["messages"][0]["content"], "reply to Mac");
        disconnect(&hub, "mint");
        disconnect(&hub, "other");
        first.abort();
        second.abort();
    }

    // Catches: reconnect retries a lifecycle notice and enqueues/wakes it twice.
    #[tokio::test]
    async fn remote_peer_retried_lifecycle_identity_does_not_duplicate_the_inbox() {
        let remote = test_state();
        let (_, recipient) = peer(&remote, "recipient").await;
        let sender = Sender {
            host: "local".into(),
            id: "reporter".into(),
            name: "reporter".into(),
        };
        let arguments = json!({"action":"send", "to":recipient, "message":"child completed"});
        let first = native_call(
            &remote,
            Some(sender.clone()),
            arguments.clone(),
            Some("notice-1".into()),
        )
        .await;
        assert_eq!(first["message_id"], "notice-1", "{first}");
        let second = native_call(
            &remote,
            Some(sender.clone()),
            arguments.clone(),
            Some("notice-1".into()),
        )
        .await;
        assert_eq!(second["delivery_path"], "inbox_duplicate", "{second}");
        assert_eq!(remote.agent_inbox.get(&recipient).unwrap().len(), 1);
        let conflicting = native_call(
            &remote,
            Some(sender),
            json!({"action":"send", "to":recipient, "message":"different payload"}),
            Some("notice-1".into()),
        )
        .await;
        assert!(conflicting.get("error").is_some(), "{conflicting}");
        assert_eq!(remote.agent_inbox.get(&recipient).unwrap().len(), 1);
        assert!(
            !remote
                .mcp
                .to_session
                .iter()
                .any(|entry| entry.key().starts_with("remote-mail:"))
        );
    }

    fn forwarded(id: &str, from: &str) -> crate::state::AgentMessage {
        crate::state::AgentMessage {
            id: id.into(),
            from_tuic_session: from.into(),
            from_name: from.into(),
            content: "one lifecycle notice".into(),
            timestamp: 0,
            delivered_via_channel: false,
        }
    }

    // Catches: sender-specific windows retain IDs beyond the shared recipient horizon,
    // or ring overflow drops a still-retained ID instead of the oldest one.
    #[tokio::test]
    async fn remote_peer_recipient_ring_retains_only_the_last_100_ids_across_senders() {
        let state = test_state();
        let (_, recipient) = peer(&state, "recipient").await;
        let first = forwarded("oldest", "sender-a");
        assert!(
            enqueue_forwarded(&state, &recipient, first.clone())
                .unwrap()
                .is_some()
        );
        state.agent_inbox.remove(&recipient);
        assert_eq!(
            enqueue_forwarded(&state, &recipient, first.clone()),
            Ok(None)
        );
        for n in 1..=100 {
            assert!(
                enqueue_forwarded(
                    &state,
                    &recipient,
                    forwarded(&format!("id-{n}"), "sender-b")
                )
                .unwrap()
                .is_some()
            );
        }
        assert_eq!(
            enqueue_forwarded(&state, &recipient, forwarded("id-1", "sender-b")),
            Ok(None)
        );
        assert!(
            enqueue_forwarded(&state, &recipient, first)
                .unwrap()
                .is_some()
        );
        let inbox = state.agent_inbox.get(&recipient).unwrap();
        assert_eq!(inbox.back().unwrap().id, "oldest");
        assert_eq!(inbox.iter().filter(|m| m.id == "oldest").count(), 1);
    }

    // Catches: peer retirement leaks its replay budget or erases another peer's ids.
    #[tokio::test]
    async fn recipient_unregister_releases_only_its_own_forwarded_history() {
        let state = test_state();
        let (sid, first) = peer(&state, "first").await;
        let (_, second) = peer(&state, "second").await;
        let original = forwarded("notice", "sender");
        assert_eq!(record_forwarded(&state, &first, &original), Ok(true));
        assert_eq!(record_forwarded(&state, &second, &original), Ok(true));
        unregister_peer(&state, &first);
        let restored = super::super::mcp_transport::local_peer_call(
            &state,
            &json!({"action":"register","tuic_session":first,"name":"restored"}),
            Some(&sid),
        )
        .await;
        assert_eq!(restored["tuic_session"], first, "{restored}");
        assert_eq!(record_forwarded(&state, &first, &original), Ok(true));
        assert_eq!(record_forwarded(&state, &second, &original), Ok(false));
    }

    // Catches: a retired recipient acquires replay state or an orphan inbox.
    #[tokio::test]
    async fn forwarded_enqueue_after_unregister_never_leaves_orphan_state() {
        let state = test_state();
        let (_, recipient) = peer(&state, "recipient").await;
        unregister_peer(&state, &recipient);
        assert_eq!(
            enqueue_forwarded(&state, &recipient, forwarded("notice", "sender")),
            Err("Forwarded recipient is no longer registered".into())
        );
        assert!(
            !state
                .remote_mail
                .forwarded_history
                .lock()
                .recipients
                .contains_key(&recipient)
        );
        assert!(!state.agent_inbox.contains_key(&recipient));
    }

    fn shadow(state: &AppState, sender: &str) {
        state.peer_agents.insert(
            sender.into(),
            crate::state::PeerAgent {
                tuic_session: sender.into(),
                mcp_session_id: format!("remote-mail:test:{sender}"),
                name: sender.into(),
                project: None,
                registered_at: 0,
            },
        );
    }

    // Catches: disconnect without a handshake lock removes unrelated host peers.
    #[tokio::test]
    async fn remote_peer_disconnect_without_a_gate_retires_only_its_host() {
        let state = test_state();
        shadow(&state, "mint/old");
        shadow(&state, "other/live");
        disconnect(&state, "mint");
        assert!(!state.peer_agents.contains_key("mint/old"));
        assert!(state.peer_agents.contains_key("other/live"));
        disconnect(&state, "mint");
        assert!(state.peer_agents.contains_key("other/live"));
    }

    // Catches: a pending handshake defers shadow retirement until lock release.
    #[tokio::test]
    async fn remote_peer_disconnect_retires_shadows_before_a_held_handshake_returns() {
        let hub = test_state();
        let (server, entered, release, _) = held_daemon(&hub).await;
        shadow(&hub, "mint/old");
        let opening_hub = hub.clone();
        let opening = tokio::spawn(async move { connection(&opening_hub, "mint").await });
        entered.await.unwrap();
        disconnect(&hub, "mint");
        assert!(!hub.peer_agents.contains_key("mint/old"));
        release.notify_one();
        assert!(opening.await.unwrap().is_err());
        server.abort();
    }

    // Catches: delayed old-generation cleanup removes a reconnect's fresh shadow.
    #[tokio::test]
    async fn remote_peer_reconnect_shadow_survives_the_old_handshakes_completion() {
        let hub = test_state();
        let (server, entered, release, _) = held_daemon(&hub).await;
        shadow(&hub, "mint/old");
        let opening_hub = hub.clone();
        let opening = tokio::spawn(async move { connection(&opening_hub, "mint").await });
        entered.await.unwrap();
        disconnect(&hub, "mint");
        connect_configured(&hub, "mint".into());
        assert!(!hub.peer_agents.contains_key("mint/old"));
        let registered = native_call(
            &hub,
            Some(Sender {
                host: "mint".into(),
                id: "fresh".into(),
                name: "fresh".into(),
            }),
            json!({"action":"register"}),
            None,
        )
        .await;
        assert_eq!(registered["tuic_session"], "mint/fresh");
        release.notify_one();
        assert!(opening.await.unwrap().is_err());
        assert!(hub.peer_agents.contains_key("mint/fresh"));
        disconnect(&hub, "mint");
        server.abort();
    }

    // The native router performs the real authenticated WebSocket handshake.
    // Middleware holds only response setup; no external protocol is fabricated.
    async fn held_daemon(
        hub: &Arc<AppState>,
    ) -> (
        tokio::task::JoinHandle<()>,
        tokio::sync::oneshot::Receiver<()>,
        Arc<tokio::sync::Notify>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let remote = test_state();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        hub.remote.force_connected_for_test(
            "mint",
            &url,
            Some(&remote.session_token.read().clone()),
        );
        let (tx, entered) = tokio::sync::oneshot::channel();
        let signal = Arc::new(Mutex::new(Some(tx)));
        let release = Arc::new(tokio::sync::Notify::new());
        let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let router = super::super::build_remote_router(remote).layer(axum::middleware::from_fn({
            let release = release.clone();
            let requests = requests.clone();
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let release = release.clone();
                let signal = signal.clone();
                let requests = requests.clone();
                async move {
                    if request.uri().path() == "/mcp/peer" {
                        requests.fetch_add(1, Ordering::AcqRel);
                        if let Some(tx) = signal.lock().take() {
                            let _ = tx.send(());
                        }
                        release.notified().await;
                    }
                    next.run(request).await
                }
            }
        }));
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        (server, entered, release, requests)
    }

    // Catches: concurrent callers for one host race into two authenticated hubs.
    #[tokio::test]
    async fn same_host_parallel_calls_open_only_one_authenticated_connection() {
        let hub = test_state();
        let (server, entered, release, requests) = held_daemon(&hub).await;
        let first_hub = hub.clone();
        let first = tokio::spawn(async move { connection(&first_hub, "mint").await });
        entered.await.unwrap();
        let mut second = Box::pin(connection(&hub, "mint"));
        assert!(matches!(
            futures_util::poll!(&mut second),
            std::task::Poll::Pending
        ));
        release.notify_one();
        assert!(first.await.unwrap().is_ok());
        assert!(second.await.is_ok());
        assert_eq!(
            requests.load(Ordering::Acquire),
            1,
            "one host must share one handshake"
        );
        disconnect(&hub, "mint");
        server.abort();
    }

    // Catches: disconnect during handshake publishes a link after retirement.
    #[tokio::test]
    async fn disconnect_during_a_handshake_never_publishes_a_stranded_link() {
        let hub = test_state();
        let (server, entered, release, _) = held_daemon(&hub).await;
        let first_hub = hub.clone();
        let opening = tokio::spawn(async move { connection(&first_hub, "mint").await });
        entered.await.unwrap();
        disconnect(&hub, "mint");
        release.notify_one();
        let result = opening.await.unwrap();
        assert!(
            matches!(result, Err(ref detail) if detail["error"].as_str().unwrap().contains("changed during"))
        );
        assert!(!hub.remote_mail.connections.contains_key("mint"));
        server.abort();
    }

    // Catches: a credential change publishes a socket authenticated with the old token.
    #[tokio::test]
    async fn configured_token_rotation_during_handshake_rejects_the_old_generation() {
        let hub = test_state();
        let (server, entered, release, _) = held_daemon(&hub).await;
        let first_hub = hub.clone();
        let opening = tokio::spawn(async move { connection(&first_hub, "mint").await });
        entered.await.unwrap();
        let url = hub.remote.base_url("mint").unwrap();
        hub.remote
            .force_connected_for_test("mint", &url, Some("rotated-token"));
        release.notify_one();
        let result = opening.await.unwrap();
        assert!(
            matches!(result, Err(ref detail) if detail["error"].as_str().unwrap().contains("changed during"))
        );
        assert!(!hub.remote_mail.connections.contains_key("mint"));
        server.abort();
    }

    // Catches: star routing disables local delivery when the hub is down, or a
    // qualified unavailable host degrades to the misleading not-registered error.
    #[tokio::test]
    async fn remote_peer_hub_down_keeps_local_mail_and_names_unavailable_connections() {
        let remote = test_state();
        let (sid, sender) = peer(&remote, "sender").await;
        let (_, recipient) = peer(&remote, "recipient").await;
        *remote.remote_mail.own_host.lock() = Some("mint".into());
        let local_args = json!({"action":"send","to":recipient,"message":"offline local"});
        assert!(dispatch(&remote, &local_args, Some(&sid)).await.is_none());
        let local =
            super::super::mcp_transport::local_peer_call(&remote, &local_args, Some(&sid)).await;
        assert!(
            local.get("message_id").is_some(),
            "native local mail must survive hub loss: {local}"
        );
        let messages = remote.agent_inbox.get(&recipient).unwrap();
        assert_eq!(messages[0].from_tuic_session, sender);
        drop(messages);
        let unavailable = dispatch(
            &remote,
            &json!({"action":"send","to":"other/peer","message":"offline remote"}),
            Some(&sid),
        )
        .await
        .unwrap();
        assert!(unavailable["error"].as_str().unwrap().contains("mint"));
        assert!(
            !unavailable["error"]
                .as_str()
                .unwrap()
                .contains("not registered")
        );
        let hub = test_state();
        let (sid, _) = peer(&hub, "mac").await;
        let missing = dispatch(
            &hub,
            &json!({"action":"send","to":"missing/peer","message":"unknown host"}),
            Some(&sid),
        )
        .await
        .unwrap();
        assert!(missing["error"].as_str().unwrap().contains("missing"));
    }
    mod authentication_and_routing {
        //! Critic tests for story 1419-ab18: authentication, provenance, replay and
        //! head-of-line behaviour of the authenticated /mcp/peer star.

        use super::*;
        use crate::mcp_http::tests::test_state;
        use std::net::SocketAddr;

        fn peer(state: &Arc<AppState>, id: &str) {
            state.peer_agents.insert(
                id.to_string(),
                crate::state::PeerAgent {
                    tuic_session: id.to_string(),
                    mcp_session_id: format!("sid-{id}"),
                    name: id.to_string(),
                    project: None,
                    registered_at: 0,
                },
            );
            state
                .mcp
                .to_session
                .insert(format!("sid-{id}"), id.to_string());
        }

        async fn serve(state: Arc<AppState>) -> SocketAddr {
            let router = axum::Router::new()
                .route("/mcp/peer", axum::routing::get(endpoint))
                .with_state(state);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            addr
        }

        async fn open(
            addr: SocketAddr,
            query: &str,
        ) -> Result<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Error,
        > {
            tokio_tungstenite::connect_async(format!("ws://{addr}/mcp/peer?{query}"))
                .await
                .map(|(socket, _)| socket)
        }

        // Catches: an endpoint that upgrades without the daemon token, with a wrong
        // token, or with an empty configured token equal to an empty `token=` param,
        // or that accepts a connection qualifier the hub could confuse with "local".
        #[tokio::test]
        async fn peer_endpoint_rejects_missing_wrong_empty_token_and_bad_qualifier() {
            let state = test_state();
            *state.session_token.write() = "secret-token".into();
            let addr = serve(state.clone()).await;
            assert!(open(addr, "connection_id=mint").await.is_err(), "no token");
            assert!(
                open(addr, "connection_id=mint&token=wrong").await.is_err(),
                "wrong token"
            );
            assert!(
                open(addr, "connection_id=local&token=secret-token")
                    .await
                    .is_err(),
                "qualifier local"
            );
            assert!(
                open(addr, "connection_id=a%2Fb&token=secret-token")
                    .await
                    .is_err(),
                "qualifier with slash"
            );
            let _good = open(addr, "connection_id=mint&token=secret-token")
                .await
                .expect("the correct token must open, or the rejections above prove nothing");
            assert!(
                open(addr, "connection_id=other&token=secret-token")
                    .await
                    .is_err(),
                "a second hub must not replace the live one"
            );

            let empty = test_state();
            *empty.session_token.write() = String::new();
            let addr = serve(empty).await;
            assert!(
                open(addr, "connection_id=mint&token=").await.is_err(),
                "empty configured token must not authenticate an empty token param"
            );
        }

        // Catches: a spoke supplying `sender.host` of its own choosing ("local" or a
        // foreign host) and mail arriving attributed to a desktop-local or other-host peer.
        #[tokio::test]
        async fn spoke_supplied_sender_host_is_overwritten_by_the_connection() {
            let state = test_state();
            peer(&state, "b");
            let result = process(
                state.clone(),
                &Role::Hub("mint".into()),
                Some(Sender {
                    host: "local".into(),
                    id: "x".into(),
                    name: "x".into(),
                }),
                json!({"action": "send", "to": "local/b", "message": "hi"}),
                None,
            )
            .await;
            assert!(result.get("error").is_none(), "{result}");
            let inbox = state.agent_inbox.get("b").expect("mail filed under b");
            assert_eq!(inbox[0].from_tuic_session, "mint/x");
        }

        // Catches: a spoke reading, waiting on or registering as a peer of another host
        // through the hub (only send/list_peers may arrive daemon-to-hub).
        #[tokio::test]
        async fn spoke_cannot_read_wait_or_register_through_the_hub() {
            let state = test_state();
            peer(&state, "b");
            for action in ["inbox", "wait", "register", "spawn", "kill"] {
                let result = process(
                    state.clone(),
                    &Role::Hub("mint".into()),
                    Some(Sender {
                        host: "mint".into(),
                        id: "b".into(),
                        name: "b".into(),
                    }),
                    json!({"action": action, "timeout_ms": 1}),
                    None,
                )
                .await;
                assert!(result.get("error").is_some(), "{action}: {result}");
            }
        }

        // Catches: an empty session_id resolving, via `starts_with("")`, to the only
        // remote PTY, so a missing target submits to a remote agent.
        #[tokio::test]
        async fn empty_session_id_never_selects_a_remote_pty() {
            let state = test_state();
            crate::remote_mirror::store_seed_for_test(
                &state,
                "mint",
                vec![crate::mcp_http::types::SessionInfo {
                    session_id: "remote-pty".into(),
                    ..Default::default()
                }],
            );
            for args in [
                json!({"action": "submit", "session_id": "", "input": "x"}),
                json!({"action": "submit", "session_id": "", "connection_id": "mint", "input": "x"}),
                json!({"action": "submit", "session_id": "mint/", "input": "x"}),
            ] {
                let resolved = super::super::remote_mcp_sessions::resolve(&state, &args);
                assert!(
                    !matches!(resolved, Some(Ok(_))),
                    "{args} resolved to a remote PTY"
                );
            }
        }

        // Catches: lifecycle outbox replay after a lost acknowledgement re-delivering a
        // message the recipient already read (dedupe only while still in the inbox).
        #[tokio::test]
        async fn replayed_message_id_is_not_redelivered_after_the_recipient_read_it() {
            let state = test_state();
            peer(&state, "a");
            peer(&state, "b");
            let args = json!({"action": "send", "to": "b", "message": "hi"});
            let first = super::super::mcp_transport::local_peer_call_with_message_id(
                &state,
                &args,
                Some("sid-a"),
                Some("m1".into()),
            )
            .await;
            assert!(first.get("error").is_none(), "{first}");
            state.agent_inbox.get_mut("b").unwrap().clear(); // recipient read it
            let _ = super::super::mcp_transport::local_peer_call_with_message_id(
                &state,
                &args,
                Some("sid-a"),
                Some("m1".into()),
            )
            .await;
            assert!(
                state
                    .agent_inbox
                    .get("b")
                    .is_none_or(|inbox| inbox.is_empty()),
                "same forwarded message id was delivered twice"
            );
        }

        // Catches: one silent (blackholed) connection holding the global connect lock
        // for its whole 20 s handshake budget and starving mail to every other host.
        #[tokio::test]
        async fn a_stalled_connection_does_not_block_calls_to_other_connections() {
            let state = test_state();
            peer(&state, "a");
            let stall = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", stall.local_addr().unwrap());
            state
                .remote
                .force_connected_for_test("stall", &url, Some("t"));
            let slow_state = state.clone();
            let slow = tokio::spawn(async move {
                dispatch(
                    &slow_state,
                    &json!({"action": "send", "to": "stall/x", "message": "m"}),
                    Some("sid-a"),
                )
                .await
            });
            tokio::time::sleep(Duration::from_millis(300)).await;
            let other = tokio::time::timeout(
                Duration::from_secs(3),
                dispatch(
                    &state,
                    &json!({"action": "send", "to": "ghost/x", "message": "m"}),
                    Some("sid-a"),
                ),
            )
            .await;
            slow.abort();
            assert!(
                other.is_ok(),
                "a call to an unrelated connection waited behind another host's handshake"
            );
        }
    }

    mod replay_and_teardown {
        //! Round 2 critic tests for story 1419-ab18: dedup horizon, session address
        //! ownership, teardown cleanup and secrets in errors.

        use super::*;
        use crate::mcp_http::tests::test_state;

        fn message(id: &str, from: &str, content: &str) -> crate::state::AgentMessage {
            crate::state::AgentMessage {
                id: id.to_string(),
                from_tuic_session: from.to_string(),
                from_name: from.to_string(),
                content: content.to_string(),
                timestamp: 0,
                delivered_via_channel: false,
            }
        }

        // Catches: the 1024-recipient cap evicting the oldest recipient wholesale, so
        // traffic to 1024 other recipients erases the dedup history of the first one.
        #[test]
        fn recipient_cap_eviction_does_not_reopen_replay_for_an_old_recipient() {
            let state = test_state();
            let first = message("m1", "mint/a", "once");
            assert_eq!(record_forwarded(&state, "r0", &first), Ok(true));
            for n in 1..=1024 {
                let other = message(&format!("o-{n}"), &format!("other-{n}/a"), "x");
                assert_eq!(record_forwarded(&state, &format!("r{n}"), &other), Ok(true));
            }
            assert_eq!(
                record_forwarded(&state, "r0", &first),
                Ok(false),
                "recipient eviction let a replayed id through"
            );
        }

        // Catches: an empty or whitespace id/sender/content boundary hashing to the
        // same fingerprint (ambiguous concatenation) and a changed body under the same
        // id being accepted as a duplicate.
        #[test]
        fn same_id_with_shifted_sender_content_boundary_is_a_collision() {
            let state = test_state();
            assert_eq!(
                record_forwarded(&state, "r", &message("m", "ab", "c")),
                Ok(true)
            );
            assert_eq!(
                record_forwarded(&state, "r", &message("m", "a", "bc")),
                Err("Forwarded message identity collision".into())
            );
        }

        // Catches: an ambiguous LOCAL address ("matches two desktop sessions") falling
        // through to remote prefix matching, so a submit/read meant for a local session
        // silently lands on the one remote row that shares the prefix.
        #[test]
        #[cfg(unix)]
        fn ambiguous_local_address_never_selects_a_remote_session() {
            let state = test_state();
            for id in [
                "11111111-89ab-cdef-0123-456789abcdef",
                "11111111-89ab-cdef-0123-456789abcdef0",
            ] {
                crate::state::tests_support::insert_dummy_session(&state, id);
            }
            crate::remote_mirror::store_seed_for_test(
                &state,
                "mint",
                vec![crate::mcp_http::types::SessionInfo {
                    session_id: "11111111-remote".into(),
                    ..Default::default()
                }],
            );
            let resolved = super::super::remote_mcp_sessions::resolve(
                &state,
                &json!({"action":"submit","session_id":"11111111","input":"x"}),
            );
            assert!(
                !matches!(resolved, Some(Ok(_))),
                "ambiguous local address was routed to a remote host: {resolved:?}"
            );
        }

        // Catches: probes for unknown hosts leaving permanent entries in connect_locks.
        #[tokio::test]
        async fn unknown_host_probes_do_not_accumulate_connect_locks() {
            let state = test_state();
            for n in 0..50 {
                let _ = connection(&state, &format!("ghost-{n}")).await;
            }
            assert!(
                state.remote_mail.connect_locks.lock().len() <= 1,
                "connect_locks leaked {} entries",
                state.remote_mail.connect_locks.lock().len()
            );
        }

        // Catches: a failed peer handshake echoing the connection token (it travels in
        // the websocket query string) in the error returned to the MCP caller.
        #[tokio::test]
        async fn failed_handshake_error_does_not_contain_the_token() {
            let state = test_state();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            drop(listener);
            state
                .remote
                .force_connected_for_test("mint", &url, Some("SECRET-TOKEN-1419"));
            let error = connection(&state, "mint").await.err().expect("must fail");
            assert!(!error.to_string().contains("SECRET-TOKEN-1419"), "{error}");
        }

        // Catches: `disconnect` removing the link itself, so the teardown task's
        // `remove_if(..).is_some()` is false and the daemon's shadow identities stay
        // registered (addressable, listed) after the configured connection is gone.
        #[tokio::test]
        async fn disconnect_removes_the_shadow_identities_of_that_host() {
            let hub = test_state();
            let mint = test_state();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let token = mint.session_token.read().clone();
            hub.remote
                .force_connected_for_test("mint", &url, Some(&token));
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    super::super::build_remote_router(mint)
                        .into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .await
                .unwrap();
            });
            hub.peer_agents.insert(
                "mint/ghost".into(),
                crate::state::PeerAgent {
                    tuic_session: "mint/ghost".into(),
                    mcp_session_id: "remote-mail:test:ghost".into(),
                    name: "ghost".into(),
                    project: None,
                    registered_at: 0,
                },
            );
            connection(&hub, "mint").await.expect("link opens");
            assert!(hub.remote_mail.connections.contains_key("mint"));
            disconnect(&hub, "mint");
            let mut cleaned = false;
            for _ in 0..40 {
                if !hub.peer_agents.contains_key("mint/ghost") {
                    cleaned = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            server.abort();
            assert!(cleaned, "shadow identity survived disconnect of its host");
        }
    }

    mod retirement_races {
        //! Round 3 security critic tests for story 1419-ab18: replay budget fairness and
        //! accounting, concurrent retirement, and disconnect racing a handshake.

        use super::*;
        use crate::mcp_http::tests::test_state;

        fn message(id: &str, from: &str) -> crate::state::AgentMessage {
            crate::state::AgentMessage {
                id: id.to_string(),
                from_tuic_session: from.to_string(),
                from_name: from.to_string(),
                content: "one lifecycle notice".to_string(),
                timestamp: 0,
                delivered_via_channel: false,
            }
        }

        // Catches: a sender's retirement (host disconnect) purges its windows, so a
        // lost-ack retry after reconnect is delivered twice.
        #[test]
        fn dedupe_survives_the_senders_own_retirement() {
            let state = test_state();
            assert_eq!(
                record_forwarded(&state, "r", &message("n1", "mint/s")),
                Ok(true)
            );
            unregister_peer(&state, "mint/s");
            assert_eq!(
                record_forwarded(&state, "r", &message("n1", "mint/s")),
                Ok(false)
            );
        }

        // Catches: checking registration before the history lock lets retirement finish
        // before an already-admitted enqueue recreates an unregistered recipient's ring.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_enqueue_and_retirement_keep_history_consistent() {
            let state = test_state();
            let (registered, registrations) = std::sync::mpsc::channel();
            let (retired, retirements) = std::sync::mpsc::channel();
            let producer = {
                let state = state.clone();
                tokio::task::spawn_blocking(move || {
                    for i in 0..3000 {
                        state.peer_agents.insert(
                            "R".into(),
                            crate::state::PeerAgent {
                                tuic_session: "R".into(),
                                mcp_session_id: "sid-R".into(),
                                name: "R".into(),
                                project: None,
                                registered_at: 0,
                            },
                        );
                        registered.send(()).unwrap();
                        let _ =
                            enqueue_forwarded(&state, "R", message(&format!("id-{i}"), "mint/s"));
                        retirements.recv().unwrap();
                        // Both operations finished; check before another registration can
                        // hide an orphan behind a live peer or another retirement clears it.
                        assert!(
                            !state
                                .remote_mail
                                .forwarded_history
                                .lock()
                                .recipients
                                .contains_key("R"),
                            "orphan replay state for an unregistered peer at interleaving {i}"
                        );
                    }
                })
            };
            let retirer = {
                let state = state.clone();
                tokio::task::spawn_blocking(move || {
                    while registrations.recv().is_ok() {
                        unregister_peer(&state, "R");
                        state.agent_inbox.remove("R");
                        if retired.send(()).is_err() {
                            break;
                        }
                    }
                })
            };
            let produced = producer.await;
            retirer.await.unwrap();
            produced.unwrap();
            if !state.peer_agents.contains_key("R") {
                assert!(
                    !state
                        .remote_mail
                        .forwarded_history
                        .lock()
                        .recipients
                        .contains_key("R"),
                    "orphan replay state for an unregistered peer"
                );
            }
        }

        async fn held_daemon(
            hub: &Arc<AppState>,
        ) -> (
            tokio::task::JoinHandle<()>,
            tokio::sync::oneshot::Receiver<()>,
            Arc<tokio::sync::Notify>,
        ) {
            let remote = test_state();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            hub.remote.force_connected_for_test(
                "mint",
                &url,
                Some(&remote.session_token.read().clone()),
            );
            let (tx, entered) = tokio::sync::oneshot::channel();
            let signal = Arc::new(parking_lot::Mutex::new(Some(tx)));
            let release = Arc::new(tokio::sync::Notify::new());
            let router =
                super::super::build_remote_router(remote).layer(axum::middleware::from_fn({
                    let release = release.clone();
                    move |request: axum::extract::Request, next: axum::middleware::Next| {
                        let release = release.clone();
                        let signal = signal.clone();
                        async move {
                            if request.uri().path() == "/mcp/peer" {
                                if let Some(tx) = signal.lock().take() {
                                    let _ = tx.send(());
                                }
                                release.notified().await;
                            }
                            next.run(request).await
                        }
                    }
                }));
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .await
                .unwrap();
            });
            (server, entered, release)
        }

        // Catches: disconnect + immediate reconnect while a handshake holds the host
        // lock skips the deferred retirement (generation moved on), so the old link's
        // shadow peers survive although the identical sequence without a racing
        // handshake retires them synchronously.
        #[tokio::test]
        async fn disconnect_then_reconnect_during_a_handshake_still_retires_old_shadows() {
            let hub = test_state();
            let (server, entered, release) = held_daemon(&hub).await;
            hub.peer_agents.insert(
                "mint/ghost".into(),
                crate::state::PeerAgent {
                    tuic_session: "mint/ghost".into(),
                    mcp_session_id: "remote-mail:test:ghost".into(),
                    name: "ghost".into(),
                    project: None,
                    registered_at: 0,
                },
            );
            let opening = {
                let hub = hub.clone();
                tokio::spawn(async move { connection(&hub, "mint").await })
            };
            entered.await.unwrap();
            disconnect(&hub, "mint");
            connect_configured(&hub, "mint".into());
            let mut relinked = false;
            for _ in 0..100 {
                release.notify_one();
                if hub.remote_mail.connections.contains_key("mint") {
                    relinked = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let first = opening.await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            let survived = hub.peer_agents.contains_key("mint/ghost");
            disconnect(&hub, "mint");
            server.abort();
            assert!(
                first.is_err(),
                "the pre-disconnect handshake must not publish"
            );
            assert!(relinked, "the reconnect must still establish its own link");
            assert!(
                !survived,
                "shadow peer of the disconnected link survived the race"
            );
        }
    }
}
