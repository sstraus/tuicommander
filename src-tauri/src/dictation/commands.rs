use super::{
    DictationState, audio, browser, continuous, corrections, echo, model, permission, speaker,
    speech, streaming, transcribe,
};
pub use crate::config::DictationConfig;
pub(crate) use crate::config::default_hold_back_ms;
use serde::{Serialize, de::DeserializeOwned};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Helper to reset recording flag on error paths.
struct RecordingGuard<'a> {
    recording: &'a std::sync::atomic::AtomicBool,
    native_release_pending: &'a std::sync::atomic::AtomicBool,
    fn_capture: &'a std::sync::atomic::AtomicBool,
    stop_request: &'a parking_lot::Mutex<Option<super::CaptureStopRequest>>,
    disarmed: bool,
}

impl<'a> RecordingGuard<'a> {
    fn new(dictation: &'a DictationState) -> Self {
        Self {
            recording: &dictation.recording,
            native_release_pending: &dictation.native_release_pending,
            fn_capture: &dictation.fn_capture,
            stop_request: &dictation.stop_request,
            disarmed: false,
        }
    }
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for RecordingGuard<'_> {
    fn drop(&mut self) {
        if !self.disarmed {
            self.recording.store(false, Ordering::Release);
            self.native_release_pending.store(false, Ordering::Release);
            self.fn_capture.store(false, Ordering::Release);
            self.stop_request.lock().take();
        }
    }
}

/// Below this ratio of final-pass text to streaming-partial text, the final pass
/// returned less than the streaming windows already had — which is what window
/// tail loss looks like from the outside.
///
/// Set at 0.9 rather than 0.8 because the incident that motivated the warning
/// landed at 81.5% (full=1175 against composed=1442) and an 80% line would have
/// stayed silent on it. A warning that misses the case it exists for is worth
/// less than an occasional false positive, which costs one log line.
const SHORT_TRANSCRIPTION_RATIO: f64 = 0.9;

/// How much of the streaming partials survived into the final transcription.
///
/// Characters on both sides. `String::len()` counts bytes, so an accented
/// dictation measures longer than it reads and any ratio built on it lies. The
/// metric this replaced compared a common-PREFIX character count against a byte
/// length: one differing leading space reported 0% and said nothing at all about
/// how much text was missing.
///
/// `None` when there are no partials to compare against.
fn transcription_ratio(full: &str, composed: &str) -> Option<f64> {
    let composed_chars = composed.chars().count();
    if composed_chars == 0 {
        return None;
    }
    Some(full.chars().count() as f64 / composed_chars as f64)
}

/// RAII guard that resets the processing flag to false on drop (including panic).
/// Holds an `Arc<AtomicBool>` so it can be moved into `spawn_blocking`.
struct ProcessingGuard(Arc<std::sync::atomic::AtomicBool>);

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_logger;

#[derive(Debug, Clone, Serialize)]
pub struct DictationStatus {
    pub model_status: String, // "not_downloaded", "ready", "error"
    pub model_name: String,
    pub model_size_mb: u64,
    pub recording: bool,
    pub processing: bool,
    /// Normalized 0.0–1.0 microphone level while recording.
    pub audio_level: f32,
    /// Another instance holds dictation for this config directory.
    pub owned_elsewhere: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub size_hint_mb: u64,
    pub downloaded: bool,
    pub actual_size_mb: u64,
}

/// Result returned by stop_dictation_and_transcribe with metadata for user feedback.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResponse {
    /// The transcribed (and corrected) text, empty if skipped.
    pub text: String,
    /// Human-readable reason when text is empty (None on success).
    pub skip_reason: Option<String>,
    /// Duration of the audio that reached the final transcription, in seconds.
    pub duration_s: f64,
    /// Seconds of speech the recording cap dropped before that transcription.
    /// Zero for any ordinary recording; non-zero means the text is missing its
    /// beginning, and the UI must say so rather than pass off a partial answer.
    pub truncated_s: f64,
}

/// The response at the stop command boundary when the final pass has no text.
fn empty_final_response(
    final_text: &str,
    final_skip_reason: Option<String>,
    duration_s: f64,
    truncated_s: f64,
) -> Option<TranscribeResponse> {
    final_text.is_empty().then(|| TranscribeResponse {
        text: String::new(),
        skip_reason: Some(final_skip_reason.unwrap_or_else(|| "no speech detected".to_string())),
        duration_s,
        truncated_s,
    })
}

/// The utterance boundaries both voice modes use. Frame activity follows the
/// Settings > Voice floor, so a quiet microphone tuned there opens utterances
/// in push-to-talk and hands-free alike.
fn segmenter_config(gates: transcribe::VoiceGates) -> continuous::SegmenterConfig {
    continuous::SegmenterConfig {
        activity_rms: gates.rms_threshold,
        ..continuous::SegmenterConfig::default()
    }
}

/// Run the final push-to-talk pass after capture has assembled the recording.
fn transcribe_final_ptt_audio(
    transcriber: &dyn transcribe::Transcriber,
    audio: &[f32],
    language: Option<&str>,
    gates: transcribe::VoiceGates,
) -> Result<transcribe::TranscribeResult, String> {
    let activity = segmenter_config(gates);
    if !continuous::has_sustained_speech(audio, activity) {
        return Ok(transcribe::TranscribeResult {
            text: String::new(),
            skip_reason: Some(format!(
                "no sustained speech (need {}ms of active audio)",
                activity.min_speech_ms
            )),
            language: None,
        });
    }
    transcriber.transcribe(audio, language, gates)
}

/// Resolve a model name from config, falling back to the default.
fn resolve_model(name: &str) -> model::WhisperModel {
    model::WhisperModel::from_name(name).unwrap_or(model::WhisperModel::LargeV3Turbo)
}

/// The model-derived half of [`DictationStatus`]: which model is configured,
/// whether it is on disk and how big the file is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelSnapshot {
    model: model::WhisperModel,
    downloaded: bool,
    size_mb: u64,
}

/// Cache slot for [`model_snapshot`]: the snapshot and when it was taken.
static MODEL_SNAPSHOT: parking_lot::Mutex<Option<(ModelSnapshot, std::time::Instant)>> =
    parking_lot::Mutex::new(None);

/// How long a snapshot may be served before it is recomputed.
///
/// The commands in this process invalidate explicitly, but they are not the only
/// writer: a debug build and the installed app share one configuration directory
/// and one model directory, so the other process can change the selected model,
/// download it or delete it with nothing to tell us. Without an expiry this cache
/// served that stale answer forever. One second keeps the 75 ms meter tick off
/// the config file — the reason the cache exists — while bounding how long a
/// change made elsewhere can go unnoticed.
const MODEL_SNAPSHOT_TTL: std::time::Duration = std::time::Duration::from_secs(1);

/// Snapshot of the configured model for [`get_dictation_status`].
///
/// Computing one costs a `dictation-config.json` read, a JSON parse and two
/// `stat` calls. The microphone meter polls `get_dictation_status` every 75 ms
/// while recording (`startAudioLevelPolling` in `src/stores/dictation.ts`), so
/// paying that per tick means ~13 config parses a second on the IPC thread.
fn model_snapshot() -> ModelSnapshot {
    let mut slot = MODEL_SNAPSHOT.lock();
    if let Some((snapshot, taken)) = slot.as_ref()
        && taken.elapsed() < MODEL_SNAPSHOT_TTL
    {
        return snapshot.clone();
    }
    let model = resolve_model(&get_dictation_config().model);
    let snapshot = ModelSnapshot {
        model,
        downloaded: model::model_exists(model),
        size_mb: model::model_size_bytes(model) / 1_048_576,
    };
    *slot = Some((snapshot.clone(), std::time::Instant::now()));
    snapshot
}

/// Drop the cached snapshot after the configured model or a model file changed.
fn invalidate_model_snapshot() {
    *MODEL_SNAPSHOT.lock() = None;
}

#[tauri::command]
pub fn get_dictation_status(
    dictation: State<'_, DictationState>,
) -> Result<DictationStatus, String> {
    Ok(dictation_status(&dictation))
}

fn dictation_status(dictation: &DictationState) -> DictationStatus {
    let snapshot = model_snapshot();
    let has_transcriber = dictation.transcriber_arc.lock().is_some();

    let model_status = if !snapshot.downloaded {
        "not_downloaded"
    } else if has_transcriber {
        "ready"
    } else {
        "downloaded" // Downloaded but not loaded yet
    };
    // Read one at a time: nesting the two locks would add a lock order that
    // nothing else in the module follows.
    let push_to_talk_level = dictation
        .audio
        .lock()
        .as_ref()
        .map(audio::AudioCapture::level);
    let hands_free_level = dictation
        .hands_free_audio
        .lock()
        .as_ref()
        .map(audio::AudioCapture::level);

    DictationStatus {
        model_status: model_status.to_string(),
        model_name: snapshot.model.name().to_string(),
        model_size_mb: snapshot.size_mb,
        recording: dictation.recording.load(Ordering::Acquire),
        processing: dictation.processing.load(Ordering::Acquire),
        audio_level: capture_level(push_to_talk_level, hands_free_level),
        owned_elsewhere: !dictation.is_owner(),
    }
}

/// The microphone level to show: push-to-talk's capture when it is open,
/// otherwise the hands-free one. Only one listens at a time; without the
/// fallback a hands-free conversation reads as a silent microphone.
fn capture_level(push_to_talk: Option<f32>, hands_free: Option<f32>) -> f32 {
    push_to_talk.or(hands_free).unwrap_or(0.0)
}

#[tauri::command]
pub fn get_model_info() -> Vec<ModelInfo> {
    model::WhisperModel::ALL
        .iter()
        .map(|m| ModelInfo {
            name: m.name().to_string(),
            display_name: m.display_name().to_string(),
            size_hint_mb: m.size_hint_mb(),
            downloaded: model::model_exists(*m),
            actual_size_mb: model::model_size_bytes(*m) / 1_048_576,
        })
        .collect()
}

#[tauri::command]
pub async fn download_whisper_model(app: AppHandle, model_name: String) -> Result<String, String> {
    let whisper_model = model::WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Unknown model: {model_name}"))?;

    if model::model_exists(whisper_model) {
        return Ok("Model already downloaded".to_string());
    }

    let app_clone = app.clone();
    let path = super::model_download::download_model(whisper_model, move |downloaded, total| {
        let payload = download_progress(None, downloaded, total);
        let _ = app_clone.emit(DICTATION_DOWNLOAD_PROGRESS, payload.clone());
        push_to_bus(
            &app_clone,
            crate::state::AppEvent::DictationDownloadProgress { payload },
        );
    })
    .await?;

    // The model is on disk now — its size and download state are cached.
    invalidate_model_snapshot();

    Ok(format!("Downloaded to {}", path.display()))
}

#[tauri::command]
pub fn delete_whisper_model(
    dictation: State<'_, DictationState>,
    model_name: String,
) -> Result<String, String> {
    let whisper_model = model::WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Unknown model: {model_name}"))?;

    // Unload transcriber if it's the active model
    let active = dictation.active_model.lock().clone();
    if active.as_deref() == Some(whisper_model.name()) {
        *dictation.transcriber_arc.lock() = None;
        *dictation.active_model.lock() = None;
    }

    model::delete_model(whisper_model)?;
    // The model file is gone — its size and download state are cached.
    invalidate_model_snapshot();
    Ok(format!("Deleted {}", whisper_model.display_name()))
}

// ---------------------------------------------------------------------------
// Speech assets — the voices and graphs behind spoken replies
// ---------------------------------------------------------------------------

/// What the settings panel needs to know about one downloadable asset.
///
/// `state` is a string rather than a bool pair because the four states are not
/// independent: an asset cannot be both downloading and incomplete as far as
/// the UI is concerned, and modelling them separately invites a panel that
/// renders "not installed" over a running progress bar.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SpeechAssetInfo {
    pub id: String,
    pub display_name: String,
    /// `"language"`, `"runtime"` or `"voice"`.
    pub kind: String,
    /// The **Whisper language code** this speaks (`"it"`), absent for the
    /// runtime library. For a voice, the code of the language it belongs to.
    ///
    /// The code rather than the engine's own name for the language
    /// (`"italian"`), because this is the field a caller joins against: the
    /// dictation setting, `SpeechStatus.language` and `for_language_code` all
    /// speak in codes. Publishing the engine name here made the settings panel
    /// compare `"italian"` against `"it"`, find no asset for the configured
    /// language, and report that no bundle ships for it while listing the
    /// bundle one row above.
    pub language: Option<String>,
    pub voices: Vec<String>,
    /// The voice a `"voice"` asset downloads (`"jean"`); absent otherwise.
    pub voice: Option<String>,
    pub download_bytes: u64,
    /// `"absent"`, `"downloading"`, `"incomplete"` or `"ready"`.
    pub state: String,
    /// Which files an incomplete asset is missing. Empty otherwise.
    pub missing: Vec<String>,
}

fn describe(asset: &speech::assets::Asset, downloading: bool) -> SpeechAssetInfo {
    use speech::assets::Status;
    let (state, missing) = if downloading {
        // Checked before the disk: a download in flight has a staging
        // directory and an install directory that still holds the old version,
        // so the on-disk answer would be the answer to a different question.
        ("downloading".to_string(), Vec::new())
    } else {
        match speech::assets::status(asset) {
            Status::Absent => ("absent".to_string(), Vec::new()),
            Status::Ready => ("ready".to_string(), Vec::new()),
            Status::Incomplete { missing } => ("incomplete".to_string(), missing),
        }
    };
    SpeechAssetInfo {
        id: asset.id.to_string(),
        display_name: asset.display_name.to_string(),
        kind: if asset.language().is_some() {
            "language".to_string()
        } else if asset.voice().is_some() {
            "voice".to_string()
        } else {
            "runtime".to_string()
        },
        language: asset
            .code()
            .or_else(|| speech::assets::language_of(asset).and_then(|language| language.code()))
            .map(str::to_string),
        voices: asset.voices().iter().map(|v| (*v).to_string()).collect(),
        voice: asset.voice().map(str::to_string),
        download_bytes: asset.download_bytes(),
        state,
        missing,
    }
}

/// Everything a user may install, and what state it is in: the runtime and
/// the languages first, then every downloadable voice (`kind: "voice"`, with
/// its language's code), so the settings page can list a language's voices
/// under it without a second request.
#[tauri::command]
pub fn get_speech_assets(dictation: tauri::State<'_, DictationState>) -> Vec<SpeechAssetInfo> {
    speech::assets::every_asset()
        .map(|asset| describe(asset, dictation.speech.is_downloading(asset.id)))
        .collect()
}

/// Look an id up in the catalogue, refusing anything that is not in it.
///
/// This is the allowlist boundary: past here an id has become a `&'static
/// Asset` with a pinned URL and a pinned hash, so nothing a caller sends can
/// name a path or a host of its own.
fn resolve_asset(id: &str) -> Result<&'static speech::assets::Asset, String> {
    speech::assets::find(id).ok_or_else(|| format!("Unknown speech asset: {id}"))
}

#[tauri::command]
pub async fn download_speech_asset(app: AppHandle, asset: String) -> Result<String, String> {
    let target = resolve_asset(&asset)?;
    // Cloned out of the managed state in its own scope: a `State` guard held
    // across an await would make this future non-`Send`, and the download is
    // minutes long.
    let library = {
        let dictation = app.state::<DictationState>();
        Arc::clone(&dictation.speech)
    };

    let id = target.id.to_string();
    let progress_app = app.clone();
    let progress_id = id.clone();
    let installed = library
        .install(
            target,
            move |downloaded, total| {
                emit_speech_download(
                    &progress_app,
                    download_progress(Some(&progress_id), downloaded, total),
                );
            },
            |asset, cancel, on_progress| async move {
                super::asset_download::stage(asset, &cancel, on_progress).await
            },
        )
        .await;

    // Sent on success and on failure alike. Only the caller that started a
    // download has a return value to clear its bar with; every other client —
    // a browser, a second window, a script on the HTTP API — learns it ended
    // from this alone, and without it shows the last percent forever.
    emit_speech_download(&app, download_finished(&id));

    let path = installed.map_err(|error| error.to_string())?;
    Ok(format!("Installed to {}", path.display()))
}

/// Emit one speech-download event on both transports.
fn emit_speech_download(app: &AppHandle, payload: serde_json::Value) {
    let _ = app.emit(SPEECH_DOWNLOAD_PROGRESS, payload.clone());
    push_to_bus(
        app,
        crate::state::AppEvent::SpeechDownloadProgress { payload },
    );
}

/// The last event of a speech-asset download, whatever its outcome.
///
/// It carries no outcome on purpose: the catalogue is the one place that says
/// whether the asset is now ready, and a client re-reads it on `done`.
fn download_finished(asset: &str) -> serde_json::Value {
    serde_json::json!({ "asset": asset, "done": true })
}

/// The event a speech-asset download reports progress on.
pub const SPEECH_DOWNLOAD_PROGRESS: &str = "speech-download-progress";

/// The event a Whisper-model download reports progress on.
pub const DICTATION_DOWNLOAD_PROGRESS: &str = "dictation-download-progress";

/// The event a spoken reply reports its state on.
pub const SPEECH_UTTERANCE: &str = "speech-utterance";

/// The body both download events carry, built once.
///
/// `asset` is present only for a speech asset: a Whisper download has no id
/// because only one runs at a time, while the runtime library and a language
/// bundle can download together and a shared percent would show each of them
/// the other's.
fn download_progress(asset: Option<&str>, downloaded: u64, total: u64) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "downloaded": downloaded,
        "total": total,
        "percent": if total > 0 { (downloaded as f64 / total as f64 * 100.0) as u32 } else { 0 },
    });
    if let Some(asset) = asset {
        payload["asset"] = serde_json::Value::String(asset.to_string());
    }
    payload
}

/// Publish on the `/events` bus beside the desktop `emit`.
///
/// Both, never one: there is no bus-to-window forwarder, so a producer that
/// sends only to the bus goes silent on the desktop, and one that only emits
/// goes silent in a browser. A missing `AppState` is not an error — the
/// headless `tuic-remote` build has a bus and no window, and a test harness has
/// neither.
#[cfg(feature = "desktop")]
fn push_to_bus(app: &AppHandle, event: crate::state::AppEvent) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<Arc<crate::state::AppState>>() {
        let _ = state.event_bus.send(event);
    }
}

/// Reports every utterance transition on both transports.
///
/// It exists because the transitions worth reporting have no caller to return
/// to: `finished` is decided by the render thread once the device drains, and
/// `interrupted` by whoever talked over the reply. A client without this is a
/// client that polls `speech_status`.
#[cfg(feature = "desktop")]
struct PushUtterance {
    app: AppHandle,
}

#[cfg(feature = "desktop")]
impl speaker::UtteranceObserver for PushUtterance {
    fn changed(&self, id: speaker::UtteranceId, state: &speaker::Utterance, generation: u64) {
        // The same struct `speak` returns and `speech_status` nests, serialized
        // once for both transports. Three builders for one shape is how the
        // three descriptions of a reply would drift.
        let Ok(payload) = serde_json::to_value(SpokenReply::new(id, state, generation)) else {
            return;
        };
        let _ = self.app.emit(SPEECH_UTTERANCE, payload.clone());
        push_to_bus(
            &self.app,
            crate::state::AppEvent::SpeechUtterance { payload },
        );
    }
}

/// Send utterance transitions to the desktop window and the `/events` bus.
///
/// Called once at startup, before any conversation can be armed. Installing it
/// later would be a conversation whose replies are invisible to a browser.
#[cfg(feature = "desktop")]
pub fn install_utterance_observer(app: &AppHandle) {
    use tauri::Manager;
    *app.state::<DictationState>().utterance_observer.lock() =
        Some(Arc::new(PushUtterance { app: app.clone() }));
}

#[tauri::command]
pub fn cancel_speech_download(
    dictation: tauri::State<'_, DictationState>,
    asset: String,
) -> Result<String, String> {
    let target = resolve_asset(&asset)?;
    if dictation.speech.cancel_download(target.id) {
        Ok(format!("Cancelled {}", target.display_name))
    } else {
        // Not an error the user caused: a download that finished between the
        // click and the command is the common way to get here.
        Ok(format!("{} was not downloading", target.display_name))
    }
}

#[tauri::command]
pub fn delete_speech_asset(
    dictation: tauri::State<'_, DictationState>,
    asset: String,
) -> Result<String, String> {
    let target = resolve_asset(&asset)?;
    dictation
        .speech
        .delete(target)
        .map_err(|error| error.to_string())?;
    Ok(format!("Deleted {}", target.display_name))
}

/// The language a voice command names, by its Whisper code (`"it"`) — the
/// same alphabet as `SpeechAssetInfo.language` and the dictation setting.
fn voice_language(language: &str) -> Result<&'static speech::assets::Asset, String> {
    speech::assets::for_language_code(language)
        .ok_or_else(|| format!("No speech bundle ships for language \"{language}\""))
}

/// The longest base64 payload an import accepts: what the voice size cap
/// encodes to. Checked on the string, before decoding, so an oversized upload
/// is refused without allocating it a second time.
fn speech_voice_base64_limit() -> usize {
    speech::assets::MAX_USER_VOICE_BYTES.div_ceil(3) * 4
}

/// The voices a language can speak with right now, and where each comes
/// from (`language` is its Whisper code). Read-only: what the voice picker
/// lists. See [`speech::library::available_voices`].
#[tauri::command]
pub fn get_speech_voices(language: String) -> Result<Vec<speech::library::VoiceChoice>, String> {
    let asset = voice_language(&language)?;
    Ok(speech::library::available_voices(asset))
}

/// The Microsoft Edge voices that speak a language (`language` is its Whisper
/// code), from the service's own list. Needs the network the first time and
/// says so when it is missing; what the voice picker lists under the Edge engine.
#[tauri::command]
pub async fn get_edge_voices(language: String) -> Result<Vec<speech::edge::EdgeVoice>, String> {
    super::edge_voices::voices_for_language(&language).await
}

/// Import a voice file the user chose into a language (`language` is its
/// Whisper code). The file travels as base64 so the payload is the same JSON
/// over IPC and over HTTP. See [`speech::library::import_speech_voice`] for
/// what is checked before anything is stored.
#[tauri::command]
pub fn import_speech_voice(
    language: String,
    name: String,
    data_base64: String,
) -> Result<String, String> {
    use base64::Engine as _;
    let asset = voice_language(&language)?;
    if data_base64.len() > speech_voice_base64_limit() {
        return Err(format!(
            "the voice file is over the {} MB limit",
            speech::assets::MAX_USER_VOICE_BYTES / (1024 * 1024)
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|error| format!("the voice file is not valid base64: {error}"))?;
    speech::library::import_speech_voice(asset, &name, &bytes)?;
    Ok(format!("Imported the {} voice {name}", asset.display_name))
}

/// Delete a voice file the user imported into a language (`language` is its
/// Whisper code). Absent is success.
#[tauri::command]
pub fn delete_speech_voice(language: String, name: String) -> Result<String, String> {
    let asset = voice_language(&language)?;
    speech::library::delete_speech_voice(asset, &name)?;
    Ok(format!("Deleted the {} voice {name}", asset.display_name))
}

// ---------------------------------------------------------------------------
// Spoken replies (817-f67c)
// ---------------------------------------------------------------------------

/// The engine that speaks replies. Edge is the default; Pocket TTS and the
/// external command live in the Expert section of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpeechEngine {
    Edge,
    Pocket,
    External,
}

/// What a configuration written before `speech_engine` existed meant, and
/// what a fresh one means.
///
/// A configured command or a chosen Pocket voice is a choice, and so is a
/// Pocket runtime on disk: nobody downloads 100+ MB they do not intend to use.
/// Those keep their engine; everyone else gets Edge. Only read while
/// `speech_engine` is unset, and the settings panel writes it as soon as the
/// user picks one, so installing Pocket later never flips a fresh install.
fn legacy_speech_engine(config: &DictationConfig, pocket_installed: bool) -> SpeechEngine {
    if !config.speech_command.is_empty() {
        SpeechEngine::External
    } else if !config.speech_voice.is_empty() || pocket_installed {
        SpeechEngine::Pocket
    } else {
        SpeechEngine::Edge
    }
}

fn speech_engine(config: &DictationConfig) -> SpeechEngine {
    match config.speech_engine.as_str() {
        "pocket" => SpeechEngine::Pocket,
        "external" => SpeechEngine::External,
        _ => SpeechEngine::Edge,
    }
}

/// `config` with `speech_engine` filled in, so every reader — the engine
/// choice here and the settings panel — sees one answer.
fn with_resolved_engine(mut config: DictationConfig) -> DictationConfig {
    if !matches!(
        config.speech_engine.as_str(),
        "edge" | "pocket" | "external"
    ) {
        let pocket_installed =
            speech::assets::status(speech::assets::runtime()) != speech::assets::Status::Absent;
        config.speech_engine = match legacy_speech_engine(&config, pocket_installed) {
            SpeechEngine::Edge => "edge",
            SpeechEngine::Pocket => "pocket",
            SpeechEngine::External => "external",
        }
        .to_string();
    }
    config
}

/// The engine and voice a reply would be spoken with, or why there is none.
///
/// Every `Err` here is a setup problem stated in the user's terms, because
/// every one of them reaches a model as "unavailable, and here is why" rather
/// than as a failure it should retry.
fn speech_service(config: &DictationConfig) -> &'static str {
    match speech_engine(config) {
        SpeechEngine::Edge => "Microsoft Edge",
        SpeechEngine::Pocket => "Pocket TTS",
        SpeechEngine::External => "External speech engine",
    }
}

fn open_voice(
    config: &DictationConfig,
    library: &speech::library::SpeechLibrary,
    language: &str,
) -> Result<(Arc<dyn speech::Speech>, String), String> {
    let (engine, voice) = build_voice(config, library, language)?;
    Ok((
        Arc::new(speech::rejection::GuardedSpeech::new(
            engine,
            library.rejections.clone(),
            speech_service(config),
            None,
        )),
        voice,
    ))
}

fn build_voice(
    config: &DictationConfig,
    library: &speech::library::SpeechLibrary,
    language: &str,
) -> Result<(Arc<dyn speech::Speech>, String), String> {
    match speech_engine(config) {
        SpeechEngine::External => {
            // The user's own engine. It names its own voices inside its template,
            // so there is nothing here to choose between and the voice is empty.
            let engine = speech::external::ExternalSpeech::new(config.speech_command.clone())
                .map_err(|error| error.to_string())?;
            return Ok((Arc::new(engine), String::new()));
        }
        SpeechEngine::Edge => {
            let voice = speech::edge::choose_voice(language, &config.speech_edge_voice)?;
            return Ok((Arc::new(speech::edge::EdgeSpeech::new()), voice));
        }
        SpeechEngine::Pocket => {}
    }

    let asset = speech::assets::for_language_code(language).ok_or_else(|| {
        // Named rather than swapped for one we do ship. Speaking Italian into
        // an English conversation is worse than saying nothing, and a model
        // that is told *why* can write its reply as text instead.
        format!("No speech bundle ships for language \"{language}\"")
    })?;
    let runtime = speech::assets::runtime();
    for needed in [runtime, asset] {
        match speech::assets::status(needed) {
            speech::assets::Status::Ready => {}
            speech::assets::Status::Absent => {
                return Err(format!("{} is not downloaded", needed.display_name));
            }
            speech::assets::Status::Incomplete { missing } => {
                return Err(format!(
                    "{} is incomplete; missing {}",
                    needed.display_name,
                    missing.join(", ")
                ));
            }
        }
    }
    let voice = choose_voice(asset, &config.speech_voice)?;
    // From the library rather than built here: it is the one instance that
    // serialises replacing a language against speaking it, and an engine built
    // beside it would hold the very files a download is about to rename away.
    let engine = library.engine(asset.language().unwrap_or_default());
    Ok((engine, voice))
}

/// Which of a language's voices to speak with.
///
/// Empty means "whatever this language ships first", which is what an
/// untouched configuration says and what every configuration said before the
/// setting existed.
///
/// A named voice must be one the language holds — the one it ships, a
/// catalogue voice that is downloaded, or a file the user imported
/// ([`speech::library::installed_voices`]). Anything else is an error rather
/// than a silent fall back to the first one. The ways to get here are a voice
/// that was deleted, a download that is not there yet, and a language the user
/// changed underneath the setting; all are cases where speaking in a voice
/// nobody chose is worse than saying why nothing was spoken, and the message
/// reaches the user through the hands-free status rather than being buried in
/// a log.
fn choose_voice(asset: &speech::assets::Asset, configured: &str) -> Result<String, String> {
    if configured.is_empty() {
        return asset
            .voices()
            .first()
            .map(|voice| (*voice).to_string())
            .ok_or_else(|| format!("{} ships no voice", asset.display_name));
    }
    let available = speech::library::installed_voices(asset);
    if available.iter().any(|choice| choice.id == configured) {
        return Ok(configured.to_string());
    }
    let language = asset.language().unwrap_or_default();
    if speech::assets::downloadable_voices(language).any(|voice| voice.voice() == Some(configured))
    {
        // The catalogue has it, so the fix is one download away.
        return Err(format!(
            "The {} voice \"{configured}\" is not downloaded; download it in Settings → Voice",
            asset.display_name
        ));
    }
    Err(format!(
        "{} has no voice called \"{configured}\"; it offers {}",
        asset.display_name,
        if available.is_empty() {
            "none".to_string()
        } else {
            available
                .iter()
                .map(|choice| choice.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    ))
}

/// The language this conversation is being held in.
///
/// A fixed setting answers for every turn, including the first. `auto` can only
/// be answered by whoever spoke: [`HandsFree::turn_language`] carries what
/// Whisper made of the last turn, and before anybody has spoken there is no
/// answer at all — which is `None`, never a default.
///
/// Reading the mode is why this must not be called with the speaker lock held.
fn conversation_language(config: &DictationConfig, dictation: &DictationState) -> Option<String> {
    if config.language != "auto" {
        return Some(config.language.clone());
    }
    dictation
        .hands_free
        .lock()
        .turn_language()
        .map(str::to_string)
}

/// The language a voice must be opened for, or why none can be.
///
/// Empty is a real answer here and means "not language-specific": a
/// user-supplied engine picks its own language inside its command template, so
/// there is nothing for us to choose and nothing that a change of detected
/// language invalidates.
fn speech_language(config: &DictationConfig, dictation: &DictationState) -> Result<String, String> {
    if speech_engine(config) == SpeechEngine::External {
        return Ok(String::new());
    }
    conversation_language(config, dictation).ok_or_else(|| {
        "Dictation language is Auto and nothing has been said yet, so there is no language to \
         speak in"
            .to_string()
    })
}

/// The persisted preference and service health are enforced before queueing.
fn replies_available(config: &DictationConfig, dictation: &DictationState) -> Result<(), String> {
    if !config.hands_free_spoken_replies {
        return Err("Spoken replies are off; reply in text".into());
    }
    if let Some(reason) = dictation.speech.rejections.reason(speech_service(config)) {
        return Err(reason);
    }
    Ok(())
}

/// Build the reply queue for a conversation that is being armed.
///
/// Failure is not fatal to arming: hands-free without a voice is dictation,
/// which still works. The reason travels back so the caller can say it once
/// rather than leaving the model to discover it on its first `speak`.
///
/// Under Auto this fails at arm time by design — nobody has spoken, so no
/// language is known. The queue is then built by the first [`speak`] that finds
/// one, which is why that path must not assume this one succeeded.
pub(crate) fn open_speaker(
    dictation: &DictationState,
    generation: u64,
) -> Result<speaker::Armed, String> {
    let config = get_dictation_config();
    replies_available(&config, dictation)?;
    let language = speech_language(&config, dictation)?;
    let owner = conversation_owner(dictation);
    open_speaker_for(
        dictation,
        generation,
        &config,
        &language,
        owner.as_deref(),
        generation,
    )
}

/// Who armed the conversation, if one is armed.
///
/// Read through the mode lock and handed on as a plain string rather than
/// re-read deeper in: the reply queue is built with the speaker lock held, and
/// taking the mode lock there would invert the order every other path uses.
fn conversation_owner(dictation: &DictationState) -> Option<String> {
    dictation
        .hands_free
        .lock()
        .binding()
        .map(|binding| binding.owner.clone())
}

/// Where a conversation's replies come out.
///
/// The owner decides, and there is no fallback: a reply for a browser-owned
/// conversation whose client has gone is a failure, never something the server
/// speakers pick up. Somebody in another room hearing the answer to a question
/// they did not ask is worse than a reply that is reported failed.
fn open_reply_output(
    dictation: &DictationState,
    owner: Option<&str>,
) -> Result<Arc<dyn speaker::Output>, String> {
    match owner {
        Some(owner) if owner != DESKTOP_OWNER => {
            let link = dictation
                .browser_endpoints
                .get(owner)
                .ok_or_else(|| format!("No client is connected for audio owner '{owner}'"))?;
            Ok(Arc::new(browser::BrowserOutput::new(link)))
        }
        // The system default device. Picking one is the Dictation panel's job
        // (#818-2a29); `config.device` is the *microphone* and using it here
        // would route replies to a capture device.
        _ => Ok(Arc::new(speaker::DeviceOutput::open(None)?)),
    }
}

/// [`open_speaker`] with the language already resolved.
///
/// The split is a lock-order rule, not a convenience: resolving the language
/// takes the hands-free lock, and this is called with the speaker lock held.
/// Taking them in that order here would invert every other path in this file.
fn open_speaker_for(
    dictation: &DictationState,
    generation: u64,
    config: &DictationConfig,
    language: &str,
    owner: Option<&str>,
    conversation_generation: u64,
) -> Result<speaker::Armed, String> {
    let (engine, voice) = build_voice(config, &dictation.speech, language)?;
    let engine = guard_replies(config, dictation, engine, conversation_generation);
    let device = open_reply_output(dictation, owner)?;
    // Wrapped so the canceller learns what is being played. Without this the
    // microphone hears the reply and the VAD opens a turn on the application's
    // own voice.
    //
    // A browser-owned conversation is tapped too: its microphone and its
    // speaker are in the same room as each other, which is the situation the
    // canceller exists for. Only the *devices* moved.
    let tapped = speech_far_end(device, dictation);
    let queue = Arc::new(speaker::Speaker::new(engine, tapped, generation));
    queue.set_loudness(config.loudness());
    // Before the first reply can be queued, which is the whole requirement:
    // `observe` is set-once, and nothing has transitioned yet.
    if let Some(observer) = dictation.utterance_observer.lock().clone() {
        queue.observe(observer);
    }
    Ok(speaker::Armed {
        speaker: queue,
        voice,
        language: language.to_string(),
    })
}

fn guard_replies(
    config: &DictationConfig,
    dictation: &DictationState,
    engine: Arc<dyn speech::Speech>,
    conversation_generation: u64,
) -> Arc<dyn speech::Speech> {
    let mode = dictation.hands_free.clone();
    Arc::new(speech::rejection::GuardedSpeech::new(
        engine,
        dictation.speech.rejections.clone(),
        speech_service(config),
        Some(Arc::new(move |reason| {
            mode.lock()
                .note_speech_outage(conversation_generation, reason);
        })),
    ))
}

/// Barge-in, pointed at the slot rather than at one queue.
///
/// The capture loop is started once, when the conversation is armed, and under
/// Auto the queue it will have to interrupt does not exist until somebody
/// speaks. Holding the slot means the loop interrupts whatever is speaking on
/// the tick the user talks over it, including a voice built minutes later and a
/// voice rebuilt because the language changed.
struct ArmedSpeaker {
    slot: Arc<parking_lot::Mutex<Option<speaker::Armed>>>,
    /// The command waiting for the slot, latest first: only the net state of a
    /// pause, a resume and a hush matters, and the order they are applied in
    /// is the order they were given.
    pending: Arc<parking_lot::Mutex<Option<SpeakerCommand>>>,
}

#[derive(Clone, Copy)]
enum SpeakerCommand {
    Pause,
    Resume,
    Hush,
}

impl ArmedSpeaker {
    fn new(slot: Arc<parking_lot::Mutex<Option<speaker::Armed>>>) -> Self {
        Self {
            slot,
            pending: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Run `command` against whatever is speaking, without ever making the
    /// capture loop wait for the slot.
    ///
    /// The only writer is a rebuild in `speak`, which holds the slot while it
    /// loads a voice and opens a device. A command that meets it is not
    /// dropped — the verdict on a pause arrives after the pause, and a lost
    /// one leaves the reply paused — and it does not block: it is recorded and
    /// a thread applies it when the slot frees.
    fn send(&self, command: SpeakerCommand) {
        *self.pending.lock() = Some(command);
        if let Some(slot) = self.slot.try_lock() {
            apply_pending(&slot, &self.pending);
            return;
        }
        let slot = Arc::clone(&self.slot);
        let pending = Arc::clone(&self.pending);
        std::thread::spawn(move || apply_pending(&slot.lock(), &pending));
    }
}

/// A command still waiting for the slot belongs to the arm that issued it. The
/// thread that will apply it holds only the shared state, not the port, so the
/// port going away is the signal to forget it: otherwise it lands on the voice
/// of the next arm.
impl Drop for ArmedSpeaker {
    fn drop(&mut self) {
        *self.pending.lock() = None;
    }
}

fn apply_pending(
    slot: &Option<speaker::Armed>,
    pending: &parking_lot::Mutex<Option<SpeakerCommand>>,
) {
    let command = pending.lock().take();
    if let (Some(command), Some(armed)) = (command, slot.as_ref()) {
        match command {
            SpeakerCommand::Pause => armed.speaker.pause(),
            SpeakerCommand::Resume => armed.speaker.resume(),
            SpeakerCommand::Hush => {
                armed.speaker.hush();
            }
        }
    }
}

impl continuous::Interruptible for ArmedSpeaker {
    fn pause(&self) {
        self.send(SpeakerCommand::Pause);
    }

    fn resume(&self) {
        self.send(SpeakerCommand::Resume);
    }

    fn hush(&self) {
        self.send(SpeakerCommand::Hush);
    }
}

fn speech_far_end(
    device: Arc<dyn speaker::Output>,
    dictation: &DictationState,
) -> Arc<dyn speaker::Output> {
    Arc::new(echo::FarEndTap::new(device, dictation.echo.clone()))
}

/// What a caller is told about one reply it asked for.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpokenReply {
    /// Identifies this reply for as long as the conversation remembers it.
    pub utterance_id: String,
    /// `queued`, `rendering`, `speaking`, `finished`, `interrupted` or
    /// `failed`. Accepting a reply reports `queued` — never `finished`.
    pub state: String,
    /// Set only for `failed`.
    pub error: Option<String>,
    /// The turn this reply belongs to. A reply for a turn that has ended is
    /// refused rather than spoken.
    pub turn: u64,
}

impl SpokenReply {
    fn new(id: speaker::UtteranceId, state: &speaker::Utterance, turn: u64) -> Self {
        Self {
            utterance_id: id.to_string(),
            state: match state {
                speaker::Utterance::Queued => "queued",
                speaker::Utterance::Rendering => "rendering",
                speaker::Utterance::Speaking => "speaking",
                speaker::Utterance::Finished => "finished",
                speaker::Utterance::Interrupted => "interrupted",
                speaker::Utterance::Failed(_) => "failed",
            }
            .to_string(),
            error: match state {
                speaker::Utterance::Failed(reason) => Some(reason.clone()),
                _ => None,
            },
            turn,
        }
    }
}

/// Whether this installation can speak right now, and into which conversation.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    /// Can a reply be spoken right now? False whenever hands-free is not
    /// armed, and whenever it is armed without a working voice.
    pub available: bool,
    /// Why not. Empty when `available`.
    pub unavailable_reason: String,
    /// The terminal replies are spoken into, absent when nothing is armed.
    pub session_id: Option<String>,
    /// The language this conversation is being held in, as a two-letter code.
    ///
    /// The dictation setting when it names one, and what Whisper made of the
    /// last turn when the setting is Auto. Empty means nobody has spoken yet
    /// under Auto, which is the one state in which no reply can be spoken and
    /// no reply language can be required — never a silent fall back to
    /// English.
    pub language: String,
    /// The turn a reply must belong to. Bumped by every interruption, so a
    /// model answering an older turn can be refused rather than played over
    /// whatever the user said next.
    ///
    /// Not the same counter as `HandsFreeStatus::generation`, which counts
    /// arms rather than interruptions; this one is the speaker's.
    pub turn: u64,
    /// The voice, empty for a user-supplied engine that names its own.
    pub voice: String,
    pub queued: usize,
    pub rendering: bool,
    pub speaking: bool,
    /// The user holds the reply where it is. `speaking` is false meanwhile.
    /// The capture loop's own short hold for a verdict on a voice is not
    /// reported here: the pill must keep offering Pause for it.
    pub paused: bool,
    /// The last synthesis or device failure, cleared by the next reply that
    /// works.
    pub last_error: Option<String>,
    /// The reply the caller asked about, absent when it asked about none.
    ///
    /// An id this conversation no longer remembers comes back with a state of
    /// `unknown` rather than as an absent field: "I have forgotten" and "you
    /// did not ask" are different answers and a caller polling for its own
    /// reply has to be able to tell them apart.
    pub utterance: Option<SpokenReply>,
}

/// Who is asking to speak.
///
/// The distinction is the binding: the owner armed the conversation and may
/// always drive it, while a model may only speak into the conversation it is
/// itself the target of. A model that could speak into another terminal's
/// conversation would be talking to somebody else's user.
pub(crate) enum Caller<'a> {
    /// The user's own UI, on either transport.
    Owner,
    /// A model, named by the live PTY its MCP connection is bound to — the
    /// same key a hands-free binding holds.
    Model(&'a str),
}

impl Caller<'_> {
    /// May this caller drive the conversation bound to `session_id`?
    fn may_drive(&self, session_id: &str) -> Result<(), String> {
        match self {
            Self::Owner => Ok(()),
            Self::Model(caller) if *caller == session_id => Ok(()),
            Self::Model(_) => Err(
                "Speech is bound to another session; only the session hands-free is armed for can speak"
                    .to_string(),
            ),
        }
    }
}

/// The armed conversation's terminal, or why there is none.
fn bound_session(dictation: &DictationState) -> Result<String, String> {
    reap_finished_runtime(dictation);
    dictation
        .hands_free
        .lock()
        .binding()
        .map(|binding| binding.session_id.clone())
        .ok_or_else(|| {
            "Hands-free is not armed; there is no conversation to speak into".to_string()
        })
}

/// Queue a spoken reply.
///
/// Returns as soon as the reply is accepted, carrying the identity the caller
/// polls to find out whether anybody heard it. `turn` refuses a reply written
/// for a turn the user has already talked over; omitting it means "now".
pub(crate) fn speak(
    dictation: &DictationState,
    caller: Caller<'_>,
    text: &str,
    turn: Option<u64>,
) -> Result<SpokenReply, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Nothing to say".to_string());
    }
    if text.chars().count() > MAX_SPOKEN_CHARS {
        return Err(format!(
            "A spoken reply is limited to {MAX_SPOKEN_CHARS} characters; this one is {}",
            text.chars().count()
        ));
    }
    let session_id = bound_session(dictation)?;
    caller.may_drive(&session_id)?;

    // Everything that reads the mode happens before the speaker lock is taken,
    // and the rebuild below is handed the answers. The two locks are always
    // taken in this order.
    let config = get_dictation_config();
    replies_available(&config, dictation)?;
    let language = speech_language(&config, dictation)?;
    let armed_at = dictation.hands_free.lock().generation();
    let owner = conversation_owner(dictation);

    let mut slot = dictation.speaker.lock();
    replies_available(&get_dictation_config(), dictation)?;
    // The queue is per language, so a conversation that changed language needs
    // a new one. `hush` on the way out is what makes the change invalidate the
    // replies written for the old language: it opens a new turn, and `say`
    // refuses anything addressed to the turn before it.
    let rebuild_at = match slot.as_ref() {
        Some(armed) if armed.language == language => None,
        Some(_) => slot.take().map(|previous| previous.speaker.hush()),
        // Nothing yet: under Auto this is the first reply of the conversation,
        // and the language only became knowable when the user spoke.
        None => Some(armed_at),
    };
    if let Some(turn) = rebuild_at {
        *slot = Some(open_speaker_for(
            dictation,
            turn,
            &config,
            &language,
            owner.as_deref(),
            armed_at,
        )?);
    }
    // A voice preview gives way to the conversation instead of playing under
    // its reply. Stopped with the speaker lock held: the preview takes the two
    // locks in the same order, so it cannot start between this and `say`.
    if let Some(preview) = dictation.preview.lock().take() {
        preview.stop();
    }
    let armed = slot.as_ref().expect("a queue was just built or kept");
    // The speaker's own counter, not the caller's guess: it is what `say`
    // compares against, and reading it here makes an interruption landing in
    // between refuse the reply rather than race it.
    let current = armed.speaker.generation();
    let wanted = turn.unwrap_or(current);
    let id = armed
        .speaker
        .say(wanted, text, &armed.voice)
        .map_err(|error| error.to_string())?;
    let state = armed
        .speaker
        .utterance(id)
        .unwrap_or(speaker::Utterance::Queued);
    // Nothing is emitted from here, on purpose (833-6fd4). The push comes from
    // the speaker's own transitions — see `PushUtterance` — because the states
    // worth reporting have no caller to return to: `finished` is decided by the
    // render thread once the device drains, and `interrupted` by whoever talked
    // over the reply. An emit here would report `queued` twice and the rest
    // never.
    Ok(SpokenReply::new(id, &state, wanted))
}

/// Stop talking now and open a new turn.
///
/// Reports the state afterwards rather than a bare success: the caller needs
/// the new turn to know which replies are still worth sending.
pub(crate) fn stop_speaking(
    dictation: &DictationState,
    caller: Caller<'_>,
) -> Result<SpeechStatus, String> {
    let session_id = bound_session(dictation)?;
    caller.may_drive(&session_id)?;
    if let Some(armed) = dictation.speaker.lock().as_ref() {
        armed.speaker.hush();
    }
    Ok(speech_status(dictation, None))
}

/// Hold the reply being spoken where it is, until [`resume_speaking`]. Nothing
/// playing is not an error: the status says it is not paused.
pub(crate) fn pause_speaking(
    dictation: &DictationState,
    caller: Caller<'_>,
) -> Result<SpeechStatus, String> {
    let session_id = bound_session(dictation)?;
    caller.may_drive(&session_id)?;
    if let Some(armed) = dictation.speaker.lock().as_ref() {
        armed.speaker.pause_by_user();
    }
    Ok(speech_status(dictation, None))
}

/// Let a reply the user held go on from where it stopped.
pub(crate) fn resume_speaking(
    dictation: &DictationState,
    caller: Caller<'_>,
) -> Result<SpeechStatus, String> {
    let session_id = bound_session(dictation)?;
    caller.may_drive(&session_id)?;
    if let Some(armed) = dictation.speaker.lock().as_ref() {
        armed.speaker.resume_by_user();
    }
    Ok(speech_status(dictation, None))
}

/// The longest reply that will be accepted, in characters.
///
/// Well past a conversational answer and well short of a model pasting a file.
/// The budget in [`speech`](super::speech::budget_seconds) already stops a
/// runaway *rendering*, but it cannot stop a caller queueing four of these and
/// filling the queue with ten minutes of audio.
const MAX_SPOKEN_CHARS: usize = 2_000;

/// [`speech_status`] for a caller whose right to this conversation must be
/// checked first.
///
/// A model bound to another terminal is told it is not the target rather than
/// shown somebody else's queue — the status fields alone would leak what the
/// other conversation is doing, and a model that can see a queue will try to
/// speak into it.
///
/// Not armed at all is a status rather than an error: the model asked a fair
/// question and the honest answer is "nothing is armed".
pub(crate) fn speech_status_for(
    dictation: &DictationState,
    caller: Caller<'_>,
    utterance: Option<&str>,
) -> Result<SpeechStatus, String> {
    if let Ok(session_id) = bound_session(dictation) {
        caller.may_drive(&session_id)?;
    }
    Ok(speech_status(dictation, utterance))
}

/// Everything a caller needs to decide whether to speak, and what became of a
/// reply it already sent.
pub(crate) fn speech_status(dictation: &DictationState, utterance: Option<&str>) -> SpeechStatus {
    let session_id = bound_session(dictation).ok();
    let config = get_dictation_config();
    // Everything that reads the mode is read here, before the speaker lock: the
    // language of the conversation, the turn a caller would address, and
    // whether a voice could be opened at all. Asking any of them later would
    // take the two locks in the opposite order to `speak`.
    let language = conversation_language(&config, dictation).unwrap_or_default();
    let openable = session_id.as_ref().map(|_| {
        replies_available(&config, dictation)
            .and_then(|()| speech_language(&config, dictation))
            .and_then(|language| open_voice(&config, &dictation.speech, &language).map(|_| ()))
    });
    let armed_at = dictation.hands_free.lock().generation();

    let armed = dictation.speaker.lock();
    let Some(armed) = armed.as_ref() else {
        // No voice open. Under Auto that is every moment before the first
        // turn, and it is not a failure: `available` answers "would a reply be
        // accepted", which is a question about the language and the bundle
        // rather than about whether anything has been said yet.
        return SpeechStatus {
            available: matches!(openable, Some(Ok(()))),
            unavailable_reason: match &openable {
                None => "Hands-free is not armed".to_string(),
                Some(Err(reason)) => reason.clone(),
                Some(Ok(())) => String::new(),
            },
            session_id,
            language,
            turn: armed_at,
            voice: String::new(),
            queued: 0,
            rendering: false,
            speaking: false,
            paused: false,
            last_error: None,
            utterance: None,
        };
    };
    let status = armed.speaker.status();
    let asked_about = utterance.map(|asked| {
        let known = asked
            .parse::<speaker::UtteranceId>()
            .ok()
            .and_then(|id| armed.speaker.utterance(id).map(|state| (id, state)));
        match known {
            Some((id, state)) => SpokenReply::new(id, &state, status.generation),
            None => SpokenReply {
                utterance_id: asked.to_string(),
                state: "unknown".to_string(),
                error: None,
                turn: status.generation,
            },
        }
    });
    let unavailable_reason = replies_available(&config, dictation).err();
    SpeechStatus {
        available: unavailable_reason.is_none(),
        unavailable_reason: unavailable_reason.unwrap_or_default(),
        session_id,
        language,
        turn: status.generation,
        voice: armed.voice.clone(),
        queued: status.queued,
        rendering: status.rendering,
        speaking: status.speaking,
        paused: status.paused,
        last_error: status.last_error,
        utterance: asked_about,
    }
}

/// Speak a reply into the armed conversation.
///
/// The desktop and browser control surface. A model does not call this — it
/// goes through the `voice` MCP tool, which supplies its own identity so the
/// binding can be checked. Here the caller *is* the owner: it is the thing
/// that armed the conversation.
#[tauri::command]
pub fn speak_reply(
    dictation: tauri::State<'_, DictationState>,
    text: String,
    turn: Option<u64>,
) -> Result<SpokenReply, String> {
    speak(&dictation, Caller::Owner, &text, turn)
}

/// Stop talking now, dropping whatever was queued for this turn.
#[tauri::command]
pub fn stop_speech(dictation: tauri::State<'_, DictationState>) -> Result<SpeechStatus, String> {
    stop_speaking(&dictation, Caller::Owner)
}

/// Hold the reply being spoken where it is.
#[tauri::command]
pub fn pause_speech(dictation: tauri::State<'_, DictationState>) -> Result<SpeechStatus, String> {
    pause_speaking(&dictation, Caller::Owner)
}

/// Continue a reply the user held.
#[tauri::command]
pub fn resume_speech(dictation: tauri::State<'_, DictationState>) -> Result<SpeechStatus, String> {
    resume_speaking(&dictation, Caller::Owner)
}

/// Whether anything can be spoken, and what became of a reply already sent.
#[tauri::command]
pub fn get_speech_status(
    dictation: tauri::State<'_, DictationState>,
    utterance: Option<String>,
) -> SpeechStatus {
    speech_status(&dictation, utterance.as_deref())
}

// ---------------------------------------------------------------------------
// Voice preview ("Listen" in Settings → Voice, 855)
// ---------------------------------------------------------------------------

/// The longest preview text, in characters: a sentence or two, enough to hear
/// a voice without turning the settings panel into a reader.
const MAX_PREVIEW_CHARS: usize = 200;

/// Speak `text` in `voice` of `language` on this machine's speaker, the way a
/// reply would sound: the same engine, the same loudness stage, and the same
/// echo-tapped output. Needs no conversation and changes no setting.
///
/// Refused, not queued, while a conversation reply is queued, rendering or
/// playing. Queued behind it, the preview would play whenever the
/// conversation left a gap — seconds or minutes later, long after the user
/// clicked. Refused, it says why at once, and a click after the reply works.
#[tauri::command(async)]
pub fn preview_speech_voice(
    dictation: tauri::State<'_, DictationState>,
    language: String,
    voice: String,
    text: String,
) -> Result<(), String> {
    preview_voice(&dictation, &language, &voice, &text)
}

pub(crate) fn preview_voice(
    dictation: &DictationState,
    language: &str,
    voice: &str,
    text: &str,
) -> Result<(), String> {
    let text = preview_text(text)?;
    let config = preview_config(get_dictation_config(), language, voice)?;
    let (engine, voice) = open_voice(&config, &dictation.speech, language)?;
    let device = open_reply_output(dictation, None)?;
    play_preview(
        dictation,
        engine.as_ref(),
        device,
        &voice,
        text,
        config.loudness(),
    )
}

/// The voice asked for, in a copy of the configuration only: previewing a
/// voice does not select it.
fn preview_config(
    current: DictationConfig,
    language: &str,
    voice: &str,
) -> Result<DictationConfig, String> {
    match speech_engine(&current) {
        // The same resolution a reply makes (`open_voice`): a stored voice that
        // does not speak the language previews as the language default, which
        // is also what the picker shows for it.
        SpeechEngine::Edge => Ok(DictationConfig {
            speech_edge_voice: voice.to_string(),
            ..current
        }),
        SpeechEngine::Pocket => {
            // Named first, so an unknown voice is reported as such rather than
            // as a missing download of the language.
            choose_voice(voice_language(language)?, voice)?;
            Ok(DictationConfig {
                speech_voice: voice.to_string(),
                ..current
            })
        }
        SpeechEngine::External => {
            Err("An external command names its own voices; there is nothing to preview".to_string())
        }
    }
}

fn preview_text(text: &str) -> Result<&str, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Nothing to say".to_string());
    }
    let length = text.chars().count();
    if length > MAX_PREVIEW_CHARS {
        return Err(format!(
            "A voice preview is limited to {MAX_PREVIEW_CHARS} characters; this one is {length}"
        ));
    }
    Ok(text)
}

/// Why a preview may not play now, if it may not.
fn conversation_busy(slot: &Option<speaker::Armed>) -> Option<String> {
    let status = slot.as_ref()?.speaker.status();
    (status.speaking || status.rendering || status.queued > 0).then(|| {
        "A hands-free reply is being spoken; listen to the voice after it finishes".to_string()
    })
}

/// Render, level and play a preview on `device`, through the echo tap.
fn play_preview(
    dictation: &DictationState,
    engine: &dyn speech::Speech,
    device: Arc<dyn speaker::Output>,
    voice: &str,
    text: &str,
    loudness: super::loudness::Loudness,
) -> Result<(), String> {
    // Checked before rendering, so a busy conversation costs no synthesis…
    if let Some(reason) = conversation_busy(&dictation.speaker.lock()) {
        return Err(reason);
    }
    let mut audio = engine
        .synthesize(text, voice, &speech::SpeechCancel::new())
        .map_err(|error| error.to_string())?;
    super::loudness::process(&mut audio, loudness);
    let device = speech_far_end(device, dictation);

    // …and again with the speaker lock held while the device is handed the
    // audio, because a reply can be queued while this renders. `speak` takes
    // the same two locks in the same order and stops a playing preview, so a
    // reply and a preview never overlap.
    let slot = dictation.speaker.lock();
    if let Some(reason) = conversation_busy(&slot) {
        return Err(reason);
    }
    let mut preview = dictation.preview.lock();
    if let Some(previous) = preview.take() {
        previous.stop();
    }
    device.play(&audio)?;
    *preview = Some(device);
    Ok(())
}

/// Start push-to-talk recording.
///
/// `command(async)` rather than a plain `command`: a sync Tauri command runs on
/// the main thread, and the first press of the hotkey loads the whisper model
/// there — a multi-second GGML + GPU init that freezes the whole UI. `async`
/// makes Tauri run this body on its async runtime instead, which is separate
/// from the runtime the HTTP server owns (see `lib.rs`), so nothing else stalls.
/// The function itself stays sync, so the HTTP route calls it unchanged.
///
// DEFERRED (2026-08-17) — the load still occupies one Tauri runtime worker for
// its duration. Moving it to `spawn_blocking` needs an async fn, which means
// changing this signature and the caller in `mcp_http/dictation_routes.rs`.
/// Make sure the microphone is usable, or say why it is not.
///
/// Shared by push-to-talk and hands-free: one spelling of the TCC dance, so the
/// two modes cannot disagree about what "denied" means.
fn ensure_microphone_access() -> Result<(), String> {
    match permission::check() {
        permission::MicPermission::Denied => Err("microphone_denied".to_string()),
        permission::MicPermission::Restricted => Err("microphone_restricted".to_string()),
        permission::MicPermission::NotDetermined => {
            // CoreAudio (cpal) does NOT trigger the TCC prompt — we must
            // explicitly request access via AVCaptureDevice to show the dialog.
            if permission::request() {
                Ok(())
            } else {
                Err("microphone_denied".to_string())
            }
        }
        permission::MicPermission::Authorized => Ok(()),
    }
}

/// The loaded recogniser, loading it first if the model changed or none is up.
///
/// Both modes share the one `transcriber_arc`: loading a second copy of a
/// multi-gigabyte model because the other mode got there first would be a
/// straightforward way to run the machine out of memory.
///
/// `app` is `None` off the desktop event loop, where there is no handle to log
/// through; the load is the same either way.
fn ensure_transcriber(
    app: Option<&AppHandle>,
    dictation: &DictationState,
    whisper_model: model::WhisperModel,
) -> Result<Arc<dyn transcribe::Transcriber>, String> {
    let mut transcriber_arc_lock = dictation.transcriber_arc.lock();
    let mut active_model_lock = dictation.active_model.lock();
    let model_changed = active_model_lock
        .as_deref()
        .map(|name| name != whisper_model.name())
        .unwrap_or(true);

    if model_changed || transcriber_arc_lock.is_none() {
        if !model::model_exists(whisper_model) {
            return Err("Model not downloaded".to_string());
        }
        let loading = format!("Loading model: {}", whisper_model.display_name());
        match app {
            Some(app) => app_logger::log_via_handle(app, "info", "dictation", &loading),
            None => tracing::info!(source = "dictation", "{loading}"),
        }
        let t = transcribe::WhisperTranscriber::load(&model::model_path(whisper_model))?;
        dictation.install_transcriber_locked(
            &mut transcriber_arc_lock,
            &mut active_model_lock,
            Arc::new(t),
            whisper_model.name(),
        );
        let loaded = format!("Model loaded (backend: {})", transcribe::backend_label());
        match app {
            Some(app) => app_logger::log_via_handle(app, "info", "dictation", &loaded),
            None => tracing::info!(source = "dictation", "{loaded}"),
        }
    }

    transcriber_arc_lock
        .clone()
        .ok_or_else(|| "Transcriber not available".to_string())
}

#[tauri::command(async)]
pub fn start_dictation(
    app: AppHandle,
    dictation: State<'_, DictationState>,
    source: Option<String>,
) -> Result<(), String> {
    dictation.ensure_owner()?;
    let from_fn = source.as_deref() == Some("fn");
    if from_fn && !dictation.fn_down.load(Ordering::Acquire) {
        return Err("Fn was released before recording started".to_string());
    }
    // Atomic test-and-set: prevents TOCTOU race from concurrent IPC calls
    if dictation
        .recording
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Already recording".to_string());
    }
    // Guard resets recording=false if we return early on any error path
    let mut recording_guard = RecordingGuard::new(&dictation);
    dictation.fn_capture.store(from_fn, Ordering::Release);
    if from_fn && !dictation.fn_down.load(Ordering::Acquire) {
        dictation
            .native_release_pending
            .store(true, Ordering::Release);
    }

    if dictation.processing.load(Ordering::Acquire) {
        return Err("Transcription in progress".to_string());
    }

    ensure_microphone_access()?;

    // One read of dictation-config.json for the whole start: the model, the
    // input device and the language all come from this snapshot.
    let config = get_dictation_config();
    let whisper_model = resolve_model(&config.model);
    let transcriber_arc = ensure_transcriber(Some(&app), &dictation, whisper_model)?;

    // Always emit backend info so the frontend gets it even when model is reused
    let _ = app.emit(
        "dictation-backend-info",
        serde_json::json!({
            "backend": transcribe::backend_label(),
        }),
    );

    // Start audio capture using the configured device (or system default)
    let device_name = config.device.as_deref().filter(|s| !s.is_empty());
    let mut capture = audio::AudioCapture::start_with_device(device_name).map_err(|e| {
        app_logger::log_via_handle(
            &app,
            "error",
            "dictation",
            &format!("Audio capture failed: {e}"),
        );
        // If a specific device failed, hint the user
        if device_name.is_some() {
            app_logger::log_via_handle(
                &app,
                "warn",
                "dictation",
                "Configured device not available — check Settings > Voice > Input device",
            );
        }
        e
    })?;

    // Get audio buffer handle for streaming thread
    let audio_buffer = capture.buffer_handle();
    let mut audio_slot = dictation.audio.lock();
    if dictation.native_release_pending.load(Ordering::Acquire)
        || (from_fn && !dictation.fn_down.load(Ordering::Acquire))
    {
        capture.stop_stream();
        tracing::info!(
            source = "dictation",
            "Capture stopped after release during microphone start"
        );
    }
    *audio_slot = Some(capture);
    drop(audio_slot);

    // Start streaming session
    let lang = if config.language == "auto" {
        None
    } else {
        Some(config.language.clone())
    };
    let (tx, rx) = mpsc::channel::<String>();

    let session = streaming::StreamingSession::start(
        transcriber_arc as Arc<dyn transcribe::Transcriber>,
        audio_buffer,
        tx,
        lang,
        config.gates(),
    );
    *dictation.streaming.lock() = Some(session);

    // recording is already true (set by compare_exchange above)
    tracing::info!(
        source = "dictation",
        origin = source.as_deref().unwrap_or("ui"),
        unix_ms = unix_ms(),
        "Streaming recording started"
    );

    // Reset accumulated partials for this session
    dictation.accumulated_partials.lock().clear();

    // Forward partial text to the live preview. The microphone meter is read
    // through get_dictation_status so desktop IPC and HTTP clients use the same
    // request/response surface.
    let app_clone = app.clone();
    let accumulated = dictation.inner().accumulated_partials.clone();
    std::thread::Builder::new()
        .name("dictation-event-forwarder".into())
        .spawn(move || {
            for text in rx {
                {
                    let mut acc = accumulated.lock();
                    if !acc.is_empty() {
                        acc.push(' ');
                    }
                    acc.push_str(&text);
                }
                if let Err(e) = app_clone.emit("dictation-partial", &text) {
                    tracing::warn!(source = "dictation", "Failed to emit partial event: {e}");
                }
            }
        })
        .map_err(|e| format!("Failed to spawn event forwarder: {e}"))?;

    // Success: keep recording=true (disarm the guard so it doesn't reset on drop)
    recording_guard.disarm();
    Ok(())
}

#[tauri::command]
pub async fn stop_dictation_and_transcribe(app: AppHandle) -> Result<TranscribeResponse, String> {
    // Gather all data from DictationState synchronously (before any .await).
    // This block ensures no MutexGuard or State borrow lives across the await point.
    let prepare = {
        let dictation = app.state::<DictationState>();

        if !dictation.recording.load(Ordering::Acquire) {
            return Err("Not recording".to_string());
        }

        let request = dictation.stop_request.lock().take();
        let (requested_at, trigger) = request
            .map(|request| (request.at, request.source))
            .unwrap_or_else(|| (std::time::Instant::now(), "ipc"));
        tracing::info!(
            source = "dictation",
            trigger,
            unix_ms = unix_ms(),
            "Stop requested"
        );

        // Set recording=false synchronously so the UI updates immediately
        dictation.recording.store(false, Ordering::Release);
        dictation.processing.store(true, Ordering::Release);

        // Stop audio capture (stops the cpal stream, but buffer data remains)
        let mut capture_lock = dictation.audio.lock();
        if let Some(ref mut capture) = *capture_lock {
            capture.stop_stream();
        }
        tracing::info!(
            source = "dictation",
            trigger,
            latency_ms = requested_at.elapsed().as_millis(),
            unix_ms = unix_ms(),
            "Stop executed"
        );
        let capture_dropped = capture_lock
            .as_ref()
            .map(audio::AudioCapture::dropped_samples)
            .unwrap_or(0);
        dictation
            .native_release_pending
            .store(false, Ordering::Release);
        dictation.fn_capture.store(false, Ordering::Release);

        // Take the streaming session (cheap — no join yet) and the audio buffer handle.
        // The actual thread join happens in spawn_blocking to avoid blocking the tokio worker.
        let session = dictation.streaming.lock().take();
        let audio_buffer = capture_lock.as_ref().map(|c| c.buffer_handle());
        drop(capture_lock);

        // Read config while we still have sync context (avoids file I/O after .await)
        let config = get_dictation_config();
        let lang_owned = if config.language == "auto" {
            None
        } else {
            Some(config.language.clone())
        };

        // Clone Arc-ed resources for the blocking task
        let transcriber = dictation.transcriber_arc.lock().clone();
        let accumulated_partials = dictation.accumulated_partials.clone();
        let corrections = dictation.corrections.clone();
        let processing = dictation.processing.clone();

        Some((
            session,
            audio_buffer,
            capture_dropped,
            lang_owned,
            config.gates(),
            transcriber,
            accumulated_partials,
            corrections,
            processing,
        ))
    };

    let (
        session,
        audio_buffer,
        capture_dropped,
        lang_owned,
        gates,
        transcriber,
        accumulated_partials,
        corrections,
        processing,
    ) = prepare.unwrap(); // always Some — the None path returns Err above

    let app_clone = app.clone();

    // Run session join + whisper inference off the IPC thread
    let result = tokio::task::spawn_blocking(move || {
        let _guard = ProcessingGuard(processing);

        // Join the streaming thread (may block while last partial window finishes)
        let streamed = session.map(|s| s.stop()).unwrap_or_default();
        let mut dropped_samples = streamed.dropped_samples + capture_dropped;
        let mut all_audio = streamed.audio;

        // Drain anything left in the audio capture buffer (arrived after last poll).
        // Safe: streaming thread is joined above, no more concurrent readers.
        if let Some(buf) = audio_buffer {
            let remaining: Vec<f32> = buf.lock().drain(..).collect();
            all_audio.extend(remaining);
        }
        // That tail never passed the streaming thread's cap, and a slow final
        // window makes it arbitrarily long. Cap the assembled recording once.
        dropped_samples += streaming::cap_finished_recording(&mut all_audio);

        let truncated_s = dropped_samples as f64 / 16000.0;
        let total_duration_s = all_audio.len() as f64 / 16000.0;
        let trace_empty_final = || {
            tracing::info!(
                source = "dictation",
                full_chars = 0,
                composed_chars = accumulated_partials.lock().chars().count(),
                audio_s = total_duration_s,
                dropped_s = truncated_s,
                "Final transcription length"
            );
        };

        // A panicked streaming thread took the recording with it. Whatever
        // reached the capture buffer afterwards is not the recording, and
        // transcribing it would report a fragment as the whole answer.
        if streamed.interrupted {
            trace_empty_final();
            app_logger::log_via_handle(
                &app_clone,
                "warn",
                "dictation",
                "Streaming thread was interrupted — the recording is not recoverable",
            );
            return TranscribeResponse {
                text: String::new(),
                // Rendered by `useDictation` as "Dictation: <reason>".
                skip_reason: Some("recording was interrupted".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }
        tracing::info!(source = "dictation", audio_s = total_duration_s, dropped_s = truncated_s, "Streaming stopped for final transcription");

        // Short audio: no transcription needed
        if all_audio.len() < 8000 {
            trace_empty_final();
            app_logger::log_via_handle(&app_clone, "info", "dictation", "No speech detected");
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("no speech detected".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }

        let mut final_text = String::new();
        let mut final_skip_reason = None;

        if let Some(ref transcriber) = transcriber {
            let lang_ref = lang_owned.as_deref();
            match transcribe_final_ptt_audio(transcriber.as_ref(), &all_audio, lang_ref, gates) {
                Ok(result) if result.skip_reason.is_none() => {
                    final_text = result.text;
                }
                Ok(result) => {
                    if let Some(reason) = &result.skip_reason {
                        app_logger::log_via_handle(
                            &app_clone,
                            "info",
                            "dictation",
                            &format!("Final transcription skipped: {reason}"),
                        );
                    }
                    final_skip_reason = result.skip_reason;
                }
                Err(e) => {
                    app_logger::log_via_handle(
                        &app_clone,
                        "warn",
                        "dictation",
                        &format!("Final transcription failed: {e}"),
                    );
                }
            }
        } else {
            trace_empty_final();
            app_logger::log_via_handle(
                &app_clone,
                "warn",
                "dictation",
                "Transcriber not available — model not loaded",
            );
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("model not loaded".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }

        let no_speech_fallback = final_skip_reason.is_none();
        if let Some(response) = empty_final_response(
            &final_text,
            final_skip_reason,
            total_duration_s,
            truncated_s,
        ) {
            trace_empty_final();
            if no_speech_fallback {
                app_logger::log_via_handle(&app_clone, "info", "dictation", "No speech detected");
            }
            return response;
        }

        // Log accuracy comparison (lengths only — no verbatim text to avoid PII in logs)
        let composed = std::mem::take(&mut *accumulated_partials.lock());
        let full_chars = final_text.chars().count();
        let composed_chars = composed.chars().count();
        let ratio = transcription_ratio(&final_text, &composed);
        tracing::info!(source = "dictation", full_chars, composed_chars, ratio = ?ratio, audio_s = total_duration_s, dropped_s = truncated_s, "Final transcription length");
        // The final pass is normally the LONGER of the two — streaming skips
        // VAD-silent windows. Coming back shorter means it lost text the
        // streaming windows already had, which is the shape of window tail loss.
        if ratio.is_some_and(|r| r < SHORT_TRANSCRIPTION_RATIO) {
            app_logger::log_via_handle(
                &app_clone,
                "warn",
                "dictation",
                &format!(
                    "Final transcription is shorter than the streaming partials: full={full_chars} chars against composed={composed_chars} chars (below {:.0}%)",
                    SHORT_TRANSCRIPTION_RATIO * 100.0
                ),
            );
        }

        // Apply corrections
        let corrected = corrections.lock().correct(&final_text);
        let final_text = corrected.replace('\n', " ");

        // _guard drops here → processing = false
        TranscribeResponse {
            text: final_text,
            skip_reason: None,
            duration_s: total_duration_s,
            truncated_s,
        }
    })
    .await
    .map_err(|e| {
        let msg = format!("Transcription task panicked: {e}");
        app_logger::log_via_handle(&app, "error", "dictation", &msg);
        msg
    })?;

    // Clean up audio capture
    *app.state::<DictationState>().audio.lock() = None;

    Ok(result)
}

#[tauri::command]
pub fn get_correction_map(dictation: State<'_, DictationState>) -> HashMap<String, String> {
    dictation.corrections.lock().get_replacements().clone()
}

#[tauri::command]
pub fn set_correction_map(
    dictation: State<'_, DictationState>,
    map: HashMap<String, String>,
) -> Result<(), String> {
    let mut corrections = dictation.corrections.lock();
    corrections.set_replacements(map);
    corrections.save_to_file(&corrections::TextCorrector::default_path())
}

/// Shared by the IPC command and `GET /dictation/devices`.
#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<audio::AudioDevice>, String> {
    crate::audio_enumeration::run_bounded(
        "listing audio input devices",
        crate::audio_enumeration::ENUMERATION_TIMEOUT,
        audio::list_input_devices,
    )
    .await
}

/// Shell integration: inject text into active terminal.
/// Currently only callable from within the app via Tauri IPC.
///
/// Future external trigger mechanisms:
/// 1. CLI: `tuicommander inject "text"` via IPC socket
/// 2. Pipe: `echo "text" | tuicommander --inject`
/// 3. Tauri deep link: `tuicommander://inject?text=...`
///
/// Security: Will require authentication token stored in env var.
#[tauri::command]
pub fn inject_text(dictation: State<'_, DictationState>, text: String) -> Result<String, String> {
    // Apply corrections before injection
    let corrected = dictation.corrections.lock().correct(&text);
    let final_text = corrected.replace('\n', " ");
    Ok(final_text)
}

// ---------------------------------------------------------------------------
// Hands-free mode
// ---------------------------------------------------------------------------

/// Longest accepted session id or audio owner.
///
/// Both come from the caller, and over HTTP that caller is a remote client. The
/// owner in particular is retained for as long as the mode stays armed and
/// echoed back in every status reply, so an unbounded one is a buffer the
/// client controls the size of.
pub(crate) const MAX_BINDING_LEN: usize = 256;

/// Hands-free state, as both transports report it.
///
/// One struct, serialized by the Tauri command and by the HTTP route, so the
/// field names and casing cannot drift between them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandsFreeStatus {
    pub armed: bool,
    /// See `continuous::Phase::as_wire`.
    pub phase: String,
    /// The bound delivery target. Unchanged by focus for as long as it is set.
    pub session_id: Option<String>,
    /// The bound audio endpoint.
    pub owner: Option<String>,
    pub generation: u64,
    /// The transcript waiting out its hold-back, or held by a dialog or a
    /// draft in the composer, so the UI can show what is about to be sent
    /// while there is still time to stop it.
    pub pending_text: Option<String>,
    pub hold_back_ms: u64,
    pub error: Option<String>,
    /// Monotonic turn counts; a client plays an earcon when one moves. See
    /// `continuous::HandsFree::delivered_turns`.
    pub delivered_turns: u64,
    pub dropped_turns: u64,
}

/// What a disarm did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandsFreeDisarmed {
    /// False when there was nothing to disarm. The other fields are then empty
    /// rather than implying work that did not happen.
    pub was_armed: bool,
    pub generation: u64,
    /// A transcript that never reached the model — inside its hold-back, or
    /// held by the composer — was dropped. Everything already typed stays.
    pub discarded_pending: bool,
    pub discarded_capture: bool,
    pub status: HandsFreeStatus,
}

pub(crate) fn hands_free_status(dictation: &DictationState) -> HandsFreeStatus {
    // A poll is also where a runtime that ended by itself gets cleaned up; see
    // `reap_finished_runtime` for why the thread cannot do it.
    reap_finished_runtime(dictation);
    let mode = dictation.hands_free.lock();
    HandsFreeStatus {
        armed: mode.binding().is_some(),
        phase: mode.phase().as_wire().to_string(),
        session_id: mode.binding().map(|binding| binding.session_id.clone()),
        owner: mode.binding().map(|binding| binding.owner.clone()),
        generation: mode.generation(),
        pending_text: mode.pending_text().map(str::to_string),
        hold_back_ms: mode.hold_back_ms(),
        error: mode.last_error().map(str::to_string),
        delivered_turns: mode.delivered_turns(),
        dropped_turns: mode.dropped_turns(),
    }
}

/// Take the settings the mode reads into it, in one config load. Both are
/// no-ops while armed — see `HandsFree::set_hold_back_ms`.
pub(crate) fn apply_config_to_mode(dictation: &DictationState) {
    let config = get_dictation_config();
    let mut mode = dictation.hands_free.lock();
    mode.set_hold_back_ms(config.hands_free_hold_back_ms.into());
    mode.set_activation(
        &config.hands_free_activation_phrase,
        continuous::ACTIVATION_WINDOW_MS,
    );
}

fn check_binding_field(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} is empty"));
    }
    if value.len() > MAX_BINDING_LEN {
        return Err(format!("{label} is too long"));
    }
    Ok(())
}

/// The owner name that means "this machine".
///
/// Any other owner is a browser or remote client, and is served by the socket
/// it connected on — never by the desktop microphone. Arming from a laptop must
/// not open the microphone on the machine running TUICommander, so an owner
/// with no socket behind it is refused rather than fallen back.
pub(crate) const DESKTOP_OWNER: &str = "desktop";

/// The desktop microphone plus the loaded whisper model.
///
/// Holds only the capture *buffer*, never the `cpal::Stream`: the stream is
/// `!Send` and stays in `DictationState`, which is also what keeps push-to-talk
/// and hands-free on separate devices.
struct DesktopVoiceEndpoint {
    buffer: Arc<parking_lot::Mutex<std::collections::VecDeque<f32>>>,
    alive: Arc<AtomicBool>,
    transcriber: Arc<dyn transcribe::Transcriber>,
    language: Option<String>,
    gates: transcribe::VoiceGates,
}

impl continuous::VoiceEndpoint for DesktopVoiceEndpoint {
    fn drain(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.buffer.lock().drain(..).collect())
    }

    fn connected(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn transcribe(&self, audio: &[f32]) -> Result<continuous::Transcript, String> {
        transcribe_utterance(
            self.transcriber.as_ref(),
            audio,
            self.language.as_deref(),
            self.gates,
        )
    }
}

/// Recognise one closed utterance, the same way for every endpoint.
///
/// Shared rather than duplicated per endpoint because the gate handling below
/// is a decision, not plumbing: where the audio came from changes nothing about
/// what a gated segment means.
pub(crate) fn transcribe_utterance(
    transcriber: &dyn transcribe::Transcriber,
    audio: &[f32],
    language: Option<&str>,
    gates: transcribe::VoiceGates,
) -> Result<continuous::Transcript, String> {
    let result = transcriber.transcribe(audio, language, gates)?;
    // A gated segment is not an error and not a message: whisper decided
    // this was not speech, so the utterance is dropped the same way an
    // empty transcript is — and with it goes the language, which would
    // otherwise be whatever whisper made of room noise.
    Ok(if result.skip_reason.is_some() {
        continuous::Transcript::default()
    } else {
        continuous::Transcript {
            text: result.text,
            language: result.language,
        }
    })
}

/// Open the capture endpoint the named owner is entitled to.
///
/// The owner decides which device is opened, and there is deliberately no
/// fallback between the two: an unknown owner is refused, not served by
/// whatever this machine happens to have.
fn open_endpoint(
    dictation: &DictationState,
    owner: &str,
) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
    let config = get_dictation_config();
    let language = (config.language != "auto").then(|| config.language.clone());
    let gates = config.gates();
    if owner != DESKTOP_OWNER {
        let link = dictation
            .browser_endpoints
            .get(owner)
            .ok_or_else(|| format!("No client is connected for audio owner '{owner}'"))?;
        // The model is loaded here for the same reason as below: recognition
        // runs on this machine whichever microphone fed it. Only the audio is
        // remote.
        let transcriber = ensure_transcriber(None, dictation, resolve_model(&config.model))?;
        // The canceller aligns a reply's start against capture not yet
        // drained; this stream's backlog is what it has to count.
        let backlog = Arc::clone(&link);
        dictation
            .echo
            .lock()
            .attach_capture(Box::new(move || backlog.pending_capture()));
        return Ok(Box::new(browser::BrowserVoiceEndpoint::new(
            link,
            transcriber,
            language,
            gates,
        )));
    }
    ensure_microphone_access()?;
    let transcriber = ensure_transcriber(None, dictation, resolve_model(&config.model))?;
    let device_name = config.device.as_deref().filter(|name| !name.is_empty());
    let capture = audio::AudioCapture::start_with_device(device_name)?;
    let buffer = capture.buffer_handle();
    // See the browser branch above: the canceller counts this backlog.
    let backlog = Arc::clone(&buffer);
    dictation
        .echo
        .lock()
        .attach_capture(Box::new(move || backlog.lock().len()));
    *dictation.hands_free_audio.lock() = Some(capture);
    dictation
        .hands_free_owner_alive
        .store(true, Ordering::Release);
    Ok(Box::new(DesktopVoiceEndpoint {
        buffer,
        alive: dictation.hands_free_owner_alive.clone(),
        transcriber,
        language,
        gates,
    }))
}

/// Release the desktop audio endpoint.
///
/// The runtime sees its owner gone on the next tick and disarms itself with
/// `OwnerDisconnected` — the mode is never left armed against a microphone that
/// is no longer the one it bound to.
pub(crate) fn release_desktop_endpoint(dictation: &DictationState) {
    dictation
        .hands_free_owner_alive
        .store(false, Ordering::Release);
}

/// Drop a runtime whose thread has already returned, and the microphone with it.
///
/// An automatic disarm (closed target, dead device) ends the thread from the
/// inside, and the thread cannot release the capture device itself — a
/// `cpal::Stream` is `!Send`, so it never crossed the thread boundary. Reaping
/// here means the microphone closes on the next status poll rather than staying
/// open until somebody arms again.
fn reap_finished_runtime(dictation: &DictationState) {
    let finished = dictation
        .hands_free_runtime
        .lock()
        .as_ref()
        .is_some_and(continuous::HandsFreeRuntime::is_finished);
    if finished {
        *dictation.hands_free_runtime.lock() = None;
        *dictation.hands_free_audio.lock() = None;
        release_desktop_endpoint(dictation);
    }
}

/// Bind hands-free capture to a session and an audio owner.
///
/// This binds the target and the audio owner, opens the endpoint that owner
/// names, and starts the runtime that types speech from it into the bound
/// agent's composer. Arming is also what 817's speech capability and 821's entry hint key
/// off, which is why it is reachable before any UI exists.
///
/// Refused when the target cannot take hands-free input. There is no
/// fallback delivery path, so an unsupported target stays unavailable — and so
/// does an audio endpoint this build does not implement.
pub(crate) fn arm_hands_free(
    state: &Arc<crate::state::AppState>,
    dictation: &DictationState,
    session_id: &str,
    owner: &str,
) -> Result<HandsFreeStatus, String> {
    arm_hands_free_with(state, dictation, session_id, owner, &open_endpoint)
}

/// `arm_hands_free` with the capture endpoint supplied.
///
/// The seam exists so a test can drive the whole armed path — bind, capture,
/// segment, transcribe, hold back, write — without a microphone or a
/// multi-gigabyte model, against a real session and the real sink.
type OpenVoiceEndpoint<'a> =
    dyn Fn(&DictationState, &str) -> Result<Box<dyn continuous::VoiceEndpoint>, String> + 'a;

pub(crate) fn arm_hands_free_with(
    state: &Arc<crate::state::AppState>,
    dictation: &DictationState,
    session_id: &str,
    owner: &str,
    open_endpoint: &OpenVoiceEndpoint<'_>,
) -> Result<HandsFreeStatus, String> {
    dictation.ensure_owner()?;
    check_binding_field(session_id, "Session id")?;
    check_binding_field(owner, "Audio owner")?;
    if !crate::pty::session_accepts_voice(state, session_id) {
        return Err("Session cannot accept hands-free input".to_string());
    }
    reap_finished_runtime(dictation);
    if dictation.hands_free.lock().binding().is_some() {
        return Err("Hands-free is already armed".to_string());
    }
    // The microphone opens before the bind, so a refused or broken endpoint
    // leaves the mode untouched rather than armed-and-deaf with a generation
    // already spent.
    let endpoint = open_endpoint(dictation, owner)?;

    apply_config_to_mode(dictation);
    let armed = dictation
        .hands_free
        .lock()
        .arm(session_id, owner, true)
        .map_err(|error| match error {
            continuous::ArmError::AlreadyArmed => "Hands-free is already armed".to_string(),
            continuous::ArmError::UnsupportedTarget => {
                "Session cannot accept hands-free input".to_string()
            }
        });
    if let Err(error) = armed {
        *dictation.hands_free_audio.lock() = None;
        release_desktop_endpoint(dictation);
        return Err(error);
    }

    // Tell the model the conversation opened, if the user asked us to. Before
    // the runtime starts, so no spoken turn can go first: a model that reads
    // "you can answer out loud" after the question it applies to has been told
    // nothing useful. A dialog or a draft holds it in the mode, and the
    // runtime retries it before any turn.
    //
    // A refusal is recorded on the mode rather than failing the arm. The
    // microphone works, ordinary turns may still land, and a conversation the
    // model was not told about is a worse conversation rather than no
    // conversation — the reason is visible in the hands-free status.
    let config = get_dictation_config();
    if config.hands_free_notify_model
        && let Some(Err(error)) = continuous::deliver_entry_hint(
            &mut dictation.hands_free.lock(),
            &super::adapters::PtyVoiceSink(state.as_ref()),
            &continuous::entry_hint_for_replies(
                &config.hands_free_start_notice,
                config.hands_free_spoken_replies,
            ),
            Some(&config.language),
        )
    {
        tracing::warn!(
            source = "dictation",
            "Hands-free start notice refused: {error}"
        );
        dictation.hands_free.lock().note_send_failed(&error);
    }

    // The reply queue, built with the turn the mode just opened so both halves
    // agree about which turn is current from the first reply onwards.
    //
    // Before the runtime, not after: the capture loop takes the queue as its
    // barge-in port, and a loop started first would spend its first ticks
    // unable to interrupt anything.
    //
    // A failure here does not fail the arm. Hands-free without a voice is
    // dictation, which is useful on its own and is what a user who has not
    // downloaded a language bundle gets; the reason is logged once here and
    // reported by the voice capability rather than being discovered per reply.
    let generation = dictation.hands_free.lock().generation();
    match open_speaker(dictation, generation) {
        Ok(armed) => *dictation.speaker.lock() = Some(armed),
        Err(reason) => {
            // Not a failure to arm, and under Auto not even a failure: no
            // language is known until the user speaks, so the first reply
            // opens the voice instead.
            tracing::info!("dictation: armed without a voice yet: {reason}");
            *dictation.speaker.lock() = None;
        }
    }
    // The slot, not the queue in it. A port bound to the queue built above
    // would be bound to nothing whenever that build failed — which is every
    // Auto conversation — and barge-in would stay dead for the whole session
    // even once a later reply opened a voice.
    let interruptible: Option<Arc<dyn continuous::Interruptible>> =
        Some(Arc::new(ArmedSpeaker::new(Arc::clone(&dictation.speaker))));

    // Frame activity follows the Settings > Voice floor (1164-ee4b: the compiled
    // 0.01 sat 10x above push-to-talk's 0.001, and ordinary speech at a normal
    // distance measures 0.1-1% RMS, so only a close mouth opened an utterance).
    // Pre-roll, trailing silence, minimum speech and the utterance cap are not
    // reachable from `DictationConfig`; no measurement has shown a default that
    // needs moving, and a knob nobody asked for must be documented, persisted
    // and migrated. Wire them when Step 8 (#818-2a29) gives Settings a place.
    *dictation.hands_free_runtime.lock() = Some(continuous::spawn_runtime(
        Arc::new(super::adapters::PtyVoicePort(state.clone())),
        dictation.hands_free.clone(),
        endpoint,
        segmenter_config(config.gates()),
        dictation.echo.clone(),
        interruptible,
    ));

    Ok(hands_free_status(dictation))
}

/// Disarm the whole mode and drop what never reached the model.
///
/// Idempotent: disarming a mode that was never armed reports `was_armed: false`
/// rather than inventing an outcome. Nothing typed can be taken back, and
/// nothing is parked anywhere else, so there is nothing to cancel.
pub(crate) fn disarm_hands_free(
    state: &crate::state::AppState,
    dictation: &DictationState,
) -> HandsFreeDisarmed {
    use super::adapters::PtyVoiceSink;
    use crate::dictation::continuous::DisarmReason;

    let disarmed = dictation.hands_free.lock().disarm(DisarmReason::Manual);
    // Stop talking first, and unconditionally. Dropping the queue cancels the
    // reply in flight and stops the device, which is what makes disarm revoke
    // speech whether or not the model ever acknowledged anything — a late
    // `speak` then finds no speaker and is told so rather than being played to
    // a user who has left.
    *dictation.speaker.lock() = None;
    // Stop the runtime and close the microphone whichever way this went: a
    // thread that already disarmed itself still has a device to release.
    *dictation.hands_free_runtime.lock() = None;
    *dictation.hands_free_audio.lock() = None;
    release_desktop_endpoint(dictation);
    let Some(disarmed) = disarmed else {
        // Both fields below read the mode, and `hands_free` is not reentrant: a
        // `lock()` temporary inside the struct literal lives until the end of
        // the whole statement, so taking it there deadlocks against the one
        // `hands_free_status` takes. Read it once, first.
        let status = hands_free_status(dictation);
        return HandsFreeDisarmed {
            was_armed: false,
            generation: status.generation,
            discarded_pending: false,
            discarded_capture: false,
            status,
        };
    };
    // Driven by what this arm actually typed, never by the setting as it reads
    // now. A user who turns the notice off mid-conversation has changed what
    // the *next* arm says; the model that already read "you can answer out
    // loud" still has to be told that stopped being true.
    continuous::report_exit_hint(&PtyVoiceSink(state), &disarmed);
    HandsFreeDisarmed {
        was_armed: true,
        generation: disarmed.generation,
        discarded_pending: disarmed.discarded_pending,
        discarded_capture: disarmed.discarded_capture,
        status: hands_free_status(dictation),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn arm_hands_free_dictation(
    state: State<'_, Arc<crate::state::AppState>>,
    dictation: State<'_, DictationState>,
    session_id: String,
    owner: String,
) -> Result<HandsFreeStatus, String> {
    arm_hands_free(&state, &dictation, &session_id, &owner)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn disarm_hands_free_dictation(
    state: State<'_, Arc<crate::state::AppState>>,
    dictation: State<'_, DictationState>,
) -> HandsFreeDisarmed {
    disarm_hands_free(&state, &dictation)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn get_hands_free_status(dictation: State<'_, DictationState>) -> HandsFreeStatus {
    hands_free_status(&dictation)
}

#[cfg(test)]
#[test]
fn partial_dictation_config_keeps_valid_fields() {
    let loaded = dictation_config_from_value(serde_json::json!({
        "hotkey": "F8",
        "language": "it",
        "speech_volume_db": "loud"
    }));

    assert_eq!(loaded.hotkey, "F8");
    assert_eq!(loaded.language, "it");
    assert_eq!(
        loaded.speech_volume_db.to_bits(),
        DictationConfig::default().speech_volume_db.to_bits()
    );
    assert!(loaded.recovered_from_corruption);
}

fn recovered_field<T: DeserializeOwned>(
    object: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    default: T,
    recovered: &mut bool,
) -> T {
    match object.get(name) {
        None => default,
        Some(value) => serde_json::from_value(value.clone()).unwrap_or_else(|_| {
            *recovered = true;
            default
        }),
    }
}

fn dictation_config_from_value(value: serde_json::Value) -> DictationConfig {
    let serde_json::Value::Object(object) = value else {
        tracing::warn!(
            source = "dictation",
            "Dictation config is not a JSON object; using defaults"
        );
        return DictationConfig {
            recovered_from_corruption: true,
            ..Default::default()
        };
    };
    let defaults = DictationConfig::default();
    let mut recovered = false;
    let config = DictationConfig {
        enabled: recovered_field(&object, "enabled", defaults.enabled, &mut recovered),
        hotkey: recovered_field(&object, "hotkey", defaults.hotkey, &mut recovered),
        language: recovered_field(&object, "language", defaults.language, &mut recovered),
        model: recovered_field(&object, "model", defaults.model, &mut recovered),
        device: recovered_field(&object, "device", defaults.device, &mut recovered),
        long_press_ms: recovered_field(
            &object,
            "long_press_ms",
            defaults.long_press_ms,
            &mut recovered,
        ),
        auto_send: recovered_field(&object, "auto_send", defaults.auto_send, &mut recovered),
        rms_threshold: recovered_field(
            &object,
            "rms_threshold",
            defaults.rms_threshold,
            &mut recovered,
        ),
        no_speech_threshold: recovered_field(
            &object,
            "no_speech_threshold",
            defaults.no_speech_threshold,
            &mut recovered,
        ),
        hands_free_hold_back_ms: recovered_field(
            &object,
            "hands_free_hold_back_ms",
            defaults.hands_free_hold_back_ms,
            &mut recovered,
        ),
        hands_free_activation_phrase: recovered_field(
            &object,
            "hands_free_activation_phrase",
            defaults.hands_free_activation_phrase,
            &mut recovered,
        ),
        hands_free_notify_model: recovered_field(
            &object,
            "hands_free_notify_model",
            defaults.hands_free_notify_model,
            &mut recovered,
        ),
        hands_free_start_notice: recovered_field(
            &object,
            "hands_free_start_notice",
            defaults.hands_free_start_notice,
            &mut recovered,
        ),
        hands_free_spoken_replies: recovered_field(
            &object,
            "hands_free_spoken_replies",
            defaults.hands_free_spoken_replies,
            &mut recovered,
        ),
        hands_free_earcons: recovered_field(
            &object,
            "hands_free_earcons",
            defaults.hands_free_earcons,
            &mut recovered,
        ),
        speech_command: recovered_field(
            &object,
            "speech_command",
            defaults.speech_command,
            &mut recovered,
        ),
        speech_voice: recovered_field(
            &object,
            "speech_voice",
            defaults.speech_voice,
            &mut recovered,
        ),
        speech_engine: recovered_field(
            &object,
            "speech_engine",
            defaults.speech_engine,
            &mut recovered,
        ),
        speech_edge_voice: recovered_field(
            &object,
            "speech_edge_voice",
            defaults.speech_edge_voice,
            &mut recovered,
        ),
        speech_volume_db: recovered_field(
            &object,
            "speech_volume_db",
            defaults.speech_volume_db,
            &mut recovered,
        ),
        speech_levelling: recovered_field(
            &object,
            "speech_levelling",
            defaults.speech_levelling,
            &mut recovered,
        ),
        model_idle_unload_minutes: recovered_field(
            &object,
            "model_idle_unload_minutes",
            defaults.model_idle_unload_minutes,
            &mut recovered,
        ),
        recovered_from_corruption: recovered,
    };
    if recovered {
        tracing::warn!(
            source = "dictation",
            "Recovered valid dictation settings from malformed fields"
        );
    }
    config
}

const DICTATION_CONFIG_FILE: &str = "dictation-config.json";

/// The built-in hands-free start notice, sent while
/// [`DictationConfig::hands_free_start_notice`] is empty.
#[tauri::command]
pub fn get_hands_free_default_notice() -> String {
    continuous::MODE_ENTRY_HINT.to_string()
}

#[tauri::command]
pub fn get_dictation_config() -> DictationConfig {
    with_resolved_engine(read_dictation_config())
}

fn read_dictation_config() -> DictationConfig {
    let path = crate::config::config_dir().join(DICTATION_CONFIG_FILE);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DictationConfig::default();
        }
        Err(error) => {
            tracing::warn!(source = "dictation", path = %path.display(), "Could not read dictation config: {error}");
            return DictationConfig {
                recovered_from_corruption: true,
                ..Default::default()
            };
        }
    };
    match serde_json::from_str(content.as_str()) {
        Ok(value) => dictation_config_from_value(value),
        Err(error) => {
            tracing::warn!(source = "dictation", path = %path.display(), "Could not parse dictation config: {error}");
            DictationConfig {
                recovered_from_corruption: true,
                ..Default::default()
            }
        }
    }
}

#[tauri::command]
pub fn set_dictation_config(
    base: DictationConfig,
    config: DictationConfig,
    dictation: State<'_, DictationState>,
) -> Result<(), String> {
    save_dictation_config(base, config, Some(&dictation))
}

/// [`set_dictation_config`] for a caller that may not have the dictation state.
///
/// `None` only affects the live conversation: the file is written either way,
/// and the next reply re-reads it. A transport that can reach `DictationState`
/// passes it so a language change takes effect on the voice that is speaking
/// right now, rather than on the one after it.
pub(crate) fn save_dictation_config(
    base: DictationConfig,
    mut config: DictationConfig,
    dictation: Option<&DictationState>,
) -> Result<(), String> {
    // DEFERRED (2026-09-21) — switching the input device while hands-free is
    // armed should release the endpoint the mode bound to (that is an owner
    // disconnect, see `release_desktop_endpoint`). The state is reachable here
    // now, but the endpoint swap needs the runtime to be restarted around it,
    // which is a change to `arm_hands_free_with` rather than to this function.
    // Until then the mode keeps capturing from the device it armed with.
    let previous = get_dictation_config();
    config.recovered_from_corruption = false;
    let file = crate::config::ConfigFile::<DictationConfig>::new(DICTATION_CONFIG_FILE);
    if base.recovered_from_corruption {
        // A concurrent writer may already have repaired the file. Recheck
        // under the file lock before deciding whether to repair or merge.
        file.save_delta_recovering(&base, &config)?;
    } else {
        file.save_delta_strict(&base, &config)?;
    }
    let config = get_dictation_config();
    // The configured model is part of the cached status snapshot.
    invalidate_model_snapshot();
    // A voice belongs to a language and to an engine. Change either and every
    // reply already queued for the old one is wrong — a sentence half spoken
    // in Italian does not become English by finishing it. Dropping the queue
    // stops the device and cancels what is in flight; the next reply opens a
    // voice for the language now configured.
    //
    // Only on those three fields. Every other setting here is a threshold or a
    // hotkey, and cutting a reply off mid-word because somebody moved a slider
    // would be a worse bug than the one this prevents.
    let voice_changed = previous.language != config.language
        || previous.speech_engine != config.speech_engine
        || previous.speech_command != config.speech_command
        || previous.speech_voice != config.speech_voice
        || previous.speech_edge_voice != config.speech_edge_voice;
    if let Some(dictation) = dictation {
        let mut slot = dictation.speaker.lock();
        if voice_changed || !config.hands_free_spoken_replies {
            *slot = None;
        } else if let Some(armed) = slot.as_ref() {
            // A level, not a voice: the queue keeps speaking and the next
            // reply it renders takes the new level.
            armed.speaker.set_loudness(config.loudness());
        }
    }
    Ok(())
}

/// Check microphone permission status (macOS TCC).
/// Returns: "authorized", "denied", "restricted", or "not_determined".
#[tauri::command]
pub fn check_microphone_permission() -> String {
    permission::check().as_str().to_string()
}

/// Open macOS System Settings > Privacy > Microphone.
#[tauri::command]
pub fn open_microphone_settings() {
    permission::open_settings();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::continuous::Phase;

    struct PhraseTranscriber;

    impl transcribe::Transcriber for PhraseTranscriber {
        fn transcribe(
            &self,
            _audio: &[f32],
            _language: Option<&str>,
            _gates: transcribe::VoiceGates,
        ) -> Result<transcribe::TranscribeResult, String> {
            Ok(transcribe::TranscribeResult {
                text: "run the tests".to_string(),
                skip_reason: None,
                language: Some("en".to_string()),
            })
        }
    }

    #[test]
    fn push_to_talk_rejects_a_short_noise_burst_before_transcription() {
        let mut audio = vec![0.03; 16_000 * 180 / 1_000];
        audio.resize(16_000, 0.0);

        let result = transcribe_final_ptt_audio(
            &PhraseTranscriber,
            &audio,
            Some("en"),
            transcribe::VoiceGates::default(),
        )
        .unwrap();

        assert!(
            result.text.is_empty(),
            "a noise burst must not reach the prompt"
        );
        assert_eq!(
            result.skip_reason.as_deref(),
            Some("no sustained speech (need 200ms of active audio)")
        );
    }

    #[test]
    fn push_to_talk_rejects_audio_below_the_transcriber_rms_floor() {
        let audio = vec![0.0005; 16_000];
        let result = transcribe_final_ptt_audio(
            &PhraseTranscriber,
            &audio,
            Some("en"),
            transcribe::VoiceGates::default(),
        )
        .unwrap();
        assert!(result.text.is_empty());
        assert_eq!(
            result.skip_reason.as_deref(),
            Some("no sustained speech (need 200ms of active audio)")
        );
    }

    #[test]
    fn push_to_talk_keeps_sustained_speech_at_normal_level() {
        let mut audio = vec![0.03; 16_000 * 300 / 1_000];
        audio.resize(16_000, 0.0);
        let result = transcribe_final_ptt_audio(
            &PhraseTranscriber,
            &audio,
            Some("en"),
            transcribe::VoiceGates::default(),
        )
        .unwrap();
        assert_eq!(result.text, "run the tests");
        assert!(result.skip_reason.is_none());
    }

    #[test]
    fn push_to_talk_keeps_400ms_speech_with_unvoiced_frames() {
        for level in [0.003, 0.015] {
            let mut audio = Vec::new();
            for frame in 0..20 {
                let sample = if frame % 4 == 3 { 0.0 } else { level };
                audio.extend(vec![sample; 320]);
            }

            let result = transcribe_final_ptt_audio(
                &PhraseTranscriber,
                &audio,
                Some("en"),
                transcribe::VoiceGates::default(),
            )
            .unwrap();
            assert_eq!(result.text, "run the tests", "400ms speech at RMS {level}");
            assert!(result.skip_reason.is_none());
        }
    }

    #[test]
    fn push_to_talk_activity_uses_the_configured_transcription_floor() {
        let audio = vec![0.003; 16_000 * 400 / 1_000];
        let gates = transcribe::VoiceGates {
            rms_threshold: 0.004,
            ..transcribe::VoiceGates::default()
        };
        let result =
            transcribe_final_ptt_audio(&PhraseTranscriber, &audio, Some("en"), gates).unwrap();
        assert!(result.text.is_empty());
        assert_eq!(
            result.skip_reason.as_deref(),
            Some("no sustained speech (need 200ms of active audio)")
        );
    }

    #[test]
    fn push_to_talk_does_not_join_separate_noise_bursts_into_speech() {
        let mut audio = vec![0.03; 16_000 * 100 / 1_000];
        audio.extend(vec![0.0; 16_000 * 1_600 / 1_000]);
        audio.extend(vec![0.03; 16_000 * 100 / 1_000]);

        let result = transcribe_final_ptt_audio(
            &PhraseTranscriber,
            &audio,
            Some("en"),
            transcribe::VoiceGates::default(),
        )
        .unwrap();
        assert!(result.text.is_empty());
        assert_eq!(
            result.skip_reason.as_deref(),
            Some("no sustained speech (need 200ms of active audio)")
        );
    }

    #[test]
    fn push_to_talk_accepts_speech_ending_at_key_release() {
        let audio = vec![0.03; 16_000 * 200 / 1_000];
        let result = transcribe_final_ptt_audio(
            &PhraseTranscriber,
            &audio,
            Some("en"),
            transcribe::VoiceGates::default(),
        )
        .unwrap();
        assert_eq!(result.text, "run the tests");
        assert!(result.skip_reason.is_none());
    }

    #[test]
    fn stopped_dictation_preserves_final_transcriber_skip_reason() {
        let response = empty_final_response(
            "",
            Some("audio too quiet (RMS 0.0005 < 0.0010)".to_string()),
            1.25,
            0.0,
        )
        .expect("empty final transcription returns a response");

        assert_eq!(response.text, "");
        assert_eq!(
            serde_json::to_value(&response).unwrap()["skip_reason"],
            "audio too quiet (RMS 0.0005 < 0.0010)"
        );
        assert_eq!(response.duration_s.to_bits(), 1.25_f64.to_bits());
    }

    #[test]
    fn stopped_dictation_uses_no_speech_for_empty_success() {
        let response = empty_final_response("", None, 0.75, 0.0)
            .expect("empty final transcription returns a response");
        assert_eq!(response.skip_reason.as_deref(), Some("no speech detected"));
        assert!(empty_final_response("hello", None, 0.75, 0.0).is_none());
    }

    /// A microphone and a recogniser the test writes the script for.
    ///
    /// After its one phrase it keeps handing back room silence, exactly as a
    /// live capture device does — a device that stops delivering samples is a
    /// failure, and this fake must not fake one.
    #[cfg(unix)]
    struct ScriptedEndpoint {
        phrase: parking_lot::Mutex<Option<Vec<f32>>>,
        transcript: String,
    }

    #[cfg(unix)]
    impl continuous::VoiceEndpoint for ScriptedEndpoint {
        fn drain(&mut self) -> Result<Vec<f32>, String> {
            Ok(self
                .phrase
                .lock()
                .take()
                .unwrap_or_else(|| vec![0.0; continuous::SAMPLE_RATE as usize / 20]))
        }

        fn connected(&self) -> bool {
            true
        }

        fn transcribe(&self, _audio: &[f32]) -> Result<continuous::Transcript, String> {
            Ok(continuous::Transcript {
                text: self.transcript.clone(),
                language: Some("it".to_string()),
            })
        }
    }

    /// A microphone in a quiet room. It keeps delivering samples, as a live
    /// device does, and nobody ever says anything into it.
    ///
    /// The notices are about the *mode*, so a test for them must not have to
    /// stage a spoken turn to see one.
    #[cfg(unix)]
    fn silent_endpoint()
    -> impl Fn(&DictationState, &str) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
        |_dictation, _owner| {
            Ok(Box::new(ScriptedEndpoint {
                phrase: parking_lot::Mutex::new(None),
                transcript: String::new(),
            }))
        }
    }

    /// Wait for `needle` to be typed into the recorded terminal.
    ///
    /// The composer flushes on its own schedule, so the alternative is a fixed
    /// sleep — a guess about scheduling rather than a deadline.
    #[cfg(unix)]
    fn wait_for_typed(bytes: &Arc<std::sync::Mutex<Vec<u8>>>, needle: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let typed = String::from_utf8_lossy(&bytes.lock().expect("recorder")).to_string();
            if typed.contains(needle) {
                return typed;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let typed = String::from_utf8_lossy(&bytes.lock().expect("recorder")).to_string();
        panic!("{needle:?} was never typed into the terminal; it holds {typed:?}");
    }

    /// Whether `needle` is typed into the recorded terminal inside `window`.
    /// Used for the assertion that it is not; the caller measures the window
    /// rather than guessing it.
    #[cfg(unix)]
    fn typed_within(
        bytes: &Arc<std::sync::Mutex<Vec<u8>>>,
        needle: &str,
        window: std::time::Duration,
    ) -> bool {
        let deadline = std::time::Instant::now() + window;
        while std::time::Instant::now() < deadline {
            if String::from_utf8_lossy(&bytes.lock().expect("recorder")).contains(needle) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    /// A spoken phrase: 600ms of tone, then long enough a pause to close it.
    #[cfg(unix)]
    fn spoken_phrase() -> Vec<f32> {
        let sample_rate = continuous::SAMPLE_RATE as f32;
        let speech = (0..(sample_rate as usize * 600 / 1000))
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin() * 0.5);
        speech
            .chain(std::iter::repeat_n(0.0, sample_rate as usize))
            .collect()
    }

    #[cfg(unix)]
    fn scripted_endpoint(
        transcript: &'static str,
    ) -> impl Fn(&DictationState, &str) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
        move |_dictation, _owner| {
            Ok(Box::new(ScriptedEndpoint {
                phrase: parking_lot::Mutex::new(Some(spoken_phrase())),
                transcript: transcript.to_string(),
            }))
        }
    }

    /// Both halves of a binding are attacker-shaped input on the HTTP transport:
    /// a remote client names the session and the owner. Neither may be empty,
    /// and neither may be unbounded — the owner string is retained for as long
    /// as the mode is armed and echoed back in every status reply.
    #[test]
    fn arming_rejects_unbounded_or_empty_identifiers() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let dictation = DictationState::new();

        assert_eq!(
            arm_hands_free(&state, &dictation, "", "desktop").unwrap_err(),
            "Session id is empty".to_string()
        );
        assert_eq!(
            arm_hands_free(&state, &dictation, "session", "").unwrap_err(),
            "Audio owner is empty".to_string()
        );
        assert_eq!(
            arm_hands_free(
                &state,
                &dictation,
                &"s".repeat(MAX_BINDING_LEN + 1),
                "desktop"
            )
            .unwrap_err(),
            "Session id is too long".to_string()
        );
        assert_eq!(
            arm_hands_free(
                &state,
                &dictation,
                "session",
                &"o".repeat(MAX_BINDING_LEN + 1)
            )
            .unwrap_err(),
            "Audio owner is too long".to_string()
        );
        assert!(
            !hands_free_status(&dictation).armed,
            "a refused arm must leave the mode disarmed"
        );
    }

    /// The unsupported-target rule, at the surface a caller actually reaches.
    #[test]
    fn arming_against_a_target_that_cannot_take_a_compose_entry_is_refused() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let dictation = DictationState::new();

        assert_eq!(
            arm_hands_free(&state, &dictation, "no-such-session", "desktop").unwrap_err(),
            "Session cannot accept hands-free input".to_string()
        );
        let status = hands_free_status(&dictation);
        assert!(!status.armed);
        assert_eq!(status.phase, "disarmed");
        assert_eq!(status.session_id, None);
    }

    /// Disarm is idempotent and says plainly that there was nothing to disarm,
    /// rather than reporting a cancellation it did not perform.
    #[test]
    fn disarming_a_mode_that_was_never_armed_reports_no_work() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let dictation = DictationState::new();

        let outcome = disarm_hands_free(&state, &dictation);

        assert!(!outcome.was_armed);
        assert!(!outcome.discarded_pending);
        assert_eq!(outcome.status.phase, "disarmed");
    }

    /// The status reply is the one shape both transports serialize, so this
    /// pins the fields a store reads on IPC *and* over HTTP.
    #[test]
    fn a_disarmed_status_names_every_field_a_client_reads() {
        let dictation = DictationState::new();

        let status = hands_free_status(&dictation);
        let wire = serde_json::to_value(&status).expect("serialize");

        assert_eq!(wire["armed"], serde_json::json!(false));
        assert_eq!(wire["phase"], serde_json::json!("disarmed"));
        assert_eq!(wire["sessionId"], serde_json::Value::Null);
        assert_eq!(wire["owner"], serde_json::Value::Null);
        assert_eq!(wire["generation"], serde_json::json!(0));
        assert_eq!(wire["pendingText"], serde_json::Value::Null);
        assert!(
            wire.get("queuedIds").is_none(),
            "nothing is parked in the Compose queue on hands-free's behalf any more"
        );
        assert_eq!(
            wire["holdBackMs"],
            serde_json::json!(default_hold_back_ms())
        );
        assert_eq!(wire["deliveredTurns"], serde_json::json!(0));
        assert_eq!(wire["droppedTurns"], serde_json::json!(0));
    }

    /// A config written before the earcons setting existed must load with
    /// them on, and an explicit off must survive the whole-document rewrite.
    #[test]
    fn the_earcons_setting_defaults_on_and_survives_a_rewrite() {
        let older: DictationConfig =
            serde_json::from_str(r#"{"enabled":true,"hotkey":"F5","language":"auto"}"#)
                .expect("an older config loads");
        assert!(older.hands_free_earcons);

        let off = DictationConfig {
            hands_free_earcons: false,
            ..Default::default()
        };
        let wire = serde_json::to_string(&off).expect("serialize");
        let back: DictationConfig = serde_json::from_str(&wire).expect("deserialize");
        assert!(!back.hands_free_earcons);
    }

    /// Auto-send is on for a fresh install and for a config written before the
    /// field existed, and an explicit off must survive the whole-document rewrite.
    #[test]
    fn auto_send_defaults_on_and_an_explicit_off_survives_a_rewrite() {
        assert!(DictationConfig::default().auto_send);
        let older: DictationConfig =
            serde_json::from_str(r#"{"enabled":true,"hotkey":"F5","language":"auto"}"#)
                .expect("an older config loads");
        assert!(older.auto_send);

        let off = DictationConfig {
            auto_send: false,
            ..Default::default()
        };
        let wire = serde_json::to_string(&off).expect("serialize");
        let back: DictationConfig = serde_json::from_str(&wire).expect("deserialize");
        assert!(!back.auto_send);
    }

    /// The hold-back is a setting, not a constant, and arming is what reads it.
    #[test]
    fn arming_takes_the_hold_back_from_the_configuration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                hands_free_hold_back_ms: 4_000,
                ..Default::default()
            },
            None,
        )
        .expect("config save");
        let dictation = DictationState::new();

        apply_config_to_mode(&dictation);

        assert_eq!(hands_free_status(&dictation).hold_back_ms, 4_000);
    }

    /// A config written before hands-free existed must keep a usable hold-back,
    /// not deserialize to zero and send every utterance the instant it lands.
    #[test]
    fn a_config_written_before_hands_free_existed_keeps_a_hold_back() {
        let stored = serde_json::json!({
            "enabled": true,
            "hotkey": "F5",
            "language": "auto",
        });
        let config: DictationConfig = serde_json::from_value(stored).expect("deserialize");

        assert_eq!(config.hands_free_hold_back_ms, default_hold_back_ms());
        assert!(config.hands_free_hold_back_ms > 0);
        assert!(
            config.hands_free_activation_phrase.is_empty(),
            "an upgrade may not start gating speech the user never configured"
        );
    }

    /// The activation phrase is a setting, and arming is what reads it. The
    /// whole gate is dead code if this wiring is missing, which is exactly the
    /// shape of a matcher with no caller.
    #[cfg(unix)]
    #[test]
    fn a_configured_activation_phrase_decides_which_speech_reaches_the_model() {
        // The start notice is story 821's and has its own tests. Off here, so
        // the first thing typed is the spoken turn.
        let _config = config_of_this_test(DictationConfig {
            hands_free_activation_phrase: "ciao tuic".to_string(),
            hands_free_hold_back_ms: 100,
            hands_free_notify_model: false,
            ..Default::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-gate", crate::pty::SHELL_BUSY);
        let typed = crate::test_support::insert_recording_session(&state, "voice-gate");

        // The addressed half first, and timed: the gate decides in the same
        // tick as the transcription, so how long an accepted turn takes to
        // appear on this machine bounds how long a rejected one could.
        let dictation = DictationState::new();
        arm_hands_free_with(
            &state,
            &dictation,
            "voice-gate",
            "desktop",
            &scripted_endpoint("Ciao Tuic, run the tests"),
        )
        .expect("arm");
        let started = std::time::Instant::now();
        // The whole submission, Enter included: the text and the Enter are two
        // writes with a real gap between them.
        let terminal = wait_for_typed(&typed, "run the tests (reply in Italian)\r");
        let accepted_in = started.elapsed();
        assert_eq!(
            terminal, "\u{15}run the tests (reply in Italian)\r",
            "the phrase addresses the tool and may not reach the model, and the language the \
             user spoke it in must"
        );
        disarm_hands_free(&state, &dictation);

        // The same pipeline, same session, unrelated speech: recognised
        // locally, then dropped.
        let dictation = DictationState::new();
        arm_hands_free_with(
            &state,
            &dictation,
            "voice-gate",
            "desktop",
            &scripted_endpoint("cancella tutto il repository"),
        )
        .expect("arm");
        let window = (accepted_in * 5).max(std::time::Duration::from_secs(2));
        assert!(
            !typed_within(&typed, "cancella", window),
            "speech without the activation phrase must not reach the model"
        );
        disarm_hands_free(&state, &dictation);
    }

    /// Bug caught: a second desktop instance on the same config directory arms
    /// hands-free and opens the microphone the owner is using. `start_dictation`
    /// runs the same `ensure_owner` as its first statement.
    #[test]
    fn a_non_owner_instance_refuses_to_arm_and_never_opens_the_microphone() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _held_by_the_other_instance =
            crate::dictation::ownership::Ownership::acquire(dir.path());
        let dictation = DictationState::new();
        dictation.claim_ownership(dir.path());
        assert!(!dictation.is_owner());
        let opened = std::sync::atomic::AtomicBool::new(false);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = arm_hands_free_with(&state, &dictation, "s", "desktop", &|_, _| {
            opened.store(true, Ordering::SeqCst);
            Err("endpoint opened".to_string())
        });
        assert_eq!(
            result.unwrap_err(),
            crate::dictation::ownership::OWNED_ELSEWHERE
        );
        assert!(!opened.load(Ordering::SeqCst), "the microphone was opened");
    }

    /// Bug caught: the status of a non-owner reports a plain "ready", so the UI
    /// never says another instance owns dictation.
    #[test]
    fn status_reports_owned_elsewhere_for_a_non_owner() {
        let (dir, _config) = config_of_this_test(DictationConfig::default());
        let _held_by_the_other_instance =
            crate::dictation::ownership::Ownership::acquire(dir.path());
        let dictation = DictationState::new();
        assert!(!dictation_status(&dictation).owned_elsewhere);
        dictation.claim_ownership(dir.path());
        assert!(dictation_status(&dictation).owned_elsewhere);
    }

    /// The whole feature, once, against a real session: arm binds, the status
    /// reports the binding, a held-back transcript is typed into the agent's
    /// composer, and the Compose queue is left exactly as it was.
    ///
    /// The target is deliberately BUSY with work parked in its Compose queue:
    /// Boss's rule is that speech reaches a working agent at once, like a line
    /// typed by hand, and never waits in — or reorders — that queue.
    #[cfg(unix)]
    #[test]
    fn arming_types_speech_into_a_busy_session_and_leaves_its_compose_queue_alone() {
        // Its own configuration, with the start notice off: this test reads
        // what the terminal holds, and the notice would be part of it. Story
        // 821 proves the notice itself.
        let _config = config_of_this_test(DictationConfig {
            hands_free_notify_model: false,
            ..Default::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-e2e", crate::pty::SHELL_BUSY);
        let typed = crate::test_support::insert_recording_session(&state, "voice-e2e");
        let dictation = DictationState::new();
        // Work a human and a peer already parked on the same session.
        let (human, notice) = {
            let mut queue = state
                .pending_injections
                .entry("voice-e2e".to_string())
                .or_default();
            let human = crate::state::PendingInjection::user_command("typed by hand");
            let notice = crate::state::PendingInjection::notice("peer mail wake");
            let ids = (human.id(), notice.id());
            queue.push_back(human);
            queue.push_back(notice);
            ids
        };

        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-e2e",
            "desktop",
            &scripted_endpoint("run the tests"),
        )
        .expect("arm");
        assert!(armed.armed);
        assert_eq!(armed.session_id.as_deref(), Some("voice-e2e"));
        assert_eq!(armed.owner.as_deref(), Some("desktop"));
        assert_eq!(armed.phase, "waiting");

        // No further pokes: the runtime thread started by `arm` captures the
        // phrase, segments it, transcribes it, waits out the hold-back and
        // types it on its own — into an agent that is still working.
        let terminal = wait_for_typed(&typed, "run the tests (reply in Italian)\r");
        assert_eq!(terminal, "\u{15}run the tests (reply in Italian)\r");

        let status = hands_free_status(&dictation);
        assert_eq!(status.session_id.as_deref(), Some("voice-e2e"));

        let disarmed = disarm_hands_free(&state, &dictation);

        assert!(disarmed.was_armed);
        let remaining: Vec<u64> = state
            .pending_injections
            .get("voice-e2e")
            .expect("queue")
            .iter()
            .map(crate::state::PendingInjection::id)
            .collect();
        assert_eq!(
            remaining,
            [human, notice],
            "the human's command and the peer notice stay parked, in order"
        );
        assert!(!disarmed.status.armed);
        assert!(
            !disarm_hands_free(&state, &dictation).was_armed,
            "disarming twice must report the second call honestly"
        );
        assert!(
            dictation.hands_free_runtime.lock().is_none(),
            "a disarm stops the capture runtime"
        );
        assert!(
            dictation.audio.lock().is_none(),
            "hands-free must never take push-to-talk's capture slot"
        );
    }

    /// Arming from a browser must not open the microphone on the machine
    /// running TUICommander (832-e730 criterion 2). An owner with no socket
    /// behind it is refused, not quietly served by the desktop mic — the
    /// failure to fear here is a fallback, because it is silent and the user
    /// who armed from a laptop would never learn the room being listened to is
    /// not theirs.
    #[cfg(unix)]
    #[test]
    fn an_owner_without_a_connected_client_is_refused_rather_than_given_the_desktop_microphone() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-remote", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-remote");
        let dictation = DictationState::new();

        let refused = arm_hands_free(&state, &dictation, "voice-remote", "browser-42")
            .expect_err("a remote owner with no socket has no endpoint");

        assert_eq!(
            refused,
            "No client is connected for audio owner 'browser-42'"
        );
        assert!(!hands_free_status(&dictation).armed);
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "a refused owner must not have opened a capture device"
        );
    }

    /// The capture side of the same rule, one step lower: with a client
    /// connected the browser owner resolves to *that* client's audio, and the
    /// desktop capture slot is still untouched.
    ///
    /// Asserting the slot rather than the endpoint's type, because the slot is
    /// what holds the physical microphone open — a browser endpoint that also
    /// opened the local device would satisfy any check on what was returned.
    #[test]
    fn a_connected_browser_owner_is_served_by_its_own_socket() {
        let dictation = DictationState::new();
        let link = dictation.browser_endpoints.connect("browser-42");
        link.push_capture(&[0.25, 0.5]);

        // Recognition needs a downloaded model, which an unattended run does
        // not have, so the endpoint cannot be built here. What can be checked
        // is the half that decides *whose* audio it would carry.
        assert!(
            dictation
                .browser_endpoints
                .get("browser-42")
                .is_some_and(|found| found.drain_capture() == vec![0.25, 0.5]),
            "the owner resolves to the socket that registered it"
        );
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "resolving a browser owner must not open the local microphone"
        );
    }

    /// The other direction of criterion 2: a desktop conversation must not be
    /// handed a browser's microphone just because one happens to be connected.
    #[cfg(unix)]
    #[test]
    fn a_desktop_conversation_ignores_a_connected_browser_client() {
        let dictation = DictationState::new();
        let link = dictation.browser_endpoints.connect("browser-42");
        link.push_capture(&[0.9; 32]);

        // `desktop` never looks at the registry: the request either opens this
        // machine's device or fails on it, and either way the browser's audio
        // is still sitting there afterwards.
        let _ = open_endpoint(&dictation, DESKTOP_OWNER);

        assert_eq!(
            link.drain_capture().len(),
            32,
            "the browser's audio was not consumed by a desktop arm"
        );
    }

    /// Criterion 4: replies for a browser-owned conversation leave the machine.
    /// A missing client is a failure rather than a fallback, for the same
    /// reason as the microphone — somebody in another room must not hear the
    /// answer to a question they did not ask.
    #[test]
    fn replies_follow_the_owner_and_never_fall_back_to_the_server_speakers() {
        let dictation = DictationState::new();
        let link = dictation.browser_endpoints.connect("browser-42");
        let mut client = link.subscribe();

        let output = open_reply_output(&dictation, Some("browser-42"))
            .expect("a connected client can be spoken to");
        output
            .play(&speech::SpeechAudio {
                samples: vec![0.0; 2_400],
                sample_rate: 24_000,
            })
            .expect("the client takes it");
        assert!(
            matches!(
                client.try_recv().expect("the client was told"),
                browser::Downlink::Speak(_)
            ),
            "the reply went to the browser"
        );

        let Err(refused) = open_reply_output(&dictation, Some("browser-99")) else {
            panic!("an owner with no client has no speaker");
        };
        assert_eq!(
            refused,
            "No client is connected for audio owner 'browser-99'"
        );
    }

    /// The bound tab closes while the mode is armed: the runtime notices on its
    /// own, disarms, and releases the capture device it was holding.
    #[cfg(unix)]
    #[test]
    fn closing_the_bound_session_disarms_the_running_mode_and_releases_the_device() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-closing", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-closing");
        let dictation = DictationState::new();

        arm_hands_free_with(
            &state,
            &dictation,
            "voice-closing",
            "desktop",
            &scripted_endpoint("never sent"),
        )
        .expect("arm");

        // What closing a tab does to the session map.
        state.session_maps.sessions.remove("voice-closing");

        // Wait for the *reap*, not for `armed`: the mode is unbound the moment
        // the runtime disarms, which is a scheduling tick before its thread
        // actually returns and its device can be released.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if !hands_free_status(&dictation).armed && dictation.hands_free_runtime.lock().is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let status = hands_free_status(&dictation);
        assert!(!status.armed, "a closed target must disarm the mode");
        assert!(
            dictation.hands_free_runtime.lock().is_none(),
            "the finished runtime must be reaped"
        );
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "and the capture device released with it"
        );
    }

    /// Phase is the one string both transports report; a rename would silently
    /// break whatever renders it.
    #[test]
    fn every_phase_has_a_stable_wire_name() {
        assert_eq!(Phase::Disarmed.as_wire(), "disarmed");
        assert_eq!(Phase::Waiting.as_wire(), "waiting");
        assert_eq!(Phase::Capturing.as_wire(), "capturing");
        assert_eq!(Phase::Transcribing.as_wire(), "transcribing");
        assert_eq!(Phase::HoldingBack.as_wire(), "holding_back");
        assert_eq!(Phase::Delivered.as_wire(), "delivered");
        assert_eq!(Phase::Error.as_wire(), "error");
    }

    /// Persist a config that names `model`, then clear the snapshot cache so the
    /// next read observes it.
    fn write_model_config(model: &str) {
        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                model: model.to_string(),
                ..Default::default()
            },
            None,
        )
        .expect("config save");
    }

    #[test]
    #[serial_test::serial]
    fn stale_dictation_saves_preserve_distinct_fields() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let base = get_dictation_config();
        let mut first = base.clone();
        first.model = "small".to_string();
        let mut second = base.clone();
        second.speech_volume_db = -24.0;

        save_dictation_config(base.clone(), first, None).unwrap();
        save_dictation_config(base, second, None).unwrap();

        let saved = get_dictation_config();
        assert_eq!(saved.model, "small");
        assert_eq!(saved.speech_volume_db.to_bits(), (-24.0_f32).to_bits());
    }

    #[test]
    #[serial_test::serial]
    fn dictation_save_keeps_valid_fields_salvaged_from_malformed_config() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        std::fs::write(
            dir.path().join(DICTATION_CONFIG_FILE),
            r#"{"enabled":false,"hotkey":"F8","language":"it","speech_volume_db":"loud"}"#,
        )
        .unwrap();
        let base = get_dictation_config();
        assert!(base.recovered_from_corruption);
        let mut desired = base.clone();
        desired.model = "small".to_string();

        save_dictation_config(base, desired, None).unwrap();

        let saved = get_dictation_config();
        assert!(!saved.recovered_from_corruption);
        assert_eq!(saved.hotkey, "F8");
        assert_eq!(saved.language, "it");
        assert_eq!(saved.model, "small");
    }

    #[test]
    #[serial_test::serial]
    fn recovered_dictation_save_preserves_valid_concurrent_write() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let path = dir.path().join(DICTATION_CONFIG_FILE);
        std::fs::write(&path, r#"{"language":"it","speech_volume_db":"loud"}"#).unwrap();
        let base = get_dictation_config();
        assert!(base.recovered_from_corruption);
        let mut desired = base.clone();
        desired.model = "small".to_string();

        let concurrent = DictationConfig {
            language: "fr".to_string(),
            ..DictationConfig::default()
        };
        std::fs::write(&path, serde_json::to_string(&concurrent).unwrap()).unwrap();
        save_dictation_config(base, desired, None).unwrap();

        let saved = get_dictation_config();
        assert_eq!(saved.model, "small");
        assert_eq!(
            saved.language, "fr",
            "the recovery save must not restore stale language"
        );
    }

    #[test]
    #[serial_test::serial]
    fn dictation_save_accepts_older_document_without_required_fields() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        std::fs::write(
            dir.path().join(DICTATION_CONFIG_FILE),
            r#"{"model":"small"}"#,
        )
        .unwrap();
        let base = get_dictation_config();
        let mut desired = base.clone();
        desired.speech_volume_db = -24.0;

        save_dictation_config(base, desired, None).unwrap();

        let saved = get_dictation_config();
        assert_eq!(saved.model, "small");
        assert_eq!(saved.speech_volume_db.to_bits(), (-24.0_f32).to_bits());
        assert_eq!(saved.hotkey, "F5");
    }

    /// The old metric compared a common-prefix character count against a byte
    /// length, so one differing leading space read as 0% and said nothing about
    /// how much text the final pass had lost. The ratio says exactly that.
    #[test]
    fn the_accuracy_ratio_compares_character_counts() {
        // Accented dictation: 3 characters, 4 bytes on the composed side. A byte
        // ratio would report the final pass as having lost text it kept.
        let ratio = transcription_ratio("città", "città").expect("both sides present");
        assert!(
            (ratio - 1.0).abs() < 1e-9,
            "identical text is 1.0, got {ratio}"
        );
    }

    /// A final pass that lost a whole window trips the warning.
    #[test]
    fn a_final_pass_that_lost_a_window_is_below_the_warning_ratio() {
        let ratio = transcription_ratio(&"x".repeat(1000), &"x".repeat(1442)).expect("present");
        assert!(ratio < SHORT_TRANSCRIPTION_RATIO, "1000/1442 = {ratio}");
    }

    /// The reported incident — full=1175 against composed=1442 — lands at 81.5%.
    /// The threshold exists to catch exactly this, so it has to warn here: an
    /// earlier 0.8 line sat below the incident and would have stayed silent on
    /// the very recording that motivated the warning.
    #[test]
    fn the_reported_incident_trips_the_warning_ratio() {
        let ratio = transcription_ratio(&"x".repeat(1175), &"x".repeat(1442)).expect("present");

        assert!((ratio - 0.815).abs() < 0.001, "1175/1442 = {ratio}");
        assert!(ratio < SHORT_TRANSCRIPTION_RATIO, "must trip the warning");
    }

    /// A final pass that merely tidies the partials is not a loss — the warning
    /// has to stay quiet there or it fires on every ordinary dictation.
    #[test]
    fn a_final_pass_close_to_the_partials_does_not_warn() {
        let ratio = transcription_ratio(&"x".repeat(95), &"x".repeat(100)).expect("present");
        assert!(ratio >= SHORT_TRANSCRIPTION_RATIO, "95/100 = {ratio}");

        // The final pass is routinely LONGER: streaming skips VAD-silent windows.
        let ratio = transcription_ratio(&"x".repeat(140), &"x".repeat(100)).expect("present");
        assert!(ratio >= SHORT_TRANSCRIPTION_RATIO, "140/100 = {ratio}");
    }

    /// With no partials there is nothing to compare against, and dividing by zero
    /// would report every such run as a loss.
    #[test]
    fn no_partials_means_no_ratio() {
        assert!(transcription_ratio("run the tests", "").is_none());
    }

    /// The gates are read from the config on every start, so a config written
    /// before they existed must not silently disable them: a missing
    /// `no_speech_threshold` deserializing to `0.0` would reject every
    /// transcription, and a missing `rms_threshold` would accept every one.
    // Exact equality on purpose: each assertion says the number is carried
    // through unchanged, not that it lands close to a computed value.
    #[allow(clippy::float_cmp)]
    #[test]
    fn a_config_written_before_the_gates_existed_keeps_the_defaults() {
        let stored = serde_json::json!({
            "enabled": true,
            "hotkey": "F5",
            "language": "auto",
        });
        let config: DictationConfig = serde_json::from_value(stored).expect("deserialize");

        assert_eq!(config.rms_threshold, transcribe::DEFAULT_RMS_THRESHOLD);
        assert_eq!(
            config.no_speech_threshold,
            transcribe::DEFAULT_NO_SPEECH_THRESHOLD
        );
        assert_eq!(config.gates(), transcribe::VoiceGates::default());
    }

    /// A config written before the loudness stage existed must load at the
    /// level a fresh install gets, not at 0 dB and no levelling.
    #[allow(clippy::float_cmp)]
    #[test]
    fn a_config_written_before_loudness_existed_keeps_the_speech_level_defaults() {
        let stored = serde_json::json!({
            "enabled": true,
            "hotkey": "F5",
            "language": "auto",
        });
        let config: DictationConfig = serde_json::from_value(stored).expect("deserialize");

        assert_eq!(config.speech_volume_db, -18.0);
        assert_eq!(config.speech_levelling, 0.67);
        let fresh = DictationConfig::default();
        assert_eq!(config.speech_volume_db, fresh.speech_volume_db);
        assert_eq!(config.speech_levelling, fresh.speech_levelling);
    }

    #[test]
    fn a_fresh_config_frees_the_model_after_twenty_idle_minutes() {
        assert_eq!(DictationConfig::default().model_idle_unload_minutes, 20);
        let stored = serde_json::json!({"enabled": true, "hotkey": "F5"});
        assert_eq!(
            dictation_config_from_value(stored).model_idle_unload_minutes,
            20,
            "a config written before the knob existed takes the default"
        );
    }

    #[test]
    fn an_explicit_zero_keeps_the_model_loaded_and_survives_a_read() {
        let stored = serde_json::json!({"model_idle_unload_minutes": 0});
        assert_eq!(
            dictation_config_from_value(stored).model_idle_unload_minutes,
            0
        );
    }

    /// A hand-edited config can carry any number; the stage only accepts the
    /// documented -30..-12 dB, or the final clamp would distort every reply.
    #[test]
    fn the_loudness_stage_gets_a_volume_in_the_documented_range() {
        for (stored, expected) in [
            (6.0, -12.0),
            (-60.0, -30.0),
            (-20.0, -20.0),
            (f32::NAN, -18.0),
        ] {
            let config = DictationConfig {
                speech_volume_db: stored,
                ..DictationConfig::default()
            };
            assert_eq!(
                config.loudness().volume_db.to_bits(),
                f32::to_bits(expected),
                "stored {stored}"
            );
        }
    }

    /// Settings > Voice moves these two numbers and nothing else carries
    /// them to the transcriber.
    // Exact equality on purpose: each assertion says the number is carried
    // through unchanged, not that it lands close to a computed value.
    #[allow(clippy::float_cmp)]
    #[test]
    fn tuned_thresholds_reach_the_gates() {
        let config = DictationConfig {
            rms_threshold: 0.004,
            no_speech_threshold: 0.35,
            ..Default::default()
        };

        let gates = config.gates();
        assert_eq!(gates.rms_threshold, 0.004);
        assert_eq!(gates.no_speech_threshold, 0.35);
    }

    /// Voiced-speech stand-in: a 120 Hz harmonic stack under a 4 Hz syllable
    /// envelope, scaled to the requested whole-signal RMS. Synthetic: the
    /// repository holds no recorded speech to feed instead.
    fn speech_at(rms: f32, seconds: usize) -> Vec<f32> {
        let rate = 16_000.0_f32;
        let mut samples: Vec<f32> = (0..16_000 * seconds)
            .map(|i| {
                let t = i as f32 / rate;
                let envelope = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                let voice: f32 = (1..=8)
                    .map(|h| (2.0 * std::f32::consts::PI * 120.0 * h as f32 * t).sin() / h as f32)
                    .sum();
                envelope * voice
            })
            .collect();
        let level = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        for s in &mut samples {
            *s *= rms / level;
        }
        samples
    }

    /// Does one second of speech at `rms`, then two seconds of quiet, close an
    /// utterance in a segmenter built from `config`?
    fn opens_an_utterance(config: continuous::SegmenterConfig, rms: f32) -> bool {
        let mut audio = speech_at(rms, 1);
        audio.extend(std::iter::repeat_n(0.0, 32_000));
        !continuous::Segmenter::new(config).push(&audio).is_empty()
    }

    /// Catches: hands-free ignoring the Settings > Voice floor and running on a
    /// compiled 0.01, so a voice at a normal distance (0.1-1% RMS) never opens
    /// an utterance while push-to-talk, on the 0.001 floor, transcribes it.
    #[test]
    fn hands_free_hears_the_levels_push_to_talk_hears() {
        let gates = DictationConfig::default().gates();
        for rms in [0.002, 0.004, 0.008] {
            let audio = speech_at(rms, 1);
            assert!(
                continuous::has_sustained_speech(&audio, segmenter_config(gates)),
                "push-to-talk must hear speech at RMS {rms}"
            );
            assert!(
                opens_an_utterance(segmenter_config(gates), rms),
                "hands-free must hear speech at RMS {rms}"
            );
        }
        // The compiled default is what hands-free used to run on.
        assert!(!opens_an_utterance(
            continuous::SegmenterConfig::default(),
            0.004
        ));
    }

    /// Catches: the configured floor not reaching hands-free (raising it in
    /// Settings to reject a noisy room would do nothing there).
    #[test]
    fn a_raised_floor_silences_quiet_speech_in_hands_free() {
        let gates = DictationConfig {
            rms_threshold: 0.02,
            ..Default::default()
        }
        .gates();
        assert!(!opens_an_utterance(segmenter_config(gates), 0.004));
    }

    /// Catches: echo cancellation ducking the near end while nothing plays,
    /// which would make hands-free quieter than push-to-talk. Prints the level
    /// at each stage so the measurement can be read with `--no-capture`.
    #[test]
    fn the_echo_canceller_leaves_the_voice_level_alone_when_nothing_plays() {
        use crate::dictation::echo::{Canceller, FRAME_SAMPLES, webrtc::WebRtc};
        let rms_of = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
        for rms in [0.002_f32, 0.005, 0.01, 0.05] {
            let input = speech_at(rms, 4);
            let mut canceller = WebRtc::new().expect("the bundled APM starts");
            let silence = vec![0.0_f32; FRAME_SAMPLES];
            let mut output = input.clone();
            for frame in output.as_chunks_mut::<FRAME_SAMPLES>().0 {
                canceller.cancel(&silence, frame);
            }
            // Skip the first second: the filter may still be settling.
            let (before, after) = (rms_of(&input[16_000..]), rms_of(&output[16_000..]));
            eprintln!(
                "aec3 silent far end: in {before:.5} out {after:.5} ratio {:.3}",
                after / before
            );
            assert!(
                after / before > 0.9,
                "AEC3 ducked the voice at RMS {rms}: {before} -> {after}"
            );
        }
    }

    #[test]
    fn the_meter_tick_does_not_re_read_config_or_stat_the_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // "small" is not the default, so a fresh read is distinguishable from
        // the fallback in `resolve_model`.
        write_model_config("small");
        assert_eq!(model_snapshot().model, model::WhisperModel::Small);

        // Delete the config file. A snapshot recomputed per call would now read
        // nothing and fall back to the default model.
        std::fs::remove_file(dir.path().join(DICTATION_CONFIG_FILE)).expect("remove config");

        for tick in 0..13 {
            assert_eq!(
                model_snapshot().model,
                model::WhisperModel::Small,
                "meter tick {tick} re-read dictation-config.json"
            );
        }
    }

    /// A debug build and the installed app share one configuration directory and
    /// one model directory. Whatever the other process changes there — the
    /// selected model, a download, a deletion — reaches this one through nothing
    /// but the expiry, so a snapshot that never expires is served forever.
    #[test]
    fn a_change_made_by_another_process_is_picked_up_when_the_snapshot_expires() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        write_model_config("small");
        assert_eq!(model_snapshot().model, model::WhisperModel::Small);

        // The other process rewrites the file. Nothing invalidates our cache:
        // `set_dictation_config` ran in a different process.
        std::fs::write(
            dir.path().join(DICTATION_CONFIG_FILE),
            serde_json::to_vec(&DictationConfig {
                model: "large-v2".to_string(),
                ..Default::default()
            })
            .expect("serialize"),
        )
        .expect("write config");

        assert_eq!(
            model_snapshot().model,
            model::WhisperModel::Small,
            "inside the window the cached answer is still served"
        );

        // Age the snapshot past its expiry.
        {
            let mut slot = MODEL_SNAPSHOT.lock();
            let (_, taken) = slot.as_mut().expect("a snapshot was cached");
            *taken = std::time::Instant::now() - MODEL_SNAPSHOT_TTL * 2;
        }
        assert_eq!(
            model_snapshot().model,
            model::WhisperModel::LargeV2,
            "an expired snapshot must be recomputed from disk"
        );
    }

    #[test]
    fn saving_the_config_invalidates_the_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        write_model_config("small");
        assert_eq!(model_snapshot().model, model::WhisperModel::Small);

        write_model_config("large-v2");
        assert_eq!(
            model_snapshot().model,
            model::WhisperModel::LargeV2,
            "set_dictation_config must invalidate the cached snapshot"
        );
    }

    #[test]
    fn the_snapshot_reports_a_missing_model_as_not_downloaded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        write_model_config("small");
        let snapshot = model_snapshot();
        assert!(!snapshot.downloaded);
        assert_eq!(snapshot.size_mb, 0);
    }

    #[test]
    fn the_snapshot_reports_a_present_model_with_its_on_disk_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // `model_exists` requires more than 1 MB to treat a file as a real model.
        std::fs::create_dir_all(model::models_dir()).expect("models dir");
        std::fs::write(
            model::model_path(model::WhisperModel::Small),
            vec![0u8; 3 * 1_048_576],
        )
        .expect("write model");

        write_model_config("small");
        let snapshot = model_snapshot();
        assert!(snapshot.downloaded);
        assert_eq!(snapshot.size_mb, 3);
    }

    #[test]
    fn resolve_model_falls_back_to_the_default_on_an_unknown_name() {
        assert_eq!(resolve_model("small"), model::WhisperModel::Small);
        assert_eq!(
            resolve_model("nonexistent"),
            model::WhisperModel::LargeV3Turbo
        );
    }

    // -----------------------------------------------------------------------
    // Spoken replies (817-f67c)
    // -----------------------------------------------------------------------

    /// Synthesis the test decides the duration of.
    ///
    /// It takes `gate` before producing anything, so a test holding that lock
    /// holds the reply in the queue: without it the worker thread finishes a
    /// one-sample render before the assertion runs, and "accepting is not
    /// hearing" would pass or fail on scheduling rather than on the rule.
    struct HeldSpeech {
        gate: Arc<parking_lot::Mutex<()>>,
    }

    impl speech::Speech for HeldSpeech {
        fn synthesize(
            &self,
            _text: &str,
            _voice: &str,
            _cancel: &speech::SpeechCancel,
        ) -> Result<speech::SpeechAudio, speech::SpeechError> {
            let _held = self.gate.lock();
            Ok(speech::SpeechAudio {
                samples: vec![0.0; 16],
                sample_rate: 24_000,
            })
        }
    }

    /// A device that accepts audio and is never busy afterwards, so a reply
    /// handed to it drains on the next poll.
    struct QuietOutput;

    impl speaker::Output for QuietOutput {
        fn play(&self, _audio: &speech::SpeechAudio) -> Result<(), String> {
            Ok(())
        }
        fn stop(&self) {}
        fn is_speaking(&self) -> bool {
            false
        }
    }

    /// A configuration directory of this test's own, holding `config`.
    ///
    /// Everything that reaches a voice now reads the dictation settings, so a
    /// test without this one reads Boss's — and passes or fails depending on
    /// the language he happens to dictate in. The returned value holds both the
    /// directory and the process-wide override; drop it and the next test gets
    /// its own.
    #[must_use]
    fn config_of_this_test(config: DictationConfig) -> (tempfile::TempDir, impl Drop) {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        save_dictation_config(get_dictation_config(), config, None).expect("config save");
        (dir, guard)
    }

    /// A conversation armed for `session_id`, with a voice whose rendering the
    /// test controls through the returned gate.
    ///
    /// The configuration comes back with it because it has to outlive the
    /// conversation: a dropped override sends the next `speak` looking for the
    /// language in the real config directory.
    fn armed_with_a_voice(
        session_id: &str,
    ) -> (
        DictationState,
        Arc<parking_lot::Mutex<()>>,
        (tempfile::TempDir, impl Drop),
    ) {
        let config = config_of_this_test(DictationConfig {
            language: "it".to_string(),
            speech_engine: "pocket".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        let generation = dictation
            .hands_free
            .lock()
            .arm(session_id, "desktop", true)
            .expect("arm");
        let gate = Arc::new(parking_lot::Mutex::new(()));
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                Arc::new(HeldSpeech {
                    gate: Arc::clone(&gate),
                }),
                Arc::new(QuietOutput),
                generation,
            )),
            voice: "giovanni".to_string(),
            language: "it".to_string(),
        });
        (dictation, gate, config)
    }

    #[test]
    fn spoken_replies_off_refuses_existing_queue_without_synthesis() {
        // catches: a UI-only mute still lets the model invoke the engine.
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CountingSpeech(Arc<AtomicUsize>);
        impl speech::Speech for CountingSpeech {
            fn synthesize(
                &self,
                _: &str,
                _: &str,
                _: &speech::SpeechCancel,
            ) -> Result<speech::SpeechAudio, speech::SpeechError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(speech::SpeechError::Failed("unexpected synthesis".into()))
            }
        }
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let calls = Arc::new(AtomicUsize::new(0));
        dictation.speaker.lock().as_mut().unwrap().speaker = Arc::new(speaker::Speaker::new(
            Arc::new(CountingSpeech(calls.clone())),
            Arc::new(QuietOutput),
            1,
        ));
        let base = get_dictation_config();
        save_dictation_config(
            base.clone(),
            DictationConfig {
                hands_free_spoken_replies: false,
                ..base
            },
            Some(&dictation),
        )
        .unwrap();
        let status = speech_status_for(&dictation, Caller::Model("session-a"), None).unwrap();
        assert!(!status.available);
        assert!(status.unavailable_reason.contains("Spoken replies are off"));
        assert!(
            speak(&dictation, Caller::Model("session-a"), "hello", None)
                .unwrap_err()
                .contains("Spoken replies are off")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(
            dictation.speaker.lock().is_none(),
            "mute cancels the old queue"
        );
        assert!(
            !get_dictation_config().hands_free_spoken_replies,
            "persisted off"
        );
        assert!(
            DictationConfig::default().hands_free_spoken_replies,
            "default on"
        );
        let base = get_dictation_config();
        save_dictation_config(
            base.clone(),
            DictationConfig {
                hands_free_spoken_replies: true,
                ..base
            },
            Some(&dictation),
        )
        .unwrap();
        assert!(get_dictation_config().hands_free_spoken_replies);
    }

    #[test]
    fn queued_auth_rejection_notifies_bound_conversation_once_and_refuses_retry() {
        // catches: a queued HTTP 403 fails silently and the model keeps calling speech.
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Rejecting(Arc<AtomicUsize>, Arc<parking_lot::Mutex<()>>);
        impl speech::Speech for Rejecting {
            fn synthesize(
                &self,
                _: &str,
                _: &str,
                _: &speech::SpeechCancel,
            ) -> Result<speech::SpeechAudio, speech::SpeechError> {
                let _guard = self.1.lock();
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(speech::SpeechError::Rejected { status: 403 })
            }
        }
        #[derive(Default)]
        struct Notices(parking_lot::Mutex<Vec<(String, String)>>);
        impl continuous::VoiceSink for Notices {
            fn write(&self, session: &str, text: &str) -> Result<continuous::VoiceWrite, String> {
                self.0.lock().push((session.into(), text.into()));
                Ok(continuous::VoiceWrite::Written)
            }
        }
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let base = get_dictation_config();
        save_dictation_config(
            base.clone(),
            DictationConfig {
                speech_engine: "edge".into(),
                ..base
            },
            Some(&dictation),
        )
        .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(parking_lot::Mutex::new(()));
        let held = release.lock();
        let generation = dictation.hands_free.lock().generation();
        let engine = guard_replies(
            &get_dictation_config(),
            &dictation,
            Arc::new(Rejecting(calls.clone(), release.clone())),
            generation,
        );
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                engine,
                Arc::new(QuietOutput),
                generation,
            )),
            voice: "edge-test".into(),
            language: "it".into(),
        });
        let queued = speak(&dictation, Caller::Model("session-a"), "hello", None).unwrap();
        assert!(matches!(queued.state.as_str(), "queued" | "rendering"));
        drop(held);
        wait_for_utterance(&dictation, &queued.utterance_id, "failed");
        let status = speech_status_for(&dictation, Caller::Model("session-a"), None).unwrap();
        assert!(!status.available);
        assert!(status.unavailable_reason.contains("HTTP 403"));
        assert!(status.unavailable_reason.contains("Microsoft Edge"));
        let sink = Notices::default();
        assert_eq!(
            continuous::deliver_speech_outage(&mut dictation.hands_free.lock(), &sink),
            Some(Ok(continuous::VoiceWrite::Written))
        );
        assert!(
            continuous::deliver_speech_outage(&mut dictation.hands_free.lock(), &sink).is_none()
        );
        assert_eq!(
            sink.0.lock().as_slice(),
            &[("session-a".into(), status.unavailable_reason.clone())]
        );
        assert_eq!(
            speak(&dictation, Caller::Model("session-a"), "retry", None).unwrap_err(),
            status.unavailable_reason
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// Poll until `id` reaches a state the test is waiting for, or say what it
    /// was stuck on. The worker thread decides when, so a fixed sleep would be
    /// a guess; the deadline is the harness bound, not the behaviour.
    fn wait_for_utterance(dictation: &DictationState, id: &str, wanted: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            last = speech_status(dictation, Some(id))
                .utterance
                .expect("an id was asked about")
                .state;
            if last == wanted {
                return last;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("utterance {id} never reached {wanted}; it is {last}");
    }

    /// A model may drive only the conversation it is the target of. Reading the
    /// queue is refused for the same reason as speaking into it: the fields
    /// alone say what somebody else's conversation is doing.
    #[test]
    fn a_model_bound_to_another_terminal_can_neither_speak_nor_read_the_queue() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        let refused = speak(&dictation, Caller::Model("session-b"), "hello", None).unwrap_err();
        assert!(
            refused.contains("bound to another session"),
            "the refusal must name the reason: {refused}"
        );
        assert_eq!(
            stop_speaking(&dictation, Caller::Model("session-b")).unwrap_err(),
            refused
        );
        assert_eq!(
            speech_status_for(&dictation, Caller::Model("session-b"), None).unwrap_err(),
            refused
        );

        // The bound model gets past the binding, and so does the owner.
        assert!(speak(&dictation, Caller::Model("session-a"), "hello", None).is_ok());
        assert!(speak(&dictation, Caller::Owner, "hello", None).is_ok());
    }

    /// Nothing armed is not an error for `status` — the model asked a fair
    /// question — but it is one for `speak`, which would otherwise have to
    /// choose a conversation itself.
    #[test]
    fn with_nothing_armed_speech_is_refused_and_status_says_why() {
        let dictation = DictationState::new();

        assert!(
            speak(&dictation, Caller::Model("session-a"), "hello", None)
                .unwrap_err()
                .contains("not armed")
        );

        let status = speech_status_for(&dictation, Caller::Model("session-a"), None)
            .expect("status is answerable when nothing is armed");
        assert!(!status.available);
        assert_eq!(status.unavailable_reason, "Hands-free is not armed");
        assert_eq!(status.session_id, None);
    }

    /// The turn is the guard against reviving a reply the user already talked
    /// over: `hush` opens a new one, and a reply written for the old turn is
    /// refused rather than played over whatever was said next.
    #[test]
    fn a_reply_written_for_a_turn_the_user_talked_over_is_refused() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let before = speech_status(&dictation, None).turn;

        let after = stop_speaking(&dictation, Caller::Owner).expect("stop").turn;
        assert!(
            after > before,
            "stopping must open a new turn: {before} -> {after}"
        );

        let stale = speak(&dictation, Caller::Owner, "too late", Some(before)).unwrap_err();
        assert!(
            stale.contains(&format!("turn {before}")) && stale.contains(&format!("turn {after}")),
            "the refusal must name both turns so the model can retry: {stale}"
        );

        // Omitting the turn means "now", which is still allowed.
        let fresh = speak(&dictation, Caller::Owner, "in time", None).expect("current turn");
        assert_eq!(fresh.turn, after);
    }

    /// Accepting a reply is not the user hearing it. The state a caller gets
    /// back from `speak` is the queue's, and only the device going quiet can
    /// produce `finished`.
    #[test]
    fn accepting_a_reply_is_never_reported_as_having_been_heard() {
        let (dictation, gate, _config) = armed_with_a_voice("session-a");
        let held = gate.lock();

        let accepted = speak(&dictation, Caller::Owner, "a spoken reply", None).expect("accepted");
        assert!(
            accepted.state == "queued" || accepted.state == "rendering",
            "acceptance reports the queue, not the speaker: {}",
            accepted.state
        );
        assert_eq!(accepted.error, None);

        let polled = speech_status(&dictation, Some(&accepted.utterance_id))
            .utterance
            .expect("the reply is remembered");
        assert_ne!(
            polled.state, "finished",
            "nothing can be finished while synthesis has not returned"
        );

        // Only now can it be rendered, played, and observed to have drained.
        drop(held);
        wait_for_utterance(&dictation, &accepted.utterance_id, "finished");
    }

    /// A reply the conversation no longer remembers is a different answer from
    /// "you asked about nothing", and a caller polling its own id has to be
    /// able to tell them apart.
    #[test]
    fn an_utterance_this_conversation_never_had_is_reported_as_unknown() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        let status = speech_status(&dictation, Some("4242"));
        let asked = status.utterance.expect("asking must produce an answer");
        assert_eq!(asked.utterance_id, "4242");
        assert_eq!(asked.state, "unknown");

        assert!(
            speech_status(&dictation, None).utterance.is_none(),
            "asking about nothing must leave the field absent"
        );
    }

    /// Bounded input, in the caller's terms. The synthesis budget already stops
    /// one runaway render; it cannot stop a model queueing several.
    #[test]
    fn an_empty_or_oversized_reply_is_refused_before_it_reaches_the_queue() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        assert_eq!(
            speak(&dictation, Caller::Owner, "   \n ", None).unwrap_err(),
            "Nothing to say"
        );

        let long = "è".repeat(MAX_SPOKEN_CHARS + 1);
        let refused = speak(&dictation, Caller::Owner, &long, None).unwrap_err();
        assert!(
            refused.contains(&(MAX_SPOKEN_CHARS + 1).to_string()),
            "counted in characters, not bytes: {refused}"
        );
        assert_eq!(speech_status(&dictation, None).queued, 0);
    }

    /// The one shape both transports serialize. This pins the field names a
    /// store reads over IPC and over HTTP, in the casing the wire uses.
    #[test]
    fn the_speech_status_wire_shape_names_every_field_a_client_reads() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let accepted = speak(&dictation, Caller::Owner, "hello", None).expect("accepted");

        let wire = serde_json::to_value(speech_status(&dictation, Some(&accepted.utterance_id)))
            .expect("serialize");

        assert_eq!(wire["available"], serde_json::json!(true));
        assert_eq!(wire["unavailableReason"], serde_json::json!(""));
        assert_eq!(wire["sessionId"], serde_json::json!("session-a"));
        assert_eq!(wire["language"], serde_json::json!("it"));
        assert_eq!(wire["voice"], serde_json::json!("giovanni"));
        assert!(wire["turn"].is_u64());
        assert!(wire["queued"].is_u64());
        assert!(wire["rendering"].is_boolean());
        assert!(wire["speaking"].is_boolean());
        assert_eq!(wire["lastError"], serde_json::Value::Null);
        assert_eq!(
            wire["utterance"]["utteranceId"],
            serde_json::json!(accepted.utterance_id)
        );
        assert!(wire["utterance"]["state"].is_string());
    }

    /// Answering a caller and answering its poll must describe a reply the same
    /// way (817-f67c criterion 5).
    ///
    /// `speak` returns a `SpokenReply`, `status` nests one, and the deferred
    /// `/events` arm will carry one — three surfaces, one shape. The assertion
    /// is on the *keys* rather than the values, and deliberately so: the render
    /// thread advances the state between the two reads, so a value comparison
    /// would be a race dressed up as a contract.
    #[test]
    fn a_reply_looks_the_same_whether_it_was_accepted_or_polled_for() {
        fn keys(reply: &SpokenReply) -> Vec<String> {
            let mut keys: Vec<String> = serde_json::to_value(reply)
                .expect("serialize")
                .as_object()
                .expect("an object")
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        }

        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let accepted = speak(&dictation, Caller::Owner, "hello", None).expect("accepted");

        let answered = serde_json::to_value(&accepted).expect("serialize");
        assert_eq!(
            answered["utteranceId"],
            serde_json::json!(accepted.utterance_id)
        );
        assert!(answered["state"].is_string());
        assert!(answered["turn"].is_u64());
        assert_eq!(
            keys(&accepted),
            ["error", "state", "turn", "utteranceId"],
            "these are the four fields every transport carries for one reply"
        );

        let polled = speech_status(&dictation, Some(&accepted.utterance_id))
            .utterance
            .expect("the id was asked about");
        assert_eq!(
            polled.utterance_id, accepted.utterance_id,
            "polling by id must answer about that id"
        );
        assert_eq!(
            keys(&polled),
            keys(&accepted),
            "a field added to one construction site and not the other reads as a \
             reply that changed while nobody touched it"
        );
    }

    // --- The push half, on both transports (833-6fd4) ---------------------

    /// The desktop window and an SSE consumer are told the same thing, under
    /// the same name, about the same action.
    ///
    /// Equality of the bodies is by construction — one value is cloned to both
    /// transports — so what this really pins is the half that can still drift:
    /// the SSE arm's event name, and its refusal to re-wrap the body. A bare
    /// object on one transport and `{"payload": {...}}` on the other is a store
    /// that works on the desktop and renders nothing in a browser, which is
    /// exactly the failure the three events were opened for.
    #[test]
    fn every_dictation_push_names_and_shapes_itself_the_same_on_both_transports() {
        use crate::mcp_http::sse_routes::{event_payload_for_test, event_type_name_for_test};
        use crate::state::AppEvent;

        let whisper = download_progress(None, 512, 2_048);
        let asset = download_progress(Some("italian"), 1, 4);
        let reply = serde_json::to_value(SpokenReply::new(
            "3".parse().expect("an utterance id"),
            &speaker::Utterance::Finished,
            9,
        ))
        .expect("serialize");

        for (event, name, desktop) in [
            (
                AppEvent::DictationDownloadProgress {
                    payload: whisper.clone(),
                },
                DICTATION_DOWNLOAD_PROGRESS,
                &whisper,
            ),
            (
                AppEvent::SpeechDownloadProgress {
                    payload: asset.clone(),
                },
                SPEECH_DOWNLOAD_PROGRESS,
                &asset,
            ),
            (
                AppEvent::SpeechUtterance {
                    payload: reply.clone(),
                },
                SPEECH_UTTERANCE,
                &reply,
            ),
        ] {
            assert_eq!(
                event_type_name_for_test(&event),
                name,
                "the SSE stream must offer the name the frontend already listens for"
            );
            assert_eq!(
                &event_payload_for_test(&event),
                desktop,
                "{name} arrives in a different shape over SSE than through the window"
            );
        }
    }

    /// The two downloads differ in one field, and the difference is
    /// load-bearing.
    #[test]
    fn only_a_speech_download_names_its_asset() {
        let whisper = download_progress(None, 512, 2_048);
        let asset = download_progress(Some("italian"), 1, 4);

        assert_eq!(whisper["percent"], serde_json::json!(25));
        assert!(
            whisper.get("asset").is_none(),
            "one Whisper model downloads at a time, so there is nothing to key on"
        );
        assert_eq!(
            asset["asset"],
            serde_json::json!("italian"),
            "the runtime library and a language download together; a shared percent \
             would show each of them the other's"
        );
        assert_eq!(asset["percent"], serde_json::json!(25));
        assert_eq!(
            download_progress(None, 7, 0)["percent"],
            serde_json::json!(0),
            "a server that sent no length must not divide by it"
        );
    }

    /// A hands-free conversation must not read as a silent microphone.
    #[test]
    fn the_level_falls_back_to_the_hands_free_capture() {
        assert_eq!(capture_level(None, Some(0.4)).to_bits(), 0.4_f32.to_bits());
        assert_eq!(
            capture_level(Some(0.2), Some(0.4)).to_bits(),
            0.2_f32.to_bits()
        );
        assert_eq!(capture_level(None, None).to_bits(), 0.0_f32.to_bits());
    }

    /// The frontend keys its bar on `asset` and ends it on `done`; a progress
    /// event must never carry `done`, or the bar would vanish mid-download.
    #[test]
    fn a_finished_speech_download_names_its_asset_and_says_done() {
        let finished = download_finished("italian");
        assert_eq!(finished["asset"], serde_json::json!("italian"));
        assert_eq!(finished["done"], serde_json::json!(true));
        assert!(
            download_progress(Some("italian"), 4, 4)
                .get("done")
                .is_none()
        );
    }

    /// Auto has no language until somebody speaks, and a voice assistant that
    /// guesses one guesses English. It has to say so instead.
    #[test]
    fn under_auto_there_is_no_language_and_so_no_voice_until_somebody_speaks() {
        let _config = config_of_this_test(DictationConfig {
            language: "auto".to_string(),
            speech_engine: "pocket".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        dictation
            .hands_free
            .lock()
            .arm("session-a", "desktop", true)
            .expect("arm");

        let before = speech_status(&dictation, None);
        assert!(!before.available);
        assert_eq!(before.language, "", "no turn, no language, and no default");
        assert!(
            before.unavailable_reason.contains("Auto"),
            "the state has to name Auto as the reason: {}",
            before.unavailable_reason
        );
        assert!(
            speak(&dictation, Caller::Owner, "ciao", None)
                .unwrap_err()
                .contains("Auto")
        );

        // The user speaks Italian. The conversation now has a language, and
        // the reason changes from "nothing said yet" to whatever is wrong with
        // Italian on this machine — here, a bundle nobody downloaded.
        let generation = dictation.hands_free.lock().generation();
        dictation
            .hands_free
            .lock()
            .accept_transcript(generation, "ciao", Some("it"), 0);

        let after = speech_status(&dictation, None);
        assert_eq!(
            after.language, "it",
            "Auto must expose what Whisper actually detected"
        );
        assert!(
            !after.unavailable_reason.contains("Auto"),
            "the detection answered Auto's question; what is left is about this machine: {}",
            after.unavailable_reason
        );
        assert!(
            after.unavailable_reason.contains("not downloaded"),
            "on a machine with no speech assets that is what is missing: {}",
            after.unavailable_reason
        );
    }

    /// A language Whisper transcribes and no bundle speaks is reported as
    /// itself. Substituting a voice we do ship is how an Italian conversation
    /// gets answered in English.
    #[test]
    fn a_language_no_bundle_speaks_is_named_rather_than_replaced() {
        let _config = config_of_this_test(DictationConfig {
            language: "ko".to_string(),
            speech_engine: "pocket".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        dictation
            .hands_free
            .lock()
            .arm("session-a", "desktop", true)
            .expect("arm");

        let status = speech_status(&dictation, None);
        assert!(!status.available);
        assert_eq!(status.language, "ko");
        assert_eq!(
            status.unavailable_reason,
            "No speech bundle ships for language \"ko\""
        );
        assert!(
            speak(&dictation, Caller::Owner, "안녕", None)
                .unwrap_err()
                .contains("ko")
        );
    }

    /// Criterion 4: the setting moves, and every reply written for the old one
    /// stops. Silence is the only honest outcome — a half-spoken Italian
    /// sentence does not become English by finishing it.
    #[test]
    fn changing_the_language_takes_the_voice_away_from_the_replies_written_for_it() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");
        assert!(speech_status(&dictation, None).queued > 0 || dictation.speaker.lock().is_some());

        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                language: "en".to_string(),
                speech_engine: "pocket".to_string(),
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(
            dictation.speaker.lock().is_none(),
            "the queue built for Italian may not speak English replies"
        );
        let status = speech_status(&dictation, None);
        assert_eq!(status.language, "en", "the model context moves with it");
        assert!(
            !status.available,
            "and nothing is speakable until a voice for the new language opens"
        );
    }

    // --- Choosing a voice (818-2a29) ---------------------------------------

    /// The one field a caller joins an asset to a conversation by.
    ///
    /// The settings panel looks the configured language up in this list to
    /// learn which voices it may offer. Published as the engine's own name for
    /// the language (`"italian"`) it matched nothing, so the panel offered no
    /// voice and said no bundle shipped for Italian directly under the row
    /// offering the Italian bundle.
    #[test]
    fn an_asset_names_its_language_by_the_code_whisper_uses() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        assert_eq!(describe(italian, false).language.as_deref(), Some("it"));
        assert_eq!(
            describe(speech::assets::runtime(), false).language,
            None,
            "the runtime library speaks nothing"
        );
    }

    /// What an untouched configuration and every configuration written before
    /// the setting existed both say.
    #[test]
    fn no_chosen_voice_means_the_first_one_the_language_ships() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        assert_eq!(
            choose_voice(italian, "").expect("a language ships at least one voice"),
            italian.voices()[0]
        );
    }

    #[test]
    fn a_chosen_voice_is_the_one_that_speaks() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let wanted = italian.voices()[0];
        assert_eq!(choose_voice(italian, wanted).expect("shipped"), wanted);
    }

    /// Never a silent fall back to the first voice: the user hears a voice
    /// nobody chose and has nothing on screen saying why.
    #[test]
    fn a_voice_the_language_does_not_ship_is_named_rather_than_replaced() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let error = choose_voice(italian, "nessuno").expect_err("not shipped");
        assert!(error.contains("nessuno"), "{error}");
        assert!(
            error.contains(italian.voices()[0]),
            "the message has to say what there is instead: {error}"
        );
    }

    /// A speech directory of its own, so installed and imported voices of one
    /// test are not another's.
    fn speech_root() -> (tempfile::TempDir, impl Drop) {
        let root = tempfile::tempdir().unwrap();
        let guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        (root, guard)
    }

    /// Put every file of `asset` where a download would, at its pinned size.
    fn install_asset(asset: &speech::assets::Asset) {
        for file in asset.installed_files() {
            let path = asset.install_dir().join(file.name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(path)
                .unwrap()
                .set_len(file.size_bytes.unwrap_or(1))
                .unwrap();
        }
    }

    #[test]
    fn get_speech_voices_offers_nothing_until_the_language_is_downloaded() {
        let (_root, _guard) = speech_root();
        assert!(
            get_speech_voices("it".into())
                .expect("Italian ships")
                .is_empty()
        );
    }

    #[test]
    fn choose_voice_accepts_an_installed_voice_asset() {
        let (_root, _guard) = speech_root();
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let jean = speech::assets::find("voice-italian-jean").expect("jean is in the catalogue");
        std::fs::create_dir_all(jean.install_dir()).unwrap();
        for file in jean.installed_files() {
            std::fs::File::create(jean.install_dir().join(file.name))
                .unwrap()
                .set_len(file.size_bytes.unwrap())
                .unwrap();
        }
        assert_eq!(choose_voice(italian, "jean").expect("downloaded"), "jean");
    }

    /// The catalogue offers it, so the fix is one click away: say which one.
    #[test]
    fn choose_voice_rejects_a_catalogue_voice_that_is_not_downloaded_and_says_download() {
        let (_root, _guard) = speech_root();
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let error = choose_voice(italian, "jean").expect_err("not downloaded");
        assert!(error.contains("jean"), "{error}");
        assert!(error.to_lowercase().contains("download"), "{error}");
    }

    #[test]
    fn choose_voice_accepts_a_user_voice() {
        let (_root, _guard) = speech_root();
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let dir = speech::assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nonna.safetensors"), b"x").unwrap();
        assert_eq!(choose_voice(italian, "nonna").expect("imported"), "nonna");
    }

    #[test]
    fn choose_voice_still_reads_empty_as_the_language_default() {
        let (_root, _guard) = speech_root();
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let dir = speech::assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aaa.safetensors"), b"x").unwrap();
        assert_eq!(
            choose_voice(italian, "").expect("default"),
            italian.voices()[0]
        );
    }

    #[test]
    fn import_speech_voice_names_a_language_by_its_whisper_code() {
        let (_root, _guard) = speech_root();
        let error = import_speech_voice("xx".into(), "nonna".into(), "AAAA".into()).unwrap_err();
        assert!(error.contains("xx"), "{error}");
    }

    #[test]
    fn import_speech_voice_refuses_what_is_not_base64() {
        let (_root, _guard) = speech_root();
        let error =
            import_speech_voice("it".into(), "nonna".into(), "not base64!".into()).unwrap_err();
        assert!(error.contains("base64"), "{error}");
    }

    /// Refused on its length, before a byte of it is decoded: decoding 100 MB
    /// to find out it is too big is the allocation the cap exists to avoid.
    #[test]
    fn import_speech_voice_refuses_an_oversized_payload_before_decoding_it() {
        let (_root, _guard) = speech_root();
        let payload = "!".repeat(speech_voice_base64_limit() + 1);
        let error = import_speech_voice("it".into(), "nonna".into(), payload).unwrap_err();
        assert!(error.contains("MB"), "{error}");
    }

    #[test]
    fn get_speech_voices_lists_what_a_language_can_speak_with_by_its_whisper_code() {
        let (_root, _guard) = speech_root();
        install_asset(speech::assets::find("italian").unwrap());
        let dir = speech::assets::user_voices_dir("italian");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nonna.safetensors"), b"x").unwrap();
        let voices = get_speech_voices("it".into()).expect("Italian ships");
        let ids: Vec<(&str, speech::library::VoiceSource)> =
            voices.iter().map(|v| (v.id.as_str(), v.source)).collect();
        assert_eq!(
            ids,
            vec![
                ("giovanni", speech::library::VoiceSource::Default),
                ("nonna", speech::library::VoiceSource::User),
            ]
        );
        assert!(get_speech_voices("xx".into()).unwrap_err().contains("xx"));
    }

    /// The wire shape the settings page reads: `source` in lowercase.
    #[test]
    fn get_speech_voices_serializes_source_as_the_frontend_expects() {
        let (_root, _guard) = speech_root();
        install_asset(speech::assets::find("italian").unwrap());
        let json = serde_json::to_value(get_speech_voices("it".into()).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{ "id": "giovanni", "source": "default" }])
        );
    }

    #[test]
    fn delete_speech_voice_names_a_language_by_its_whisper_code() {
        let (_root, _guard) = speech_root();
        assert!(delete_speech_voice("xx".into(), "nonna".into()).is_err());
        assert!(delete_speech_voice("it".into(), "nonna".into()).is_ok());
    }

    #[test]
    fn a_voice_asset_is_described_as_a_voice_of_its_language() {
        let jean = speech::assets::find("voice-italian-jean").expect("jean is in the catalogue");
        let info = describe(jean, false);
        assert_eq!(info.kind, "voice");
        assert_eq!(info.language.as_deref(), Some("it"));
        assert_eq!(info.voice.as_deref(), Some("jean"));
        let italian = speech::assets::for_language_code("it").unwrap();
        assert_eq!(describe(italian, false).voice, None);
    }

    /// A voice belongs to a conversation exactly as much as a language does,
    /// so it obeys the same rule: change it and the replies written for the
    /// old one stop rather than finishing in the new one.
    #[test]
    fn changing_the_voice_takes_it_away_from_the_replies_written_for_it() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");
        assert!(dictation.speaker.lock().is_some());

        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                language: "it".to_string(),
                speech_engine: "pocket".to_string(),
                speech_voice: "giovanni".to_string(),
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(
            dictation.speaker.lock().is_none(),
            "the queue built for the old voice may not speak in the new one"
        );
    }

    /// The other half of that rule. Every dictation setting goes through the
    /// same function, and cutting a reply off mid-word because somebody moved
    /// a threshold slider would be the worse bug.
    #[test]
    fn moving_a_threshold_leaves_the_voice_alone() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let accepted = speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");

        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                language: "it".to_string(),
                speech_engine: "pocket".to_string(),
                rms_threshold: 0.05,
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(dictation.speaker.lock().is_some());
        assert!(
            !speech_status(&dictation, Some(&accepted.utterance_id))
                .utterance
                .expect("asked")
                .state
                .is_empty(),
            "the reply in flight still has a fate to report"
        );
    }

    /// The volume and levelling sliders are not a voice change: the queue
    /// that is speaking keeps speaking, and its next reply takes the new level.
    #[test]
    fn moving_the_volume_reaches_the_voice_that_is_speaking() {
        struct Tone(Arc<parking_lot::Mutex<()>>);
        impl speech::Speech for Tone {
            fn synthesize(
                &self,
                _text: &str,
                _voice: &str,
                _cancel: &speech::SpeechCancel,
            ) -> Result<speech::SpeechAudio, speech::SpeechError> {
                let _held = self.0.lock();
                let amplitude = 10f32.powf(-32.0 / 20.0) * 2f32.sqrt();
                let phase = 2.0 * std::f32::consts::PI * 440.0 / 24_000.0;
                Ok(speech::SpeechAudio {
                    samples: (0..24_000)
                        .map(|i| amplitude * (phase * i as f32).sin())
                        .collect(),
                    sample_rate: 24_000,
                })
            }
        }

        #[derive(Default)]
        struct RecordingOutput(parking_lot::Mutex<Vec<f32>>);
        impl speaker::Output for RecordingOutput {
            fn play(&self, audio: &speech::SpeechAudio) -> Result<(), String> {
                *self.0.lock() = audio.samples.clone();
                Ok(())
            }
            fn stop(&self) {}
            fn is_speaking(&self) -> bool {
                false
            }
        }

        let (dictation, gate, _config) = armed_with_a_voice("session-a");
        let output = Arc::new(RecordingOutput::default());
        let generation = dictation.hands_free.lock().generation();
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                Arc::new(Tone(Arc::clone(&gate))),
                Arc::clone(&output) as Arc<dyn speaker::Output>,
                generation,
            )),
            voice: "giovanni".to_string(),
            language: "it".to_string(),
        });
        let held = gate.lock();
        let accepted = speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");
        let before = Arc::clone(&dictation.speaker.lock().as_ref().expect("armed").speaker);

        let config = DictationConfig {
            language: "it".to_string(),
            speech_engine: "pocket".to_string(),
            speech_volume_db: -24.0,
            speech_levelling: 0.2,
            ..Default::default()
        };
        save_dictation_config(get_dictation_config(), config.clone(), Some(&dictation))
            .expect("config save");

        let after = Arc::clone(
            &dictation
                .speaker
                .lock()
                .as_ref()
                .expect("the queue was dropped")
                .speaker,
        );
        assert!(Arc::ptr_eq(&before, &after), "the queue was rebuilt");
        assert_ne!(
            speech_status(&dictation, Some(&accepted.utterance_id))
                .utterance
                .expect("asked")
                .state,
            "interrupted",
            "the reply in flight was cut off"
        );
        drop(held);
        wait_for_utterance(&dictation, &accepted.utterance_id, "finished");
        let samples = output.0.lock();
        assert!(!samples.is_empty(), "the reply never reached the output");
        let power = samples
            .iter()
            .map(|&sample| f64::from(sample) * f64::from(sample))
            .sum::<f64>()
            / samples.len() as f64;
        let level = 10.0 * power.log10();
        assert!(
            (level + 24.0).abs() <= 1.0,
            "the played reply reached {level} dBFS, configured -24"
        );
    }

    /// Barge-in reaches whatever is speaking now, not the queue that existed
    /// when the capture loop started — which under Auto is no queue at all.
    #[test]
    fn barge_in_interrupts_the_voice_that_is_open_at_the_time() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let port = ArmedSpeaker::new(Arc::clone(&dictation.speaker));
        let before = speech_status(&dictation, None).turn;

        continuous::Interruptible::hush(&port);

        assert_eq!(
            speech_status(&dictation, None).turn,
            before + 1,
            "the interruption has to open a new turn, or the model's next reply is refused"
        );

        // And an empty slot is a tick with nothing to interrupt, not a panic:
        // the capture loop runs on every conversation, including the ones that
        // never opened a voice.
        *dictation.speaker.lock() = None;
        continuous::Interruptible::hush(&port);
    }

    #[derive(Default)]
    struct PausableOutput(std::sync::atomic::AtomicBool);

    impl speaker::Output for PausableOutput {
        fn play(&self, _audio: &speech::SpeechAudio) -> Result<(), String> {
            Ok(())
        }
        fn stop(&self) {
            // The real device un-pauses on stop, so the next reply is heard.
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        fn pause(&self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        fn resume(&self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_speaking(&self) -> bool {
            false
        }
    }

    /// An armed voice whose output records whether it is paused, and the port
    /// the capture loop uses to reach it.
    fn a_voice_behind_the_port() -> (DictationState, Arc<PausableOutput>, Arc<ArmedSpeaker>) {
        let (dictation, gate, _config) = armed_with_a_voice("session-a");
        let output = Arc::new(PausableOutput::default());
        let generation = dictation.hands_free.lock().generation();
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                Arc::new(HeldSpeech { gate }),
                Arc::clone(&output) as Arc<dyn speaker::Output>,
                generation,
            )),
            voice: "giovanni".to_string(),
            language: "it".to_string(),
        });
        let port = Arc::new(ArmedSpeaker::new(Arc::clone(&dictation.speaker)));
        (dictation, output, port)
    }

    /// Issue `command` while a rebuild holds the slot: the call must return
    /// without waiting for it, and the command must still land once it frees.
    fn command_while_the_slot_is_busy(
        dictation: &DictationState,
        port: &Arc<ArmedSpeaker>,
        command: fn(&ArmedSpeaker),
    ) {
        let busy = dictation.speaker.lock();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let contender = {
            let port = Arc::clone(port);
            std::thread::spawn(move || {
                command(&port);
                let _ = done_tx.send(());
            })
        };
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the capture loop waited for a rebuild");
        drop(busy);
        contender.join().expect("the contender panicked");
    }

    /// Applied from another thread once the slot frees.
    fn eventually(condition: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if condition() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        condition()
    }

    /// The verdict arrives after the pause, so a `hush` that misses the slot
    /// lock (`speak` holds it while it rebuilds the voice) is no longer "a tick
    /// with nothing to interrupt": nothing resumes the reply afterwards, and
    /// the next one is queued into a silent device. Catches: `hush` giving up
    /// on a contended slot while the output is paused.
    #[test]
    fn a_hush_that_meets_a_busy_slot_does_not_leave_the_reply_paused() {
        let (dictation, output, port) = a_voice_behind_the_port();
        continuous::Interruptible::pause(&*port);
        assert!(output.0.load(std::sync::atomic::Ordering::SeqCst));

        command_while_the_slot_is_busy(&dictation, &port, |port| {
            continuous::Interruptible::hush(port)
        });

        assert!(
            eventually(|| !output.0.load(std::sync::atomic::Ordering::SeqCst)),
            "the verdict came while the slot was busy and the reply stayed paused"
        );
    }

    /// The same for a reply that was not for us: the resume that meets a busy
    /// slot must not be lost either. Catches: `resume` dropped on a contended
    /// slot.
    #[test]
    fn a_resume_that_meets_a_busy_slot_does_not_leave_the_reply_paused() {
        let (dictation, output, port) = a_voice_behind_the_port();
        continuous::Interruptible::pause(&*port);
        assert!(output.0.load(std::sync::atomic::Ordering::SeqCst));

        command_while_the_slot_is_busy(&dictation, &port, |port| {
            continuous::Interruptible::resume(port)
        });

        assert!(
            eventually(|| !output.0.load(std::sync::atomic::Ordering::SeqCst)),
            "the resume came while the slot was busy and the reply stayed paused"
        );
    }

    /// The command waiting for a busy slot belongs to the arm that gave it.
    /// Catches: a pending `Pause` surviving a disarm and holding the first
    /// reply of the next conversation.
    #[test]
    fn a_pause_waiting_for_the_slot_is_dropped_when_the_arm_ends() {
        let (dictation, output, port) = a_voice_behind_the_port();
        let busy = dictation.speaker.lock();
        continuous::Interruptible::pause(&*port);
        // The applier thread holds a clone of `pending` until it has run, so
        // the count says when it is done: no sleeping on a thread that may not
        // have been scheduled yet.
        let pending = Arc::clone(&port.pending);
        drop(port);
        drop(busy);
        assert!(
            eventually(|| Arc::strong_count(&pending) == 1),
            "the applier thread never finished"
        );
        assert!(
            !output.0.load(std::sync::atomic::Ordering::SeqCst),
            "a pause from the previous arm held the new voice"
        );
    }

    /// Disarming takes the voice away before the engine goes, so a reply
    /// queued against the old conversation cannot be spoken into the next one.
    #[test]
    fn disarming_drops_the_voice_with_the_conversation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        disarm_hands_free(&state, &dictation);

        assert!(dictation.speaker.lock().is_none());
        assert!(
            speak(&dictation, Caller::Owner, "hello", None)
                .unwrap_err()
                .contains("not armed")
        );
    }

    /// A device that keeps playing until it is told to stop, and remembers
    /// being told.
    ///
    /// `QuietOutput` cannot answer the question below: it is never speaking, so
    /// a disarm that silenced nothing looks exactly like one that silenced
    /// everything.
    #[derive(Default)]
    struct LoudOutput {
        speaking: std::sync::atomic::AtomicBool,
        stops: std::sync::atomic::AtomicUsize,
    }

    impl speaker::Output for LoudOutput {
        fn play(&self, _audio: &speech::SpeechAudio) -> Result<(), String> {
            self.speaking
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn stop(&self) {
            self.stops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.speaking
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_speaking(&self) -> bool {
            self.speaking.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// Disarming silences the speaker, it does not merely forget it
    /// (820-21a5 criterion 2, the playback half of "no stale playback").
    ///
    /// The test above asserts the slot is empty, which is what a *caller* sees.
    /// A user hears the device. Those are the same thing only because dropping
    /// the `Speaker` stops its output, and nothing that looks at the slot alone
    /// can tell a silenced room from a handle dropped while the audio played
    /// on.
    #[test]
    fn disarming_while_a_reply_is_playing_stops_the_device() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let _config = config_of_this_test(DictationConfig {
            language: "it".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        let generation = dictation
            .hands_free
            .lock()
            .arm("session-a", "desktop", true)
            .expect("arm");
        let device = Arc::new(LoudOutput::default());
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                Arc::new(HeldSpeech {
                    gate: Arc::new(parking_lot::Mutex::new(())),
                }),
                Arc::clone(&device) as Arc<dyn speaker::Output>,
                generation,
            )),
            voice: "giovanni".to_string(),
            language: "it".to_string(),
        });

        let reply = speak(&dictation, Caller::Owner, "una risposta", None).expect("accepted");
        wait_for_utterance(&dictation, &reply.utterance_id, "speaking");
        assert!(
            device.speaking.load(std::sync::atomic::Ordering::SeqCst),
            "the fixture has nothing coming out of it, so silencing it would prove nothing"
        );

        disarm_hands_free(&state, &dictation);

        assert!(
            !device.speaking.load(std::sync::atomic::Ordering::SeqCst),
            "the conversation ended while the reply was still audible"
        );
        assert_eq!(
            device.stops.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exactly one stop, from the drop: disarm silences by taking the speaker away, \
             not by hushing it, and a second stop would mean two paths do the same job"
        );
    }

    // --- Telling the model the mode changed (821-842a) --------------------

    /// The setting is read at arm time: what the user wrote is what the model
    /// reads, folded to one line, and the built-in text is not sent beside it.
    #[cfg(unix)]
    #[test]
    fn arming_sends_the_configured_start_notice_instead_of_the_built_in_one() {
        let _config = config_of_this_test(DictationConfig {
            hands_free_start_notice: "Voice on.\nAnswer in Italian.".to_string(),
            ..DictationConfig::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-custom", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-custom");
        let dictation = DictationState::new();

        arm_hands_free_with(
            &state,
            &dictation,
            "voice-custom",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");
        let terminal = wait_for_typed(&typed, "Voice on. Answer in Italian.");

        assert!(
            !terminal.contains(continuous::MODE_ENTRY_HINT),
            "{terminal:?}"
        );
        disarm_hands_free(&state, &dictation);
    }

    #[test]
    fn the_default_notice_accessor_returns_the_text_an_empty_setting_sends() {
        assert_eq!(
            get_hands_free_default_notice(),
            continuous::entry_hint_text(&DictationConfig::default().hands_free_start_notice)
        );
    }

    /// Criterion 1, end to end against a real terminal: the model is told it
    /// can answer out loud when the conversation opens, and told to go back to
    /// text when it ends.
    ///
    /// Both notices are typed at once: the start notice into the idle agent,
    /// the end notice into the agent the start notice just set working. Typed
    /// is the only state in which the model has *read* the start notice — and
    /// reading it is what the end notice is owed to.
    #[cfg(unix)]
    #[test]
    fn arming_tells_the_model_it_can_answer_out_loud_and_disarming_takes_it_back() {
        let _config = config_of_this_test(DictationConfig::default());
        assert!(
            get_dictation_config().hands_free_notify_model,
            "the notices are on by default; a voice nobody is told about is never used"
        );
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-hint", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-hint");
        let dictation = DictationState::new();

        arm_hands_free_with(
            &state,
            &dictation,
            "voice-hint",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");
        let terminal = wait_for_typed(&typed, continuous::MODE_ENTRY_HINT);
        assert_eq!(
            terminal.matches(continuous::MODE_ENTRY_HINT).count(),
            1,
            "one arm, one notice"
        );

        disarm_hands_free(&state, &dictation);

        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            terminal.ends_with(&format!("\u{15}{}\r", continuous::MODE_EXIT_HINT)),
            "the end notice is typed into the session the mode was bound to by the time \
             disarm returns; the terminal holds {terminal:?}"
        );
        assert_eq!(
            crate::pty::queued_command_count(&state, "voice-hint"),
            0,
            "neither notice waits in the Compose queue"
        );
    }

    /// Criterion 3: a notice the model never read is withdrawn, and nothing is
    /// written to contradict something it was never told.
    ///
    /// A permission dialog owns the composer, so the notice is still held when
    /// the user changes their mind — the rapid arm/disarm case.
    #[cfg(unix)]
    #[test]
    fn a_start_notice_the_model_never_read_is_withdrawn_rather_than_contradicted() {
        let _config = config_of_this_test(DictationConfig::default());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-rapid", crate::pty::SHELL_BUSY);
        state
            .session_maps
            .session_states
            .get_mut("voice-rapid")
            .expect("the session was just inserted")
            .question_confident = true;
        let typed = crate::test_support::insert_recording_session(&state, "voice-rapid");
        let dictation = DictationState::new();

        arm_hands_free_with(
            &state,
            &dictation,
            "voice-rapid",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");
        disarm_hands_free(&state, &dictation);
        // The dialog closes after the disarm: nothing may be waiting anywhere
        // to be typed into it then.
        state
            .session_maps
            .session_states
            .get_mut("voice-rapid")
            .expect("session")
            .question_confident = false;

        assert_eq!(
            crate::pty::queued_command_count(&state, "voice-rapid"),
            0,
            "nothing may be parked to undo it"
        );
        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            terminal.is_empty(),
            "neither notice reached the model: the start notice was never read, and an end \
             notice on its own is the only thing it would hear about the mode; got {terminal:?}"
        );

        // Criterion 4, on the same disarm: speech is revoked whether or not
        // the model ever acknowledged — or even read — anything.
        assert!(dictation.speaker.lock().is_none());
        assert!(
            speak(&dictation, Caller::Owner, "too late", None)
                .unwrap_err()
                .contains("not armed")
        );
    }

    /// Criterion 1's other half: the setting turns both notices off, and
    /// nothing else about the mode changes.
    #[cfg(unix)]
    #[test]
    fn the_setting_turned_off_sends_neither_notice() {
        let _config = config_of_this_test(DictationConfig {
            hands_free_notify_model: false,
            ..Default::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-quiet", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-quiet");
        let dictation = DictationState::new();

        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-quiet",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");

        assert!(armed.armed, "the mode still arms; only the notices are off");
        let disarmed = disarm_hands_free(&state, &dictation);
        assert!(disarmed.was_armed);
        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            !terminal.contains(continuous::MODE_ENTRY_HINT)
                && !terminal.contains(continuous::MODE_EXIT_HINT),
            "the terminal holds {terminal:?}"
        );
    }

    /// Criterion 5: push-to-talk is a different feature and shares nothing
    /// with hands-free. Nothing it does arms the mode, opens a VAD runtime,
    /// makes speech available, or tells the model anything.
    #[cfg(unix)]
    #[test]
    fn push_to_talk_alone_arms_nothing_and_tells_the_model_nothing() {
        let _config = config_of_this_test(DictationConfig::default());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-ptt", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-ptt");
        let dictation = DictationState::new();

        // The backend half of a push-to-talk dictation: the transcript is
        // corrected here and handed to the caller, which types it itself.
        let delivered = {
            let text = dictation.corrections.lock().correct("run the tests");
            text.replace('\n', " ")
        };
        assert_eq!(delivered, "run the tests");

        let status = hands_free_status(&dictation);
        assert!(!status.armed, "push-to-talk may not arm hands-free");
        assert!(
            dictation.hands_free_runtime.lock().is_none(),
            "no VAD runtime, so no activation phrase and no continuous capture"
        );
        assert!(
            speak(&dictation, Caller::Owner, "hello", None)
                .unwrap_err()
                .contains("not armed"),
            "MCP speech belongs to an armed conversation, not to a hotkey"
        );
        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            terminal.is_empty(),
            "push-to-talk writes nothing of its own; it holds {terminal:?}"
        );
        assert!(
            state.pending_injections.get("voice-ptt").is_none(),
            "no Compose entry belongs to a push-to-talk transcription"
        );
    }

    // -- Voice preview (855) ------------------------------------------------

    use crate::dictation::loudness::Loudness;

    /// Renders half a second of a -32 dBFS tone: a quiet voice, so the test
    /// can tell whether the loudness stage ran.
    struct QuietTone;

    impl speech::Speech for QuietTone {
        fn synthesize(
            &self,
            _text: &str,
            _voice: &str,
            _cancel: &speech::SpeechCancel,
        ) -> Result<speech::SpeechAudio, speech::SpeechError> {
            let amplitude = 10f32.powf(-32.0 / 20.0) * 2f32.sqrt();
            let phase = 2.0 * std::f32::consts::PI * 440.0 / 24_000.0;
            Ok(speech::SpeechAudio {
                samples: (0..12_000)
                    .map(|n| amplitude * (phase * n as f32).sin())
                    .collect(),
                sample_rate: 24_000,
            })
        }
    }

    /// A device that remembers what it played and how often it was stopped.
    #[derive(Default)]
    struct PreviewDevice {
        played: parking_lot::Mutex<Vec<Vec<f32>>>,
        stops: std::sync::atomic::AtomicUsize,
    }

    impl speaker::Output for PreviewDevice {
        fn play(&self, audio: &speech::SpeechAudio) -> Result<(), String> {
            self.played.lock().push(audio.samples.clone());
            Ok(())
        }
        fn stop(&self) {
            self.stops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_speaking(&self) -> bool {
            !self.played.lock().is_empty()
        }
    }

    const REPLY_LEVEL: Loudness = Loudness {
        volume_db: -18.0,
        levelling: 0.67,
    };

    fn preview_on(dictation: &DictationState, device: &Arc<PreviewDevice>) -> Result<(), String> {
        play_preview(
            dictation,
            &QuietTone,
            Arc::clone(device) as Arc<dyn speaker::Output>,
            "giovanni",
            "Ciao, sono la voce.",
            REPLY_LEVEL,
        )
    }

    #[test]
    fn a_preview_sounds_like_a_reply_and_is_heard_by_the_canceller() {
        // "Listen" must play what a reply would: at the reply level, and
        // through the echo tap, or an armed microphone hears the preview as
        // the user talking.
        let dictation = DictationState::new();
        let device = Arc::new(PreviewDevice::default());

        preview_on(&dictation, &device).expect("played");

        let played = device.played.lock().clone();
        assert_eq!(played.len(), 1);
        let power = played[0]
            .iter()
            .map(|&x| f64::from(x) * f64::from(x))
            .sum::<f64>()
            / played[0].len() as f64;
        let level = 10.0 * power.log10();
        assert!(
            (level + 18.0).abs() <= 1.0,
            "previewed at {level} dBFS, replies at -18"
        );
        assert!(
            dictation.echo.lock().is_playing(),
            "the canceller was not told about the preview"
        );
        assert!(
            dictation.preview.lock().is_some(),
            "the device must outlive the call, or the preview is cut off"
        );
    }

    #[test]
    fn a_preview_is_refused_while_a_reply_is_pending() {
        // Refused, not queued: a preview that waits for a gap in the
        // conversation plays long after the click, over whatever comes next.
        let (dictation, gate, _config) = armed_with_a_voice("session-a");
        let held = gate.lock();
        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");
        let device = Arc::new(PreviewDevice::default());

        let error = preview_on(&dictation, &device).expect_err("the conversation is speaking");

        assert!(error.contains("hands-free reply"), "{error}");
        assert!(
            device.played.lock().is_empty(),
            "the preview played over the reply"
        );
        drop(held);
    }

    #[test]
    fn a_reply_stops_a_preview_before_it_is_queued() {
        // The other direction: the conversation wins, and the preview stops
        // rather than playing under the reply.
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let device = Arc::new(PreviewDevice::default());
        preview_on(&dictation, &device).expect("the conversation is idle");

        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");

        assert_eq!(device.stops.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(dictation.preview.lock().is_none());
    }

    #[test]
    fn a_second_preview_replaces_the_first() {
        let dictation = DictationState::new();
        let first = Arc::new(PreviewDevice::default());
        let second = Arc::new(PreviewDevice::default());

        preview_on(&dictation, &first).expect("played");
        preview_on(&dictation, &second).expect("played");

        assert_eq!(first.stops.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(second.played.lock().len(), 1);
    }

    #[test]
    fn preview_text_is_bounded_and_not_empty() {
        assert_eq!(preview_text("  ciao \n"), Ok("ciao"));
        assert!(preview_text(&"a".repeat(MAX_PREVIEW_CHARS)).is_ok());
        let error = preview_text(&"a".repeat(MAX_PREVIEW_CHARS + 1)).expect_err("too long");
        assert!(error.contains("201"), "{error}");
        assert!(preview_text("   ").is_err());
    }

    #[test]
    fn previewing_an_unknown_voice_names_it_and_selects_nothing() {
        let _config = config_of_this_test(DictationConfig {
            language: "it".to_string(),
            speech_engine: "pocket".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();

        let error = preview_voice(&dictation, "it", "nessuno", "ciao").expect_err("no such voice");

        assert!(error.contains("nessuno"), "{error}");
        assert_eq!(
            get_dictation_config().speech_voice,
            "",
            "a preview selected the voice"
        );
        assert!(preview_voice(&dictation, "xx", "", "ciao").is_err());
    }

    // --- Edge as the default engine (1357-7d37) -----------------------------

    fn engine_of_file(content: &str) -> String {
        let (dir, guard) = speech_root();
        std::fs::write(dir.path().join(DICTATION_CONFIG_FILE), content).unwrap();
        let engine = get_dictation_config().speech_engine;
        drop(guard);
        engine
    }

    #[test]
    fn a_fresh_install_speaks_with_edge() {
        // Catches: the default staying Pocket, which needs a 100+ MB download
        // before the first word.
        {
            // The override lock is not reentrant: this scope ends before the next.
            let (_dir, _guard) = speech_root();
            assert_eq!(get_dictation_config().speech_engine, "edge");
        }
        assert_eq!(engine_of_file("{}"), "edge");
    }

    #[test]
    fn an_existing_pocket_user_keeps_pocket_without_touching_the_settings() {
        // Catches: the upgrade silently switching someone who chose Pocket to a
        // cloud engine. Evidence of the choice: a voice, or the runtime on disk.
        assert_eq!(engine_of_file(r#"{"speech_voice":"giovanni"}"#), "pocket");
        let (_dir, _guard) = speech_root();
        install_asset(speech::assets::runtime());
        assert_eq!(get_dictation_config().speech_engine, "pocket");
    }

    #[test]
    fn an_existing_external_command_keeps_the_external_engine() {
        assert_eq!(
            engine_of_file(r#"{"speech_command":["piper","--output_file","{out}"]}"#),
            "external"
        );
    }

    #[test]
    fn an_explicit_engine_beats_what_the_installation_holds() {
        // Catches: the legacy rule overriding a choice made in the Expert
        // section (a fresh user who downloaded Pocket and stayed on Edge).
        assert_eq!(
            engine_of_file(r#"{"speech_engine":"edge","speech_voice":"giovanni"}"#),
            "edge"
        );
        let (_dir, _guard) = speech_root();
        install_asset(speech::assets::runtime());
        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                speech_engine: "edge".to_string(),
                ..get_dictation_config()
            },
            None,
        )
        .unwrap();
        assert_eq!(get_dictation_config().speech_engine, "edge");
    }

    #[test]
    fn edge_opens_a_voice_without_any_download() {
        // Catches: Edge still gated on Pocket assets ("is not downloaded").
        let (_dir, _guard) = speech_root();
        let library = speech::library::SpeechLibrary::new();
        let config = DictationConfig {
            speech_engine: "edge".to_string(),
            ..Default::default()
        };

        let (_engine, voice) = open_voice(&config, &library, "it").expect("edge opens");

        assert_eq!(voice, "it-IT-IsabellaNeural");
    }

    #[test]
    fn switching_engine_or_edge_voice_takes_the_voice_away_from_queued_replies() {
        // Catches: a reply queued for Pocket finishing in the Edge voice (or
        // the reverse) after the setting moved.
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");

        save_dictation_config(
            get_dictation_config(),
            DictationConfig {
                language: "it".to_string(),
                speech_engine: "edge".to_string(),
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(dictation.speaker.lock().is_none());
    }

    #[test]
    fn listen_speaks_a_voice_of_another_language_as_the_language_default_like_a_reply() {
        // Catches: Listen refusing a stored voice that the picker shows as the
        // default and that replies silently replace with the default.
        let (_dir, _guard) = speech_root();
        let library = speech::library::SpeechLibrary::new();
        let current = DictationConfig {
            speech_engine: "edge".to_string(),
            ..Default::default()
        };

        let config =
            preview_config(current, "it", "en-US-AriaNeural").expect("the preview is not refused");
        let (_engine, voice) = open_voice(&config, &library, "it").expect("edge opens");

        assert_eq!(voice, "it-IT-IsabellaNeural");
    }
}

/// Adversarial cases from the critic of 1357-7d37 (round 1).
#[cfg(test)]
mod critic_round1 {
    use super::*;

    #[test]
    fn a_command_beats_a_voice_and_an_installed_runtime_when_the_engine_is_unset() {
        // Catches: the legacy rule putting an external-command user on Pocket
        // because they also once chose a voice or downloaded the runtime.
        let config = DictationConfig {
            speech_command: vec!["piper".to_string()],
            speech_voice: "giovanni".to_string(),
            ..Default::default()
        };
        assert_eq!(legacy_speech_engine(&config, true), SpeechEngine::External);
    }

    #[test]
    fn nothing_chosen_and_nothing_installed_is_edge() {
        // Catches: an empty-string voice or empty command counting as a choice.
        let config = DictationConfig {
            speech_command: Vec::new(),
            speech_voice: String::new(),
            ..Default::default()
        };
        assert_eq!(legacy_speech_engine(&config, false), SpeechEngine::Edge);
    }

    #[test]
    fn an_unknown_engine_value_resolves_like_an_unset_one_for_the_whole_config() {
        // Catches: a hand-edited "Edge"/"pocket " silently ignoring the command
        // the user also configured (resolver and reader disagreeing).
        for value in ["EDGE", "pocket ", "garbage"] {
            let config = DictationConfig {
                speech_engine: value.to_string(),
                speech_command: vec!["piper".to_string()],
                ..Default::default()
            };
            let resolved = with_resolved_engine(config);
            assert_eq!(resolved.speech_engine, "external", "{value:?}");
            assert_eq!(speech_engine(&resolved), SpeechEngine::External);
        }
    }

    #[test]
    fn the_external_engine_with_no_command_is_a_setup_error_not_silence() {
        // Catches: Expert → External with an empty command speaking nothing and
        // reporting nothing (or falling through to Edge).
        let library = speech::library::SpeechLibrary::new();
        let config = DictationConfig {
            speech_engine: "external".to_string(),
            speech_command: Vec::new(),
            ..Default::default()
        };
        assert!(open_voice(&config, &library, "it").is_err());
    }

    #[test]
    fn an_edge_voice_that_is_not_a_plain_name_is_not_opened_for_a_reply() {
        // Catches: a hand-edited speech_edge_voice reaching the SSML attribute
        // (the adapter refuses it, but open_voice must hand it over unchanged,
        // not repair it into something else).
        let library = speech::library::SpeechLibrary::new();
        let config = DictationConfig {
            speech_engine: "edge".to_string(),
            speech_edge_voice: "it-IT-A' x='y".to_string(),
            ..Default::default()
        };
        let (engine, voice) = open_voice(&config, &library, "it").expect("opens");
        let result = engine.synthesize("ciao", &voice, &speech::SpeechCancel::new());
        assert!(
            matches!(result, Err(speech::SpeechError::UnknownVoice(_))),
            "{result:?}"
        );
    }

    #[test]
    fn listen_resolves_a_voice_exactly_as_a_reply_does() {
        // Catches: Listen and replies diverging again: a hostile or foreign
        // stored voice must preview as the language default, a multilingual one
        // must be kept.
        let library = speech::library::SpeechLibrary::new();
        let edge = || DictationConfig {
            speech_engine: "edge".to_string(),
            ..Default::default()
        };
        for (asked, expected) in [
            ("evil'><x", "it-IT-IsabellaNeural"),
            ("en-US-AriaNeural", "it-IT-IsabellaNeural"),
            ("", "it-IT-IsabellaNeural"),
            ("en-US-AvaMultilingualNeural", "en-US-AvaMultilingualNeural"),
        ] {
            let config = preview_config(edge(), "it", asked).expect("not refused");
            assert_eq!(config.speech_edge_voice, asked);
            let (_engine, voice) = open_voice(&config, &library, "it").expect("opens");
            assert_eq!(voice, expected, "{asked:?}");
        }
    }

    #[test]
    fn listen_has_nothing_to_preview_under_an_external_command() {
        // Catches: Listen running the user's command with a made-up voice.
        let config = DictationConfig {
            speech_engine: "external".to_string(),
            speech_command: vec!["piper".to_string()],
            ..Default::default()
        };
        assert!(preview_config(config, "it", "x").is_err());
    }
}
