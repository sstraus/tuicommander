use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::Serialize;
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
pub struct AudioCapture {
    buffer: Arc<Mutex<Vec<f32>>>,
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
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let buffer_clone = buffer.clone();

        // Build a stream that collects f32 samples, converting to 16kHz mono
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

    /// Stop capturing and return the collected audio as 16kHz mono f32 PCM.
    pub fn stop(mut self) -> Vec<f32> {
        // Drop the stream to stop capturing
        self.stream.take();
        let buffer = self.buffer.lock();
        buffer.clone()
    }

}

/// Process an audio chunk: convert to mono and resample to 16kHz.
fn process_audio_chunk(
    data: &[f32],
    sample_rate: u32,
    channels: usize,
    buffer: &Arc<Mutex<Vec<f32>>>,
) {
    // Convert to mono by averaging channels
    let mono: Vec<f32> = data
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    // Simple nearest-neighbor resampling to 16kHz
    // For production quality, consider using a proper resampling library
    if sample_rate == 16000 {
        buffer.lock().extend_from_slice(&mono);
    } else {
        let ratio = 16000.0 / f64::from(sample_rate);
        let output_len = (mono.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src_idx = (i as f64 / ratio) as usize;
            if src_idx < mono.len() {
                resampled.push(mono[src_idx]);
            }
        }
        buffer.lock().extend_from_slice(&resampled);
    }
}
