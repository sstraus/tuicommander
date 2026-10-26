//! Hearing the user over our own voice.
//!
//! The microphone hears the speaker. Without this, the energy VAD in
//! [`continuous`](super::continuous) opens a turn on the reply the application
//! is speaking, transcribes it, and answers itself. Muting capture while
//! speaking would stop that and would also stop the user interrupting, which
//! is the one thing hands-free has to get right — so the capture stream stays
//! open and the echo is subtracted from it instead.
//!
//! ## The two streams must arrive in step
//!
//! A canceller subtracts a delayed, filtered copy of the **far end** (what we
//! played) from the **near end** (what the microphone heard). It can only do
//! that if it is fed both at the same pace: one 10 ms frame of each, in turn.
//!
//! Our two sources do not behave that way. Capture arrives in small chunks as
//! the device produces them, while [`speaker`](super::speaker) hands over a
//! whole rendered utterance at once — seconds of audio in one call, before a
//! single sample of it has left the speaker. Pushing that straight into a
//! canceller would put it seconds ahead of the microphone and it would subtract
//! nothing.
//!
//! [`FarEnd`] is the buffer that fixes the pace: the reply goes in whole, and
//! comes out only as fast as capture is consumed, padded with silence whenever
//! nothing is playing. Alignment is therefore by sample count rather than by
//! wall clock, and the residual offset — the device's own output latency — is
//! what the canceller's delay estimator is for.
//!
//! ## Everything here runs at 16 kHz
//!
//! Not a preference: [`audio`](super::audio) already converts capture to mono
//! 16 kHz for Whisper, and 16 kHz is one of the rates the WebRTC APM accepts,
//! so the near end needs no conversion at all. The far end is whatever the
//! speech engine rendered — 24 kHz for Pocket TTS, anything for a
//! user-supplied command — and is converted on the way in.

// Same reason as `speech.rs` and `speaker.rs`: the port and its adapter are
// finished and tested, and the consumer that arms them is the hands-free
// runtime in `continuous.rs`. Drop this when that wiring lands.
#![allow(dead_code)]

pub mod webrtc;

use std::collections::VecDeque;

use super::speech::SpeechAudio;

/// The rate both streams are fed at, which is the rate capture already
/// arrives at. See the module note.
pub const SAMPLE_RATE: u32 = 16_000;

/// 10 ms. The APM takes this and nothing else.
pub const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 100;

/// How much un-consumed reply audio the far end will hold: two minutes.
///
/// A reply arrives whole, before any of it has played, and replies queue
/// behind the one playing — so the buffer must hold every reply the speaker
/// has accepted, not a window of recent audio. Two seconds used to be the
/// cap, and it cut every longer reply down to its last two seconds: the
/// start of playback was matched against the end of the reply, then against
/// silence, and hands-free heard itself.
///
/// Reached only when capture stops consuming — the mode was disarmed
/// mid-reply, or the device stopped delivering. 120 s at 16 kHz is ~7.7 MB,
/// and only while that much is actually queued.
const FAR_END_CAPACITY: usize = SAMPLE_RATE as usize * 120;

/// Subtracts our own voice from what the microphone heard.
///
/// A port so the pace-keeping in [`EchoGuard`] and the wiring in
/// [`continuous`](super::continuous) can be proven without a canceller, and so
/// a build without one degrades to [`PassThrough`] instead of failing to arm.
pub trait Canceller: Send {
    /// Subtract `far_end` from `near_end`, in place. Both are exactly
    /// [`FRAME_SAMPLES`] long, mono, at [`SAMPLE_RATE`].
    fn cancel(&mut self, far_end: &[f32], near_end: &mut [f32]);
}

/// No cancellation. What a build without an echo canceller gets.
///
/// Not a silent fallback: whoever installs this owns saying so. It exists
/// because a hands-free mode that refuses to arm is worse than one that cannot
/// be interrupted over the speaker — headphones still work.
#[derive(Debug, Default)]
pub struct PassThrough;

impl Canceller for PassThrough {
    fn cancel(&mut self, _far_end: &[f32], _near_end: &mut [f32]) {}
}

/// Reply audio waiting to be matched against capture.
///
/// Filled a whole utterance at a time, drained one capture chunk at a time.
#[derive(Debug, Default)]
pub struct FarEnd {
    samples: VecDeque<f32>,
    /// Samples dropped because capture never caught up. Reported rather than
    /// hidden: a climbing count means playback and capture have lost step,
    /// which shows up as an echo that stops being cancelled.
    dropped: u64,
    /// Capture samples handed out so far, which is what the pause positions
    /// below are measured in.
    position: usize,
    /// Capture positions `(from, until)` where the speaker was held silent.
    /// What is queued has not left it, so capture is matched against silence
    /// inside a window; `until` is `None` while the pause lasts. Positions, not
    /// a flag: capture recorded before the pause but not yet cleaned still
    /// heard the reply. A list, because a second pause can open while the
    /// backlog still holds the first.
    pauses: VecDeque<(usize, Option<usize>)>,
}

impl FarEnd {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a rendered reply.
    pub fn push(&mut self, audio: &SpeechAudio) {
        if audio.sample_rate == 0 || audio.samples.is_empty() {
            return;
        }
        if audio.sample_rate == SAMPLE_RATE {
            self.samples.extend(audio.samples.iter().copied());
        } else {
            self.samples
                .extend(resample(&audio.samples, audio.sample_rate));
        }
        if self.samples.len() > FAR_END_CAPACITY {
            let excess = self.samples.len() - FAR_END_CAPACITY;
            // Drop the newest. Nothing queued here has played yet, so the head
            // is what the microphone hears next; dropping it would align the
            // start of playback with the wrong seconds of the reply.
            self.samples.truncate(FAR_END_CAPACITY);
            self.dropped += excess as u64;
            tracing::warn!(
                source = "dictation",
                "echo: far-end reference full, dropped the newest {excess} samples \
                 ({} dropped in total); that part of the reply will not be cancelled",
                self.dropped
            );
        }
    }

    /// Queue `count` samples of silence: capture that precedes the next reply.
    pub fn pad(&mut self, count: usize) {
        self.samples.extend(std::iter::repeat_n(0.0, count));
    }

    /// Take the next `count` samples, padding with silence.
    ///
    /// Silence is the truthful answer when nothing is playing, and it is also
    /// what keeps the canceller converged through the gaps between replies:
    /// skipping the call instead would make it lose its place.
    pub fn take(&mut self, count: usize) -> Vec<f32> {
        let mut frame = Vec::with_capacity(count);
        for _ in 0..count {
            while self
                .pauses
                .front()
                .is_some_and(|&(_, until)| until.is_some_and(|at| self.position >= at))
            {
                self.pauses.pop_front();
            }
            let silent = self
                .pauses
                .front()
                .is_some_and(|&(from, _)| self.position >= from);
            self.position += 1;
            frame.push(if silent {
                0.0
            } else {
                self.samples.pop_front().unwrap_or(0.0)
            });
        }
        frame
    }

    /// Forget the queued reply. Called when playback is stopped: that audio is
    /// never going to reach the microphone, so matching capture against it
    /// would subtract a sound that is not there.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.pauses.clear();
    }

    /// Where the next sample handed out will sit in the capture stream, plus
    /// `recorded` samples that capture already holds but this has not seen.
    fn position_after(&self, recorded: usize) -> usize {
        self.position + recorded
    }

    /// How many of the next `count` capture positions lie inside a closed
    /// pause window.
    fn silent_within(&self, count: usize) -> usize {
        let (start, end) = (self.position, self.position + count);
        self.pauses
            .iter()
            .filter_map(|&(from, until)| Some((from, until?)))
            .map(|(from, until)| until.min(end).saturating_sub(from.max(start)))
            .sum()
    }

    /// Is the reply held back right now, with no continuation yet?
    fn is_paused(&self) -> bool {
        self.pauses
            .back()
            .is_some_and(|&(_, until)| until.is_none())
    }

    /// Is any reply audio still waiting to be matched?
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// Linear interpolation to [`SAMPLE_RATE`].
///
/// Linear rather than the nearest-neighbour [`audio`](super::audio) uses on
/// the capture path. There it feeds a recogniser that tolerates the artefacts;
/// here it feeds a canceller that models the far end as a *linear* filter of
/// this signal, so every artefact this adds is echo it cannot subtract.
fn resample(samples: &[f32], from_rate: u32) -> Vec<f32> {
    let ratio = f64::from(SAMPLE_RATE) / f64::from(from_rate);
    let out_len = ((samples.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len {
        let position = n as f64 / ratio;
        let left = position.floor() as usize;
        if left + 1 >= samples.len() {
            out.push(samples[samples.len() - 1]);
            continue;
        }
        let fraction = (position - left as f64) as f32;
        out.push(samples[left] * (1.0 - fraction) + samples[left + 1] * fraction);
    }
    out
}

/// How much capture has been recorded but not yet handed to
/// [`EchoGuard::clean`]: the samples still waiting in the endpoint's buffer.
///
/// A probe rather than a number because it is read by the render thread, at
/// the moment a reply is handed to the device, while the buffer belongs to
/// whatever feeds the capture loop.
pub type Backlog = Box<dyn Fn() -> usize + Send>;

/// The far-end buffer and the canceller behind it, kept in step.
///
/// One of these is shared by [`speaker`](super::speaker), which fills it, and
/// the hands-free tick, which drains it.
pub struct EchoGuard {
    far_end: FarEnd,
    canceller: Box<dyn Canceller>,
    /// Capture samples left over from the last call: the canceller works in
    /// whole 10 ms frames and a device chunk is not a multiple of one.
    remainder: Vec<f32>,
    /// Capture not yet drained, for aligning the start of a reply. `None`
    /// until a capture stream is attached.
    backlog: Option<Backlog>,
}

impl EchoGuard {
    pub fn new(canceller: Box<dyn Canceller>) -> Self {
        Self {
            far_end: FarEnd::new(),
            canceller,
            remainder: Vec::new(),
            backlog: None,
        }
    }

    /// A new capture stream feeds this guard from now on.
    ///
    /// Whatever the previous stream left — queued reply, a partial frame — was
    /// counted against capture that will never be cleaned, so it goes.
    pub fn attach_capture(&mut self, backlog: Backlog) {
        self.far_end.clear();
        self.remainder.clear();
        self.backlog = Some(backlog);
    }

    /// A reply about to be played. Called by the render thread, before the
    /// audio reaches the device, so the far end is never behind the near end.
    ///
    /// With nothing queued, the device starts this reply now, and every
    /// capture sample recorded but not yet cleaned — the endpoint's backlog
    /// plus the carried partial frame — was heard *before* it. Those samples
    /// are matched against silence first. Without that the reply's head is
    /// paired with audio captured before it played, and the reference runs
    /// ahead of its echo by however long the capture loop was busy.
    ///
    /// A reply queued behind one still playing follows it with no gap,
    /// because that is how the device plays it.
    pub fn note_rendered(&mut self, audio: &SpeechAudio) {
        // Not while paused: the reply starts when playback continues, and the
        // capture recorded until then is not ahead of it.
        if self.far_end.is_empty() && !self.far_end.is_paused() {
            // Pad up to where the reply starts, minus what a closed pause
            // window already covers: `take` hands out silence inside a window
            // without using the pad, so counting it twice starts the reply late.
            let recorded = self.recorded();
            let covered = self.far_end.silent_within(recorded);
            self.far_end.pad(recorded - covered);
        }
        self.far_end.push(audio);
    }

    /// The speaker was paused: nothing reaches the microphone until
    /// [`note_resumed`](Self::note_resumed), and the queued reply must not be
    /// consumed meanwhile.
    ///
    /// Takes effect after the capture already recorded: the device went quiet
    /// now, and everything the microphone recorded until now heard the reply.
    pub fn note_paused(&mut self) {
        if !self.far_end.is_paused() {
            let from = self.far_end.position_after(self.recorded());
            self.far_end.pauses.push_back((from, None));
        }
    }

    /// The same offset on the way out: what was recorded while paused heard
    /// silence, and the reply continues with what is recorded from now on.
    pub fn note_resumed(&mut self) {
        if self.far_end.is_paused() {
            let until = self.far_end.position_after(self.recorded());
            if let Some(window) = self.far_end.pauses.back_mut() {
                window.1 = Some(until);
            }
        }
    }

    /// Capture recorded but not yet cleaned: the carried partial frame and the
    /// endpoint's backlog.
    fn recorded(&self) -> usize {
        self.remainder.len() + self.backlog.as_ref().map_or(0, |backlog| backlog())
    }

    /// Playback was stopped. Drop the queued reply and the partial frame with
    /// it — both describe a sound that is no longer going to happen.
    pub fn note_stopped(&mut self) {
        self.far_end.clear();
    }

    /// Clean one chunk of capture.
    ///
    /// Returns the samples it could clean, which is everything but a partial
    /// trailing frame — under 10 ms, carried into the next call. The caller
    /// gets fewer samples than it gave, never different ones, and never out of
    /// order.
    pub fn clean(&mut self, capture: &[f32]) -> Vec<f32> {
        self.remainder.extend_from_slice(capture);
        let frames = self.remainder.len() / FRAME_SAMPLES;
        if frames == 0 {
            return Vec::new();
        }

        let mut cleaned: Vec<f32> = self.remainder.drain(..frames * FRAME_SAMPLES).collect();
        for frame in cleaned.as_chunks_mut::<FRAME_SAMPLES>().0 {
            let far_end = self.far_end.take(FRAME_SAMPLES);
            self.canceller.cancel(&far_end, frame);
        }
        cleaned
    }

    /// Is a reply still queued against capture? The hands-free tick asks this
    /// to tell "the user is talking" from "we are still hearing ourselves".
    pub fn is_playing(&self) -> bool {
        !self.far_end.is_empty()
    }

    pub fn dropped_far_end(&self) -> u64 {
        self.far_end.dropped()
    }
}

/// Installs a canceller, saying out loud which one.
///
/// Never fails: a hands-free mode that refuses to arm is worse than one that
/// cannot be interrupted over the speaker, and headphones still work. The
/// warning is the whole point — [`PassThrough`] must not be reached quietly.
pub fn install() -> EchoGuard {
    match webrtc::WebRtc::new() {
        Ok(canceller) => EchoGuard::new(Box::new(canceller)),
        Err(reason) => {
            tracing::warn!(
                "dictation: no echo cancellation ({reason}). Hands-free will hear its own \
                 replies unless you wear headphones."
            );
            EchoGuard::new(Box::new(PassThrough))
        }
    }
}

/// A [`speaker::Output`](super::speaker::Output) that tells an [`EchoGuard`]
/// what it is about to play.
///
/// The far-end tap belongs here rather than inside
/// [`Speaker`](super::speaker::Speaker): the queue's job is deciding *what* is
/// played and when it stops, and both of those are already expressed through
/// the `Output` port. Wrapping the port catches every one of them without the
/// queue growing an opinion about microphones.
pub struct FarEndTap {
    inner: std::sync::Arc<dyn super::speaker::Output>,
    guard: std::sync::Arc<parking_lot::Mutex<EchoGuard>>,
}

impl FarEndTap {
    pub fn new(
        inner: std::sync::Arc<dyn super::speaker::Output>,
        guard: std::sync::Arc<parking_lot::Mutex<EchoGuard>>,
    ) -> Self {
        Self { inner, guard }
    }
}

impl super::speaker::Output for FarEndTap {
    fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
        // Noted before the device is handed the audio, never after: the far
        // end may run ahead of what the microphone hears, because alignment is
        // by sample count and the canceller's estimator absorbs the offset. It
        // may not run behind, because reference that arrives after its own
        // echo cannot be subtracted from it.
        self.guard.lock().note_rendered(audio);
        self.inner.play(audio)
    }

    fn pause(&self) {
        // Device first, for the same reason as `stop`: the far end is what the
        // microphone has yet to hear.
        self.inner.pause();
        // Only if the output actually holds its audio. One that keeps playing
        // would leave the reference silent while the microphone still hears
        // the reply, and the far end a whole pause behind it afterwards.
        if self.inner.can_pause() {
            self.guard.lock().note_paused();
        }
    }

    fn resume(&self) {
        // Far end first: it may run ahead of the microphone, never behind.
        if self.inner.can_pause() {
            self.guard.lock().note_resumed();
        }
        self.inner.resume();
    }

    fn can_pause(&self) -> bool {
        self.inner.can_pause()
    }

    fn stop(&self) {
        // Device first. The far end is what remains to be cancelled, so
        // forgetting it before the speaker is actually quiet would leave the
        // last few milliseconds of the reply in the microphone with no
        // reference to subtract.
        self.inner.stop();
        self.guard.lock().note_stopped();
    }

    fn is_speaking(&self) -> bool {
        self.inner.is_speaking()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(samples: Vec<f32>, sample_rate: u32) -> SpeechAudio {
        SpeechAudio {
            samples,
            sample_rate,
        }
    }

    /// A guard whose canceller can be read back afterwards.
    struct Probe {
        guard: EchoGuard,
        seen: std::sync::Arc<parking_lot::Mutex<Vec<Vec<f32>>>>,
    }

    /// Records what it was asked to subtract, and subtracts it exactly. A real
    /// canceller is adaptive; this one lets a test state what reached it.
    struct Tee(std::sync::Arc<parking_lot::Mutex<Vec<Vec<f32>>>>);

    impl Canceller for Tee {
        fn cancel(&mut self, far_end: &[f32], near_end: &mut [f32]) {
            self.0.lock().push(far_end.to_vec());
            for (near, far) in near_end.iter_mut().zip(far_end) {
                *near -= far;
            }
        }
    }

    impl Probe {
        fn new() -> Self {
            let seen = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
            Self {
                guard: EchoGuard::new(Box::new(Tee(std::sync::Arc::clone(&seen)))),
                seen,
            }
        }

        /// The far-end frames the canceller was handed, flattened.
        fn far_end_seen(&self) -> Vec<f32> {
            self.seen.lock().iter().flatten().copied().collect()
        }
    }

    #[test]
    fn a_whole_reply_is_released_only_as_fast_as_capture_is_consumed() {
        // The case the buffer exists for: the speaker hands over a second of
        // audio at once, long before any of it has been heard.
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));

        let cleaned = probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);

        assert_eq!(cleaned.len(), FRAME_SAMPLES, "one frame in, one frame out");
        assert_eq!(
            probe.far_end_seen().len(),
            FRAME_SAMPLES,
            "the canceller was handed one frame, not a second of audio"
        );
        assert!(probe.guard.is_playing(), "the rest is still queued");
    }

    #[test]
    fn a_partial_frame_is_carried_rather_than_cleaned_early_or_dropped() {
        // A device chunk is not a multiple of 10 ms. The tail must reappear,
        // in order, in the next call — losing it would notch the audio the
        // recogniser sees.
        let mut probe = Probe::new();
        let first: Vec<f32> = (0..FRAME_SAMPLES + 40).map(|n| n as f32).collect();
        let second: Vec<f32> = (0..FRAME_SAMPLES).map(|n| (n + 1000) as f32).collect();

        let a = probe.guard.clean(&first);
        let b = probe.guard.clean(&second);

        assert_eq!(a.len(), FRAME_SAMPLES);
        assert_eq!(b.len(), FRAME_SAMPLES);
        let seen: Vec<f32> = a.into_iter().chain(b).collect();
        let expected: Vec<f32> = first
            .iter()
            .chain(second.iter())
            .take(2 * FRAME_SAMPLES)
            .copied()
            .collect();
        assert_eq!(seen, expected, "samples were reordered or lost");
    }

    #[test]
    fn capture_shorter_than_a_frame_cleans_nothing_yet() {
        let mut probe = Probe::new();
        assert_eq!(probe.guard.clean(&[0.1; 40]), [] as [f32; 0]);
        assert!(
            probe.seen.lock().is_empty(),
            "the canceller was called early"
        );
    }

    #[test]
    fn silence_is_fed_when_nothing_is_playing() {
        // Skipping the call instead would let an adaptive canceller lose its
        // place between replies, so the first frame of the next one comes
        // through uncancelled.
        let mut probe = Probe::new();

        probe.guard.clean(&vec![0.5; FRAME_SAMPLES]);

        assert_eq!(probe.far_end_seen(), vec![0.0; FRAME_SAMPLES]);
        assert!(!probe.guard.is_playing());
    }

    #[test]
    fn stopping_playback_forgets_the_reply_that_will_not_be_heard() {
        // hush() stopped the device. Matching capture against audio nobody
        // played would subtract a sound that is not in it.
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));

        probe.guard.note_stopped();
        probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);

        assert_eq!(probe.far_end_seen(), vec![0.0; FRAME_SAMPLES]);
        assert!(!probe.guard.is_playing());
    }

    #[test]
    fn the_echo_is_actually_subtracted_from_the_capture_that_is_returned() {
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![0.25; FRAME_SAMPLES], SAMPLE_RATE));

        let cleaned = probe.guard.clean(&vec![0.75; FRAME_SAMPLES]);

        assert_eq!(cleaned, vec![0.5; FRAME_SAMPLES]);
    }

    #[test]
    fn a_reply_at_the_engines_rate_is_converted_on_the_way_in() {
        // Pocket TTS renders at 24 kHz. Feeding that to a 16 kHz canceller
        // unconverted would hand it a far end that is 1.5x too fast, which
        // correlates with nothing the microphone heard.
        let mut far_end = FarEnd::new();
        far_end.push(&audio(vec![1.0; 2_400], 24_000));

        assert_eq!(
            far_end.take(10_000).iter().filter(|s| **s != 0.0).count(),
            1_600
        );
    }

    #[test]
    fn conversion_interpolates_rather_than_picking_the_nearest_sample() {
        // A ramp resampled by nearest neighbour comes out as steps, and every
        // step is echo a linear canceller cannot model.
        let ramp: Vec<f32> = (0..300).map(|n| n as f32).collect();

        let converted = resample(&ramp, 30_000);

        // 30 kHz -> 16 kHz: output sample n sits at input position n * 30/16.
        assert_eq!(converted.len(), 160);
        assert!((converted[1] - 1.875).abs() < 1e-4, "{}", converted[1]);
        assert!((converted[3] - 5.625).abs() < 1e-4, "{}", converted[3]);
    }

    #[test]
    fn a_reply_already_at_the_capture_rate_is_passed_through_untouched() {
        let mut far_end = FarEnd::new();
        let samples: Vec<f32> = (0..FRAME_SAMPLES).map(|n| n as f32).collect();
        far_end.push(&audio(samples.clone(), SAMPLE_RATE));

        assert_eq!(far_end.take(FRAME_SAMPLES), samples);
    }

    #[test]
    fn a_reply_nobody_consumes_is_bounded_rather_than_unbounded() {
        // Disarming mid-reply, or a device that stops delivering. The buffer
        // must not grow with the conversation.
        let mut far_end = FarEnd::new();
        for _ in 0..=FAR_END_CAPACITY / SAMPLE_RATE as usize {
            far_end.push(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));
        }

        assert!(far_end.samples.len() <= FAR_END_CAPACITY);
        assert!(far_end.dropped() > 0, "dropping silently is the failure");
    }

    #[test]
    fn a_reply_longer_than_two_seconds_reaches_the_canceller_from_its_first_sample() {
        // The reply is handed over whole, before a sample of it has played, so
        // its first second is what the microphone hears first. A buffer that
        // kept only the newest two seconds matched the start of playback
        // against the end of the reply, then against silence: nothing a
        // six-second reply said was cancelled, and hands-free heard itself.
        let mut probe = Probe::new();
        let reply: Vec<f32> = (0..SAMPLE_RATE as usize * 6)
            .map(|n| (n % 1_000) as f32 + 1.0)
            .collect();
        probe
            .guard
            .note_rendered(&audio(reply.clone(), SAMPLE_RATE));

        probe.guard.clean(&vec![0.0; reply.len()]);

        let seen = probe.far_end_seen();
        let diverged = seen
            .iter()
            .zip(&reply)
            .position(|(a, b)| a.to_bits() != b.to_bits());
        assert_eq!(
            (seen.len(), diverged),
            (reply.len(), None),
            "the reference is not the reply as played (length, first differing sample)"
        );
        assert_eq!(probe.guard.dropped_far_end(), 0);
    }

    /// A guard whose capture endpoint still holds `pending` undrained samples.
    fn probe_with_backlog(pending: usize) -> Probe {
        let mut probe = Probe::new();
        probe.guard.attach_capture(Box::new(move || pending));
        probe
    }

    fn ramp(len: usize) -> Vec<f32> {
        (0..len).map(|n| n as f32 + 1.0).collect()
    }

    #[test]
    fn capture_waiting_to_be_drained_when_a_reply_starts_is_matched_against_silence() {
        // The capture loop drains every 50 ms and transcribes inline, so a
        // reply can start with seconds of capture still undrained. That audio
        // was recorded before the reply played; pairing it with the reply's
        // first samples puts the reference ahead of its echo, and AEC3 cannot
        // cancel an echo that arrives before its reference is due.
        let pending = 3 * FRAME_SAMPLES;
        let mut probe = probe_with_backlog(pending);
        let reply = ramp(2 * FRAME_SAMPLES);
        probe
            .guard
            .note_rendered(&audio(reply.clone(), SAMPLE_RATE));

        probe.guard.clean(&vec![0.0; pending + reply.len()]);

        let mut expected = vec![0.0; pending];
        expected.extend(&reply);
        assert_eq!(probe.far_end_seen(), expected);
    }

    #[test]
    fn a_partial_frame_already_drained_also_precedes_the_reply() {
        // The carried remainder was drained before the reply existed, so it is
        // backlog too, even with nothing left in the endpoint.
        let mut probe = probe_with_backlog(0);
        probe.guard.clean(&[0.0; 40]);
        let reply = ramp(FRAME_SAMPLES);
        probe
            .guard
            .note_rendered(&audio(reply.clone(), SAMPLE_RATE));

        probe.guard.clean(&vec![0.0; 2 * FRAME_SAMPLES - 40]);

        let seen = probe.far_end_seen();
        assert_eq!(seen[..40], vec![0.0; 40][..]);
        assert_eq!(seen[40..40 + FRAME_SAMPLES], reply[..]);
    }

    #[test]
    fn a_reply_queued_behind_one_still_playing_follows_it_without_a_gap() {
        // The device plays the second reply straight after the first, so its
        // reference must follow the first's directly. Padding it with the
        // backlog would open a gap that the device never plays.
        let pending = 3 * FRAME_SAMPLES;
        let mut probe = probe_with_backlog(pending);
        let first = ramp(FRAME_SAMPLES);
        let second: Vec<f32> = first.iter().map(|s| -s).collect();
        probe
            .guard
            .note_rendered(&audio(first.clone(), SAMPLE_RATE));
        probe
            .guard
            .note_rendered(&audio(second.clone(), SAMPLE_RATE));

        probe.guard.clean(&vec![0.0; pending + 2 * FRAME_SAMPLES]);

        let seen = probe.far_end_seen();
        assert_eq!(seen[pending..pending + FRAME_SAMPLES], first[..]);
        assert_eq!(seen[pending + FRAME_SAMPLES..], second[..]);
    }

    #[test]
    fn attaching_a_new_capture_stream_forgets_what_the_old_one_left() {
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; FRAME_SAMPLES], SAMPLE_RATE));
        probe.guard.clean(&[0.0; 40]);

        probe.guard.attach_capture(Box::new(|| 0));
        probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);

        assert_eq!(probe.far_end_seen(), vec![0.0; FRAME_SAMPLES]);
        assert!(!probe.guard.is_playing());
    }

    #[test]
    fn a_full_buffer_drops_the_newest_audio_never_the_reply_about_to_play() {
        // Only reached when capture has stopped consuming. The head is still
        // what the microphone hears next; the tail is what has to go.
        let mut far_end = FarEnd::new();
        far_end.push(&audio(vec![1.0; FAR_END_CAPACITY], SAMPLE_RATE));
        far_end.push(&audio(vec![2.0; FRAME_SAMPLES], SAMPLE_RATE));

        assert_eq!(far_end.take(FAR_END_CAPACITY), vec![1.0; FAR_END_CAPACITY]);
        assert_eq!(far_end.dropped(), FRAME_SAMPLES as u64);
    }

    #[test]
    fn an_engine_that_reports_nothing_playable_is_ignored_rather_than_queued() {
        let mut far_end = FarEnd::new();
        far_end.push(&audio(Vec::new(), SAMPLE_RATE));
        far_end.push(&audio(vec![1.0; 100], 0));

        assert!(far_end.is_empty());
    }

    #[test]
    fn without_a_canceller_the_capture_is_returned_as_it_arrived() {
        // The degraded build. It must still be a working microphone.
        let mut guard = EchoGuard::new(Box::new(PassThrough));
        guard.note_rendered(&audio(vec![1.0; FRAME_SAMPLES], SAMPLE_RATE));
        let capture: Vec<f32> = (0..FRAME_SAMPLES).map(|n| n as f32 * 0.01).collect();

        assert_eq!(guard.clean(&capture), capture);
    }

    #[test]
    fn frames_reach_the_canceller_in_the_order_they_were_rendered() {
        // Two replies queued back to back come out as one continuous far end.
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; FRAME_SAMPLES], SAMPLE_RATE));
        probe
            .guard
            .note_rendered(&audio(vec![2.0; FRAME_SAMPLES], SAMPLE_RATE));

        probe.guard.clean(&vec![0.0; FRAME_SAMPLES * 2]);

        let seen = probe.far_end_seen();
        assert_eq!(seen[..FRAME_SAMPLES], vec![1.0; FRAME_SAMPLES][..]);
        assert_eq!(seen[FRAME_SAMPLES..], vec![2.0; FRAME_SAMPLES][..]);
    }

    /// Renders half a second of a quiet tone at the capture rate, so the far
    /// end needs no conversion and can be compared sample for sample.
    struct QuietTone;

    impl super::super::speech::Speech for QuietTone {
        fn synthesize(
            &self,
            _text: &str,
            _voice: &str,
            _cancel: &super::super::speech::SpeechCancel,
        ) -> Result<SpeechAudio, super::super::speech::SpeechError> {
            let phase = 2.0 * std::f32::consts::PI * 440.0 / SAMPLE_RATE as f32;
            Ok(audio(
                (0..SAMPLE_RATE as usize / 2)
                    .map(|n| 0.03 * (phase * n as f32).sin())
                    .collect(),
                SAMPLE_RATE,
            ))
        }
    }

    /// Remembers what the device was handed.
    #[derive(Default)]
    struct PlayedOutput(parking_lot::Mutex<Vec<f32>>);

    impl super::super::speaker::Output for PlayedOutput {
        fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
            self.0.lock().extend_from_slice(&audio.samples);
            Ok(())
        }
        fn stop(&self) {}
        fn is_speaking(&self) -> bool {
            false
        }
    }

    #[test]
    fn the_far_end_is_the_audio_played_after_the_loudness_stage() {
        // The speaker changes the level of every reply. A reference taken
        // before that change would be a different signal from the one in the
        // room, and the canceller would subtract the wrong thing.
        use super::super::loudness::Loudness;
        use super::super::speaker::Speaker;
        use std::sync::Arc;

        let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let guard = Arc::new(parking_lot::Mutex::new(EchoGuard::new(Box::new(Tee(
            Arc::clone(&seen),
        )))));
        let device = Arc::new(PlayedOutput::default());
        let tapped = Arc::new(FarEndTap::new(Arc::clone(&device) as _, Arc::clone(&guard)));
        let speaker = Speaker::new(Arc::new(QuietTone), tapped, 0);
        speaker.set_loudness(Loudness {
            volume_db: -18.0,
            levelling: 0.67,
        });

        speaker.say(0, "ciao", "").expect("queued");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while device.0.lock().is_empty() {
            assert!(std::time::Instant::now() < deadline, "nothing was played");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let played = device.0.lock().clone();
        guard.lock().clean(&vec![0.0; played.len()]);

        let far_end: Vec<f32> = seen.lock().iter().flatten().copied().collect();
        assert_eq!(far_end.len(), played.len());
        assert_eq!(far_end, played, "the reference is not what was played");
        let input_peak = 0.03;
        let played_peak = played.iter().fold(0f32, |m, &x| m.max(x.abs()));
        assert!(
            played_peak > input_peak * 2.0,
            "the loudness stage did not run, so this proves nothing: peak {played_peak}"
        );
    }

    /// Capture goes on while the speaker is paused, and the reply it will play
    /// next has not left it. Consuming it meanwhile pairs the reply's start
    /// with audio recorded before it, so the canceller subtracts the wrong
    /// seconds the moment playback continues.
    #[test]
    fn a_paused_reply_is_matched_against_silence_and_kept_for_later() {
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));

        probe.guard.note_paused();
        probe.guard.clean(&vec![0.0; FRAME_SAMPLES * 3]);
        assert!(
            probe.far_end_seen().iter().all(|sample| *sample == 0.0),
            "a paused reply reached the canceller"
        );

        probe.guard.note_resumed();
        probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);
        assert_eq!(
            probe.far_end_seen().last().copied(),
            Some(1.0),
            "the paused reply was consumed while it was held"
        );
    }

    /// A stop while paused must not leave the next reply muted in the
    /// reference.
    #[test]
    fn the_reply_after_a_stop_that_interrupted_a_pause_reaches_the_canceller() {
        let mut probe = Probe::new();
        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));
        probe.guard.note_paused();
        probe.guard.note_stopped();

        probe
            .guard
            .note_rendered(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE));
        probe.guard.clean(&vec![0.0; FRAME_SAMPLES]);

        assert_eq!(probe.far_end_seen().last().copied(), Some(1.0));
    }

    /// Survivors of cargo-mutants on the pause arithmetic (1379-f455).
    mod mutation_1379 {
        use super::*;
        use crate::speaker::Output;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        fn ramp(len: usize) -> SpeechAudio {
            audio((1..=len).map(|n| n as f32).collect(), SAMPLE_RATE)
        }

        /// A pausable device that counts what it was told.
        #[derive(Default)]
        struct Counting {
            pauses: AtomicUsize,
            resumes: AtomicUsize,
        }

        impl Output for Counting {
            fn play(&self, _audio: &SpeechAudio) -> Result<(), String> {
                Ok(())
            }
            fn stop(&self) {}
            fn is_speaking(&self) -> bool {
                false
            }
            fn pause(&self) {
                self.pauses.fetch_add(1, Ordering::Relaxed);
            }
            fn resume(&self) {
                self.resumes.fetch_add(1, Ordering::Relaxed);
            }
            fn can_pause(&self) -> bool {
                true
            }
        }

        /// Catches: `position += 1` turned into `*= 1`, which freezes the
        /// capture position at 0 so no later pause window is ever reached.
        #[test]
        fn take_advances_the_capture_position_across_calls() {
            let mut far_end = FarEnd::new();
            far_end.push(&ramp(20));
            far_end.pauses.push_back((4, Some(7)));

            let first = far_end.take(5);
            let second = far_end.take(5);

            assert_eq!(first, [1.0, 2.0, 3.0, 4.0, 0.0]);
            assert_eq!(second, [0.0, 0.0, 5.0, 6.0, 7.0]);
        }

        /// Catches: `position + recorded` turned into `*`, and `silent_within`
        /// returning 0 or computing its end as `position * count`.
        #[test]
        fn pause_positions_are_measured_from_the_capture_cursor() {
            let mut far_end = FarEnd::new();
            far_end.position = 10;
            far_end.pauses.push_back((8, Some(13)));
            far_end.pauses.push_back((14, Some(16)));
            far_end.pauses.push_back((20, None));

            assert_eq!(far_end.position_after(4), 14);
            // Window [10, 15): three positions of the first pause, one of the
            // second; the open pause is not counted.
            assert_eq!(far_end.silent_within(5), 4);
        }

        /// Catches: `recorded - covered` turned into `+` in `note_rendered`,
        /// which pads twice what a closed pause window already silences.
        #[test]
        fn a_closed_pause_window_is_not_padded_twice() {
            let mut guard = EchoGuard::new(Box::new(PassThrough));
            guard.attach_capture(Box::new(|| 10));
            guard.far_end.pauses.push_back((2, Some(6)));

            guard.note_rendered(&ramp(4));

            // 10 recorded, 4 of them inside the window: pad 6, then the reply.
            assert_eq!(guard.far_end.samples.len(), 6 + 4);
        }

        /// Catches: `FarEndTap::pause` / `resume` doing nothing, and
        /// `can_pause` constant: the device must be told, and the reference
        /// must go silent for a pausable output and come back on resume.
        #[test]
        fn the_tap_forwards_pause_and_resume_and_tracks_them_in_the_reference() {
            let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let guard = Arc::new(parking_lot::Mutex::new(EchoGuard::new(Box::new(Tee(
                Arc::clone(&seen),
            )))));
            let device = Arc::new(Counting::default());
            let tapped = FarEndTap::new(Arc::clone(&device) as _, Arc::clone(&guard));
            assert!(tapped.can_pause());
            assert!(
                !FarEndTap::new(Arc::new(PlayedOutput::default()), Arc::clone(&guard)).can_pause()
            );
            tapped.play(&ramp(SAMPLE_RATE as usize)).expect("played");

            tapped.pause();
            assert_eq!(device.pauses.load(Ordering::Relaxed), 1);
            guard.lock().clean(&vec![0.0; FRAME_SAMPLES]);
            assert!(seen.lock().iter().flatten().all(|sample| *sample == 0.0));

            tapped.resume();
            assert_eq!(device.resumes.load(Ordering::Relaxed), 1);
            guard.lock().clean(&vec![0.0; FRAME_SAMPLES]);
            assert_eq!(
                seen.lock().last().and_then(|frame| frame.first()).copied(),
                Some(1.0),
                "the reference did not resume at the head of the reply"
            );
        }
    }

    /// Probes by the critic of 1376-f33e.
    mod critic_1376 {
        use super::*;
        use crate::speaker::Output;
        use std::sync::Arc;

        /// The browser's output cannot pause: `Output::pause` defaults to a
        /// no-op and the client keeps playing. A tap that mutes the reference
        /// anyway leaves the canceller blind to a reply that is still coming
        /// out of the speaker, and the far end falls a whole pause behind it.
        /// Catches: `FarEndTap::pause` marking the far end paused whatever the
        /// inner output did.
        #[test]
        fn a_pause_the_output_cannot_honour_does_not_mute_the_reference() {
            let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let guard = Arc::new(parking_lot::Mutex::new(EchoGuard::new(Box::new(Tee(
                Arc::clone(&seen),
            )))));
            let device = Arc::new(PlayedOutput::default());
            let tapped = FarEndTap::new(Arc::clone(&device) as _, Arc::clone(&guard));
            tapped
                .play(&audio(vec![1.0; SAMPLE_RATE as usize], SAMPLE_RATE))
                .expect("played");

            tapped.pause();
            guard.lock().clean(&vec![0.0; FRAME_SAMPLES * 3]);

            let far_end: Vec<f32> = seen.lock().iter().flatten().copied().collect();
            assert_eq!(far_end.len(), FRAME_SAMPLES * 3);
            assert!(
                far_end
                    .iter()
                    .all(|sample| sample.to_bits() == 1.0_f32.to_bits()),
                "the reference went silent while the device kept playing"
            );
        }

        /// Capture that was recorded before the pause but not yet cleaned heard
        /// the reply. Matching it against silence throws that much reference
        /// away, and after `resume` the far end replays audio the device has
        /// already played: behind its own echo by the backlog, which
        /// `note_rendered` documents as the one direction that cannot be
        /// cancelled. Catches: `note_paused` taking effect at the cleaned
        /// position instead of the recorded one.
        #[test]
        fn capture_recorded_before_a_pause_is_matched_against_the_reply_it_heard() {
            let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let backlog = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut guard = EchoGuard::new(Box::new(Tee(Arc::clone(&seen))));
            let probe = Arc::clone(&backlog);
            guard.attach_capture(Box::new(move || {
                probe.load(std::sync::atomic::Ordering::Relaxed)
            }));
            let ramp: Vec<f32> = (0..SAMPLE_RATE).map(|n| n as f32 + 1.0).collect();
            guard.note_rendered(&audio(ramp.clone(), SAMPLE_RATE));
            guard.clean(&vec![0.0; FRAME_SAMPLES]);

            // Three frames are recorded and waiting when the speaker is paused.
            backlog.store(FRAME_SAMPLES * 3, std::sync::atomic::Ordering::Relaxed);
            guard.note_paused();
            backlog.store(0, std::sync::atomic::Ordering::Relaxed);
            guard.clean(&vec![0.0; FRAME_SAMPLES * 3]);
            guard.note_resumed();
            guard.clean(&vec![0.0; FRAME_SAMPLES]);

            let far_end: Vec<f32> = seen.lock().iter().flatten().copied().collect();
            assert_eq!(
                far_end[FRAME_SAMPLES..FRAME_SAMPLES * 4],
                ramp[FRAME_SAMPLES..FRAME_SAMPLES * 4],
                "the capture that heard the reply was matched against something else"
            );
            assert_eq!(
                far_end[FRAME_SAMPLES * 4..],
                ramp[FRAME_SAMPLES * 4..FRAME_SAMPLES * 5],
                "after the resume the reference is not where the speaker is"
            );
        }
    }
}
