//! HTTP mirrors of the `dictation::commands` Tauri commands.
//!
//! Every handler answers exactly what its IPC twin resolves to — a bare string
//! for `inject_text`, `null` for the `Result<(), String>` commands — because the
//! same store code (`src/stores/dictation.ts`) reads both transports.
//!
//! Desktop-only: `crate::dictation` is gated on the `desktop` feature, so the
//! routes are registered in the desktop half of `build_router`.

use crate::AppState;
use crate::dictation::{self, DictationState};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;

use super::json_result;

pub(super) async fn get_dictation_status_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::get_dictation_status(dictation))
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
    json_result(dictation::commands::download_whisper_model(app, body.model).await)
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
    json_result(dictation::commands::delete_whisper_model(
        dictation, body.model,
    ))
}

/// Named `asset` rather than `id` so the wire key says what it identifies —
/// the IPC twin's parameter is `asset` too, and the two must match.
#[derive(serde::Deserialize)]
pub(super) struct SpeechAssetRequest {
    pub asset: String,
}

pub(super) async fn get_speech_assets_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::get_speech_assets(dictation)).into_response()
}

pub(super) async fn download_speech_asset_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechAssetRequest>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    json_result(dictation::commands::download_speech_asset(app, body.asset).await)
}

pub(super) async fn cancel_speech_download_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechAssetRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::cancel_speech_download(
        dictation, body.asset,
    ))
}

pub(super) async fn delete_speech_asset_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechAssetRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::delete_speech_asset(
        dictation, body.asset,
    ))
}

/// `language` is a Whisper code, the same as the IPC twin's argument.
#[derive(serde::Deserialize)]
pub(super) struct SpeechVoicesQuery {
    pub language: String,
}

pub(super) async fn get_speech_voices_http(
    axum::extract::Query(query): axum::extract::Query<SpeechVoicesQuery>,
) -> Response {
    json_result(dictation::commands::get_speech_voices(query.language))
}

pub(super) async fn get_edge_voices_http(
    axum::extract::Query(query): axum::extract::Query<SpeechVoicesQuery>,
) -> Response {
    json_result(dictation::commands::get_edge_voices(query.language).await)
}

/// The same keys as the IPC twin's arguments, which Tauri spells in
/// camelCase (`dataBase64`). It is the whole voice file, which is why its
/// route has a larger body limit than the rest (see
/// `SPEECH_VOICE_IMPORT_BODY_BYTES`).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportSpeechVoiceRequest {
    pub language: String,
    pub name: String,
    pub data_base64: String,
}

pub(super) async fn import_speech_voice_http(
    Json(body): Json<ImportSpeechVoiceRequest>,
) -> Response {
    // Decoding and checking up to 64 MB is disk and CPU work; keep it off the
    // async workers.
    let result = tokio::task::spawn_blocking(move || {
        dictation::commands::import_speech_voice(body.language, body.name, body.data_base64)
    })
    .await
    .unwrap_or_else(|error| Err(format!("importing the voice failed: {error}")));
    json_result(result)
}

#[derive(serde::Deserialize)]
pub(super) struct DeleteSpeechVoiceRequest {
    pub language: String,
    pub name: String,
}

pub(super) async fn delete_speech_voice_http(
    Json(body): Json<DeleteSpeechVoiceRequest>,
) -> Response {
    json_result(dictation::commands::delete_speech_voice(
        body.language,
        body.name,
    ))
}

#[derive(serde::Deserialize)]
pub(super) struct PreviewSpeechVoiceRequest {
    pub language: String,
    pub voice: String,
    pub text: String,
}

pub(super) async fn preview_speech_voice_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PreviewSpeechVoiceRequest>,
) -> Response {
    let Some(app) = state.app_handle.read().clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    // Synthesis takes seconds; keep it off the async workers, as the IPC
    // twin keeps it off the main thread.
    let result = tokio::task::spawn_blocking(move || {
        dictation::commands::preview_voice(
            &app.state::<DictationState>(),
            &body.language,
            &body.voice,
            &body.text,
        )
    })
    .await
    .unwrap_or_else(|error| Err(format!("previewing the voice failed: {error}")));
    json_result(result)
}

/// `turn` is optional on the wire and on IPC: omitting it means "the turn that
/// is current now", which is what a caller answering immediately wants.
#[derive(serde::Deserialize)]
pub(super) struct SpeakRequest {
    pub text: String,
    #[serde(default)]
    pub turn: Option<u64>,
}

#[derive(serde::Deserialize)]
pub(super) struct SpeechStatusQuery {
    #[serde(default)]
    pub utterance: Option<String>,
}

pub(super) async fn speak_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeakRequest>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::speak_reply(
        dictation, body.text, body.turn,
    ))
}

pub(super) async fn stop_speech_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::stop_speech(dictation))
}

pub(super) async fn pause_speech_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::pause_speech(dictation))
}

pub(super) async fn resume_speech_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    json_result(dictation::commands::resume_speech(dictation))
}

pub(super) async fn get_speech_status_http(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<SpeechStatusQuery>,
) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::get_speech_status(
        dictation,
        query.utterance,
    ))
    .into_response()
}

#[derive(serde::Deserialize)]
pub(super) struct StartDictationBody {
    source: Option<String>,
}

pub(super) async fn start_dictation_http(
    State(state): State<Arc<AppState>>,
    body: Option<Json<StartDictationBody>>,
) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let source = body.and_then(|body| body.0.source);
    // Opening the capture stream is a synchronous CoreAudio call; keep it off
    // the async workers.
    let result = tokio::task::spawn_blocking(move || {
        let dictation = app.state::<DictationState>();
        dictation::commands::start_dictation(app.clone(), dictation, source)
    })
    .await
    .unwrap_or_else(|error| Err(format!("starting dictation failed: {error}")));
    json_result(result)
}

pub(super) async fn stop_dictation_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    json_result(dictation::commands::stop_dictation_and_transcribe(app).await)
}

pub(super) async fn get_correction_map_http(State(state): State<Arc<AppState>>) -> Response {
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
    let result = dictation::commands::save_correction_map(&dictation.corrections, body.map);
    if result.is_ok() {
        crate::settings::api::publish_change(&state, "corrections", true);
    }
    json_result(result)
}

pub(super) async fn list_audio_devices_http() -> Response {
    json_result(dictation::commands::list_audio_devices().await)
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
    json_result(dictation::commands::inject_text(dictation, body.text))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArmHandsFreeRequest {
    pub session_id: String,
    pub owner: String,
}

/// Browser/PWA counterpart of `arm_hands_free_dictation`.
///
/// The owner is the caller's own endpoint identity: a remote client that arms
/// here binds *itself*, so a later disconnect can disarm the mode it owns.
pub(super) async fn arm_hands_free_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ArmHandsFreeRequest>,
) -> Response {
    let Some(app) = state.app_handle.read().clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    // Arming opens the capture stream (synchronous CoreAudio); keep it off the
    // async workers.
    let result = tokio::task::spawn_blocking(move || {
        let dictation = app.state::<DictationState>();
        dictation::commands::arm_hands_free(&state, &dictation, &body.session_id, &body.owner)
    })
    .await
    .unwrap_or_else(|error| Err(format!("arming hands-free failed: {error}")));
    json_result(result)
}

pub(super) async fn disarm_hands_free_http(State(state): State<Arc<AppState>>) -> Response {
    let Some(app) = state.app_handle.read().clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    // ACP exit notices use the same synchronous voice sink as capture. Keep
    // its prompt acknowledgement and runtime join off the async workers.
    match tokio::task::spawn_blocking(move || {
        let dictation = app.state::<DictationState>();
        dictation::commands::disarm_hands_free(&state, &dictation)
    })
    .await
    {
        Ok(status) => Json(status).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

pub(super) async fn get_hands_free_status_http(State(state): State<Arc<AppState>>) -> Response {
    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    let dictation = app.state::<DictationState>();
    Json(dictation::commands::hands_free_status(&dictation)).into_response()
}

pub(super) async fn get_hands_free_default_notice_http() -> impl IntoResponse {
    Json(dictation::commands::get_hands_free_default_notice())
}

pub(super) async fn get_dictation_config_http() -> impl IntoResponse {
    Json(dictation::commands::get_dictation_config())
}

pub(super) async fn set_dictation_config_http(
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::config::ConfigSaveRequest<dictation::commands::DictationConfig>>,
) -> Response {
    // The state is passed when the app has it, so a language changed from a
    // browser cancels the reply being spoken on the desktop. Before startup
    // finishes there is no conversation to cancel, and the file is still
    // written — the setting is not lost, it simply has nothing live to affect.
    let result = {
        let app_handle = state.app_handle.read();
        let dictation = app_handle.as_ref().map(|app| app.state::<DictationState>());
        dictation::commands::save_dictation_config(
            request.base,
            request.config,
            dictation.as_deref(),
        )
    };
    if result.is_ok() {
        crate::settings::api::publish_change(&state, "voice", true);
    }
    json_result(result)
}

/// The audio socket a browser client holds while it wants a conversation.
///
/// One socket, both directions, because they share the client's lifetime: the
/// microphone that feeds a conversation and the speaker that answers it are the
/// same tab, and splitting them would let one of the two survive the other.
///
/// ## Wire format
///
/// | Direction | Frame | Meaning |
/// |---|---|---|
/// | client → server | binary | `f32` little-endian samples, mono, 16 kHz |
/// | client → server | text `{"type":"playback-ended","sequence":"1"}` | the reply finished playing |
/// | server → client | binary | `u32` little-endian sample rate, `u64` little-endian reply sequence, then `f32` samples |
/// | server → client | text `{"type":"stop"}` | stop playing and drop the queue |
///
/// Binary for audio and text for control, so a frame's kind already says which
/// it is: a length-prefixed union would put the whole conversation behind one
/// parser, and a mis-framed control message would then be played as sound.
///
/// Resampling to 16 kHz happens in the browser. Its `AudioContext` resamples
/// anyway, and sending 48 kHz here to downsample on this side would triple the
/// bytes for audio the segmenter discards.
pub(super) async fn hands_free_audio_ws(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(owner) = query
        .get("owner")
        .filter(|owner| !owner.is_empty())
        .cloned()
    else {
        return (StatusCode::BAD_REQUEST, "owner is required").into_response();
    };
    // The desktop microphone is opened by `arm`, not by a socket. Accepting
    // this name would register a link nothing can ever reach, and hide the
    // mistake behind a socket that looks connected.
    if owner == dictation::commands::DESKTOP_OWNER {
        return (
            StatusCode::BAD_REQUEST,
            "'desktop' is this machine's own audio and has no socket",
        )
            .into_response();
    }
    let app_handle = state.app_handle.read().clone();
    let Some(app) = app_handle else {
        return (StatusCode::SERVICE_UNAVAILABLE, "App not initialized").into_response();
    };
    ws.on_upgrade(move |socket| serve_hands_free_audio(socket, owner, app))
}

async fn serve_hands_free_audio(
    socket: axum::extract::ws::WebSocket,
    owner: String,
    app: tauri::AppHandle,
) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};

    let dictation = app.state::<DictationState>();
    let endpoints = dictation.browser_endpoints.clone();
    let link = endpoints.connect(&owner);
    let mut replies = link.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Two halves of one socket, joined below so either ending closes the
    // other: a client that stops sending audio has stopped talking, and a
    // client whose downlink failed cannot hear the answer to what it says next.
    let downlink = {
        let link = link.clone();
        tokio::spawn(async move {
            while let Ok(message) = replies.recv().await {
                let frame = match message {
                    dictation::browser::Downlink::Speak { sequence, audio } => {
                        Message::Binary(encode_reply(sequence, &audio).into())
                    }
                    dictation::browser::Downlink::Stop => {
                        Message::Text(r#"{"type":"stop"}"#.into())
                    }
                    dictation::browser::Downlink::Pause => {
                        Message::Text(r#"{"type":"pause"}"#.into())
                    }
                    dictation::browser::Downlink::Resume => {
                        Message::Text(r#"{"type":"resume"}"#.into())
                    }
                };
                if sender.send(frame).await.is_err() {
                    break;
                }
            }
            // Whatever ended the downlink — a closed socket or a dropped
            // sender — the client can no longer be spoken to, and a
            // conversation it still owns has to end.
            link.disconnect();
        })
    };

    while let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Binary(bytes) => {
                link.push_capture(&decode_samples(&bytes));
            }
            // Anything else is a control message this server does not know.
            // Ignored rather than fatal: a newer client must be able to send
            // one without losing its microphone.
            Message::Text(text) => {
                if let Ok(control) = serde_json::from_str::<serde_json::Value>(&text)
                    && control["type"] == "playback-ended"
                    && let Some(sequence) =
                        control["sequence"].as_str().and_then(|id| id.parse().ok())
                {
                    link.note_playback_ended(sequence);
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    endpoints.disconnect(&owner, &link);
    downlink.abort();
}

/// The server-owned downlink header, shared with browserVoice::decodeReply.
fn encode_reply(sequence: u64, audio: &dictation::speech::SpeechAudio) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12 + audio.samples.len() * 4);
    bytes.extend_from_slice(&audio.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    for sample in &audio.samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

// Catches: missing, truncated or misaligned sequence bytes play header bits as PCM.
#[test]
fn browser_reply_header_preserves_rate_sequence_and_pcm_bytes() {
    let audio = dictation::speech::SpeechAudio {
        sample_rate: 24_000,
        samples: vec![0.5, -0.5],
    };
    assert_eq!(
        encode_reply(9_007_199_254_740_993, &audio),
        vec![
            0xc0, 0x5d, 0, 0, // 24000 Hz
            1, 0, 0, 0, 0, 0, 0x20, 0, // 2^53 + 1, not rounded through f64
            0, 0, 0, 0x3f, 0, 0, 0, 0xbf, // +0.5, -0.5
        ]
    );
}

/// Little-endian `f32` samples, ignoring a trailing partial sample.
///
/// A truncated frame is a transport accident, not a message: the whole frame
/// would otherwise be discarded and the conversation would lose the speech in
/// it, which is a worse answer than one missing sample.
fn decode_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Attached AI Chat targets are host state, independent of the panel window.
pub(super) async fn get_voice_targets_http(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(dictation::adapters::chat_targets(&state))
}
