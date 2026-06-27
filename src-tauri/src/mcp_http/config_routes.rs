use crate::{AppState, MAX_CONCURRENT_SESSIONS};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use std::net::SocketAddr;
use std::sync::Arc;

use super::guards::{Authenticated, require_local_or_auth};
use super::types::*;

pub(super) async fn get_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    let config = state.config.read().clone();
    let mut json = match serde_json::to_value(config) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize config: {e}")})),
            )
                .into_response();
        }
    };
    // Strip sensitive fields from nested services config
    if let Some(services) = json.pointer_mut("/services") {
        if let Some(auth) = services.pointer_mut("/auth")
            && let Some(o) = auth.as_object_mut()
        {
            o.remove("password_hash");
            o.remove("session_token");
        }
        if let Some(push) = services.pointer_mut("/push")
            && let Some(o) = push.as_object_mut()
        {
            o.remove("vapid_private_key");
        }
    }
    Json(json).into_response()
}

pub(super) async fn put_config(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::AppConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // Preserve server-managed secrets — clients must not overwrite these via save
    let mut config = config;
    {
        let current = state.config.read();
        config.services.auth.session_token = current.services.auth.session_token.clone();
        config.services.push.vapid_private_key = current.services.push.vapid_private_key.clone();
        config.services.push.vapid_public_key = current.services.push.vapid_public_key.clone();
    }
    match crate::config::save_app_config(config.clone()) {
        Ok(()) => {
            let (old_disabled, old_collapse) = {
                let c = state.config.read();
                (c.disabled_native_tools.clone(), c.collapse_tools)
            };
            *state.config.write() = config.clone();
            if old_disabled != config.disabled_native_tools || old_collapse != config.collapse_tools
            {
                let _ = state.mcp_tools_changed.send(());
            }
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn hash_password_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(req): Json<HashPasswordRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match bcrypt::hash(&req.password, 12) {
        Ok(hash) => (StatusCode::OK, Json(serde_json::json!({"hash": hash}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to hash: {e}")})),
        ),
    }
}

pub(super) async fn rotate_session_token(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    let new_token = uuid::Uuid::new_v4().to_string();
    *state.session_token.write() = new_token.clone();
    let mut cfg = state.config.read().clone();
    cfg.services.auth.session_token = new_token;
    if let Err(e) = crate::config::save_app_config(cfg) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to persist token: {e}")})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn get_notification_config() -> impl IntoResponse {
    Json(crate::config::load_notification_config())
}

pub(super) async fn put_notification_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::NotificationConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_notification_config(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_ui_prefs() -> impl IntoResponse {
    Json(crate::config::load_ui_prefs())
}

pub(super) async fn put_ui_prefs(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::UIPrefsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_ui_prefs(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_repo_settings() -> impl IntoResponse {
    Json(crate::config::load_repo_settings())
}

pub(super) async fn put_repo_settings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::RepoSettingsMap>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repo_settings(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn check_has_custom_settings_http(
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    Json(crate::config::check_has_custom_settings(q.path))
}

pub(super) async fn get_repositories() -> impl IntoResponse {
    Json(crate::config::load_repositories())
}

pub(super) async fn put_repositories(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repositories(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_pane_layout() -> impl IntoResponse {
    Json(crate::config::load_pane_layout())
}

pub(super) async fn put_pane_layout(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(layout): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_pane_layout(layout) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn clear_caches(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    state.clear_caches();
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn clear_repo_caches(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Some(path) = body.get("path").and_then(|v| v.as_str()) {
        state.invalidate_repo_caches(path);
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn get_repo_local_config(Query(q): Query<PathQuery>) -> impl IntoResponse {
    Json(crate::config::load_repo_local_config_from_path(
        std::path::Path::new(&q.path),
    ))
}

pub(super) async fn get_prompt_library() -> impl IntoResponse {
    Json(crate::config::load_prompt_library())
}

pub(super) async fn put_prompt_library(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::PromptLibraryConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_prompt_library(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Repo Defaults ---

pub(super) async fn get_repo_defaults() -> impl IntoResponse {
    Json(crate::config::load_repo_defaults())
}

pub(super) async fn put_repo_defaults(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::RepoDefaultsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repo_defaults(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Notes ---

pub(super) async fn get_notes() -> impl IntoResponse {
    Json(crate::config::load_notes())
}

pub(super) async fn put_notes(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_notes(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Activity ---

pub(super) async fn get_activity() -> impl IntoResponse {
    Json(crate::config::load_activity())
}

pub(super) async fn put_activity(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(items): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_activity(items) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Keybindings ---

pub(super) async fn get_keybindings() -> impl IntoResponse {
    Json(crate::config::load_keybindings())
}

pub(super) async fn put_keybindings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_keybindings(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Agents Config ---

pub(super) async fn get_agents_config() -> impl IntoResponse {
    Json(crate::config::load_agents_config())
}

pub(super) async fn put_agents_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(config): Json<crate::config::AgentsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_agents_config(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Agent hook instrumentation (browser-mode parity for the toggle) ---

pub(super) async fn get_agent_hook_state(Path(agent): Path<String>) -> impl IntoResponse {
    Json(serde_json::json!({
        "state": crate::agent_hook_commands::get_agent_hook_state(agent),
    }))
}

pub(super) async fn put_agent_hook_instrumentation(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(agent): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    let enabled = body
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match crate::agent_hook_commands::set_agent_hook_instrumentation(agent, enabled) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Provider Registry ---

pub(super) async fn get_provider_registry() -> impl IntoResponse {
    Json(crate::provider_registry::load_provider_registry())
}

pub(super) async fn put_provider_registry(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(registry): Json<crate::provider_registry::ProviderRegistry>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::provider_registry::save_provider_registry(registry) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- MCP Status ---

pub(super) async fn get_mcp_status_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Real connect attempt — file.exists() is unreliable for stale sockets.
    #[cfg(unix)]
    let running = tokio::net::UnixStream::connect(super::socket_path())
        .await
        .is_ok();
    #[cfg(not(unix))]
    let running = false;
    Json(serde_json::json!({
        "enabled": true,
        "running": running,
        "active_sessions": state.sessions.len(),
        "mcp_clients": state.mcp_sessions.len(),
        "max_sessions": MAX_CONCURRENT_SESSIONS,
    }))
}

// ---------------------------------------------------------------------------
// Remote connections
// ---------------------------------------------------------------------------

pub(super) async fn get_remote_connections(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    match crate::remote_connection::RemoteConnectionStore::load(&state.data_dir) {
        Ok(connections) => match serde_json::to_value(connections) {
            Ok(v) => Json(v).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize connections: {e}")})),
            )
                .into_response(),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub(super) async fn put_remote_connection(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(connection): Json<crate::remote_connection::RemoteConnection>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = connection.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    let _guard = state.connections_lock.lock().await;
    let mut connections =
        match crate::remote_connection::RemoteConnectionStore::load(&state.data_dir) {
            Ok(c) => c,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        };
    if let Some(pos) = connections.iter().position(|c| c.id == connection.id) {
        connections[pos] = connection;
    } else {
        connections.push(connection);
    }
    match crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &connections) {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub(super) async fn delete_remote_connection(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    state.tunnel_manager.stop_if_running(&id);
    let _guard = state.connections_lock.lock().await;
    let mut connections =
        match crate::remote_connection::RemoteConnectionStore::load(&state.data_dir) {
            Ok(c) => c,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        };
    let before = connections.len();
    connections.retain(|c| c.id != id);
    if connections.len() == before {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("connection '{id}' not found")})),
        )
            .into_response();
    }
    match crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &connections) {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loopback() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }
    fn lan() -> SocketAddr {
        "192.168.1.2:1".parse().unwrap()
    }
    fn authed() -> Option<Extension<Authenticated>> {
        Some(Extension(Authenticated))
    }
    fn req() -> Json<HashPasswordRequest> {
        Json(HashPasswordRequest {
            password: "hunter2".to_string(),
        })
    }

    // hash_password_http is a representative config route: it shares the exact
    // `require_local_or_auth(&addr, auth.is_some())` guard every config handler
    // uses, and needs no AppState — so it cleanly proves the config guard wiring.

    #[tokio::test]
    async fn config_guard_loopback_passes() {
        let resp = hash_password_http(ConnectInfo(loopback()), None, req())
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_guard_authenticated_remote_passes() {
        let resp = hash_password_http(ConnectInfo(lan()), authed(), req())
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_guard_unauthenticated_remote_rejected() {
        let resp = hash_password_http(ConnectInfo(lan()), None, req())
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
