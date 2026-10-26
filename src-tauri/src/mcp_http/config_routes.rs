use crate::{AppState, MAX_CONCURRENT_SESSIONS};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use std::net::SocketAddr;
use std::sync::Arc;

use super::guards::{Authenticated, require_local_or_auth};
use super::types::*;
use super::{json_result, validate_repo_path};

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
        if let Some(relay) = services.pointer_mut("/relay")
            && let Some(o) = relay.as_object_mut()
        {
            o.remove("token");
        }
    }
    Json(json).into_response()
}

pub(super) async fn put_config(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::AppConfig>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // The merge runs INSIDE the config write lock (commit_config_change) so a partial
    // body is applied to the config as it is at write time, not to a snapshot another
    // writer has already replaced. Blocking pool: the critical section does disk I/O.
    let saved = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            crate::config::commit_config_save(&state, request.base, request.config)
        })
        .await
    };

    let effects = match saved {
        Ok(Ok(effects)) => effects,
        Ok(Err(e)) => {
            // A merge/validation failure is the caller's fault; a write failure is ours.
            let code = if e.starts_with("Invalid config") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return (code, Json(serde_json::json!({"error": e})));
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("config save task failed: {e}")})),
            );
        }
    };

    if effects.tools_changed {
        let _ = state.mcp.tools_changed.send(());
    }
    // Parity with the IPC `save_config`: rebind the listener so the running process
    // cannot keep serving a config the disk disagrees with.
    if effects.server_changed {
        super::restart_after_server_settings_change(
            &state,
            "remote-access configuration changed over HTTP",
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}

pub(super) async fn hash_password_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(req): Json<HashPasswordRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // bcrypt at cost 12 is CPU-heavy (~300ms) — run it off the async runtime.
    let password = req.password;
    let hash_result = tokio::task::spawn_blocking(move || bcrypt::hash(&password, 12)).await;
    match hash_result {
        Ok(Ok(hash)) => (StatusCode::OK, Json(serde_json::json!({"hash": hash}))),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to hash: {e}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Hash task failed: {e}")})),
        ),
    }
}

/// Hand the running session token to a caller that already authenticated.
///
/// This is the only way a remote client can reach a WebSocket: an upgrade
/// request cannot carry an `Authorization` header, and `remote_auth` serves
/// `Access-Control-Allow-Origin: *`, which forbids credentialed cookies. A
/// client therefore authenticates once with Basic Auth here and then puts
/// `?token=` on every later request, exactly as the QR-code flow does.
///
/// The token is never written to `config.json` and stays redacted in `/config`;
/// it lives in memory and changes on every daemon restart, so a client re-reads
/// it whenever it reconnects.
pub(super) async fn get_session_token(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    let token = state.session_token.read().clone();
    if token.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no session token configured"})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"token": token})))
}

pub(super) async fn rotate_session_token(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    // Blocking pool: the rotation takes the config write lock and touches disk.
    let rotated = tokio::task::spawn_blocking(move || crate::config::rotate_session_token(&state))
        .await
        .map_err(|e| format!("token rotation task failed: {e}"))
        .and_then(|r| r);
    if let Err(e) = rotated {
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::NotificationConfig>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_notification_config(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "notifications", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn get_ui_prefs() -> impl IntoResponse {
    Json(crate::config::load_ui_prefs())
}

/// `_http` suffix: `crate::config::get_config_defaults` already owns the bare name.
pub(super) async fn get_config_defaults_http() -> impl IntoResponse {
    Json(crate::config::get_config_defaults(None))
}

pub(super) async fn preview_config_defaults_http(
    Json(preview): Json<crate::settings::agents::LaunchPreview>,
) -> impl IntoResponse {
    Json(crate::config::get_config_defaults(Some(preview)))
}

pub(super) async fn put_ui_prefs(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::UIPrefsConfig>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_ui_prefs(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "preferences", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::RepoSettingsMap>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repo_settings(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "repositories", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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
    State(state): State<Arc<AppState>>,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repositories_request(config) {
        Ok(changed) => {
            if changed {
                state.notify_repositories_changed();
                crate::settings::api::publish_change(&state, "repositories_registration", true);
            }
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => {
            let status = match &e {
                crate::config::RepositorySaveError::Conflict(_) => StatusCode::CONFLICT,
                crate::config::RepositorySaveError::Invalid(_) => StatusCode::BAD_REQUEST,
                crate::config::RepositorySaveError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

pub(super) async fn get_stale_temp_repository_candidates() -> impl IntoResponse {
    Json(crate::config::list_stale_temp_repository_candidates())
}

#[derive(serde::Deserialize)]
pub(super) struct RepairStaleTempRepositoriesBody {
    paths: Vec<String>,
}

pub(super) async fn post_repair_stale_temp_repositories(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RepairStaleTempRepositoriesBody>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::repair_stale_temp_repositories_request(body.paths) {
        Ok(summary) => {
            state.notify_repositories_changed();
            (StatusCode::OK, Json(serde_json::to_value(summary).unwrap()))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<serde_json::Value>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_pane_layout(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "pane_layout", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::PromptLibraryConfig>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_prompt_library(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "prompts", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::RepoDefaultsConfig>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_repo_defaults(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "repo_defaults", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// --- Notes ---

pub(super) async fn get_notes() -> impl IntoResponse {
    match crate::config::load_notes() {
        Ok(v) => (StatusCode::OK, Json(v)),
        // 500 so the client stays un-hydrated and never overwrites the file it could not read.
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

pub(super) async fn put_notes(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(request): Json<crate::config::ConfigSaveRequest<serde_json::Value>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_notes(request.base, request.config) {
        Ok(()) => {
            state.notify_ideas_changed();
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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
    Json(request): Json<crate::config::ConfigSaveRequest<serde_json::Value>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_activity(request.base, request.config) {
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<serde_json::Value>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_keybindings(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "keybindings", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<crate::config::AgentsConfig>>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    match crate::config::save_agents_config(request.base, request.config) {
        Ok(()) => {
            crate::settings::api::publish_change(&state, "agents", true);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
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

pub(super) async fn get_agent_native_status_signals(
    Path(agent): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "enabled": crate::agent_hook_commands::get_agent_native_status_signals(agent),
    }))
}

pub(super) async fn put_agent_native_status_signals(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(agent): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp;
    }
    let Some(enabled) = body.get("enabled").and_then(serde_json::Value::as_bool) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "enabled must be a boolean"})),
        );
    };
    match crate::agent_hook_commands::set_agent_native_status_signals(agent, enabled) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
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
        "native_tools": super::mcp_transport::native_tool_catalog(),
        "native_capabilities": super::mcp_transport::native_capability_catalog(&state),
        "enabled": true,
        "running": running,
        "active_sessions": state.session_maps.sessions.len(),
        "mcp_clients": state.mcp.sessions.len(),
        "max_sessions": MAX_CONCURRENT_SESSIONS,
        "tls_active": state.tls_listener.read().strong_count() > 0,
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
    Json(request): Json<crate::remote_connection::RemoteConnectionSaveRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    // Validate up front so a malformed request gets a precise 400 (the shared
    // upsert helper re-validates as its canonical gate — that path stays a 500).
    if let Err(e) = request.connection.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    let _guard = state.connections_lock.lock().await;
    match crate::remote_connection::save_connection_record(&state, request.base, request.connection)
    {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
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
    // `stop_if_running(&id)` stood here and did nothing: the tunnel is keyed by
    // the TunnelProfile's own UUID, not by the connection's, so the call could
    // only ever miss. Teardown stops it by the id the runtime recorded, and
    // takes the poll, the mirror, its rows and the session token with it.
    crate::remote_runtime::teardown_deleted(&state, &id);
    let _guard = state.connections_lock.lock().await;
    match crate::remote_connection::delete_connection_record(&state, &id, |id| {
        crate::remote_connection::set_connection_password(id, "")
    }) {
        Ok(true) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("connection '{id}' not found")})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// Store the Basic Auth password for a connection, or forget it when the body
/// carries an empty string. Read-back is deliberately impossible: the only
/// answers this file gives about a stored secret are "exists" and a token
/// exchanged against the daemon.
pub(super) async fn put_remote_connection_password(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RemoteConnectionPasswordRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::json_result(
        crate::remote_connection::set_connection_password_with_refresh(&state, &id, &body.password),
    )
}

/// Import the relay pairing bundle printed by `tuic-remote --relay-pair`, or
/// forget the pairing when the body carries an empty string.
pub(super) async fn put_remote_connection_relay_pairing(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<super::types::RemoteConnectionRelayPairingRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::json_result(crate::remote_connection::set_relay_pairing_with_refresh(
        &state,
        &id,
        &body.bundle,
    ))
}

pub(super) async fn get_remote_connection_password_exists(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::json_result(crate::remote_connection::connection_password_exists(&id))
}

/// Exchange the stored password for the remote daemon's session token.
///
/// The failure is an upstream one — a wrong password or an unreachable daemon —
/// so it answers 502 rather than 500, like every other call that leaves this
/// machine. Which of the two happened is in the message.
pub(super) async fn post_remote_connection_token(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Path(id): Path<String>,
    Json(body): Json<RemoteConnectionTokenRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::upstream_json_result(
        crate::remote_connection::fetch_connection_token(&id, &body.base_url, &body.username).await,
    )
}

/// The live state of every remote connection this backend has been asked to
/// connect: status, and — only while connected — where it answers and with
/// which token.
pub(super) async fn get_remote_connection_statuses(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Guarded like a write, not like a read: the snapshot carries the daemon's
    // session token for every connected connection.
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::json_result(Ok::<_, String>(state.remote.snapshot()))
}

pub(super) async fn get_remote_update_preview(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::upstream_json_result(crate::remote_update::prepare(&state, &id).await)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteUpdateRequest {
    confirmed_sessions: usize,
    expected_sha256: String,
}

pub(super) async fn post_remote_update(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<RemoteUpdateRequest>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::upstream_json_result(
        crate::remote_update::update_and_restart(
            &state,
            &id,
            request.confirmed_sessions,
            &request.expected_sha256,
        )
        .await,
    )
}

/// Bring a remote connection up. The upstream failures — unreachable daemon,
/// rejected password, tunnel that never came up — answer 502, as every call that
/// leaves this machine does.
pub(super) async fn post_remote_connection_connect(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::upstream_json_result(crate::remote_runtime::connect(&state, &id).await)
}

/// Take a remote connection down. Always succeeds: a connection that was never
/// up is already where the caller wants it.
pub(super) async fn delete_remote_connection_connect(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    crate::remote_runtime::teardown(&state, &id);
    super::json_result(Ok::<_, String>(serde_json::json!({ "ok": true })))
}

pub(super) async fn post_remote_connection_install(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::upstream_json_result(
        crate::remote_deploy::service::install_remote_daemon_shared(&state, &id).await,
    )
}

pub(super) async fn delete_remote_connection_install(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    super::upstream_json_result(
        crate::remote_deploy::service::uninstall_remote_daemon_shared(&state, &id).await,
    )
}

// --- Story 066: config / themes / notes / misc stateless parity (loopback router) ---
//
// Mutating / action handlers carry the same `require_local_or_auth` guard as the
// other config writes in this file. Pure reads skip it (matching get_prompt_library
// / get_repo_local_config). `/exec/shell-script` and `/agent/open-in-custom` run
// processes, so they are guarded.

pub(super) async fn save_repo_local_config_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SaveRepoLocalConfigRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(crate::config::save_repo_local_config(body.repo_path))
}

pub(super) async fn set_branch_label_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SetBranchLabelRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(crate::config::set_branch_label(
        body.repo_path,
        body.branch_name,
        body.label,
    ))
}

pub(super) async fn save_note_image_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SaveNoteImageRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::save_note_image(
        body.note_id,
        body.data_base64,
        body.extension,
    ))
}

pub(super) async fn delete_note_assets_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<DeleteNoteAssetsRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::delete_note_assets(body.note_id))
}

pub(super) async fn delete_note_assets_batch_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<DeleteNoteAssetsBatchRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::config::delete_note_assets_batch(body.note_ids))
}

pub(super) async fn list_themes_http() -> impl IntoResponse {
    let themes_dir = crate::config::config_dir().join("themes");
    Json(crate::themes::load_themes(&themes_dir))
}

pub(super) async fn execute_shell_script_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<ExecuteShellScriptRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(
        crate::smart_prompt::execute_shell_script(
            body.script_content,
            body.timeout_ms,
            body.repo_path,
        )
        .await,
    )
}

pub(super) async fn list_audio_output_devices_http() -> impl IntoResponse {
    // `notification_sound` is desktop-only; a headless remote daemon has no audio
    // output context, so it reports an empty device list.
    #[cfg(feature = "desktop")]
    let devices = crate::notification_sound::list_audio_output_devices().await;
    #[cfg(not(feature = "desktop"))]
    let devices: Result<Vec<serde_json::Value>, String> = Ok(Vec::new());
    json_result(devices)
}

pub(super) async fn discover_agent_session_http(
    Json(body): Json<DiscoverAgentSessionRequest>,
) -> impl IntoResponse {
    Json(
        crate::agent_session::discover_agent_session(
            body.agent_type,
            body.cwd,
            body.claimed_ids,
            body.agent_pid,
            body.env_overrides,
        )
        .await,
    )
}

pub(super) async fn claude_project_dir_http(
    Json(body): Json<ClaudeProjectDirRequest>,
) -> axum::response::Response {
    json_result(crate::agent_session::claude_project_dir(
        body.cwd,
        body.claude_config_dir,
    ))
}

pub(super) async fn open_in_custom_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<OpenInCustomRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::agent::open_in_custom(
        body.executable,
        body.args,
        body.ctx,
    ))
}

pub(super) async fn generate_value_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<GenerateValueRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    json_result(crate::generators::generate_value(body.request))
}

pub(super) async fn fetch_plugin_registry_http() -> axum::response::Response {
    json_result(crate::registry::fetch_plugin_registry().await)
}

pub(super) async fn set_project_mcp_upstreams_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetProjectMcpUpstreamsRequest>,
) -> axum::response::Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    json_result(crate::mcp_upstream_config::set_project_mcp_upstreams_inner(
        &state,
        &body.repo_path,
        body.upstream_names,
    ))
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

    /// Deleting over HTTP must take the LIVE connection down, not just the record.
    ///
    /// `stop_if_running(&id)` stood in this handler and could only ever miss:
    /// the tunnel is keyed by the `TunnelProfile`'s own UUID, never the
    /// connection's. So a browser or remote client deleting a connected machine
    /// left the status poll, the mirror task and an `ssh` child running against
    /// a connection id that no longer named anything — with no way left to
    /// address them. The desktop command had no teardown at all.
    #[tokio::test]
    async fn deleting_a_connection_over_http_tears_the_live_one_down() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "vps",
            "http://remote.invalid:9876",
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(
            &state.data_dir,
            std::slice::from_ref(&connection),
        )
        .expect("seed the store the route is about to rewrite");

        // The ssh binary does not exist on purpose: the manager records the
        // tunnel before its child matters, which is all this test asks about.
        let tunnel_id = state
            .tunnel_manager
            .start_with_binary_for_test(
                crate::tunnels::profile::TunnelProfile::new("vps", "example.invalid", "boss"),
                std::path::PathBuf::from("/nonexistent/ssh"),
            )
            .await
            .expect("the manager records a tunnel before its ssh child matters");
        state
            .remote
            .force_connected_for_test(&id, "http://remote.invalid:9876", Some("tok"));
        state.remote.adopt_tunnel_for_test(&id, &tunnel_id);

        let resp = delete_remote_connection(
            ConnectInfo(loopback()),
            None,
            State(Arc::clone(&state)),
            Path(id.clone()),
        )
        .await
        .into_response();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            state.tunnel_manager.list().is_empty(),
            "the delete left a tunnel running: {:?}",
            state.tunnel_manager.list()
        );
        assert!(
            state.remote.snapshot().is_empty(),
            "the delete left the runtime holding the connection: {:?}",
            state.remote.snapshot()
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn stale_repository_delta_returns_http_conflict() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let original = serde_json::json!({"path":"/repo","displayName":"Original","branches":{}});
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": original.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");
        crate::config::save_repositories_request(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{
                "id":"/repo",
                "before":original.clone(),
                "after":{"path":"/repo","displayName":"First","branches":{}}
            }],
            "groups": []
        }))
        .expect("first mutation");

        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(super::super::tests::test_state()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{
                    "id":"/repo",
                    "before":original,
                    "after":{"path":"/repo","displayName":"Stale","branches":{}}
                }],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    /// The whole point of the write is the announcement: a second client only learns
    /// the document moved because this fires. Both tests subscribe before the request,
    /// so a broadcast dropped or made unconditional fails here rather than in a
    /// two-window session nobody can reproduce on demand.
    #[tokio::test]
    #[serial_test::serial]
    async fn an_accepted_delta_announces_the_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = super::super::tests::test_state();
        let mut events = state.event_bus.subscribe();

        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(state.clone()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{
                    "id":"/repo",
                    "before":null,
                    "after":{"path":"/repo","displayName":"Added","branches":{}}
                }],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            matches!(
                events.try_recv(),
                Ok(crate::state::AppEvent::RepositoriesChanged)
            ),
            "an accepted delta must announce itself on the event bus"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn a_delta_that_changes_nothing_stays_quiet() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let record = serde_json::json!({"path":"/repo","displayName":"Added","branches":{}});
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": record.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");
        let state = super::super::tests::test_state();
        let mut events = state.event_bus.subscribe();

        // Same record, already on disk: accepted, but nothing moved. Announcing it
        // would make every client re-read for nothing.
        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(state.clone()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{"id":"/repo","before":record.clone(),"after":record}],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            events.try_recv().is_err(),
            "a no-op delta must not announce a change"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn malformed_repository_delta_returns_http_bad_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(super::super::tests::test_state()),
            Json(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{"id":"/repo","before":null,"after":"not-an-object"}],
                "groups": []
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn unversioned_repository_document_returns_http_bad_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let response = put_repositories(
            ConnectInfo(loopback()),
            None,
            State(super::super::tests::test_state()),
            Json(serde_json::json!({"repos": {}, "repoOrder": []})),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[cfg(test)]
mod refresh_review_tests {
    use super::*;

    // Catches: an HTTP save returning success without invalidating that owner's clients.
    #[tokio::test]
    async fn every_http_config_save_wakes_its_settings_section() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut events = state.event_bus.subscribe();
        macro_rules! save {
            ($save:ident, $load:ident, $section:literal) => {{
                let base = crate::config::$load();
                let desired = crate::settings::refresh_test_support::edited($section, &base);
                let expected = serde_json::to_value(&desired).unwrap();
                let response = $save(
                    ConnectInfo("127.0.0.1:1234".parse().unwrap()), None, State(state.clone()),
                    Json(crate::config::ConfigSaveRequest { base, config: desired }),
                ).await.into_response();
                assert_eq!(response.status(), StatusCode::OK, $section);
                assert_eq!(serde_json::to_value(crate::config::$load()).unwrap(), expected);
                let event = events.try_recv().expect(concat!($section, " HTTP save did not publish"));
                assert!(matches!(event, crate::state::AppEvent::SettingsChanged(ref change) if change.section == $section && change.persisted && change.applied));
                assert!(events.try_recv().is_err(), "one save must publish once");
            }};
        }
        save!(
            put_notification_config,
            load_notification_config,
            "notifications"
        );
        save!(put_ui_prefs, load_ui_prefs, "preferences");
        save!(put_repo_settings, load_repo_settings, "repositories");
        save!(put_repo_defaults, load_repo_defaults, "repo_defaults");
        save!(put_prompt_library, load_prompt_library, "prompts");
        save!(put_agents_config, load_agents_config, "agents");
        save!(put_keybindings, load_keybindings, "keybindings");
        save!(put_pane_layout, load_pane_layout, "pane_layout");
    }
    // Catches: a transport retaining its own publisher after CRUD is shared, emitting duplicate generations.
    #[tokio::test]
    async fn http_connection_crud_publishes_once_per_persisted_change() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_ssh(
            "fixture",
            "not-contacted.example",
            "fixture",
        );
        let mut events = state.event_bus.subscribe();
        let response = put_remote_connection(
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            None,
            State(state.clone()),
            Json(crate::remote_connection::RemoteConnectionSaveRequest {
                base: None,
                connection: connection.clone(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let event = events
            .try_recv()
            .expect("HTTP connection save must publish");
        assert!(
            matches!(event, crate::state::AppEvent::SettingsChanged(ref change) if change.section == "connections" && change.persisted && change.applied)
        );
        assert!(events.try_recv().is_err());
        let response = delete_remote_connection(
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            None,
            State(state.clone()),
            Path(connection.id.clone()),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let event = events
            .try_recv()
            .expect("HTTP connection delete must publish");
        assert!(
            matches!(event, crate::state::AppEvent::SettingsChanged(ref change) if change.section == "connections" && change.persisted && change.applied)
        );
        assert!(events.try_recv().is_err());
        let response = delete_remote_connection(
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            None,
            State(state),
            Path(connection.id),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(events.try_recv().is_err());
    }
}
