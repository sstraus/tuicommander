/// Streaming transcription engine.
///
/// A background thread drains audio from the shared buffer in adaptive sliding
/// windows (1.5s → 3s), runs VAD to skip silence, and feeds whisper-rs for
/// partial transcription results emitted via `mpsc::Sender<String>`.
///
/// Follows the `stream.cpp` pattern from whisper.cpp:
/// - Overlapping windows with `keep_ms` of previous context
/// - `set_single_segment(true)` + `set_no_timestamps(true)` for short windows
use crate::dictation::transcribe::Transcriber;
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
        transcriber: Arc<dyn Transcriber>,
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
                    tracing::error!(source = "dictation", "Streaming thread panicked: {e:?}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    }

    /// Check if the streaming thread is still running.
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
}

impl Drop for StreamingSession {
    fn drop(&mut self) {
        // Signal the stop flag so the streaming thread exits its loop.
        // Without this, dropping a StreamingSession without calling stop()
        // leaves the thread running (holding an Arc<dyn Transcriber>),
        // which prevents GGML Metal cleanup on process exit.
        self.stop.store(true, Ordering::Release);
        // Join the thread to ensure it has exited before the session is gone.
        // This blocks briefly (at most one POLL_INTERVAL_MS + one transcription window).
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The core streaming loop, runs on a dedicated thread.
///
/// Returns ALL audio captured during the session (both processed and unprocessed)
/// so the caller can do a single high-quality final transcription.
fn streaming_loop(
    transcriber: Arc<dyn Transcriber>,
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    tx: mpsc::Sender<String>,
    stop: Arc<AtomicBool>,
    language: Option<String>,
) -> Vec<f32> {
    let mut all_audio: Vec<f32> = Vec::new(); // complete recording for final pass
    let mut step_buf: Vec<f32> = Vec::new();
    let mut prev_tail: Vec<f32> = Vec::new(); // keep_ms overlap from previous window
    let mut window_buf: Vec<f32> = Vec::new(); // reusable window buffer
    let mut swap_buf: VecDeque<f32> = VecDeque::new(); // for O(1) lock swap
    let mut current_step_ms = INITIAL_STEP_MS;

    let keep_samples = ms_to_samples(KEEP_MS);
    let max_buffer_samples = (MAX_BUFFER_S * SAMPLE_RATE as f32) as usize;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        // Drain available samples from shared buffer using swap for O(1) lock hold
        {
            let mut buf = audio_buffer.lock();
            if !buf.is_empty() {
                std::mem::swap(&mut *buf, &mut swap_buf);
            }
        }
        if !swap_buf.is_empty() {
            all_audio.extend(swap_buf.iter());
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
                window_buf.reserve(prev_tail.len() + step_buf.len());
                window_buf.extend_from_slice(&prev_tail);
                window_buf.extend_from_slice(&step_buf);

                // Transcribe the window
                if let Some(text) =
                    transcribe_window(&*transcriber, &window_buf, language.as_deref())
                    && !text.is_empty()
                    && tx.send(text).is_err()
                {
                    // Receiver dropped — stop streaming
                    break;
                }

                // Save tail as overlap for next window — reuse buffer
                prev_tail.clear();
                if step_buf.len() > keep_samples {
                    prev_tail.extend_from_slice(&step_buf[step_buf.len() - keep_samples..]);
                } else {
                    prev_tail.extend_from_slice(&step_buf);
                }
            }

            // Clear step buffer after processing (whether speech or silence)
            step_buf.clear();

            // Grow window toward MAX_STEP_MS (grows on both speech and silence
            // to improve quality as the session progresses; only the first window
            // uses the small INITIAL_STEP_MS for low-latency first partial).
            if current_step_ms < MAX_STEP_MS {
                current_step_ms = (current_step_ms + STEP_GROWTH_MS).min(MAX_STEP_MS);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    all_audio
}

/// Transcribe a single window using the whisper transcriber.
///
/// Returns the transcribed text, or None on error.
fn transcribe_window(
    transcriber: &dyn Transcriber,
    window: &[f32],
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
        Err(e) => {
            tracing::error!(source = "dictation", "Transcription error: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::transcribe::{TranscribeResult, Transcriber};
    use std::sync::atomic::AtomicUsize;

    /// Mock transcriber that echoes the sample count as text.
    struct EchoTranscriber {
        call_count: AtomicUsize,
    }

    impl EchoTranscriber {
        fn new() -> Self {
            Self { call_count: AtomicUsize::new(0) }
        }
    }

    impl Transcriber for EchoTranscriber {
        fn transcribe(&self, audio: &[f32], _language: Option<&str>) -> Result<TranscribeResult, String> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(TranscribeResult {
                text: format!("{}samples", audio.len()),
                skip_reason: None,
            })
        }
    }

    /// Mock transcriber that always skips (simulates silence detection).
    struct SkipTranscriber;

    impl Transcriber for SkipTranscriber {
        fn transcribe(&self, _audio: &[f32], _language: Option<&str>) -> Result<TranscribeResult, String> {
            Ok(TranscribeResult {
                text: String::new(),
                skip_reason: Some("silence".to_string()),
            })
        }
    }

    /// Generate a sine wave (speech-like signal) for testing.
    fn speech_samples(duration_ms: u32) -> Vec<f32> {
        use std::f32::consts::PI;
        let n = ms_to_samples(duration_ms);
        (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.5)
            .collect()
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
        let mut current_step_ms = INITIAL_STEP_MS;
        let mut sizes = vec![current_step_ms];

        for _ in 0..10 {
            if current_step_ms < MAX_STEP_MS {
                current_step_ms = (current_step_ms + STEP_GROWTH_MS).min(MAX_STEP_MS);
            }
            sizes.push(current_step_ms);
        }

        assert_eq!(sizes[0], 1500);
        assert_eq!(sizes[1], 2000);
        assert_eq!(sizes[2], 2500);
        assert_eq!(sizes[3], 3000);
        assert_eq!(sizes[4], 3000);
    }

    #[test]
    fn test_vad_skips_silence() {
        let silence = vec![0.0f32; ms_to_samples(2000)];
        let is_silent = vad::vad_simple(&silence, SAMPLE_RATE, VAD_LAST_MS, VAD_THRESHOLD, VAD_FREQ_THRESHOLD);
        assert!(is_silent, "VAD should detect silence");
    }

    #[test]
    fn test_vad_detects_speech() {
        let speech = speech_samples(2000);
        let is_silent = vad::vad_simple(&speech, SAMPLE_RATE, VAD_LAST_MS, VAD_THRESHOLD, VAD_FREQ_THRESHOLD);
        assert!(!is_silent, "VAD should detect speech");
    }

    #[test]
    fn test_stop_signal_terminates_real_loop() {
        // Test the REAL streaming_loop with EchoTranscriber — no fabricated loop
        let transcriber: Arc<dyn Transcriber> = Arc::new(EchoTranscriber::new());
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<String>();

        let buf_clone = buffer.clone();
        let stop_clone = stop.clone();

        let handle = std::thread::spawn(move || {
            streaming_loop(transcriber, buf_clone, tx, stop_clone, None)
        });

        // Let it poll a few times with no data
        std::thread::sleep(std::time::Duration::from_millis(120));
        stop.store(true, Ordering::Release);

        let remaining = handle.join().expect("Loop should not panic");
        assert!(remaining.is_empty(), "No unconsumed audio expected");
        assert!(rx.try_recv().is_err(), "No partials expected with empty buffer");
    }

    #[test]
    fn test_streaming_loop_emits_partials_on_speech() {
        // Feed speech data into the buffer and verify partials arrive
        let transcriber: Arc<dyn Transcriber> = Arc::new(EchoTranscriber::new());
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<String>();

        // Pre-fill buffer with enough speech for the first window (1500ms)
        {
            let mut buf = buffer.lock();
            buf.extend(speech_samples(2000));
        }

        let buf_clone = buffer.clone();
        let stop_clone = stop.clone();

        let handle = std::thread::spawn(move || {
            streaming_loop(transcriber, buf_clone, tx, stop_clone, None)
        });

        // Wait for the loop to process the speech window
        std::thread::sleep(std::time::Duration::from_millis(200));
        stop.store(true, Ordering::Release);
        handle.join().expect("Loop should not panic");

        // Should have received at least one partial
        let partial = rx.try_recv().expect("Should have received a partial");
        assert!(partial.contains("samples"), "EchoTranscriber returns sample count");
    }

    #[test]
    fn test_streaming_loop_skips_silence() {
        // Feed silence into the buffer — no partials should arrive
        let transcriber: Arc<dyn Transcriber> = Arc::new(EchoTranscriber::new());
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<String>();

        // Pre-fill with silence
        {
            let mut buf = buffer.lock();
            buf.extend(vec![0.0f32; ms_to_samples(2000)]);
        }

        let buf_clone = buffer.clone();
        let stop_clone = stop.clone();

        let handle = std::thread::spawn(move || {
            streaming_loop(transcriber, buf_clone, tx, stop_clone, None)
        });

        std::thread::sleep(std::time::Duration::from_millis(200));
        stop.store(true, Ordering::Release);
        handle.join().expect("Loop should not panic");

        // VAD should have skipped — no partials
        assert!(rx.try_recv().is_err(), "No partials expected for silence");
    }

    #[test]
    fn test_streaming_loop_breaks_on_channel_disconnect() {
        // Drop the receiver — loop should break when tx.send() fails
        let transcriber: Arc<dyn Transcriber> = Arc::new(EchoTranscriber::new());
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<String>();

        // Pre-fill with speech
        {
            let mut buf = buffer.lock();
            buf.extend(speech_samples(2000));
        }

        // Drop receiver before loop processes
        drop(rx);

        let buf_clone = buffer.clone();
        let stop_clone = stop.clone();

        let handle = std::thread::spawn(move || {
            streaming_loop(transcriber, buf_clone, tx, stop_clone, None)
        });

        // Loop should terminate on its own due to channel disconnect
        let remaining = handle.join().expect("Loop should not panic");
        // It may or may not have remaining audio depending on timing
        let _ = remaining;
    }

    #[test]
    fn test_keep_samples_overlap() {
        let keep_samples = ms_to_samples(KEEP_MS);
        assert_eq!(keep_samples, 3200);

        let step_buf: Vec<f32> = (0..48000).map(|i| i as f32 * 0.001).collect();
        let prev_tail = if step_buf.len() > keep_samples {
            step_buf[step_buf.len() - keep_samples..].to_vec()
        } else {
            step_buf.clone()
        };

        assert_eq!(prev_tail.len(), 3200);
        assert!((prev_tail[0] - (48000 - 3200) as f32 * 0.001).abs() < 0.01);
    }

    #[test]
    fn test_transcribe_window_with_skip() {
        let transcriber = SkipTranscriber;
        let window = speech_samples(2000);
        let result = transcribe_window(&transcriber, &window, None);
        assert!(result.is_none(), "SkipTranscriber should return None");
    }

    #[test]
    fn test_transcribe_window_with_echo() {
        let transcriber = EchoTranscriber::new();
        let window = speech_samples(2000);
        let result = transcribe_window(&transcriber, &window, None);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("samples"));
    }

    #[test]
    fn test_all_audio_no_duplication_on_pending_step_buf() {
        // Regression test: when the loop exits with unprocessed samples in
        // step_buf, all_audio must NOT contain those samples twice.
        // (step_buf samples are already in all_audio from the drain at line 141.)
        let transcriber: Arc<dyn Transcriber> = Arc::new(EchoTranscriber::new());
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = mpsc::channel::<String>();

        // Feed exactly 1000ms — less than INITIAL_STEP_MS (1500ms), so the loop
        // will drain it into step_buf but never process a window. When stop fires,
        // step_buf still holds those samples.
        let input_samples = speech_samples(1000);
        let expected_len = input_samples.len();
        {
            let mut buf = buffer.lock();
            buf.extend(input_samples);
        }

        let buf_clone = buffer.clone();
        let stop_clone = stop.clone();

        let handle = std::thread::spawn(move || {
            streaming_loop(transcriber, buf_clone, tx, stop_clone, None)
        });

        // Let the loop drain the buffer
        std::thread::sleep(std::time::Duration::from_millis(120));
        stop.store(true, Ordering::Release);

        let all_audio = handle.join().expect("Loop should not panic");
        assert_eq!(
            all_audio.len(),
            expected_len,
            "all_audio should contain each sample exactly once, got {} instead of {}",
            all_audio.len(),
            expected_len
        );
    }
}
