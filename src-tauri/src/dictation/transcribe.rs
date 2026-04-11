use serde::Serialize;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// True when a GPU backend is compiled into this build.
/// - macOS: Metal (always, via target-specific dep).
/// - Windows: Vulkan (always, via target-specific dep).
/// - Linux: when `cuda` or `vulkan` feature is explicitly enabled.
/// Used only for `n_threads` tuning — `use_gpu(true)` is always set regardless,
/// because whisper.cpp falls back to CPU gracefully when no GPU is available.
pub(super) const GPU_COMPILED: bool = cfg!(any(
    target_os = "macos",
    target_os = "windows",
    feature = "cuda",
    feature = "vulkan",
));

/// Optimal n_threads: 1 when a GPU backend is compiled in (GPU handles compute), 4 otherwise.
pub(super) fn optimal_n_threads() -> i32 {
    if GPU_COMPILED { 1 } else { 4 }
}

/// Backend label for logging and frontend events.
/// Always "gpu" because `use_gpu(true)` is set unconditionally — whisper.cpp
/// attempts GPU first and falls back to CPU transparently.
pub(super) fn backend_label() -> &'static str {
    "gpu"
}

/// Build WhisperContextParameters with GPU always preferred.
/// whisper.cpp attempts GPU first and falls back to CPU gracefully if unavailable.
pub(super) fn build_context_params() -> WhisperContextParameters<'static> {
    let mut params = WhisperContextParameters::new();
    params.use_gpu(true);
    params
}

/// Result of a transcription attempt with metadata for user feedback.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResult {
    /// The transcribed text (empty if skipped/filtered).
    pub text: String,
    /// Human-readable reason when text is empty (None when transcription succeeded).
    pub skip_reason: Option<String>,
}

/// Trait for transcription, enabling mock implementations in tests.
pub trait Transcriber: Send + Sync {
    fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<TranscribeResult, String>;
}

/// Whisper model wrapper for transcription.
pub struct WhisperTranscriber {
    ctx: WhisperContext,
}

impl WhisperTranscriber {
    /// Load a Whisper GGML model from disk.
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let path_str = model_path
            .to_str()
            .ok_or("Invalid model path (non-UTF8)")?;

        let params = build_context_params();
        let backend = backend_label();
        tracing::info!(backend, n_threads = optimal_n_threads(), "Loading Whisper model");

        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| format!("Failed to load Whisper model: {e}"))?;

        tracing::info!(backend, "Whisper model loaded");
        Ok(Self { ctx })
    }
}

impl Transcriber for WhisperTranscriber {
    /// Transcribe audio samples (16kHz mono f32 PCM) to text.
    fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<TranscribeResult, String> {
        if audio.is_empty() {
            return Ok(TranscribeResult {
                text: String::new(),
                skip_reason: Some("no audio captured".to_string()),
            });
        }

        let duration_s = audio.len() as f64 / 16000.0;

        // Minimum 0.5s of audio (8000 samples at 16kHz)
        if audio.len() < 8000 {
            return Ok(TranscribeResult {
                text: String::new(),
                skip_reason: Some(format!("too short ({duration_s:.1}s, need 0.5s)")),
            });
        }

        // Reject silent/near-silent audio to prevent hallucinations.
        // Whisper hallucinates phrases like "Thank you" on silence.
        let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt();
        if rms < 0.001 {
            return Ok(TranscribeResult {
                text: String::new(),
                skip_reason: Some(format!("no speech detected (RMS {rms:.6} < 0.001)")),
            });
        }

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("Failed to create Whisper state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_n_threads(optimal_n_threads());
        // Suppress non-speech tokens for cleaner output
        params.set_suppress_nst(true);
        // Disable timestamp computation — primary fix for hallucination on silence
        // See: https://github.com/ggml-org/whisper.cpp/issues/1724
        params.set_no_timestamps(true);
        // Force single segment for short dictation recordings
        params.set_single_segment(true);

        state
            .full(params, audio)
            .map_err(|e| format!("Transcription failed: {e}"))?;

        let n_segments = state.full_n_segments();
        let mut text = String::new();

        for i in 0..n_segments {
            if let Some(segment) = state.get_segment(i)
                && let Ok(s) = segment.to_str() {
                    text.push_str(s);
                }
        }

        let result = text.trim().to_string();

        // Filter known hallucination phrases that Whisper produces on near-silence
        if is_hallucination(&result) {
            return Ok(TranscribeResult {
                text: String::new(),
                skip_reason: Some(format!("filtered hallucination: \"{result}\"")),
            });
        }

        Ok(TranscribeResult {
            text: result,
            skip_reason: None,
        })
    }
}

/// Known hallucination phrases Whisper produces on silence/noise.
/// These are artifacts of YouTube subtitle training data.
fn is_hallucination(text: &str) -> bool {
    let lower = text.to_lowercase();
    const HALLUCINATIONS: &[&str] = &[
        "thank you",
        "thanks for watching",
        "thanks for listening",
        "subtitles by",
        "transcribed by",
        "subscribe",
        "like and subscribe",
    ];
    HALLUCINATIONS.iter().any(|h| lower.contains(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimal_n_threads_consistent_with_gpu_compiled() {
        let threads = optimal_n_threads();
        if GPU_COMPILED {
            assert_eq!(threads, 1, "GPU compiled: n_threads should be 1");
        } else {
            assert_eq!(threads, 4, "CPU only: n_threads should be 4");
        }
    }

    #[test]
    fn backend_label_always_gpu() {
        assert_eq!(backend_label(), "gpu");
    }

    #[test]
    fn build_context_params_does_not_panic() {
        let _params = build_context_params();
    }
}
