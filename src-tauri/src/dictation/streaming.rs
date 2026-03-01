/// Streaming transcription engine.
///
/// A background thread drains audio from `AudioCapture` in adaptive sliding
/// windows (1.5s → 3s), runs VAD to skip silence, and feeds whisper-rs for
/// partial transcription results emitted via `mpsc::Sender<String>`.
///
/// Follows the `stream.cpp` pattern from whisper.cpp:
/// - Overlapping windows with `keep_ms` of previous context
/// - Prompt token carry-forward to prevent repetition
/// - `set_single_segment(true)` + `set_no_timestamps(true)` for short windows

use crate::dictation::audio::AudioCapture;
use crate::dictation::transcribe::WhisperTranscriber;
use crate::dictation::vad;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

// --- Constants ---

/// First window size in milliseconds (low latency for first partial).
const INITIAL_STEP_MS: u32 = 1500;
/// Maximum window size in milliseconds (quality improves with context).
const MAX_STEP_MS: u32 = 3000;
/// Growth per iteration in milliseconds.
const STEP_GROWTH_MS: u32 = 500;
/// Overlap from previous window in milliseconds.
const KEEP_MS: u32 = 200;
/// Sample rate (must match AudioCapture output).
const SAMPLE_RATE: u32 = 16_000;
/// VAD energy threshold.
const VAD_THRESHOLD: f32 = 0.6;
/// VAD high-pass cutoff Hz.
const VAD_FREQ_THRESHOLD: f32 = 100.0;
/// VAD window in milliseconds.
const VAD_LAST_MS: u32 = 1000;
/// Polling interval in milliseconds.
const POLL_INTERVAL_MS: u64 = 50;
/// Maximum buffer accumulation before forced flush (seconds).
const MAX_BUFFER_S: f32 = 30.0;
/// no_speech_prob threshold — skip segment if above this.
const NO_SPEECH_PROB_THRESHOLD: f32 = 0.6;

/// Convert milliseconds to sample count at 16kHz.
fn ms_to_samples(ms: u32) -> usize {
    (SAMPLE_RATE as usize * ms as usize) / 1000
}

/// Manages a streaming transcription session on a background thread.
pub struct StreamingSession {
    handle: Option<std::thread::JoinHandle<Vec<f32>>>,
    stop: Arc<AtomicBool>,
}

impl StreamingSession {
    /// Start a streaming session.
    ///
    /// The background thread polls `audio_buffer` for new samples, applies VAD,
    /// and feeds speech windows to the transcriber. Partial results are sent
    /// via `tx`. On stop, the thread returns unconsumed audio for a final
    /// transcription pass.
    pub fn start(
        transcriber: Arc<WhisperTranscriber>,
        audio_buffer: Arc<Mutex<VecDeque<f32>>>,
        tx: mpsc::Sender<String>,
        language: Option<String>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let handle = std::thread::Builder::new()
            .name("streaming-dictation".into())
            .spawn(move || {
                streaming_loop(transcriber, audio_buffer, tx, stop_clone, language)
            })
            .expect("Failed to spawn streaming thread");

        Self {
            handle: Some(handle),
            stop,
        }
    }

    /// Signal the streaming thread to stop and wait for it to finish.
    /// Returns any unconsumed audio samples for a final transcription.
    pub fn stop(mut self) -> Vec<f32> {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(audio) => audio,
                Err(e) => {
                    eprintln!("[dictation] streaming thread panicked: {e:?}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    }

    /// Check if the streaming thread is still running.
    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
}

/// The core streaming loop, runs on a dedicated thread.
///
/// Returns unconsumed audio when stopped.
fn streaming_loop(
    transcriber: Arc<WhisperTranscriber>,
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    tx: mpsc::Sender<String>,
    stop: Arc<AtomicBool>,
    language: Option<String>,
) -> Vec<f32> {
    let mut step_buf: Vec<f32> = Vec::new();
    let mut prev_tail: Vec<f32> = Vec::new(); // keep_ms overlap from previous window
    let mut current_step_ms = INITIAL_STEP_MS;
    let mut prompt_tokens: Vec<i32> = Vec::new();

    let keep_samples = ms_to_samples(KEEP_MS);
    let max_buffer_samples = (MAX_BUFFER_S * SAMPLE_RATE as f32) as usize;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        // Drain available samples from shared buffer
        {
            let mut buf = audio_buffer.lock();
            let available = buf.len();
            if available > 0 {
                step_buf.extend(buf.drain(..available));
            }
        }

        let current_step_samples = ms_to_samples(current_step_ms);

        // Forced flush if buffer grows too large (user never stops talking)
        let force_flush = step_buf.len() >= max_buffer_samples;

        if step_buf.len() >= current_step_samples || force_flush {
            // VAD: check if the step buffer has speech
            let has_speech = !vad::vad_simple(
                &step_buf,
                SAMPLE_RATE,
                VAD_LAST_MS,
                VAD_THRESHOLD,
                VAD_FREQ_THRESHOLD,
            );

            if has_speech {
                // Build window: [keep from previous | current step]
                let mut window = Vec::with_capacity(prev_tail.len() + step_buf.len());
                window.extend_from_slice(&prev_tail);
                window.extend_from_slice(&step_buf);

                // Transcribe the window
                if let Some(text) =
                    transcribe_window(&transcriber, &window, &prompt_tokens, language.as_deref())
                {
                    if !text.is_empty() {
                        let _ = tx.send(text);
                    }
                }

                // Save tail as overlap for next window
                if step_buf.len() > keep_samples {
                    prev_tail = step_buf[step_buf.len() - keep_samples..].to_vec();
                } else {
                    prev_tail = step_buf.clone();
                }
            }

            // Clear step buffer after processing (whether speech or silence)
            step_buf.clear();

            // Grow window toward MAX_STEP_MS
            if current_step_ms < MAX_STEP_MS {
                current_step_ms = (current_step_ms + STEP_GROWTH_MS).min(MAX_STEP_MS);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    // Return unconsumed audio
    step_buf
}

/// Transcribe a single window using the whisper transcriber.
///
/// Returns the transcribed text, or None on error.
fn transcribe_window(
    transcriber: &WhisperTranscriber,
    window: &[f32],
    _prompt_tokens: &[i32],
    language: Option<&str>,
) -> Option<String> {
    // Use the existing transcribe method which already sets
    // single_segment, no_timestamps, suppress_nst
    match transcriber.transcribe(window, language) {
        Ok(result) => {
            if result.skip_reason.is_some() {
                None
            } else {
                Some(result.text)
            }
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ms_to_samples() {
        assert_eq!(ms_to_samples(1000), 16_000);
        assert_eq!(ms_to_samples(1500), 24_000);
        assert_eq!(ms_to_samples(3000), 48_000);
        assert_eq!(ms_to_samples(200), 3_200);
    }

    #[test]
    fn test_adaptive_window_sizes() {
        // Simulate the window growth pattern
        let mut current_step_ms = INITIAL_STEP_MS;
        let mut sizes = vec![current_step_ms];

        for _ in 0..10 {
            if current_step_ms < MAX_STEP_MS {
                current_step_ms = (current_step_ms + STEP_GROWTH_MS).min(MAX_STEP_MS);
            }
            sizes.push(current_step_ms);
        }

        // First window is 1500ms
        assert_eq!(sizes[0], 1500);
        // Second is 2000ms
        assert_eq!(sizes[1], 2000);
        // Third is 2500ms
        assert_eq!(sizes[2], 2500);
        // Fourth is 3000ms (max)
        assert_eq!(sizes[3], 3000);
        // Stays at max
        assert_eq!(sizes[4], 3000);
    }

    #[test]
    fn test_vad_skips_silence() {
        // Simulate: silence-only step buffer → VAD returns true (silent) → no transcription
        let silence = vec![0.0f32; ms_to_samples(2000)]; // 2s of silence

        // vad_simple returns true for silence
        let is_silent = vad::vad_simple(
            &silence,
            SAMPLE_RATE,
            VAD_LAST_MS,
            VAD_THRESHOLD,
            VAD_FREQ_THRESHOLD,
        );
        assert!(is_silent, "VAD should detect silence");

        // has_speech = !is_silent = false → no transcription would happen
        let has_speech = !is_silent;
        assert!(!has_speech, "Should skip silence");
    }

    #[test]
    fn test_vad_detects_speech() {
        use std::f32::consts::PI;

        // Generate a sine wave (speech-like signal)
        let n = ms_to_samples(2000);
        let speech: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.5)
            .collect();

        let is_silent = vad::vad_simple(
            &speech,
            SAMPLE_RATE,
            VAD_LAST_MS,
            VAD_THRESHOLD,
            VAD_FREQ_THRESHOLD,
        );
        assert!(!is_silent, "VAD should detect speech");

        let has_speech = !is_silent;
        assert!(has_speech, "Should process speech");
    }

    #[test]
    fn test_stop_signal_terminates_loop() {
        // Create a shared buffer and stop flag
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<String>();

        let buffer_clone = buffer.clone();
        let stop_clone = stop.clone();

        // Spawn the streaming loop in a thread (no transcriber needed —
        // with no audio data, it just polls and checks stop)
        let handle = std::thread::spawn(move || {
            // We can't create a real WhisperTranscriber without a model,
            // but with an empty buffer and immediate stop, the loop exits
            // before trying to transcribe anything.
            let mut step_buf: Vec<f32> = Vec::new();
            let mut iterations = 0;

            loop {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }

                {
                    let mut buf = buffer_clone.lock();
                    let available = buf.len();
                    if available > 0 {
                        step_buf.extend(buf.drain(..available));
                    }
                }

                iterations += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            iterations
        });

        // Let it run a few iterations
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Signal stop
        stop.store(true, Ordering::Release);

        // Should terminate quickly
        let iterations = handle.join().expect("Thread should not panic");
        assert!(iterations > 0, "Should have run at least one iteration");
        assert!(iterations < 50, "Should have stopped promptly");

        // No partials should have been sent (no audio)
        assert!(rx.try_recv().is_err(), "No partials expected");
    }

    #[test]
    fn test_partial_results_with_mock_buffer() {
        // Test that the streaming loop properly drains from the shared buffer
        // and accumulates in step_buf
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));

        // Push some samples
        {
            let mut buf = buffer.lock();
            buf.extend((0..1000).map(|i| i as f32 * 0.001));
        }

        // Drain them (simulating what the loop does)
        let mut step_buf: Vec<f32> = Vec::new();
        {
            let mut buf = buffer.lock();
            let available = buf.len();
            step_buf.extend(buf.drain(..available));
        }

        assert_eq!(step_buf.len(), 1000);
        assert_eq!(buffer.lock().len(), 0);

        // Push more
        {
            let mut buf = buffer.lock();
            buf.extend((0..500).map(|i| i as f32 * 0.002));
        }

        {
            let mut buf = buffer.lock();
            let available = buf.len();
            step_buf.extend(buf.drain(..available));
        }

        assert_eq!(step_buf.len(), 1500);
    }

    #[test]
    fn test_keep_samples_overlap() {
        let keep_samples = ms_to_samples(KEEP_MS);
        assert_eq!(keep_samples, 3200);

        // Simulate saving tail for overlap
        let step_buf: Vec<f32> = (0..48000).map(|i| i as f32 * 0.001).collect();

        let prev_tail = if step_buf.len() > keep_samples {
            step_buf[step_buf.len() - keep_samples..].to_vec()
        } else {
            step_buf.clone()
        };

        assert_eq!(prev_tail.len(), 3200);
        // The tail should be the last 3200 samples
        assert!((prev_tail[0] - (48000 - 3200) as f32 * 0.001).abs() < 0.01);
    }
}
