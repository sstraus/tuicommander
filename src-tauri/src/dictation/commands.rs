use super::{audio, corrections, model, permission, streaming, transcribe, DictationState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};

/// Helper to reset recording flag on error paths.
struct RecordingGuard<'a> {
    recording: &'a std::sync::atomic::AtomicBool,
    disarmed: bool,
}

impl<'a> RecordingGuard<'a> {
    fn new(recording: &'a std::sync::atomic::AtomicBool) -> Self {
        Self { recording, disarmed: false }
    }
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for RecordingGuard<'_> {
    fn drop(&mut self) {
        if !self.disarmed {
            self.recording.store(false, Ordering::Release);
        }
    }
}

/// RAII guard that resets the processing flag to false on drop (including panic).
/// Holds an `Arc<AtomicBool>` so it can be moved into `spawn_blocking`.
struct ProcessingGuard(Arc<std::sync::atomic::AtomicBool>);

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_logger;

#[derive(Debug, Clone, Serialize)]
pub struct DictationStatus {
    pub model_status: String, // "not_downloaded", "ready", "error"
    pub model_name: String,
    pub model_size_mb: u64,
    pub recording: bool,
    pub processing: bool,
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
    /// Duration of captured audio in seconds.
    pub duration_s: f64,
}

/// Resolve the configured model from persisted config.
fn configured_model() -> model::WhisperModel {
    let config = get_dictation_config();
    model::WhisperModel::from_name(&config.model)
        .unwrap_or(model::WhisperModel::LargeV3Turbo)
}

#[tauri::command]
pub fn get_dictation_status(
    dictation: State<'_, DictationState>,
) -> Result<DictationStatus, String> {
    let whisper_model = configured_model();
    let model_downloaded = model::model_exists(whisper_model);
    let has_transcriber = dictation.transcriber_arc.lock().is_some();

    let model_status = if !model_downloaded {
        "not_downloaded"
    } else if has_transcriber {
        "ready"
    } else {
        "downloaded" // Downloaded but not loaded yet
    };

    Ok(DictationStatus {
        model_status: model_status.to_string(),
        model_name: whisper_model.name().to_string(),
        model_size_mb: model::model_size_bytes(whisper_model) / 1_048_576,
        recording: dictation.recording.load(Ordering::Acquire),
        processing: dictation.processing.load(Ordering::Acquire),
    })
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
pub async fn download_whisper_model(
    app: AppHandle,
    model_name: String,
) -> Result<String, String> {
    let whisper_model = model::WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Unknown model: {model_name}"))?;

    if model::model_exists(whisper_model) {
        return Ok("Model already downloaded".to_string());
    }

    let app_clone = app.clone();
    let path = model::download_model(whisper_model, move |downloaded, total| {
        let _ = app_clone.emit(
            "dictation-download-progress",
            serde_json::json!({
                "downloaded": downloaded,
                "total": total,
                "percent": if total > 0 { (downloaded as f64 / total as f64 * 100.0) as u32 } else { 0 },
            }),
        );
    })
    .await?;

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
    Ok(format!("Deleted {}", whisper_model.display_name()))
}

#[tauri::command]
pub fn start_dictation(
    app: AppHandle,
    dictation: State<'_, DictationState>,
) -> Result<(), String> {
    // Atomic test-and-set: prevents TOCTOU race from concurrent IPC calls
    if dictation.recording
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Already recording".to_string());
    }
    // Guard resets recording=false if we return early on any error path
    let mut recording_guard = RecordingGuard::new(&dictation.recording);

    if dictation.processing.load(Ordering::Acquire) {
        return Err("Transcription in progress".to_string());
    }

    // Check microphone permission before attempting audio capture
    let mic_status = permission::check();
    match mic_status {
        permission::MicPermission::Denied => {
            return Err("microphone_denied".to_string());
        }
        permission::MicPermission::Restricted => {
            return Err("microphone_restricted".to_string());
        }
        permission::MicPermission::NotDetermined => {
            // CoreAudio (cpal) does NOT trigger the TCC prompt — we must
            // explicitly request access via AVCaptureDevice to show the dialog.
            if !permission::request() {
                return Err("microphone_denied".to_string());
            }
        }
        permission::MicPermission::Authorized => {}
    }

    let whisper_model = configured_model();

    // Reload transcriber if model changed or not loaded
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
        app_logger::log_via_handle(&app, "info", "dictation", &format!("Loading model: {}", whisper_model.display_name()));
        let t = transcribe::WhisperTranscriber::load(&model::model_path(whisper_model))?;
        *transcriber_arc_lock = Some(Arc::new(t));
        *active_model_lock = Some(whisper_model.name().to_string());
        let backend = if transcribe::gpu_enabled() { "gpu" } else { "cpu" };
        app_logger::log_via_handle(&app, "info", "dictation", &format!("Model loaded (backend: {backend})"));
        let _ = app.emit("dictation-backend-info", serde_json::json!({
            "backend": backend,
        }));
    }

    let transcriber_arc = transcriber_arc_lock.clone()
        .ok_or("Transcriber not available")?;
    drop(active_model_lock);
    drop(transcriber_arc_lock);

    // Start audio capture using the configured device (or system default)
    let config = get_dictation_config();
    let device_name = config.device.as_deref().filter(|s| !s.is_empty());
    let capture = audio::AudioCapture::start_with_device(device_name).map_err(|e| {
        app_logger::log_via_handle(&app, "error", "dictation", &format!("Audio capture failed: {e}"));
        // If a specific device failed, hint the user
        if device_name.is_some() {
            app_logger::log_via_handle(&app, "warn", "dictation",
                "Configured device not available — check Settings > Dictation > Microphone");
        }
        e
    })?;

    // Get audio buffer handle for streaming thread
    let audio_buffer = capture.buffer_handle();
    *dictation.audio.lock() = Some(capture);

    // Start streaming session
    let config = get_dictation_config();
    let lang = if config.language == "auto" { None } else { Some(config.language.clone()) };
    let (tx, rx) = mpsc::channel::<String>();

    let session = streaming::StreamingSession::start(
        transcriber_arc as Arc<dyn transcribe::Transcriber>,
        audio_buffer,
        tx,
        lang,
    );
    *dictation.streaming.lock() = Some(session);

    // recording is already true (set by compare_exchange above)
    app_logger::log_via_handle(&app, "info", "dictation", "Streaming recording started");

    // Reset accumulated partials for this session
    dictation.accumulated_partials.lock().clear();

    // Spawn event forwarder: reads partials from channel, emits Tauri events,
    // and concatenates them for accuracy comparison at the end.
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
pub async fn stop_dictation_and_transcribe(
    app: AppHandle,
) -> Result<TranscribeResponse, String> {
    // Gather all data from DictationState synchronously (before any .await).
    // This block ensures no MutexGuard or State borrow lives across the await point.
    let prepare = {
        let dictation = app.state::<DictationState>();

        if !dictation.recording.load(Ordering::Acquire) {
            return Err("Not recording".to_string());
        }

        // Set recording=false synchronously so the UI updates immediately
        dictation.recording.store(false, Ordering::Release);
        dictation.processing.store(true, Ordering::Release);

        // Stop audio capture (stops the cpal stream, but buffer data remains)
        let mut capture_lock = dictation.audio.lock();
        if let Some(ref mut capture) = *capture_lock {
            capture.stop_stream();
        }

        // Take the streaming session (cheap — no join yet) and the audio buffer handle.
        // The actual thread join happens in spawn_blocking to avoid blocking the tokio worker.
        let session = dictation.streaming.lock().take();
        let audio_buffer = capture_lock.as_ref().map(|c| c.buffer_handle());
        drop(capture_lock);

        // Read config while we still have sync context (avoids file I/O after .await)
        let config = get_dictation_config();
        let lang_owned = if config.language == "auto" { None } else { Some(config.language.clone()) };

        // Clone Arc-ed resources for the blocking task
        let transcriber = dictation.transcriber_arc.lock().clone();
        let accumulated_partials = dictation.accumulated_partials.clone();
        let corrections = dictation.corrections.clone();
        let processing = dictation.processing.clone();

        Some((session, audio_buffer, lang_owned, transcriber, accumulated_partials, corrections, processing))
    };

    let (session, audio_buffer, lang_owned, transcriber, accumulated_partials, corrections, processing) =
        prepare.unwrap(); // always Some — the None path returns Err above

    let app_clone = app.clone();

    // Run session join + whisper inference off the IPC thread
    let result = tokio::task::spawn_blocking(move || {
        let _guard = ProcessingGuard(processing);

        // Join the streaming thread (may block while last partial window finishes)
        let mut all_audio = session.map(|s| s.stop()).unwrap_or_default();

        // Drain anything left in the audio capture buffer (arrived after last poll).
        // Safe: streaming thread is joined above, no more concurrent readers.
        if let Some(buf) = audio_buffer {
            let remaining: Vec<f32> = buf.lock().drain(..).collect();
            all_audio.extend(remaining);
        }

        let total_duration_s = all_audio.len() as f64 / 16000.0;
        app_logger::log_via_handle(&app_clone, "info", "dictation",
            &format!("Streaming stopped, {:.1}s total audio for final transcription", total_duration_s));

        // Short audio: no transcription needed
        if all_audio.len() < 8000 {
            app_logger::log_via_handle(&app_clone, "info", "dictation", "No speech detected");
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("no speech detected".to_string()),
                duration_s: total_duration_s,
            };
        }

        let mut final_text = String::new();

        if let Some(ref transcriber) = transcriber {
            let lang_ref = lang_owned.as_deref();
            match transcriber.transcribe(&all_audio, lang_ref) {
                Ok(result) if result.skip_reason.is_none() => {
                    final_text = result.text;
                }
                Ok(result) => {
                    if let Some(reason) = &result.skip_reason {
                        app_logger::log_via_handle(&app_clone, "info", "dictation",
                            &format!("Final transcription skipped: {reason}"));
                    }
                }
                Err(e) => {
                    app_logger::log_via_handle(&app_clone, "warn", "dictation",
                        &format!("Final transcription failed: {e}"));
                }
            }
        } else {
            app_logger::log_via_handle(&app_clone, "warn", "dictation",
                "Transcriber not available — model not loaded");
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("model not loaded".to_string()),
                duration_s: total_duration_s,
            };
        }

        if final_text.is_empty() {
            app_logger::log_via_handle(&app_clone, "info", "dictation", "No speech detected");
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("no speech detected".to_string()),
                duration_s: total_duration_s,
            };
        }

        // Log accuracy comparison (lengths only — no verbatim text to avoid PII in logs)
        let composed = std::mem::take(&mut *accumulated_partials.lock());
        let match_pct = if !composed.is_empty() && !final_text.is_empty() {
            let common = final_text.chars().zip(composed.chars())
                .take_while(|(a, b)| a == b).count();
            let max_len = final_text.len().max(composed.len());
            (common as f64 / max_len as f64 * 100.0).round() as u32
        } else {
            0
        };
        app_logger::log_via_handle(&app_clone, "info", "dictation",
            &format!("[accuracy] full={} chars, composed={} chars, match={}%, audio={:.1}s",
                final_text.len(), composed.len(), match_pct, total_duration_s));

        // Apply corrections
        let corrected = corrections.lock().correct(&final_text);
        let final_text = corrected.replace('\n', " ");

        // _guard drops here → processing = false
        TranscribeResponse {
            text: final_text,
            skip_reason: None,
            duration_s: total_duration_s,
        }
    }).await.map_err(|e| {
        let msg = format!("Transcription task panicked: {e}");
        app_logger::log_via_handle(&app, "error", "dictation", &msg);
        msg
    })?;

    // Clean up audio capture
    *app.state::<DictationState>().audio.lock() = None;

    Ok(result)
}

#[tauri::command]
pub fn get_correction_map(
    dictation: State<'_, DictationState>,
) -> HashMap<String, String> {
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

#[tauri::command]
pub fn list_audio_devices() -> Vec<audio::AudioDevice> {
    audio::list_input_devices()
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
pub fn inject_text(
    dictation: State<'_, DictationState>,
    text: String,
) -> Result<String, String> {
    // Apply corrections before injection
    let corrected = dictation.corrections.lock().correct(&text);
    let final_text = corrected.replace('\n', " ");
    Ok(final_text)
}

/// Dictation configuration persisted to <config_dir>/dictation-config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationConfig {
    pub enabled: bool,
    pub hotkey: String,
    pub language: String,
    /// Selected whisper model name (e.g. "large-v3-turbo", "small")
    #[serde(default = "default_model")]
    pub model: String,
    /// Selected audio input device name. None or empty = system default.
    #[serde(default)]
    pub device: Option<String>,
    /// Long-press threshold in milliseconds for push-to-talk activation.
    /// A short press (below this duration) passes through as normal input.
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u32,
    /// Automatically send (press Enter) after injecting transcribed text.
    #[serde(default)]
    pub auto_send: bool,
}

fn default_model() -> String {
    "large-v3-turbo".to_string()
}

fn default_long_press_ms() -> u32 {
    400
}

impl Default for DictationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: "F5".to_string(),
            language: "auto".to_string(),
            model: default_model(),
            device: None,
            long_press_ms: default_long_press_ms(),
            auto_send: false,
        }
    }
}

const DICTATION_CONFIG_FILE: &str = "dictation-config.json";

#[tauri::command]
pub fn get_dictation_config() -> DictationConfig {
    crate::config::load_json_config(DICTATION_CONFIG_FILE)
}

#[tauri::command]
pub fn set_dictation_config(config: DictationConfig) -> Result<(), String> {
    crate::config::save_json_config(DICTATION_CONFIG_FILE, &config)
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
