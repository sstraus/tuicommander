use parking_lot::Mutex;
use serde::Serialize;
use std::path::Path;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

/// True when a GPU backend is compiled into this build.
/// - macOS: Metal (always, via target-specific dep).
/// - Windows: Vulkan (always, via target-specific dep).
/// - Linux: when `cuda` or `vulkan` feature is explicitly enabled.
///
/// Used only for `n_threads` tuning — `use_gpu(true)` is always set regardless,
/// because whisper.cpp falls back to CPU gracefully when no GPU is available.
pub(super) const GPU_COMPILED: bool = cfg!(any(
    target_os = "macos",
    target_os = "windows",
    feature = "cuda",
    feature = "vulkan",
));

/// Optimal n_threads: 1 when a GPU backend is compiled in (GPU handles compute), 4 otherwise.
pub(super) fn optimal_n_threads() -> i32 {
    if GPU_COMPILED { 1 } else { 4 }
}

/// Backend label for logging and frontend events.
/// Always "gpu" because `use_gpu(true)` is set unconditionally — whisper.cpp
/// attempts GPU first and falls back to CPU transparently.
pub fn backend_label() -> &'static str {
    "gpu"
}

/// Build WhisperContextParameters with GPU always preferred.
/// whisper.cpp attempts GPU first and falls back to CPU gracefully if unavailable.
pub(super) fn build_context_params() -> WhisperContextParameters<'static> {
    let mut params = WhisperContextParameters::new();
    params.use_gpu(true);
    params
}

/// Result of a transcription attempt with metadata for user feedback.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResult {
    /// The transcribed text (empty if skipped/filtered).
    pub text: String,
    /// Human-readable reason when text is empty (None when transcription succeeded).
    pub skip_reason: Option<String>,
    /// The language this was recognised as, as a two-letter code.
    ///
    /// The requested one when the caller named a language, and the one Whisper
    /// chose when it asked for `auto` — the same field either way, because
    /// everything downstream needs "what language is this turn", not "who
    /// decided". `None` when nothing was transcribed, and when whisper reports
    /// an id that maps to no code.
    ///
    /// There is no confidence with it. `whisper_full_lang_id_from_state` hands
    /// back the id and nothing else, so an ambiguous detection is
    /// indistinguishable from a certain one on this path; the probabilities
    /// live behind `WhisperState::lang_detect`, which needs its own mel and
    /// encoder pass and would double the cost of every utterance. Treat a
    /// `Some` as "this is what whisper decided", not as "this is certainly the
    /// language" — a caller that needs a threshold has to pay for that pass.
    pub language: Option<String>,
}

impl TranscribeResult {
    /// Nothing was transcribed, and here is why.
    pub(super) fn skipped(reason: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            skip_reason: Some(reason.into()),
            language: None,
        }
    }

    /// Somebody said this, in this language.
    pub(super) fn heard(text: String, language: Option<String>) -> Self {
        Self {
            text,
            skip_reason: None,
            language,
        }
    }
}

/// The language whisper used for the pass that just ran.
///
/// Reads the id off the state rather than echoing the requested language: with
/// `auto` there is nothing to echo, and with a fixed language the two agree, so
/// one source answers both cases.
fn language_of_last_pass(state: &WhisperState) -> Option<String> {
    let id = state.full_lang_id_from_state();
    whisper_rs::get_lang_str(id).map(str::to_string)
}

/// The two thresholds that decide whether captured audio is speech at all.
///
/// They are settings rather than constants because the right value depends on
/// the room and the microphone: a headset a metre away picks up enough noise to
/// clear a fixed floor, which is exactly how Whisper ends up transcribing an
/// empty room. Settings > Voice exposes both with a live meter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceGates {
    /// Minimum RMS of the captured audio. Below it, the audio never reaches
    /// Whisper at all.
    pub rms_threshold: f32,
    /// Highest `no_speech_probability` a segment may report before its text is
    /// discarded. `1.0` disables the gate — no probability can exceed it.
    pub no_speech_threshold: f32,
}

impl Default for VoiceGates {
    fn default() -> Self {
        Self {
            rms_threshold: DEFAULT_RMS_THRESHOLD,
            no_speech_threshold: DEFAULT_NO_SPEECH_THRESHOLD,
        }
    }
}

pub use tuic_core::voice_defaults::{DEFAULT_NO_SPEECH_THRESHOLD, DEFAULT_RMS_THRESHOLD};

/// One whisper encoder window, in samples at 16 kHz (`WHISPER_CHUNK_SIZE` is
/// 30 s). The two streaming flags are safe at or under this length and lossy
/// above it, which is the whole reason [`decode_flags_for`] exists.
const SINGLE_WINDOW_SAMPLES: usize = 30 * 16_000;

/// The two `FullParams` flags that suit a streaming window and ruin a long
/// recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DecodeFlags {
    pub single_segment: bool,
    pub no_timestamps: bool,
}

/// Pick the decode flags for a buffer of `n_samples`.
///
/// `no_timestamps` suppresses every timestamp token outright
/// (`whisper.cpp:6191`: `for (int i = vocab.token_beg; i < n_logits; ++i)
/// logits[i] = -INFINITY;`), so `has_ts` never becomes true. With either flag
/// set, the end of a segment then forces the window shift to a whole chunk
/// (`whisper.cpp:7381`: `if (params.single_segment || params.no_timestamps) {
/// result_len = i + 1; seek_delta = 100*WHISPER_CHUNK_SIZE; }`) and
/// `seek += seek_delta` (`whisper.cpp:7734`) advances a full 30 s no matter how
/// much the decoder actually reached. An early end-of-text token or the
/// 220-token decode limit (`whisper.cpp:7184`) therefore drops the rest of that
/// window for good — whisper cannot re-seek to it, because with timestamps on it
/// would have advanced only to the last decoded timestamp.
///
/// At or under one window that cannot happen: the loop breaks once `seek`
/// reaches the end of the audio (`whisper.cpp:7008`), so nothing follows the
/// first window to be lost. Short dictation keeps both flags, which is where the
/// hallucination suppression they were added for was measured
/// (whisper.cpp issue 1724).
pub(super) fn decode_flags_for(n_samples: usize) -> DecodeFlags {
    let within_one_window = n_samples <= SINGLE_WINDOW_SAMPLES;
    DecodeFlags {
        single_segment: within_one_window,
        no_timestamps: within_one_window,
    }
}

/// One decoded segment with Whisper's own answer to "was anyone speaking?".
#[derive(Debug, Clone)]
pub(super) struct ScoredSegment {
    pub text: String,
    pub no_speech_probability: f32,
}

impl ScoredSegment {
    #[cfg(test)]
    pub(super) fn new(text: &str, no_speech_probability: f32) -> Self {
        Self {
            text: text.to_string(),
            no_speech_probability,
        }
    }
}

/// What the per-segment no-speech gate decided about a run.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SegmentFilter {
    /// The text of the segments Whisper scored as speech. Empty when the run
    /// decoded no segments at all — that is not a rejection, and the caller
    /// already has its own message for it.
    Speech(String),
    /// Every segment scored above the threshold, carrying the worst score so the
    /// skip reason can name it.
    AllNoSpeech(f32),
}

/// Keep the segments Whisper scored as speech and drop the rest.
///
/// The gate used to take the worst score across the whole run, which was
/// harmless while the run was one segment. A recording longer than one window
/// decodes into many, and an ordinary pause inside a long dictation scores as
/// no-speech — so one silent segment discarded a transcript that was almost
/// entirely speech. The gate is per segment; only a run with nothing but
/// no-speech segments is rejected outright.
pub(super) fn filter_speech_segments(
    segments: &[ScoredSegment],
    no_speech_threshold: f32,
) -> SegmentFilter {
    let mut kept = String::new();
    let mut worst_rejected = 0.0f32;
    let mut rejected_any = false;

    for segment in segments {
        if segment.no_speech_probability > no_speech_threshold {
            worst_rejected = worst_rejected.max(segment.no_speech_probability);
            rejected_any = true;
            continue;
        }
        kept.push_str(&segment.text);
    }

    let kept = kept.trim().to_string();
    if kept.is_empty() && rejected_any {
        return SegmentFilter::AllNoSpeech(worst_rejected);
    }
    SegmentFilter::Speech(kept)
}

/// Trait for transcription, enabling mock implementations in tests.
pub trait Transcriber: Send + Sync {
    fn transcribe(
        &self,
        audio: &[f32],
        language: Option<&str>,
        gates: VoiceGates,
    ) -> Result<TranscribeResult, String>;
}

/// Whisper model wrapper for transcription.
pub struct WhisperTranscriber {
    /// Decoder state, created once at load and reused for every transcription.
    ///
    /// `create_state` allocates the decoder's KV cache and mel buffers, which
    /// the streaming loop otherwise paid on every 1.5–3 s window.
    /// `whisper_full_with_state` resets the state at the start of each run, so
    /// reuse is exactly what whisper.cpp's own streaming example does.
    ///
    /// The mutex is only there to satisfy `full()`'s `&mut self`; it never
    /// contends, because the streaming thread is joined before the final pass
    /// runs. The state owns an `Arc` to the loaded model, so the model lives as
    /// long as this transcriber.
    state: Mutex<WhisperState>,
}

impl WhisperTranscriber {
    /// Load a Whisper GGML model from disk.
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let path_str = model_path.to_str().ok_or("Invalid model path (non-UTF8)")?;

        let params = build_context_params();
        let backend = backend_label();
        tracing::info!(
            backend,
            n_threads = optimal_n_threads(),
            "Loading Whisper model"
        );

        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| format!("Failed to load Whisper model: {e}"))?;
        let state = ctx
            .create_state()
            .map_err(|e| format!("Failed to create Whisper state: {e}"))?;

        tracing::info!(backend, "Whisper model loaded");
        Ok(Self {
            state: Mutex::new(state),
        })
    }
}

impl Transcriber for WhisperTranscriber {
    /// Transcribe audio samples (16kHz mono f32 PCM) to text.
    fn transcribe(
        &self,
        audio: &[f32],
        language: Option<&str>,
        gates: VoiceGates,
    ) -> Result<TranscribeResult, String> {
        if audio.is_empty() {
            return Ok(TranscribeResult::skipped("no audio captured"));
        }

        let duration_s = audio.len() as f64 / 16000.0;

        // Minimum 0.5s of audio (8000 samples at 16kHz)
        if audio.len() < 8000 {
            return Ok(TranscribeResult::skipped(format!(
                "too short ({duration_s:.1}s, need 0.5s)"
            )));
        }

        // Reject silent/near-silent audio to prevent hallucinations.
        // Whisper hallucinates phrases like "Thank you" on silence.
        let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt();
        if rms < gates.rms_threshold {
            let floor = gates.rms_threshold;
            return Ok(TranscribeResult::skipped(format!(
                "no speech detected (RMS {rms:.6} < {floor:.6})"
            )));
        }

        let mut state = self.state.lock();

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_n_threads(optimal_n_threads());
        // Suppress non-speech tokens for cleaner output
        params.set_suppress_nst(true);
        // Both flags suit a single streaming window and silently drop the tail
        // of every window above one — see `decode_flags_for`. `no_timestamps` is
        // also the primary fix for hallucination on silence, which is why a
        // recording that fits one window keeps it.
        // See: https://github.com/ggml-org/whisper.cpp/issues/1724
        let flags = decode_flags_for(audio.len());
        params.set_no_timestamps(flags.no_timestamps);
        params.set_single_segment(flags.single_segment);

        state
            .full(params, audio)
            .map_err(|e| format!("Transcription failed: {e}"))?;

        let n_segments = state.full_n_segments();
        // Whisper's own answer to "was anyone speaking?", read per segment. It
        // generalises where a phrase list cannot: it rejects whatever the model
        // invents on room noise, not only the wordings someone remembered to add
        // to HALLUCINATION_EXACT.
        let mut segments = Vec::with_capacity(n_segments as usize);

        for i in 0..n_segments {
            if let Some(segment) = state.get_segment(i) {
                segments.push(ScoredSegment {
                    text: segment.to_str().unwrap_or_default().to_string(),
                    no_speech_probability: segment.no_speech_probability(),
                });
            }
        }

        let result = match filter_speech_segments(&segments, gates.no_speech_threshold) {
            SegmentFilter::Speech(text) => text,
            SegmentFilter::AllNoSpeech(worst_no_speech) => {
                let thold = gates.no_speech_threshold;
                return Ok(TranscribeResult::skipped(format!(
                    "no speech detected (no_speech {worst_no_speech:.2} > {thold:.2})"
                )));
            }
        };

        // Filter known hallucination phrases that Whisper produces on near-silence
        if is_hallucination(&result) {
            return Ok(TranscribeResult::skipped(format!(
                "filtered hallucination: \"{result}\""
            )));
        }

        Ok(TranscribeResult::heard(
            result,
            language_of_last_pass(&state),
        ))
    }
}

/// Known hallucination phrases Whisper produces on silence/noise.
/// These are artifacts of YouTube subtitle training data, so they come in the
/// language Whisper was told to transcribe: quiet Italian audio yields a bare
/// "Grazie.", quiet English audio a bare "Thank you.".
///
/// Two lists, because the two shapes need different matching:
///
/// - [`HALLUCINATION_EXACT`] holds words a person genuinely dictates ("grazie",
///   "thank you"). Substring matching would delete a real sentence that merely
///   contains one, so these only count when they ARE the whole transcript.
/// - [`HALLUCINATION_SUBSTRING`] holds channel boilerplate nobody dictates into
///   a terminal; it may appear anywhere in the text.
///
/// Both lists cover every language offered in `WHISPER_LANGUAGES`, because the
/// default setting is `auto`: the language of the hallucination is whatever
/// Whisper decided the silence was.
const HALLUCINATION_EXACT: &[&str] = &[
    // en
    "thank you",
    "thanks",
    "thank you very much",
    // es
    "gracias",
    "muchas gracias",
    // fr
    "merci",
    "merci beaucoup",
    // de
    "danke",
    "danke schön",
    "vielen dank",
    // it
    "grazie",
    "grazie mille",
    "grazie a tutti",
    // pt
    "obrigado",
    "obrigada",
    // nl
    "bedankt",
    "dank je wel",
    // ja
    "ご視聴ありがとうございました",
    // zh
    "谢谢观看",
    "谢谢大家",
    // ko
    "감사합니다",
    "시청해주셔서 감사합니다",
    // ru
    "спасибо",
    "спасибо за просмотр",
];

const HALLUCINATION_SUBSTRING: &[&str] = &[
    // The Amara subtitle credit is the single most common one and appears
    // translated into every language, so match the domain and cover them all.
    "amara.org",
    // en
    "thanks for watching",
    "thanks for listening",
    "subtitles by",
    "transcribed by",
    // "subscribe" on its own is a verb this codebase dictates constantly
    // ("add a subscribe handler", "unsubscribe the grid"), and substring
    // matching would condemn the whole sentence. The boilerplate form is
    // already covered by the entry below.
    "like and subscribe",
    // es
    "gracias por ver el video",
    "subtítulos realizados por",
    // fr
    "sous-titres réalisés par",
    "merci d'avoir regardé cette vidéo",
    "sous-titrage société radio-canada",
    // de
    "untertitel der",
    "untertitelung im auftrag des",
    // it
    "grazie per aver guardato il video",
    "sottotitoli e revisione a cura di",
    // pt
    "legendas pela comunidade",
    "obrigado por assistir",
    // nl
    "ondertiteld door",
    // zh
    "请不吝点赞",
    // ko
    "시청해주셔서",
    // ru
    "субтитры сделал",
    "редактор субтитров",
];

/// Sentence terminators Whisper emits, ASCII and CJK. Newline included because
/// a looping decode returns one segment per line.
const SENTENCE_ENDS: &[char] = &['.', '!', '?', '\n', '…', '。', '！', '？'];

fn is_hallucination(text: &str) -> bool {
    let lower = text.to_lowercase();

    // Channel boilerplate is not something anyone dictates into a terminal, so
    // one occurrence anywhere condemns the whole transcript.
    if HALLUCINATION_SUBSTRING.iter().any(|h| lower.contains(h)) {
        return true;
    }

    // The short phrases ARE dictated on purpose ("grazie, ora committa"), so
    // they only count when they are the WHOLE transcript. On several seconds of
    // noise Whisper loops instead of emitting one bare word, so "the whole
    // transcript" has to mean every sentence — comparing the trimmed string as a
    // single unit let "Grazie. Grazie." through, which is the form the final
    // full-buffer pass actually produces.
    let mut saw_sentence = false;
    for sentence in lower.split(SENTENCE_ENDS) {
        // Whisper punctuates its hallucinations; compare against the bare words.
        let bare = sentence.trim_matches(|c: char| !c.is_alphanumeric());
        if bare.is_empty() {
            continue;
        }
        if !HALLUCINATION_EXACT.contains(&bare) {
            return false;
        }
        saw_sentence = true;
    }
    saw_sentence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimal_n_threads_consistent_with_gpu_compiled() {
        let threads = optimal_n_threads();
        if GPU_COMPILED {
            assert_eq!(threads, 1, "GPU compiled: n_threads should be 1");
        } else {
            assert_eq!(threads, 4, "CPU only: n_threads should be 4");
        }
    }

    #[test]
    fn bare_thanks_in_any_language_is_a_hallucination() {
        // What quiet audio actually produces, punctuation and casing included.
        assert!(is_hallucination("Grazie."));
        assert!(is_hallucination("grazie"));
        assert!(is_hallucination("Grazie mille!"));
        assert!(is_hallucination("Thank you."));
        assert!(is_hallucination("  Thanks!  "));
        assert!(is_hallucination("Gracias."));
        assert!(is_hallucination("Merci."));
        assert!(is_hallucination("Vielen Dank!"));
        assert!(is_hallucination("Obrigado."));
        assert!(is_hallucination("Bedankt."));
        assert!(is_hallucination("Спасибо."));
        assert!(is_hallucination("ご視聴ありがとうございました。"));
        assert!(is_hallucination("谢谢观看"));
        assert!(is_hallucination("감사합니다."));
    }

    /// A bare subtitle sign-off must not reach the composer, but the same
    /// words inside a real instruction must remain available to dictate.
    #[test]
    fn bare_grazie_a_tutti_is_filtered_without_eating_an_instruction() {
        // — catches: the missing phrase or a substring match that erases speech.
        assert!(is_hallucination("Grazie a tutti."));
        assert!(is_hallucination("Grazie a tutti! Grazie a tutti."));
        assert!(!is_hallucination("Scrivi grazie a tutti nel messaggio."));
    }

    #[test]
    fn the_amara_credit_is_caught_in_every_language() {
        // One pattern, every translation of the same subtitle credit.
        assert!(is_hallucination(
            "Sottotitoli creati dalla comunità Amara.org"
        ));
        assert!(is_hallucination("Subtitles by the Amara.org community"));
        assert!(is_hallucination("Untertitel der Amara.org-Community"));
        assert!(is_hallucination(
            "Sous-titres réalisés par la communauté d'Amara.org"
        ));
    }

    #[test]
    fn channel_boilerplate_is_a_hallucination_anywhere_in_the_text() {
        assert!(is_hallucination(
            "Grazie per aver guardato il video, ci vediamo alla prossima"
        ));
        assert!(is_hallucination("Sottotitoli e revisione a cura di QTSS"));
        assert!(is_hallucination("Thanks for watching!"));
    }

    #[test]
    fn a_real_sentence_containing_thanks_survives() {
        // The whole reason the short phrases are matched exactly: these are
        // things Boss dictates on purpose.
        assert!(!is_hallucination("grazie, ora committa e pusha"));
        assert!(!is_hallucination("thank you for the review, apply it"));
        assert!(!is_hallucination("scrivi grazie nel commento"));
    }

    #[test]
    fn ordinary_dictation_is_not_filtered() {
        assert!(!is_hallucination("apri il file browser"));
        assert!(!is_hallucination("run the tests"));
        assert!(!is_hallucination(""));
    }

    /// "subscribe" is a verb this codebase dictates constantly, so it may only
    /// be matched in the channel-boilerplate form. A bare substring entry made
    /// every one of these sentences vanish with a "filtered hallucination" skip.
    #[test]
    fn a_sentence_about_subscribing_survives() {
        assert!(!is_hallucination(
            "add a subscribe handler to the event bus"
        ));
        assert!(!is_hallucination("unsubscribe the terminal grid on close"));
        assert!(!is_hallucination("chat_subscribe returns a receiver"));
        // The boilerplate form must still go.
        assert!(is_hallucination("Please like and subscribe!"));
    }

    /// The short-phrase list was written from what a 1.5–3 s streaming window
    /// produces: one bare "Grazie.". The final pass runs on the WHOLE recording,
    /// and on several seconds of room noise Whisper repeats itself instead.
    /// The repeated form is the one that reaches the terminal.
    #[test]
    fn a_repeated_bare_thanks_is_still_a_hallucination() {
        assert!(is_hallucination("Grazie. Grazie."));
        assert!(is_hallucination("Grazie. Grazie. Grazie."));
        assert!(is_hallucination("Thank you. Thank you."));
        assert!(is_hallucination("Grazie! Grazie..."));
        // Whisper emits one segment per line when it loops on noise.
        assert!(is_hallucination("Grazie.\nGrazie."));
        // `language = "auto"` is decided per window, so a noise-only recording
        // can come back in two languages at once.
        assert!(is_hallucination("Grazie. Thank you."));
    }

    /// The boundary of the repetition rule: filtering needs EVERY sentence to be
    /// boilerplate. One real instruction in the recording makes the whole
    /// transcript real — dropping it would eat dictation Boss meant to send.
    #[test]
    fn a_thanks_followed_by_a_real_instruction_survives() {
        assert!(!is_hallucination("Grazie. Ora committa e pusha."));
        assert!(!is_hallucination("Thank you. Now run the tests."));
    }

    /// A recording that fits one 30 s whisper window cannot lose audio to the
    /// streaming flags, because `seek` never advances past the end of the audio.
    /// Above one window it can, so the flags must go.
    #[test]
    fn a_recording_within_one_whisper_window_keeps_the_streaming_flags() {
        let flags = decode_flags_for(SINGLE_WINDOW_SAMPLES);
        assert!(flags.single_segment);
        assert!(flags.no_timestamps);

        // A short dictation — the case the hallucination suppression was tuned for.
        let flags = decode_flags_for(16_000);
        assert!(flags.single_segment);
        assert!(flags.no_timestamps);
    }

    /// One sample past the window is enough: with the flags set, whisper forces
    /// `seek_delta` to a full 30 s and skips whatever the decoder did not reach.
    #[test]
    fn a_recording_past_one_whisper_window_clears_the_streaming_flags() {
        let flags = decode_flags_for(SINGLE_WINDOW_SAMPLES + 1);
        assert!(!flags.single_segment);
        assert!(!flags.no_timestamps);

        // The 127 s recording that lost its window tails.
        let flags = decode_flags_for(127 * 16_000);
        assert!(!flags.single_segment);
        assert!(!flags.no_timestamps);
    }

    /// A long dictation decodes into many segments, and a pause inside it scores
    /// as no-speech. Taking the worst score across all of them discarded the
    /// whole transcript over one silent segment.
    #[test]
    fn a_no_speech_segment_inside_a_long_dictation_drops_only_itself() {
        let segments = [
            ScoredSegment::new(" run the tests", 0.05),
            ScoredSegment::new(" Thank you.", 0.95),
            ScoredSegment::new(" then commit", 0.10),
        ];

        match filter_speech_segments(&segments, DEFAULT_NO_SPEECH_THRESHOLD) {
            SegmentFilter::Speech(text) => assert_eq!(text, "run the tests then commit"),
            SegmentFilter::AllNoSpeech(worst) => panic!("speech segments were dropped ({worst})"),
        }
    }

    /// The gate still has to fire when nothing was speech, and still has to name
    /// the worst score — that value is what the UI shows the user.
    #[test]
    fn a_recording_with_no_speech_in_any_segment_is_still_skipped() {
        let segments = [
            ScoredSegment::new(" Grazie.", 0.72),
            ScoredSegment::new(" Grazie.", 0.91),
        ];

        match filter_speech_segments(&segments, DEFAULT_NO_SPEECH_THRESHOLD) {
            SegmentFilter::AllNoSpeech(worst) => {
                assert!((worst - 0.91).abs() < 1e-6, "worst score reported: {worst}");
            }
            SegmentFilter::Speech(text) => panic!("no-speech audio produced text: {text:?}"),
        }
    }

    /// A run that decoded nothing is not a no-speech rejection: it has no score
    /// to report, and the caller already turns empty text into its own message.
    #[test]
    fn a_run_with_no_segments_reports_empty_text_rather_than_no_speech() {
        match filter_speech_segments(&[], DEFAULT_NO_SPEECH_THRESHOLD) {
            SegmentFilter::Speech(text) => assert_eq!(text, ""),
            SegmentFilter::AllNoSpeech(worst) => panic!("empty run scored as no-speech ({worst})"),
        }
    }

    /// The threshold is a setting, so the filter must honour the value it is
    /// given rather than the default.
    #[test]
    fn the_per_segment_filter_uses_the_threshold_it_is_given() {
        let segments = [ScoredSegment::new(" run the tests", 0.5)];

        match filter_speech_segments(&segments, 1.0) {
            SegmentFilter::Speech(text) => assert_eq!(text, "run the tests"),
            SegmentFilter::AllNoSpeech(_) => panic!("1.0 disables the gate"),
        }
        match filter_speech_segments(&segments, 0.4) {
            SegmentFilter::AllNoSpeech(worst) => assert!((worst - 0.5).abs() < 1e-6),
            SegmentFilter::Speech(_) => panic!("a lowered threshold must still reject"),
        }
    }

    /// Reusing one decoder state across windows is only safe if
    /// `whisper_full_with_state` really resets it per run. Transcribing the same
    /// audio twice must therefore give the same text — greedy sampling is
    /// deterministic, so any drift means the previous run leaked into this one.
    ///
    /// Ignored by default: it needs the ~1.6 GB model on disk. Run with
    /// `cargo nextest run --run-ignored all decoder_state`.
    #[test]
    #[ignore = "requires a downloaded whisper model"]
    fn a_reused_decoder_state_gives_identical_results_across_calls() {
        use std::f32::consts::PI;

        let path = crate::model::model_path(crate::model::WhisperModel::LargeV3Turbo);
        let transcriber = WhisperTranscriber::load(&path).expect("model load");

        // Two seconds of tone: the text is irrelevant, its stability is not.
        let audio: Vec<f32> = (0..32_000)
            .map(|i| (2.0 * PI * 220.0 * i as f32 / 16_000.0).sin() * 0.3)
            .collect();

        let first = transcriber
            .transcribe(&audio, Some("en"), VoiceGates::default())
            .expect("first run");
        let second = transcriber
            .transcribe(&audio, Some("en"), VoiceGates::default())
            .expect("second run");

        assert_eq!(first.text, second.text, "reused state leaked between runs");
        assert_eq!(first.skip_reason, second.skip_reason);
    }
}
