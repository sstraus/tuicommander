//! A connected remote machine's sessions, mirrored onto this one.
//!
//! The desktop renders idle / busy / question badges from two things: the rows
//! `list_active_sessions` returns, and the `session-state-changed` push that
//! moves them between polls. A remote daemon runs its own accumulator and
//! publishes exactly the same pair on its own `/events` — so parity needs no
//! second state machine here, only a pipe.
//!
//! That is what this module is. Per connected connection one task:
//!
//! * seeds from the daemon's `GET /sessions`, so a tab that was already running
//!   when we connected has a badge before anything moves;
//! * consumes the daemon's `/events` with **no** `types=` filter and repeats
//!   every frame on the local bus, with the same dual-emit a local producer
//!   uses. An unfiltered stream is the point: a future event type crosses with
//!   no change here.
//!
//! A mirrored row carries `connection_id`, which is the only field that tells
//! it apart from a local one. Everything downstream — the badge, the
//! notification, the queue gate — is the local code path, unchanged.
//!
//! The daemon's session ids are UUIDs minted on the daemon, so they cannot
//! collide with this machine's.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures_util::StreamExt;

use crate::AppState;
use crate::mcp_http::types::SessionInfo;
use crate::state::AppEvent;

/// Wait before re-seeding after the event stream ends. A daemon restart, a
/// dropped tunnel and a network blip all land here; the status poll in
/// `remote_runtime` is what decides whether the connection is still up, so this
/// only has to avoid a hot loop.
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Timeout for the one-shot `GET /sessions` seed. The event stream gets a
/// read-idle deadline instead — see [`STREAM_IDLE_TIMEOUT`].
const SEED_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the event stream may say nothing before it is treated as dead.
///
/// A whole-request timeout would be wrong — the stream is long-lived on purpose
/// — but no deadline at all was worse: `stream.next()` on a half-open socket is
/// pending forever, so a laptop that slept, a NAT that dropped its entry or a
/// tunnel killed mid-flight left the mirror parked on a connection that would
/// never speak again, holding rows nothing would ever update.
///
/// Three missed keep-alives. The daemon's `/events` sends one every 15s
/// (`sse_routes.rs`), so real silence this long is not a quiet machine.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// Sessions running on remote machines, keyed by connection id then session id.
///
/// A map per connection rather than one flat map because a disconnect has to
/// drop exactly one machine's rows, and "which machine ran this" is not
/// recoverable from a session id.
#[derive(Default)]
pub(crate) struct RemoteSessions {
    by_connection: DashMap<String, HashMap<String, SessionInfo>>,
}

/// Every mirrored session, as session-list rows.
pub(crate) fn mirrored_rows(state: &AppState) -> Vec<SessionInfo> {
    state
        .remote_sessions
        .by_connection
        .iter()
        .flat_map(|entry| entry.value().values().cloned().collect::<Vec<_>>())
        .collect()
}

/// The daemon that owns a mirrored PTY, if the session is still advertised.
pub(crate) fn owner_connection(state: &AppState, session_id: &str) -> Option<String> {
    state
        .remote_sessions
        .by_connection
        .iter()
        .find(|entry| entry.value().contains_key(session_id))
        .map(|entry| entry.key().clone())
}

/// Replace one connection's rows with what its daemon just reported.
///
/// A replace rather than a merge: the daemon's answer is the whole truth about
/// that machine, so a session it no longer lists is gone.
fn store_seed(state: &AppState, connection_id: &str, rows: Vec<SessionInfo>) {
    let rows = rows
        .into_iter()
        .map(|mut row| {
            row.connection_id = Some(connection_id.to_string());
            (row.session_id.clone(), row)
        })
        .collect();
    state
        .remote_sessions
        .by_connection
        .insert(connection_id.to_string(), rows);
}

/// Put rows in the mirror without a daemon, so a router test can prove the HTTP
/// transport carries them.
#[cfg(test)]
pub(crate) fn store_seed_for_test(state: &AppState, connection_id: &str, rows: Vec<SessionInfo>) {
    store_seed(state, connection_id, rows);
}

/// Read the daemon's session list.
async fn seed(
    client: &reqwest::Client,
    base_url: &str,
    token: Option<&str>,
) -> Result<Vec<SessionInfo>, String> {
    let url = format!("{}/sessions", base_url.trim_end_matches('/'));
    let mut request = client.get(&url).timeout(SEED_TIMEOUT);
    if let Some(token) = token {
        request = request.header(
            reqwest::header::COOKIE,
            format!("{}={token}", crate::mcp_http::auth::SESSION_COOKIE),
        );
    }
    let response = request
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("GET /sessions answered {}", response.status()));
    }
    response
        .json()
        .await
        .map_err(|e| e.without_url().to_string())
}

/// Stamped into every mirrored payload, and the reason a mirrored event cannot
/// cross a second hop.
///
/// Its *presence* is the whole rule: `apply_frame` drops a frame that already
/// carries one, so a payload can never be marked twice and a hop count would
/// always read 1. Two machines pointed at each other, or a Direct connection to
/// this machine's own daemon, therefore stop after one repeat instead of
/// looping forever — the self-connection check in `remote_runtime::connect`
/// refuses the second case outright, this is what holds when the loop runs
/// through a third machine it cannot see.
pub(crate) const ORIGIN_MARKER: &str = "__tuic_origin";

/// Mirrored event names the desktop window is allowed to hear.
///
/// The window emit uses the daemon's own event name, so an unfiltered mirror
/// hands a remote machine's `session-created`, `ui-tab`, `worktree-created`,
/// `worktree-removed` and `repo-changed` to handlers that mutate LOCAL state: a
/// phantom tab per remote session attached to the local transport, a workspace
/// written into the local repositories store, git work spawned for a path that
/// does not exist here. The allowed notices are session-scoped and idempotent
/// against a session this client already knows about — the badge push, and the
/// close that retires it, Progress/workflow receipts, and labelled MCP toasts.
/// Everything else still reaches the local bus, where
/// `state.rs` ignores `RemoteMirrored`, and `/events`, where a client that
/// asked for the mirror wants it.
#[cfg_attr(all(not(feature = "desktop"), not(test)), allow(dead_code))]
const WINDOW_MIRRORABLE_EVENTS: [&str; 16] = [
    "session-state-changed",
    "settings-changed",
    "session-closed",
    "progress-recorded",
    "workflow-run-changed",
    "mcp-toast",
    "mcp-confirm",
    "mcp-confirm-resolved",
    "review-progress",
    "proposals-ready",
    "conflict-assist-status",
    "upstream-status-changed",
    "acp-notice",
    "github-transition",
    "github-pr-update",
    "github-issues-update",
];

/// Whether a mirrored event may be repeated on the desktop window.
#[cfg_attr(all(not(feature = "desktop"), not(test)), allow(dead_code))]
fn window_may_hear(event: &str) -> bool {
    WINDOW_MIRRORABLE_EVENTS.contains(&event)
}

/// One decoded SSE frame.
struct Frame {
    event: String,
    data: String,
}

/// Feed raw SSE bytes in, get whole frames out.
///
/// Hand-written rather than an SSE client crate for one reason: the frames are
/// republished verbatim, so nothing here needs to understand them, and a
/// dependency that parses into its own event type would only have to be undone.
/// Comment lines (`:` keep-alives) and `id:` are dropped — the daemon's ids are
/// its own counter and mean nothing on this machine.
///
/// The buffer holds **bytes**, not text, and a line is decoded only once it is
/// whole. Decoding each network chunk first is what a naive version did, and a
/// multibyte character that straddled two chunks was replaced with U+FFFD
/// before the decoder ever saw it — a corrupted question, tab title or path,
/// with nothing downstream able to tell it apart from what the daemon sent.
#[derive(Default)]
struct FrameDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl FrameDecoder {
    /// Append a chunk and return every frame it completed.
    fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        // A frame ends at a blank line, so the last (possibly partial) line
        // stays in the buffer until more bytes arrive.
        while let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
            let raw = &self.buffer[..newline];
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            let line = String::from_utf8_lossy(raw).into_owned();
            self.buffer.drain(..=newline);
            if line.is_empty() {
                if let Some(frame) = self.take_frame() {
                    frames.push(frame);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line.as_str(), ""),
            };
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                _ => {}
            }
        }
        frames
    }

    fn take_frame(&mut self) -> Option<Frame> {
        let event = self.event.take();
        let data = std::mem::take(&mut self.data);
        // A frame with no `event:` is a plain `message`, which this backend
        // never sends; one with no `data:` carries nothing to republish.
        let event = event?;
        if data.is_empty() {
            return None;
        }
        Some(Frame {
            event,
            data: data.join("\n"),
        })
    }
}

/// Apply one frame: patch the mirrored rows, then repeat it locally.
///
/// Returns true when the row set itself may have moved and a re-seed is owed —
/// a created or closed session changes the list, and the daemon's event carries
/// less than its `GET /sessions` row does.
fn apply_frame(state: &Arc<AppState>, connection_id: &str, frame: &Frame) -> bool {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&frame.data) else {
        tracing::warn!(
            source = "remote",
            connection = connection_id,
            event = %frame.event,
            "Dropping a mirrored event with a body that is not JSON"
        );
        return false;
    };
    // Only an object can be stamped, and an unstampable payload is an
    // unstoppable one: it would cross hop after hop with nothing to mark it.
    // Every event this backend publishes builds a JSON object, so this rejects
    // nothing a daemon of ours sends.
    let Some(body) = payload.as_object() else {
        tracing::warn!(
            source = "remote",
            connection = connection_id,
            event = %frame.event,
            "Dropping a mirrored event whose body is not a JSON object"
        );
        return false;
    };
    if body.contains_key(ORIGIN_MARKER) {
        tracing::debug!(
            source = "remote",
            connection = connection_id,
            event = %frame.event,
            "Dropping an already-mirrored event rather than repeating it a second hop"
        );
        return false;
    }
    // Teardown removes the runtime entry before aborting this task. Do not
    // deliver a toast from a chunk already buffered when that happens.
    if matches!(
        frame.event.as_str(),
        "settings-changed"
            | "mcp-toast"
            | "mcp-confirm"
            | "mcp-confirm-resolved"
            | "review-progress"
            | "proposals-ready"
            | "conflict-assist-status"
            | "upstream-status-changed"
            | "acp-notice"
            | "github-transition"
            | "github-pr-update"
            | "github-issues-update"
    ) && state.remote.base_url(connection_id).is_none()
    {
        return false;
    }
    // A missed daemon frame invalidates only this owner's readers. Never
    // forward `lagged`: local SSE clients would resync unrelated owners too.
    if frame.event == "lagged" {
        crate::remote_runtime::resync_connected(state, connection_id);
        return true;
    }
    let reseed = match frame.event.as_str() {
        "session-state-changed" => {
            patch_state(state, connection_id, &payload);
            false
        }
        "session-created" | "session-closed" | "pty-exit" => true,
        _ => false,
    };
    republish(state, connection_id, &frame.event, payload);
    reseed
}

/// Move a mirrored row's state, so the next `list_active_sessions` agrees with
/// the push that just went out.
fn patch_state(state: &Arc<AppState>, connection_id: &str, payload: &serde_json::Value) {
    let Some(session_id) = payload.get("session_id").and_then(|v| v.as_str()) else {
        return;
    };
    let Ok(session_state) =
        serde_json::from_value::<crate::state::SessionState>(payload["state"].clone())
    else {
        return;
    };
    if let Some(mut rows) = state.remote_sessions.by_connection.get_mut(connection_id)
        && let Some(row) = rows.get_mut(session_id)
    {
        row.state = Some(session_state);
    }
}

/// Dual-emit, exactly as a local producer does: nothing forwards the bus to the
/// desktop window, so the window listener is fed here and the bus feeds the
/// local `/events` SSE — the IPC/HTTP parity rule in AGENTS.md.
///
/// The desktop event name is the daemon's own, so a window listener registered
/// for `session-state-changed` receives a remote one without knowing it exists.
/// That is also why the window hears only `WINDOW_MIRRORABLE_EVENTS`: a name
/// the local handlers act on locally must not arrive from another machine.
///
/// The payload leaves here stamped with `ORIGIN_MARKER`, which is what stops a
/// third machine from mirroring it onward.
fn republish(state: &Arc<AppState>, connection_id: &str, event: &str, payload: serde_json::Value) {
    let mut payload = payload;
    if let Some(body) = payload.as_object_mut() {
        let mut origin = serde_json::json!({ "connection": connection_id });
        if matches!(
            event,
            "mcp-toast"
                | "mcp-confirm"
                | "mcp-confirm-resolved"
                | "upstream-status-changed"
                | "acp-notice"
                | "github-transition"
                | "github-pr-update"
                | "github-issues-update"
        ) {
            // Names come from this machine's saved connection, never the peer.
            let name = match crate::remote_connection::RemoteConnectionStore::load(&state.data_dir)
            {
                Ok(connections) => connections
                    .into_iter()
                    .find(|connection| connection.id == connection_id)
                    .map(|connection| connection.name),
                Err(error) => {
                    tracing::warn!(source = "remote", connection = connection_id, %error,
                        "Could not read the connection name for a remote notice");
                    None
                }
            };
            origin["name"] = serde_json::json!(name.as_deref().unwrap_or(connection_id));
        }
        if event == "mcp-toast" {
            // MCP identifies the speaking peer; terminal navigation needs its
            // PTY UUID. The daemon's session rows bind the two identities.
            if let Some(session) = body.get("origin_session_id").and_then(|id| id.as_str())
                && let Some(rows) = state.remote_sessions.by_connection.get(connection_id)
                && let Some(row) = rows.values().find(|row| {
                    row.session_id == session || row.tuic_session.as_deref() == Some(session)
                })
            {
                body.insert(
                    "origin_session_id".into(),
                    serde_json::json!(row.session_id),
                );
            }
        }
        body.insert(ORIGIN_MARKER.to_string(), origin);
    }
    #[cfg(feature = "desktop")]
    if window_may_hear(event)
        && let Some(app) = state.app_handle.read().as_ref()
    {
        use tauri::Emitter;
        let _ = app.emit(event, &payload);
    }
    let _ = state.event_bus.send(AppEvent::RemoteMirrored {
        connection_id: connection_id.to_string(),
        event: event.to_string(),
        payload,
    });
}

/// Consume the daemon's `/events` until it ends, errors, or goes quiet for
/// `idle`.
///
/// No `types=`: the allowlist is the one thing that would have to be edited
/// every time an event type is added, and a filtered mirror is a mirror that
/// silently lags the local machine.
///
/// `idle` is a parameter rather than the constant so a test can prove the
/// deadline without waiting 45 seconds for it.
async fn consume_stream(
    state: &Arc<AppState>,
    client: &reqwest::Client,
    connection_id: &str,
    base_url: &str,
    token: Option<&str>,
    idle: Duration,
) -> Result<(), String> {
    let url = format!("{}/events", base_url.trim_end_matches('/'));
    let mut request = client.get(&url);
    if let Some(token) = token {
        request = request.header(
            reqwest::header::COOKIE,
            format!("{}={token}", crate::mcp_http::auth::SESSION_COOKIE),
        );
    }
    let response = request
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("GET /events answered {}", response.status()));
    }
    // Health probes can stay connected across an SSE-only interruption or a
    // daemon restart. Each successful subscription starts a fresh hint epoch.
    if state.remote.base_url(connection_id).as_deref() == Some(base_url) {
        crate::remote_runtime::resync_connected(state, connection_id);
    }
    let mut decoder = FrameDecoder::default();
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::time::timeout(idle, stream.next()).await.map_err(|_| {
            format!(
                "the daemon sent nothing for {}s, past its own keep-alive — treating the stream as dead",
                idle.as_secs_f32()
            )
        })?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|e| e.without_url().to_string())?;
        for frame in decoder.push(&chunk) {
            if apply_frame(state, connection_id, &frame) {
                match seed(client, base_url, token).await {
                    Ok(rows) => store_seed(state, connection_id, rows),
                    Err(e) => tracing::warn!(
                        source = "remote",
                        connection = connection_id,
                        %e,
                        "Could not re-read the remote session list"
                    ),
                }
            }
        }
    }
    Ok(())
}

/// Run the mirror for one connection until the task is aborted.
///
/// `retry_delay` is a parameter rather than the constant so a test can prove the
/// re-seed ordering without waiting on it: the delay is not what is under test,
/// and a test that sleeps on the real one is a test of how fast this machine is.
async fn run(state: Arc<AppState>, connection_id: String, retry_delay: Duration) {
    // The runtime's client, not a new one: the seed and the probes talk to the
    // same daemon, so they share the same connection pool.
    let client = state.remote.http_client();
    loop {
        let Some(base_url) = state.remote.base_url(&connection_id) else {
            // The connection is no longer connected. `disconnect` aborts this
            // task, so this only happens in the window between the two.
            tokio::time::sleep(retry_delay).await;
            continue;
        };
        let token = state.remote.token(&connection_id);
        match seed(&client, &base_url, token.as_deref()).await {
            Ok(rows) => store_seed(&state, &connection_id, rows),
            Err(e) => {
                tracing::warn!(
                    source = "remote",
                    connection = %connection_id,
                    %e,
                    "Could not read the remote session list"
                );
                tokio::time::sleep(retry_delay).await;
                continue;
            }
        }
        if let Err(e) = consume_stream(
            &state,
            &client,
            &connection_id,
            &base_url,
            token.as_deref(),
            STREAM_IDLE_TIMEOUT,
        )
        .await
        {
            tracing::warn!(
                source = "remote",
                connection = %connection_id,
                %e,
                "Remote event stream ended"
            );
        }
        tokio::time::sleep(retry_delay).await;
    }
}

/// Start mirroring one connection. The handle belongs to the caller, which
/// aborts it on disconnect.
pub(crate) fn spawn(state: &Arc<AppState>, connection_id: String) -> tokio::task::JoinHandle<()> {
    let task_state = Arc::clone(state);
    tokio::spawn(run(task_state, connection_id, RECONNECT_DELAY))
}

/// Forget one connection's sessions.
///
/// Every row is announced closed before it is dropped: a badge is sticky by
/// construction, so a tab whose row simply vanishes keeps rendering the last
/// state the machine was in. `session-closed` is what a local session sends
/// when it goes, and a disconnect is the same news for a mirrored one.
pub(crate) fn drop_connection(state: &Arc<AppState>, connection_id: &str) {
    crate::mcp_http::remote_peer::disconnect(state, connection_id);
    let Some((_, rows)) = state.remote_sessions.by_connection.remove(connection_id) else {
        return;
    };
    for session_id in rows.into_keys() {
        republish(
            state,
            connection_id,
            "session-closed",
            serde_json::json!({ "session_id": session_id, "reason": "remote-disconnected" }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests_support::make_test_app_state;

    fn frames(chunks: &[&str]) -> Vec<(String, String)> {
        frame_bytes(&chunks.iter().map(|c| c.as_bytes()).collect::<Vec<_>>())
    }

    fn frame_bytes(chunks: &[&[u8]]) -> Vec<(String, String)> {
        let mut decoder = FrameDecoder::default();
        let mut out = Vec::new();
        for chunk in chunks {
            for frame in decoder.push(chunk) {
                out.push((frame.event, frame.data));
            }
        }
        out
    }

    #[test]
    fn a_whole_frame_decodes_to_its_event_and_its_body() {
        assert_eq!(
            frames(&["event: pty-activity\ndata: {\"session_id\":\"s1\"}\n\n"]),
            vec![(
                "pty-activity".to_string(),
                "{\"session_id\":\"s1\"}".to_string()
            )]
        );
    }

    #[test]
    fn a_frame_split_across_chunks_is_one_frame() {
        assert_eq!(
            frames(&["event: pty-ac", "tivity\ndata: {\"a\":", "1}\n\n"]),
            vec![("pty-activity".to_string(), "{\"a\":1}".to_string())]
        );
    }

    #[test]
    fn a_keep_alive_comment_produces_nothing() {
        assert_eq!(frames(&[":ping\n\n:ping\n\n"]), vec![]);
    }

    #[test]
    fn an_id_line_is_dropped_and_the_frame_still_decodes() {
        assert_eq!(
            frames(&["id: 7\nevent: pty-exit\ndata: {}\n\n"]),
            vec![("pty-exit".to_string(), "{}".to_string())]
        );
    }

    #[test]
    fn two_frames_in_one_chunk_both_come_out() {
        assert_eq!(
            frames(&["event: a\ndata: 1\n\nevent: b\ndata: 2\n\n"]),
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string())
            ]
        );
    }

    #[test]
    fn a_multi_line_body_is_rejoined_with_newlines() {
        assert_eq!(
            frames(&["event: a\ndata: {\ndata: }\n\n"]),
            vec![("a".to_string(), "{\n}".to_string())]
        );
    }

    /// Chunk boundaries are the network's business, not the daemon's. A
    /// character split across two of them used to be replaced with U+FFFD
    /// before the decoder ever saw it, because each chunk was decoded on
    /// arrival; the buffer holds bytes now, so the question text survives.
    #[test]
    fn a_multibyte_character_split_across_two_chunks_decodes_intact() {
        let body = "event: session-state-changed\ndata: {\"q\":\"è\"}\n\n".as_bytes();
        // `è` is 0xC3 0xA8: cut between its two bytes.
        let split = body
            .windows(2)
            .position(|w| w == [0xC3, 0xA8])
            .expect("the body carries the two-byte character")
            + 1;
        assert_eq!(
            frame_bytes(&[&body[..split], &body[split..]]),
            vec![(
                "session-state-changed".to_string(),
                "{\"q\":\"è\"}".to_string()
            )]
        );
    }

    /// Every emoji, CJK and accented path in the stream, one boundary at a
    /// time: no cut position may change what comes out.
    #[test]
    fn no_chunk_boundary_changes_the_decoded_body() {
        let body = "event: ui-tab\ndata: {\"t\":\"日本語 — café 🎛\"}\n\n".as_bytes();
        let whole = frame_bytes(&[body]);
        for cut in 1..body.len() {
            assert_eq!(
                frame_bytes(&[&body[..cut], &body[cut..]]),
                whole,
                "a cut at byte {cut} changed the frame"
            );
        }
    }

    #[test]
    fn a_seeded_row_is_tagged_with_the_connection_that_owns_it() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let rows = mirrored_rows(&state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].connection_id.as_deref(), Some("vps"));
    }

    #[test]
    fn a_second_seed_replaces_the_rows_it_no_longer_lists() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![
                SessionInfo {
                    session_id: "s1".into(),
                    ..Default::default()
                },
                SessionInfo {
                    session_id: "s2".into(),
                    ..Default::default()
                },
            ],
        );
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s2".into(),
                ..Default::default()
            }],
        );
        let rows = mirrored_rows(&state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, "s2");
    }

    #[test]
    fn a_remote_state_change_moves_the_row_a_list_call_returns() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let frame = Frame {
            event: "session-state-changed".into(),
            data:
                r#"{"session_id":"s1","state":{"awaiting_input":true,"question_text":"pick one"}}"#
                    .into(),
        };
        assert!(!apply_frame(&state, "vps", &frame));
        let rows = mirrored_rows(&state);
        let session_state = rows[0].state.as_ref().expect("the row carries state");
        assert!(session_state.awaiting_input);
        assert_eq!(session_state.question_text.as_deref(), Some("pick one"));
    }

    #[test]
    fn a_mirrored_event_reaches_the_local_bus_under_the_daemons_own_name() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        let frame = Frame {
            event: "session-state-changed".into(),
            data: r#"{"session_id":"s1","state":{"awaiting_input":true}}"#.into(),
        };
        apply_frame(&state, "vps", &frame);
        match rx.try_recv().expect("one event was published") {
            AppEvent::RemoteMirrored {
                connection_id,
                event,
                payload,
            } => {
                assert_eq!(connection_id, "vps");
                assert_eq!(event, "session-state-changed");
                assert_eq!(payload["session_id"], "s1");
            }
            other => panic!("expected a mirrored event, got {other:?}"),
        }
    }

    #[test]
    fn a_created_session_asks_for_a_fresh_list_and_a_state_change_does_not() {
        let state = Arc::new(make_test_app_state());
        assert!(apply_frame(
            &state,
            "vps",
            &Frame {
                event: "session-created".into(),
                data: r#"{"session_id":"s1"}"#.into(),
            }
        ));
        assert!(!apply_frame(
            &state,
            "vps",
            &Frame {
                event: "pty-activity".into(),
                data: r#"{"session_id":"s1"}"#.into(),
            }
        ));
    }

    /// Two hops: A's frame is repeated by B, and what B publishes must not be
    /// repeatable again. Machine C here is a second `apply_frame` fed B's own
    /// output — the shape a chain of three machines, or two pointed at each
    /// other, produces.
    #[test]
    fn a_frame_that_was_already_mirrored_does_not_cross_a_second_hop() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();

        assert!(!apply_frame(
            &state,
            "vps",
            &Frame {
                event: "pty-activity".into(),
                data: r#"{"session_id":"s1"}"#.into(),
            }
        ));
        let AppEvent::RemoteMirrored {
            event,
            payload: first_hop,
            ..
        } = rx.try_recv().expect("the first hop published")
        else {
            panic!("expected a mirrored event");
        };
        assert_eq!(
            first_hop[ORIGIN_MARKER]["connection"], "vps",
            "the first hop must stamp where it came from"
        );

        // Machine C reads B's `/events` and sees exactly this body.
        apply_frame(
            &state,
            "other-machine",
            &Frame {
                event,
                data: first_hop.to_string(),
            },
        );
        assert!(
            rx.try_recv().is_err(),
            "a mirrored frame must not be mirrored again"
        );
    }

    /// The origin marker is only reachable when the body can hold it.
    #[test]
    fn a_body_that_is_json_but_not_an_object_is_dropped() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "vps",
            &Frame {
                event: "pty-activity".into(),
                data: "[1,2,3]".into(),
            },
        );
        assert!(rx.try_recv().is_err(), "nothing was published");
    }

    /// The window emit runs the local handlers. `session-created` there builds
    /// a tab on the local transport for a session this machine does not run;
    /// `repo-changed` and the worktree pair spawn git work for a path that does
    /// not exist here. The badge pair and Progress receipt are safe, and the
    /// bus still carries everything.
    #[test]
    fn only_safe_mirrored_events_reach_the_desktop_window() {
        for event in [
            "session-state-changed",
            "session-closed",
            "progress-recorded",
            "workflow-run-changed",
            "review-progress",
            "proposals-ready",
            "conflict-assist-status",
            "upstream-status-changed",
            "acp-notice",
            "github-transition",
            "github-pr-update",
            "github-issues-update",
        ] {
            assert!(window_may_hear(event), "{event} has a safe window consumer");
        }
        for event in [
            "session-created",
            "ui-tab",
            "worktree-created",
            "worktree-removed",
            "repo-changed",
            "head-changed",
            "repositories-changed",
        ] {
            assert!(
                !window_may_hear(event),
                "{event} mutates local state and must not arrive from a remote daemon"
            );
        }
    }

    // Catches: confirmations lose their locally trusted host label or a buffered
    // question resurfaces after disconnect and can be answered on the wrong host.
    #[test]
    fn remote_confirmations_keep_host_identity_and_drop_after_disconnect() {
        use crate::remote_connection::{RemoteConnection, RemoteConnectionStore};
        let state = Arc::new(make_test_app_state());
        let mut connection = RemoteConnection::new_direct("mint", "http://daemon:9876", "boss");
        connection.id = "vps".into();
        RemoteConnectionStore::save(&state.data_dir, &[connection]).unwrap();
        state
            .remote
            .force_connected_for_test("vps", "http://daemon:9876", None);
        let mut rx = state.event_bus.subscribe();
        for event in ["mcp-confirm", "mcp-confirm-resolved"] {
            let frame = Frame {
                event: event.into(),
                data: r#"{"request_id":"same"}"#.into(),
            };
            apply_frame(&state, "vps", &frame);
            let AppEvent::RemoteMirrored { payload, .. } = rx.try_recv().unwrap() else {
                panic!("missing remote confirmation");
            };
            assert_eq!(
                payload[ORIGIN_MARKER],
                serde_json::json!({"connection":"vps","name":"mint"})
            );
        }
        crate::remote_runtime::teardown(&state, "vps");
        // Teardown itself publishes lifecycle events; drain those before the probe.
        while rx.try_recv().is_ok() {}
        apply_frame(
            &state,
            "vps",
            &Frame {
                event: "mcp-confirm".into(),
                data: r#"{"request_id":"late"}"#.into(),
            },
        );
        assert!(
            rx.try_recv().is_err(),
            "a disconnected question must not reappear"
        );
    }

    // Catches: mcp-toast is excluded from the window, loses its sound/level,
    // trusts the daemon's host label, or navigates using a peer rather than PTY id.
    #[test]
    fn remote_toast_reaches_window_with_local_host_label_and_originating_pty() {
        use crate::remote_connection::{RemoteConnection, RemoteConnectionStore};
        let state = Arc::new(make_test_app_state());
        let mut connection = RemoteConnection::new_direct("mac-mint", "http://daemon:9876", "boss");
        connection.id = "vps".into();
        RemoteConnectionStore::save(&state.data_dir, &[connection]).unwrap();
        state
            .remote
            .force_connected_for_test("vps", "http://daemon:9876", None);
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "pty-1".into(),
                tuic_session: Some("peer-1".into()),
                ..Default::default()
            }],
        );
        let mut rx = state.event_bus.subscribe();
        apply_frame(&state, "vps", &Frame {
            event: "mcp-toast".into(),
            data: r#"{"title":"Need input","message":"Which branch?","level":"warn","sound":"attention","origin_session_id":"peer-1","origin_repo_path":"/remote/repo","name":"forged"}"#.into(),
        });
        let AppEvent::RemoteMirrored { event, payload, .. } = rx.try_recv().unwrap() else {
            panic!("missing mirrored toast");
        };
        assert!(
            window_may_hear(&event),
            "a remote MCP toast must reach the window emit path"
        );
        assert_eq!(
            payload[ORIGIN_MARKER],
            serde_json::json!({"connection":"vps", "name":"mac-mint"})
        );
        assert_eq!(payload["title"], "Need input");
        assert_eq!(payload["message"], "Which branch?");
        assert_eq!(payload["level"], "warn");
        assert_eq!(payload["sound"], "attention");
        assert_eq!(payload["origin_session_id"], "pty-1");
        assert_eq!(payload["origin_repo_path"], "/remote/repo");
        assert!(rx.try_recv().is_err(), "one frame produces one notice");
        apply_frame(
            &state,
            "vps",
            &Frame {
                event,
                data: payload.to_string(),
            },
        );
        assert!(rx.try_recv().is_err(), "a toast cannot cross a second hop");
    }

    // Catches: a buffered toast is delivered after teardown, or replayed on reconnect.
    #[test]
    fn disconnected_toast_is_dropped_and_reconnect_only_delivers_new_frames() {
        let state = Arc::new(make_test_app_state());
        state
            .remote
            .force_connected_for_test("vps", "http://daemon:9876", None);
        crate::remote_runtime::teardown(&state, "vps");
        let mut rx = state.event_bus.subscribe();
        let toast = Frame {
            event: "mcp-toast".into(),
            data: r#"{"title":"Old","level":"error","sound":null}"#.into(),
        };
        apply_frame(&state, "vps", &toast);
        assert!(rx.try_recv().is_err());
        state
            .remote
            .force_connected_for_test("vps", "http://daemon:9876", None);
        assert!(
            rx.try_recv().is_err(),
            "reconnect must not replay dropped notices"
        );
        apply_frame(
            &state,
            "vps",
            &Frame {
                event: "mcp-toast".into(),
                data: r#"{"title":"New","level":"info","sound":null}"#.into(),
            },
        );
        let AppEvent::RemoteMirrored { payload, .. } = rx.try_recv().unwrap() else {
            panic!("missing fresh toast");
        };
        assert_eq!(payload["title"], "New");
        assert_eq!(
            payload[ORIGIN_MARKER]["name"], "vps",
            "missing saved names fall back to the connection id"
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_body_that_is_not_json_is_dropped_rather_than_republished() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "vps",
            &Frame {
                event: "pty-activity".into(),
                data: "not json".into(),
            },
        );
        assert!(rx.try_recv().is_err(), "nothing was published");
    }

    #[test]
    fn disconnecting_closes_every_mirrored_session_before_forgetting_it() {
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let mut rx = state.event_bus.subscribe();
        drop_connection(&state, "vps");
        match rx.try_recv().expect("the row was announced closed") {
            AppEvent::RemoteMirrored { event, payload, .. } => {
                assert_eq!(event, "session-closed");
                assert_eq!(payload["session_id"], "s1");
            }
            other => panic!("expected a mirrored event, got {other:?}"),
        }
        assert!(mirrored_rows(&state).is_empty());
    }

    #[test]
    fn disconnecting_a_connection_that_never_mirrored_anything_says_nothing() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        drop_connection(&state, "vps");
        assert!(rx.try_recv().is_err());
    }

    // Catches: a restarted daemon's reset generation remaining below the old
    // high-water mark because health never observed a disconnection.
    #[tokio::test]
    async fn stream_resubscription_resyncs_only_its_connected_owner() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/events")
            .with_header("content-type", "text/event-stream")
            .with_body("")
            .expect(2)
            .create_async()
            .await;
        let state = Arc::new(make_test_app_state());
        state
            .remote
            .force_connected_for_test("a", &server.url(), None);
        state
            .remote
            .force_connected_for_test("b", "http://other:9876", None);
        let mut rx = state.event_bus.subscribe();
        for _ in 0..2 {
            consume_stream(
                &state,
                &reqwest::Client::new(),
                "a",
                &server.url(),
                None,
                STREAM_IDLE_TIMEOUT,
            )
            .await
            .expect("subscription ended cleanly");
            match rx
                .try_recv()
                .expect("successful subscription resyncs readers")
            {
                AppEvent::RemoteConnectionStatusChanged { payload } => {
                    assert_eq!(payload["id"], "a");
                    assert_eq!(payload["status"], "connected");
                }
                other => panic!("expected owner resync, got {other:?}"),
            }
            assert!(rx.try_recv().is_err(), "other owners must not resync");
        }
        route.assert_async().await;
    }

    // Catches: daemon lag propagating as a global SSE resync or failing to
    // invalidate settings while reseeding only terminal rows.
    #[test]
    fn lagged_resyncs_only_its_owner_and_requests_session_reseed() {
        let state = Arc::new(make_test_app_state());
        state
            .remote
            .force_connected_for_test("a", "http://daemon:9876", None);
        state
            .remote
            .force_connected_for_test("b", "http://other:9876", None);
        let mut rx = state.event_bus.subscribe();
        let frame = Frame {
            event: "lagged".into(),
            data: r#"{"missed":3}"#.into(),
        };
        assert!(apply_frame(&state, "a", &frame));
        match rx.try_recv().expect("lag invalidates the owner") {
            AppEvent::RemoteConnectionStatusChanged { payload } => assert_eq!(payload["id"], "a"),
            other => panic!("expected owner resync, got {other:?}"),
        }
        assert!(
            rx.try_recv().is_err(),
            "no global lagged event or other owner resync"
        );
        apply_frame(&state, "disconnected", &frame);
        assert!(
            rx.try_recv().is_err(),
            "disconnected owners stay disconnected"
        );
    }

    #[tokio::test]
    async fn the_seed_signs_its_call_and_returns_the_daemons_own_rows() {
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/sessions")
            .match_header("cookie", "tui-session=tok")
            .match_query(mockito::Matcher::Missing)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"session_id":"s1","display_name":"vps claude",
                     "state":{"awaiting_input":true}}]"#,
            )
            .create_async()
            .await;
        let rows = seed(&reqwest::Client::new(), &server.url(), Some("tok"))
            .await
            .expect("the daemon answered");
        route.assert_async().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_name.as_deref(), Some("vps claude"));
        assert!(
            rows[0]
                .state
                .as_ref()
                .expect("state came back")
                .awaiting_input
        );
    }

    #[tokio::test]
    async fn the_event_stream_is_read_unfiltered_and_every_frame_is_repeated_locally() {
        let mut server = mockito::Server::new_async().await;
        // No query is the assertion that matters: a `types=` allowlist here
        // would silently drop every event type nobody thought to name.
        let route = server
            .mock("GET", "/events")
            .match_header("cookie", "tui-session=tok")
            .match_query(mockito::Matcher::Missing)
            .with_header("content-type", "text/event-stream")
            .with_body(concat!(
                ":ping\n\n",
                "event: session-state-changed\n",
                "data: {\"session_id\":\"s1\",\"state\":{\"awaiting_input\":true}}\n\n",
                "event: pty-activity\ndata: {\"session_id\":\"s1\"}\n\n",
            ))
            .create_async()
            .await;
        let state = Arc::new(make_test_app_state());
        store_seed(
            &state,
            "vps",
            vec![SessionInfo {
                session_id: "s1".into(),
                ..Default::default()
            }],
        );
        let mut rx = state.event_bus.subscribe();
        consume_stream(
            &state,
            &reqwest::Client::new(),
            "vps",
            &server.url(),
            Some("tok"),
            STREAM_IDLE_TIMEOUT,
        )
        .await
        .expect("the stream ended cleanly");
        route.assert_async().await;

        let mut seen = Vec::new();
        while let Ok(AppEvent::RemoteMirrored { event, .. }) = rx.try_recv() {
            seen.push(event);
        }
        assert_eq!(seen, vec!["session-state-changed", "pty-activity"]);
        assert!(
            mirrored_rows(&state)[0]
                .state
                .as_ref()
                .expect("the state change landed on the row")
                .awaiting_input
        );
    }

    /// The end-to-end shape the story asks for: a daemon says a session is
    /// awaiting, and what leaves this machine's own `/events` is a
    /// `session-state-changed` frame a client cannot tell from a local one.
    /// That is what makes the existing badge and notification handlers fire
    /// with no new subscription.
    #[tokio::test]
    async fn a_remote_question_leaves_this_machine_as_an_ordinary_state_change() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/events")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "text/event-stream")
            .with_body(concat!(
                "event: session-state-changed\n",
                "data: {\"session_id\":\"s1\",\"state\":",
                "{\"awaiting_input\":true,\"question_confident\":true,",
                "\"question_text\":\"Which one?\"}}\n\n",
            ))
            .create_async()
            .await;
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        consume_stream(
            &state,
            &reqwest::Client::new(),
            "vps",
            &server.url(),
            None,
            STREAM_IDLE_TIMEOUT,
        )
        .await
        .expect("the stream ended cleanly");

        let event = rx.try_recv().expect("one event crossed");
        assert_eq!(
            crate::mcp_http::sse_routes::event_type_name_for_test(&event),
            "session-state-changed"
        );
        let payload = crate::mcp_http::sse_routes::event_payload_for_test(&event);
        assert_eq!(payload["session_id"], "s1");
        assert_eq!(payload["state"]["awaiting_input"], true);
        assert_eq!(payload["state"]["question_confident"], true);
        assert_eq!(payload["state"]["question_text"], "Which one?");
    }

    /// A dropped stream must not leave a badge frozen at whatever the last
    /// frame said. The loop re-reads `GET /sessions` before it follows the
    /// stream again, so every reconnect starts from the daemon's own truth.
    ///
    /// The retry delay is passed in at 1ms: this proves the ordering, not how
    /// long the loop waits.
    #[tokio::test]
    async fn every_stream_reconnect_re_reads_the_session_list() {
        let mut server = mockito::Server::new_async().await;
        let list = server
            .mock("GET", "/sessions")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"session_id":"s1"}]"#)
            .expect_at_least(3)
            .create_async()
            .await;
        // Answers immediately and ends, so the task loops: seed, stream, wait,
        // seed again.
        let stream = server
            .mock("GET", "/events")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "text/event-stream")
            .with_body("")
            .expect_at_least(3)
            .create_async()
            .await;

        let state = Arc::new(make_test_app_state());
        state
            .remote
            .force_connected_for_test("vps", &server.url(), None);
        let task = tokio::spawn(run(
            Arc::clone(&state),
            "vps".to_string(),
            Duration::from_millis(1),
        ));
        tokio::time::sleep(Duration::from_millis(400)).await;
        task.abort();

        list.assert_async().await;
        stream.assert_async().await;
        assert_eq!(mirrored_rows(&state).len(), 1);
    }

    /// A stream that stops speaking is dead, not quiet.
    ///
    /// The daemon sends a keep-alive every 15s, so real silence means the socket
    /// went away without saying so — a slept laptop, a dropped NAT entry, a
    /// tunnel killed mid-flight. With no deadline `stream.next()` stays pending
    /// for the life of the process and the mirror holds rows nothing will ever
    /// update, while the status poll next door keeps reporting a healthy daemon.
    ///
    /// The frame assertion is not decoration: it is what stops this test passing
    /// vacuously. A deadline that fired during the HTTP response rather than
    /// during the silence would produce the same error, and the only thing that
    /// tells the two apart is whether the frame the daemon DID send arrived.
    #[tokio::test]
    async fn a_stream_that_stops_speaking_is_retired_rather_than_awaited_forever() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let daemon = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("the mirror connected");
            let mut request = [0u8; 2048];
            let _ = socket.read(&mut request).await;
            let frame = "event: pty-activity\ndata: {\"session_id\":\"s1\"}\n\n";
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                         transfer-encoding: chunked\r\n\r\n{:x}\r\n{frame}\r\n",
                        frame.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            socket.flush().await.unwrap();
            // Then nothing at all, with the socket still open.
            std::future::pending::<()>().await;
        });

        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();

        let error = consume_stream(
            &state,
            &reqwest::Client::new(),
            "vps",
            &format!("http://{addr}"),
            None,
            Duration::from_millis(300),
        )
        .await
        .expect_err("silence past the deadline is a dead stream");

        assert!(
            error.contains("keep-alive"),
            "the error must name why the stream was retired: {error}"
        );
        assert!(
            matches!(rx.try_recv(), Ok(AppEvent::RemoteMirrored { .. })),
            "the frame that arrived before the silence was dropped, so the \
             deadline fired on the handshake rather than on the silence"
        );
        daemon.abort();
    }

    fn critic_toast(data: &str) -> Frame {
        Frame {
            event: "mcp-toast".into(),
            data: data.into(),
        }
    }

    fn critic_connected(state: &Arc<AppState>, id: &str) {
        state
            .remote
            .force_connected_for_test(id, "http://daemon:9876", None);
    }

    // Catches: a daemon-supplied origin marker (fake host label, "I am local")
    // is trusted and shown instead of being dropped as an already-mirrored frame.
    #[test]
    fn critic_toast_carrying_a_forged_origin_marker_is_dropped() {
        let state = Arc::new(make_test_app_state());
        critic_connected(&state, "vps");
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "vps",
            &critic_toast(
                r#"{"title":"t","level":"info","sound":null,"__tuic_origin":{"connection":"local","name":"This Mac"}}"#,
            ),
        );
        assert!(rx.try_recv().is_err(), "forged marker must not cross");
    }

    // Catches: peer->PTY translation searches other connections' rows, so a
    // peer id owned by machine B rewrites a toast raised on machine A onto B's PTY.
    #[test]
    fn critic_toast_peer_is_translated_only_within_its_own_connection() {
        let state = Arc::new(make_test_app_state());
        critic_connected(&state, "a");
        critic_connected(&state, "b");
        store_seed(
            &state,
            "b",
            vec![SessionInfo {
                session_id: "pty-b".into(),
                tuic_session: Some("peer-x".into()),
                ..Default::default()
            }],
        );
        store_seed(&state, "a", vec![]);
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "a",
            &critic_toast(
                r#"{"title":"t","level":"info","sound":null,"origin_session_id":"peer-x"}"#,
            ),
        );
        let AppEvent::RemoteMirrored { payload, .. } = rx.try_recv().unwrap() else {
            panic!("missing toast");
        };
        assert_eq!(payload["origin_session_id"], "peer-x");
        assert_eq!(payload[ORIGIN_MARKER]["connection"], "a");
    }

    // Catches: a Rust-side dedup/rate guard that swallows a second legitimate,
    // identical toast (dedup belongs to the bell, which has its own window).
    #[test]
    fn critic_two_identical_toasts_are_both_forwarded() {
        let state = Arc::new(make_test_app_state());
        critic_connected(&state, "vps");
        let mut rx = state.event_bus.subscribe();
        let frame = critic_toast(r#"{"title":"same","level":"warn","sound":"attention"}"#);
        apply_frame(&state, "vps", &frame);
        apply_frame(&state, "vps", &frame);
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
    }

    // Catches: an unreadable connections.json drops the toast or panics
    // instead of labelling it with the connection id.
    #[test]
    fn critic_toast_survives_an_unreadable_connection_store() {
        let state = Arc::new(make_test_app_state());
        critic_connected(&state, "vps");
        std::fs::write(state.data_dir.join("connections.json"), "{not json").unwrap();
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "vps",
            &critic_toast(r#"{"title":"t","level":"info","sound":null}"#),
        );
        let AppEvent::RemoteMirrored { payload, .. } = rx.try_recv().unwrap() else {
            panic!("toast lost");
        };
        assert_eq!(payload[ORIGIN_MARKER]["name"], "vps");
    }

    // Catches: the disconnect guard also gates a non-toast event, or a toast
    // for a never-connected id (no runtime entry) is delivered.
    #[test]
    fn critic_toast_for_an_unknown_connection_is_dropped() {
        let state = Arc::new(make_test_app_state());
        let mut rx = state.event_bus.subscribe();
        apply_frame(
            &state,
            "ghost",
            &critic_toast(r#"{"title":"t","level":"info","sound":null}"#),
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_machine_with_no_remote_connection_mirrors_nothing() {
        let state = Arc::new(make_test_app_state());
        assert!(mirrored_rows(&state).is_empty());
        assert!(
            crate::mcp_http::session::session_rows_including_remote(&state).is_empty(),
            "a list call on a machine with no remote connection is the local list"
        );
    }

    // Catches: reqwest URL credentials reaching mirror errors from seed or SSE requests.
    #[tokio::test]
    async fn dead_peer_mirror_errors_never_contain_query_credentials() {
        let socket = crate::test_support::resetting_tcp_peer();
        let base = format!("http://{}", socket.local_addr().expect("address"));
        let client = reqwest::Client::new();
        let state = Arc::new(make_test_app_state());
        let secret = "MIRROR_SECRET_1457";
        let seed_error = seed(&client, &base, Some(secret))
            .await
            .expect_err("seed resetting peer");
        let stream_error = consume_stream(
            &state,
            &client,
            "closed",
            &base,
            Some(secret),
            STREAM_IDLE_TIMEOUT,
        )
        .await
        .expect_err("stream resetting peer");
        for error in [seed_error, stream_error] {
            assert!(
                !error.contains(secret),
                "credential in mirror error: {error}"
            );
            assert!(
                !error.contains(&base),
                "request URL in mirror error: {error}"
            );
        }
    }

    #[tokio::test]
    async fn a_daemon_that_rejects_the_token_is_an_error_not_an_empty_mirror() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/sessions")
            .match_query(mockito::Matcher::Any)
            .with_status(401)
            .create_async()
            .await;
        let error = seed(&reqwest::Client::new(), &server.url(), Some("stale"))
            .await
            .expect_err("401 is not a session list");
        assert!(error.contains("401"), "{error}");
    }
}

#[cfg(test)]
mod critic_confirmation_delivery {
    // Catches: remote questions/resolutions reach SSE but are suppressed for
    // the desktop confirmation host, which subscribes to native window events.
    #[test]
    fn remote_confirmation_and_resolution_are_deliverable_to_desktop() {
        assert!(
            super::window_may_hear("mcp-confirm"),
            "remote confirmation never reaches desktop"
        );
        assert!(
            super::window_may_hear("mcp-confirm-resolved"),
            "remote resolution never dismisses desktop confirmation"
        );
    }
}
