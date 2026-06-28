//! HTTP parity for the request/response (non-streaming) AI commands — story 068/070
//! "RPC-now" slice (see plans/http-parity-ai-event-bridge.md). Loopback router only,
//! matching the other AI/config surfaces. Streaming commands (start_conversation,
//! chat_subscribe) use dedicated WS endpoints in a later step.

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use parking_lot::RwLock;
use serde::Deserialize;
use std::sync::Arc;

use super::{err_500, json_result};
use crate::AppState;
use crate::ai_agent::watcher::{self, WatcherConfig, WatcherTrigger};

/// Shared accessor for the watcher config (mirrors the engine-get the Tauri
/// commands do). The rule mutations themselves live in `ai_agent::watcher::*_rule`.
fn watcher_cfg(state: &Arc<AppState>) -> Result<Arc<RwLock<WatcherConfig>>, String> {
    Ok(state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?
        .config())
}

#[derive(Deserialize)]
pub(super) struct WatcherCreateRequest {
    pub name: String,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub trigger: WatcherTrigger,
    pub instructions: Option<String>,
    #[serde(rename = "promptId")]
    pub prompt_id: Option<String>,
    #[serde(rename = "repoPath")]
    pub repo_path: Option<String>,
    #[serde(rename = "maxFires")]
    pub max_fires: Option<u32>,
    #[serde(rename = "cooldownSecs")]
    pub cooldown_secs: Option<u32>,
}

pub(super) async fn watcher_create_http(
    State(state): State<Arc<AppState>>,
    Json(b): Json<WatcherCreateRequest>,
) -> Response {
    json_result(crate::ai_agent::commands::watcher_create_impl(
        &state,
        b.name,
        b.session_id,
        b.trigger,
        b.instructions,
        b.prompt_id,
        b.repo_path,
        b.max_fires,
        b.cooldown_secs,
    ))
}

pub(super) async fn watcher_list_http(State(state): State<Arc<AppState>>) -> Response {
    match watcher_cfg(&state) {
        Ok(cfg) => json_result(Ok::<_, String>(cfg.read().rules.clone())),
        Err(e) => err_500(&e),
    }
}

#[derive(Deserialize)]
pub(super) struct WatcherIdRequest {
    pub id: String,
}

pub(super) async fn watcher_delete_http(
    State(state): State<Arc<AppState>>,
    Json(b): Json<WatcherIdRequest>,
) -> Response {
    match watcher_cfg(&state) {
        Ok(cfg) => json_result(watcher::delete_rule(&mut cfg.write(), &b.id)),
        Err(e) => err_500(&e),
    }
}

#[derive(Deserialize)]
pub(super) struct WatcherToggleRequest {
    pub id: String,
    pub enabled: bool,
}

pub(super) async fn watcher_toggle_http(
    State(state): State<Arc<AppState>>,
    Json(b): Json<WatcherToggleRequest>,
) -> Response {
    match watcher_cfg(&state) {
        Ok(cfg) => json_result(watcher::toggle_rule(&mut cfg.write(), &b.id, b.enabled)),
        Err(e) => err_500(&e),
    }
}

#[derive(Deserialize)]
pub(super) struct WatcherAttachRequest {
    #[serde(rename = "templateId")]
    pub template_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
}

pub(super) async fn watcher_attach_http(
    State(state): State<Arc<AppState>>,
    Json(b): Json<WatcherAttachRequest>,
) -> Response {
    match watcher_cfg(&state) {
        Ok(cfg) => json_result(watcher::attach_rule(
            &mut cfg.write(),
            &b.template_id,
            b.session_id,
        )),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn watcher_detach_http(
    State(state): State<Arc<AppState>>,
    Json(b): Json<WatcherIdRequest>,
) -> Response {
    match watcher_cfg(&state) {
        Ok(cfg) => json_result(watcher::detach_rule(&mut cfg.write(), &b.id)),
        Err(e) => err_500(&e),
    }
}

#[derive(Deserialize)]
pub(super) struct WatcherUpdateRequest {
    pub id: String,
    pub name: Option<String>,
    pub trigger: Option<WatcherTrigger>,
    pub instructions: Option<String>,
    #[serde(rename = "promptId")]
    pub prompt_id: Option<String>,
    #[serde(rename = "repoPath")]
    pub repo_path: Option<String>,
    #[serde(rename = "maxFires")]
    pub max_fires: Option<u32>,
    #[serde(rename = "cooldownSecs")]
    pub cooldown_secs: Option<u32>,
}

pub(super) async fn watcher_update_http(
    State(state): State<Arc<AppState>>,
    Json(b): Json<WatcherUpdateRequest>,
) -> Response {
    json_result(crate::ai_agent::commands::watcher_update_impl(
        &state,
        b.id,
        b.name,
        b.trigger,
        b.instructions,
        b.prompt_id,
        b.repo_path,
        b.max_fires,
        b.cooldown_secs,
    ))
}
