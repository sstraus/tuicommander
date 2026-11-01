use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Information about an available audio input device.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

/// Lists available audio input devices.
pub fn list_input_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.description().ok())
        .map(|desc| desc.name().to_string())
        .unwrap_or_default();

    host.input_devices()
        .map(|devices| {
            devices
                .filter_map(|d| {
                    let name = d.description().ok()?.name().to_string();
                    Some(AudioDevice {
                        is_default: name == default_name,
                        name,
                    })
                })
                .collect()
        })
        .map_err(|_| "Audio input device enumeration unavailable".to_string())
}

/// Reject a device that reports a nonsensical channel count.
///
/// The count divides every captured chunk into frames. A zero would reach
/// `data.chunks(0)` inside the real-time audio callback, which panics — and a
/// panic on that thread takes the process with it. Refusing the device at
/// start-up turns it into an error the caller can show instead.
fn validate_channel_count(channels: usize) -> Result<usize, String> {
    if channels == 0 {
        return Err("Input device reported 0 channels".to_string());
    }
    Ok(channels)
}

/// Reject a device that reports a 0 Hz sample rate.
///
/// Zero makes the resample ratio infinite, and `(len as f64 * f64::INFINITY)`
/// saturates to `usize::MAX` on the cast — so `reserve` aborts on capacity
/// overflow inside the real-time callback. Same failure shape as a zero
/// channel count, one line further down the same function.
fn validate_sample_rate(sample_rate: u32) -> Result<u32, String> {
    if sample_rate == 0 {
        return Err("Input device reported a 0 Hz sample rate".to_string());
    }
    Ok(sample_rate)
}

/// Keep capture on its supported F32/I16 path when cpal's default heuristics
/// prefer a higher-precision format. Preserve the default rate where possible;
/// the callback resamples whatever rate the device supports to 16 kHz.
fn compatible_input_config(
    configs: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
    preferred_rate: u32,
) -> Result<cpal::SupportedStreamConfig, String> {
    configs
        .filter(|config| {
            matches!(
                config.sample_format(),
                cpal::SampleFormat::F32 | cpal::SampleFormat::I16
            )
        })
        .max_by(cpal::SupportedStreamConfigRange::cmp_default_heuristics)
        .map(|config| {
            config
                .try_with_sample_rate(preferred_rate)
                .unwrap_or_else(|| config.with_max_sample_rate())
        })
        .ok_or_else(|| "Input device has no supported F32 or I16 configuration".to_string())
}

/// Audio capture manager. Captures microphone input as 16kHz mono f32 PCM.
///
/// Uses `VecDeque` so the streaming thread can drain from the front while
/// the cpal callback appends at the back.
pub struct AudioCapture {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    /// Latest normalized microphone level. Atomic so the UI meter never blocks audio capture.
    level: Arc<AtomicU32>,
    dropped_samples: Arc<AtomicUsize>,
    capture_batches: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    stream: Option<cpal::Stream>,
}

impl AudioCapture {
    /// Start capturing audio from a specific device (or system default if None).
    pub fn start_with_device(device_name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();

        let device = if let Some(name) = device_name {
            host.input_devices()
                .map_err(|e| format!("Failed to enumerate devices: {e}"))?
                .find(|d| {
                    d.description()
                        .map(|desc| desc.name() == name)
                        .unwrap_or(false)
                })
                .ok_or_else(|| format!("Input device '{name}' not found"))?
        } else {
            host.default_input_device()
                .ok_or("No default input device available")?
        };

        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get input config: {e}"))?;

        // cpal 0.18 ranks F64/I32/I24 above I16. Do not reject a microphone
        // that still offers one of the formats our capture callback supports.
        let config = if matches!(
            config.sample_format(),
            cpal::SampleFormat::F32 | cpal::SampleFormat::I16
        ) {
            config
        } else {
            compatible_input_config(
                device
                    .supported_input_configs()
                    .map_err(|e| format!("Failed to get input configs: {e}"))?,
                config.sample_rate(),
            )?
        };

        let sample_rate = validate_sample_rate(config.sample_rate())?;
        let channels = validate_channel_count(config.channels() as usize)?;
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = buffer.clone();
        let level = Arc::new(AtomicU32::new(0));
        let level_clone = level.clone();
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let dropped_clone = dropped_samples.clone();
        let capture_batches = Arc::new(AtomicUsize::new(0));
        let batches_clone = capture_batches.clone();
        let active = Arc::new(AtomicBool::new(true));
        let active_clone = active.clone();

        // Build a stream that collects f32 samples, converting to 16kHz mono.
        // Uses try_lock() to avoid blocking the real-time audio callback.
        // Pre-allocate scratch buffers in the closure to avoid per-callback allocations.
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let mut mono_buf: Vec<f32> = Vec::new();
                let mut resample_buf: Vec<f32> = Vec::new();
                device
                    .build_input_stream(
                        config.into(),
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            batches_clone.fetch_add(1, Ordering::Relaxed);
                            process_audio_chunk(
                                data,
                                (sample_rate, channels),
                                &buffer_clone,
                                &level_clone,
                                &dropped_clone,
                                &active_clone,
                                (&mut mono_buf, &mut resample_buf),
                            );
                        },
                        |err| tracing::error!(source = "dictation", "Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?
            }
            cpal::SampleFormat::I16 => {
                let mut float_buf: Vec<f32> = Vec::new();
                let mut mono_buf: Vec<f32> = Vec::new();
                let mut resample_buf: Vec<f32> = Vec::new();
                device
                    .build_input_stream(
                        config.into(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            batches_clone.fetch_add(1, Ordering::Relaxed);
                            float_buf.clear();
                            float_buf
                                .extend(data.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)));
                            process_audio_chunk(
                                &float_buf,
                                (sample_rate, channels),
                                &buffer_clone,
                                &level_clone,
                                &dropped_clone,
                                &active_clone,
                                (&mut mono_buf, &mut resample_buf),
                            );
                        },
                        |err| tracing::error!(source = "dictation", "Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?
            }
            format => return Err(format!("Unsupported sample format: {format:?}")),
        };

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        Ok(Self {
            buffer,
            level,
            dropped_samples,
            capture_batches,
            active,
            stream: Some(stream),
        })
    }

    /// Stop capturing and return all collected audio as 16kHz mono f32 PCM.
    /// Consumes self (batch mode compatibility).
    #[allow(dead_code)] // genuinely unused: the only stop() caller (commands.rs) is StreamingSession::stop, not this batch-mode API
    pub fn stop(mut self) -> Vec<f32> {
        self.stop_stream();
        self.buffer.lock().drain(..).collect()
    }

    /// Stop the cpal stream without consuming self.
    /// Audio already in the buffer remains available for `drain_all()`.
    pub fn stop_stream(&mut self) {
        self.active.store(false, Ordering::Release);
        // Wait for a callback that acquired the buffer before the release.
        // Once this lock is acquired, no later callback can append audio.
        drop(self.buffer.lock());
        self.stream.take();
    }

    /// Drain up to `count` samples from the front of the buffer.
    /// Returns fewer samples if fewer are available.
    #[allow(dead_code)]
    pub fn drain_samples(&self, count: usize) -> Vec<f32> {
        let mut buf = self.buffer.lock();
        let n = count.min(buf.len());
        buf.drain(..n).collect()
    }

    /// Drain all remaining samples from the buffer.
    #[allow(dead_code)]
    pub fn drain_all(&self) -> Vec<f32> {
        self.buffer.lock().drain(..).collect()
    }

    /// How many samples are currently buffered.
    #[allow(dead_code)]
    pub fn available(&self) -> usize {
        self.buffer.lock().len()
    }

    /// Get a clone of the buffer Arc for sharing with other threads.
    pub fn buffer_handle(&self) -> Arc<Mutex<VecDeque<f32>>> {
        self.buffer.clone()
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Callback-batch and lost 16 kHz sample counters for the armed endpoint.
    pub fn diagnostic_counters(&self) -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
        (self.capture_batches.clone(), self.dropped_samples.clone())
    }

    pub fn dropped_samples(&self) -> usize {
        self.dropped_samples.load(Ordering::Relaxed)
    }
}

/// Process an audio chunk: convert to mono and resample to 16kHz.
/// Uses `try_lock()` so the real-time audio callback never blocks.
/// If the lock is contended, samples are silently dropped (acceptable
/// for dictation — 16kHz mono = ~64KB/s, contention is rare).
///
/// `mono_buf` and `resample_buf` are pre-allocated scratch buffers owned by
/// the closure to avoid per-callback heap allocations.
fn process_audio_chunk(
    data: &[f32],
    input_format: (u32, usize),
    buffer: &Arc<Mutex<VecDeque<f32>>>,
    level: &Arc<AtomicU32>,
    dropped_samples: &Arc<AtomicUsize>,
    active: &Arc<AtomicBool>,
    scratch: (&mut Vec<f32>, &mut Vec<f32>),
) {
    let (sample_rate, channels) = input_format;
    let (mono_buf, resample_buf) = scratch;
    if !active.load(Ordering::Acquire) {
        return;
    }
    // `validate_channel_count` rejects a zero-channel device at start-up, so
    // this is unreachable in practice. It stays because the alternative here
    // is `chunks(0)` panicking on the real-time audio thread, where an unwind
    // aborts the process rather than surfacing an error.
    if channels == 0 {
        return;
    }

    // Same reasoning for the rate: `validate_sample_rate` rejects a 0 Hz device
    // at start-up, so this is unreachable in practice. Without it the resample
    // ratio below is infinite and the cast saturates to `usize::MAX`, which
    // aborts on `reserve` — on the audio thread, where that is fatal.
    if sample_rate == 0 {
        return;
    }

    // Convert to mono by averaging channels — reuse buffer
    mono_buf.clear();
    mono_buf.extend(
        data.chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32),
    );

    // RMS responds to speech without the jitter of peak levels. Consumer mics
    // commonly produce only 0.1–1% normalized RMS, so use a square-root curve
    // to make ordinary speech visible in the compact preview meter.
    let rms = if mono_buf.is_empty() {
        0.0
    } else {
        (mono_buf.iter().map(|sample| sample * sample).sum::<f32>() / mono_buf.len() as f32).sqrt()
    };
    let meter_level = (rms * 20.0).sqrt().clamp(0.0, 1.0);
    level.store(meter_level.to_bits(), Ordering::Relaxed);

    // Simple nearest-neighbor resampling to 16kHz
    let output = if sample_rate == 16000 {
        &mono_buf[..]
    } else {
        resample_buf.clear();
        let ratio = 16000.0 / f64::from(sample_rate);
        let output_len = (mono_buf.len() as f64 * ratio) as usize;
        resample_buf.reserve(output_len);
        for i in 0..output_len {
            let src_idx = (i as f64 / ratio) as usize;
            if src_idx < mono_buf.len() {
                resample_buf.push(mono_buf[src_idx]);
            }
        }
        &resample_buf[..]
    };

    // try_lock: never block the audio callback
    if let Some(mut buf) = buffer.try_lock() {
        if !active.load(Ordering::Acquire) {
            return;
        }
        buf.extend(output.iter());
        // The streaming thread can spend longer than 30s decoding one window.
        // Keep the same bounded horizon as its final recording buffer.
        let max_samples = (16_000.0 * crate::streaming::MAX_RECORDING_S) as usize;
        let len = buf.len();
        if len > max_samples {
            let dropped = len - max_samples;
            buf.drain(..dropped);
            dropped_samples.fetch_add(dropped, Ordering::Relaxed);
        }
    } else {
        dropped_samples.fetch_add(output.len(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: accepting cpal's new integer default and refusing a device that
    // still supports I16; also selecting I16 before the preferred F32 path.
    #[test]
    fn higher_precision_defaults_keep_a_compatible_capture_format() {
        let configs = [
            cpal::SampleFormat::I32,
            cpal::SampleFormat::I16,
            cpal::SampleFormat::F32,
        ]
        .map(|format| {
            cpal::SupportedStreamConfigRange::new(
                2,
                16_000,
                96_000,
                cpal::SupportedBufferSize::Unknown,
                format,
            )
        });
        let config = compatible_input_config(configs.into_iter(), 48_000).unwrap();
        assert_eq!(config.sample_format(), cpal::SampleFormat::F32);
        assert_eq!(config.sample_rate(), 48_000);
        assert_eq!(config.channels(), 2);
    }

    // Catches: dropping the I16 fallback, or requesting an unsupported rate.
    #[test]
    fn integer_only_capture_uses_a_supported_rate() {
        let config = cpal::SupportedStreamConfigRange::new(
            1,
            44_100,
            44_100,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::I16,
        );
        let selected = compatible_input_config(std::iter::once(config), 48_000).unwrap();
        assert_eq!(selected.sample_format(), cpal::SampleFormat::I16);
        assert_eq!(selected.sample_rate(), 44_100);
    }

    // Catches: silently selecting a format the callback cannot consume.
    #[test]
    fn unsupported_capture_formats_fail_before_stream_creation() {
        let config = cpal::SupportedStreamConfigRange::new(
            1,
            48_000,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::I32,
        );
        assert_eq!(
            compatible_input_config(std::iter::once(config), 48_000).unwrap_err(),
            "Input device has no supported F32 or I16 configuration",
        );
    }

    /// The scratch state a cpal callback owns, so each chunk test reads as one
    /// call instead of six lines of setup.
    struct ChunkFixture {
        buffer: Arc<Mutex<VecDeque<f32>>>,
        level: Arc<AtomicU32>,
        dropped_samples: Arc<AtomicUsize>,
        active: Arc<AtomicBool>,
        mono: Vec<f32>,
        resample: Vec<f32>,
    }

    impl ChunkFixture {
        fn new() -> Self {
            Self {
                buffer: Arc::new(Mutex::new(VecDeque::new())),
                level: Arc::new(AtomicU32::new(0)),
                dropped_samples: Arc::new(AtomicUsize::new(0)),
                active: Arc::new(AtomicBool::new(true)),
                mono: Vec::new(),
                resample: Vec::new(),
            }
        }

        /// Push one callback's worth of samples through the chunk processor.
        fn process(&mut self, data: &[f32], sample_rate: u32, channels: usize) {
            process_audio_chunk(
                data,
                (sample_rate, channels),
                &self.buffer,
                &self.level,
                &self.dropped_samples,
                &self.active,
                (&mut self.mono, &mut self.resample),
            );
        }

        fn captured(&self) -> Vec<f32> {
            self.buffer.lock().iter().copied().collect()
        }
    }

    /// Compare samples on magnitude: the values under test are exact binary
    /// fractions, but an epsilon keeps the assertion honest about floats.
    fn assert_samples(got: &[f32], expected: &[f32]) {
        assert_eq!(got.len(), expected.len(), "sample count: {got:?}");
        for (i, (g, e)) in got.iter().zip(expected).enumerate() {
            assert!(
                (g - e).abs() < 1e-6,
                "sample {i}: got {g}, expected {e} (all: {got:?})"
            );
        }
    }

    /// A device that reports zero channels must not take the process down.
    /// `data.chunks(0)` panics, and it would panic on the real-time audio
    /// thread, where an unwind aborts.
    #[test]
    fn process_audio_chunk_survives_zero_channel_device() {
        let mut fx = ChunkFixture::new();

        fx.process(&[0.1, 0.2, 0.3, 0.4], 16_000, 0);

        assert!(
            fx.captured().is_empty(),
            "a zero-channel chunk must contribute no samples"
        );
        assert_eq!(
            fx.level.load(Ordering::Relaxed),
            0,
            "the meter must not move for a rejected chunk"
        );
    }

    #[test]
    fn process_audio_chunk_passes_mono_through() {
        let mut fx = ChunkFixture::new();
        fx.process(&[0.25, 0.5], 16_000, 1);
        assert_samples(&fx.captured(), &[0.25, 0.5]);
    }

    #[test]
    fn thirty_one_second_capture_preserves_its_first_sample() {
        let mut fx = ChunkFixture::new();
        fx.process(&[0.25], 16_000, 1);
        fx.process(&vec![0.5; 31 * 16_000], 16_000, 1);

        let captured = fx.captured();
        assert_eq!(captured.len(), 31 * 16_000 + 1);
        assert_eq!(
            captured[0].to_bits(),
            0.25_f32.to_bits(),
            "the beginning of the dictation was lost"
        );
        assert_eq!(captured.last(), Some(&0.5));
    }

    #[test]
    fn process_audio_chunk_averages_stereo_frames() {
        let mut fx = ChunkFixture::new();
        fx.process(&[0.0, 1.0, 0.5, 0.5], 16_000, 2);
        assert_samples(&fx.captured(), &[0.5, 0.5]);
    }

    /// A chunk that does not divide evenly into frames must still be processed
    /// rather than panic or drop samples. The short trailing frame is divided
    /// by the declared channel count, not by the samples actually present, so
    /// it comes out attenuated (0.4 / 2 channels = 0.2) — cpal delivers whole
    /// frames, so this documents the edge rather than endorsing it.
    #[test]
    fn process_audio_chunk_handles_partial_trailing_frame() {
        let mut fx = ChunkFixture::new();
        fx.process(&[0.0, 1.0, 0.4], 16_000, 2);
        assert_samples(&fx.captured(), &[0.5, 0.2]);
    }

    /// Catches the meter arithmetic in `process_audio_chunk` (square, mean,
    /// `* 20.0`, `sqrt`, `clamp`) flipping: a constant 0.0125 has RMS 0.0125,
    /// so the meter reads sqrt(0.25) = 0.5; full scale saturates at 1.0.
    #[test]
    fn process_audio_chunk_meter_follows_the_square_root_rms_curve() {
        let meter = |data: &[f32]| {
            let mut fx = ChunkFixture::new();
            fx.process(data, 16_000, 1);
            f32::from_bits(fx.level.load(Ordering::Relaxed))
        };
        assert!((meter(&[0.0125; 4]) - 0.5).abs() < 1e-3);
        assert_eq!(meter(&[1.0; 4]).to_bits(), 1.0_f32.to_bits());
        assert_eq!(meter(&[0.0; 4]).to_bits(), 0.0_f32.to_bits());
    }

    /// Catches the resample index arithmetic flipping (`i / ratio`): 48 kHz
    /// keeps every third sample, 8 kHz repeats each sample twice.
    #[test]
    fn process_audio_chunk_resamples_to_16khz_nearest_neighbour() {
        let mut down = ChunkFixture::new();
        down.process(&[0.0, 0.1, 0.2, 0.3, 0.4, 0.5], 48_000, 1);
        assert_samples(&down.captured(), &[0.0, 0.3]);

        let mut up = ChunkFixture::new();
        up.process(&[0.1, 0.2, 0.3], 8_000, 1);
        assert_samples(&up.captured(), &[0.1, 0.1, 0.2, 0.2, 0.3, 0.3]);
    }

    /// Catches the cap comparison (`len > max_samples` becoming `==` or `<`)
    /// and the drop count (`len - max_samples` becoming `+` or `/`): a few
    /// samples over the cap drops exactly the oldest ones.
    /// `len > max_samples` -> `>=` (audio.rs:315) is an equivalent mutant: at
    /// `len == max_samples` it drops 0 samples either way, so no test can
    /// tell the two apart.
    #[test]
    fn process_audio_chunk_caps_the_buffer_at_the_recording_horizon() {
        let max = (16_000.0 * crate::streaming::MAX_RECORDING_S) as usize;

        // Boundary sanity check, not a mutant-killer: exactly the cap keeps
        // everything and drops nothing.
        let mut at_cap = ChunkFixture::new();
        at_cap.process(&vec![0.25; max], 16_000, 1);
        assert_eq!(at_cap.buffer.lock().len(), max);
        assert_eq!(at_cap.dropped_samples.load(Ordering::Relaxed), 0);

        let mut over = ChunkFixture::new();
        over.process(&[0.125, 0.125, 0.125], 16_000, 1);
        over.process(&vec![0.25; max], 16_000, 1);
        assert_eq!(over.buffer.lock().len(), max);
        assert_eq!(over.dropped_samples.load(Ordering::Relaxed), 3);
        assert!(
            over.buffer
                .lock()
                .iter()
                .all(|&s| s.to_bits() == 0.25_f32.to_bits()),
            "the oldest samples must be the ones dropped"
        );
    }

    /// 44.1 kHz is the other common device rate and its ratio is not an exact
    /// fraction. Catches `output_len` rounding up (the audio grows by a sample
    /// per callback, so dictation drifts against real time) and a source index
    /// that overruns or reorders the chunk. 442 ramp samples -> 160 outputs;
    /// out[i] is input sample floor(i * 2.75625), checked away from exact
    /// integer products where f64 rounding may legitimately pick either side.
    #[test]
    fn process_audio_chunk_resamples_44_1khz_keeping_length_and_order() {
        let ramp: Vec<f32> = (0..442).map(|i| i as f32).collect();
        let mut fx = ChunkFixture::new();
        fx.process(&ramp, 44_100, 1);
        let out = fx.captured();
        assert_eq!(out.len(), 160, "442 * 16000 / 44100 = 160.36, floored");
        assert!(out.windows(2).all(|w| w[0] <= w[1]), "order lost: {out:?}");
        assert_eq!(out[0].to_bits(), 0.0_f32.to_bits());
        assert_eq!(out[1].to_bits(), 2.0_f32.to_bits());
        assert_eq!(out[100].to_bits(), 275.0_f32.to_bits());
        assert_eq!(out[159].to_bits(), 438.0_f32.to_bits());
    }

    /// A device reporting 0 Hz makes `ratio` infinite, and
    /// `(len as f64 * f64::INFINITY) as usize` saturates to `usize::MAX` — so
    /// `reserve` aborts on capacity overflow, on the real-time audio thread.
    /// Same class as the zero-channel bug, one line further down the callback.
    #[test]
    fn process_audio_chunk_survives_zero_sample_rate_device() {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let level = Arc::new(AtomicU32::new(0));
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(true));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        process_audio_chunk(
            &[0.1, 0.2, 0.3, 0.4],
            (0, 1),
            &buffer,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );

        assert!(
            buffer.lock().is_empty(),
            "a 0 Hz device must be dropped, not resampled into a usize::MAX allocation"
        );
    }

    #[test]
    fn validate_sample_rate_rejects_zero_and_accepts_real_devices() {
        assert!(
            validate_sample_rate(0).is_err(),
            "a 0 Hz device must be reported, not accepted"
        );
        assert_eq!(validate_sample_rate(16_000).expect("16 kHz"), 16_000);
        assert_eq!(validate_sample_rate(48_000).expect("48 kHz"), 48_000);
    }

    #[test]
    fn validate_channel_count_rejects_zero_and_accepts_real_devices() {
        assert!(
            validate_channel_count(0).is_err(),
            "a 0-channel device must be reported, not accepted"
        );
        assert_eq!(validate_channel_count(1), Ok(1));
        assert_eq!(validate_channel_count(2), Ok(2));
    }

    #[test]
    fn test_streaming_buffer_drain() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        // Push 100 samples
        {
            let mut b = buf.lock();
            b.extend((0..100).map(|i| i as f32));
        }

        // Create a mock AudioCapture with no stream
        let capture = AudioCapture {
            buffer: buf,
            level: Arc::new(AtomicU32::new(0)),
            dropped_samples: Arc::new(AtomicUsize::new(0)),
            capture_batches: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicBool::new(true)),
            stream: None,
        };

        // Drain 30 — should get 30 and leave 70
        let drained = capture.drain_samples(30);
        assert_eq!(drained.len(), 30);
        assert_eq!(drained[0] as u32, 0);
        assert_eq!(drained[29] as u32, 29);
        assert_eq!(capture.available(), 70);

        // Drain 100 — should get only the remaining 70
        let drained = capture.drain_samples(100);
        assert_eq!(drained.len(), 70);
        assert_eq!(drained[0] as u32, 30);
        assert_eq!(capture.available(), 0);
    }

    #[test]
    fn test_drain_all() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut b = buf.lock();
            b.extend((0..50).map(|i| i as f32));
        }
        let capture = AudioCapture {
            buffer: buf,
            level: Arc::new(AtomicU32::new(0)),
            dropped_samples: Arc::new(AtomicUsize::new(0)),
            capture_batches: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicBool::new(true)),
            stream: None,
        };
        let all = capture.drain_all();
        assert_eq!(all.len(), 50);
        assert_eq!(capture.available(), 0);
    }

    #[test]
    fn test_stop_stream_preserves_buffer() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut b = buf.lock();
            b.extend((0..10).map(|i| i as f32));
        }
        let mut capture = AudioCapture {
            buffer: buf,
            level: Arc::new(AtomicU32::new(0)),
            dropped_samples: Arc::new(AtomicUsize::new(0)),
            capture_batches: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicBool::new(true)),
            stream: None,
        };
        capture.stop_stream(); // no-op since stream is None
        assert_eq!(capture.available(), 10);
        let all = capture.drain_all();
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn stopped_capture_rejects_late_audio_callbacks() {
        let mut capture = AudioCapture {
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            level: Arc::new(AtomicU32::new(0)),
            dropped_samples: Arc::new(AtomicUsize::new(0)),
            capture_batches: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicBool::new(true)),
            stream: None,
        };
        let mut mono = Vec::new();
        let mut resample = Vec::new();
        process_audio_chunk(
            &[0.25],
            (16_000, 1),
            &capture.buffer,
            &capture.level,
            &capture.dropped_samples,
            &capture.active,
            (&mut mono, &mut resample),
        );
        capture.stop_stream();
        process_audio_chunk(
            &[0.75],
            (16_000, 1),
            &capture.buffer,
            &capture.level,
            &capture.dropped_samples,
            &capture.active,
            (&mut mono, &mut resample),
        );

        assert_eq!(capture.drain_all(), vec![0.25]);
    }

    #[test]
    fn test_process_audio_chunk_try_lock() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let level = Arc::new(AtomicU32::new(0));
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(true));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();
        let data = vec![0.5f32; 16]; // 16 mono samples at 16kHz
        process_audio_chunk(
            &data,
            (16000, 1),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        assert_eq!(buf.lock().len(), 16);
        assert!(f32::from_bits(level.load(Ordering::Relaxed)) > 0.9);

        // While lock is held, process_audio_chunk silently drops samples
        let guard = buf.lock();
        process_audio_chunk(
            &data,
            (16000, 1),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        assert_eq!(guard.len(), 16); // still 16, new samples dropped
        assert_eq!(dropped_samples.load(Ordering::Relaxed), 16);
    }

    #[test]
    fn test_process_audio_chunk_stereo_to_mono() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let level = Arc::new(AtomicU32::new(0));
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(true));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        // 8 stereo frames (16 samples total) at 16kHz
        // Left=1.0, Right=0.0 → mono should average to 0.5
        let stereo_data: Vec<f32> = (0..8).flat_map(|_| vec![1.0, 0.0]).collect();
        assert_eq!(stereo_data.len(), 16);

        process_audio_chunk(
            &stereo_data,
            (16000, 2),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        let result: Vec<f32> = buf.lock().drain(..).collect();
        assert_eq!(result.len(), 8, "Stereo→mono should halve the sample count");
        for s in &result {
            assert!(
                (s - 0.5).abs() < 1e-6,
                "Each mono sample should be 0.5, got {s}"
            );
        }
    }

    #[test]
    fn test_process_audio_chunk_resample_48k_to_16k() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let level = Arc::new(AtomicU32::new(0));
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(true));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        // 480 mono samples at 48kHz = 10ms of audio → should produce ~160 samples at 16kHz
        let data = vec![0.25f32; 480];
        process_audio_chunk(
            &data,
            (48000, 1),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        let result: Vec<f32> = buf.lock().drain(..).collect();

        // 480 * (16000/48000) = 160
        assert_eq!(result.len(), 160, "48kHz→16kHz should produce 1/3 samples");
        for s in &result {
            assert!(
                (s - 0.25).abs() < 1e-6,
                "Nearest-neighbor should preserve value"
            );
        }
    }

    #[test]
    fn test_process_audio_chunk_stereo_48k() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let level = Arc::new(AtomicU32::new(0));
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(true));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        // 960 samples = 480 stereo frames at 48kHz = 10ms
        let stereo_data: Vec<f32> = (0..480).flat_map(|_| vec![0.8, 0.2]).collect();
        process_audio_chunk(
            &stereo_data,
            (48000, 2),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        let result: Vec<f32> = buf.lock().drain(..).collect();

        // 480 mono frames at 48kHz → 160 samples at 16kHz
        assert_eq!(result.len(), 160, "Stereo 48kHz→mono 16kHz");
        for s in &result {
            assert!((s - 0.5).abs() < 1e-6, "Averaged stereo (0.8+0.2)/2 = 0.5");
        }
    }

    #[test]
    fn test_scratch_buffers_reused() {
        // Verify that scratch buffers are reused across calls (capacity grows once)
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let level = Arc::new(AtomicU32::new(0));
        let dropped_samples = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(true));
        let mut mono_buf: Vec<f32> = Vec::new();
        let mut resample_buf: Vec<f32> = Vec::new();

        let data = vec![0.5f32; 480];
        process_audio_chunk(
            &data,
            (48000, 1),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        let cap_after_first = mono_buf.capacity();

        buf.lock().clear();
        process_audio_chunk(
            &data,
            (48000, 1),
            &buf,
            &level,
            &dropped_samples,
            &active,
            (&mut mono_buf, &mut resample_buf),
        );
        let cap_after_second = mono_buf.capacity();

        // Capacity should not have changed — buffers are reused
        assert_eq!(
            cap_after_first, cap_after_second,
            "Buffer should be reused, not reallocated"
        );
    }
}
