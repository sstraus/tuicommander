pub mod audio;
pub mod commands;
pub mod corrections;
pub mod fn_key_monitor;
pub mod model;
pub mod permission;
pub mod streaming;
pub mod transcribe;
pub mod vad;

use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Shared dictation state accessible from Tauri commands.
/// Tauri's `.manage()` wraps this in `Arc` internally, so we don't double-wrap.
pub struct DictationState {
    pub audio: Mutex<Option<audio::AudioCapture>>,
    /// Name of the model currently loaded in `transcriber_arc` (e.g. "large-v3-turbo")
    pub active_model: Mutex<Option<String>>,
    pub corrections: Mutex<corrections::TextCorrector>,
    pub recording: AtomicBool,
    pub processing: AtomicBool,
    /// Active streaming session (None when not streaming).
    pub streaming: Mutex<Option<streaming::StreamingSession>>,
    /// Arc-wrapped transcriber for sharing with the streaming thread.
    pub transcriber_arc: Mutex<Option<Arc<dyn transcribe::Transcriber>>>,
    /// Concatenation of all streaming partials (for accuracy comparison logging).
    pub accumulated_partials: Arc<Mutex<String>>,
}

impl DictationState {
    pub fn new() -> Self {
        Self {
            audio: Mutex::new(None),
            active_model: Mutex::new(None),
            corrections: Mutex::new(corrections::TextCorrector::load_or_default()),
            recording: AtomicBool::new(false),
            processing: AtomicBool::new(false),
            streaming: Mutex::new(None),
            transcriber_arc: Mutex::new(None),
            accumulated_partials: Arc::new(Mutex::new(String::new())),
        }
    }
}
