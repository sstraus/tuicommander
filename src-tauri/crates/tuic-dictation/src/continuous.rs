//! Continuous (hands-free) dictation: utterance segmentation and safe delivery.
//!
//! Two separate machines live here, and keeping them apart is the point:
//!
//! * [`Segmenter`] turns a stream of audio frames into bounded utterances. It
//!   owns pre-roll, end-of-speech and the maximum utterance length, and it never
//!   grows a buffer while nobody is speaking.
//! * [`HandsFree`] owns the mode: what it is bound to, which generation is
//!   current, the visible hold-back before a send, and what a disarm discards.
//!
//! Neither reads a clock. Both take `now_ms` (or frame durations) from the
//! caller, so a test states the timeline instead of sleeping through it and a
//! loaded machine cannot turn a behaviour assertion into a flake.
//!
//! Delivery is a port: [`VoiceSink`]. The application's PTY adapter types the
//! turn into the bound agent's composer
//! through `pty::write_voice_turn` — at once, even while the agent works, the
//! way a line typed by hand reaches a working agent. It is not the Compose
//! queue: that queue is "one message, let the agent work, then the next", and
//! speech is not that. A confident question or a draft in the composer holds
//! the turn here, in the mode, until the next tick can write it. There is
//! deliberately no other way out of this module — no raw PTY write, no ACP
//! prompt.
//!
//! # The activation phrase
//!
//! An optional setting decides whether a recognised utterance was addressed
//! here at all. Three properties are the contract:
//!
//! * **It is local.** Whisper still runs on this machine on every utterance,
//!   before the gate sees anything — the gate reads text, not audio. Gating
//!   does not reduce what is recognised; it reduces what is *submitted*.
//! * **It gates new model input, never the microphone.** A rejected transcript
//!   is dropped at [`HandsFree::accept_transcript`], which is upstream of the
//!   send slot, so it cannot reach the sink — and the sink is the only
//!   exit, which is what makes "it never reaches PTY, ACP or MCP" a structural
//!   fact rather than a check that has to be repeated per call site.
//! * **It bounds a conversation, not a sentence.** One accepted turn opens
//!   [`ACTIVATION_WINDOW_MS`] in which follow-ups need no phrase, restarted by
//!   each accepted turn and closed by every disarm.
//!
//! Interrupting spoken playback is story 816 and does not exist yet. When it
//! does, barge-in is an *audio* concern — stopping the speaker is not new model
//! input — so it belongs upstream of this gate and must not be wired through
//! it. What this module already guarantees is the other half of that criterion:
//! whatever stops playback, the words that follow are new model input and stay
//! gated whenever a phrase is configured.

/// What became of a hands-free turn written to an agent's composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceWrite {
    /// Typed and submitted. Also returned when the write was cut short after
    /// its first byte: typing it again could submit it twice.
    Written,
    /// Nothing typed. The hands-free side keeps the turn and retries.
    Held(VoiceHold),
}

/// Why a hands-free turn was not typed yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceHold {
    /// A confident question or permission dialog owns the composer.
    Question,
    /// The user has a draft in the composer.
    Draft,
    /// Another write holds the composer, or an earlier one is uncertain.
    InFlight,
    /// The PTY refused the first byte.
    WriteNotStarted,
}

impl VoiceHold {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Question => "confident question on screen",
            Self::Draft => "partial user input in the composer",
            Self::InFlight => "another write holds the composer",
            Self::WriteNotStarted => "the write did not start",
        }
    }
}

// ---------------------------------------------------------------------------
// Utterance segmentation
// ---------------------------------------------------------------------------

/// Capture sample rate. Must match `audio::AudioCapture`.
pub const SAMPLE_RATE: u32 = 16_000;

/// Analysis frame. 20 ms is short enough that end-of-speech lands inside one
/// trailing-silence budget, and long enough that the RMS of one frame is a
/// meaningful energy measurement rather than a sample-level accident.
pub const FRAME_MS: u32 = 20;

/// Samples in one analysis frame.
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize * FRAME_MS as usize) / 1000;

/// How an utterance ended. The caller reports a truncated one differently: the
/// speaker was cut off, so the text is not the whole thought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtteranceEnd {
    /// The configured quiet interval elapsed.
    Silence,
    /// The maximum utterance length was reached while the speaker kept going.
    MaxLength,
}

/// One bounded utterance, pre-roll included.
#[derive(Clone, Debug, PartialEq)]
pub struct Utterance {
    pub audio: Vec<f32>,
    pub end: UtteranceEnd,
    /// Milliseconds of *active* audio, pre-roll and trailing silence excluded.
    pub speech_ms: u32,
    /// The longest run of active frames, dips under `VOICE_GAP_MS` included.
    /// `speech_ms` adds up frames that may be seconds apart; this is what a
    /// voice looks like.
    pub longest_run_ms: u32,
}

/// Utterance boundaries, all in milliseconds except the energy floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmenterConfig {
    /// Audio retained before speech starts, so a soft first syllable survives.
    pub pre_roll_ms: u32,
    /// Quiet interval that ends an utterance.
    pub trailing_silence_ms: u32,
    /// Utterances with less active audio than this are discarded, not sent.
    pub min_speech_ms: u32,
    /// Hard cap on a single utterance.
    pub max_utterance_ms: u32,
    /// Frame RMS at or above which a frame counts as speech.
    pub activity_rms: f32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            pre_roll_ms: 300,
            trailing_silence_ms: 1_500,
            min_speech_ms: 200,
            max_utterance_ms: 30_000,
            activity_rms: 0.01,
        }
    }
}

/// Speech-boundary state machine over fixed-size frames.
///
/// While nobody is speaking the only retained audio is the pre-roll ring, so an
/// armed-and-silent microphone holds a constant, small amount of memory however
/// long it stays armed — and runs no inference at all.
pub struct Segmenter {
    config: SegmenterConfig,
    /// Samples not yet forming a whole frame.
    remainder: Vec<f32>,
    /// Bounded pre-roll, only while idle.
    pre_roll: std::collections::VecDeque<f32>,
    /// `Some` while an utterance is open.
    open: Option<OpenUtterance>,
}

struct OpenUtterance {
    audio: Vec<f32>,
    speech_ms: u32,
    silence_ms: u32,
    /// The current run of activity and the longest so far, in milliseconds. A
    /// dip of up to [`VOICE_GAP_MS`] does not end a run, provided the activity
    /// after it lasts [`VOICE_CONFIRM_FRAMES`] frames.
    run_ms: u32,
    longest_run_ms: u32,
    /// The dip and the active frames after it, not yet part of the run: a few
    /// frames after a dip are as likely residual echo as speech, so they only
    /// join the run once [`VOICE_CONFIRM_FRAMES`] consecutive ones confirm it.
    bridge_ms: u32,
    bridge_frames: u32,
    /// Quiet frames since the last active one, in milliseconds.
    gap_ms: u32,
}

/// The longest dip inside a voice: a plosive closure is 40-100 ms of quiet in
/// the middle of a word. Scattered residual echo, a frame in every 200 ms,
/// stays well past it; a denser scatter (one frame in four) is closed out by
/// the confirmation after the dip.
const VOICE_GAP_MS: u32 = 60;

/// Contiguous active frames needed after a dip before the run continues across
/// it. Voiced segments of speech outlast 60 ms; residual echo comes in singles
/// and pairs.
const VOICE_CONFIRM_FRAMES: u32 = 3;

impl Segmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        Self {
            config,
            remainder: Vec::with_capacity(FRAME_SAMPLES),
            pre_roll: std::collections::VecDeque::new(),
            open: None,
        }
    }

    /// True while an utterance is open — what the UI renders as "capturing".
    pub fn is_capturing(&self) -> bool {
        self.open.is_some()
    }

    /// True once the open utterance holds enough speech to be sent — the same
    /// `min_speech_ms` rule [`close`](Self::close) applies. Opening needs one
    /// frame over the floor; this needs a sustained voice, which is what tells
    /// a person apart from a few frames of echo the canceller left behind.
    pub fn has_speech(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| self.is_speech(open.speech_ms))
    }

    /// True once the open utterance holds an unbroken `min_speech_ms` of
    /// activity. [`has_speech`](Self::has_speech) cannot say this: it adds up
    /// every active frame since the utterance opened, and an utterance stays
    /// open until a whole `trailing_silence_ms` of quiet, so the scattered
    /// frames of residual echo from a reply that is still playing add up to a
    /// "sustained" voice within a second.
    pub fn has_voice(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| self.is_speech(open.longest_run_ms))
    }

    /// Does this closed utterance hold an unbroken `min_speech_ms` of
    /// activity? See [`has_voice`](Self::has_voice).
    pub fn is_voice(&self, utterance: &Utterance) -> bool {
        self.is_speech(utterance.longest_run_ms)
    }

    fn is_speech(&self, speech_ms: u32) -> bool {
        speech_ms >= self.config.min_speech_ms
    }

    /// Samples currently retained. Bounded by the pre-roll while idle and by
    /// `max_utterance_ms` while capturing; the assertion a silence test makes.
    pub fn retained_samples(&self) -> usize {
        self.remainder.len()
            + self.pre_roll.len()
            + self.open.as_ref().map_or(0, |open| open.audio.len())
    }

    /// Feed captured audio. Returns every utterance that closed inside this
    /// chunk — a long chunk can close more than one.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Utterance> {
        let mut closed = Vec::new();
        self.remainder.extend_from_slice(samples);
        let mut start = 0;
        while start + FRAME_SAMPLES <= self.remainder.len() {
            let frame_range = start..start + FRAME_SAMPLES;
            if let Some(utterance) = self.push_frame(frame_range.clone()) {
                closed.push(utterance);
            }
            start += FRAME_SAMPLES;
        }
        self.remainder.drain(..start);
        closed
    }

    /// Drop everything captured so far.
    ///
    /// Nothing calls it in production today: a disarm ends the runtime, and the
    /// next arm builds a fresh `Capture`, so the segmenter is never reused
    /// across generations. It stays because that is the invariant — if a future
    /// caller ever keeps one alive across an arm, this is what it must call.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.remainder.clear();
        self.pre_roll.clear();
        self.open = None;
    }

    fn push_frame(&mut self, range: std::ops::Range<usize>) -> Option<Utterance> {
        let active = frame_rms(&self.remainder[range.clone()]) >= self.config.activity_rms;
        let frame: &[f32] = &self.remainder[range];
        match self.open.as_mut() {
            None => {
                if active {
                    let mut audio: Vec<f32> = self.pre_roll.drain(..).collect();
                    audio.extend_from_slice(frame);
                    self.open = Some(OpenUtterance {
                        audio,
                        speech_ms: FRAME_MS,
                        silence_ms: 0,
                        run_ms: FRAME_MS,
                        longest_run_ms: FRAME_MS,
                        bridge_ms: 0,
                        bridge_frames: 0,
                        gap_ms: 0,
                    });
                } else {
                    let cap = ms_to_samples(self.config.pre_roll_ms);
                    self.pre_roll.extend(frame.iter().copied());
                    while self.pre_roll.len() > cap {
                        self.pre_roll.pop_front();
                    }
                }
                None
            }
            Some(open) => {
                open.audio.extend_from_slice(frame);
                if active {
                    open.speech_ms += FRAME_MS;
                    open.silence_ms = 0;
                    // A short dip is part of the run, and counts as time in it,
                    // once VOICE_CONFIRM_FRAMES active frames follow it.
                    if open.gap_ms == 0 && open.bridge_frames == 0 {
                        open.run_ms += FRAME_MS;
                    } else if open.gap_ms == 0 {
                        open.bridge_ms += FRAME_MS;
                        open.bridge_frames += 1;
                        if open.bridge_frames >= VOICE_CONFIRM_FRAMES {
                            open.run_ms += open.bridge_ms;
                            open.bridge_frames = 0;
                        }
                    } else if open.gap_ms <= VOICE_GAP_MS {
                        open.bridge_ms = open.gap_ms + FRAME_MS;
                        open.bridge_frames = 1;
                    } else {
                        open.run_ms = FRAME_MS;
                        open.bridge_frames = 0;
                    }
                    open.gap_ms = 0;
                    open.longest_run_ms = open.longest_run_ms.max(open.run_ms);
                } else {
                    open.silence_ms += FRAME_MS;
                    open.gap_ms += FRAME_MS;
                    open.bridge_frames = 0;
                }
                let end = if open.audio.len() >= ms_to_samples(self.config.max_utterance_ms) {
                    Some(UtteranceEnd::MaxLength)
                } else if open.silence_ms >= self.config.trailing_silence_ms {
                    Some(UtteranceEnd::Silence)
                } else {
                    None
                };
                end.and_then(|end| self.close(end))
            }
        }
    }

    /// Close the open utterance, discarding it when it holds too little speech.
    fn close(&mut self, end: UtteranceEnd) -> Option<Utterance> {
        let open = self.open.take()?;
        self.pre_roll.clear();
        if !self.is_speech(open.speech_ms) {
            return None;
        }
        Some(Utterance {
            audio: open.audio,
            end,
            speech_ms: open.speech_ms,
            longest_run_ms: open.longest_run_ms,
        })
    }
}

/// Apply the hands-free utterance gate to a finished push-to-talk recording.
/// Feed bounded chunks so a long recording does not copy its whole buffer into
/// the segmenter at once. An open utterance at key release also counts.
pub fn has_sustained_speech(samples: &[f32], config: SegmenterConfig) -> bool {
    let mut segmenter = Segmenter::new(config);
    for chunk in samples.chunks(SAMPLE_RATE as usize) {
        if !segmenter.push(chunk).is_empty() || segmenter.has_speech() {
            return true;
        }
    }
    false
}

fn ms_to_samples(ms: u32) -> usize {
    (SAMPLE_RATE as usize * ms as usize) / 1000
}

fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

// ---------------------------------------------------------------------------
// Hands-free mode
// ---------------------------------------------------------------------------

/// What the mode is bound to while armed.
///
/// Both halves are pinned at arming. `session_id` is the delivery target and
/// `owner` is the audio endpoint that armed it — the desktop adapter, or one
/// remote client. A focus change touches neither, which is the whole reason
/// this struct exists instead of reading "the active tab" at send time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub session_id: String,
    pub owner: String,
}

/// What the user sees. Model state and audio state are separate elsewhere;
/// this is the capture/delivery half.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Disarmed,
    /// Armed, microphone open, nobody speaking.
    Waiting,
    Capturing,
    Transcribing,
    /// Transcribed and visible, counting down the hold-back before the write
    /// — or past it, held by a dialog or a draft in the composer.
    HoldingBack,
    /// Typed into the bound agent's composer.
    Delivered,
    Error,
}

impl Phase {
    /// The label both transports report. IPC and HTTP serialize the same
    /// status struct, so this string is the only spelling a client ever sees.
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Disarmed => "disarmed",
            Self::Waiting => "waiting",
            Self::Capturing => "capturing",
            Self::Transcribing => "transcribing",
            Self::HoldingBack => "holding_back",
            Self::Delivered => "delivered",
            Self::Error => "error",
        }
    }
}

/// Why the mode disarmed. Every one of these is terminal: nothing re-arms by
/// itself, per Boss's decision that a manual abort kills the mode rather than
/// the utterance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisarmReason {
    /// Hotkey or mic button.
    Manual,
    TargetClosed,
    OwnerDisconnected,
    DeviceFailed(String),
}

/// Why arming was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmError {
    AlreadyArmed,
    /// The target cannot take hands-free input (not an agent PTY session, or
    /// an ACP target). It stays unavailable — there is no fallback.
    UnsupportedTarget,
}

/// What a disarm leaves for the caller to finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disarmed {
    /// The generation that was current. Anything asynchronous still carrying it
    /// is stale from here on.
    pub generation: u64,
    /// The target this mode was bound to. Carried here because the binding is
    /// gone by the time a caller sees this, and the exit hint needs it.
    pub session_id: String,
    /// The composer typed this arm's entry hint. `false` means the model was
    /// never told this conversation began — no notice sent, or one still held
    /// back by a dialog — so it is owed no notice that it ended.
    pub entry_hint_written: bool,
    /// A transcript was waiting out its hold-back, or held by the composer,
    /// and never reached the model.
    pub discarded_pending: bool,
    /// An utterance was open or a transcription was in flight.
    pub discarded_capture: bool,
    pub reason: DisarmReason,
}

/// What happened to a transcript handed back by the transcriber.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptOutcome {
    /// Accepted and visible; it will be written at `send_at_ms` unless the
    /// user disarms first.
    HeldBack { send_at_ms: u64 },
    /// Carried an old generation — a result from before a disarm or re-arm.
    Stale,
    /// Nothing was recognised, so nothing is sent.
    Empty,
    /// The mode is not armed; late results cannot resurrect it.
    NotArmed,
    /// An activation phrase is configured, this turn did not begin with it and
    /// no window was open. The speech was not addressed here, so it is dropped.
    Rejected,
    /// The transcript was the activation phrase and nothing else. The window is
    /// open until `window_until_ms`; there is nothing to submit.
    Activated { window_until_ms: u64 },
}

/// A transcript whose hold-back has expired, ready for the composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSend {
    pub generation: u64,
    pub session_id: String,
    pub text: String,
    /// The language this turn was spoken in, as a two-letter code, or `None`
    /// when the recogniser named none. It travels with the text because the
    /// model is told to answer in it.
    pub language: Option<String>,
}

struct PendingSend {
    generation: u64,
    text: String,
    language: Option<String>,
    send_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Activation phrase
// ---------------------------------------------------------------------------

/// How long one activated turn keeps the next one open.
///
/// Long enough to ask a follow-up without addressing the tool again, short
/// enough that a microphone left armed after a conversation stops forwarding
/// the room. Not a setting: Boss asked for the phrase, and a window length is a
/// knob nobody has asked for yet. Every accepted turn restarts it, so the bound
/// is on the pause between turns rather than on the conversation.
///
/// DEFERRED (2026-09-21) — the window is anchored on *acceptance*, not on the
/// model's reply. A user who asks something, waits out a long answer and then
/// speaks again will have to say the phrase a second time. Anchoring it on the
/// reply needs a signal this module deliberately does not have: it writes and
/// observes nothing coming back, which is what keeps the sink its only exit. Story 816-cbbf adds spoken playback and is the first caller that will
/// know when an answer ended; revisit the anchor there rather than teaching
/// this module to watch agent state.
pub const ACTIVATION_WINDOW_MS: u64 = 15_000;

/// Keep a keyword-addressed turn open long enough for a short follow-up to
/// belong to the same terminal message, even after the segmenter closed it.
pub const KEYWORD_CONTINUATION_MS: u64 = 5_000;

/// The local gate between a recognised transcript and the send slot.
///
/// It runs *after* whisper, on text, and decides whether the user was speaking
/// to the tool at all. No configured phrase means no gate; a configured one
/// must open every new turn.
#[derive(Default)]
struct Activation {
    /// Lowercased words of the configured phrase. Empty means ungated.
    phrase: Vec<String>,
    window_ms: u64,
    /// While set and not yet elapsed, a turn needs no phrase.
    open_until_ms: Option<u64>,
}

/// What the gate decided about one transcript.
enum Admission<'a> {
    /// Nothing is configured; this is ordinary dictation.
    Ungated,
    /// Submit this, the phrase already removed if it carried one.
    Accept(&'a str),
    /// The phrase and nothing else: the window opened, there is nothing to say.
    PhraseOnly { window_until_ms: u64 },
    /// Not addressed here.
    Rejected,
}

impl Activation {
    fn set(&mut self, phrase: &str, window_ms: u64) {
        self.phrase = phrase_words(phrase);
        self.window_ms = window_ms;
        self.open_until_ms = None;
    }

    /// Close the window. Every disarm calls this: the mode ending is the user
    /// stopping addressing it, so the next arm must be gated again.
    fn close(&mut self) {
        self.open_until_ms = None;
    }

    /// `continuing` is true while an addressed turn is still held back: the
    /// text belongs to it whatever the clock says. The window is read at the
    /// moment the recogniser *returns*, so a long second phrase (whisper runs
    /// after the speech ends) would otherwise find it already shut and be
    /// dropped as unaddressed, while its first half is sent alone.
    fn admit<'a>(&mut self, text: &'a str, now_ms: u64, continuing: bool) -> Admission<'a> {
        if self.phrase.is_empty() {
            return Admission::Ungated;
        }
        let open = continuing || self.open_until_ms.is_some_and(|until| now_ms < until);
        // The phrase is stripped even inside an open window: saying it again is
        // still addressing the tool, and the model may not read it either way.
        match strip_leading_phrase(&self.phrase, text) {
            Some(rest) => {
                let window_until_ms = now_ms + self.window_ms;
                self.open_until_ms = Some(window_until_ms);
                if rest.is_empty() {
                    Admission::PhraseOnly { window_until_ms }
                } else {
                    Admission::Accept(rest)
                }
            }
            // A rejected turn does not extend the window; only speech that was
            // addressed here can keep the conversation alive.
            None if open => {
                self.open_until_ms = Some(now_ms + self.window_ms);
                Admission::Accept(text)
            }
            None => Admission::Rejected,
        }
    }
}

/// Split a configured phrase into the folded words a transcript must begin with.
fn phrase_words(phrase: &str) -> Vec<String> {
    leading_words(phrase, usize::MAX)
        .into_iter()
        .map(|range| fold_word(&phrase[range]))
        .collect()
}

/// A word as the gate compares it: lowercase, with the accent removed from
/// every Latin vowel and from `ç` and `ñ`.
///
/// Case folding alone is not enough: Whisper writes "Sentì" for a spoken
/// "senti" when it reads the verb as past tense, and `ì` does not lowercase to
/// `i`. A table rather than Unicode decomposition, because Whisper emits
/// precomposed characters and the table needs no new dependency.
fn fold_word(word: &str) -> String {
    word.chars()
        .flat_map(char::to_lowercase)
        .map(|character| match character {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// Whether a heard, folded word is `expected`, allowing one extra trailing
/// letter on an expected word of three or more letters ("mac" → "mack").
///
/// That is the whole tolerance, and it is one-sided on purpose. A recogniser
/// spelling a name by its sound tends to *add* a letter. A shorter word or a
/// different last letter is how ordinary speech starts: after "senti", "ma",
/// "mai" and "max" are all a person being addressed, not the tool. Short words
/// get no tolerance at all, because "ok" plus one letter is too many words.
fn word_matches(heard: &str, expected: &str) -> bool {
    heard == expected
        || (expected.chars().count() >= 3
            && heard.starts_with(expected)
            && heard[expected.len()..].chars().count() == 1)
}

/// Byte ranges of the first `count` words in `text`.
///
/// A word is a maximal run of alphanumeric characters, so the punctuation and
/// capitalisation a recogniser invents never decide a match, and a longer word
/// that merely starts with the phrase is a different word rather than a prefix.
fn leading_words(text: &str, count: usize) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        match (character.is_alphanumeric(), start) {
            (true, None) => start = Some(index),
            (false, Some(from)) => {
                ranges.push(from..index);
                start = None;
                if ranges.len() == count {
                    return ranges;
                }
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        ranges.push(from..text.len());
    }
    ranges
}

/// How many words of a rejected transcript the DEBUG log keeps: the phrase
/// plus a word or two, never the whole remark.
const REJECTED_EXCERPT_WORDS: usize = 4;

/// The first `count` words of `text`, verbatim: punctuation, accents and case
/// between them are kept, because they are what a matching miss is made of.
fn leading_excerpt(text: &str, count: usize) -> &str {
    let words = leading_words(text, count);
    match (words.first(), words.last()) {
        (Some(first), Some(last)) => &text[first.start..last.end],
        _ => "",
    }
}

/// The text after a leading `phrase`, or `None` when it does not begin with it.
///
/// Only the separators between the phrase and the speech are removed: sentence
/// punctuation, dashes and the *closing* half of a delimiter pair. An opening
/// quote or bracket survives, because after the phrase it belongs to the speech
/// rather than to the phrase, and this function may not edit what was said.
///
/// A heard word may also be several phrase words run together, because
/// Whisper joins a short name onto the word before it ("Sentimac"). The joined
/// form must still be exactly the phrase words in order, so "sentimento" stays
/// a different word.
fn strip_leading_phrase<'a>(phrase: &[String], text: &'a str) -> Option<&'a str> {
    let words = leading_words(text, phrase.len());
    let mut next = 0;
    let mut end = 0;
    for range in words {
        if next == phrase.len() {
            break;
        }
        let heard = fold_word(&text[range.clone()]);
        let joined = (next + 1..=phrase.len())
            .find(|&upto| word_matches(&heard, &phrase[next..upto].concat()))?;
        next = joined;
        end = range.end;
    }
    if next < phrase.len() {
        return None;
    }
    Some(
        text[end..]
            .trim_start_matches(|character: char| {
                character.is_whitespace() || ",.;:!?…–—»”’)]".contains(character)
            })
            .trim(),
    )
}

/// The hands-free mode state machine.
pub struct HandsFree {
    hold_back_ms: u64,
    generation: u64,
    binding: Option<Binding>,
    phase: Phase,
    pending: Option<PendingSend>,
    /// Why the last write was held, so a hold that lasts many ticks is
    /// reported once rather than on every tick.
    last_hold: Option<VoiceHold>,
    last_error: Option<String>,
    /// Spoken turns typed into the composer, and turns the activation
    /// gate dropped. Monotonic for the process, never reset on arm: a client
    /// plays an earcon when one moves between two polls, and a reset would
    /// read as nothing — or, against a stale baseline, as a turn.
    delivered_turns: u64,
    dropped_turns: u64,
    activation: Activation,
    /// The language of the last transcript this mode accepted.
    ///
    /// The turn's language, not the setting's: with a fixed language the two
    /// are the same, and with `auto` this is the only record of what the user
    /// actually spoke. The voice that answers is chosen from it, so it is
    /// cleared on arm and on disarm — a reply must never be spoken in the
    /// language of a conversation that has ended.
    turn_language: Option<String>,
    /// Where this arm's entry hint is. The exit hint is owed only to a model
    /// that read it, so `Written` is the only state that buys one.
    entry_hint: EntryHint,
    /// The language this conversation's model was last told to reply in.
    ///
    /// Stated once — by the start notice when the language is already known,
    /// otherwise by the first spoken turn — and again only when the user
    /// switches language. Set only after the sink typed the entry that said
    /// it, because a held or refused entry told the model nothing. Cleared on arm
    /// and disarm with `turn_language`: a new conversation has been told
    /// nothing yet.
    announced_language: Option<String>,
}

impl HandsFree {
    pub fn new(hold_back_ms: u64) -> Self {
        Self {
            hold_back_ms,
            generation: 0,
            binding: None,
            phase: Phase::Disarmed,
            pending: None,
            last_hold: None,
            last_error: None,
            delivered_turns: 0,
            dropped_turns: 0,
            activation: Activation::default(),
            turn_language: None,
            entry_hint: EntryHint::NotSent,
            announced_language: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// See the field. Counts [`note_written`](Self::note_written) calls.
    pub fn delivered_turns(&self) -> u64 {
        self.delivered_turns
    }

    /// See the field. Counts [`TranscriptOutcome::Rejected`].
    pub fn dropped_turns(&self) -> u64 {
        self.dropped_turns
    }

    /// The hold-back this mode will apply to the next transcript.
    pub fn hold_back_ms(&self) -> u64 {
        if self.activation.phrase.is_empty() {
            self.hold_back_ms
        } else {
            self.hold_back_ms.max(KEYWORD_CONTINUATION_MS)
        }
    }

    /// Take the configured hold-back. Refused while armed: changing it under a
    /// transcript that is already counting down would move a deadline the user
    /// is currently watching.
    pub fn set_hold_back_ms(&mut self, hold_back_ms: u64) {
        if self.binding.is_none() {
            self.hold_back_ms = hold_back_ms;
        }
    }

    /// Take the configured activation phrase. An empty one is no gate at all.
    ///
    /// Refused while armed for the same reason as the hold-back: the phrase is
    /// the rule the user is currently speaking against, and moving it under a
    /// live conversation would silently drop the next thing they say.
    pub fn set_activation(&mut self, phrase: &str, window_ms: u64) {
        if self.binding.is_none() {
            self.activation.set(phrase, window_ms);
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn binding(&self) -> Option<&Binding> {
        self.binding.as_ref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// The transcript currently waiting out its hold-back, for the UI to show.
    pub fn pending_text(&self) -> Option<&str> {
        self.pending.as_ref().map(|pending| pending.text.as_str())
    }

    /// Bind a target session and an audio owner, and open a new generation.
    ///
    /// `target_supported` is the caller's answer to "can this session take
    /// hands-free input"; a `false` keeps the mode unavailable rather than
    /// arming it against a target that would need a bypass to reach.
    pub fn arm(
        &mut self,
        session_id: impl Into<String>,
        owner: impl Into<String>,
        target_supported: bool,
    ) -> Result<u64, ArmError> {
        if self.binding.is_some() {
            return Err(ArmError::AlreadyArmed);
        }
        if !target_supported {
            return Err(ArmError::UnsupportedTarget);
        }
        self.generation += 1;
        self.binding = Some(Binding {
            session_id: session_id.into(),
            owner: owner.into(),
        });
        self.phase = Phase::Waiting;
        self.last_error = None;
        // Nobody has spoken yet, so this conversation has no language. With a
        // fixed setting the first reply could borrow it, but with `auto` there
        // is nothing to borrow and inheriting the last conversation's language
        // is how a new user is answered in the previous one's.
        self.turn_language = None;
        // Nothing has been said to the model about this conversation yet, so
        // nothing is owed to it when the conversation ends.
        self.entry_hint = EntryHint::NotSent;
        self.announced_language = None;
        self.last_hold = None;
        Ok(self.generation)
    }

    /// The language of the most recent accepted turn, as a two-letter code.
    ///
    /// `None` before anybody has spoken, and after a turn the recogniser could
    /// not name. Callers treat that as "not yet known" and refuse to speak
    /// rather than choosing a language on the user's behalf.
    pub fn turn_language(&self) -> Option<&str> {
        self.turn_language.as_deref()
    }

    /// An utterance opened.
    pub fn note_capturing(&mut self) {
        if self.binding.is_some() {
            self.phase = Phase::Capturing;
        }
    }

    /// An utterance ended without reaching the transcriber, for too little
    /// speech: the capture is over, and the phase it set must not outlive it.
    pub fn note_capture_discarded(&mut self) {
        if self.phase == Phase::Capturing {
            self.phase = if self.pending.is_some() {
                Phase::HoldingBack
            } else {
                Phase::Waiting
            };
        }
    }

    /// An utterance closed and went to the transcriber.
    pub fn note_transcribing(&mut self) {
        if self.binding.is_some() {
            self.phase = Phase::Transcribing;
        }
    }

    /// Hand back a transcription result. `generation` is the one the capture
    /// carried, which is how a result that outlived its mode is rejected.
    ///
    /// `language` is what the recogniser made of this turn. It is recorded on
    /// the same call as the text rather than on a call of its own: the two are
    /// one result, and two entry points would let a turn be admitted with the
    /// previous turn's language still standing.
    pub fn accept_transcript(
        &mut self,
        generation: u64,
        text: &str,
        language: Option<&str>,
        now_ms: u64,
    ) -> TranscriptOutcome {
        if self.binding.is_none() {
            return TranscriptOutcome::NotArmed;
        }
        if generation != self.generation {
            return TranscriptOutcome::Stale;
        }
        let text = text.trim();
        if text.is_empty() {
            self.phase = Phase::Waiting;
            return TranscriptOutcome::Empty;
        }
        // The gate sits here on purpose: after the local recogniser, before the
        // send slot. Everything past this point is on its way to a model, and
        // the send slot is the only thing `poll_send` can hand to the sink.
        let text = match self.activation.admit(text, now_ms, self.pending.is_some()) {
            Admission::Ungated => text,
            Admission::Accept(rest) => rest,
            Admission::PhraseOnly { window_until_ms } => {
                // Addressed here, so it counts: the phrase alone already tells
                // us which language this conversation is being held in.
                self.turn_language = language.map(str::to_string);
                self.phase = Phase::Waiting;
                return TranscriptOutcome::Activated { window_until_ms };
            }
            Admission::Rejected => {
                // Not addressed here, so its language is not ours either.
                // Recording it would let a remark across the room choose the
                // voice the next real turn is answered in.
                self.dropped_turns += 1;
                self.phase = Phase::Waiting;
                return TranscriptOutcome::Rejected;
            }
        };
        let language = language.map(str::to_string);
        self.turn_language = language.clone();
        let send_at_ms = now_ms + self.hold_back_ms();
        // A transcript that arrives while another is held back is the same
        // turn: the user paused mid-sentence long enough for the segmenter to
        // close the first half. Preserve the phrase boundary with a newline
        // and restart the hold-back; replacing it would drop the first half.
        // The PTY sink frames multiline text as a paste, so this newline does
        // not submit: the whole turn gets one Enter after the paste ends.
        let text = match self.pending.take() {
            Some(pending) => format!("{}\n{text}", pending.text),
            None => text.to_string(),
        };
        self.pending = Some(PendingSend {
            generation,
            text,
            language,
            send_at_ms,
        });
        self.phase = Phase::HoldingBack;
        TranscriptOutcome::HeldBack { send_at_ms }
    }

    /// Take the pending transcript once its hold-back has expired.
    ///
    /// Returns `None` while the hold-back is still running, which is what makes
    /// the hold-back visible *and* cancellable: nothing has been written yet.
    pub fn poll_send(&mut self, now_ms: u64) -> Option<VoiceSend> {
        // The continuation began before the deadline but its recogniser has
        // not returned yet. Keep the first phrase in the composer until the
        // speech closes, then accept_transcript joins both before rescheduling.
        if self.phase == Phase::Capturing {
            return None;
        }
        let ready = self
            .pending
            .as_ref()
            .is_some_and(|pending| now_ms >= pending.send_at_ms);
        if !ready {
            return None;
        }
        let pending = self.pending.take()?;
        let session_id = self.binding.as_ref()?.session_id.clone();
        Some(VoiceSend {
            generation: pending.generation,
            session_id,
            text: pending.text,
            language: pending.language,
        })
    }

    /// A spoken turn reached the composer.
    pub fn note_written(&mut self, generation: u64) {
        if generation == self.generation && self.binding.is_some() {
            self.delivered_turns += 1;
            self.phase = Phase::Delivered;
            self.last_hold = None;
        }
    }

    /// This arm's entry hint reached the composer.
    ///
    /// The phase is deliberately left alone: the phase describes what the
    /// *user's* speech is doing, and a mode that has heard nothing yet is still
    /// `Waiting`.
    pub fn note_hint_written(&mut self, generation: u64) {
        if generation == self.generation && self.binding.is_some() {
            self.entry_hint = EntryHint::Written;
            self.last_hold = None;
        }
    }

    /// Put a due turn back because the composer held it. It stays due, so the
    /// next tick retries it; a transcript that arrives meanwhile joins it.
    /// Returns whether this is a new hold, for a log that is not per tick.
    fn hold(&mut self, send: VoiceSend, reason: VoiceHold) -> bool {
        if send.generation == self.generation && self.binding.is_some() {
            self.pending = Some(PendingSend {
                generation: send.generation,
                text: send.text,
                language: send.language,
                send_at_ms: 0,
            });
            self.phase = Phase::HoldingBack;
        }
        self.note_hold(reason)
    }

    fn note_hold(&mut self, reason: VoiceHold) -> bool {
        self.last_hold.replace(reason) != Some(reason)
    }

    /// The name of `language` if the model still has to be told it: a
    /// language this conversation has not announced yet. `None` for a turn
    /// nobody could name — it says nothing, and must not make the model forget
    /// what it was already told.
    fn language_to_announce(&self, language: Option<&str>) -> Option<&'static str> {
        let code = language?;
        let name = super::language::name_for(code)?;
        (self.announced_language.as_deref() != Some(code)).then_some(name)
    }

    /// The sink refused a delivery.
    ///
    /// The mode stays armed — the target is still bound and the next utterance
    /// may well land — but the failure is reported rather than swallowed, so a
    /// message that never reached a model does not look like one that did.
    pub fn note_send_failed(&mut self, message: &str) {
        if self.binding.is_some() {
            self.last_error = Some(message.to_string());
            self.phase = Phase::Waiting;
        }
    }

    /// Disarm the whole mode. Never re-arms itself; `arm` is the only way back.
    pub fn disarm(&mut self, reason: DisarmReason) -> Option<Disarmed> {
        let session_id = self.binding.as_ref()?.session_id.clone();
        let discarded_capture = matches!(self.phase, Phase::Capturing | Phase::Transcribing);
        let disarmed = Disarmed {
            generation: self.generation,
            session_id,
            entry_hint_written: std::mem::replace(&mut self.entry_hint, EntryHint::NotSent)
                == EntryHint::Written,
            discarded_pending: self.pending.take().is_some(),
            discarded_capture,
            reason: reason.clone(),
        };
        self.binding = None;
        // Every reason this machine can end for — the user's abort, a closed
        // target, a lost owner, a dead microphone — is the user no longer
        // addressing it. None of them may hand the next arm an open window,
        // and none of them may hand it a language either: a reply queued after
        // this point belongs to a conversation nobody is having.
        self.activation.close();
        self.turn_language = None;
        self.announced_language = None;
        self.last_hold = None;
        self.phase = match &reason {
            DisarmReason::DeviceFailed(message) => {
                self.last_error = Some(message.clone());
                Phase::Error
            }
            _ => Phase::Disarmed,
        };
        Some(disarmed)
    }

    /// The bound target went away.
    pub fn note_session_closed(&mut self, session_id: &str) -> Option<Disarmed> {
        if self.binding.as_ref()?.session_id != session_id {
            return None;
        }
        self.disarm(DisarmReason::TargetClosed)
    }

    /// The audio owner went away — a closed browser tab, a lost WS client.
    pub fn note_owner_disconnected(&mut self, owner: &str) -> Option<Disarmed> {
        if self.binding.as_ref()?.owner != owner {
            return None;
        }
        self.disarm(DisarmReason::OwnerDisconnected)
    }

    /// The capture device failed.
    pub fn note_device_failed(&mut self, message: impl Into<String>) -> Option<Disarmed> {
        self.disarm(DisarmReason::DeviceFailed(message.into()))
    }
}

// ---------------------------------------------------------------------------
// Delivery port
// ---------------------------------------------------------------------------

/// Where this arm's entry hint is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EntryHint {
    NotSent,
    /// Held by a dialog or a draft; retried on every tick before any turn, so
    /// the model never reads a spoken turn before the notice it depends on.
    /// `language` is set when the text announces one.
    Pending {
        text: String,
        language: Option<String>,
    },
    Written,
}

/// The only exit from hands-free capture to a model.
pub trait VoiceSink {
    /// Type one line into the bound agent's composer now, or say why not.
    fn write(&self, session_id: &str, text: &str) -> Result<VoiceWrite, String>;
}

/// What one delivery attempt did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// A spoken turn reached the composer.
    Turn,
    /// The held entry hint reached the composer.
    Notice,
    /// Nothing was typed; the mode keeps what it tried. `first` is false while
    /// the same hold lasts, so the runtime logs it once.
    Held { reason: VoiceHold, first: bool },
}

/// What a spoken turn looks like once it reaches the model.
///
/// The transcript, plus — when the model has not been told it yet — the one
/// thing it cannot work out for itself: a spoken conversation has a language,
/// and a model that answers a question in English because English is what it
/// defaults to has ended the conversation. The requirement travels in the
/// entry rather than in a system prompt, because the sink is the only thing
/// that reaches the model — hints can be turned off, and the requirement
/// may not be. It is stated once per conversation and again on a switch
/// ([`HandsFree::language_to_announce`]), not on every turn.
///
/// One line, never two. The sink types this into a terminal and submits it,
/// and a newline in the middle submits half a sentence.
///
/// An unnamed language adds nothing. There is no default to fall back to: an
/// invented "reply in English" is the exact failure this exists to prevent, so
/// a turn nobody could name goes to the model as the user said it.
fn compose_entry(text: &str, announce: Option<&str>) -> String {
    match announce {
        Some(name) => format!("{text} (reply in {name})"),
        None => text.to_string(),
    }
}

/// Write what is due: a held entry hint first, then a transcript whose
/// hold-back expired. `None` when there was nothing to try.
///
/// The hint goes first and alone: a model that reads "you can answer out
/// loud" after the question it applies to has been told nothing useful, so no
/// turn is written while the hint is still held. A held turn goes back into
/// the mode and is retried on the next tick — never into the Compose queue.
/// The announced language is recorded only after a write, because a held or
/// refused entry told the model nothing.
pub fn deliver_due(
    mode: &mut HandsFree,
    sink: &dyn VoiceSink,
    now_ms: u64,
) -> Option<Result<Delivery, String>> {
    if let EntryHint::Pending { text, language } = mode.entry_hint.clone() {
        let session_id = mode.binding()?.session_id.clone();
        let generation = mode.generation();
        return Some(match sink.write(&session_id, &text) {
            Ok(VoiceWrite::Written) => {
                mode.note_hint_written(generation);
                if language.is_some() {
                    mode.announced_language = language;
                }
                Ok(Delivery::Notice)
            }
            Ok(VoiceWrite::Held(reason)) => Ok(Delivery::Held {
                reason,
                first: mode.note_hold(reason),
            }),
            Err(error) => {
                mode.entry_hint = EntryHint::NotSent;
                Err(error)
            }
        });
    }
    let send = mode.poll_send(now_ms)?;
    let announce = mode.language_to_announce(send.language.as_deref());
    let entry = compose_entry(&send.text, announce);
    Some(match sink.write(&send.session_id, &entry) {
        Ok(VoiceWrite::Written) => {
            mode.note_written(send.generation);
            if announce.is_some() {
                mode.announced_language = send.language;
            }
            Ok(Delivery::Turn)
        }
        Ok(VoiceWrite::Held(reason)) => Ok(Delivery::Held {
            reason,
            first: mode.hold(send, reason),
        }),
        Err(error) => Err(error),
    })
}

// ---------------------------------------------------------------------------
// Telling the model the mode changed (821-842a)
// ---------------------------------------------------------------------------

/// What the model is told when a hands-free conversation opens.
///
/// It names the two things the model cannot observe: that the words arriving
/// from now on were spoken rather than typed, and that it has a way to answer
/// out loud. Without the second half the `voice` tool is listed and never used,
/// because a model with no reason to speak writes text.
///
/// One line, for the reason [`compose_entry`] gives: the sink types this into
/// a terminal and submits it, and a newline in the middle submits half of it.
pub const MODE_ENTRY_HINT: &str = "Hands-free voice is now on for this terminal: what arrives from \
     here on was spoken out loud, and you can answer out loud with the `voice` tool of the \
     TUICommander MCP server (tuicommander), called with action \"speak\". If that tool is not \
     loaded yet, search your deferred tools for \"voice\". Keep spoken replies short enough to \
     listen to.";

/// The start notice to send, given the `hands_free_start_notice` setting.
///
/// Blank means the built-in [`MODE_ENTRY_HINT`]. The reply language is not
/// part of it: [`deliver_entry_hint`] appends it by code when it is known, so
/// a user-written notice cannot drop it. Line breaks and runs of whitespace fold to single spaces, because
/// the sink submits on a newline and a notice typed on three lines would
/// reach the model as three prompts.
pub fn entry_hint_text(configured: &str) -> String {
    let folded = configured.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.is_empty() {
        MODE_ENTRY_HINT.to_string()
    } else {
        folded
    }
}

/// What the model is told when the conversation ends.
///
/// Sent only to a model that read the entry hint — see [`deliver_exit_hint`].
/// Its whole job is to undo that one, so it must not describe a capability or
/// imply anything is still listening.
pub const MODE_EXIT_HINT: &str = "Hands-free voice is off for this terminal. The voice tool can no \
     longer speak here, so reply as text from now on.";

/// Tell the bound model that a hands-free conversation just opened.
///
/// Through the sink like every spoken turn — typed at once, even into a busy
/// agent, and into the session the mode bound to rather than whatever tab the
/// user has since focused. A dialog or a draft holds it in the mode, and
/// [`deliver_due`] retries it before any turn. `None` when nothing is armed.
///
/// The caller decides whether the user asked for this at all; the mode only
/// remembers that it was sent.
///
/// `language` is the dictation setting. A fixed language is known before
/// anybody speaks, so the notice states it and the first turn need not; `auto`
/// names nothing, and the first spoken turn carries it instead.
pub fn deliver_entry_hint(
    mode: &mut HandsFree,
    sink: &dyn VoiceSink,
    notice: &str,
    language: Option<&str>,
) -> Option<Result<Delivery, String>> {
    let binding = mode.binding()?;
    let session_id = binding.session_id.clone();
    let generation = mode.generation();
    let announce = mode.language_to_announce(language);
    let notice = match announce {
        Some(name) => format!("{notice} Reply in {name}."),
        None => notice.to_string(),
    };
    let language = announce.and(language).map(str::to_string);
    match sink.write(&session_id, &notice) {
        Ok(VoiceWrite::Written) => {
            mode.note_hint_written(generation);
            if language.is_some() {
                mode.announced_language = language;
            }
            Some(Ok(Delivery::Notice))
        }
        Ok(VoiceWrite::Held(reason)) => {
            mode.entry_hint = EntryHint::Pending {
                text: notice,
                language,
            };
            Some(Ok(Delivery::Held {
                reason,
                first: mode.note_hold(reason),
            }))
        }
        Err(error) => Some(Err(error)),
    }
}

/// Tell the model the conversation ended — but only if it heard it begin.
///
/// `entry_hint_written` is the evidence. An entry hint still held when the mode
/// disarmed was never typed, so the model never read it: an exit hint after
/// that would be the only thing it ever heard about a mode it never had, which
/// is the contradictory pair criterion 3 forbids. An entry hint the composer
/// typed cannot be retracted, so the model believes it can speak and has to be
/// told otherwise.
///
/// One attempt. The mode is gone, so there is nothing left to hold it in: a
/// dialog or a draft on screen at disarm drops it (the caller logs a warning).
/// A target that has gone away refuses the write, which is reported rather
/// than swallowed: there is nobody left to tell, and that is not a failure of
/// this call.
pub fn deliver_exit_hint(
    sink: &dyn VoiceSink,
    disarmed: &Disarmed,
) -> Option<Result<VoiceWrite, String>> {
    if !disarmed.entry_hint_written {
        return None;
    }
    Some(sink.write(&disarmed.session_id, MODE_EXIT_HINT))
}

// ---------------------------------------------------------------------------
// Capture port and the runtime loop
// ---------------------------------------------------------------------------

/// One armed capture endpoint: a microphone and the recogniser behind it.
///
/// The desktop adapter is `commands::DesktopVoiceEndpoint`. The browser/remote
/// endpoint is Step 8 (story 818) and is deliberately absent rather than
/// stubbed — an unimplemented endpoint must refuse to arm, not arm and go deaf.
pub trait VoiceEndpoint: Send {
    /// Audio captured since the last call, empty when nothing arrived yet.
    /// `Err` is a hard capture failure and disarms the mode.
    fn drain(&mut self) -> Result<Vec<f32>, String>;
    /// False once the endpoint that armed the mode is gone.
    fn connected(&self) -> bool;
    /// Transcribe one closed utterance. Called without the mode lock held.
    fn transcribe(&self, audio: &[f32]) -> Result<Transcript, String>;
}

/// What a recogniser made of one closed utterance.
///
/// The language rides with the text instead of being asked for separately,
/// because it is a property of *this* pass: a recogniser that has moved on to
/// the next utterance can no longer answer "and what language was the previous
/// one".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    /// What was said. Empty when nothing was recognised.
    pub text: String,
    /// The two-letter code it was recognised as, or `None` when the recogniser
    /// named none — which includes every empty result.
    pub language: Option<String>,
}

/// Whether the bound session can still take hands-free input.
pub trait TargetProbe {
    fn accepts(&self, session_id: &str) -> bool;
}

/// The production loop needs both ports from the same application adapter.
pub trait VoicePort: VoiceSink + TargetProbe + Send + Sync {}

impl<T: VoiceSink + TargetProbe + Send + Sync> VoicePort for T {}

/// How long a stream may deliver nothing before it counts as a dead device.
///
/// A live microphone in a silent room still delivers samples — silence is a
/// value, not an absence. No samples at all means the stream stopped, which
/// cpal reports through a callback the capture owner cannot return from.
pub const DEVICE_SILENCE_TIMEOUT_MS: u64 = 5_000;

/// Whatever is speaking for us, and can be told to stop.
///
/// A port rather than the reply queue itself, for the same reason as
/// [`VoiceEndpoint`]: this loop must stay testable without an audio device or a
/// loaded synthesis graph, and barge-in is a rule about *when* to interrupt,
/// not about what interrupting does.
pub trait Interruptible: Send + Sync {
    /// Go quiet now, keeping the reply: somebody is talking and it is not yet
    /// known whether they are talking to us. Same constraint as `hush`.
    fn pause(&self);

    /// Carry on from where `pause` stopped: it was not for us.
    fn resume(&self);

    /// Stop talking now and open a new turn.
    ///
    /// Called from the capture loop on the tick where the user starts speaking,
    /// so it must not block: anything it waits for delays the next chunk of the
    /// user's own voice.
    fn hush(&self);
}

/// Everything the loop carries between ticks.
pub struct Capture {
    segmenter: Segmenter,
    /// When audio last arrived, for the starvation rule above.
    last_audio_ms: u64,
    device_silence_timeout_ms: u64,
    /// Our own voice, subtracted before the segmenter's VAD can hear it.
    /// Shared with the reply queue, which is the other half of the pair — see
    /// [`echo`](super::echo).
    echo: std::sync::Arc<parking_lot::Mutex<super::echo::EchoGuard>>,
    /// Interrupted when the user talks over a reply. `None` for a conversation
    /// armed without a voice, which is ordinary dictation and has nothing to
    /// interrupt.
    speaker: Option<std::sync::Arc<dyn Interruptible>>,
    /// When the speaker was paused for voice activity and has had no verdict
    /// yet. Every way out of this state is a verdict on the speech or a limit.
    paused_at_ms: Option<u64>,
}

impl Capture {
    pub fn new(
        config: SegmenterConfig,
        device_silence_timeout_ms: u64,
        now_ms: u64,
        echo: std::sync::Arc<parking_lot::Mutex<super::echo::EchoGuard>>,
        speaker: Option<std::sync::Arc<dyn Interruptible>>,
    ) -> Self {
        Self {
            segmenter: Segmenter::new(config),
            last_audio_ms: now_ms,
            device_silence_timeout_ms,
            echo,
            speaker,
            paused_at_ms: None,
        }
    }

    /// Carry on speaking if the reply is held for a verdict that will no
    /// longer come: the loop is ending, or the speech outlasted
    /// [`PAUSE_LIMIT_MS`].
    fn resume_speaker(&mut self) {
        if self.paused_at_ms.take().is_some()
            && let Some(speaker) = self.speaker.as_ref()
        {
            tracing::info!(
                source = "dictation",
                "speech: resumed, nothing addressed to us"
            );
            speaker.resume();
        }
    }

    /// Call when the loop ends, so a reply is never left paused behind it.
    pub fn release(&mut self) {
        self.resume_speaker();
    }

    /// Retained audio. Nothing in production needs this number; it exists so
    /// the bounded-memory rule can be asserted rather than asserted about.
    #[allow(dead_code)]
    pub fn retained_samples(&self) -> usize {
        self.segmenter.retained_samples()
    }
}

/// What one tick did.
#[derive(Debug)]
pub enum Tick {
    /// Nothing to do: the mode is not armed. The driver stops.
    NotArmed,
    /// Still armed. `delivered` is what the write attempted this tick did;
    /// `send_error` is a delivery the sink refused.
    Running {
        delivered: Option<Delivery>,
        send_error: Option<String>,
    },
    /// The mode disarmed itself. The driver sends the exit hint and stops.
    Disarmed(Disarmed),
}

/// One pass of the hands-free runtime: check the bindings still hold, segment
/// whatever audio arrived, transcribe what closed, and deliver what is due.
///
/// Clock-free like the machines it drives — `now_ms` comes from the driver, so
/// a test states the timeline instead of sleeping through it.
///
/// The mode lock is taken in short sections and **released across
/// `transcribe`**: a whisper pass takes seconds, and an abort issued during one
/// has to be able to land. That is what makes the generation check on the way
/// back out load-bearing rather than decorative.
pub fn tick(
    capture: &mut Capture,
    mode: &parking_lot::Mutex<HandsFree>,
    endpoint: &mut dyn VoiceEndpoint,
    target: &dyn TargetProbe,
    sink: &dyn VoiceSink,
    now_ms: u64,
) -> Tick {
    let Some(binding) = mode.lock().binding().cloned() else {
        return Tick::NotArmed;
    };

    if !target.accepts(&binding.session_id) {
        return disarmed_or_not_armed(mode.lock().note_session_closed(&binding.session_id));
    }
    if !endpoint.connected() {
        return disarmed_or_not_armed(mode.lock().note_owner_disconnected(&binding.owner));
    }

    let samples = match endpoint.drain() {
        Ok(samples) => samples,
        Err(error) => return disarmed_or_not_armed(mode.lock().note_device_failed(error)),
    };
    if samples.is_empty() {
        if now_ms.saturating_sub(capture.last_audio_ms) > capture.device_silence_timeout_ms {
            return disarmed_or_not_armed(mode.lock().note_device_failed(format!(
                "The capture device delivered no audio for {}s",
                capture.device_silence_timeout_ms / 1_000
            )));
        }
    } else {
        capture.last_audio_ms = now_ms;
    }

    // Subtract our own replies before the segmenter's VAD can hear them.
    // Without this the energy gate opens a turn on the audio we are speaking,
    // transcribes it, and answers itself.
    //
    // Deliberately after the starvation rule above, not before it: the
    // canceller works in whole 10 ms frames and carries the rest, so it can
    // return nothing from a chunk that did arrive. "The device is dead" is a
    // question about the device.
    let samples = capture.echo.lock().clean(&samples);

    // The other half of barge-in. Read before the push, because the edge is
    // what matters: a level trigger would pause once per tick.
    let had_voice = capture.segmenter.has_voice();
    let closed = capture.segmenter.push(&samples);
    // Somebody is talking — an unbroken `min_speech_ms`, not merely a few
    // frames over the floor and not frames added up over seconds: the
    // canceller removes the linear echo and not all of it, so a real room
    // leaves scattered residual frames. The first words are not lost to the
    // wait: the gate opens on the first frame, with its pre-roll. A closed
    // utterance has passed the same bar, so one that opened and closed inside
    // a single chunk counts too.
    //
    // The reply is only *paused*. Whether this is for us is the activation
    // rule's to say, and it needs the transcript: voice activity alone is also
    // a remark across the room and whatever echo was left. The verdict comes
    // from the loop below, and the pause never outlives it.
    if !had_voice
        && capture.paused_at_ms.is_none()
        && (capture.segmenter.has_voice()
            || closed
                .iter()
                .any(|utterance| capture.segmenter.is_voice(utterance)))
        && let Some(speaker) = capture.speaker.as_ref()
    {
        speaker.pause();
        capture.paused_at_ms = Some(now_ms);
    }
    if closed.is_empty() {
        if capture.segmenter.is_capturing() {
            mode.lock().note_capturing();
        } else {
            // An utterance that opened and closed on too little speech never
            // reaches the recogniser, so nothing else would end its phase.
            mode.lock().note_capture_discarded();
            capture.resume_speaker();
        }
    }
    if capture
        .paused_at_ms
        .is_some_and(|at| now_ms.saturating_sub(at) > PAUSE_LIMIT_MS)
    {
        capture.resume_speaker();
    }

    for utterance in closed {
        // The generation is read with the lock, before the pass starts. What
        // comes back is checked against the generation *then* current, which is
        // how a result that outlived its mode is refused.
        let generation = {
            let mut mode = mode.lock();
            mode.note_transcribing();
            mode.generation()
        };
        match endpoint.transcribe(&utterance.audio) {
            Ok(transcript) => {
                let outcome = mode.lock().accept_transcript(
                    generation,
                    &transcript.text,
                    transcript.language.as_deref(),
                    now_ms,
                );
                // A dropped turn is otherwise indistinguishable from a deaf
                // microphone, and a miss is undiagnosable without what Whisper
                // wrote. Only the first words go to the log, and only at DEBUG:
                // enough to see how the phrase was spelled, not the remark
                // the user did not address here.
                // The verdict on the pause. Addressed here: the reply is over and
                // the user's turn begins. Anything else: not for us, carry on.
                match outcome {
                    TranscriptOutcome::HeldBack { .. } | TranscriptOutcome::Activated { .. } => {
                        capture.paused_at_ms = None;
                        if let Some(speaker) = capture.speaker.as_ref() {
                            speaker.hush();
                        }
                    }
                    _ => capture.resume_speaker(),
                }
                match outcome {
                    TranscriptOutcome::Rejected => tracing::debug!(
                        source = "dictation",
                        heard = leading_excerpt(&transcript.text, REJECTED_EXCERPT_WORDS),
                        "Hands-free turn dropped: it did not begin with the activation phrase"
                    ),
                    TranscriptOutcome::Activated { window_until_ms } => tracing::debug!(
                        source = "dictation",
                        "Hands-free activated; the next turn needs no phrase before {window_until_ms}ms"
                    ),
                    // At INFO, unlike the drop above: an accepted turn is sent
                    // to the model anyway, and its first words are what tells
                    // a turn the user spoke from the reply the microphone
                    // heard coming back out of the speaker.
                    TranscriptOutcome::HeldBack { .. } => tracing::info!(
                        source = "dictation",
                        heard = leading_excerpt(&transcript.text, REJECTED_EXCERPT_WORDS),
                        "Hands-free turn accepted"
                    ),
                    _ => {}
                }
            }
            Err(error) => {
                return disarmed_or_not_armed(mode.lock().note_device_failed(error));
            }
        }
    }

    // Bound in its own statement, never in the `match` scrutinee: a temporary
    // there lives until the end of the whole match, and the error arm locks the
    // mode again. `parking_lot` is not reentrant, so that shape deadlocks the
    // runtime — and every status poll behind it — the first time the sink
    // refuses a delivery.
    let delivered = deliver_due(&mut mode.lock(), sink, now_ms);
    match delivered {
        Some(Ok(delivery)) => Tick::Running {
            delivered: Some(delivery),
            send_error: None,
        },
        Some(Err(error)) => {
            mode.lock().note_send_failed(&error);
            Tick::Running {
                delivered: None,
                send_error: Some(error),
            }
        }
        None => Tick::Running {
            delivered: None,
            send_error: None,
        },
    }
}

/// How long a reply stays paused for voice activity before it carries on
/// without a verdict. A whole turn is a sentence, 1.5 s of quiet and one
/// recognition pass; a pause past this is a noisy room, not a turn.
const PAUSE_LIMIT_MS: u64 = 8_000;

/// How often the driver thread ticks. Short enough that end-of-speech is not
/// noticeably late, long enough to cost nothing while nobody is speaking.
pub const POLL_INTERVAL_MS: u64 = 50;

/// The thread that drives [`tick`] while the mode is armed.
///
/// Dropping it stops the thread and waits for it — bounded by one poll plus,
/// at worst, one transcription in flight.
pub struct HandsFreeRuntime {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl HandsFreeRuntime {
    /// True once the loop has returned — it disarmed itself, or the mode was
    /// disarmed from elsewhere. The caller uses this to release the capture
    /// device, which cannot travel to this thread (cpal streams are `!Send`).
    pub fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
    }
}

impl Drop for HandsFreeRuntime {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Run the hands-free loop against the supplied delivery port until it disarms.
pub fn spawn_runtime(
    port: std::sync::Arc<dyn VoicePort>,
    mode: std::sync::Arc<parking_lot::Mutex<HandsFree>>,
    mut endpoint: Box<dyn VoiceEndpoint>,
    config: SegmenterConfig,
    echo: std::sync::Arc<parking_lot::Mutex<super::echo::EchoGuard>>,
    speaker: Option<std::sync::Arc<dyn Interruptible>>,
) -> HandsFreeRuntime {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_clone = stop.clone();
    let handle = std::thread::Builder::new()
        .name("hands-free-dictation".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let mut capture = Capture::new(config, DEVICE_SILENCE_TIMEOUT_MS, 0, echo, speaker);
            loop {
                if stop_clone.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let now_ms = started.elapsed().as_millis() as u64;
                let outcome = tick(
                    &mut capture,
                    &mode,
                    endpoint.as_mut(),
                    port.as_ref(),
                    port.as_ref(),
                    now_ms,
                );
                match outcome {
                    Tick::NotArmed => break,
                    Tick::Disarmed(disarmed) => {
                        tracing::info!(
                            source = "dictation",
                            session = %disarmed.session_id,
                            discarded_pending = disarmed.discarded_pending,
                            "Hands-free disarmed: {:?}",
                            disarmed.reason
                        );
                        // Every reason this loop ends for is one the user did
                        // not ask for — a closed target, a lost owner, a dead
                        // microphone — so a model that was told the mode began
                        // has to be told it ended here too, not only on the
                        // manual path in `commands::disarm_hands_free`.
                        report_exit_hint(port.as_ref(), &disarmed);
                        break;
                    }
                    Tick::Running {
                        delivered,
                        send_error,
                    } => {
                        if let Some(delivery) = delivered {
                            log_delivery(&mode.lock(), delivery);
                        }
                        if let Some(error) = send_error {
                            tracing::warn!(
                                source = "dictation",
                                "Hands-free delivery refused: {error}"
                            );
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
            }
            capture.release();
        })
        .expect("Failed to spawn hands-free thread");
    HandsFreeRuntime {
        stop,
        handle: Some(handle),
    }
}

/// A disarm that found nothing to disarm means someone else got there first.
fn disarmed_or_not_armed(disarmed: Option<Disarmed>) -> Tick {
    match disarmed {
        Some(disarmed) => Tick::Disarmed(disarmed),
        None => Tick::NotArmed,
    }
}

/// One INFO line per delivery: typed now, or held and why — a hold once, not
/// once per tick while it lasts.
pub(super) fn log_delivery(mode: &HandsFree, delivery: Delivery) {
    let session = mode
        .binding()
        .map(|binding| binding.session_id.as_str())
        .unwrap_or("");
    match delivery {
        Delivery::Turn => {
            tracing::info!(source = "dictation", session, "Hands-free turn typed now")
        }
        Delivery::Notice => tracing::info!(
            source = "dictation",
            session,
            "Hands-free start notice typed now"
        ),
        Delivery::Held {
            reason,
            first: true,
        } => tracing::info!(
            source = "dictation",
            session,
            reason = reason.as_str(),
            "Hands-free turn held"
        ),
        Delivery::Held { first: false, .. } => {}
    }
}

/// [`deliver_exit_hint`] with the outcome logged instead of returned.
///
/// The two disarm paths — the user's and this loop's — both want the notice
/// sent and neither has anybody to hand a failure to: by the time it is sent
/// the mode is already gone.
pub fn report_exit_hint(sink: &dyn VoiceSink, disarmed: &Disarmed) {
    match deliver_exit_hint(sink, disarmed) {
        Some(Ok(VoiceWrite::Written)) => tracing::info!(
            source = "dictation",
            session = %disarmed.session_id,
            "Hands-free end notice typed now"
        ),
        Some(Ok(VoiceWrite::Held(reason))) => tracing::warn!(
            source = "dictation",
            session = %disarmed.session_id,
            reason = reason.as_str(),
            "Hands-free end notice dropped: the composer could not take it at disarm"
        ),
        Some(Err(error)) => tracing::info!(
            source = "dictation",
            session = %disarmed.session_id,
            "Hands-free end notice not delivered: {error}"
        ),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const SR: f32 = SAMPLE_RATE as f32;

    fn speech(ms: u32) -> Vec<f32> {
        let n = ms_to_samples(ms);
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin() * 0.5)
            .collect()
    }

    fn silence(ms: u32) -> Vec<f32> {
        vec![0.0; ms_to_samples(ms)]
    }

    fn test_config() -> SegmenterConfig {
        SegmenterConfig {
            pre_roll_ms: 200,
            trailing_silence_ms: 400,
            min_speech_ms: 100,
            max_utterance_ms: 2_000,
            activity_rms: 0.01,
        }
    }

    // --- Segmentation -----------------------------------------------------

    /// The defect 811-9313 fixed one layer down, stated as a boundary rule: a
    /// phrase that ends in a pause is exactly what a hands-free utterance is,
    /// so the pause must close it and the speech must survive whole.
    #[test]
    fn a_phrase_that_ends_in_a_pause_closes_and_keeps_its_speech() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = speech(500);
        input.extend(silence(600));

        let closed = segmenter.push(&input);

        assert_eq!(
            closed.len(),
            1,
            "the pause must close exactly one utterance"
        );
        assert_eq!(closed[0].end, UtteranceEnd::Silence);
        assert!(
            closed[0].speech_ms >= 480,
            "the whole phrase is speech, got {}ms",
            closed[0].speech_ms
        );
        assert!(
            closed[0].audio.len() >= ms_to_samples(500),
            "the utterance must carry the phrase, not a trimmed window"
        );
    }

    /// A soft first syllable is below the activity floor, so without pre-roll
    /// the utterance starts a frame late and the word is clipped.
    ///
    /// Measured against the same input with the pre-roll turned off, because a
    /// length assertion on its own is vacuous here: the utterance also carries
    /// its trailing silence, which is longer than the pre-roll and hides its
    /// absence completely.
    #[test]
    fn speech_carries_the_pre_roll_that_preceded_it() {
        let mut input = silence(1_000);
        input.extend(speech(300));
        input.extend(silence(600));

        let mut with_pre_roll = Segmenter::new(test_config());
        let mut without = Segmenter::new(SegmenterConfig {
            pre_roll_ms: 0,
            ..test_config()
        });

        let kept = with_pre_roll.push(&input);
        let clipped = without.push(&input);

        assert_eq!(kept.len(), 1);
        assert_eq!(clipped.len(), 1);
        let pre_roll = kept[0].audio.len() - clipped[0].audio.len();
        assert!(
            pre_roll >= ms_to_samples(200) - FRAME_SAMPLES,
            "expected ~200ms of audio ahead of the first speech frame, got {pre_roll} samples"
        );
    }

    /// A cough or a chair is speech-shaped for one frame. Sending it would put
    /// noise in front of a model.
    #[test]
    fn a_blip_too_short_to_be_speech_sends_nothing() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = speech(40);
        input.extend(silence(600));

        assert!(
            segmenter.push(&input).is_empty(),
            "40ms is below the 100ms floor and must be discarded"
        );
    }

    /// A pause *inside* a sentence is shorter than the quiet interval and must
    /// not split the sentence in two turns.
    #[test]
    fn a_short_pause_inside_a_sentence_does_not_split_it() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = speech(300);
        input.extend(silence(200));
        input.extend(speech(300));
        input.extend(silence(600));

        let closed = segmenter.push(&input);

        assert_eq!(closed.len(), 1, "one sentence, one utterance");
        assert!(closed[0].audio.len() >= ms_to_samples(800));
    }

    /// A normal breath can exceed the old 800 ms threshold. The recogniser
    /// must receive both phrases together, before either can be submitted.
    #[test]
    fn a_one_second_breath_keeps_both_phrases_in_one_utterance() {
        let mut segmenter = Segmenter::new(SegmenterConfig::default());
        let mut first = speech(500);
        first.extend(silence(1_000));
        assert!(
            segmenter.push(&first).is_empty(),
            "a breath is not the end of the turn"
        );

        let mut rest = speech(500);
        rest.extend(silence(1_600));
        let closed = segmenter.push(&rest);
        assert_eq!(closed.len(), 1, "both phrases must reach one transcription");
        assert_eq!(closed[0].speech_ms, 1_000);
        assert!(closed[0].audio.len() >= ms_to_samples(2_000));
    }

    /// Someone who never stops talking must not grow an unbounded buffer, and
    /// must not stall delivery forever either.
    #[test]
    fn an_endless_monologue_is_cut_at_the_maximum_length() {
        let mut segmenter = Segmenter::new(test_config());

        let closed = segmenter.push(&speech(5_000));

        assert!(!closed.is_empty(), "the cap must close an utterance");
        assert_eq!(closed[0].end, UtteranceEnd::MaxLength);
        assert!(
            closed[0].audio.len() <= ms_to_samples(2_000) + FRAME_SAMPLES,
            "an utterance may not exceed the configured maximum"
        );
        assert!(
            segmenter.retained_samples() <= ms_to_samples(2_000) + FRAME_SAMPLES,
            "and the segmenter may not hold more than one utterance's worth"
        );
    }

    /// An armed microphone in a quiet room is the steady state of this feature.
    /// It may not accumulate audio, and it may not produce work.
    #[test]
    fn silence_never_grows_a_buffer_or_produces_an_utterance() {
        let mut segmenter = Segmenter::new(test_config());
        let pre_roll = ms_to_samples(200);

        for _ in 0..600 {
            assert!(segmenter.push(&silence(100)).is_empty());
        }

        assert!(
            segmenter.retained_samples() <= pre_roll + FRAME_SAMPLES,
            "60s of silence retained {} samples; the pre-roll is {pre_roll}",
            segmenter.retained_samples()
        );
        assert!(!segmenter.is_capturing());
    }

    /// Audio arrives in device-sized chunks that do not align to frames. The
    /// boundaries must not depend on where the chunks happen to split.
    #[test]
    fn chunking_does_not_change_the_boundaries() {
        let mut input = speech(500);
        input.extend(silence(600));

        let mut whole = Segmenter::new(test_config());
        let expected = whole.push(&input);

        let mut split = Segmenter::new(test_config());
        let mut closed = Vec::new();
        for chunk in input.chunks(377) {
            closed.extend(split.push(chunk));
        }

        assert_eq!(
            closed, expected,
            "frame alignment must not leak into the cut"
        );
    }

    // --- Mode -------------------------------------------------------------

    fn armed() -> HandsFree {
        let mut mode = HandsFree::new(1_500);
        mode.arm("target", "desktop", true).expect("arm");
        mode
    }

    #[test]
    fn an_unsupported_target_stays_unavailable() {
        let mut mode = HandsFree::new(1_500);

        assert_eq!(
            mode.arm("ego", "desktop", false),
            Err(ArmError::UnsupportedTarget)
        );
        assert_eq!(*mode.phase(), Phase::Disarmed);
        assert!(
            mode.binding().is_none(),
            "a refused arm must not bind anything"
        );
    }

    /// The reason this story exists: whatever the user clicks on afterwards,
    /// the transcript goes to the session that was bound when they armed.
    #[test]
    fn delivery_follows_the_bound_session_not_the_focus() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.note_capturing();
        mode.note_transcribing();

        // A focus change is not an input to this machine at all — nothing here
        // reads "the active tab", which is what makes the redirect impossible.
        assert_eq!(
            mode.accept_transcript(generation, "run the tests", None, 0),
            TranscriptOutcome::HeldBack { send_at_ms: 1_500 }
        );
        let send = mode.poll_send(1_500).expect("hold-back expired");

        assert_eq!(send.session_id, "target");
        assert_eq!(send.text, "run the tests");
    }

    #[test]
    fn nothing_is_enqueued_before_the_hold_back_expires() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "delete everything", None, 1_000);

        assert!(
            mode.poll_send(2_499).is_none(),
            "the hold-back must still be running"
        );
        assert_eq!(*mode.phase(), Phase::HoldingBack);
        assert_eq!(mode.pending_text(), Some("delete everything"));
        assert!(mode.poll_send(2_500).is_some());
    }

    /// A pause mid-sentence longer than the trailing silence closes the first
    /// half as an utterance of its own. The continuation arrives while the
    /// first half is still held back, and it belongs to the same turn: losing
    /// either half sends the model a sentence the user never said.
    #[test]
    fn speech_that_arrives_during_the_hold_back_joins_the_pending_turn() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "open the file", None, 0);

        assert_eq!(
            mode.accept_transcript(generation, "and run the tests", Some("en"), 1_000),
            TranscriptOutcome::HeldBack { send_at_ms: 2_500 },
            "the user is still talking, so the hold-back starts again"
        );
        assert!(
            mode.poll_send(1_500).is_none(),
            "the first half's deadline must not send the turn early"
        );
        let send = mode.poll_send(2_500).expect("hold-back expired");
        assert_eq!(send.text, "open the file\nand run the tests");
        assert_eq!(send.language.as_deref(), Some("en"));
        assert!(
            mode.poll_send(u64::MAX).is_none(),
            "two utterances are one turn, delivered once"
        );
    }

    #[test]
    fn keyword_turn_keeps_a_five_second_follow_up_in_the_same_terminal_message() {
        let mut mode = armed_with_phrase("computer");
        let generation = mode.generation();
        let queue = FakeSink::default();
        mode.accept_transcript(generation, "computer appena faccio una pausa", None, 0);

        assert!(deliver_due(&mut mode, &queue, 1_500).is_none());
        assert!(
            queue.written.borrow().is_empty(),
            "the first phrase was sent alone"
        );
        mode.accept_transcript(generation, "la frase continua", None, 4_900);
        assert_eq!(
            mode.pending_text(),
            Some("appena faccio una pausa\nla frase continua")
        );
        assert!(deliver_due(&mut mode, &queue, 9_899).is_none());
        assert_eq!(
            deliver_due(&mut mode, &queue, 9_900),
            Some(Ok(Delivery::Turn))
        );
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written("appena faccio una pausa\nla frase continua")]
        );
    }

    /// Boss's rule: the abort kills the mode, not just the utterance.
    #[test]
    fn a_manual_abort_disarms_the_whole_mode_and_discards_the_pending_send() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "never send this", None, 0);

        let disarmed = mode.disarm(DisarmReason::Manual).expect("was armed");

        assert!(disarmed.discarded_pending);
        assert_eq!(*mode.phase(), Phase::Disarmed);
        assert!(mode.binding().is_none());
        assert!(
            mode.poll_send(u64::MAX).is_none(),
            "a discarded transcript may not surface later"
        );
        assert_eq!(
            mode.accept_transcript(generation, "late result", None, 10_000),
            TranscriptOutcome::NotArmed,
            "a disarmed mode must not re-arm itself on the next phrase"
        );
    }

    #[test]
    fn a_transcript_from_a_previous_generation_cannot_send() {
        let mut mode = armed();
        let stale_generation = mode.generation();
        mode.disarm(DisarmReason::Manual);
        mode.arm("target", "desktop", true).expect("re-arm");

        assert_eq!(
            mode.accept_transcript(stale_generation, "from the last session", None, 0),
            TranscriptOutcome::Stale
        );
        assert!(mode.poll_send(u64::MAX).is_none());
    }

    #[test]
    fn an_empty_transcription_sends_nothing_and_returns_to_waiting() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.note_transcribing();

        assert_eq!(
            mode.accept_transcript(generation, "   ", None, 0),
            TranscriptOutcome::Empty
        );
        assert_eq!(*mode.phase(), Phase::Waiting);
        assert!(mode.poll_send(u64::MAX).is_none());
    }

    // --- Activation phrase ------------------------------------------------

    /// The window is 10s here so a test can state "inside" and "outside"
    /// against round numbers rather than against the shipped default.
    fn armed_with_phrase(phrase: &str) -> HandsFree {
        let mut mode = HandsFree::new(1_500);
        mode.set_activation(phrase, 10_000);
        mode.arm("target", "desktop", true).expect("arm");
        mode
    }

    /// The setting is optional, and an unset one may not turn dictation into a
    /// feature that ignores the user.
    #[test]
    fn an_empty_activation_phrase_lets_every_turn_through() {
        let mut mode = armed_with_phrase("");
        let generation = mode.generation();

        assert_eq!(
            mode.accept_transcript(generation, "apri il file", None, 0),
            TranscriptOutcome::HeldBack { send_at_ms: 1_500 }
        );
        assert_eq!(mode.pending_text(), Some("apri il file"));
    }

    /// The phrase addresses the tool; the model must never see it.
    #[test]
    fn a_configured_phrase_gates_a_turn_and_is_stripped_from_what_is_sent() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();

        assert_eq!(
            mode.accept_transcript(generation, "Ciao Tuic, apri il file", None, 0),
            TranscriptOutcome::HeldBack { send_at_ms: 5_000 }
        );
        assert_eq!(mode.pending_text(), Some("apri il file"));
    }

    /// A recogniser decides capitalisation and punctuation on its own, and an
    /// Italian phrase carries accents in both cases. None of that may decide
    /// whether the user is heard.
    #[test]
    fn the_phrase_matches_across_case_italian_accents_and_punctuation() {
        for spoken in [
            "attività tuic che ore sono",
            "ATTIVITÀ TUIC: che ore sono",
            "Attività, Tuic! Che ore sono",
            "  «Attività Tuic» — che ore sono  ",
        ] {
            let mut mode = armed_with_phrase("Attività Tuic");
            let generation = mode.generation();

            assert_eq!(
                mode.accept_transcript(generation, spoken, None, 0),
                TranscriptOutcome::HeldBack { send_at_ms: 5_000 },
                "{spoken:?} must activate"
            );
            assert_eq!(
                mode.pending_text().map(str::to_lowercase).as_deref(),
                Some("che ore sono"),
                "{spoken:?} must submit the speech without the phrase"
            );
        }
    }

    /// "Complete leading words" is the rule that keeps a homophone-rich name
    /// from firing on everything that starts with it.
    #[test]
    fn a_longer_word_that_merely_starts_with_the_phrase_does_not_activate() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();

        assert_eq!(
            mode.accept_transcript(generation, "Ciao Tuicommander, cancella tutto", None, 0),
            TranscriptOutcome::Rejected
        );
        assert!(mode.pending_text().is_none());
        assert_eq!(*mode.phase(), Phase::Waiting);
        assert!(mode.poll_send(u64::MAX).is_none());
    }

    /// The phrase opens a turn. Hearing it in the middle of a sentence means
    /// the user was talking *about* it, to somebody else.
    #[test]
    fn a_phrase_that_is_not_leading_does_not_activate() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();

        assert_eq!(
            mode.accept_transcript(generation, "Per favore, ciao tuic, cancella tutto", None, 0),
            TranscriptOutcome::Rejected
        );
        assert!(mode.pending_text().is_none());
    }

    /// What Whisper large-v3-turbo may write for a spoken "senti mac" in
    /// Italian. It invents the accent on a verb it reads as past tense, joins a
    /// short name onto the word before it, and spells an English name the way
    /// it sounds. Every one of these was the user addressing the tool.
    #[test]
    fn the_phrase_matches_the_spellings_whisper_invents_for_it() {
        for spoken in [
            "Senti, Mac. Che ore sono?",
            "Senti Mac, che ore sono?",
            "Senti, Mack, che ore sono?",
            "Sentì mac che ore sono?",
            "SENTÌ, MACK! Che ore sono?",
            "Sentimac, che ore sono?",
            "Senti-Mac, che ore sono?",
        ] {
            let mut mode = armed_with_phrase("senti mac");
            let generation = mode.generation();

            assert_eq!(
                mode.accept_transcript(generation, spoken, None, 0),
                TranscriptOutcome::HeldBack { send_at_ms: 5_000 },
                "{spoken:?} must activate"
            );
            assert_eq!(
                mode.pending_text().map(str::to_lowercase).as_deref(),
                Some("che ore sono?"),
                "{spoken:?} must submit the speech without the phrase"
            );
        }
    }

    /// The tolerance above is one extra trailing letter on a word of three or
    /// more, plus the joined form. Everything below is ordinary Italian that
    /// starts close to the phrase and is addressed to a person: a shorter word
    /// ("ma"), a different last letter ("mai", "max"), a longer word
    /// ("sentimento", "macchina"), a different verb ("sento"), or the phrase in
    /// the middle of a sentence.
    #[test]
    fn speech_that_only_resembles_the_phrase_does_not_activate() {
        for spoken in [
            "Senti, ma che ore sono?",
            "Senti mai niente?",
            "Senti Max, vieni qui",
            "Sentimento e ragione",
            "Senti macchina nuova",
            "Sento mac che parte",
            "Senti, maccheroni stasera?",
            "Ehi, senti mac, che ore sono?",
            "Senti",
        ] {
            let mut mode = armed_with_phrase("senti mac");
            let generation = mode.generation();

            assert_eq!(
                mode.accept_transcript(generation, spoken, None, 0),
                TranscriptOutcome::Rejected,
                "{spoken:?} must not activate"
            );
            assert!(mode.pending_text().is_none(), "{spoken:?}");
        }
    }

    /// A configured phrase carries its own accents and capitals; the setting
    /// is folded exactly like the transcript, so either spelling works.
    #[test]
    fn an_accented_or_capitalised_phrase_setting_matches_the_plain_transcript() {
        let mut mode = armed_with_phrase("  Sentì   MAC ");
        let generation = mode.generation();

        assert_eq!(
            mode.accept_transcript(generation, "senti mac apri il file", None, 0),
            TranscriptOutcome::HeldBack { send_at_ms: 5_000 }
        );
        assert_eq!(mode.pending_text(), Some("apri il file"));
    }

    /// The rejection log names what was heard, bounded, so a miss can be
    /// diagnosed without recording the whole remark.
    #[test]
    fn the_rejection_excerpt_keeps_the_first_words_verbatim_and_nothing_more() {
        assert_eq!(
            leading_excerpt("Sentì, Mack. Che ore sono adesso?", 4),
            "Sentì, Mack. Che ore"
        );
        assert_eq!(leading_excerpt("  Ciao.  ", 4), "Ciao");
        assert_eq!(leading_excerpt("", 4), "");
    }

    /// Saying only the phrase is how a user opens a turn before knowing what
    /// to ask. It must arm the window and submit nothing at all.
    #[test]
    fn the_phrase_alone_opens_the_window_without_sending_anything() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();

        assert_eq!(
            mode.accept_transcript(generation, "Ciao Tuic.", None, 0),
            TranscriptOutcome::Activated {
                window_until_ms: 10_000
            }
        );
        assert!(mode.pending_text().is_none());
        assert_eq!(*mode.phase(), Phase::Waiting);

        assert_eq!(
            mode.accept_transcript(generation, "apri il file", None, 1_000),
            TranscriptOutcome::HeldBack { send_at_ms: 6_000 },
            "the phrase must have opened the window for what follows"
        );
        assert_eq!(mode.pending_text(), Some("apri il file"));
    }

    /// A conversation is a sequence of turns. Repeating the phrase before each
    /// one would make hands-free unusable.
    #[test]
    fn follow_up_speech_inside_the_window_needs_no_phrase() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();
        mode.accept_transcript(generation, "Ciao Tuic, apri il file", None, 0);
        mode.poll_send(5_000).expect("first turn");

        assert_eq!(
            mode.accept_transcript(generation, "e adesso committa", None, 9_999),
            TranscriptOutcome::HeldBack { send_at_ms: 14_999 }
        );
        assert_eq!(mode.pending_text(), Some("e adesso committa"));
    }

    /// The window is bounded so a microphone left armed after a conversation
    /// stops forwarding the room.
    #[test]
    fn speech_after_the_window_expires_needs_the_phrase_again() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();
        mode.accept_transcript(generation, "Ciao Tuic, apri il file", None, 0);
        mode.poll_send(5_000).expect("first turn");

        assert_eq!(
            mode.accept_transcript(generation, "passami il sale", None, 10_000),
            TranscriptOutcome::Rejected,
            "the window must not survive its own bound"
        );
        assert!(mode.pending_text().is_none());

        assert_eq!(
            mode.accept_transcript(generation, "Ciao Tuic, committa", None, 10_000),
            TranscriptOutcome::HeldBack { send_at_ms: 15_000 },
            "the phrase must still reopen it"
        );
        assert_eq!(mode.pending_text(), Some("committa"));
    }

    /// Every way the mode can end is a way the user stops addressing it. None
    /// of them may hand the next arm a window that is already open.
    #[test]
    fn every_disarm_closes_the_activation_window() {
        type Closer = fn(&mut HandsFree);
        let closers: [(&str, Closer); 4] = [
            ("a manual abort", |mode| {
                mode.disarm(DisarmReason::Manual);
            }),
            ("a closed target", |mode| {
                mode.note_session_closed("target");
            }),
            ("a disconnected owner", |mode| {
                mode.note_owner_disconnected("desktop");
            }),
            ("a capture failure", |mode| {
                mode.note_device_failed("input device disappeared");
            }),
        ];

        for (label, close) in closers {
            let mut mode = armed_with_phrase("ciao tuic");
            let generation = mode.generation();
            assert!(matches!(
                mode.accept_transcript(generation, "Ciao Tuic", None, 0),
                TranscriptOutcome::Activated { .. }
            ));

            close(&mut mode);
            mode.arm("target", "desktop", true).expect("re-arm");
            let generation = mode.generation();

            assert_eq!(
                mode.accept_transcript(generation, "cancella tutto", None, 100),
                TranscriptOutcome::Rejected,
                "{label} must close the activation window"
            );
        }
    }

    #[test]
    fn a_closed_target_disarms_and_a_different_session_does_not() {
        let mut mode = armed();

        assert!(mode.note_session_closed("someone-else").is_none());
        assert!(mode.binding().is_some());

        let disarmed = mode.note_session_closed("target").expect("bound target");
        assert_eq!(disarmed.reason, DisarmReason::TargetClosed);
        assert!(mode.binding().is_none());
    }

    #[test]
    fn a_disconnected_owner_disarms_and_a_different_owner_does_not() {
        let mut mode = armed();

        assert!(mode.note_owner_disconnected("browser-42").is_none());
        assert!(mode.binding().is_some());

        assert_eq!(
            mode.note_owner_disconnected("desktop")
                .expect("bound owner")
                .reason,
            DisarmReason::OwnerDisconnected
        );
    }

    #[test]
    fn a_failed_device_disarms_into_a_reported_error() {
        let mut mode = armed();
        mode.note_capturing();

        let disarmed = mode
            .note_device_failed("input device disappeared")
            .expect("armed");

        assert!(disarmed.discarded_capture);
        assert_eq!(*mode.phase(), Phase::Error);
        assert_eq!(mode.last_error(), Some("input device disappeared"));
        assert!(mode.binding().is_none());
    }

    // --- Delivery ---------------------------------------------------------

    /// A sink that records what it was asked to type, and nothing else — the
    /// point being that a delivery path with any other exit would show up here
    /// as a message that never reached `write`. `hold` plays the composer
    /// refusing (a dialog, a draft); `fail` plays a target that has gone.
    #[derive(Default)]
    struct FakeSink {
        written: RefCell<Vec<(String, String)>>,
        hold: RefCell<Option<VoiceHold>>,
        fail: RefCell<Option<String>>,
    }

    impl VoiceSink for FakeSink {
        fn write(&self, session_id: &str, text: &str) -> Result<VoiceWrite, String> {
            if let Some(error) = self.fail.borrow().as_ref() {
                return Err(error.clone());
            }
            if let Some(reason) = *self.hold.borrow() {
                return Ok(VoiceWrite::Held(reason));
            }
            self.written
                .borrow_mut()
                .push((session_id.to_string(), text.to_string()));
            Ok(VoiceWrite::Written)
        }
    }

    fn written(text: &str) -> (String, String) {
        ("target".to_string(), text.to_string())
    }

    #[test]
    fn a_held_back_transcript_is_typed_when_its_hold_back_expires() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        mode.accept_transcript(generation, "run the tests", None, 0);

        assert!(deliver_due(&mut mode, &queue, 1_000).is_none());
        let delivered = deliver_due(&mut mode, &queue, 1_500).expect("hold-back expired");

        assert_eq!(delivered, Ok(Delivery::Turn));
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written("run the tests")]
        );
        assert_eq!(*mode.phase(), Phase::Delivered);
        assert_eq!(mode.pending_text(), None);
    }

    /// The earcons key on these counters, not on the phase: a poll can miss a
    /// `Delivered` the next utterance overwrote, and a gate drop never has a
    /// phase of its own at all.
    #[test]
    fn only_a_delivered_turn_advances_the_delivered_count() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        mode.accept_transcript(generation, "run the tests", None, 0);
        assert_eq!(mode.delivered_turns(), 0, "held back is not delivered");

        assert!(matches!(deliver_due(&mut mode, &queue, 1_500), Some(Ok(_))));
        assert_eq!(mode.delivered_turns(), 1);
        mode.note_hint_written(generation);
        assert_eq!(
            mode.delivered_turns(),
            1,
            "the entry hint is not a spoken turn"
        );
        assert_eq!(mode.dropped_turns(), 0);
    }

    #[test]
    fn only_a_gate_rejection_advances_the_dropped_count() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();

        mode.accept_transcript(generation, "   ", None, 0);
        mode.accept_transcript(generation, "Ciao Tuic", None, 0);
        assert_eq!(
            mode.dropped_turns(),
            0,
            "silence and the bare phrase are not drops"
        );

        mode.accept_transcript(generation, "passami il sale", None, 20_000);
        assert_eq!(mode.dropped_turns(), 1);
        assert_eq!(mode.delivered_turns(), 0);
    }

    /// Monotonic across arms, so a client comparing two polls never reads a
    /// re-arm as a turn.
    #[test]
    fn the_turn_counts_survive_a_disarm_and_re_arm() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();
        mode.accept_transcript(generation, "passami il sale", None, 0);
        mode.disarm(DisarmReason::Manual);
        mode.arm("target", "desktop", true).expect("re-arm");

        assert_eq!(mode.dropped_turns(), 1);
    }

    /// A dialog or a draft holds the turn *here*, not in the Compose queue: it
    /// stays visible and cancellable in the mode, is retried every tick, and is
    /// typed the first tick the composer is free. The hold is reported once,
    /// not once per tick.
    #[test]
    fn a_turn_the_composer_holds_stays_in_the_mode_and_is_retried() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        *queue.hold.borrow_mut() = Some(VoiceHold::Question);
        mode.accept_transcript(generation, "yes, go on", None, 0);

        assert_eq!(
            deliver_due(&mut mode, &queue, 1_500),
            Some(Ok(Delivery::Held {
                reason: VoiceHold::Question,
                first: true
            }))
        );
        assert_eq!(mode.pending_text(), Some("yes, go on"));
        assert_eq!(*mode.phase(), Phase::HoldingBack);
        assert_eq!(mode.delivered_turns(), 0);
        assert_eq!(
            deliver_due(&mut mode, &queue, 1_550),
            Some(Ok(Delivery::Held {
                reason: VoiceHold::Question,
                first: false
            })),
            "the same hold is not news on the next tick"
        );

        *queue.hold.borrow_mut() = None;
        assert_eq!(
            deliver_due(&mut mode, &queue, 1_600),
            Some(Ok(Delivery::Turn))
        );
        assert_eq!(queue.written.borrow().as_slice(), [written("yes, go on")]);
        assert_eq!(mode.pending_text(), None);
    }

    /// Speech that arrives while a turn is held is the same conversation, and
    /// reaches the model as one message in the order it was said.
    #[test]
    fn speech_while_a_turn_is_held_joins_it() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        *queue.hold.borrow_mut() = Some(VoiceHold::Draft);
        mode.accept_transcript(generation, "first", None, 0);
        deliver_due(&mut mode, &queue, 1_500);

        mode.accept_transcript(generation, "second", None, 2_000);
        *queue.hold.borrow_mut() = None;
        assert!(
            deliver_due(&mut mode, &queue, 2_100).is_none(),
            "the joined turn restarts its hold-back"
        );
        deliver_due(&mut mode, &queue, 3_500)
            .expect("due")
            .expect("written");

        assert_eq!(queue.written.borrow().as_slice(), [written("first second")]);
    }

    /// A held turn never reached the model, so a disarm discards it like one
    /// still inside its hold-back.
    #[test]
    fn a_disarm_discards_a_held_turn() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        *queue.hold.borrow_mut() = Some(VoiceHold::Question);
        mode.accept_transcript(generation, "delete it", None, 0);
        deliver_due(&mut mode, &queue, 1_500);

        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");

        assert!(disarmed.discarded_pending);
        assert!(queue.written.borrow().is_empty());
    }

    #[test]
    fn a_refused_write_is_reported_and_types_nothing() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        *queue.fail.borrow_mut() = Some("Session is not running an agent".to_string());
        mode.accept_transcript(generation, "run the tests", None, 0);

        let outcome = deliver_due(&mut mode, &queue, 1_500).expect("hold-back expired");

        assert_eq!(outcome, Err("Session is not running an agent".to_string()));
        assert!(queue.written.borrow().is_empty());
        assert_eq!(mode.delivered_turns(), 0);
    }

    // --- Telling the model the mode changed (821-842a) --------------------

    /// The model cannot see a microphone open. If nothing says so, the `voice`
    /// tool is listed and never called, because a model with no reason to
    /// speak writes text.
    #[test]
    fn a_new_conversation_tells_the_model_it_can_answer_out_loud() {
        let mut mode = armed();
        let queue = FakeSink::default();

        let sent = deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None).expect("armed");

        assert_eq!(sent, Ok(Delivery::Notice));
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written(MODE_ENTRY_HINT)],
            "the notice goes through the sink like every spoken turn"
        );
        assert_eq!(
            *mode.phase(),
            Phase::Waiting,
            "a notice is not speech; the phase still describes what the user is doing"
        );
    }

    /// The sink types an entry into a terminal and submits it. A second line
    /// submits the first half of a sentence and leaves the rest as a command.
    /// In some Claude Code sessions MCP tools are deferred: listed by name
    /// only, loaded on demand. A notice that says just "the voice tool" gives
    /// the model nothing to search for, so it names the server and says how to
    /// load the tool.
    #[test]
    fn the_start_notice_names_the_mcp_tool_and_how_to_load_it() {
        assert!(
            MODE_ENTRY_HINT.contains("`voice` tool of the TUICommander MCP server (tuicommander)"),
            "{MODE_ENTRY_HINT}"
        );
        assert!(
            MODE_ENTRY_HINT.contains("search your deferred tools for \"voice\""),
            "{MODE_ENTRY_HINT}"
        );
        assert!(MODE_ENTRY_HINT.contains("action \"speak\""));
    }

    #[test]
    fn neither_notice_can_submit_half_of_itself() {
        for notice in [MODE_ENTRY_HINT, MODE_EXIT_HINT] {
            assert!(
                !notice.contains('\n') && !notice.contains('\r'),
                "a notice must be one line: {notice:?}"
            );
            assert!(!notice.trim().is_empty());
        }
        assert!(
            MODE_ENTRY_HINT.contains("`voice` tool"),
            "the entry notice exists to name the capability"
        );
        assert!(
            !MODE_EXIT_HINT.contains("voice tool can speak"),
            "the exit notice may not read as an offer"
        );
    }

    /// An empty setting is "no preference", not "say nothing": turning the
    /// notice off is `hands_free_notify_model`'s job, and a blank field that
    /// silenced it would ship the voice tool with no reason to call it.
    #[test]
    fn an_empty_start_notice_setting_sends_the_built_in_notice() {
        for configured in ["", "   ", "\n\t "] {
            assert_eq!(
                entry_hint_text(configured),
                MODE_ENTRY_HINT,
                "{configured:?}"
            );
        }
    }

    #[test]
    fn a_configured_start_notice_is_what_the_model_reads() {
        let mut mode = armed();
        let queue = FakeSink::default();
        let notice = entry_hint_text("  Voice is on. Answer in one sentence.  ");

        deliver_entry_hint(&mut mode, &queue, &notice, None)
            .expect("armed")
            .expect("written");

        assert_eq!(
            queue.written.borrow().as_slice(),
            [written("Voice is on. Answer in one sentence.")]
        );
    }

    /// The settings field is multiline, but the sink submits on a newline:
    /// a notice typed on three lines would reach the model as three prompts.
    #[test]
    fn a_multiline_start_notice_reaches_the_terminal_as_one_line() {
        let notice = entry_hint_text("Voice is on.\nKeep it short.\r\n\n  Use Italian.");

        assert_eq!(notice, "Voice is on. Keep it short. Use Italian.");
    }

    /// A notice the composer cannot take yet waits in the mode, is retried
    /// every tick, and goes before any spoken turn: a turn read before "you
    /// can answer out loud" has been answered in text already. Its language is
    /// announced only once it is typed.
    #[test]
    fn a_held_start_notice_is_retried_before_any_turn() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        *queue.hold.borrow_mut() = Some(VoiceHold::Question);

        let sent =
            deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, Some("it")).expect("armed");
        assert!(matches!(sent, Ok(Delivery::Held { .. })));
        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        assert!(matches!(
            deliver_due(&mut mode, &queue, 1_500),
            Some(Ok(Delivery::Held { .. }))
        ));

        *queue.hold.borrow_mut() = None;
        assert_eq!(
            deliver_due(&mut mode, &queue, 1_550),
            Some(Ok(Delivery::Notice)),
            "the notice alone, on the tick the composer frees up"
        );
        assert_eq!(
            deliver_due(&mut mode, &queue, 1_600),
            Some(Ok(Delivery::Turn))
        );
        assert_eq!(
            entries(&queue),
            vec![
                format!("{MODE_ENTRY_HINT} Reply in Italian."),
                "esegui i test".to_string(),
            ]
        );
    }

    /// Criterion 3, the contradictory pair: a notice the composer never typed
    /// is dropped with the mode, and nothing may be written to undo something
    /// the model never read.
    #[test]
    fn a_start_notice_the_model_never_read_is_not_contradicted() {
        let mut mode = armed();
        let queue = FakeSink::default();
        *queue.hold.borrow_mut() = Some(VoiceHold::Question);
        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None)
            .expect("armed")
            .expect("held");

        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");
        *queue.hold.borrow_mut() = None;

        assert!(!disarmed.entry_hint_written);
        assert_eq!(
            deliver_exit_hint(&queue, &disarmed),
            None,
            "an end notice would be the only thing the model ever heard about the mode"
        );
        assert!(queue.written.borrow().is_empty());
    }

    /// The other half: once the composer has typed it, nothing can take it
    /// back, so the model believes it can speak until it is told otherwise.
    #[test]
    fn a_start_notice_the_model_read_is_undone_when_the_conversation_ends() {
        let mut mode = armed();
        let queue = FakeSink::default();
        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None)
            .expect("armed")
            .expect("written");

        let disarmed = mode.disarm(DisarmReason::TargetClosed).expect("armed");
        let sent = deliver_exit_hint(&queue, &disarmed).expect("the model was told");

        assert_eq!(sent, Ok(VoiceWrite::Written));
        assert_eq!(
            queue.written.borrow().last(),
            Some(&written(MODE_EXIT_HINT))
        );
    }

    /// The mode is gone at disarm, so there is nothing left to hold the end
    /// notice in: a dialog on screen then drops it rather than parking it.
    #[test]
    fn an_end_notice_the_composer_cannot_take_is_dropped() {
        let mut mode = armed();
        let queue = FakeSink::default();
        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None)
            .expect("armed")
            .expect("written");
        *queue.hold.borrow_mut() = Some(VoiceHold::Question);

        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");

        assert_eq!(
            deliver_exit_hint(&queue, &disarmed),
            Some(Ok(VoiceWrite::Held(VoiceHold::Question)))
        );
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written(MODE_ENTRY_HINT)]
        );
    }

    /// The setting is off, so nothing was ever sent — and an end notice on its
    /// own is worse than silence.
    #[test]
    fn a_conversation_the_model_was_never_told_about_ends_quietly() {
        let mut mode = armed();
        let queue = FakeSink::default();

        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");

        assert!(!disarmed.entry_hint_written);
        assert_eq!(deliver_exit_hint(&queue, &disarmed), None);
        assert!(queue.written.borrow().is_empty());
    }

    /// Criterion 3's other half: whatever the user does with the hotkey, what
    /// the model ends up holding matches the mode it is in, in the order the
    /// changes happened — never two "on"s and never an "off" it cannot place.
    #[test]
    fn rapid_arming_leaves_the_model_holding_off_then_on_in_that_order() {
        let mut mode = armed();
        let queue = FakeSink::default();
        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None)
            .expect("armed")
            .expect("written");
        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");
        deliver_exit_hint(&queue, &disarmed)
            .expect("the model was told")
            .expect("written");

        mode.arm("target", "desktop", true).expect("re-arm");
        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None)
            .expect("armed again")
            .expect("written");

        assert_eq!(
            entries(&queue),
            [MODE_ENTRY_HINT, MODE_EXIT_HINT, MODE_ENTRY_HINT],
            "the terminal must read as the history of the mode, with no repeated state"
        );
    }

    /// A late arrival cannot announce a conversation that is not happening.
    #[test]
    fn nothing_is_announced_for_a_mode_that_is_not_armed() {
        let mut mode = HandsFree::new(1_500);
        let queue = FakeSink::default();

        assert!(deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None).is_none());
        assert!(queue.written.borrow().is_empty());
    }

    /// A refused notice was never read, so it buys no end notice either.
    #[test]
    fn a_refused_start_notice_is_never_undone() {
        let mut mode = armed();
        let queue = FakeSink::default();
        *queue.fail.borrow_mut() = Some("Session is not running an agent".to_string());

        let sent = deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, None).expect("armed");

        assert_eq!(sent, Err("Session is not running an agent".to_string()));
        let disarmed = mode.disarm(DisarmReason::Manual).expect("armed");
        assert!(!disarmed.entry_hint_written);
        assert_eq!(deliver_exit_hint(&queue, &disarmed), None);
    }

    // --- The real sink ----------------------------------------------------

    /// Late asynchronous work is the failure this whole generation scheme
    /// exists for: a whisper pass that finishes after the abort must not send.
    #[test]
    fn a_transcription_that_finishes_after_a_disarm_never_reaches_the_sink() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();
        mode.note_transcribing();
        mode.disarm(DisarmReason::Manual);

        assert_eq!(
            mode.accept_transcript(generation, "too late", None, 0),
            TranscriptOutcome::NotArmed
        );
        assert!(deliver_due(&mut mode, &queue, u64::MAX).is_none());
        assert!(queue.written.borrow().is_empty());
    }

    // --- The language of the turn -----------------------------------------

    /// Boss's requirement, and the one thing a model cannot work out for
    /// itself: the entry says which language it is being spoken to in.
    #[test]
    fn a_spoken_turn_tells_the_model_which_language_to_answer_in() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("the hold-back has expired")
            .expect("written");

        assert_eq!(
            queue.written.borrow()[0].1,
            "esegui i test (reply in Italian)",
            "the requirement travels in the entry, which is the only thing that reaches the model"
        );
    }

    /// Boss's requirement: the language is stated once per conversation, not on
    /// every message. A second turn in the same language already has it.
    #[test]
    fn a_second_turn_in_the_same_language_carries_no_requirement() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("first turn is due")
            .expect("written");
        mode.accept_transcript(generation, "ora rilanciali", Some("it"), 2_000);
        deliver_due(&mut mode, &queue, 4_000)
            .expect("second turn is due")
            .expect("written");

        assert_eq!(
            entries(&queue),
            vec![
                "esegui i test (reply in Italian)".to_string(),
                "ora rilanciali".to_string(),
            ],
            "the model was told once; repeating it on every turn is the noise this removes"
        );
    }

    /// A conversation that changes language must not keep requiring the
    /// previous one, so a switch is announced again — and only the switch.
    #[test]
    fn a_language_switch_is_announced_again() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        for (at, text, language) in [
            (0, "esegui i test", "it"),
            (2_000, "now run them again", "en"),
            (4_000, "and once more", "en"),
            (6_000, "di nuovo", "it"),
        ] {
            mode.accept_transcript(generation, text, Some(language), at);
            deliver_due(&mut mode, &queue, at + 1_500)
                .expect("due")
                .expect("written");
        }

        assert_eq!(
            entries(&queue),
            vec![
                "esegui i test (reply in Italian)".to_string(),
                "now run them again (reply in English)".to_string(),
                "and once more".to_string(),
                "di nuovo (reply in Italian)".to_string(),
            ]
        );
    }

    /// A turn the recogniser could not name says nothing, and must not make
    /// the model forget the language it was already told.
    #[test]
    fn an_unnamed_turn_does_not_reset_the_announced_language() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        for (at, text, language) in [
            (0, "esegui i test", Some("it")),
            (2_000, "mh", None),
            (4_000, "ora rilanciali", Some("it")),
        ] {
            mode.accept_transcript(generation, text, language, at);
            deliver_due(&mut mode, &queue, at + 1_500)
                .expect("due")
                .expect("written");
        }

        assert_eq!(
            entries(&queue),
            vec![
                "esegui i test (reply in Italian)".to_string(),
                "mh".to_string(),
                "ora rilanciali".to_string(),
            ]
        );
    }

    /// A refused turn never reached the model, so it announced nothing: the
    /// next turn that does land still has to name the language.
    #[test]
    fn a_refused_turn_does_not_count_as_an_announcement() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        assert!(
            deliver_due(&mut mode, &RefusingSink, 1_500)
                .expect("due")
                .is_err()
        );

        let queue = FakeSink::default();
        mode.accept_transcript(generation, "esegui i test", Some("it"), 2_000);
        deliver_due(&mut mode, &queue, 3_500)
            .expect("due")
            .expect("written");

        assert_eq!(
            entries(&queue),
            vec!["esegui i test (reply in Italian)".to_string()]
        );
    }

    /// The start notice is where the language is stated when it is already
    /// known — a fixed dictation language — so the first turn need not repeat it.
    #[test]
    fn the_start_notice_names_a_known_language_and_the_first_turn_does_not_repeat_it() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, Some("it"))
            .expect("armed")
            .expect("written");
        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("due")
            .expect("written");
        mode.accept_transcript(generation, "now in English", Some("en"), 2_000);
        deliver_due(&mut mode, &queue, 3_500)
            .expect("due")
            .expect("written");

        assert_eq!(
            entries(&queue),
            vec![
                format!("{MODE_ENTRY_HINT} Reply in Italian."),
                "esegui i test".to_string(),
                "now in English (reply in English)".to_string(),
            ],
            "the language line is added by code, whatever the configured notice says"
        );
    }

    /// With `auto` nothing is known at arm, so the notice names nothing and the
    /// first spoken turn carries the language instead.
    #[test]
    fn a_notice_without_a_known_language_leaves_the_first_turn_to_name_it() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        deliver_entry_hint(&mut mode, &queue, MODE_ENTRY_HINT, Some("auto"))
            .expect("armed")
            .expect("written");
        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("due")
            .expect("written");

        assert_eq!(
            entries(&queue),
            vec![
                MODE_ENTRY_HINT.to_string(),
                "esegui i test (reply in Italian)".to_string()
            ]
        );
    }

    /// A new conversation is a new model context as far as this mode knows:
    /// what the last one was told does not carry over.
    #[test]
    fn a_new_arm_announces_the_language_again() {
        let mut mode = armed();
        let queue = FakeSink::default();
        let generation = mode.generation();
        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("due")
            .expect("written");
        mode.disarm(DisarmReason::Manual);

        let generation = mode.arm("target", "desktop", true).expect("re-arm");
        mode.accept_transcript(generation, "di nuovo", Some("it"), 2_000);
        deliver_due(&mut mode, &queue, 3_500)
            .expect("due")
            .expect("written");

        assert_eq!(
            entries(&queue),
            vec![
                "esegui i test (reply in Italian)".to_string(),
                "di nuovo (reply in Italian)".to_string(),
            ]
        );
    }

    fn entries(queue: &FakeSink) -> Vec<String> {
        queue
            .written
            .borrow()
            .iter()
            .map(|(_, text)| text.clone())
            .collect()
    }

    /// The failure this whole story exists to prevent, at its smallest: an
    /// unnamed language must produce no requirement rather than a default one.
    #[test]
    fn a_turn_the_recogniser_could_not_name_reaches_the_model_as_it_was_spoken() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        mode.accept_transcript(generation, "run the tests", None, 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("due")
            .expect("written");

        assert_eq!(
            queue.written.borrow()[0].1,
            "run the tests",
            "no language means no requirement; inventing English here is the bug"
        );
    }

    /// Whisper recognises about a hundred languages and the panel offers
    /// eleven. One it cannot name is one it cannot require.
    #[test]
    fn a_language_the_panel_does_not_offer_names_nothing() {
        let mut mode = armed();
        let generation = mode.generation();
        let queue = FakeSink::default();

        mode.accept_transcript(generation, "rhedwch y profion", Some("cy"), 0);
        deliver_due(&mut mode, &queue, 1_500)
            .expect("due")
            .expect("written");

        assert_eq!(queue.written.borrow()[0].1, "rhedwch y profion");
    }

    /// The voice that answers is chosen from this, so it has to be the language
    /// of the turn rather than of the setting.
    #[test]
    fn the_conversation_remembers_the_language_of_the_last_turn() {
        let mut mode = armed();
        let generation = mode.generation();
        assert_eq!(
            mode.turn_language(),
            None,
            "before anybody speaks there is no language, and no default either"
        );

        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);
        assert_eq!(mode.turn_language(), Some("it"));

        mode.accept_transcript(generation, "run the tests", Some("en"), 2_000);
        assert_eq!(mode.turn_language(), Some("en"));
    }

    /// A remark across the room is not this conversation, and must not decide
    /// which language the next real turn is answered in.
    #[test]
    fn speech_that_was_not_addressed_here_does_not_choose_the_language() {
        let mut mode = armed_with_phrase("ciao tuic");
        let generation = mode.generation();
        mode.accept_transcript(generation, "ciao tuic, esegui i test", Some("it"), 0);
        // Sent, as it is in the runtime long before the window shuts; a turn
        // still held back would make the next transcript its continuation.
        assert!(mode.poll_send(5_000).is_some());

        // Past the window the first turn opened: inside it every turn is
        // addressed here, which is a different rule and a different test.
        assert_eq!(
            mode.accept_transcript(generation, "did you watch the game", Some("en"), 20_000),
            TranscriptOutcome::Rejected
        );
        assert_eq!(
            mode.turn_language(),
            Some("it"),
            "the rejected turn never reached the model, so it cannot pick the reply language"
        );
    }

    /// Arming is a new conversation with a new person in front of it.
    #[test]
    fn a_new_conversation_starts_without_the_previous_language() {
        let mut mode = armed();
        let generation = mode.generation();
        mode.accept_transcript(generation, "esegui i test", Some("it"), 0);

        mode.disarm(DisarmReason::Manual).expect("armed");
        assert_eq!(mode.turn_language(), None, "a disarm ends the conversation");

        mode.arm("target", "desktop", true).expect("arm");
        assert_eq!(
            mode.turn_language(),
            None,
            "inheriting it would answer the next user in the previous one's language"
        );
    }

    // --- The runtime loop -------------------------------------------------

    /// A microphone and a recogniser, under the test's control.
    ///
    /// `drain` hands back one queued chunk per call, so a test states the audio
    /// timeline chunk by chunk exactly as the desktop adapter delivers it.
    struct FakeEndpoint {
        chunks: RefCell<std::collections::VecDeque<Vec<f32>>>,
        connected: std::cell::Cell<bool>,
        drain_error: RefCell<Option<String>>,
        transcript: String,
        /// What the recogniser says this speech was, per call.
        language: RefCell<Option<String>>,
        calls: std::cell::Cell<usize>,
        /// The audio of every utterance handed to `transcribe`. `tick` consumes
        /// the utterances itself, so this is the only place a test can see what
        /// the segmenter decided to send — which is what a pre-roll assertion
        /// has to look at.
        heard: RefCell<Vec<Vec<f32>>>,
        /// Run inside `transcribe`, to model work that lands while a whisper
        /// pass is still running.
        during_transcribe: RefCell<Option<Box<dyn Fn() + Send>>>,
    }

    impl FakeEndpoint {
        fn new(transcript: &str) -> Self {
            Self {
                chunks: RefCell::new(std::collections::VecDeque::new()),
                connected: std::cell::Cell::new(true),
                drain_error: RefCell::new(None),
                transcript: transcript.to_string(),
                language: RefCell::new(None),
                calls: std::cell::Cell::new(0),
                heard: RefCell::new(Vec::new()),
                during_transcribe: RefCell::new(None),
            }
        }

        fn feed(&self, samples: Vec<f32>) {
            self.chunks.borrow_mut().push_back(samples);
        }
    }

    impl VoiceEndpoint for FakeEndpoint {
        fn drain(&mut self) -> Result<Vec<f32>, String> {
            if let Some(error) = self.drain_error.borrow().as_ref() {
                return Err(error.clone());
            }
            Ok(self.chunks.borrow_mut().pop_front().unwrap_or_default())
        }

        fn connected(&self) -> bool {
            self.connected.get()
        }

        fn transcribe(&self, audio: &[f32]) -> Result<Transcript, String> {
            self.calls.set(self.calls.get() + 1);
            self.heard.borrow_mut().push(audio.to_vec());
            if let Some(during) = self.during_transcribe.borrow().as_ref() {
                during();
            }
            Ok(Transcript {
                text: self.transcript.clone(),
                language: self.language.borrow().clone(),
            })
        }
    }

    /// A target that answers whatever the test last said.
    struct FakeTarget(std::cell::Cell<bool>);

    impl TargetProbe for FakeTarget {
        fn accepts(&self, _session_id: &str) -> bool {
            self.0.get()
        }
    }

    fn runtime_capture() -> Capture {
        Capture::new(test_config(), 5_000, 0, echo_guard(), None)
    }

    /// A reply queue that only records what it was told to do.
    #[derive(Default)]
    struct CountingSpeaker {
        pauses: std::sync::atomic::AtomicUsize,
        resumes: std::sync::atomic::AtomicUsize,
        hushes: std::sync::atomic::AtomicUsize,
    }

    impl CountingSpeaker {
        fn pauses(&self) -> usize {
            self.pauses.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn resumes(&self) -> usize {
            self.resumes.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn hushes(&self) -> usize {
            self.hushes.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    impl Interruptible for CountingSpeaker {
        fn pause(&self) {
            self.pauses
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        fn resume(&self) {
            self.resumes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        fn hush(&self) {
            self.hushes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn capture_with_a_voice() -> (Capture, std::sync::Arc<CountingSpeaker>) {
        let speaker = std::sync::Arc::new(CountingSpeaker::default());
        let capture = Capture::new(
            test_config(),
            5_000,
            0,
            echo_guard(),
            Some(speaker.clone() as std::sync::Arc<dyn Interruptible>),
        );
        (capture, speaker)
    }

    /// A guard with no canceller: capture comes back exactly as it arrived,
    /// which is what every test but the echo ones wants to reason about.
    fn echo_guard() -> std::sync::Arc<parking_lot::Mutex<super::super::echo::EchoGuard>> {
        std::sync::Arc::new(parking_lot::Mutex::new(super::super::echo::EchoGuard::new(
            Box::new(super::super::echo::PassThrough),
        )))
    }

    fn armed_shared() -> parking_lot::Mutex<HandsFree> {
        let mut mode = HandsFree::new(1_000);
        mode.arm("target", "desktop", true).expect("arm");
        parking_lot::Mutex::new(mode)
    }

    /// Subtracts exactly what it is told was played. A real canceller is
    /// adaptive and never this clean; this one makes the question "did the
    /// far end reach the capture path" answerable without audio hardware.
    struct Subtract;

    impl super::super::echo::Canceller for Subtract {
        fn cancel(&mut self, far_end: &[f32], near_end: &mut [f32]) {
            for (near, far) in near_end.iter_mut().zip(far_end) {
                *near -= far;
            }
        }
    }

    fn cancelling_capture() -> (
        Capture,
        std::sync::Arc<parking_lot::Mutex<super::super::echo::EchoGuard>>,
    ) {
        let echo = std::sync::Arc::new(parking_lot::Mutex::new(
            super::super::echo::EchoGuard::new(Box::new(Subtract)),
        ));
        (
            Capture::new(test_config(), 5_000, 0, echo.clone(), None),
            echo,
        )
    }

    /// Barge-in. Answering is only conversational if the user can cut it off:
    /// without this they have to sit through a sentence they have already
    /// decided against, and their own words land in the turn after it.
    #[test]
    fn the_user_starting_to_talk_silences_the_reply_in_progress() {
        let mode = armed_shared();
        let (mut capture, speaker) = capture_with_a_voice();
        let mut endpoint = FakeEndpoint::new("actually, no");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.feed(silence(200));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert_eq!(
            speaker.pauses(),
            0,
            "a quiet room must not interrupt the reply"
        );

        endpoint.feed(speech(500));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 400);
        assert_eq!(
            speaker.pauses(),
            1,
            "the reply was talked over and survived"
        );
        assert_eq!(
            speaker.hushes(),
            0,
            "the reply was dropped before anybody knew the speech was for us"
        );
    }

    /// `hush` opens a new turn on every call, so a level trigger would open one
    /// per 50 ms tick — and every reply the model wrote for the turn in
    /// progress would then be refused as stale while the user was still
    /// speaking one sentence.
    #[test]
    fn a_reply_is_paused_once_per_utterance_not_once_per_tick() {
        let mode = armed_shared();
        let (mut capture, speaker) = capture_with_a_voice();
        let mut endpoint = FakeEndpoint::new("one long sentence");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        for tick_ms in [100, 400, 700, 1_000] {
            endpoint.feed(speech(300));
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, tick_ms);
        }

        assert!(
            capture.segmenter.is_capturing(),
            "the test needs one utterance that is still open"
        );
        assert_eq!(speaker.pauses(), 1);
    }

    /// A short sentence can open and close inside one device chunk. The gate is
    /// shut again by the end of the tick, so the hush has to come from the
    /// closed utterance — once, not zero times and not once per rule.
    #[test]
    fn an_utterance_opened_and_closed_in_one_chunk_pauses_and_then_stops_the_reply_once() {
        let mode = armed_shared();
        let (mut capture, speaker) = capture_with_a_voice();
        let mut endpoint = FakeEndpoint::new("no");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let mut chunk = speech(300);
        chunk.extend(silence(500));
        endpoint.feed(chunk);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);

        assert!(
            !capture.segmenter.is_capturing() && endpoint.calls.get() == 1,
            "the test needs one utterance that opened and closed inside the chunk"
        );
        // No activation phrase: the turn is accepted, so the pause becomes a stop.
        assert_eq!((speaker.pauses(), speaker.hushes()), (1, 1));
        assert_eq!(speaker.resumes(), 0);
    }

    /// The edge is only trustworthy because the canceller runs before it. Our
    /// own reply coming back through the microphone would otherwise interrupt
    /// itself on the first word — the one failure that makes speaking useless
    /// rather than merely imperfect.
    #[test]
    fn our_own_reply_heard_by_the_microphone_does_not_interrupt_itself() {
        let mode = armed_shared();
        let speaker = std::sync::Arc::new(CountingSpeaker::default());
        let echo = std::sync::Arc::new(parking_lot::Mutex::new(
            super::super::echo::EchoGuard::new(Box::new(Subtract)),
        ));
        let mut capture = Capture::new(
            test_config(),
            5_000,
            0,
            echo.clone(),
            Some(speaker.clone() as std::sync::Arc<dyn Interruptible>),
        );
        let mut endpoint = FakeEndpoint::new("the words we just said");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let reply = speech(500);
        echo.lock().note_rendered(&crate::speech::SpeechAudio {
            samples: reply.clone(),
            sample_rate: SAMPLE_RATE,
        });
        endpoint.feed(reply);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);

        assert_eq!(
            speaker.pauses(),
            0,
            "the reply interrupted itself as soon as the microphone heard it"
        );

        // The other half: the same audio, with nothing subtracting it, is a
        // user and must interrupt. Otherwise this proves only that silence is
        // silent.
        let (mut capture, speaker) = capture_with_a_voice();
        endpoint.feed(speech(500));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 200);
        assert_eq!(speaker.pauses(), 1);
    }

    /// The failure the echo path exists to stop: the microphone hears the reply
    /// we are speaking, the energy gate calls it an utterance, and the model is
    /// handed its own words back.
    #[test]
    fn a_reply_coming_out_of_the_speaker_does_not_become_a_turn() {
        let mode = armed_shared();
        let (mut capture, echo) = cancelling_capture();
        let mut endpoint = FakeEndpoint::new("the words we just said");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let reply = speech(500);
        echo.lock().note_rendered(&crate::speech::SpeechAudio {
            samples: reply.clone(),
            sample_rate: SAMPLE_RATE,
        });

        // The microphone hears the reply and nothing else.
        let mut heard = reply;
        heard.extend(silence(600));
        endpoint.feed(heard);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 3_000);

        assert_eq!(
            endpoint.calls.get(),
            0,
            "our own voice was transcribed as if the user had spoken"
        );
        assert!(
            queue.written.borrow().is_empty(),
            "the model was sent its own reply"
        );
        assert_eq!(*mode.lock().phase(), Phase::Waiting);
    }

    /// The other half of the pair. Without it the test above proves only that
    /// silence is silent: the same audio, with nothing subtracting it, must
    /// reach the queue.
    #[test]
    fn the_same_audio_with_nothing_subtracted_does_become_a_turn() {
        let mode = armed_shared();
        let (mut capture, _echo) = cancelling_capture();
        let mut endpoint = FakeEndpoint::new("the words we just said");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        // Same capture, but nobody told the guard anything was played.
        let mut heard = speech(500);
        heard.extend(silence(600));
        endpoint.feed(heard);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 3_000);

        assert_eq!(endpoint.calls.get(), 1, "the utterance was never segmented");
        assert_eq!(
            queue.written.borrow().len(),
            1,
            "a real user's words must still get through"
        );
    }

    /// Interruption clears the reference as well as the sound. Audio nobody is
    /// going to hear must not be subtracted from the user talking over it.
    #[test]
    fn a_hushed_reply_stops_being_subtracted_from_what_the_user_says() {
        let mode = armed_shared();
        let (mut capture, echo) = cancelling_capture();
        let mut endpoint = FakeEndpoint::new("stop");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let reply = speech(500);
        echo.lock().note_rendered(&crate::speech::SpeechAudio {
            samples: reply.clone(),
            sample_rate: SAMPLE_RATE,
        });
        // The user interrupts: playback stops, so the rest of the reply is
        // never heard and must stop being treated as reference.
        echo.lock().note_stopped();

        let mut heard = reply;
        heard.extend(silence(600));
        endpoint.feed(heard);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 3_000);

        assert_eq!(
            queue.written.borrow().len(),
            1,
            "the user was cancelled against a reply that was never played"
        );
    }

    /// The whole point of the pass: audio in one end, one write to the sink
    /// out the other, and no other exit in between.
    #[test]
    fn a_spoken_phrase_travels_from_capture_to_the_sink() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("run the tests");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);

        let first = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert!(
            matches!(
                first,
                Tick::Running {
                    delivered: None,
                    ..
                }
            ),
            "the hold-back must still be running, got {first:?}"
        );
        assert_eq!(
            mode.lock().pending_text(),
            Some("run the tests"),
            "the transcript must be visible while it is held back"
        );
        assert!(queue.written.borrow().is_empty());

        let sent = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 1_200);

        assert!(
            matches!(
                sent,
                Tick::Running {
                    delivered: Some(Delivery::Turn),
                    ..
                }
            ),
            "the expired hold-back must be typed, got {sent:?}"
        );
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written("run the tests")]
        );
    }

    #[test]
    fn speech_started_before_the_five_second_deadline_is_not_submitted_mid_phrase() {
        let mut armed = HandsFree::new(1_500);
        armed.set_activation("computer", ACTIVATION_WINDOW_MS);
        armed.arm("target", "desktop", true).expect("arm");
        armed.accept_transcript(
            armed.generation(),
            "computer appena faccio una pausa",
            None,
            0,
        );
        let mode = parking_lot::Mutex::new(armed);
        let mut capture = Capture::new(SegmenterConfig::default(), 5_000, 0, echo_guard(), None);
        let mut endpoint = FakeEndpoint::new("la frase continua");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.feed(speech(500));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 4_900);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 5_000);
        assert!(
            queue.written.borrow().is_empty(),
            "the first phrase was sent while speech was open"
        );

        endpoint.feed(silence(1_600));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 6_600);
        assert_eq!(
            mode.lock().pending_text(),
            Some("appena faccio una pausa\nla frase continua")
        );
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 11_600);
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written("appena faccio una pausa\nla frase continua")]
        );
    }

    /// catches: a long second phrase dropped as unaddressed because whisper
    /// returned after the activation window shut, so only the first phrase of
    /// the sentence reached the agent while the whole sentence was spoken.
    #[test]
    fn a_long_second_phrase_finished_after_the_window_still_reaches_the_agent() {
        let mut armed = HandsFree::new(1_500);
        armed.set_activation("computer", ACTIVATION_WINDOW_MS);
        armed.arm("target", "desktop", true).expect("arm");
        armed.accept_transcript(armed.generation(), "computer open the file", None, 0);
        let mode = parking_lot::Mutex::new(armed);
        let mut capture = Capture::new(SegmenterConfig::default(), 5_000, 0, echo_guard(), None);
        let mut endpoint = FakeEndpoint::new("and then run the tests");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        // Speech opens before the five-second deadline and runs well past the
        // fifteen-second window, which was opened when the first phrase was
        // accepted at t=0.
        endpoint.feed(speech(500));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 4_900);
        for second in 5..=16u64 {
            endpoint.feed(speech(1_000));
            tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                second * 1_000,
            );
        }
        endpoint.feed(silence(1_600));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 18_000);
        assert_eq!(
            mode.lock().pending_text(),
            Some("open the file\nand then run the tests"),
            "the second phrase was dropped; the first would be sent alone"
        );
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 23_000);
        assert_eq!(
            queue.written.borrow().as_slice(),
            [written("open the file\nand then run the tests")]
        );
    }

    /// The continuation rule must not become an open door: with nothing held
    /// back, an unaddressed phrase after the window is still dropped.
    #[test]
    fn an_unaddressed_phrase_with_nothing_held_back_is_still_dropped() {
        let mut mode = armed_with_phrase("computer");
        let generation = mode.generation();
        mode.accept_transcript(generation, "computer open the file", None, 0);
        assert!(mode.poll_send(5_000).is_some());

        assert_eq!(
            mode.accept_transcript(generation, "unrelated chatter", None, 30_000),
            TranscriptOutcome::Rejected
        );
    }

    /// An armed microphone in an empty room must cost nothing: no inference at
    /// all, and a retained-audio figure that does not move with time.
    #[test]
    fn an_armed_and_silent_microphone_never_infers_and_stays_bounded() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("should never be asked");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        // 60s of room silence, delivered the way cpal delivers it.
        for step in 0..1_200u64 {
            endpoint.feed(silence(50));
            tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                step * 50,
            );
        }

        assert_eq!(
            endpoint.calls.get(),
            0,
            "silence must never reach the recogniser"
        );
        assert!(queue.written.borrow().is_empty());
        assert!(
            capture.retained_samples() <= ms_to_samples(test_config().pre_roll_ms) + FRAME_SAMPLES,
            "silence retained {} samples — the pre-roll is the only buffer",
            capture.retained_samples()
        );
    }

    /// Whisper hands back an empty string for a cough. Nothing may be sent, and
    /// the mode must go back to listening rather than sit in `transcribing`.
    #[test]
    fn a_transcript_the_recogniser_rejects_sends_nothing() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("   ");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);
        // Well past any hold-back, with the stream still alive.
        endpoint.feed(silence(50));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 3_000);

        assert_eq!(endpoint.calls.get(), 1, "the utterance was transcribed");
        assert!(
            queue.written.borrow().is_empty(),
            "an empty transcript is not a message"
        );
        assert_eq!(*mode.lock().phase(), Phase::Waiting);
    }

    /// The reason the mode lock is released across a whisper pass: an abort
    /// during transcription has to be able to land, and the result that arrives
    /// afterwards has to be refused.
    #[test]
    fn an_abort_during_transcription_lands_and_its_result_is_refused() {
        let mode = std::sync::Arc::new(armed_shared());
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("too late");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let aborting = mode.clone();
        *endpoint.during_transcribe.borrow_mut() = Some(Box::new(move || {
            let mut mode = aborting
                .try_lock()
                .expect("the runtime must not hold the mode lock across a transcription");
            mode.disarm(DisarmReason::Manual);
        }));

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);

        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 10_000);

        assert_eq!(endpoint.calls.get(), 1);
        assert!(
            queue.written.borrow().is_empty(),
            "a result that outlived its mode must not reach a model"
        );
        assert!(mode.lock().binding().is_none());
    }

    /// The bound tab is closed while the mode is armed.
    #[test]
    fn a_closed_target_disarms_the_running_mode() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        target.0.set(false);
        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        match outcome {
            Tick::Disarmed(disarmed) => assert_eq!(disarmed.reason, DisarmReason::TargetClosed),
            other => panic!("a closed target must disarm, got {other:?}"),
        }
        assert!(mode.lock().binding().is_none());
    }

    /// The endpoint that armed the mode goes away — the desktop audio endpoint
    /// released on shutdown, or (in 818) a remote client's socket closing.
    #[test]
    fn a_disconnected_owner_disarms_the_running_mode() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.connected.set(false);
        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        match outcome {
            Tick::Disarmed(disarmed) => {
                assert_eq!(disarmed.reason, DisarmReason::OwnerDisconnected)
            }
            other => panic!("a disconnected owner must disarm, got {other:?}"),
        }
    }

    /// A hard capture error is a device failure, and the message reaches the
    /// status rather than a log nobody reads.
    #[test]
    fn a_capture_error_disarms_with_its_message() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        *endpoint.drain_error.borrow_mut() = Some("input device disappeared".to_string());
        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        match outcome {
            Tick::Disarmed(disarmed) => assert_eq!(
                disarmed.reason,
                DisarmReason::DeviceFailed("input device disappeared".to_string())
            ),
            other => panic!("a capture error must disarm, got {other:?}"),
        }
        assert_eq!(mode.lock().last_error(), Some("input device disappeared"));
    }

    /// A microphone that was unplugged does not report an error — it simply
    /// stops delivering samples. Silence in the *audio* is normal; silence in
    /// the *stream* is a dead device, and the mode must not stay armed on it.
    #[test]
    fn a_device_that_stops_delivering_samples_disarms_after_its_timeout() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        // Still inside the timeout: no samples yet, but not a failure either.
        assert!(matches!(
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 4_000),
            Tick::Running { .. }
        ));

        let outcome = tick(&mut capture, &mode, &mut endpoint, &target, &queue, 5_001);

        match outcome {
            Tick::Disarmed(disarmed) => assert!(
                matches!(disarmed.reason, DisarmReason::DeviceFailed(_)),
                "got {:?}",
                disarmed.reason
            ),
            other => panic!("a silent stream must disarm, got {other:?}"),
        }
    }

    /// Audio arriving resets the starvation clock: a long dictation session
    /// must not disarm itself just because it passed the timeout.
    #[test]
    fn audio_keeps_the_device_alive_past_the_timeout() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        for step in 0..300u64 {
            endpoint.feed(silence(50));
            let outcome = tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                step * 50,
            );
            assert!(
                matches!(outcome, Tick::Running { .. }),
                "a live stream must stay armed at {}ms, got {outcome:?}",
                step * 50
            );
        }
    }

    /// A sink that refuses everything, and is `Send` so the tick under test
    /// can run on a thread the test can put a deadline on.
    struct RefusingSink;

    impl VoiceSink for RefusingSink {
        fn write(&self, _session_id: &str, _text: &str) -> Result<VoiceWrite, String> {
            Err("Session not found".to_string())
        }
    }

    /// A refused delivery is an ordinary outcome — the target can disappear
    /// between the transcript and the send — so the runtime must report it and
    /// keep going.
    ///
    /// It runs on its own thread with a deadline because the failure this
    /// guards against is a *deadlock*, not a wrong answer: holding the mode
    /// lock across the error arm parks the runtime and every status poll behind
    /// it forever. The 5s is a harness bound on a clock-free call, not a
    /// behaviour assertion — nothing inside `tick` waits for anything.
    #[test]
    fn a_refused_delivery_is_reported_without_parking_the_runtime() {
        let mode = std::sync::Arc::new(armed_shared());
        let generation = mode.lock().generation();
        mode.lock()
            .accept_transcript(generation, "run the tests", None, 0);

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let mode = mode.clone();
            scope.spawn(move || {
                let mut capture = runtime_capture();
                let mut endpoint = FakeEndpoint::new("unused");
                endpoint.feed(silence(50));
                let outcome = tick(
                    &mut capture,
                    &mode,
                    &mut endpoint,
                    &FakeTarget(std::cell::Cell::new(true)),
                    &RefusingSink,
                    2_000,
                );
                let _ = tx.send(matches!(
                    outcome,
                    Tick::Running {
                        send_error: Some(_),
                        ..
                    }
                ));
            });

            let reported = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("tick parked on the mode lock instead of reporting the refusal");
            assert!(reported, "the refusal must reach the caller");
        });

        let mode = mode
            .try_lock()
            .expect("the mode lock must have been released");
        assert_eq!(mode.last_error(), Some("Session not found"));
        assert!(
            mode.binding().is_some(),
            "a refused send is not a reason to disarm the mode"
        );
    }

    /// The hold-back is only a safety net if the abort inside it works.
    #[test]
    fn an_abort_inside_the_hold_back_sends_nothing() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("delete everything");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 0);

        let disarmed = mode.lock().disarm(DisarmReason::Manual).expect("armed");
        assert!(
            disarmed.discarded_pending,
            "the held transcript was dropped"
        );

        assert!(matches!(
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 10_000),
            Tick::NotArmed
        ));
        assert!(queue.written.borrow().is_empty());
    }

    /// The gate against the real pass, not against `accept_transcript` alone:
    /// unrelated speech is captured, segmented and transcribed — locally — and
    /// then stops. Nothing reaches the only exit this module has.
    ///
    /// The transcribe count is the half that makes the rest meaningful. Without
    /// it a broken capture would pass this test by never recognising anything.
    #[test]
    fn speech_without_the_activation_phrase_never_reaches_the_queue() {
        let mode = {
            let mut mode = HandsFree::new(1_000);
            mode.set_activation("ciao tuic", 10_000);
            mode.arm("target", "desktop", true).expect("arm");
            parking_lot::Mutex::new(mode)
        };
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("cancella tutto il repository");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let mut input = speech(500);
        input.extend(silence(600));
        endpoint.feed(input);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 5_000);

        assert_eq!(
            endpoint.calls.get(),
            1,
            "the utterance must still be recognised locally"
        );
        assert!(
            mode.lock().pending_text().is_none(),
            "a rejected transcript may not occupy the send slot"
        );
        assert!(
            queue.written.borrow().is_empty(),
            "a rejected transcript may not reach the sink"
        );
    }

    // --- Barge-in, measured against the real canceller (816-cbbf) ----------
    //
    // Everything above this line proves barge-in with `Subtract`, a perfect
    // non-adaptive canceller. That answers "is the wiring right" and cannot
    // answer "how long does it take" or "how often does it fire when nobody
    // spoke", because a perfect subtraction leaves no residual to misjudge.
    // The block below drives the shipping AEC3 adapter through the same `tick`
    // and turns those two questions into numbers.

    /// What one run of [`play_over_the_user`] observed, in ticks and samples.
    ///
    /// Raw rather than interpreted, because the two tests below read it in
    /// opposite directions: one asks how *well* the canceller did, the other
    /// asks whether the same room breaks without it.
    struct Run {
        /// The tick at the end of which each `pause()` was issued, in order.
        pauses: Vec<usize>,
        /// Every utterance that reached the recogniser: the tick at the end of
        /// which it closed, and its audio.
        utterances: Vec<(usize, Vec<f32>)>,
        /// The user's first sample, counted from the start of the run.
        onset: usize,
    }

    /// The numbers story 816-cbbf asks for out loud.
    ///
    /// The test that bounds them prints them too: a regression that stays
    /// inside the bounds is still worth seeing move.
    #[derive(Debug)]
    struct BargeIn {
        /// Milliseconds from the user's first sample to the tick that paused
        /// the speaker.
        stop_latency_ms: u32,
        /// Times the speaker was paused while only the reply was audible.
        false_triggers: usize,
        /// Milliseconds the utterance kept from *before* the user's first
        /// sample — what the pre-roll ring saved while the gate was still shut.
        pre_roll_ms: u32,
        /// The whole utterance handed to the recogniser, trailing silence
        /// included.
        utterance_ms: u32,
    }

    /// The microphone hears the speaker this much later than we rendered it:
    /// output buffering plus about a metre of air.
    const ROOM_DELAY_MS: u32 = 40;
    /// And this much quieter.
    const ROOM_GAIN: f32 = 0.35;

    /// How the reply comes back to the microphone.
    struct Room {
        delay_ms: u32,
        gain: f32,
        /// Heard on top of the echo, starting with the reply's first audible
        /// sample and uncorrelated with it: what no canceller can subtract
        /// because it is not in the reference. Empty in the linear room.
        residual: Vec<f32>,
    }

    /// The linear room an echo canceller is specified against.
    fn linear_room() -> Room {
        Room {
            delay_ms: ROOM_DELAY_MS,
            gain: ROOM_GAIN,
            residual: Vec::new(),
        }
    }

    const REPLY_MS: u32 = 1_800;
    const USER_ONSET_MS: u32 = 1_200;
    const USER_MS: u32 = 600;
    // User speech ends at 1800 ms; retain enough quiet for the shipping
    // 1500 ms trailing-silence rule to close the utterance.
    const TIMELINE_MS: u32 = 3_500;
    /// One device chunk per tick, at the cadence the runtime actually polls.
    const TICK_SAMPLES: usize = (SAMPLE_RATE as usize * POLL_INTERVAL_MS as usize) / 1_000;

    /// A sine of a chosen pitch at half scale.
    fn tone(ms: u32, hz: f32) -> Vec<f32> {
        (0..ms_to_samples(ms))
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / SR).sin() * 0.5)
            .collect()
    }

    /// Samples the segmenter has consumed after tick `tick`.
    ///
    /// Not `(tick + 1) * TICK_SAMPLES`: a 50 ms chunk is not a whole number of
    /// 20 ms segmenter frames, so every other tick leaves half a frame behind.
    /// Utterance boundaries land on this grid, and computing one from the tick
    /// alone would put them up to 10 ms out.
    fn consumed(tick: usize) -> usize {
        ((tick + 1) * TICK_SAMPLES / FRAME_SAMPLES) * FRAME_SAMPLES
    }

    /// One rendered reply, one room, one user talking over it.
    ///
    /// The room is a model, and the model *is* the fixture: the microphone
    /// hears the reply `room.delay_ms` late and `room.gain` quieter, plus any
    /// residual the room adds, with the user added on top. That is the signal an echo canceller is specified
    /// against — a linear path with a delay — which is what makes these numbers
    /// comparable from run to run. What it deliberately does not model is a
    /// real room's reverberation, the microphone's own noise floor, and a
    /// speaker driven into distortion. Those are why the story also asks for a
    /// pass with real hardware, and why this function is not a substitute for
    /// one.
    ///
    /// One tick is one [`POLL_INTERVAL_MS`] chunk, which is what the runtime
    /// loop does, so the stop latency this reports is the one a person in front
    /// of the machine waits through rather than the detection stage on its own.
    /// The device's own capture latency is on top of it and is not ours to
    /// measure here.
    fn play_over_the_user(canceller: Box<dyn super::super::echo::Canceller>, room: Room) -> Run {
        let mode = armed_shared();
        let speaker = std::sync::Arc::new(CountingSpeaker::default());
        let echo = std::sync::Arc::new(parking_lot::Mutex::new(
            super::super::echo::EchoGuard::new(canceller),
        ));
        // The shipping boundaries, not `test_config`: a latency measured
        // against a pre-roll nobody runs is a measurement of the test.
        let mut capture = Capture::new(
            SegmenterConfig::default(),
            5_000,
            0,
            echo.clone(),
            Some(speaker.clone() as std::sync::Arc<dyn Interruptible>),
        );
        let mut endpoint = FakeEndpoint::new("no, stop");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        // Handed over whole, exactly as `speaker` hands it over. It has to fit
        // the far end's two-second buffer: a longer one loses its head, and the
        // two streams would start the run already out of step.
        let reply = tone(REPLY_MS, 440.0);
        echo.lock().note_rendered(&crate::speech::SpeechAudio {
            samples: reply.clone(),
            sample_rate: SAMPLE_RATE,
        });

        // A different pitch from the reply, so a canceller cannot subtract the
        // user by subtracting the echo and still look like it worked.
        let user = tone(USER_MS, 220.0);
        let delay = ms_to_samples(room.delay_ms);
        let onset = ms_to_samples(USER_ONSET_MS);

        // How much of the reply the speaker has actually emitted. Unbounded
        // until it is paused; after that the room goes quiet one delay later.
        // A pause the user did not ask for stops playback just the same — that
        // is what makes a false trigger cost them the rest of the answer.
        let mut emitted = usize::MAX;
        let mut run = Run {
            pauses: Vec::new(),
            utterances: Vec::new(),
            onset,
        };
        let ticks = ms_to_samples(TIMELINE_MS) / TICK_SAMPLES;

        for tick_index in 0..ticks {
            let chunk: Vec<f32> = (tick_index * TICK_SAMPLES..(tick_index + 1) * TICK_SAMPLES)
                .map(|sample| {
                    let mut heard = 0.0;
                    if let Some(played) = sample.checked_sub(delay)
                        && played < reply.len().min(emitted)
                    {
                        heard += room.gain * reply[played];
                        if let Some(residual) = room.residual.get(played) {
                            heard += residual;
                        }
                    }
                    if let Some(spoken) = sample.checked_sub(onset)
                        && spoken < user.len()
                    {
                        heard += user[spoken];
                    }
                    heard
                })
                .collect();

            endpoint.feed(chunk);
            let pauses = speaker.pauses();
            let transcriptions = endpoint.calls.get();
            tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                tick_index as u64 * POLL_INTERVAL_MS,
            );

            if speaker.pauses() > pauses {
                run.pauses.push(tick_index);
                if emitted == usize::MAX {
                    emitted = (tick_index + 1) * TICK_SAMPLES;
                    echo.lock().note_paused();
                }
            }
            if endpoint.calls.get() > transcriptions {
                let audio = endpoint
                    .heard
                    .borrow()
                    .last()
                    .cloned()
                    .expect("a transcription has audio");
                run.utterances.push((tick_index, audio));
            }
        }
        run
    }

    fn samples_to_ms(samples: usize) -> u32 {
        (samples * 1_000 / SAMPLE_RATE as usize) as u32
    }

    impl Run {
        /// The numbers, read out of one run.
        ///
        /// Panics rather than reporting a zero when the reply was never
        /// interrupted or the words were lost: those are the failures the
        /// story is about, and a number that reads well because nothing
        /// happened is the one outcome worth refusing to print.
        fn measured(&self) -> BargeIn {
            let onset = self.onset;
            let hushed_at = *self
                .pauses
                .iter()
                .find(|tick| !self.before_the_user(**tick))
                .expect("the user talked over the reply and it kept playing");
            let (closed_at, utterance) = self
                .utterances
                .iter()
                .find(|(tick, _)| !self.before_the_user(*tick))
                .expect("what the user said never reached the recogniser");

            // A hush is issued after its tick's chunk is processed, so the end
            // of that chunk is the honest moment the reply stopped. An
            // utterance instead ends on the segmenter's own frame grid, which
            // is what `consumed` tracks, so its first sample is a subtraction
            // rather than an estimate.
            let stopped_at = (hushed_at + 1) * TICK_SAMPLES;
            let began_at = consumed(*closed_at) - utterance.len();
            assert!(
                began_at <= onset,
                "the utterance began {}ms after the user did, so their first words are gone",
                samples_to_ms(began_at - onset)
            );

            BargeIn {
                stop_latency_ms: samples_to_ms(stopped_at - onset),
                false_triggers: self
                    .pauses
                    .iter()
                    .filter(|tick| self.before_the_user(**tick))
                    .count(),
                pre_roll_ms: samples_to_ms(onset - began_at),
                utterance_ms: samples_to_ms(utterance.len()),
            }
        }

        /// Did this tick end before the user's first sample? Only then is
        /// whatever it did attributable to the reply alone.
        fn before_the_user(&self, tick: usize) -> bool {
            (tick + 1) * TICK_SAMPLES <= self.onset
        }
    }

    /// Criterion 3 of 816-cbbf, measured rather than satisfied by construction.
    ///
    /// The bounds are deliberately loose — they are a regression fence around
    /// measured behaviour, not a specification somebody tuned the canceller to
    /// meet. The numbers themselves are in the failure messages and in the
    /// printed line, which nextest shows when this test fails.
    #[test]
    fn talking_over_the_reply_stops_it_without_losing_the_first_words() {
        let measured = play_over_the_user(
            Box::new(super::super::echo::webrtc::WebRtc::new().expect("the bundled APM starts")),
            linear_room(),
        )
        .measured();
        println!("816-cbbf barge-in over a real AEC3 canceller: {measured:?}");

        assert_eq!(
            measured.false_triggers, 0,
            "the reply interrupted itself {} times before the user said anything",
            measured.false_triggers
        );
        // The pause waits for `min_speech_ms` of speech, so that is the floor;
        // one poll interval on top is the tick the threshold lands inside.
        let bound = SegmenterConfig::default().min_speech_ms + POLL_INTERVAL_MS as u32;
        assert!(
            measured.stop_latency_ms <= bound,
            "the user had to talk for {}ms before the reply stopped",
            measured.stop_latency_ms
        );
        assert!(
            measured.pre_roll_ms >= measured.stop_latency_ms,
            "the pre-roll saved {}ms but the gate opened {}ms late, so the words in \
             between reached nobody",
            measured.pre_roll_ms,
            measured.stop_latency_ms
        );
        // Reaching back far enough is not the same as keeping what is there.
        // An utterance that starts before the user and is then truncated would
        // satisfy every bound above and still hand the recogniser half a
        // sentence.
        assert!(
            measured.utterance_ms >= measured.pre_roll_ms + USER_MS,
            "the utterance is {}ms, too short to hold {}ms of pre-roll and {USER_MS}ms of speech",
            measured.utterance_ms,
            measured.pre_roll_ms
        );
    }

    /// The control, and the only reason the zero above means anything.
    ///
    /// A false-trigger count of zero has two explanations: the canceller
    /// removed the echo, or the fixture never put enough echo in the room to
    /// trip anything. The same room with no canceller at all separates them —
    /// it must interrupt the reply before the user has said a word.
    #[test]
    fn without_the_canceller_the_same_room_interrupts_the_reply_on_its_own_echo() {
        let run = play_over_the_user(Box::new(super::super::echo::PassThrough), linear_room());
        let spurious = run
            .pauses
            .iter()
            .filter(|tick| run.before_the_user(**tick))
            .count();

        assert!(
            spurious > 0,
            "the room is too quiet to prove anything: {USER_ONSET_MS}ms of reply reached \
             the microphone uncancelled and never opened the gate"
        );
    }

    /// The failure seen in a real room: with laptop speakers every reply
    /// stopped on its first syllable while the user said nothing.
    ///
    /// The linear room above leaves AEC3 nothing it cannot subtract, so it
    /// never reproduced that. A real one does — reverberation, a speaker driven
    /// into distortion, a canceller that has not converged at the start of a
    /// reply — and what survives cancellation is a few frames above the
    /// activity floor. This room models only that survivor: a canceller that
    /// removes the linear echo exactly, and a burst it cannot see. The burst
    /// opens the gate but is shorter than `min_speech_ms`, which is the
    /// difference between echo that leaked and a person who started talking.
    #[test]
    fn a_residual_echo_burst_does_not_stop_the_reply() {
        const BURST_MS: u32 = 100;
        // Deterministic white noise at an RMS of about 0.035, three and a half
        // times the activity floor.
        let mut seed: u32 = 0x9e37_79b9;
        let burst: Vec<f32> = (0..ms_to_samples(BURST_MS))
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * 0.06
            })
            .collect();
        let config = SegmenterConfig::default();
        assert!(
            frame_rms(&burst[..FRAME_SAMPLES]) >= config.activity_rms
                && BURST_MS < config.min_speech_ms,
            "the burst must open the gate and stay short of speech, or this proves nothing"
        );

        let run = play_over_the_user(
            Box::new(Subtract),
            Room {
                delay_ms: 0,
                gain: 1.0,
                residual: burst,
            },
        );

        // `measured` also proves the user who does talk over the reply still
        // stops it and still keeps their first words.
        let measured = run.measured();
        assert_eq!(
            measured.false_triggers, 0,
            "the reply stopped itself on its own residual echo"
        );
    }

    // --- Barge-in obeys the wake word (1376-f33e) -------------------------

    fn capture_with_a_voice_and_a_phrase() -> (
        Capture,
        std::sync::Arc<CountingSpeaker>,
        parking_lot::Mutex<HandsFree>,
    ) {
        let (capture, speaker) = capture_with_a_voice();
        (
            capture,
            speaker,
            parking_lot::Mutex::new(armed_with_phrase("computer")),
        )
    }

    /// The log of 2026-10-02: replies cut with no turn accepted after them,
    /// because voice activity alone stopped them while a turn needs the wake
    /// word. Speech that is not addressed to us may only pause the reply.
    #[test]
    fn speech_without_the_wake_word_pauses_the_reply_and_lets_it_carry_on() {
        let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
        let mut endpoint = FakeEndpoint::new("what a nice day");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.feed(speech(500));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert_eq!((speaker.pauses(), speaker.resumes()), (1, 0));

        endpoint.feed(silence(600));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 800);

        assert_eq!(speaker.resumes(), 1, "the reply stayed paused");
        assert_eq!(
            speaker.hushes(),
            0,
            "speech for somebody else ended the reply"
        );
    }

    #[test]
    fn the_wake_word_over_a_reply_stops_it_and_takes_the_turn() {
        let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
        let mut endpoint = FakeEndpoint::new("computer, stop");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.feed(speech(500));
        endpoint.feed(silence(600));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 800);

        assert_eq!((speaker.pauses(), speaker.hushes()), (1, 1));
        assert_eq!(speaker.resumes(), 0, "a stopped reply was resumed");
    }

    /// A pause is held for a verdict. A speech that never closes (a noisy
    /// room) or a loop that ends must not leave the reply silent for good.
    #[test]
    fn a_pause_never_outlives_the_loop_or_the_pause_limit() {
        let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
        let mut endpoint = FakeEndpoint::new("noise");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.feed(speech(300));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert_eq!(speaker.pauses(), 1);

        endpoint.feed(speech(300));
        tick(
            &mut capture,
            &mode,
            &mut endpoint,
            &target,
            &queue,
            100 + PAUSE_LIMIT_MS + 1,
        );
        assert_eq!(speaker.resumes(), 1, "still paused past the limit");

        capture.release();
        assert_eq!(speaker.resumes(), 1, "released twice");

        let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
        endpoint.feed(speech(300));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        capture.release();
        assert_eq!(speaker.resumes(), 1, "the loop ended with the reply paused");
    }

    /// The pause limit is exclusive: a reply paused at 100 ms is still held at
    /// exactly `100 + PAUSE_LIMIT_MS` and released one millisecond later.
    /// Catches: `>` relaxed to `>=` in the limit check.
    #[test]
    fn a_pause_is_held_up_to_and_including_the_pause_limit() {
        let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
        let mut endpoint = FakeEndpoint::new("noise");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        endpoint.feed(speech(300));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert_eq!(speaker.pauses(), 1);

        endpoint.feed(speech(300));
        tick(
            &mut capture,
            &mode,
            &mut endpoint,
            &target,
            &queue,
            100 + PAUSE_LIMIT_MS,
        );
        assert_eq!(speaker.resumes(), 0, "released at the limit, not past it");
    }

    /// Residual echo from a reply that is still playing: a 20 ms frame over
    /// the floor every 200 ms. It keeps one utterance open and its frames add
    /// up to `min_speech_ms` inside a second, which stopped every reply 600-900
    /// ms in. A voice is an unbroken run.
    #[test]
    fn scattered_residual_echo_frames_do_not_add_up_to_a_voice() {
        let mut segmenter = Segmenter::new(test_config());
        for _ in 0..6 {
            segmenter.push(&speech(20));
            segmenter.push(&silence(180));
        }
        assert!(
            segmenter.is_capturing(),
            "the test needs one open utterance"
        );
        assert!(
            segmenter.has_speech(),
            "the frames do add up for the send rule"
        );
        assert!(!segmenter.has_voice(), "echo frames counted as a voice");

        segmenter.push(&speech(120));
        assert!(segmenter.has_voice(), "an unbroken run is a voice");
    }

    /// Push `chunks` (one list per call) and close the utterance with a long
    /// silence; returns the single closed utterance.
    fn closed_utterance(chunks: &[Vec<f32>]) -> Utterance {
        let mut segmenter = Segmenter::new(test_config());
        let mut closed = Vec::new();
        for chunk in chunks {
            closed.extend(segmenter.push(chunk));
        }
        closed.extend(segmenter.push(&silence(600)));
        assert_eq!(closed.len(), 1, "the test needs exactly one utterance");
        closed.remove(0)
    }

    /// A dip of 40 ms followed by 60 ms of activity joins the run: the dip and
    /// the three confirming frames all count as time in the run (100 + 40 + 60).
    /// Catches: the bridge counters at the dip, `bridge_ms` arithmetic
    /// (`+=` flipped), `bridge_frames` counting (`+=` flipped) and the
    /// `bridge_ms = gap + frame` seed, each of which changes the exact length.
    #[test]
    fn a_confirmed_dip_adds_the_dip_and_the_confirming_frames_to_the_run() {
        let utterance = closed_utterance(&[speech(100), silence(40), speech(60)]);
        assert_eq!(utterance.longest_run_ms, 200);
    }

    /// Two active frames after a dip do not confirm it: the run is still the
    /// 100 ms before the dip. Catches: the confirm comparison flipped, which
    /// would credit the unconfirmed bridge.
    #[test]
    fn a_dip_followed_by_too_few_frames_is_not_added_to_the_run() {
        let utterance = closed_utterance(&[speech(100), silence(40), speech(40)]);
        assert_eq!(utterance.longest_run_ms, 100);
    }

    /// `is_voice` judges the run, not the summed speech. Five frames 100 ms
    /// apart add up to `min_speech_ms` and close an utterance, but no run
    /// reaches it. Catches: `is_voice` returning true unconditionally.
    #[test]
    fn a_closed_utterance_of_scattered_frames_is_not_a_voice() {
        let mut segmenter = Segmenter::new(test_config());
        let mut input = Vec::new();
        for _ in 0..5 {
            input.extend(speech(20));
            input.extend(silence(80));
        }
        input.extend(silence(600));
        let closed = segmenter.push(&input);
        assert_eq!(closed.len(), 1, "the test needs one closed utterance");
        assert!(
            closed[0].speech_ms >= 100,
            "enough summed speech to be sent"
        );
        assert!(!segmenter.is_voice(&closed[0]));

        let sustained = closed_utterance(&[speech(500)]);
        assert!(segmenter.is_voice(&sustained));
    }

    /// The pill stuck on "capturing" (or showing the last state) after an
    /// utterance that was too short to transcribe.
    #[test]
    fn an_utterance_dropped_for_too_little_speech_ends_the_capturing_phase() {
        let mode = armed_shared();
        let mut capture = runtime_capture();
        let mut endpoint = FakeEndpoint::new("unused");
        let target = FakeTarget(std::cell::Cell::new(true));
        let queue = FakeSink::default();

        let mut chunk = speech(20);
        chunk.extend(silence(100));
        endpoint.feed(chunk);
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
        assert_eq!(*mode.lock().phase(), Phase::Capturing);

        endpoint.feed(silence(500));
        tick(&mut capture, &mode, &mut endpoint, &target, &queue, 700);

        assert_eq!(
            endpoint.calls.get(),
            0,
            "the test needs a dropped utterance"
        );
        assert_eq!(*mode.lock().phase(), Phase::Waiting);
    }

    /// Probes by the critic of 1376-f33e.
    mod critic_1376 {
        use super::*;

        /// Catches: `paused_at_ms` left set after a stop, so the next time the
        /// user talks over the reply it is never paused again.
        #[test]
        fn the_reply_is_paused_again_by_the_next_speech_after_a_verdict() {
            for transcript in ["what a nice day", "computer, stop"] {
                let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
                let mut endpoint = FakeEndpoint::new(transcript);
                let target = FakeTarget(std::cell::Cell::new(true));
                let queue = FakeSink::default();

                endpoint.feed(speech(300));
                tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
                endpoint.feed(silence(500));
                tick(&mut capture, &mode, &mut endpoint, &target, &queue, 700);
                endpoint.feed(speech(300));
                tick(&mut capture, &mode, &mut endpoint, &target, &queue, 800);

                assert_eq!(
                    speaker.pauses(),
                    2,
                    "{transcript}: the second speech did not pause"
                );
            }
        }

        /// Catches: the wake word, spoken after the pause limit already let the
        /// reply carry on, being treated as a no-op because nothing is paused.
        #[test]
        fn the_wake_word_still_stops_a_reply_the_pause_limit_let_go_on() {
            let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
            let mut endpoint = FakeEndpoint::new("computer, stop");
            let target = FakeTarget(std::cell::Cell::new(true));
            let queue = FakeSink::default();

            endpoint.feed(speech(300));
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
            endpoint.feed(speech(300));
            let late = 100 + PAUSE_LIMIT_MS + 1;
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, late);
            assert_eq!(speaker.resumes(), 1);
            endpoint.feed(silence(500));
            tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &target,
                &queue,
                late + 600,
            );

            assert_eq!(
                speaker.hushes(),
                1,
                "the reply played on over the wake word"
            );
            assert_eq!(speaker.resumes(), 1, "resumed twice");
        }

        /// Catches: an arm added for `Empty`/`Stale`/`NotArmed` that stops the
        /// reply, or drops the resume, when nothing was recognised.
        #[test]
        fn nothing_recognised_resumes_the_reply_and_does_not_stop_it() {
            let (mut capture, speaker, mode) = capture_with_a_voice_and_a_phrase();
            let mut endpoint = FakeEndpoint::new("");
            let target = FakeTarget(std::cell::Cell::new(true));
            let queue = FakeSink::default();

            endpoint.feed(speech(300));
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 100);
            endpoint.feed(silence(500));
            tick(&mut capture, &mode, &mut endpoint, &target, &queue, 700);

            assert_eq!(endpoint.calls.get(), 1, "the test needs a transcription");
            assert_eq!((speaker.resumes(), speaker.hushes()), (1, 0));
        }

        /// Speech has dips: a plosive closure is 40-100 ms of quiet inside one
        /// word. A voice that is only "unbroken" never reaches the 200 ms bar
        /// when its bursts are shorter than that, so the reply talks on over a
        /// user who is plainly speaking. Scattered echo (20 ms in 200) must stay
        /// out. Catches: `has_voice` requiring a gapless run.
        #[test]
        fn speech_with_short_dips_is_a_voice() {
            let mut segmenter = Segmenter::new(SegmenterConfig::default());
            for _ in 0..8 {
                segmenter.push(&speech(140));
                segmenter.push(&silence(40));
            }
            assert!(
                segmenter.has_voice(),
                "1.4 s of speech with 40 ms dips was not a voice"
            );
        }
    }
}
