use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Deserialize;

use crate::AppState;
use crate::state::AppEvent;

struct SseClientGuard(Arc<AppState>);

impl SseClientGuard {
    fn new(state: Arc<AppState>) -> Self {
        state.sse_client_count.fetch_add(1, Ordering::Relaxed);
        state
            .remote_client_generation
            .fetch_add(1, Ordering::Relaxed);
        Self(state)
    }
}

impl Drop for SseClientGuard {
    fn drop(&mut self) {
        self.0.sse_client_count.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Deserialize)]
pub(super) struct SseQuery {
    /// Comma-separated event type filter (e.g. "repo-changed,session-created").
    /// When omitted, all events are forwarded.
    pub types: Option<String>,
    /// Client-chosen id of this stream. With one, the filter above is only the
    /// *initial* value and `POST /events/types` can widen it while the stream
    /// runs. Without one the filter is fixed for the life of the connection.
    pub stream_id: Option<String>,
}

/// Live type filters of the open `/events` streams, keyed by the client-supplied
/// `stream_id`.
///
/// A client learns it needs a new event type when a panel mounts, long after it
/// connected. Reopening the stream with a wider filter loses every event
/// published between the close and the new subscription — the bus has no replay
/// — so the filter is updated in place instead.
#[derive(Clone, Default)]
pub(crate) struct SseFilters {
    inner: Arc<parking_lot::Mutex<FilterRegistry>>,
}

#[derive(Default)]
struct FilterRegistry {
    streams: std::collections::HashMap<String, FilterSlot>,
    /// Distinguishes two registrations of the same id, so the guard of an
    /// already-replaced stream cannot deregister its successor.
    next_generation: u64,
}

struct FilterSlot {
    generation: u64,
    tx: tokio::sync::watch::Sender<Option<Vec<String>>>,
}

/// Streams that may hold a live filter at once. A guard deregisters each stream
/// as it ends, so this is a backstop, not a working limit: past it a stream
/// still runs, with the filter it connected with.
const MAX_FILTERED_STREAMS: usize = 64;

/// Deregisters a stream's filter when the stream ends. Held by the stream
/// itself, so a client disconnect drops it.
pub(crate) struct FilterGuard {
    filters: SseFilters,
    stream_id: String,
    generation: u64,
}

impl Drop for FilterGuard {
    fn drop(&mut self) {
        let mut registry = self.filters.inner.lock();
        if registry
            .streams
            .get(&self.stream_id)
            .is_some_and(|slot| slot.generation == self.generation)
        {
            registry.streams.remove(&self.stream_id);
        }
    }
}

impl SseFilters {
    /// Register `stream_id` with its initial filter. `None` is "every type".
    /// Returns nothing when the registry is full — the caller then keeps the
    /// filter it was given, and `update` answers false so the client reconnects.
    fn register(
        &self,
        stream_id: &str,
        initial: Option<Vec<String>>,
    ) -> Option<(
        tokio::sync::watch::Receiver<Option<Vec<String>>>,
        FilterGuard,
    )> {
        let mut registry = self.inner.lock();
        if registry.streams.len() >= MAX_FILTERED_STREAMS
            && !registry.streams.contains_key(stream_id)
        {
            tracing::warn!(
                source = "http",
                "Refusing to track the filter of SSE stream \"{stream_id}\": \
                 {MAX_FILTERED_STREAMS} streams already tracked. It runs with a fixed filter."
            );
            return None;
        }
        registry.next_generation += 1;
        let generation = registry.next_generation;
        let (tx, rx) = tokio::sync::watch::channel(initial);
        // Replaces any earlier slot for this id — an EventSource that
        // auto-reconnected reuses its id, and the old guard is about to drop.
        registry
            .streams
            .insert(stream_id.to_string(), FilterSlot { generation, tx });
        Some((
            rx,
            FilterGuard {
                filters: self.clone(),
                stream_id: stream_id.to_string(),
                generation,
            },
        ))
    }

    /// Replace the filter of a live stream. False when that stream is unknown —
    /// it ended, it never sent an id, or the registry was full.
    fn update(&self, stream_id: &str, types: Option<Vec<String>>) -> bool {
        let registry = self.inner.lock();
        match registry.streams.get(stream_id) {
            Some(slot) => {
                let _ = slot.tx.send(types);
                true
            }
            None => false,
        }
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.inner.lock().streams.len()
    }
}

/// Body of `POST /events/types`.
#[derive(Deserialize)]
pub(super) struct SseTypesBody {
    stream_id: String,
    /// The full set the client wants from now on, not a delta. An empty list is
    /// an empty allowlist, exactly as `?types=` is.
    types: Vec<String>,
}

/// `POST /events/types` — widen (or narrow) the filter of a live SSE stream.
///
/// 404 means the stream is not tracked; the client falls back to reconnecting
/// with a wider `?types=`, which is lossy but still correct.
pub(super) async fn sse_update_types(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<SseTypesBody>,
) -> axum::http::StatusCode {
    if state.sse_filters.update(&body.stream_id, Some(body.types)) {
        axum::http::StatusCode::NO_CONTENT
    } else {
        axum::http::StatusCode::NOT_FOUND
    }
}

/// SSE endpoint: `GET /events?types=repo-changed,pty-parsed`
///
/// Subscribes to the broadcast channel and streams events to the client.
/// Supports optional `?types=` filter for comma-separated event names.
/// Uses monotonic event IDs from `state.event_counter`.
pub(super) async fn sse_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SseQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_bus.subscribe();
    let client_guard = SseClientGuard::new(state.clone());
    let initial_types: Option<Vec<String>> = query.types.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });
    // With a stream id the filter is live: the client widens it in place rather
    // than reconnecting, which would drop everything published in between. The
    // guard rides the stream, so the entry disappears when the client goes.
    let tracked = query
        .stream_id
        .as_ref()
        .and_then(|id| state.sse_filters.register(id, initial_types.clone()));
    let (filter_rx, filter_guard) = match tracked {
        Some((rx, guard)) => (Some(rx), Some(guard)),
        None => (None, None),
    };

    let stream = async_stream::stream! {
        let _client_guard = client_guard;
        // Moved in so it lives exactly as long as the stream does.
        let _filter_guard = filter_guard;
        // Send retry directive as first event
        yield Ok(Event::default().retry(Duration::from_secs(5)));

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let event_name = event_type_name(&event);
                    let allowed = match filter_rx {
                        Some(ref live) => allows(&live.borrow(), event_name),
                        None => allows(&initial_types, event_name),
                    };
                    if !allowed {
                        continue;
                    }
                    let id = state.event_counter.fetch_add(1, Ordering::Relaxed);
                    let payload = match serde_json::to_string(&event_payload(&event)) {
                        Ok(json) => json,
                        Err(_) => continue,
                    };
                    yield Ok(
                        Event::default()
                            .event(event_name)
                            .id(id.to_string())
                            .data(payload)
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // Client fell behind — send a warning event and continue
                    yield Ok(
                        Event::default()
                            .event("lagged")
                            .data(format!("{{\"missed\":{n}}}")),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// `None` is "every type"; a list is an allowlist, so an empty one passes
/// nothing. That asymmetry is the wire contract: omitting `?types=` asks for
/// everything, sending it empty asks for nothing.
fn allows(filter: &Option<Vec<String>>, event_name: &str) -> bool {
    match filter {
        Some(types) => types.iter().any(|t| t == event_name),
        None => true,
    }
}

/// Extract the normalized event type name (matches SSE `event:` field).
///
/// Borrowed rather than `&'static str`: a mirrored event carries the far
/// daemon's own name, so the name is data rather than a constant.
/// Let another module's test check what name an event leaves this machine under.
#[cfg(test)]
pub(crate) fn event_type_name_for_test(event: &AppEvent) -> &str {
    event_type_name(event)
}

fn event_type_name(event: &AppEvent) -> &str {
    match event {
        AppEvent::HeadChanged { .. } => "head-changed",
        AppEvent::RepoChanged { .. } => "repo-changed",
        AppEvent::SessionCreated { .. } => "session-created",
        AppEvent::SessionClosed { .. } => "session-closed",
        AppEvent::DesignModeChanged { .. } => "design-mode-changed",
        AppEvent::PtyParsed { .. } => "pty-parsed",
        AppEvent::PtyExit { .. } => "pty-exit",
        AppEvent::PtyActivity { .. } => "pty-activity",
        AppEvent::PtyTitle { .. } => "pty-title",
        AppEvent::PtyOsc133 { .. } => "pty-osc133",
        AppEvent::PtyCwd { .. } => "pty-cwd",
        AppEvent::PluginWatcherLines { .. } => "plugin-watcher-lines",
        AppEvent::PtyDescriptionChanged { .. } => "pty-description-changed",
        AppEvent::ChatViewChanged { .. } => "chat-view-changed",
        AppEvent::SessionRenamed { .. } => "session-renamed",
        AppEvent::SessionSuspendRequested { .. } => "session-suspend-requested",
        AppEvent::TermAliasAssigned { .. } => "term-alias-assigned",
        AppEvent::PluginChanged { .. } => "plugin-changed",
        AppEvent::UpstreamStatusChanged { .. } => "upstream-status-changed",
        AppEvent::McpOAuthStart { .. } => "mcp-oauth-start",
        AppEvent::McpToast { .. } => "mcp-toast",
        AppEvent::McpConfirm { .. } => "mcp-confirm",
        AppEvent::McpConfirmResolved { .. } => "mcp-confirm-resolved",
        AppEvent::AcpNotice(_) => "acp-notice",
        AppEvent::RepositoriesChanged => "repositories-changed",
        AppEvent::DirChanged { .. } => "dir-changed",
        AppEvent::SessionWorktreeDeclared { .. } => "session-worktree-declared",
        AppEvent::WorktreeCreated { .. } => "worktree-created",
        AppEvent::WorktreeRemoved { .. } => "worktree-removed",
        AppEvent::PeerRegistered { .. } => "peer-registered",
        AppEvent::PeerUnregistered { .. } => "peer-unregistered",
        AppEvent::UiTab { .. } => "ui-tab",
        AppEvent::GitHubPrUpdate { .. } => "github-pr-update",
        AppEvent::GitHubTransition { .. } => "github-transition",
        AppEvent::GitHubIssuesUpdate { .. } => "github-issues-update",
        AppEvent::CloseHtmlTabs { .. } => "close-html-tabs",
        AppEvent::ConflictAssistStatus { .. } => "conflict-assist-status",
        AppEvent::ProgressRecorded { .. } => "progress-recorded",
        AppEvent::WorkflowRunChanged { .. } => "workflow-run-changed",
        AppEvent::AutomationRunChanged { .. } => "automation-run-changed",
        AppEvent::ReviewProgress { .. } => "review-progress",
        AppEvent::ProposalsReady { .. } => "proposals-ready",
        AppEvent::SessionStateChanged { .. } => "session-state-changed",
        AppEvent::RemoteConnectionStatusChanged { .. } => "remote-connection-status",
        // Not "remote-mirrored": a client must not be able to tell a mirrored
        // event from a local one, and a `?types=` filter has to match the name
        // the client asked for.
        AppEvent::RemoteMirrored { event, .. } => event,
        #[cfg(feature = "dictation")]
        AppEvent::DictationDownloadProgress { .. } => "dictation-download-progress",
        #[cfg(feature = "dictation")]
        AppEvent::SpeechDownloadProgress { .. } => "speech-download-progress",
        #[cfg(feature = "dictation")]
        AppEvent::SpeechUtterance { .. } => "speech-utterance",
    }
}

/// Extract just the payload (without the wrapping `event`/`payload` tags).
/// The SSE `event:` field already carries the type, so we only need the inner data.
/// Let another module's test compare this payload against the desktop one.
///
/// The two are built by different code in different files, which is exactly why
/// they drift; a test that can only see one of them cannot catch it.
#[cfg(test)]
pub(crate) fn event_payload_for_test(event: &AppEvent) -> serde_json::Value {
    event_payload(event)
}

fn event_payload(event: &AppEvent) -> serde_json::Value {
    match event {
        AppEvent::HeadChanged { repo_path, branch } => {
            serde_json::json!({ "repo_path": repo_path, "branch": branch })
        }
        AppEvent::RepoChanged { repo_path, kind } => {
            serde_json::json!({ "repo_path": repo_path, "kind": kind })
        }
        AppEvent::SessionCreated {
            session_id,
            cwd,
            agent_type,
            display_name,
            parent_session,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "cwd": cwd,
                "agent_type": agent_type,
                "display_name": display_name,
                "parent_session": parent_session,
            })
        }
        AppEvent::SessionClosed { session_id, reason } => {
            serde_json::json!({ "session_id": session_id, "reason": reason })
        }
        AppEvent::DesignModeChanged {
            repo_path,
            session_id,
            status,
        } => {
            serde_json::json!({ "repo_path": repo_path, "session_id": session_id, "status": status })
        }
        AppEvent::PtyParsed { session_id, parsed } => {
            serde_json::json!({ "session_id": session_id, "parsed": parsed })
        }
        AppEvent::PtyExit { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::PtyTitle { session_id, title } => {
            serde_json::json!({ "session_id": session_id, "title": title })
        }
        AppEvent::PtyActivity { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::PtyOsc133 {
            session_id,
            marker,
            line,
            exit_code,
        } => {
            serde_json::json!({
                "session_id": session_id,
                "marker": marker,
                "line": line,
                "exit_code": exit_code,
            })
        }
        AppEvent::PtyCwd { session_id, cwd } => {
            serde_json::json!({ "session_id": session_id, "cwd": cwd })
        }
        AppEvent::PluginWatcherLines { session_id, lines } => {
            serde_json::json!({ "session_id": session_id, "lines": lines })
        }
        AppEvent::ChatViewChanged { session_id, seq } => {
            serde_json::json!({ "session_id": session_id, "seq": seq })
        }
        AppEvent::PtyDescriptionChanged {
            session_id,
            description,
        } => {
            serde_json::json!({ "session_id": session_id, "description": description })
        }
        AppEvent::SessionRenamed {
            session_id,
            name,
            is_custom,
        } => {
            serde_json::json!({ "session_id": session_id, "name": name, "is_custom": is_custom })
        }
        AppEvent::SessionSuspendRequested {
            session_id,
            request_id,
        } => {
            serde_json::json!({ "session_id": session_id, "request_id": request_id })
        }
        AppEvent::TermAliasAssigned { session_id, alias } => {
            serde_json::json!({ "session_id": session_id, "alias": alias })
        }
        AppEvent::PluginChanged { plugin_ids } => {
            serde_json::json!({ "plugin_ids": plugin_ids })
        }
        AppEvent::UpstreamStatusChanged { name, status } => {
            serde_json::json!({ "name": name, "status": status })
        }
        AppEvent::McpOAuthStart {
            name,
            authorization_url,
        } => {
            serde_json::json!({ "name": name, "authorization_url": authorization_url })
        }
        AppEvent::McpToast {
            title,
            message,
            level,
            sound,
            origin_repo_path,
            origin_session_id,
        } => {
            serde_json::json!({
                "title": title,
                "message": message,
                "level": level,
                "sound": sound,
                "origin_repo_path": origin_repo_path,
                "origin_session_id": origin_session_id,
            })
        }
        AppEvent::McpConfirm {
            request_id,
            title,
            message,
            origin_repo_path,
            origin_session_id,
        } => {
            serde_json::json!({
                "request_id": request_id,
                "title": title,
                "message": message,
                "origin_repo_path": origin_repo_path,
                "origin_session_id": origin_session_id,
            })
        }
        AppEvent::McpConfirmResolved {
            request_id,
            confirmed,
        } => {
            serde_json::json!({ "request_id": request_id, "confirmed": confirmed })
        }
        // Forwarded whole: the notice IS the payload, in the same camelCase the
        // `/acp` routes use, so a client needs no per-transport translation.
        AppEvent::AcpNotice(notice) => serde_json::json!(notice),
        // Payload-free: the receiver re-reads `repositories.json` itself.
        AppEvent::RepositoriesChanged => serde_json::json!({}),
        AppEvent::DirChanged { dir_path } => {
            serde_json::json!({ "dir_path": dir_path })
        }
        // Forwarded whole, like `AcpNotice`: the desktop `emit` in
        // `notify_worktree_created`/`notify_worktree_removed` serializes this
        // same struct, so the two transports cannot spell a field differently.
        AppEvent::SessionWorktreeDeclared(payload) => serde_json::json!(payload),
        AppEvent::WorktreeCreated(payload) => serde_json::json!(payload),
        AppEvent::WorktreeRemoved(payload) => serde_json::json!(payload),
        AppEvent::PeerRegistered { tuic_session, name } => {
            serde_json::json!({ "tuic_session": tuic_session, "name": name })
        }
        AppEvent::PeerUnregistered { tuic_session } => {
            serde_json::json!({ "tuic_session": tuic_session })
        }
        AppEvent::UiTab {
            id,
            title,
            html,
            url,
            pinned,
            focus,
            origin_repo_path,
        } => {
            let mut v = serde_json::json!({ "id": id, "title": title, "html": html, "pinned": pinned, "focus": focus });
            if let Some(u) = url {
                v["url"] = serde_json::Value::String(u.clone());
            }
            if let Some(p) = origin_repo_path {
                v["origin_repo_path"] = serde_json::Value::String(p.clone());
            }
            v
        }
        AppEvent::GitHubPrUpdate {
            repo_path,
            statuses,
        } => {
            serde_json::json!({ "repo_path": repo_path, "statuses": statuses })
        }
        AppEvent::GitHubTransition { transition } => {
            serde_json::to_value(transition).unwrap_or_default()
        }
        AppEvent::GitHubIssuesUpdate { repo_path, issues } => {
            serde_json::json!({ "repo_path": repo_path, "issues": issues })
        }
        AppEvent::CloseHtmlTabs { tab_ids } => {
            serde_json::json!({ "tab_ids": tab_ids })
        }
        AppEvent::ConflictAssistStatus { repo_path, payload }
        | AppEvent::ProgressRecorded { repo_path, payload }
        | AppEvent::WorkflowRunChanged { repo_path, payload }
        | AppEvent::ReviewProgress { repo_path, payload }
        | AppEvent::ProposalsReady { repo_path, payload } => {
            serde_json::json!({ "repo_path": repo_path, "payload": payload })
        }
        AppEvent::SessionStateChanged { session_id, state } => {
            // Built by the same function the desktop window emit uses, so the
            // two transports cannot drift into two shapes for one thing.
            crate::state::session_state_payload(session_id, state)
        }
        // Already the shape the desktop window emit carries: the publisher builds
        // it once and hands the same value to both transports.
        AppEvent::RemoteConnectionStatusChanged { payload } => payload.clone(),
        // The daemon's own body, untouched.
        AppEvent::RemoteMirrored { payload, .. } => payload.clone(),
        AppEvent::AutomationRunChanged { payload } => payload.clone(),
        // Built once by the producer and handed to both transports, for the
        // same reason as `SessionStateChanged` above: the frontend applies one
        // shape, and a pair built from separate code in separate files drifts.
        #[cfg(feature = "dictation")]
        AppEvent::DictationDownloadProgress { payload }
        | AppEvent::SpeechDownloadProgress { payload }
        | AppEvent::SpeechUtterance { payload } => payload.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use futures_util::StreamExt;

    /// Read the next SSE chunk, or `None` if nothing arrives promptly. The
    /// generator is a pull stream: it only advances while polled, so "nothing
    /// arrives" needs a timeout rather than an immediate poll.
    async fn next_chunk(body: &mut axum::body::BodyDataStream) -> Option<String> {
        match tokio::time::timeout(Duration::from_millis(250), body.next()).await {
            Ok(Some(Ok(bytes))) => Some(String::from_utf8_lossy(&bytes).to_string()),
            _ => None,
        }
    }

    fn repo_changed() -> AppEvent {
        AppEvent::RepoChanged {
            repo_path: "/repo".into(),
            kind: crate::repo_watcher::RepoChangeKind::WorkingTree,
        }
    }

    fn dir_changed() -> AppEvent {
        AppEvent::DirChanged {
            dir_path: "/dir".into(),
        }
    }

    #[tokio::test]
    async fn event_stream_lifetime_tracks_only_the_connected_client() {
        let state = crate::mcp_http::tests::test_state();
        let _internal = state.event_bus.subscribe();
        assert_eq!(state.sse_client_count.load(Ordering::Relaxed), 0);

        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: None,
                stream_id: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(state.sse_client_count.load(Ordering::Relaxed), 1);

        drop(response);
        assert_eq!(state.sse_client_count.load(Ordering::Relaxed), 0);
    }

    /// A panel that mounts late adds an event type the stream was not opened
    /// with. Reopening the stream to widen the filter loses everything published
    /// between the close and the new subscription — the bus has no replay — so
    /// the filter has to change on the live stream.
    #[tokio::test]
    async fn a_live_stream_widens_its_filter_without_reconnecting() {
        let state = crate::mcp_http::tests::test_state();
        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: Some("repo-changed".into()),
                stream_id: Some("s1".into()),
            }),
        )
        .await
        .into_response();
        let mut body = response.into_body().into_data_stream();
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("retry")),
            "the stream opens with the retry directive"
        );

        let _ = state.event_bus.send(repo_changed());
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("repo-changed")),
            "the type it connected with arrives"
        );

        let _ = state.event_bus.send(dir_changed());
        assert!(
            next_chunk(&mut body).await.is_none(),
            "a type outside the filter is dropped"
        );

        assert_eq!(
            sse_update_types(
                State(state.clone()),
                axum::Json(SseTypesBody {
                    stream_id: "s1".into(),
                    types: vec!["repo-changed".into(), "dir-changed".into()],
                }),
            )
            .await,
            axum::http::StatusCode::NO_CONTENT
        );

        let _ = state.event_bus.send(dir_changed());
        assert!(
            next_chunk(&mut body)
                .await
                .is_some_and(|c| c.contains("dir-changed")),
            "the widened type arrives on the same connection"
        );
    }

    #[tokio::test]
    async fn a_stream_without_an_id_keeps_the_filter_it_connected_with() {
        let state = crate::mcp_http::tests::test_state();
        let response = sse_events(
            State(state.clone()),
            Query(SseQuery {
                types: Some("repo-changed".into()),
                stream_id: None,
            }),
        )
        .await
        .into_response();
        let mut body = response.into_body().into_data_stream();
        next_chunk(&mut body).await;

        // Nothing to update: the client must reconnect, and it learns that from
        // the 404 rather than from a silent success.
        assert_eq!(
            sse_update_types(
                State(state.clone()),
                axum::Json(SseTypesBody {
                    stream_id: "unknown".into(),
                    types: vec!["dir-changed".into()],
                }),
            )
            .await,
            axum::http::StatusCode::NOT_FOUND
        );

        let _ = state.event_bus.send(dir_changed());
        assert!(next_chunk(&mut body).await.is_none());
    }

    /// The registry must not outlive the streams it describes: every entry is
    /// dropped with its stream, and an EventSource that auto-reconnects reuses
    /// its id, so the guard of the *previous* connection must not take the new
    /// slot with it.
    #[test]
    fn a_filter_entry_dies_with_its_stream_and_never_takes_its_successor() {
        let filters = SseFilters::default();
        let (_rx, first) = filters.register("s1", None).expect("registered");
        assert_eq!(filters.tracked(), 1);

        let (_rx2, second) = filters.register("s1", None).expect("re-registered");
        assert_eq!(filters.tracked(), 1, "one id is one slot");
        drop(first);
        assert!(
            filters.update("s1", Some(vec!["repo-changed".into()])),
            "the reconnected stream still owns the slot"
        );

        drop(second);
        assert_eq!(filters.tracked(), 0);
        assert!(!filters.update("s1", None));
    }

    #[test]
    fn the_filter_registry_is_bounded() {
        let filters = SseFilters::default();
        let guards: Vec<_> = (0..MAX_FILTERED_STREAMS)
            .map(|i| {
                filters
                    .register(&format!("s{i}"), None)
                    .expect("registered")
            })
            .collect();
        assert!(
            filters.register("one-too-many", None).is_none(),
            "past the cap a stream runs with the filter it connected with"
        );
        // An already-tracked id is not a new stream, so it still updates.
        assert!(filters.register("s0", None).is_some());
        drop(guards);
    }

    #[test]
    fn an_absent_filter_passes_everything_and_an_empty_one_nothing() {
        assert!(allows(&None, "repo-changed"));
        assert!(!allows(&Some(Vec::new()), "repo-changed"));
        assert!(allows(&Some(vec!["repo-changed".into()]), "repo-changed"));
        assert!(!allows(&Some(vec!["repo-changed".into()]), "dir-changed"));
    }

    #[test]
    fn session_created_preserves_stable_display_name() {
        let event = AppEvent::SessionCreated {
            session_id: "session-1".into(),
            cwd: Some("/repo".into()),
            agent_type: Some("codex".into()),
            display_name: Some("linux-primary".into()),
            parent_session: Some("tuic-parent".into()),
        };

        assert_eq!(event_type_name(&event), "session-created");
        let body = event_payload(&event);
        assert_eq!(body["session_id"], "session-1");
        assert_eq!(body["cwd"], "/repo");
        assert_eq!(body["agent_type"], "codex");
        assert_eq!(body["display_name"], "linux-primary");
        // Browser clients tag sub-agent tabs from this field, as the desktop does.
        assert_eq!(body["parent_session"], "tuic-parent");
    }

    #[test]
    fn pty_title_has_matching_sse_name_and_payload() {
        let event = AppEvent::PtyTitle {
            session_id: "session-1".into(),
            title: "Claude Code".into(),
        };

        assert_eq!(event_type_name(&event), "pty-title");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_id": "session-1", "title": "Claude Code"})
        );
    }

    #[test]
    fn pty_description_changed_has_matching_sse_name_and_payload() {
        let event = AppEvent::PtyDescriptionChanged {
            session_id: "session-1".into(),
            description: None,
        };

        assert_eq!(event_type_name(&event), "pty-description-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_id": "session-1", "description": null})
        );
    }

    #[test]
    fn term_alias_assigned_has_matching_sse_name_and_payload() {
        let event = AppEvent::TermAliasAssigned {
            session_id: "session-1".into(),
            alias: "tu-3".into(),
        };

        assert_eq!(event_type_name(&event), "term-alias-assigned");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"session_id": "session-1", "alias": "tu-3"})
        );
    }

    /// The GitHub Ops lifecycle events share a `{repo_path, payload}` shape.
    /// Each must map to its own SSE `event:` name and round-trip the payload
    /// verbatim so browser/PWA clients receive the same data as desktop.
    ///
    /// Two of the three original cases (`review-progress`, `proposals-ready`)
    /// went with the embedded engine that produced them (#784-0aec); the arm
    /// they shared is what this still guards.
    #[test]
    fn ops_lifecycle_events_have_distinct_names_and_passthrough_payload() {
        let payload = serde_json::json!({ "pr_number": 42, "phase": "done", "done": true });
        let cases: Vec<(AppEvent, &str)> = vec![
            (
                AppEvent::ConflictAssistStatus {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "conflict-assist-status",
            ),
            (
                AppEvent::ProgressRecorded {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "progress-recorded",
            ),
            (
                AppEvent::ReviewProgress {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "review-progress",
            ),
            (
                AppEvent::ProposalsReady {
                    repo_path: "/repo".into(),
                    payload: payload.clone(),
                },
                "proposals-ready",
            ),
        ];

        // Names must all be distinct and match the expected kebab-case tag.
        let mut seen = std::collections::HashSet::new();
        for (event, expected_name) in &cases {
            assert_eq!(event_type_name(event), *expected_name);
            assert!(
                seen.insert(*expected_name),
                "duplicate event name {expected_name}"
            );
            let body = event_payload(event);
            assert_eq!(body["repo_path"], "/repo");
            assert_eq!(body["payload"], payload);
        }
    }

    #[test]
    fn progress_recorded_has_the_same_receipt_and_event_envelope_on_sse() {
        let payload = serde_json::json!({
            "receipt": {"status":"recorded", "revision":7, "eventId":"event-7"},
            "event": {"id":"event-7", "revision":7, "type":"milestone", "summary":"Done."}
        });
        let event = AppEvent::ProgressRecorded {
            repo_path: "/repo".into(),
            payload: payload.clone(),
        };

        assert_eq!(event_type_name(&event), "progress-recorded");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({"repo_path":"/repo", "payload":payload})
        );
    }

    #[test]
    fn workflow_run_changed_is_a_cursor_wake_hint() {
        let payload = serde_json::json!({ "runId": "run-1", "sequence": 7 });
        let event = AppEvent::WorkflowRunChanged {
            repo_path: "/repo".into(),
            payload: payload.clone(),
        };
        assert_eq!(event_type_name(&event), "workflow-run-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({
                "repo_path": "/repo", "payload": payload,
            })
        );
    }

    /// A browser learns a session's lifecycle from this arm; the desktop learns
    /// it from the window event of the same name. Both must hand the frontend
    /// the shape `list_active_sessions` returns per session — `{session_id,
    /// state}` with `state` in snake_case — because one applier consumes all
    /// three and a renamed key silently stops updating a badge.
    #[test]
    fn session_state_changed_carries_a_list_active_sessions_entry() {
        let event = AppEvent::SessionStateChanged {
            session_id: "sess-1".into(),
            state: Box::new(crate::state::SessionState {
                awaiting_input: true,
                question_confident: true,
                shell_state: Some("idle".into()),
                agent_state: Some("awaiting_input".into()),
                queued_commands: 2,
                ..Default::default()
            }),
        };
        assert_eq!(event_type_name(&event), "session-state-changed");
        let body = event_payload(&event);
        assert_eq!(body["session_id"], "sess-1");
        assert_eq!(body["state"]["awaiting_input"], true);
        assert_eq!(body["state"]["question_confident"], true);
        assert_eq!(body["state"]["shell_state"], "idle");
        assert_eq!(body["state"]["agent_state"], "awaiting_input");
        assert_eq!(body["state"]["queued_commands"], 2);
        // Not flattened: the frontend reads `payload.state.*`, exactly as it
        // reads `session.state.*` from the polled snapshot it replaces.
        assert!(body.get("awaiting_input").is_none());
    }

    #[test]
    fn design_mode_changed_carries_the_bound_session_and_status() {
        let event = AppEvent::DesignModeChanged {
            repo_path: "/repo".into(),
            session_id: "agent-1".into(),
            status: "armed".into(),
        };
        assert_eq!(event_type_name(&event), "design-mode-changed");
        assert_eq!(
            event_payload(&event),
            serde_json::json!({
                "repo_path": "/repo", "session_id": "agent-1", "status": "armed"
            })
        );
    }

    /// An ACP wake signal reaches a browser unchanged.
    ///
    /// Field-for-field the object the `/acp` routes and the desktop commands
    /// return, camelCase included: a client that reacts by fetching the
    /// connection snapshot has to be able to use the ids it was just handed,
    /// and a renamed key here would be a second spelling for one thing.
    #[test]
    fn an_acp_notice_reaches_the_browser_as_the_object_the_acp_routes_return() {
        let connection_id = crate::acp::AcpConnectionId::new();
        let request_id = crate::acp::AcpHostRequestId::new();
        let event = AppEvent::AcpNotice(crate::acp::AcpNotice {
            connection_id,
            generation: 3,
            session_id: Some(agent_client_protocol::schema::v1::SessionId::new("sess-1")),
            request_id: Some(request_id),
            sequence: 42,
            kind: crate::acp::AcpNoticeKind::InteractionPending,
        });
        assert_eq!(event_type_name(&event), "acp-notice");
        let body = event_payload(&event);
        assert_eq!(body["connectionId"], serde_json::json!(connection_id));
        assert_eq!(body["requestId"], serde_json::json!(request_id));
        assert_eq!(body["sessionId"], "sess-1");
        assert_eq!(body["generation"], 3);
        assert_eq!(body["sequence"], 42);
        assert_eq!(body["kind"], "interaction_pending");
        assert!(body.get("connection_id").is_none());
    }
}
