//! The speech port: synthesis as this application needs it, with no engine in it.
//!
//! Dictation listens; this is the other direction. The port exists because the
//! engine behind it has already changed once: Kokoro was evaluated, rejected on
//! Italian, and removed, and the replacement was picked from four candidates
//! that differ in language coverage, licence and runtime. An engine swap must
//! cost an adapter, not a rewrite, so nothing here names one.
//!
//! Three properties the port carries, and why each is here rather than in an
//! adapter:
//!
//! 1. **A voice is an opaque identifier.** Engines disagree about what a voice
//!    is — a named speaker, a checkpoint, an embedding file, a cloned sample.
//!    The port carries the string and lets the adapter resolve it.
//! 2. **Synthesis is cancellable.** A spoken reply that is no longer wanted is
//!    the normal case, not an error case: the user starts talking again. The
//!    caller holds a [`SpeechCancel`] and the adapter checks it as it goes.
//! 3. **Synthesis is bounded.** Generative TTS can run away: in our own
//!    evaluation one candidate produced 195 KB and 675 KB of audio for the same
//!    sentence across four runs — a 3.5x spread on one voice. An adapter that
//!    cannot stop itself hands the user a minute of babble, so [`budget_seconds`]
//!    states the ceiling once, for every adapter.

pub mod assets;
pub mod edge;
pub mod external;
pub mod library;
pub mod pocket;
pub mod rejection;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Rendered audio: mono PCM in `[-1.0, 1.0]` at the engine's own rate.
///
/// The rate is reported rather than fixed because engines differ (24 kHz is
/// common, 22.05 and 16 kHz both occur) and resampling belongs to whoever plays
/// the audio, not to the port.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl SpeechAudio {
    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

/// Why synthesis produced no audio.
///
/// Typed rather than a string because the caller reacts differently to each:
/// a missing model is a setup prompt, an unknown voice is a settings problem,
/// a cancellation is not a failure at all, and a runaway is a defect worth
/// reporting with its ceiling attached.
#[derive(Debug, Clone, PartialEq)]
pub enum SpeechError {
    /// The engine has no voice under that identifier.
    UnknownVoice(String),
    /// The engine needs files this installation does not have.
    ModelUnavailable { what: String, reason: String },
    /// The caller abandoned the request. Not a failure.
    Cancelled,
    /// Generation exceeded what the text can justify and was stopped.
    Runaway { budget_seconds: f32 },
    /// Anything the engine itself reported.
    Failed(String),
    /// The service refused authentication or access; retries must cool down.
    Rejected { status: u16 },
}

impl std::fmt::Display for SpeechError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownVoice(voice) => write!(f, "Unknown voice: {voice}"),
            Self::ModelUnavailable { what, reason } => {
                write!(f, "Speech model unavailable ({what}): {reason}")
            }
            Self::Cancelled => write!(f, "Speech synthesis was cancelled"),
            Self::Runaway { budget_seconds } => write!(
                f,
                "Speech synthesis exceeded its budget of {budget_seconds:.1}s and was stopped"
            ),
            Self::Rejected { status } => write!(
                f,
                "The speech service rejected the request (HTTP {status}); reply in text or choose another speech engine"
            ),
            Self::Failed(reason) => write!(f, "Speech synthesis failed: {reason}"),
        }
    }
}

impl std::error::Error for SpeechError {}

/// A handle the caller keeps to abandon an in-flight request.
///
/// Cloning shares the flag: the caller holds one, the adapter reads another,
/// and neither needs to know which thread the other is on.
#[derive(Debug, Clone, Default)]
pub struct SpeechCancel(Arc<AtomicBool>);

impl SpeechCancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Abandon the request. Idempotent — a second call changes nothing.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Characters of Italian or English read per second, at a deliberately slow
/// estimate. Measured on our own Pocket TTS evaluation samples: 85 characters
/// of Italian rendered as 6.72 s of audio, which is 12.6 characters a second.
/// Rounding down widens the budget rather than narrowing it, and a budget that
/// is too tight truncates real speech — the failure this must not cause.
const CHARS_PER_SECOND: f32 = 10.0;

/// How far past the estimate a healthy engine may still be. Prosody, pauses and
/// emphasis all stretch the same text, so the ceiling is a multiple rather than
/// a margin.
const BUDGET_FACTOR: f32 = 3.0;

/// Floor for very short text, where the estimate is meaningless: "Sì." is three
/// characters and still takes about a second to say with a leading breath.
const BUDGET_FLOOR_SECONDS: f32 = 3.0;

/// The longest audio this text can justify.
///
/// Adapters stop at this ceiling and report [`SpeechError::Runaway`]. It is
/// stated here, once, because it is a property of the text rather than of any
/// engine — and because the engine that needed it was not the engine we chose.
pub fn budget_seconds(text: &str) -> f32 {
    let estimate = text.chars().count() as f32 / CHARS_PER_SECOND * BUDGET_FACTOR;
    estimate.max(BUDGET_FLOOR_SECONDS)
}

/// The port. One method, because one thing is being asked for.
pub trait Speech: Send + Sync {
    /// Render `text` in the voice named by `voice`.
    ///
    /// Implementations check `cancel` as they go and return
    /// [`SpeechError::Cancelled`] rather than finishing the work, and stop at
    /// [`budget_seconds`] rather than returning unbounded audio.
    fn synthesize(
        &self,
        text: &str,
        voice: &str,
        cancel: &SpeechCancel,
    ) -> Result<SpeechAudio, SpeechError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_handle_shares_its_flag_with_its_clones() {
        // The adapter runs on another thread and holds a clone; cancelling the
        // caller's handle has to reach it, or nothing can be abandoned.
        let caller = SpeechCancel::new();
        let adapter = caller.clone();

        assert!(!adapter.is_cancelled());
        caller.cancel();
        assert!(adapter.is_cancelled());
    }

    #[test]
    fn cancelling_twice_is_the_same_as_cancelling_once() {
        let cancel = SpeechCancel::new();
        cancel.cancel();
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn the_budget_never_falls_below_the_floor() {
        // Short replies are the common case in a voice conversation, and a
        // budget proportional to three characters would truncate every one.
        for text in ["", "Sì.", "Ok", "No"] {
            assert!(
                budget_seconds(text) >= BUDGET_FLOOR_SECONDS,
                "{text:?} was budgeted {}s",
                budget_seconds(text)
            );
        }
    }

    #[test]
    fn the_budget_leaves_real_speech_room_to_finish() {
        // The sentence below is one of the evaluation samples: 85 characters
        // rendered as 6.72 s of Italian by the engine we chose. A budget that
        // does not clear that measurement truncates correct output.
        let sentence =
            "Ho lasciato le chiavi sull'erba bagnata vicino all'acqua. La famiglia era in cucina.";
        assert_eq!(sentence.chars().count(), 84);
        assert!(
            budget_seconds(sentence) > 6.72,
            "budget {}s would truncate a measured 6.72s rendering",
            budget_seconds(sentence)
        );
    }

    #[test]
    fn the_budget_still_stops_a_runaway() {
        // The defect this exists for: one candidate engine produced a 3.5x
        // spread on the same sentence across four runs. The ceiling has to be
        // low enough to catch that, or it is decoration.
        let sentence = "Ho lasciato le chiavi sull'erba bagnata vicino all'acqua.";
        let honest = sentence.chars().count() as f32 / CHARS_PER_SECOND;
        assert!(budget_seconds(sentence) < honest * 3.5);
    }

    #[test]
    fn the_budget_grows_with_the_text() {
        let short = budget_seconds("Ho lasciato le chiavi sul tavolo della cucina.");
        let long = budget_seconds(
            "Ho lasciato le chiavi sul tavolo della cucina, vicino ai bicchieri \
             che gli ospiti hanno portato ieri sera dopo cena.",
        );
        assert!(long > short);
    }

    #[test]
    fn audio_reports_its_own_duration() {
        let audio = SpeechAudio {
            samples: vec![0.0; 24_000],
            sample_rate: 24_000,
        };
        assert_eq!(audio.duration_seconds().to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn audio_at_an_unset_rate_reports_no_duration_instead_of_dividing_by_zero() {
        let audio = SpeechAudio {
            samples: vec![0.0; 24_000],
            sample_rate: 0,
        };
        assert_eq!(audio.duration_seconds().to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn every_error_says_which_one_it_is() {
        // These strings reach the user through a toast, so an error that
        // renders as its variant name is a defect the compiler cannot see.
        assert_eq!(
            SpeechError::UnknownVoice("giovanni".into()).to_string(),
            "Unknown voice: giovanni"
        );
        assert_eq!(
            SpeechError::ModelUnavailable {
                what: "italian".into(),
                reason: "directory not found".into()
            }
            .to_string(),
            "Speech model unavailable (italian): directory not found"
        );
        assert_eq!(
            SpeechError::Cancelled.to_string(),
            "Speech synthesis was cancelled"
        );
        assert_eq!(
            SpeechError::Runaway {
                budget_seconds: 17.1
            }
            .to_string(),
            "Speech synthesis exceeded its budget of 17.1s and was stopped"
        );
        assert_eq!(
            SpeechError::Failed("no such tensor".into()).to_string(),
            "Speech synthesis failed: no such tensor"
        );
    }
}
