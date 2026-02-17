pub mod audio;
pub mod commands;
pub mod corrections;
pub mod model;
pub mod transcribe;

use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;

/// Shared dictation state accessible from Tauri commands.
/// Tauri's `.manage()` wraps this in `Arc` internally, so we don't double-wrap.
pub struct DictationState {
    pub audio: Mutex<Option<audio::AudioCapture>>,
    pub transcriber: Mutex<Option<transcribe::WhisperTranscriber>>,
    /// Name of the model currently loaded in `transcriber` (e.g. "large-v3-turbo")
    pub active_model: Mutex<Option<String>>,
    pub corrections: Mutex<corrections::TextCorrector>,
    pub recording: AtomicBool,
    pub processing: AtomicBool,
}

impl DictationState {
    pub fn new() -> Self {
        Self {
            audio: Mutex::new(None),
            transcriber: Mutex::new(None),
            active_model: Mutex::new(None),
            corrections: Mutex::new(corrections::TextCorrector::load_or_default()),
            recording: AtomicBool::new(false),
            processing: AtomicBool::new(false),
        }
    }
}
