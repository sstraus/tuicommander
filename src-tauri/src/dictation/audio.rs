use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;

/// Information about an available audio input device.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

/// Lists available audio input devices.
pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    host.input_devices()
        .map(|devices| {
            devices
                .filter_map(|d| {
                    let name = d.name().ok()?;
                    Some(AudioDevice {
                        is_default: name == default_name,
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Audio capture manager. Captures microphone input as 16kHz mono f32 PCM.
///
/// Uses `VecDeque` so the streaming thread can drain from the front while
/// the cpal callback appends at the back.
pub struct AudioCapture {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    stream: Option<cpal::Stream>,
}

// Safety: cpal::Stream is !Send because it holds platform-specific raw pointers.
// AudioCapture wraps Stream inside Option<cpal::Stream>, which is only created,
// played, and dropped on the same thread (via DictationState's parking_lot::Mutex).
// The Mutex serialises all access, so no data race is possible.
unsafe impl Send for AudioCapture {}

impl AudioCapture {
    /// Start capturing audio from the default input device.
    pub fn start() -> Result<Self, String> {
        Self::start_with_device(None)
    }

    /// Start capturing audio from a specific device (or default if None).
    pub fn start_with_device(device_name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();

        let device = if let Some(name) = device_name {
            host.input_devices()
                .map_err(|e| format!("Failed to enumerate devices: {e}"))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| format!("Input device '{name}' not found"))?
        } else {
            host.default_input_device()
                .ok_or("No default input device available")?
        };

        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get input config: {e}"))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = buffer.clone();

        // Build a stream that collects f32 samples, converting to 16kHz mono.
        // Uses try_lock() to avoid blocking the real-time audio callback.
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_audio_chunk(data, sample_rate, channels, &buffer_clone);
                    },
                    |err| eprintln!("Audio stream error: {err}"),
                    None,
                )
                .map_err(|e| format!("Failed to build input stream: {e}"))?,
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let float_data: Vec<f32> =
                            data.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect();
                        process_audio_chunk(&float_data, sample_rate, channels, &buffer_clone);
                    },
                    |err| eprintln!("Audio stream error: {err}"),
                    None,
                )
                .map_err(|e| format!("Failed to build input stream: {e}"))?,
            format => return Err(format!("Unsupported sample format: {format:?}")),
        };

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        Ok(Self {
            buffer,
            stream: Some(stream),
        })
    }

    /// Stop capturing and return all collected audio as 16kHz mono f32 PCM.
    /// Consumes self (batch mode compatibility).
    pub fn stop(mut self) -> Vec<f32> {
        self.stream.take();
        self.buffer.lock().drain(..).collect()
    }

    /// Stop the cpal stream without consuming self.
    /// Audio already in the buffer remains available for `drain_all()`.
    pub fn stop_stream(&mut self) {
        self.stream.take();
    }

    /// Drain up to `count` samples from the front of the buffer.
    /// Returns fewer samples if fewer are available.
    pub fn drain_samples(&self, count: usize) -> Vec<f32> {
        let mut buf = self.buffer.lock();
        let n = count.min(buf.len());
        buf.drain(..n).collect()
    }

    /// Drain all remaining samples from the buffer.
    pub fn drain_all(&self) -> Vec<f32> {
        self.buffer.lock().drain(..).collect()
    }

    /// How many samples are currently buffered.
    pub fn available(&self) -> usize {
        self.buffer.lock().len()
    }

    /// Get a clone of the buffer Arc for sharing with other threads.
    pub fn buffer_handle(&self) -> Arc<Mutex<VecDeque<f32>>> {
        self.buffer.clone()
    }
}

/// Maximum buffer size: 30 seconds at 16kHz = 480,000 samples.
/// If the streaming thread falls behind, older samples are dropped.
const MAX_BUFFER_SAMPLES: usize = 16_000 * 30;

/// Process an audio chunk: convert to mono and resample to 16kHz.
/// Uses `try_lock()` so the real-time audio callback never blocks.
/// If the lock is contended, samples are silently dropped (acceptable
/// for dictation — 16kHz mono = ~64KB/s, contention is rare).
fn process_audio_chunk(
    data: &[f32],
    sample_rate: u32,
    channels: usize,
    buffer: &Arc<Mutex<VecDeque<f32>>>,
) {
    // try_lock: never block the real-time audio callback
    let Some(mut buf) = buffer.try_lock() else {
        return; // contention — drop samples (acceptable for dictation)
    };

    if channels == 1 && sample_rate == 16000 {
        // Fast path: no conversion needed — extend directly
        buf.extend(data.iter());
    } else if sample_rate == 16000 {
        // Mono-mix only (no resampling)
        let ch_f = channels as f32;
        for frame in data.chunks(channels) {
            buf.push_back(frame.iter().sum::<f32>() / ch_f);
        }
    } else {
        // Full path: mono-mix + nearest-neighbor resample
        let ch_f = channels as f32;
        let mono_len = data.len() / channels;
        let ratio = 16000.0 / f64::from(sample_rate);
        let output_len = (mono_len as f64 * ratio) as usize;

        for i in 0..output_len {
            let src_idx = (i as f64 / ratio) as usize;
            if src_idx < mono_len {
                let frame = &data[src_idx * channels..(src_idx + 1) * channels];
                buf.push_back(frame.iter().sum::<f32>() / ch_f);
            }
        }
    }

    // Cap buffer to prevent unbounded growth if the consumer falls behind
    if buf.len() > MAX_BUFFER_SAMPLES {
        let excess = buf.len() - MAX_BUFFER_SAMPLES;
        buf.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            stream: None,
        };
        capture.stop_stream(); // no-op since stream is None
        assert_eq!(capture.available(), 10);
        let all = capture.drain_all();
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn test_process_audio_chunk_try_lock() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let data = vec![0.5f32; 16]; // 16 mono samples at 16kHz
        process_audio_chunk(&data, 16000, 1, &buf);
        assert_eq!(buf.lock().len(), 16);

        // While lock is held, process_audio_chunk silently drops samples
        let guard = buf.lock();
        process_audio_chunk(&data, 16000, 1, &buf);
        assert_eq!(guard.len(), 16); // still 16, new samples dropped
    }

    #[test]
    fn test_process_audio_chunk_stereo_16khz() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        // 4 stereo frames: [L0, R0, L1, R1, L2, R2, L3, R3]
        let data = vec![0.2, 0.8, 0.4, 0.6, 1.0, 0.0, -0.5, 0.5];
        process_audio_chunk(&data, 16000, 2, &buf);

        let result: Vec<f32> = buf.lock().drain(..).collect();
        assert_eq!(result.len(), 4);
        // Each output sample is the average of L and R
        assert!((result[0] - 0.5).abs() < 1e-6); // (0.2 + 0.8) / 2
        assert!((result[1] - 0.5).abs() < 1e-6); // (0.4 + 0.6) / 2
        assert!((result[2] - 0.5).abs() < 1e-6); // (1.0 + 0.0) / 2
        assert!((result[3] - 0.0).abs() < 1e-6); // (-0.5 + 0.5) / 2
    }

    #[test]
    fn test_process_audio_chunk_resample_48khz_mono() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        // 48 samples at 48kHz mono = 1ms → should produce ~16 samples at 16kHz
        let data: Vec<f32> = (0..48).map(|i| i as f32 / 48.0).collect();
        process_audio_chunk(&data, 48000, 1, &buf);

        let result: Vec<f32> = buf.lock().drain(..).collect();
        // 48 samples at 48kHz → 16 samples at 16kHz (ratio 1/3)
        assert_eq!(result.len(), 16);
        // First sample should be data[0] = 0.0
        assert!((result[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_process_audio_chunk_resample_48khz_stereo() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        // 48 stereo frames at 48kHz = 96 f32 values, 1ms
        // Each frame: [L=0.4, R=0.6] → mono average = 0.5
        let data: Vec<f32> = (0..48).flat_map(|_| vec![0.4f32, 0.6]).collect();
        process_audio_chunk(&data, 48000, 2, &buf);

        let result: Vec<f32> = buf.lock().drain(..).collect();
        // 48 frames at 48kHz → 16 samples at 16kHz
        assert_eq!(result.len(), 16);
        // All samples should be ~0.5 (average of 0.4 and 0.6)
        for (i, &sample) in result.iter().enumerate() {
            assert!(
                (sample - 0.5).abs() < 1e-6,
                "sample {i} should be 0.5, got {sample}"
            );
        }
    }

    #[test]
    fn test_process_audio_chunk_buffer_cap() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        // Pre-fill buffer near max capacity
        {
            let mut b = buf.lock();
            b.extend(std::iter::repeat(0.0f32).take(MAX_BUFFER_SAMPLES));
        }
        assert_eq!(buf.lock().len(), MAX_BUFFER_SAMPLES);

        // Add 100 more samples — should trigger cap, keeping total at MAX_BUFFER_SAMPLES
        let extra = vec![1.0f32; 100];
        process_audio_chunk(&extra, 16000, 1, &buf);

        let b = buf.lock();
        assert_eq!(b.len(), MAX_BUFFER_SAMPLES);
        // The last 100 samples should be 1.0 (the new data)
        assert!((b[MAX_BUFFER_SAMPLES - 1] - 1.0).abs() < 1e-6);
        // The oldest samples (0.0) should have been drained
        assert!((b[0] - 0.0).abs() < 1e-6); // still 0.0 since only 100 were drained
    }
}
