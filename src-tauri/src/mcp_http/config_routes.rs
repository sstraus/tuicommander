use crate::{AppState, MAX_CONCURRENT_SESSIONS};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::net::SocketAddr;
use std::sync::Arc;

use super::guards::localhost_only;
use super::types::*;

pub(super) async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.config.read().clone();
    let mut json = match serde_json::to_value(config) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize config: {e}")})),
            ).into_response();
        }
    };
    // Strip sensitive fields from HTTP responses
    if let Some(obj) = json.as_object_mut() {
        obj.remove("remote_access_password_hash");
        obj.remove("session_token");
        obj.remove("vapid_private_key");
    }
    Json(json).into_response()
}

pub(super) async fn put_config(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<crate::config::AppConfig>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    // Preserve server-managed secrets — clients must not overwrite these via save
    let mut config = config;
    {
        let current = state.config.read();
        config.session_token = current.session_token.clone();
        config.vapid_private_key = current.vapid_private_key.clone();
        config.vapid_public_key = current.vapid_public_key.clone();
    }
    match crate::config::save_app_config(config.clone()) {
        Ok(()) => {
            let (old_disabled, old_collapse) = {
                let c = state.config.read();
                (c.disabled_native_tools.clone(), c.collapse_tools)
            };
            *state.config.write() = config.clone();
            if old_disabled != config.disabled_native_tools || old_collapse != config.collapse_tools {
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
    Json(req): Json<HashPasswordRequest>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match bcrypt::hash(&req.password, 12) {
        Ok(hash) => (StatusCode::OK, Json(serde_json::json!({"hash": hash}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to hash: {e}")})),
        ),
    }
}

pub(super) async fn get_notification_config() -> impl IntoResponse {
    Json(crate::config::load_notification_config())
}

pub(super) async fn put_notification_config(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<crate::config::NotificationConfig>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_notification_config(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn get_ui_prefs() -> impl IntoResponse {
    Json(crate::config::load_ui_prefs())
}

pub(super) async fn put_ui_prefs(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<crate::config::UIPrefsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_ui_prefs(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn get_repo_settings() -> impl IntoResponse {
    Json(crate::config::load_repo_settings())
}

pub(super) async fn put_repo_settings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<crate::config::RepoSettingsMap>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_repo_settings(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn check_has_custom_settings_http(Query(q): Query<PathQuery>) -> impl IntoResponse {
    Json(crate::config::check_has_custom_settings(q.path))
}

pub(super) async fn get_repositories() -> impl IntoResponse {
    Json(crate::config::load_repositories())
}

pub(super) async fn put_repositories(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_repositories(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn get_pane_layout() -> impl IntoResponse {
    Json(crate::config::load_pane_layout())
}

pub(super) async fn put_pane_layout(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(layout): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_pane_layout(layout) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn clear_caches(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.clear_caches();
    Json(serde_json::json!({"ok": true}))
}

pub(super) async fn clear_repo_caches(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Some(path) = body.get("path").and_then(|v| v.as_str()) {
        state.invalidate_repo_caches(path);
    }
    Json(serde_json::json!({"ok": true}))
}

pub(super) async fn get_repo_local_config(Query(q): Query<PathQuery>) -> impl IntoResponse {
    Json(crate::config::load_repo_local_config_from_path(std::path::Path::new(&q.path)))
}

pub(super) async fn get_prompt_library() -> impl IntoResponse {
    Json(crate::config::load_prompt_library())
}

pub(super) async fn put_prompt_library(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<crate::config::PromptLibraryConfig>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_prompt_library(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

// --- Repo Defaults ---

pub(super) async fn get_repo_defaults() -> impl IntoResponse {
    Json(crate::config::load_repo_defaults())
}

pub(super) async fn put_repo_defaults(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<crate::config::RepoDefaultsConfig>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_repo_defaults(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

// --- Notes ---

pub(super) async fn get_notes() -> impl IntoResponse {
    Json(crate::config::load_notes())
}

pub(super) async fn put_notes(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = localhost_only(&addr) { return resp; }
    match crate::config::save_notes(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn get_mcp_status_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Real connect attempt — file.exists() is unreliable for stale sockets.
    #[cfg(unix)]
    let running = tokio::net::UnixStream::connect(super::socket_path()).await.is_ok();
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
