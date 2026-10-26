//! The `tuicommander` MCP server as ego reaches it: over the ACP connection.
//!
//! Not a second server. Every request goes through [`mcp_post`], the handler
//! behind HTTP `/mcp`, with the headers a bridge would have sent — so the tool
//! registry, the collapsed surface ego is given and the peer binding are the
//! ones every other client gets. What this module owns is only what HTTP gets
//! from the transport: a protocol session per MCP connection, bound to the
//! identity ego was launched under, and the server-to-client stream that GET
//! `/mcp` would have carried.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::mcp_transport::{
    MCP_SESSION_HEADER, TUIC_SESSION_HEADER, add_result_envelope, end_mcp_session, mcp_post,
    refresh_mcp_session,
};
use crate::AppState;
use crate::acp::{McpNotify, McpOverAcpError, McpOverAcpHost, McpReply};

/// JSON-RPC's "invalid params", for a connection id this host never issued.
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

/// The address a request over ACP is served as: ego runs on this machine, and
/// the IPC listener hands its requests the same synthetic loopback peer.
fn loopback() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

pub(crate) struct AcpMcpHost {
    state: Weak<AppState>,
    connections: Mutex<HashMap<String, Link>>,
    /// Wake an idle ego whose inbox has mail it has not read.
    wake: Waker,
}

/// Given a peer id, start a turn in its conversation if nothing is running.
type WakeResult = std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>;
type Waker = Arc<dyn Fn(String) -> WakeResult + Send + Sync>;

/// How long a burst of mail is gathered before an idle ego is woken, so ten
/// messages cost one turn rather than ten.
const WAKE_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(2);

/// What one MCP connection holds beyond its protocol session.
struct Link {
    peer_id: Option<String>,
    /// The resources ego asked to hear about.
    subscriptions: Subscriptions,
    /// The stream GET `/mcp` would have carried, forwarded to ego.
    forwarder: JoinHandle<()>,
}

type Subscriptions = Arc<Mutex<HashSet<String>>>;

impl AcpMcpHost {
    pub(crate) fn new(state: &Arc<AppState>) -> Self {
        let weak = Arc::downgrade(state);
        let wake: Waker = Arc::new(move |peer: String| {
            let state = weak.upgrade();
            Box::pin(async move {
                let Some(state) = state else { return false };
                match state
                    .acp
                    .wake_idle_peer(&peer, crate::pty::PEER_MAIL_WAKE)
                    .await
                {
                    Ok(Some(turn)) => {
                        tracing::info!(source = "acp_mcp", tuic_session = %peer,
                            turn = ?turn, "Woke idle ego for new mail");
                        true
                    }
                    Ok(None) => false,
                    Err(error) => {
                        tracing::warn!(source = "acp_mcp", tuic_session = %peer,
                            "Waking ego for new mail failed: {error:?}");
                        false
                    }
                }
            })
        });
        Self {
            state: Arc::downgrade(state),
            connections: Mutex::new(HashMap::new()),
            wake,
        }
    }

    fn state(&self) -> Result<Arc<AppState>, McpOverAcpError> {
        self.state
            .upgrade()
            .ok_or_else(|| McpOverAcpError::new(INTERNAL_ERROR, "TUICommander is shutting down"))
    }

    fn link(
        &self,
        connection_id: &str,
    ) -> Result<(Option<String>, Subscriptions), McpOverAcpError> {
        self.connections
            .lock()
            .get(connection_id)
            .map(|link| (link.peer_id.clone(), Arc::clone(&link.subscriptions)))
            .ok_or_else(|| {
                McpOverAcpError::new(INVALID_PARAMS, format!("no MCP connection {connection_id}"))
            })
    }
}

/// Serve `tuicommander` to every ACP connection opened from now on.
///
/// Installed next to the maintenance sweep, on the desktop app and on the
/// daemon alike: both launch ego, and a connection opened before this runs
/// answers `mcp/connect` with "this client serves no MCP over ACP".
pub(crate) fn install(state: &Arc<AppState>) {
    state.acp.set_mcp_host(Arc::new(AcpMcpHost::new(state)));
}

impl McpOverAcpHost for AcpMcpHost {
    fn has_inbox_subscriber(&self, peer_id: &str) -> bool {
        self.connections.lock().values().any(|link| {
            link.peer_id.as_deref() == Some(peer_id)
                && !link.forwarder.is_finished()
                && link
                    .subscriptions
                    .lock()
                    .iter()
                    .any(|uri| uri == INBOX_URI || subscription_id(uri).is_some())
        })
    }

    fn connect(&self, peer_id: Option<&str>, notify: McpNotify) -> Result<String, McpOverAcpError> {
        let state = self.state()?;
        let id = uuid::Uuid::new_v4().to_string();
        refresh_mcp_session(&state, &id, false, peer_id);
        // Subscribed under the session entry, the order `mcp_get` keeps:
        // `mcp_sessions` before `messaging_channels`.
        let messages = {
            let Some(mut meta) = state.mcp.sessions.get_mut(&id) else {
                return Err(McpOverAcpError::new(
                    INTERNAL_ERROR,
                    "MCP session vanished while opening",
                ));
            };
            meta.has_sse_stream = true;
            state
                .session_maps
                .messaging_channels
                .entry(id.clone())
                .or_insert_with(|| broadcast::channel(64).0)
                .subscribe()
        };
        let subscriptions = Subscriptions::default();
        let feeds = Feeds {
            tools: state.mcp.tools_changed.subscribe(),
            messages,
            events: state.event_bus.subscribe(),
            inbox: peer_id.map(|peer| state.subscribe_agent_inbox(peer)),
            peer: peer_id.map(str::to_owned),
        };
        let wake = peer_id.map(|peer| {
            let wake = Arc::clone(&self.wake);
            let peer = peer.to_owned();
            Box::new(move || wake(peer.clone())) as Box<dyn Fn() -> WakeResult + Send>
        });
        let forwarder = tokio::spawn(forward(
            Arc::downgrade(&state),
            feeds,
            Arc::clone(&subscriptions),
            notify,
            wake,
        ));
        self.connections.lock().insert(
            id.clone(),
            Link {
                peer_id: peer_id.map(str::to_owned),
                subscriptions,
                forwarder,
            },
        );
        Ok(id)
    }

    fn message(
        &self,
        connection_id: &str,
        method: String,
        params: Option<Map<String, Value>>,
    ) -> McpReply {
        let prepared = self.state().and_then(|state| {
            let (peer_id, subscriptions) = self.link(connection_id)?;
            let mut headers = HeaderMap::new();
            let header = |value: &str| {
                value.parse().map_err(|_| {
                    McpOverAcpError::new(INVALID_PARAMS, "identity is not a header value")
                })
            };
            headers.insert(MCP_SESSION_HEADER, header(connection_id)?);
            if let Some(peer_id) = &peer_id {
                headers.insert(TUIC_SESSION_HEADER, header(peer_id)?);
            }
            Ok((state, headers, peer_id, subscriptions))
        });
        Box::pin(async move {
            let (state, headers, peer_id, subscriptions) = prepared?;
            match method.as_str() {
                "resources/list" => {
                    let mut result = resource_list();
                    add_result_envelope(&mut result);
                    return Ok(result);
                }
                "resources/read" => {
                    let uri = uri_param(params.as_ref())?;
                    let contents = match uri.as_str() {
                        WORKSPACE_URI => {
                            workspace_snapshot(&state, &crate::config::load_repositories())
                        }
                        INBOX_URI => inbox_snapshot(&state, peer_id.as_deref()),
                        _ if subscription_id(&uri).is_some() => {
                            let peer = peer_id.as_deref().ok_or_else(|| {
                                McpOverAcpError::new(INVALID_PARAMS, "Inbox requires a bound peer")
                            })?;
                            crate::mail_subscriptions::read(
                                &state,
                                peer,
                                &json!({"action":"inbox","subscription_id":subscription_id(&uri)}),
                            )
                            .await
                        }
                        _ => return Err(unknown_resource(&uri)),
                    };
                    let mut result = json!({ "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": contents.to_string(),
                    }] });
                    add_result_envelope(&mut result);
                    return Ok(result);
                }
                "resources/subscribe" | "resources/unsubscribe" => {
                    let uri = uri_param(params.as_ref())?;
                    if uri != WORKSPACE_URI && uri != INBOX_URI && subscription_id(&uri).is_none() {
                        return Err(unknown_resource(&uri));
                    }
                    if method == "resources/subscribe" {
                        if let Some(id) = subscription_id(&uri) {
                            let peer = peer_id.as_deref().ok_or_else(|| {
                                McpOverAcpError::new(INVALID_PARAMS, "Inbox requires a bound peer")
                            })?;
                            let batch = crate::mail_subscriptions::read(
                                &state,
                                peer,
                                &json!({"action":"inbox","subscription_id":id}),
                            )
                            .await;
                            if let Some(error) = batch["error"].as_str() {
                                return Err(McpOverAcpError::new(INVALID_PARAMS, error));
                            }
                        }
                        subscriptions.lock().insert(uri);
                        if let Some(peer) = peer_id.as_deref() {
                            state.notify_mail_subscription_change(peer);
                        }
                    } else {
                        subscriptions.lock().remove(&uri);
                    }
                    let mut result = json!({});
                    add_result_envelope(&mut result);
                    return Ok(result);
                }
                _ => {}
            }
            let body = json!({ "jsonrpc": "2.0", "id": 0, "method": method, "params": params });
            let response = Box::pin(mcp_post(
                State(state),
                ConnectInfo(loopback()),
                headers,
                Json(body),
            ))
            .await
            .into_response();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|error| McpOverAcpError::new(INTERNAL_ERROR, error.to_string()))?;
            let reply: Value = serde_json::from_slice(&bytes)
                .map_err(|error| McpOverAcpError::new(INTERNAL_ERROR, error.to_string()))?;
            if let Some(error) = reply.get("error") {
                return Err(McpOverAcpError {
                    code: error["code"]
                        .as_i64()
                        .and_then(|code| i32::try_from(code).ok())
                        .unwrap_or(INTERNAL_ERROR),
                    message: error["message"].as_str().unwrap_or_default().to_owned(),
                    data: error.get("data").cloned(),
                });
            }
            let mut result = reply.get("result").cloned().unwrap_or(Value::Null);
            // The resources exist only on this channel, so only this channel's
            // handshake says so; `/mcp` answers every other client unchanged.
            if matches!(method.as_str(), "initialize" | "server/discover")
                && let Some(capabilities) = result
                    .get_mut("capabilities")
                    .and_then(Value::as_object_mut)
            {
                capabilities.insert(
                    "resources".to_owned(),
                    json!({ "subscribe": true, "listChanged": false }),
                );
            }
            Ok(result)
        })
    }

    /// Nothing ego notifies changes server state: `notifications/initialized`
    /// is an acknowledgement, and `notifications/cancelled` is served by the
    /// ACP request cancellation of the `mcp/message` it names.
    fn notification(
        &self,
        _connection_id: &str,
        _method: String,
        _params: Option<Map<String, Value>>,
    ) {
    }

    fn disconnect(&self, connection_id: &str) {
        let Some(link) = self.connections.lock().remove(connection_id) else {
            return;
        };
        link.forwarder.abort();
        if let Ok(state) = self.state() {
            end_mcp_session(&state, connection_id);
            state.session_maps.messaging_channels.remove(connection_id);
        }
    }
}

/// The workspace snapshot ego reads instead of polling tools for it.
pub(crate) const WORKSPACE_URI: &str = "tuic://workspace";
/// This peer's unread mail.
pub(crate) const INBOX_URI: &str = "tuic://inbox";
const SUBSCRIPTION_PREFIX: &str = "tuic://inbox/subscriptions/";
fn subscription_id(uri: &str) -> Option<&str> {
    uri.strip_prefix(SUBSCRIPTION_PREFIX)
        .filter(|id| !id.is_empty())
}

/// How long a burst of changes is gathered into one `resources/updated`.
const COALESCE: std::time::Duration = std::time::Duration::from_millis(250);
/// The most unread messages one inbox read carries.
const INBOX_PAGE: usize = 100;

fn resource_list() -> Value {
    json!({ "resources": [
        { "uri": WORKSPACE_URI, "name": "workspace", "mimeType": "application/json",
          "description": "Every repository: name, path, branch, live agents, and the one Boss is viewing" },
        { "uri": INBOX_URI, "name": "inbox", "mimeType": "application/json",
          "description": "Unread peer mail for this ego, in arrival order; `agent inbox` marks it read" },
    ] })
}

fn uri_param(params: Option<&Map<String, Value>>) -> Result<String, McpOverAcpError> {
    params
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| McpOverAcpError::new(INVALID_PARAMS, "missing uri"))
}

fn unknown_resource(uri: &str) -> McpOverAcpError {
    // MCP's "resource not found".
    McpOverAcpError::new(-32002, format!("no resource {uri}"))
}

/// Every repository in sidebar order, the agents running in each, and the one
/// Boss is viewing. `data` is `repositories.json`, passed in so a test can
/// name one.
fn workspace_snapshot(state: &AppState, data: &Value) -> Value {
    let order = data["repoOrder"].as_array().cloned().unwrap_or_default();
    // Agent sessions, by working directory: presence only, so a turn flipping
    // between busy and idle does not change the snapshot.
    let agents: Vec<(String, Value)> = state
        .session_maps
        .session_states
        .iter()
        .filter_map(|entry| {
            let agent_type = entry.value().agent_type.clone()?;
            let session = state.session_maps.sessions.get(entry.key())?;
            let session = session.lock();
            let cwd = session.cwd.clone()?;
            let name = state
                .session_maps
                .term_aliases
                .get(entry.key())
                .map(|alias| alias.value().clone())
                .or_else(|| session.display_name.clone());
            Some((
                cwd,
                json!({ "sessionId": entry.key(), "name": name, "agentType": agent_type }),
            ))
        })
        .collect();
    let repos: Vec<Value> = order
        .iter()
        .filter_map(Value::as_str)
        .map(|path| {
            let info = crate::git::get_repo_info_cached(state, path);
            let display = data["repos"][path]["displayName"]
                .as_str()
                .unwrap_or_default();
            let worktrees = crate::worktree::get_worktree_paths_cached(state, path);
            let inside = |cwd: &str| {
                let cwd = std::path::Path::new(cwd);
                cwd.starts_with(path)
                    || worktrees.as_ref().is_ok_and(|paths| {
                        paths
                            .values()
                            .any(|worktree| cwd.starts_with(&worktree.path))
                    })
            };
            let mut live: Vec<Value> = agents
                .iter()
                .filter(|(cwd, _)| inside(cwd))
                .map(|(_, agent)| agent.clone())
                .collect();
            live.sort_by(|a, b| a["sessionId"].as_str().cmp(&b["sessionId"].as_str()));
            json!({
                "name": if display.is_empty() { info.name.as_str() } else { display },
                "path": path,
                "branch": (!info.branch.is_empty()).then_some(&info.branch),
                "liveAgents": live,
                "worktreeError": worktrees.as_ref().err(),
            })
        })
        .collect();
    json!({ "repos": repos, "viewedRepo": data["activeRepoPath"].as_str() })
}

/// This peer's unread mail, without marking any of it read.
fn inbox_snapshot(state: &AppState, peer_id: Option<&str>) -> Value {
    let Some(peer_id) = peer_id else {
        return json!({ "messages": [], "nextSince": 0 });
    };
    let since = state
        .agent_read_cursor
        .get(peer_id)
        .map_or(0, |cursor| *cursor.value());
    let messages: Vec<Value> = state
        .agent_inbox
        .get(peer_id)
        .map(|inbox| {
            inbox
                .iter()
                .filter(|message| message.timestamp > since)
                .take(INBOX_PAGE)
                .map(|message| {
                    json!({
                        "id": message.id,
                        "from": { "session": message.from_tuic_session, "name": message.from_name },
                        "body": message.content,
                        "timestamp": message.timestamp,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let next_since = messages
        .last()
        .and_then(|message| message["timestamp"].as_u64())
        .unwrap_or(since);
    json!({ "messages": messages, "nextSince": next_since })
}

/// Whether an application event can change what the workspace snapshot says.
fn touches_workspace(event: &crate::state::AppEvent) -> bool {
    use crate::state::AppEvent;
    matches!(
        event,
        AppEvent::HeadChanged { .. }
            | AppEvent::RepositoriesChanged
            | AppEvent::SessionCreated { .. }
            | AppEvent::SessionClosed { .. }
            | AppEvent::PtyExit { .. }
            | AppEvent::SessionStateChanged { .. }
            | AppEvent::SessionWorktreeDeclared(_)
            | AppEvent::WorktreeCreated(_)
            | AppEvent::WorktreeRemoved(_)
            | AppEvent::TermAliasAssigned { .. }
    )
}

/// Everything one MCP connection listens to.
struct Feeds {
    tools: broadcast::Receiver<()>,
    messages: broadcast::Receiver<String>,
    events: broadcast::Receiver<crate::state::AppEvent>,
    inbox: Option<tokio::sync::watch::Receiver<u64>>,
    /// The peer whose inbox `inbox` watches.
    peer: Option<String>,
}

/// Carry what GET `/mcp` would have streamed to ego, and announce subscribed
/// resources when they change, until the connection ends.
///
/// A resource is announced once per burst: the first change arms a deadline,
/// later ones ride on it. The workspace is also compared with what was last
/// announced, because most events that could change it do not.
async fn forward(
    state: Weak<AppState>,
    mut feeds: Feeds,
    subscriptions: Subscriptions,
    notify: McpNotify,
    wake: Option<Box<dyn Fn() -> WakeResult + Send>>,
) {
    use broadcast::error::RecvError;
    let updated = |uri: &str| {
        let mut params = Map::new();
        params.insert("uri".to_owned(), json!(uri));
        notify("notifications/resources/updated".to_owned(), Some(params));
    };
    let subscribed = |uri: &str| subscriptions.lock().contains(uri);
    let mut workspace_due: Option<tokio::time::Instant> = None;
    let mut inbox_due: Option<tokio::time::Instant> = None;
    let mut wake_due: Option<tokio::time::Instant> = None;
    let mut last_workspace: Option<Value> = None;
    let mut notified_subscriptions = std::collections::HashMap::new();
    let mut woken_subscriptions = std::collections::HashMap::new();
    let arm = |due: &mut Option<tokio::time::Instant>| {
        due.get_or_insert_with(|| tokio::time::Instant::now() + COALESCE);
    };
    let until = |due: Option<tokio::time::Instant>| async move {
        match due {
            Some(deadline) => tokio::time::sleep_until(deadline).await,
            None => std::future::pending().await,
        }
    };
    loop {
        tokio::select! {
            changed = feeds.tools.recv() => match changed {
                Ok(()) | Err(RecvError::Lagged(_)) => {
                    let Some(state) = state.upgrade() else { return };
                    super::mcp_transport_catalogue::rebuild_tool_search_index(&state);
                    notify("notifications/tools/list_changed".to_owned(), None);
                }
                Err(RecvError::Closed) => return,
            },
            message = feeds.messages.recv() => match message {
                Ok(text) => {
                    let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                        tracing::warn!(source = "acp_mcp", "dropped a channel frame that is not JSON");
                        continue;
                    };
                    let Some(method) = frame["method"].as_str() else {
                        continue;
                    };
                    notify(method.to_owned(), frame.get("params").and_then(Value::as_object).cloned());
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            event = feeds.events.recv() => match event {
                Ok(event) if touches_workspace(&event) && subscribed(WORKSPACE_URI) => arm(&mut workspace_due),
                Ok(_) => {}
                // Missed events could have been anything: look again.
                Err(RecvError::Lagged(_)) => if subscribed(WORKSPACE_URI) { arm(&mut workspace_due) },
                Err(RecvError::Closed) => return,
            },
            changed = async {
                match feeds.inbox.as_mut() {
                    Some(inbox) => inbox.changed().await,
                    None => std::future::pending().await,
                }
            } => match changed {
                Ok(()) => {
                    if subscriptions.lock().iter().any(|uri| uri == INBOX_URI || subscription_id(uri).is_some()) {
                        arm(&mut inbox_due);
                    }
                    // Armed whether or not ego subscribed: an idle ego is not
                    // running, so it cannot read a notification.
                    wake_due.get_or_insert_with(|| tokio::time::Instant::now() + WAKE_DEBOUNCE);
                }
                Err(_) => feeds.inbox = None,
            },
            () = until(workspace_due) => {
                workspace_due = None;
                let Some(state) = state.upgrade() else { return };
                let snapshot = workspace_snapshot(&state, &crate::config::load_repositories());
                if last_workspace.as_ref() != Some(&snapshot) {
                    last_workspace = Some(snapshot);
                    updated(WORKSPACE_URI);
                }
            },
            () = until(inbox_due) => {
                inbox_due = None;
                if subscribed(INBOX_URI) { updated(INBOX_URI); }
                let uris: Vec<_> = subscriptions.lock().iter().filter(|uri| subscription_id(uri).is_some()).cloned().collect();
                notified_subscriptions.retain(|uri, _| uris.contains(uri));
                let Some(state) = state.upgrade() else { return };
                if let Some(peer) = feeds.peer.as_deref() {
                    for uri in uris {
                        let batch = crate::mail_subscriptions::read(&state,peer,&json!({"action":"inbox","subscription_id":subscription_id(&uri)})).await;
                        let cursor = batch["cursor"].as_u64();
                        if (crate::mail_subscriptions::pending(&batch) || batch.get("error").is_some()) && notified_subscriptions.get(&uri) != Some(&cursor) {
                            notified_subscriptions.insert(uri.clone(),cursor);
                            updated(&uri);
                        }
                    }
                }
            },
            () = until(wake_due) => {
                wake_due = None;
                let Some(state) = state.upgrade() else { return };
                let uris: Vec<_> = subscriptions.lock().iter().filter(|uri| subscription_id(uri).is_some()).cloned().collect();
                woken_subscriptions.retain(|uri, _| uris.contains(uri));
                let mut unread = false;
                let mut candidates = Vec::new();
                if uris.is_empty() {
                    unread = feeds.peer.as_deref().is_some_and(|peer| {
                        inbox_snapshot(&state, Some(peer))["messages"].as_array().is_some_and(|messages| !messages.is_empty())
                    });
                } else if let Some(peer) = feeds.peer.as_deref() {
                    // A durable resource selection also filters the idle ego wake.
                    for uri in uris {
                        let batch = crate::mail_subscriptions::read(&state,peer,&json!({"action":"inbox","subscription_id":subscription_id(&uri)})).await;
                        let cursor = batch["cursor"].as_u64();
                        if crate::mail_subscriptions::pending(&batch) && woken_subscriptions.get(&uri) != Some(&cursor) {
                            candidates.push((uri,cursor));
                            unread = true;
                        }
                    }
                }
                if unread && let Some(wake) = &wake && wake().await {
                    // An attempted wake can be skipped while ego is busy. Only
                    // an accepted turn consumes this frozen batch's wake.
                    woken_subscriptions.extend(candidates);
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_state;
    use super::*;

    const PEER: &str = "550e8400-e29b-41d4-a716-446655440a01";

    type Heard = Arc<Mutex<Vec<(String, Option<Map<String, Value>>)>>>;

    fn listener() -> (McpNotify, Heard) {
        let heard: Heard = Arc::default();
        let sink = Arc::clone(&heard);
        (
            Arc::new(move |method, params| sink.lock().push((method, params))),
            heard,
        )
    }

    fn object(value: Value) -> Option<Map<String, Value>> {
        value.as_object().cloned()
    }

    /// ego's per-request identity, as it sends it on every MCP request.
    fn ego_meta() -> Value {
        json!({ "io.modelcontextprotocol/clientInfo": { "name": "ego", "version": "test" } })
    }

    fn names(result: &Value) -> Vec<String> {
        let mut names: Vec<String> = result["tools"]
            .as_array()
            .expect("a tools list")
            .iter()
            .map(|tool| tool["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        names.sort();
        names
    }

    async fn eventually(what: &str, ready: impl Fn() -> bool) {
        for _ in 0..500 {
            if ready() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("{what} within 5s");
    }

    /// One registry, one surface: what ego lists over ACP is what HTTP `/mcp`
    /// lists for the same client, and the MCP connection carries ego's peer.
    #[tokio::test]
    async fn an_acp_connection_reaches_the_http_handler_under_egos_identity() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");

        assert_eq!(
            state
                .mcp
                .to_session
                .get(&id)
                .map(|peer| peer.value().clone()),
            Some(PEER.to_owned()),
            "the MCP connection is bound to the identity ego was launched under"
        );

        let over_acp = host
            .message(
                &id,
                "tools/list".to_owned(),
                object(json!({ "_meta": ego_meta() })),
            )
            .await
            .expect("tools/list over ACP");
        let over_http = mcp_post(
            State(Arc::clone(&state)),
            ConnectInfo("127.0.0.1:0".parse().unwrap()),
            HeaderMap::new(),
            Json(json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/list",
                "params": { "_meta": ego_meta() }
            })),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(over_http.into_body(), usize::MAX)
            .await
            .unwrap();
        let over_http: Value = serde_json::from_slice(&body).unwrap();
        assert!(!names(&over_acp).is_empty(), "{over_acp}");
        assert_eq!(names(&over_acp), names(&over_http["result"]));

        let peers = host
            .message(
                &id,
                "tools/call".to_owned(),
                object(json!({ "name": "agent", "arguments": { "action": "list_peers" } })),
            )
            .await
            .expect("tools/call over ACP");
        assert!(
            peers.to_string().contains(PEER),
            "a tool call runs as ego's peer: {peers}"
        );
    }

    #[tokio::test]
    async fn an_unknown_method_is_an_mcp_error_not_a_result() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");
        let error = host
            .message(&id, "tools/boom".to_owned(), None)
            .await
            .expect_err("no such method");
        assert_eq!(error.code, -32601);
    }

    /// What GET `/mcp` would have streamed reaches ego as notifications.
    #[tokio::test]
    async fn server_notifications_reach_ego_on_its_connection() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, heard) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");

        let _ = state.mcp.tools_changed.send(());
        eventually("tools/list_changed", || !heard.lock().is_empty()).await;
        assert_eq!(heard.lock()[0].0, "notifications/tools/list_changed");

        let channel = state
            .session_maps
            .messaging_channels
            .get(&id)
            .map(|sender| sender.value().clone())
            .expect("the connection has a delivery channel, as an SSE stream would");
        let _ = channel.send(
            json!({ "jsonrpc": "2.0", "method": "notifications/claude/channel", "params": { "content": "hi" } })
                .to_string(),
        );
        eventually("the channel notification", || heard.lock().len() == 2).await;
        let (method, params) = heard.lock()[1].clone();
        assert_eq!(method, "notifications/claude/channel");
        assert_eq!(
            params.and_then(|p| p.get("content").cloned()),
            Some(json!("hi"))
        );
    }

    /// Nothing the connection opened survives it.
    #[tokio::test]
    async fn disconnecting_releases_the_protocol_session_and_the_identity() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, heard) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");
        assert!(state.mcp.sessions.contains_key(&id));

        host.disconnect(&id);

        assert!(!state.mcp.sessions.contains_key(&id), "protocol session");
        assert!(!state.mcp.to_session.contains_key(&id), "identity route");
        assert!(
            !state.peer_agents.contains_key(PEER),
            "peer with no transport left"
        );
        assert!(
            !state.session_maps.messaging_channels.contains_key(&id),
            "delivery channel"
        );
        let _ = state.mcp.tools_changed.send(());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            heard.lock().is_empty(),
            "a released connection hears nothing"
        );
    }

    fn mail(id: &str, body: &str) -> crate::state::AgentMessage {
        crate::state::AgentMessage {
            id: id.to_owned(),
            from_tuic_session: "11111111-2222-4333-8444-555555555555".to_owned(),
            from_name: "orc-1".to_owned(),
            content: body.to_owned(),
            timestamp: 1,
            delivered_via_channel: false,
        }
    }

    async fn read(host: &AcpMcpHost, id: &str, uri: &str) -> Value {
        let result = host
            .message(
                id,
                "resources/read".to_owned(),
                object(json!({ "uri": uri })),
            )
            .await
            .expect("resources/read");
        let text = result["contents"][0]["text"]
            .as_str()
            .expect("a text resource");
        assert_eq!(result["contents"][0]["mimeType"], "application/json");
        serde_json::from_str(text).expect("JSON contents")
    }

    /// Only this channel lists the two ego-only resources, and says it can
    /// subscribe; HTTP `/mcp` answers discovery exactly as before.
    #[tokio::test]
    async fn the_acp_channel_offers_workspace_and_inbox_resources() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");

        let listed = host
            .message(&id, "resources/list".to_owned(), None)
            .await
            .expect("resources/list");
        let uris: Vec<&str> = listed["resources"]
            .as_array()
            .expect("a resource list")
            .iter()
            .filter_map(|resource| resource["uri"].as_str())
            .collect();
        assert_eq!(uris, vec![WORKSPACE_URI, INBOX_URI]);

        let discovered = host
            .message(
                &id,
                "server/discover".to_owned(),
                object(json!({ "_meta": ego_meta() })),
            )
            .await
            .expect("server/discover");
        assert_eq!(
            discovered["capabilities"]["resources"]["subscribe"],
            json!(true)
        );

        let over_http = mcp_post(
            State(Arc::clone(&state)),
            ConnectInfo(loopback()),
            HeaderMap::new(),
            Json(json!({ "jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {} })),
        )
        .await
        .into_response();
        let body = axum::body::to_bytes(over_http.into_body(), usize::MAX)
            .await
            .unwrap();
        let over_http: Value = serde_json::from_slice(&body).unwrap();
        assert!(
            over_http["result"]["capabilities"]
                .get("resources")
                .is_none(),
            "other clients are not offered ego's resources: {over_http}"
        );
    }

    /// ego refuses to admit a server whose `resources/list` omits the
    /// 2026-07-28 result envelope (#1318-abd7): `session/new` fails with
    /// "MCP server `tuicommander`: server configuration is invalid". Every
    /// resource result on this channel must carry all three fields, exactly as
    /// `tools/list` does.
    #[tokio::test]
    async fn every_resource_result_carries_the_result_envelope() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");

        for (method, params) in [
            ("resources/list", None),
            ("resources/read", object(json!({ "uri": WORKSPACE_URI }))),
            ("resources/subscribe", object(json!({ "uri": INBOX_URI }))),
            ("resources/unsubscribe", object(json!({ "uri": INBOX_URI }))),
        ] {
            let result = host
                .message(&id, method.to_owned(), params)
                .await
                .expect(method);
            assert_eq!(result["resultType"], "complete", "{method}: {result}");
            assert_eq!(result["ttlMs"], 0, "{method}: {result}");
            assert_eq!(result["cacheScope"], "private", "{method}: {result}");
        }
    }

    #[test]
    fn the_workspace_names_every_repo_and_the_one_being_viewed() {
        let state = test_state();
        let repos = json!({
            "repos": { "/nowhere/alpha": { "displayName": "Alpha" }, "/nowhere/beta": {} },
            "repoOrder": ["/nowhere/alpha", "/nowhere/beta"],
            "activeRepoPath": "/nowhere/beta",
        });
        let workspace = workspace_snapshot(&state, &repos);
        let paths: Vec<&str> = workspace["repos"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|repo| repo["path"].as_str())
            .collect();
        assert_eq!(paths, vec!["/nowhere/alpha", "/nowhere/beta"]);
        assert_eq!(workspace["repos"][0]["name"], "Alpha");
        assert!(workspace["repos"][0].get("branch").is_some(), "{workspace}");
        assert_eq!(workspace["repos"][0]["liveAgents"], json!([]));
        assert_eq!(workspace["viewedRepo"], "/nowhere/beta");
    }

    /// Reading the inbox shows unread mail and leaves the cursor alone: the
    /// `agent inbox` tool is still what marks it read.
    #[tokio::test]
    async fn the_inbox_resource_shows_unread_mail_without_reading_it() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");
        state.push_agent_inbox(PEER, mail("m-1", "first"));
        state.push_agent_inbox(PEER, mail("m-2", "second"));

        let inbox = read(&host, &id, INBOX_URI).await;
        let bodies: Vec<&str> = inbox["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|message| message["body"].as_str())
            .collect();
        assert_eq!(bodies, vec!["first", "second"]);
        assert_eq!(inbox["messages"][0]["id"], "m-1");
        assert_eq!(inbox["messages"][0]["from"]["name"], "orc-1");
        assert!(inbox["nextSince"].as_u64().is_some(), "{inbox}");
        assert!(
            state.agent_read_cursor.get(PEER).is_none(),
            "a resource read must not mark mail read"
        );
        assert_eq!(
            read(&host, &id, INBOX_URI).await["messages"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    /// A subscribed resource is announced when it changes; an unsubscribed one
    /// is not, so ego is never told about what it did not ask for.
    #[tokio::test]
    async fn subscribed_resources_announce_their_changes() {
        let state = test_state();
        let host = AcpMcpHost::new(&state);
        let (notify, heard) = listener();
        let id = host.connect(Some(PEER), notify).expect("mcp/connect");
        let updated = |uri: &str| {
            heard
                .lock()
                .iter()
                .filter(|(method, params)| {
                    method == "notifications/resources/updated"
                        && params.as_ref().and_then(|p| p.get("uri")) == Some(&json!(uri))
                })
                .count()
        };

        state.push_agent_inbox(PEER, mail("m-0", "before subscribing"));
        let _ = state
            .event_bus
            .send(crate::state::AppEvent::RepositoriesChanged);
        tokio::time::sleep(COALESCE * 3).await;
        assert_eq!((updated(INBOX_URI), updated(WORKSPACE_URI)), (0, 0));

        for uri in [INBOX_URI, WORKSPACE_URI] {
            host.message(
                &id,
                "resources/subscribe".to_owned(),
                object(json!({ "uri": uri })),
            )
            .await
            .expect("resources/subscribe");
        }
        state.push_agent_inbox(PEER, mail("m-1", "for ego"));
        state.push_agent_inbox(
            "99999999-2222-4333-8444-555555555555",
            mail("m-x", "not for ego"),
        );
        eventually("the inbox update", || updated(INBOX_URI) == 1).await;

        for _ in 0..5 {
            let _ = state
                .event_bus
                .send(crate::state::AppEvent::RepositoriesChanged);
        }
        eventually("the workspace update", || updated(WORKSPACE_URI) >= 1).await;
        tokio::time::sleep(COALESCE * 3).await;
        assert_eq!(
            updated(WORKSPACE_URI),
            1,
            "a burst of changes is one update"
        );
        assert_eq!(updated(INBOX_URI), 1, "mail for another peer is not ego's");
    }

    /// A burst of mail wakes an idle ego once; mail it already read wakes
    /// nothing.
    #[tokio::test]
    async fn mail_wakes_ego_once_per_burst_and_not_after_it_was_read() {
        let state = test_state();
        let woken: Arc<Mutex<Vec<String>>> = Arc::default();
        let host = AcpMcpHost {
            wake: {
                let woken = Arc::clone(&woken);
                Arc::new(move |peer| {
                    woken.lock().push(peer);
                    Box::pin(async { true })
                })
            },
            ..AcpMcpHost::new(&state)
        };
        let (notify, _) = listener();
        let _id = host.connect(Some(PEER), notify).expect("mcp/connect");

        for n in 0..3 {
            state.push_agent_inbox(PEER, mail(&format!("m-{n}"), "burst"));
        }
        eventually("the wake", || !woken.lock().is_empty()).await;
        tokio::time::sleep(WAKE_DEBOUNCE + COALESCE).await;
        assert_eq!(
            *woken.lock(),
            vec![PEER.to_owned()],
            "one wake for the burst"
        );

        let last = state.push_agent_inbox(PEER, mail("m-read", "read at once"));
        state.agent_read_cursor.insert(PEER.to_owned(), last);
        tokio::time::sleep(WAKE_DEBOUNCE + COALESCE).await;
        assert_eq!(woken.lock().len(), 1, "mail already read wakes nothing");
    }

    /// ego admits the session only if every answer the host gives parses under
    /// the 2026-07-28 result envelope: it calls `server/discover`, `tools/list`
    /// and `resources/list`, and refuses `session/new` when any of them is
    /// malformed. A `resources/list` without `resultType`/`ttlMs`/`cacheScope`
    /// passed every test of our own answers and still made AI Chat unusable
    /// (#1318-abd7).
    ///
    /// Needs a real ego binary, which this repository does not build: run with
    /// `TUIC_EGO_BIN=<path to ego> cargo nextest run --run-ignored only -E
    /// 'test(ego_admits_the_session)'`. ego runs under a throwaway
    /// `HOME`/`EGO_HOME` so it reads none of the developer's configuration.
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "needs a real ego binary: set TUIC_EGO_BIN"]
    async fn ego_admits_the_session_the_host_answers() {
        use crate::acp::{AcpConnectRequest, AcpSessionAuthority, EgoAcpConfig};
        use std::os::unix::fs::PermissionsExt;

        let ego = std::env::var("TUIC_EGO_BIN").expect("TUIC_EGO_BIN names a real ego binary");
        let root = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).expect("root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&workspace).expect("workspace");
        let launcher = root.path().join("ego-isolated.sh");
        std::fs::write(
            &launcher,
            format!(
                "#!/bin/sh\nHOME='{home}' EGO_HOME='{home}' exec '{ego}' \"$@\"\n",
                home = home.display()
            ),
        )
        .expect("launcher");
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("launcher mode");

        let state = test_state();
        install(&state);
        let connection = state
            .acp
            .connect_with_peer(
                &EgoAcpConfig {
                    executable: launcher,
                    profile: String::new(),
                },
                AcpConnectRequest {
                    root: workspace.clone(),
                },
                PEER.to_owned(),
            )
            .await
            .expect("connect to ego");

        let session = state
            .acp
            .new_session(
                connection.connection_id,
                AcpSessionAuthority {
                    cwd: workspace,
                    additional_directories: Vec::new(),
                    mcp_servers: Vec::new(),
                },
            )
            .await
            .expect("ego admits the tuicommander MCP server");
        assert_ne!(session.session_id.to_string(), "");

        state
            .acp
            .disconnect(connection.connection_id)
            .await
            .expect("disconnect");
    }
    // Catches: a durable resource still forwards idle lifecycle notices, or an
    // acknowledged RESULT wakes ego again after its MCP connection reconnects.
    #[tokio::test]
    async fn actionable_subscription_acp_filters_notifications_and_wakes_once() {
        let state = test_state();
        let woken: Arc<Mutex<Vec<String>>> = Arc::default();
        let host = AcpMcpHost {
            state: Arc::downgrade(&state),
            connections: Mutex::default(),
            wake: {
                let woken = woken.clone();
                Arc::new(move |peer| {
                    woken.lock().push(peer);
                    Box::pin(async { true })
                })
            },
        };
        let (notify, heard) = listener();
        let id = host.connect(Some(PEER), notify).unwrap();
        let create = crate::mail_subscriptions::action(
            &state,
            PEER,
            &json!({"action":"subscribe","subscription_id":"acp"}),
        );
        assert!(create.get("error").is_none(), "{create}");
        let uri = "tuic://inbox/subscriptions/acp";
        host.message(
            &id,
            "resources/subscribe".into(),
            object(json!({"uri":uri})),
        )
        .await
        .unwrap();
        let fixture: Vec<Value> =
            serde_json::from_str(include_str!("fixtures/actionable-lifecycle-mail.json")).unwrap();
        for m in fixture {
            state.push_agent_inbox(
                PEER,
                crate::state::AgentMessage {
                    id: m["id"].as_str().unwrap().into(),
                    from_tuic_session: m["from_tuic_session"].as_str().unwrap().into(),
                    from_name: "tuic".into(),
                    content: m["content"].as_str().unwrap().into(),
                    timestamp: m["timestamp"].as_u64().unwrap(),
                    delivered_via_channel: false,
                },
            );
        }
        // Let both production debounce paths run before asserting no wake.
        tokio::time::sleep(WAKE_DEBOUNCE + COALESCE * 2).await;
        assert!(woken.lock().is_empty(), "routine traffic must not wake ego");
        let updates = || {
            heard
                .lock()
                .iter()
                .filter(|(method, params)| {
                    method == "notifications/resources/updated"
                        && params.as_ref().and_then(|p| p.get("uri")) == Some(&json!(uri))
                })
                .count()
        };
        assert_eq!(updates(), 0);
        state.push_agent_inbox(PEER, mail("durable-acp-result", "RESULT acp delivery"));
        eventually("one actionable notification", || updates() == 1).await;
        eventually("one actionable wake", || woken.lock().len() == 1).await;
        let batch = read(&host, &id, uri).await;
        assert_eq!(batch["count"], 1);
        let ack = crate::mail_subscriptions::action(
            &state,
            PEER,
            &json!({"action":"ack","subscription_id":"acp","cursor":batch["cursor"]}),
        );
        assert!(ack.get("error").is_none(), "{ack}");
        host.disconnect(&id);
        let (notify, _) = listener();
        let resumed = host.connect(Some(PEER), notify).unwrap();
        host.message(
            &resumed,
            "resources/subscribe".into(),
            object(json!({"uri":uri})),
        )
        .await
        .unwrap();
        assert_eq!(read(&host, &resumed, uri).await["count"], 0);
        tokio::time::sleep(WAKE_DEBOUNCE + COALESCE).await;
        assert_eq!(
            woken.lock().len(),
            1,
            "acknowledged RESULT must not wake after reconnect"
        );
        host.disconnect(&resumed);
    }
    // Catches: a skipped busy wake consumes the frozen cursor and suppresses a later accepted turn.
    #[tokio::test]
    async fn actionable_subscription_skipped_busy_wake_remains_eligible() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let state = test_state();
        let busy = Arc::new(AtomicBool::new(true));
        let attempts = Arc::new(AtomicUsize::new(0));
        let accepted = Arc::new(AtomicUsize::new(0));
        let host = AcpMcpHost {
            wake: {
                let busy = busy.clone();
                let attempts = attempts.clone();
                let accepted = accepted.clone();
                Arc::new(move |_| {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    let idle = !busy.load(Ordering::SeqCst);
                    if idle {
                        accepted.fetch_add(1, Ordering::SeqCst);
                    }
                    Box::pin(async move { idle })
                })
            },
            ..AcpMcpHost::new(&state)
        };
        let (notify, _) = listener();
        let id = host.connect(Some(PEER), notify).unwrap();
        let created = crate::mail_subscriptions::action(
            &state,
            PEER,
            &json!({"action":"subscribe","subscription_id":"busy"}),
        );
        assert!(created.get("error").is_none(), "{created}");
        let uri = "tuic://inbox/subscriptions/busy";
        host.message(
            &id,
            "resources/subscribe".into(),
            object(json!({"uri":uri})),
        )
        .await
        .unwrap();
        state.push_agent_inbox(PEER, mail("busy-result", "RESULT while busy"));
        eventually("first skipped attempt", || {
            attempts.load(Ordering::SeqCst) == 1
        })
        .await;
        let frozen = read(&host, &id, uri).await;
        assert_eq!(accepted.load(Ordering::SeqCst), 0);
        busy.store(false, Ordering::SeqCst);
        state.push_agent_inbox(PEER, mail("idle-result", "RESULT after idle"));
        assert_eq!(read(&host, &id, uri).await["cursor"], frozen["cursor"]);
        eventually("accepted wake after skipped attempt", || {
            accepted.load(Ordering::SeqCst) == 1
        })
        .await;
        state.push_agent_inbox(PEER, mail("later-result", "RESULT same frozen cursor"));
        tokio::time::sleep(WAKE_DEBOUNCE + COALESCE).await;
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        host.disconnect(&id);
    }
}
