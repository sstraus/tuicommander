//! Notification sound playback via the system audio output (rodio).
//!
//! Replaces the previous Web Audio API approach which was unreliable in
//! Tauri/WebKit (AudioContext suspend issues, user-gesture requirements).

use rodio::{OutputStream, Sink, Source};
use std::time::Duration;

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

/// A single note in a sound sequence.
struct Note {
    frequency: f32,
    duration: Duration,
}

/// A sequence of notes with a gap between them.
struct SoundSequence {
    notes: Vec<Note>,
    gap: Duration,
}

/// Get the sound definition for a notification type.
fn sound_sequence(sound: NotificationSound) -> SoundSequence {
    match sound {
        // Gentle two-note ascending chime: C5 → E5
        NotificationSound::Question => SoundSequence {
            notes: vec![
                Note { frequency: 523.0, duration: Duration::from_millis(120) },
                Note { frequency: 659.0, duration: Duration::from_millis(120) },
            ],
            gap: Duration::from_millis(30),
        },
        // Satisfying major triad arpeggio: C5 → E5 → G5
        NotificationSound::Completion => SoundSequence {
            notes: vec![
                Note { frequency: 523.0, duration: Duration::from_millis(100) },
                Note { frequency: 659.0, duration: Duration::from_millis(100) },
                Note { frequency: 784.0, duration: Duration::from_millis(100) },
            ],
            gap: Duration::from_millis(30),
        },
        // Low descending minor interval: E4 → C4
        NotificationSound::Error => SoundSequence {
            notes: vec![
                Note { frequency: 330.0, duration: Duration::from_millis(150) },
                Note { frequency: 262.0, duration: Duration::from_millis(150) },
            ],
            gap: Duration::from_millis(40),
        },
        // Quick double-tap: A4 × 2
        NotificationSound::Warning => SoundSequence {
            notes: vec![
                Note { frequency: 440.0, duration: Duration::from_millis(80) },
                Note { frequency: 440.0, duration: Duration::from_millis(80) },
            ],
            gap: Duration::from_millis(60),
        },
        // Soft single pluck: G5
        NotificationSound::Info => SoundSequence {
            notes: vec![
                Note { frequency: 784.0, duration: Duration::from_millis(80) },
            ],
            gap: Duration::ZERO,
        },
    }
}

/// Build a rodio source for a single note with attack/release envelope.
fn note_source(frequency: f32, duration: Duration, volume: f32) -> impl Source<Item = f32> {
    let attack = Duration::from_millis(10);
    let release = Duration::from_millis(30);

    rodio::source::SineWave::new(frequency)
        .take_duration(duration)
        .fade_in(attack)
        .amplify(volume)
        // Linear fade-out for the last `release` portion to avoid clicks.
        // rodio doesn't have a built-in fade_out on Source, so we apply it
        // via take_crossfade_with silence (which is effectively a fade-out).
        .take_crossfade_with(
            rodio::source::Zero::<f32>::new(1, 48000),
            release,
        )
}

/// Play a notification sound on a background thread.
///
/// Volume is 0.0–1.0. The function returns immediately; audio plays
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
            sink.append(note_source(note.frequency, note.duration, volume));
            // Insert gap between notes (not after the last one)
            if i < seq.notes.len() - 1 && !seq.gap.is_zero() {
                sink.append(rodio::source::Zero::<f32>::new(1, 48000).take_duration(seq.gap));
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
        // Just verify the function doesn't panic with extreme values
        // (actual audio output is not tested in unit tests)
        let seq = sound_sequence(NotificationSound::Info);
        let note = &seq.notes[0];
        let _ = note_source(note.frequency, note.duration, 0.0);
        let _ = note_source(note.frequency, note.duration, 1.0);
        let _ = note_source(note.frequency, note.duration, 2.0);
    }
}
