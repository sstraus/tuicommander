use crate::AppState;
use crate::dictation::{self, DictationState};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;

pub(super) async fn get_dictation_status_http(
    State(state): State<Arc<AppState>>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    match dictation::commands::get_dictation_status(dictation) {
        Ok(status) => Json(status).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn get_model_info_http() -> impl IntoResponse {
    Json(dictation::commands::get_model_info())
}

#[derive(serde::Deserialize)]
pub(super) struct ModelNameRequest {
    pub model: String,
}

pub(super) async fn download_whisper_model_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ModelNameRequest>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    match dictation::commands::download_whisper_model(app, body.model).await {
        Ok(msg) => Json(serde_json::json!({"ok": true, "message": msg})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn delete_whisper_model_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ModelNameRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    match dictation::commands::delete_whisper_model(dictation, body.model) {
        Ok(msg) => Json(serde_json::json!({"ok": true, "message": msg})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn start_dictation_http(
    State(state): State<Arc<AppState>>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    match dictation::commands::start_dictation(app.clone(), dictation) {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn stop_dictation_http(
    State(state): State<Arc<AppState>>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    match dictation::commands::stop_dictation_and_transcribe(app).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn get_correction_map_http(
    State(state): State<Arc<AppState>>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::get_correction_map(dictation)).into_response()
}

#[derive(serde::Deserialize)]
pub(super) struct CorrectionMapRequest {
    pub map: HashMap<String, String>,
}

pub(super) async fn set_correction_map_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CorrectionMapRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    match dictation::commands::set_correction_map(dictation, body.map) {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn list_audio_devices_http() -> impl IntoResponse {
    Json(dictation::commands::list_audio_devices())
}

#[derive(serde::Deserialize)]
pub(super) struct InjectTextRequest {
    pub text: String,
}

pub(super) async fn inject_text_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InjectTextRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    match dictation::commands::inject_text(dictation, body.text) {
        Ok(text) => Json(serde_json::json!({"text": text})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn get_dictation_config_http() -> impl IntoResponse {
    Json(dictation::commands::get_dictation_config())
}

pub(super) async fn set_dictation_config_http(
    Json(config): Json<dictation::commands::DictationConfig>,
) -> Response {
    match dictation::commands::set_dictation_config(config) {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}
