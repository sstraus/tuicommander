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

// Safety: AudioCapture is only accessed through parking_lot::Mutex in DictationState.
// cpal::Stream is !Send due to internal raw pointers, but we never move the stream
// across threads — it lives in one place behind a Mutex and is dropped in-place.
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
                    |err| eprintln!("[dictation] audio stream error: {err}"),
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
                    |err| eprintln!("[dictation] audio stream error: {err}"),
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
    // Convert to mono by averaging channels
    let mono: Vec<f32> = data
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    // Simple nearest-neighbor resampling to 16kHz
    let resampled = if sample_rate == 16000 {
        mono
    } else {
        let ratio = 16000.0 / f64::from(sample_rate);
        let output_len = (mono.len() as f64 * ratio) as usize;
        let mut out = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src_idx = (i as f64 / ratio) as usize;
            if src_idx < mono.len() {
                out.push(mono[src_idx]);
            }
        }
        out
    };

    // try_lock: never block the audio callback
    if let Some(mut buf) = buffer.try_lock() {
        buf.extend(resampled.iter());
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
}
