use serde::Serialize;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Result of a transcription attempt with metadata for user feedback.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResult {
    /// The transcribed text (empty if skipped/filtered).
    pub text: String,
    /// Human-readable reason when text is empty (None when transcription succeeded).
    pub skip_reason: Option<String>,
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

        let ctx = WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
            .map_err(|e| format!("Failed to load Whisper model: {e}"))?;

        Ok(Self { ctx })
    }

    /// Transcribe audio samples (16kHz mono f32 PCM) to text.
    pub fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<TranscribeResult, String> {
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
        params.set_n_threads(4);
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
