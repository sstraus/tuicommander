use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Deserialize;

use crate::state::AppEvent;
use crate::AppState;

#[derive(Deserialize)]
pub(super) struct SseQuery {
    /// Comma-separated event type filter (e.g. "repo-changed,session-created").
    /// When omitted, all events are forwarded.
    pub types: Option<String>,
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
    let allowed_types: Option<Vec<String>> = query.types.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let stream = async_stream::stream! {
        // Send retry directive as first event
        yield Ok(Event::default().retry(Duration::from_secs(5)));

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let event_name = event_type_name(&event);
                    if let Some(ref types) = allowed_types
                        && !types.iter().any(|t| t == event_name) {
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

/// Extract the normalized event type name (matches SSE `event:` field).
fn event_type_name(event: &AppEvent) -> &'static str {
    match event {
        AppEvent::HeadChanged { .. } => "head-changed",
        AppEvent::RepoChanged { .. } => "repo-changed",
        AppEvent::SessionCreated { .. } => "session-created",
        AppEvent::SessionClosed { .. } => "session-closed",
        AppEvent::PtyParsed { .. } => "pty-parsed",
        AppEvent::PtyExit { .. } => "pty-exit",
        AppEvent::PluginChanged { .. } => "plugin-changed",
        AppEvent::UpstreamStatusChanged { .. } => "upstream-status-changed",
    }
}

/// Extract just the payload (without the wrapping `event`/`payload` tags).
/// The SSE `event:` field already carries the type, so we only need the inner data.
fn event_payload(event: &AppEvent) -> serde_json::Value {
    match event {
        AppEvent::HeadChanged { repo_path, branch } => {
            serde_json::json!({ "repo_path": repo_path, "branch": branch })
        }
        AppEvent::RepoChanged { repo_path } => {
            serde_json::json!({ "repo_path": repo_path })
        }
        AppEvent::SessionCreated { session_id, cwd } => {
            serde_json::json!({ "session_id": session_id, "cwd": cwd })
        }
        AppEvent::SessionClosed { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::PtyParsed { session_id, parsed } => {
            serde_json::json!({ "session_id": session_id, "parsed": parsed })
        }
        AppEvent::PtyExit { session_id } => {
            serde_json::json!({ "session_id": session_id })
        }
        AppEvent::PluginChanged { plugin_ids } => {
            serde_json::json!({ "plugin_ids": plugin_ids })
        }
        AppEvent::UpstreamStatusChanged { name, status } => {
            serde_json::json!({ "name": name, "status": status })
        }
    }
}
