//! Notification sound playback via the system audio output (rodio).
//!
//! Generates tones natively to bypass WebKit AudioContext restrictions.
//! Each note uses a custom Source with selectable waveform (sine/triangle)
//! and an integrated ADSR amplitude envelope for smooth, click-free playback.

use rodio::{OutputStream, Sink, Source};
use std::time::Duration;

const SAMPLE_RATE: u32 = 48_000;

/// Notification sound types — mirrors the TypeScript `NotificationSound` union.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NotificationSound {
    Question,
    Completion,
    Error,
    Warning,
    Info,
}

/// Waveform shape for tone generation.
#[derive(Debug, Clone, Copy)]
enum Waveform {
    Sine,
    Triangle,
}

/// A single note in a sound sequence.
struct Note {
    frequency: f32,
    duration: Duration,
    waveform: Waveform,
}

/// A sequence of notes with a gap between them.
struct SoundSequence {
    notes: Vec<Note>,
    gap: Duration,
}

/// Get the sound definition for a notification type.
/// Waveform choices match the original Web Audio implementation:
/// sine for melodic sounds, triangle for warmer/softer tones.
fn sound_sequence(sound: NotificationSound) -> SoundSequence {
    match sound {
        // Gentle two-note ascending chime: C5 -> E5
        NotificationSound::Question => SoundSequence {
            notes: vec![
                Note { frequency: 523.0, duration: Duration::from_millis(120), waveform: Waveform::Sine },
                Note { frequency: 659.0, duration: Duration::from_millis(120), waveform: Waveform::Sine },
            ],
            gap: Duration::from_millis(30),
        },
        // Satisfying major triad arpeggio: C5 -> E5 -> G5
        NotificationSound::Completion => SoundSequence {
            notes: vec![
                Note { frequency: 523.0, duration: Duration::from_millis(100), waveform: Waveform::Sine },
                Note { frequency: 659.0, duration: Duration::from_millis(100), waveform: Waveform::Sine },
                Note { frequency: 784.0, duration: Duration::from_millis(100), waveform: Waveform::Sine },
            ],
            gap: Duration::from_millis(30),
        },
        // Low descending minor interval: E4 -> C4 (triangle = warmer)
        NotificationSound::Error => SoundSequence {
            notes: vec![
                Note { frequency: 330.0, duration: Duration::from_millis(150), waveform: Waveform::Triangle },
                Note { frequency: 262.0, duration: Duration::from_millis(150), waveform: Waveform::Triangle },
            ],
            gap: Duration::from_millis(40),
        },
        // Quick double-tap: A4 x 2 (triangle = softer)
        NotificationSound::Warning => SoundSequence {
            notes: vec![
                Note { frequency: 440.0, duration: Duration::from_millis(80), waveform: Waveform::Triangle },
                Note { frequency: 440.0, duration: Duration::from_millis(80), waveform: Waveform::Triangle },
            ],
            gap: Duration::from_millis(60),
        },
        // Soft single pluck: G5
        NotificationSound::Info => SoundSequence {
            notes: vec![
                Note { frequency: 784.0, duration: Duration::from_millis(80), waveform: Waveform::Sine },
            ],
            gap: Duration::ZERO,
        },
    }
}

// ---------------------------------------------------------------------------
// Custom rodio Source: waveform + integrated ADSR envelope
// ---------------------------------------------------------------------------

/// A tone source with selectable waveform and linear attack/release envelope.
///
/// The envelope ramps amplitude from 0 to `volume` over `attack_samples`,
/// holds at `volume` for the sustain region, then ramps down to 0 over
/// `release_samples`. Total duration = attack + sustain + release.
struct EnvelopedTone {
    sample_rate: u32,
    sample_index: u64,
    volume: f32,
    waveform: Waveform,
    attack_samples: u64,
    sustain_end: u64,
    total_samples: u64,
    /// Precomputed: sample_rate / frequency
    period: f32,
}

impl EnvelopedTone {
    fn new(
        frequency: f32,
        duration: Duration,
        volume: f32,
        waveform: Waveform,
    ) -> Self {
        let attack = Duration::from_millis(10);
        let release = Duration::from_millis(30);

        let attack_samples = (attack.as_secs_f64() * SAMPLE_RATE as f64) as u64;
        let release_samples = (release.as_secs_f64() * SAMPLE_RATE as f64) as u64;
        let duration_samples = (duration.as_secs_f64() * SAMPLE_RATE as f64) as u64;

        // Total = note duration + release tail (release extends beyond the note)
        let total_samples = duration_samples + release_samples;
        let sustain_end = duration_samples;

        Self {
            sample_rate: SAMPLE_RATE,
            sample_index: 0,
            volume,
            waveform,
            attack_samples,
            sustain_end,
            total_samples,
            period: SAMPLE_RATE as f32 / frequency,
        }
    }

    /// Compute the amplitude envelope at the current sample position.
    fn envelope(&self) -> f32 {
        let i = self.sample_index;
        if i < self.attack_samples {
            // Linear ramp up: 0 -> 1
            i as f32 / self.attack_samples as f32
        } else if i < self.sustain_end {
            // Full amplitude
            1.0
        } else {
            // Linear ramp down: 1 -> 0
            let release_len = self.total_samples - self.sustain_end;
            if release_len == 0 {
                return 0.0;
            }
            let release_pos = i - self.sustain_end;
            1.0 - (release_pos as f32 / release_len as f32)
        }
    }

    /// Generate one sample of the selected waveform at the given phase.
    fn waveform_sample(&self, phase: f32) -> f32 {
        match self.waveform {
            Waveform::Sine => (std::f32::consts::TAU * phase).sin(),
            // Triangle: rises from -1 to +1 in first half, falls back in second half
            Waveform::Triangle => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
        }
    }
}

impl Iterator for EnvelopedTone {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.sample_index >= self.total_samples {
            return None;
        }

        let phase = (self.sample_index as f32 / self.period).fract();
        let sample = self.waveform_sample(phase);
        let amplitude = self.envelope() * self.volume;

        self.sample_index += 1;
        Some(sample * amplitude)
    }
}

impl Source for EnvelopedTone {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let secs = self.total_samples as f64 / self.sample_rate as f64;
        Some(Duration::from_secs_f64(secs))
    }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// Play a notification sound on a background thread.
///
/// Volume is 0.0-1.0. The function returns immediately; audio plays
/// asynchronously on a short-lived thread.
pub(crate) fn play(sound: NotificationSound, volume: f32) {
    let volume = volume.clamp(0.0, 1.0);
    std::thread::spawn(move || {
        let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
            eprintln!("[notification_sound] Failed to open audio output");
            return;
        };
        let Ok(sink) = Sink::try_new(&stream_handle) else {
            eprintln!("[notification_sound] Failed to create audio sink");
            return;
        };

        let seq = sound_sequence(sound);
        for (i, note) in seq.notes.iter().enumerate() {
            sink.append(EnvelopedTone::new(
                note.frequency,
                note.duration,
                volume,
                note.waveform,
            ));
            // Insert silence gap between notes (not after the last one)
            if i < seq.notes.len() - 1 && !seq.gap.is_zero() {
                sink.append(
                    rodio::source::Zero::<f32>::new(1, SAMPLE_RATE)
                        .take_duration(seq.gap),
                );
            }
        }

        sink.sleep_until_end();
    });
}

/// Tauri command: play a notification sound.
#[tauri::command]
pub(crate) fn play_notification_sound(sound: NotificationSound, volume: f32) {
    play(sound, volume);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sound_types_produce_sequences() {
        let types = [
            NotificationSound::Question,
            NotificationSound::Completion,
            NotificationSound::Error,
            NotificationSound::Warning,
            NotificationSound::Info,
        ];
        for sound in types {
            let seq = sound_sequence(sound);
            assert!(!seq.notes.is_empty(), "{sound:?} should have at least one note");
            for note in &seq.notes {
                assert!(note.frequency > 0.0, "{sound:?} note frequency must be positive");
                assert!(!note.duration.is_zero(), "{sound:?} note duration must be non-zero");
            }
        }
    }

    #[test]
    fn volume_is_clamped() {
        // Verify extreme volumes don't panic
        let _ = EnvelopedTone::new(440.0, Duration::from_millis(100), 0.0, Waveform::Sine);
        let _ = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Triangle);
        let _ = EnvelopedTone::new(440.0, Duration::from_millis(100), 2.0, Waveform::Sine);
    }

    #[test]
    fn envelope_attack_ramps_up() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Sine);
        // At sample 0, envelope should be 0
        assert_eq!(tone.envelope(), 0.0);
        // At half of attack (attack = 10ms = 480 samples at 48kHz)
        let mut tone = tone;
        tone.sample_index = 240;
        let env = tone.envelope();
        assert!((env - 0.5).abs() < 0.01, "Expected ~0.5 at half-attack, got {env}");
    }

    #[test]
    fn envelope_sustain_is_full() {
        let mut tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Sine);
        // After attack ends (480 samples), envelope should be 1.0
        tone.sample_index = 500;
        assert_eq!(tone.envelope(), 1.0);
    }

    #[test]
    fn envelope_release_ramps_down() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 1.0, Waveform::Sine);
        let sustain_end = tone.sustain_end;
        let total = tone.total_samples;
        let release_mid = sustain_end + (total - sustain_end) / 2;

        let mut tone = tone;
        tone.sample_index = release_mid;
        let env = tone.envelope();
        assert!((env - 0.5).abs() < 0.02, "Expected ~0.5 at mid-release, got {env}");

        // At the very end, should be ~0
        tone.sample_index = total - 1;
        let env = tone.envelope();
        assert!(env < 0.05, "Expected ~0 at end of release, got {env}");
    }

    #[test]
    fn source_produces_correct_sample_count() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(100), 0.5, Waveform::Sine);
        let expected = tone.total_samples as usize;
        let count = tone.count(); // consumes iterator
        assert_eq!(count, expected);
    }

    #[test]
    fn triangle_waveform_range() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(50), 1.0, Waveform::Triangle);
        // Collect all samples and verify they're in [-1, 1] (before volume scaling)
        for sample in tone {
            assert!(
                sample >= -1.0 && sample <= 1.0,
                "Triangle sample out of range: {sample}"
            );
        }
    }

    #[test]
    fn sine_waveform_range() {
        let tone = EnvelopedTone::new(440.0, Duration::from_millis(50), 1.0, Waveform::Sine);
        for sample in tone {
            assert!(
                sample >= -1.0 && sample <= 1.0,
                "Sine sample out of range: {sample}"
            );
        }
    }

    #[test]
    fn error_and_warning_use_triangle() {
        let error_seq = sound_sequence(NotificationSound::Error);
        let warning_seq = sound_sequence(NotificationSound::Warning);
        for note in &error_seq.notes {
            assert!(matches!(note.waveform, Waveform::Triangle), "Error notes should use triangle");
        }
        for note in &warning_seq.notes {
            assert!(matches!(note.waveform, Waveform::Triangle), "Warning notes should use triangle");
        }
    }

    #[test]
    fn question_completion_info_use_sine() {
        let types = [
            NotificationSound::Question,
            NotificationSound::Completion,
            NotificationSound::Info,
        ];
        for sound in types {
            let seq = sound_sequence(sound);
            for note in &seq.notes {
                assert!(matches!(note.waveform, Waveform::Sine), "{sound:?} should use sine");
            }
        }
    }
}
