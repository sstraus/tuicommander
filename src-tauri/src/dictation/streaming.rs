/// Streaming transcription engine.
///
/// A background thread drains audio from the shared buffer in adaptive sliding
/// windows (1.5s → 3s), runs VAD to skip silence, and feeds whisper-rs for
/// partial transcription results emitted via `mpsc::Sender<String>`.
///
/// Follows the `stream.cpp` pattern from whisper.cpp:
/// - Overlapping windows with `keep_ms` of previous context
/// - `set_single_segment(true)` + `set_no_timestamps(true)` for short windows

use crate::dictation::transcribe::Transcribe;
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
        transcriber: Arc<dyn Transcribe>,
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
    transcriber: Arc<dyn Transcribe>,
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    tx: mpsc::Sender<String>,
    stop: Arc<AtomicBool>,
    language: Option<String>,
) -> Vec<f32> {
    let mut step_buf: Vec<f32> = Vec::new();
    let mut prev_tail: Vec<f32> = Vec::new(); // keep_ms overlap from previous window
    let mut window_buf: Vec<f32> = Vec::new(); // reusable window buffer
    let mut current_step_ms = INITIAL_STEP_MS;
    let mut swap_buf: VecDeque<f32> = VecDeque::new(); // for O(1) lock swap

    let keep_samples = ms_to_samples(KEEP_MS);
    let max_buffer_samples = (MAX_BUFFER_S * SAMPLE_RATE as f32) as usize;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        // Swap shared buffer with empty local VecDeque (O(1) lock hold)
        {
            let mut buf = audio_buffer.lock();
            std::mem::swap(&mut *buf, &mut swap_buf);
        }
        if !swap_buf.is_empty() {
            step_buf.extend(swap_buf.drain(..));
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
                // Build window: [keep from previous | current step] — reuse buffer
                window_buf.clear();
                window_buf.extend_from_slice(&prev_tail);
                window_buf.extend_from_slice(&step_buf);

                // Transcribe the window
                if let Some(text) =
                    transcribe_window(&*transcriber, &window_buf, language.as_deref())
                {
                    if !text.is_empty() {
                        if tx.send(text).is_err() {
                            // Receiver dropped — stop streaming
                            eprintln!("[dictation] partial channel disconnected, stopping");
                            break;
                        }
                    }
                }

                // Save tail as overlap for next window — reuse buffer
                prev_tail.clear();
                if step_buf.len() > keep_samples {
                    prev_tail.extend_from_slice(&step_buf[step_buf.len() - keep_samples..]);
                } else {
                    prev_tail.extend_from_slice(&step_buf);
                }

                // Grow window toward MAX_STEP_MS during speech
                if current_step_ms < MAX_STEP_MS {
                    current_step_ms = (current_step_ms + STEP_GROWTH_MS).min(MAX_STEP_MS);
                }
            } else {
                // Reset window size on silence for low-latency first partial
                current_step_ms = INITIAL_STEP_MS;
                prev_tail.clear();
            }

            // Clear step buffer after processing (whether speech or silence)
            step_buf.clear();
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
    transcriber: &dyn Transcribe,
    window: &[f32],
    language: Option<&str>,
) -> Option<String> {
    match transcriber.transcribe(window, language) {
        Ok(result) => {
            if result.skip_reason.is_some() {
                None
            } else {
                eprintln!("[dictation] partial: {} chars", result.text.len());
                Some(result.text)
            }
        }
        Err(e) => {
            eprintln!("[dictation] transcribe_window error: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::transcribe::{Transcribe, TranscribeResult};

    /// Test transcriber that echoes the sample count as text.
    struct EchoTranscriber;

    impl Transcribe for EchoTranscriber {
        fn transcribe(
            &self,
            audio: &[f32],
            _language: Option<&str>,
        ) -> Result<TranscribeResult, String> {
            Ok(TranscribeResult {
                text: format!("samples:{}", audio.len()),
                skip_reason: None,
            })
        }
    }

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
    fn test_streaming_session_with_echo_transcriber() {
        use std::f32::consts::PI;

        let transcriber: Arc<dyn Transcribe> = Arc::new(EchoTranscriber);
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, rx) = mpsc::channel::<String>();

        let session = StreamingSession::start(
            transcriber,
            buffer.clone(),
            tx,
            None,
        );

        // Push 2s of speech-like audio (440Hz sine wave) — exceeds INITIAL_STEP_MS (1.5s)
        let n = ms_to_samples(2000);
        let speech: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.5)
            .collect();
        {
            let mut buf = buffer.lock();
            buf.extend(speech.iter());
        }

        // Wait for the streaming loop to process (poll interval is 50ms)
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Stop the session
        let remaining = session.stop();

        // Should have received at least one partial transcription
        let mut partials = Vec::new();
        while let Ok(text) = rx.try_recv() {
            partials.push(text);
        }
        assert!(
            !partials.is_empty(),
            "Should have received at least one partial from EchoTranscriber"
        );
        // Each partial should report the sample count
        for partial in &partials {
            assert!(
                partial.starts_with("samples:"),
                "Expected 'samples:N', got '{partial}'"
            );
        }
    }

    #[test]
    fn test_streaming_loop_skips_silence() {
        let transcriber: Arc<dyn Transcribe> = Arc::new(EchoTranscriber);
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, rx) = mpsc::channel::<String>();

        let session = StreamingSession::start(
            transcriber,
            buffer.clone(),
            tx,
            None,
        );

        // Push 2s of silence — VAD should skip it, no transcription
        let n = ms_to_samples(2000);
        {
            let mut buf = buffer.lock();
            buf.extend(std::iter::repeat(0.0f32).take(n));
        }

        std::thread::sleep(std::time::Duration::from_millis(300));
        session.stop();

        // Should NOT have received any partials (silence is skipped)
        assert!(
            rx.try_recv().is_err(),
            "Should not transcribe silence"
        );
    }

    #[test]
    fn test_streaming_session_stop_terminates() {
        let transcriber: Arc<dyn Transcribe> = Arc::new(EchoTranscriber);
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, _rx) = mpsc::channel::<String>();

        let session = StreamingSession::start(
            transcriber,
            buffer.clone(),
            tx,
            None,
        );

        // Let it run briefly with no audio
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Stop should return promptly (no hang)
        let leftover = session.stop();
        assert!(leftover.is_empty(), "No audio was pushed, so no remainder");
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
