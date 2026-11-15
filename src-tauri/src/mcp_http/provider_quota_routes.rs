use crate::provider_quota::Clock;
use crate::{AppState, provider_quota::SubscriptionKey, provider_quota_transport as quota};
use axum::{
    Json,
    extract::{Path, State},
    response::Response,
};
use std::sync::Arc;

pub(super) async fn all(State(state): State<Arc<AppState>>) -> Response {
    super::json_result(Ok(quota::all(&state)))
}
pub(super) async fn binding(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Response {
    super::json_result(Ok(quota::binding(&state, &session_id)))
}
#[derive(serde::Deserialize)]
pub(super) struct RefreshRequest {
    key: SubscriptionKey,
}
pub(super) async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RefreshRequest>,
) -> Response {
    super::json_result(quota::refresh(&state, &request.key).await)
}

/// `POST /sessions/{id}/claude-status-line` — the Claude status line relay
/// (spike S5). The body is the status line stdin; `x-claude-config-dir` and
/// `x-home` carry the agent environment that names the credential. Answers
/// with the user's own status line command (empty when there is none).
/// Served only on the local IPC socket, like the hook side channel.
pub(super) async fn status_line(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> String {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let (config_dir, home) = (header("x-claude-config-dir"), header("x-home"));
    let now = crate::provider_quota::SystemClock.now_ms();
    let relayed = tokio::task::spawn_blocking(move || {
        crate::provider_quota::status_line::relay(&body, &config_dir, &home, now)
    })
    .await
    .unwrap_or_default();
    if let Some(key) = &relayed.changed {
        // Pulls the new value through `observed()`; a fresh value never reads.
        state.provider_quota.ensure(key).await;
    }
    relayed.user_command.unwrap_or_default()
}
