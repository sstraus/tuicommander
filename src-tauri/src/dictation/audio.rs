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

// Safety: cpal::Stream is !Send due to platform audio API raw pointers.
// AudioCapture is stored in `Mutex<Option<AudioCapture>>` in DictationState.
// The stream is created on one thread and only dropped (via `stop_stream()` or `Drop`)
// while holding the mutex lock. We never dereference or use cpal's internal raw
// pointers directly — all interaction goes through cpal's public API.
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
                .find(|d| d.description().map(|desc| desc.name() == name).unwrap_or(false))
                .ok_or_else(|| format!("Input device '{name}' not found"))?
        } else {
            host.default_input_device()
                .ok_or("No default input device available")?
        };

        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get input config: {e}"))?;

        let sample_rate = config.sample_rate();
        let channels = config.channels() as usize;
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = buffer.clone();

        // Build a stream that collects f32 samples, converting to 16kHz mono.
        // Uses try_lock() to avoid blocking the real-time audio callback.
        // Pre-allocate scratch buffers in the closure to avoid per-callback allocations.
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let mut mono_buf: Vec<f32> = Vec::new();
                let mut resample_buf: Vec<f32> = Vec::new();
                device
                    .build_input_stream(
                        &config.into(),
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            process_audio_chunk(data, sample_rate, channels, &buffer_clone,
                                &mut mono_buf, &mut resample_buf);
                        },
                        |err| eprintln!("[dictation] audio stream error: {err}"),
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
                        &config.into(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            float_buf.clear();
                            float_buf.extend(data.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)));
                            process_audio_chunk(&float_buf, sample_rate, channels, &buffer_clone,
                                &mut mono_buf, &mut resample_buf);
                        },
                        |err| eprintln!("[dictation] audio stream error: {err}"),
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
            stream: Some(stream),
        })
    }

    /// Stop capturing and return all collected audio as 16kHz mono f32 PCM.
    /// Consumes self (batch mode compatibility).
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn available(&self) -> usize {
        self.buffer.lock().len()
    }

    /// Get a clone of the buffer Arc for sharing with other threads.
    pub fn buffer_handle(&self) -> Arc<Mutex<VecDeque<f32>>> {
        self.buffer.clone()
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
    sample_rate: u32,
    channels: usize,
    buffer: &Arc<Mutex<VecDeque<f32>>>,
    mono_buf: &mut Vec<f32>,
    resample_buf: &mut Vec<f32>,
) {
    // Convert to mono by averaging channels — reuse buffer
    mono_buf.clear();
    mono_buf.extend(
        data.chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32),
    );

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
        buf.extend(output.iter());
        // Cap at 30s (480k samples at 16kHz) to prevent unbounded growth
        const MAX_SAMPLES: usize = 16_000 * 30;
        let len = buf.len();
        if len > MAX_SAMPLES {
            buf.drain(..len - MAX_SAMPLES);
        }
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
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();
        let data = vec![0.5f32; 16]; // 16 mono samples at 16kHz
        process_audio_chunk(&data, 16000, 1, &buf, &mut mono_buf, &mut resample_buf);
        assert_eq!(buf.lock().len(), 16);

        // While lock is held, process_audio_chunk silently drops samples
        let guard = buf.lock();
        process_audio_chunk(&data, 16000, 1, &buf, &mut mono_buf, &mut resample_buf);
        assert_eq!(guard.len(), 16); // still 16, new samples dropped
    }

    #[test]
    fn test_process_audio_chunk_stereo_to_mono() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        // 8 stereo frames (16 samples total) at 16kHz
        // Left=1.0, Right=0.0 → mono should average to 0.5
        let stereo_data: Vec<f32> = (0..8).flat_map(|_| vec![1.0, 0.0]).collect();
        assert_eq!(stereo_data.len(), 16);

        process_audio_chunk(&stereo_data, 16000, 2, &buf, &mut mono_buf, &mut resample_buf);
        let result: Vec<f32> = buf.lock().drain(..).collect();
        assert_eq!(result.len(), 8, "Stereo→mono should halve the sample count");
        for s in &result {
            assert!((s - 0.5).abs() < 1e-6, "Each mono sample should be 0.5, got {s}");
        }
    }

    #[test]
    fn test_process_audio_chunk_resample_48k_to_16k() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        // 480 mono samples at 48kHz = 10ms of audio → should produce ~160 samples at 16kHz
        let data = vec![0.25f32; 480];
        process_audio_chunk(&data, 48000, 1, &buf, &mut mono_buf, &mut resample_buf);
        let result: Vec<f32> = buf.lock().drain(..).collect();

        // 480 * (16000/48000) = 160
        assert_eq!(result.len(), 160, "48kHz→16kHz should produce 1/3 samples");
        for s in &result {
            assert!((s - 0.25).abs() < 1e-6, "Nearest-neighbor should preserve value");
        }
    }

    #[test]
    fn test_process_audio_chunk_stereo_48k() {
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut mono_buf = Vec::new();
        let mut resample_buf = Vec::new();

        // 960 samples = 480 stereo frames at 48kHz = 10ms
        let stereo_data: Vec<f32> = (0..480).flat_map(|_| vec![0.8, 0.2]).collect();
        process_audio_chunk(&stereo_data, 48000, 2, &buf, &mut mono_buf, &mut resample_buf);
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
        let mut mono_buf: Vec<f32> = Vec::new();
        let mut resample_buf: Vec<f32> = Vec::new();

        let data = vec![0.5f32; 480];
        process_audio_chunk(&data, 48000, 1, &buf, &mut mono_buf, &mut resample_buf);
        let cap_after_first = mono_buf.capacity();

        buf.lock().clear();
        process_audio_chunk(&data, 48000, 1, &buf, &mut mono_buf, &mut resample_buf);
        let cap_after_second = mono_buf.capacity();

        // Capacity should not have changed — buffers are reused
        assert_eq!(cap_after_first, cap_after_second, "Buffer should be reused, not reallocated");
    }
}
