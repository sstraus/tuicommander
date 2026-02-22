use super::{audio, corrections, model, transcribe, DictationState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

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
    let has_transcriber = dictation.transcriber.lock().is_some();

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
        recording: dictation.recording.load(Ordering::Relaxed),
        processing: dictation.processing.load(Ordering::Relaxed),
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
        *dictation.transcriber.lock() = None;
        *dictation.active_model.lock() = None;
    }

    model::delete_model(whisper_model)?;
    Ok(format!("Deleted {}", whisper_model.display_name()))
}

#[tauri::command]
pub fn start_dictation(
    dictation: State<'_, DictationState>,
) -> Result<(), String> {
    // Prevent concurrent recordings
    if dictation.recording.load(Ordering::Relaxed) {
        return Err("Already recording".to_string());
    }
    if dictation.processing.load(Ordering::Relaxed) {
        return Err("Transcription in progress".to_string());
    }

    let whisper_model = configured_model();

    // Reload transcriber if model changed or not loaded
    let mut transcriber_lock = dictation.transcriber.lock();
    let mut active_model_lock = dictation.active_model.lock();
    let model_changed = active_model_lock
        .as_deref()
        .map(|name| name != whisper_model.name())
        .unwrap_or(true);

    if model_changed || transcriber_lock.is_none() {
        if !model::model_exists(whisper_model) {
            return Err("Model not downloaded".to_string());
        }
        let t = transcribe::WhisperTranscriber::load(&model::model_path(whisper_model))?;
        *transcriber_lock = Some(t);
        *active_model_lock = Some(whisper_model.name().to_string());
    }
    drop(active_model_lock);
    drop(transcriber_lock);

    // Start audio capture
    let capture = audio::AudioCapture::start()?;
    *dictation.audio.lock() = Some(capture);
    dictation.recording.store(true, Ordering::Relaxed);

    Ok(())
}

#[tauri::command]
pub fn stop_dictation_and_transcribe(
    dictation: State<'_, DictationState>,
) -> Result<String, String> {
    if !dictation.recording.load(Ordering::Relaxed) {
        return Err("Not recording".to_string());
    }

    dictation.recording.store(false, Ordering::Relaxed);
    dictation.processing.store(true, Ordering::Relaxed);

    // Stop audio capture and get samples
    let capture = dictation
        .audio
        .lock()
        .take()
        .ok_or("No audio capture active")?;
    let audio_data = capture.stop();
    eprintln!("[dictation] Captured {} samples ({:.1}s)", audio_data.len(), audio_data.len() as f64 / 16000.0);

    // Transcribe
    let transcriber_lock = dictation.transcriber.lock();
    let transcriber = transcriber_lock
        .as_ref()
        .ok_or("Transcriber not loaded")?;

    // Pass configured language (None = auto-detect if "auto")
    let config = get_dictation_config();
    let lang = if config.language == "auto" { None } else { Some(config.language.as_str()) };
    eprintln!("[dictation] Language config: {:?}, using: {:?}", config.language, lang);
    let raw_text = transcriber.transcribe(&audio_data, lang)?;
    eprintln!("[dictation] Transcribed text: {:?}", raw_text);

    // Apply corrections
    let corrected = dictation.corrections.lock().correct(&raw_text);

    // Replace newlines with spaces to prevent accidental command execution
    let final_text = corrected.replace('\n', " ");
    eprintln!("[dictation] Final text to inject: {:?}", final_text);

    dictation.processing.store(false, Ordering::Relaxed);

    Ok(final_text)
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
}

fn default_model() -> String {
    "large-v3-turbo".to_string()
}

impl Default for DictationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: "F5".to_string(),
            language: "auto".to_string(),
            model: default_model(),
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
