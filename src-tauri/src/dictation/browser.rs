//! Hands-free for a client that is not this machine.
//!
//! The desktop endpoint opens a `cpal` device and a `rodio` sink. A browser has
//! neither: its microphone and its speaker are on the other side of a
//! WebSocket, several hundred milliseconds away and able to vanish without
//! notice. What does *not* change is where the decisions are made — utterance
//! boundaries, the activation phrase, the hold-back and whether a turn may be
//! delivered all stay in [`continuous`](super::continuous). This module is the
//! transport underneath them and nothing else.
//!
//! ```text
//!   browser mic ──ws──▶ [BrowserLink.capture] ──▶ VoiceEndpoint::drain
//!   browser out ◀──ws── [BrowserLink.playback] ◀── speaker::Output::play
//! ```
//!
//! One [`BrowserLink`] per connected client, keyed by the owner id the client
//! names when it opens the socket. That id is the same string
//! [`arm_hands_free`](super::commands::arm_hands_free) binds, which is what
//! keeps the two halves from being wired to different clients: arming with an
//! owner nobody has connected is refused rather than served by this machine's
//! microphone.
//!
//! ## Why a link outlives its socket
//!
//! The socket closing is the *event* that ends a conversation, but it is not
//! the thing the runtime polls — it polls [`VoiceEndpoint::connected`] once a
//! tick. So the reader task marks the link dead and leaves it; the runtime sees
//! it on the next tick and disarms with `OwnerDisconnected`, which is the same
//! path a dead desktop microphone takes, cancelling the pending transcript and
//! the queued voice entries with it.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::Mutex;

use super::continuous::{Transcript, VoiceEndpoint};
use super::speaker;
use super::speech::SpeechAudio;
use super::transcribe;

/// How much captured audio a link holds before the oldest is dropped.
///
/// Sixty seconds at 16 kHz. The runtime drains every tick, so reaching this
/// means nothing is draining — an armed conversation whose runtime died, or a
/// client that kept streaming after a disarm. Dropping the oldest is right for
/// both: the newest second of a conversation nobody is listening to is worth no
/// more than the first, and an unbounded queue would grow for as long as the
/// tab stays open.
const MAX_CAPTURE_SAMPLES: usize = 16_000 * 60;

/// What the server sends a client that owns a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Downlink {
    /// Play this, now.
    Speak(SpeechAudio),
    /// Stop playing and drop whatever is queued.
    Stop,
    /// Hold what is playing where it is, and anything that arrives, until
    /// [`Resume`](Self::Resume).
    Pause,
    /// Continue from where [`Pause`](Self::Pause) stopped.
    Resume,
}

/// One connected browser client.
///
/// Both directions and the liveness flag in one place, because they share a
/// lifetime: the socket that feeds `capture` is the socket that carries
/// `playback`, and when it goes both stop being true at the same instant.
pub struct BrowserLink {
    capture: Mutex<VecDeque<f32>>,
    playback: tokio::sync::broadcast::Sender<Downlink>,
    alive: AtomicBool,
    /// When the audio handed to the client is expected to have finished.
    ///
    /// A browser cannot be asked "are you still speaking" the way a `rodio`
    /// sink can, and a client that is told to play 3 seconds of audio and then
    /// closes its tab would otherwise leave the reply `Speaking` for ever. The
    /// deadline is the authority; a client that finishes early says so and
    /// moves it to now.
    speaking_until: Mutex<Option<Instant>>,
    /// While the client is told to hold its audio: how much of it is left to
    /// play. The deadline above is wall-clock, which a pause would run out.
    paused_with: Mutex<Option<Duration>>,
}

impl BrowserLink {
    fn new() -> Self {
        let (playback, _) = tokio::sync::broadcast::channel(8);
        Self {
            capture: Mutex::new(VecDeque::new()),
            playback,
            alive: AtomicBool::new(true),
            speaking_until: Mutex::new(None),
            paused_with: Mutex::new(None),
        }
    }

    /// Audio arriving from the client's microphone, already 16 kHz mono.
    ///
    /// Resampling happens in the browser rather than here: the `AudioContext`
    /// the capture already runs in resamples for free, and shipping 48 kHz
    /// across the socket to downsample on this side would triple the bytes for
    /// a result the segmenter throws away.
    pub fn push_capture(&self, samples: &[f32]) {
        let mut buffer = self.capture.lock();
        buffer.extend(samples.iter().copied());
        let overflow = buffer.len().saturating_sub(MAX_CAPTURE_SAMPLES);
        buffer.drain(..overflow);
    }

    /// Everything captured since the last call.
    pub fn drain_capture(&self) -> Vec<f32> {
        self.capture.lock().drain(..).collect()
    }

    /// How much captured audio is waiting for the next [`drain_capture`](Self::drain_capture).
    pub fn pending_capture(&self) -> usize {
        self.capture.lock().len()
    }

    /// Watch what the server wants played.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Downlink> {
        self.playback.subscribe()
    }

    /// The client reported that it finished playing.
    pub fn note_playback_ended(&self) {
        *self.speaking_until.lock() = None;
        // Nothing left to hold: a reply that ended while paused cannot happen
        // on a client that honours the pause, and one that does not has
        // finished.
        *self.paused_with.lock() = None;
    }

    /// Is the client still connected?
    pub fn connected(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// The socket is gone.
    pub fn disconnect(&self) {
        self.alive.store(false, Ordering::Release);
        *self.speaking_until.lock() = None;
    }
}

/// Every browser client currently holding an audio socket.
///
/// A `DashMap` rather than a `Mutex<HashMap>` because the two sides that touch
/// it never touch the same key: a socket task registers and removes its own
/// owner, and the arming path looks one up.
#[derive(Default)]
pub struct BrowserEndpoints {
    links: DashMap<String, Arc<BrowserLink>>,
}

impl BrowserEndpoints {
    /// Register a client under `owner`, replacing any previous one.
    ///
    /// Replacing rather than refusing, because the previous one is by
    /// definition a socket this server has not noticed closing yet — a reload
    /// of the same tab is the ordinary case. The old link is disconnected
    /// first, so a conversation still armed against it disarms itself instead
    /// of quietly moving to the new socket: the new client has to arm.
    pub fn connect(&self, owner: &str) -> Arc<BrowserLink> {
        let link = Arc::new(BrowserLink::new());
        if let Some(previous) = self.links.insert(owner.to_string(), link.clone()) {
            previous.disconnect();
        }
        link
    }

    /// The socket for `owner` closed.
    ///
    /// Takes the link it is closing so a socket that has already been replaced
    /// cannot evict its successor: the guard is identity, not the key.
    pub fn disconnect(&self, owner: &str, link: &Arc<BrowserLink>) {
        link.disconnect();
        self.links
            .remove_if(owner, |_, current| Arc::ptr_eq(current, link));
    }

    /// The link for `owner`, if a client is connected under it.
    pub fn get(&self, owner: &str) -> Option<Arc<BrowserLink>> {
        self.links.get(owner).map(|entry| entry.clone())
    }
}

/// Capture and recognition for a conversation a browser owns.
pub struct BrowserVoiceEndpoint {
    link: Arc<BrowserLink>,
    transcriber: Arc<dyn transcribe::Transcriber>,
    language: Option<String>,
    gates: transcribe::VoiceGates,
}

impl BrowserVoiceEndpoint {
    pub fn new(
        link: Arc<BrowserLink>,
        transcriber: Arc<dyn transcribe::Transcriber>,
        language: Option<String>,
        gates: transcribe::VoiceGates,
    ) -> Self {
        Self {
            link,
            transcriber,
            language,
            gates,
        }
    }
}

impl VoiceEndpoint for BrowserVoiceEndpoint {
    fn drain(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.link.drain_capture())
    }

    fn connected(&self) -> bool {
        self.link.connected()
    }

    fn transcribe(&self, audio: &[f32]) -> Result<Transcript, String> {
        super::commands::transcribe_utterance(
            self.transcriber.as_ref(),
            audio,
            self.language.as_deref(),
            self.gates,
        )
    }
}

/// Replies for a conversation a browser owns.
///
/// The audio never reaches this machine's speakers, which is the whole point:
/// a user talking to TUICommander from a laptop must not have the answer come
/// out of the server in another room.
pub struct BrowserOutput {
    link: Arc<BrowserLink>,
}

impl BrowserOutput {
    pub fn new(link: Arc<BrowserLink>) -> Self {
        Self { link }
    }
}

impl speaker::Output for BrowserOutput {
    fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
        let duration = Duration::from_secs_f32(audio.duration_seconds());
        // Armed before the send, so a client that answers instantly cannot have
        // its "I finished" overwritten by the deadline it beat. While paused
        // the client holds the audio instead of playing it, so what is armed
        // is what is left to play, not a deadline.
        {
            let mut paused = self.link.paused_with.lock();
            if paused.is_some() {
                *paused = Some(duration);
            } else {
                *self.link.speaking_until.lock() = Some(Instant::now() + duration);
            }
        }
        if self
            .link
            .playback
            .send(Downlink::Speak(audio.clone()))
            .is_err()
        {
            *self.link.speaking_until.lock() = None;
            *self.link.paused_with.lock() = None;
            return Err("the browser client that owns this conversation is gone".to_string());
        }
        Ok(())
    }

    fn pause(&self) {
        let mut paused = self.link.paused_with.lock();
        if paused.is_none() {
            let left = self
                .link
                .speaking_until
                .lock()
                .take()
                .map_or(Duration::ZERO, |deadline| {
                    deadline.saturating_duration_since(Instant::now())
                });
            *paused = Some(left);
        }
        let _ = self.link.playback.send(Downlink::Pause);
    }

    fn resume(&self) {
        if let Some(left) = self.link.paused_with.lock().take() {
            *self.link.speaking_until.lock() = (!left.is_zero()).then(|| Instant::now() + left);
        }
        let _ = self.link.playback.send(Downlink::Resume);
    }

    fn can_pause(&self) -> bool {
        true
    }

    fn stop(&self) {
        *self.link.speaking_until.lock() = None;
        *self.link.paused_with.lock() = None;
        // A closed receiver means there is nobody left to stop, which is the
        // outcome asked for. `hush` runs on the capture loop and must not fail.
        let _ = self.link.playback.send(Downlink::Stop);
    }

    fn is_speaking(&self) -> bool {
        if !self.link.connected() {
            return false;
        }
        if self
            .link
            .paused_with
            .lock()
            .is_some_and(|left| !left.is_zero())
        {
            return true;
        }
        self.link
            .speaking_until
            .lock()
            .is_some_and(|deadline| Instant::now() < deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed browser socket ends the conversation it owned through the
    /// same runtime path as a released desktop microphone.
    #[test]
    fn a_browser_client_that_closed_its_socket_disarms_the_conversation_it_owned() {
        use super::super::{continuous, echo};

        struct AcceptingTarget;
        impl continuous::TargetProbe for AcceptingTarget {
            fn accepts(&self, _session_id: &str) -> bool {
                true
            }
        }

        struct UnusedSink;
        impl continuous::VoiceSink for UnusedSink {
            fn write(
                &self,
                _session_id: &str,
                _text: &str,
            ) -> Result<continuous::VoiceWrite, String> {
                panic!("no transcript is expected before the browser disconnects")
            }
        }

        let endpoints = BrowserEndpoints::default();
        let link = endpoints.connect("browser-42");
        let mut endpoint = BrowserVoiceEndpoint::new(
            link.clone(),
            Arc::new(SilentTranscriber),
            None,
            transcribe::VoiceGates::default(),
        );
        let mut hands_free = continuous::HandsFree::new(1_000);
        hands_free.arm("target", "browser-42", true).expect("arm");
        let mode = Mutex::new(hands_free);
        let echo = Arc::new(Mutex::new(echo::EchoGuard::new(Box::new(
            echo::PassThrough,
        ))));
        let mut capture = continuous::Capture::new(
            continuous::SegmenterConfig::default(),
            continuous::DEVICE_SILENCE_TIMEOUT_MS,
            0,
            echo,
            None,
        );

        assert!(matches!(
            continuous::tick(
                &mut capture,
                &mode,
                &mut endpoint,
                &AcceptingTarget,
                &UnusedSink,
                0
            ),
            continuous::Tick::Running { .. }
        ));
        endpoints.disconnect("browser-42", &link);

        match continuous::tick(
            &mut capture,
            &mode,
            &mut endpoint,
            &AcceptingTarget,
            &UnusedSink,
            0,
        ) {
            continuous::Tick::Disarmed(disarmed) => {
                assert_eq!(disarmed.reason, continuous::DisarmReason::OwnerDisconnected);
            }
            other => panic!("a closed browser socket must disarm, got {other:?}"),
        }
        assert!(mode.lock().binding().is_none());
    }

    /// Recognises nothing. The recogniser is not what any test here is about —
    /// what matters is that a browser endpoint reaches one at all.
    struct SilentTranscriber;

    impl transcribe::Transcriber for SilentTranscriber {
        fn transcribe(
            &self,
            _audio: &[f32],
            _language: Option<&str>,
            _gates: transcribe::VoiceGates,
        ) -> Result<transcribe::TranscribeResult, String> {
            Ok(transcribe::TranscribeResult {
                text: String::new(),
                skip_reason: None,
                language: None,
            })
        }
    }

    fn audio(seconds: f32) -> SpeechAudio {
        SpeechAudio {
            samples: vec![0.0; (24_000.0 * seconds) as usize],
            sample_rate: 24_000,
        }
    }

    /// The registry is the thing that keeps one client's microphone from
    /// feeding another client's conversation, so look up by the wrong name and
    /// there must be nothing there — not the desktop, not the other tab.
    #[test]
    fn a_link_is_reachable_only_under_the_owner_that_registered_it() {
        let endpoints = BrowserEndpoints::default();
        let link = endpoints.connect("browser-a");
        assert!(endpoints.get("browser-b").is_none());
        assert!(Arc::ptr_eq(
            &endpoints.get("browser-a").expect("registered"),
            &link
        ));
    }

    /// A reload opens the new socket before the old one finishes closing. The
    /// new client must win the name, and the conversation the old one armed
    /// must end rather than silently continue against a tab that is gone.
    #[test]
    fn reconnecting_under_the_same_owner_kills_the_conversation_the_old_socket_held() {
        let endpoints = BrowserEndpoints::default();
        let old = endpoints.connect("browser-a");
        let new = endpoints.connect("browser-a");

        assert!(!old.connected(), "the replaced socket is dead");
        assert!(new.connected(), "the replacement is live");
        assert!(Arc::ptr_eq(
            &endpoints.get("browser-a").expect("registered"),
            &new
        ));
    }

    /// The close handler of the *old* socket runs after the new one registered.
    /// Keyed on the name alone it would evict the live client; keyed on
    /// identity it does nothing, which is what must happen.
    #[test]
    fn a_late_close_from_a_replaced_socket_does_not_evict_its_successor() {
        let endpoints = BrowserEndpoints::default();
        let old = endpoints.connect("browser-a");
        let new = endpoints.connect("browser-a");

        endpoints.disconnect("browser-a", &old);

        assert!(
            endpoints
                .get("browser-a")
                .is_some_and(|current| Arc::ptr_eq(&current, &new)),
            "the live socket keeps the name"
        );
        assert!(new.connected());
    }

    /// Capture is a queue the runtime empties, and `drain` must hand over
    /// everything exactly once — a second call that returned the same audio
    /// would make the segmenter hear every utterance twice.
    #[test]
    fn captured_audio_is_handed_over_once_and_in_order() {
        let link = BrowserLink::new();
        link.push_capture(&[0.1, 0.2]);
        link.push_capture(&[0.3]);

        assert_eq!(link.drain_capture(), vec![0.1, 0.2, 0.3]);
        assert!(link.drain_capture().is_empty(), "drained means drained");
    }

    /// Nothing drains a conversation that is not armed, so the buffer must be
    /// bounded by something other than the runtime's good behaviour.
    #[test]
    fn capture_that_nobody_drains_keeps_the_newest_audio_and_not_the_oldest() {
        let link = BrowserLink::new();
        link.push_capture(&vec![0.5; MAX_CAPTURE_SAMPLES]);
        link.push_capture(&[0.9, 0.9]);

        let held = link.drain_capture();
        assert_eq!(held.len(), MAX_CAPTURE_SAMPLES, "the bound holds");
        assert_eq!(
            &held[held.len() - 2..],
            &[0.9, 0.9],
            "the newest audio survived"
        );
    }

    /// The reply has to leave the machine rather than come out of it, and the
    /// only proof available here is that the client was told.
    #[test]
    fn a_reply_is_sent_to_the_client_rather_than_played_locally() {
        use speaker::Output;

        let link = Arc::new(BrowserLink::new());
        let mut client = link.subscribe();
        let output = BrowserOutput::new(link.clone());

        output
            .play(&audio(0.5))
            .expect("a connected client takes it");

        assert!(matches!(
            client.try_recv().expect("the client was told"),
            Downlink::Speak(_)
        ));
        assert!(output.is_speaking(), "half a second has not passed");

        output.stop();
        assert_eq!(
            client.try_recv().expect("a stop reached it"),
            Downlink::Stop
        );
        assert!(!output.is_speaking(), "a stopped reply is not speaking");
    }

    /// A paused reply is still being spoken, and its time does not run out
    /// while it is held. Catches: the wall-clock deadline expiring during a
    /// pause, which finishes a reply the user has not heard.
    #[test]
    fn a_paused_reply_keeps_what_is_left_of_it_and_continues_from_there() {
        use speaker::Output;

        let link = Arc::new(BrowserLink::new());
        let mut client = link.subscribe();
        let output = BrowserOutput::new(link.clone());
        output.play(&audio(30.0)).expect("sent");
        client.try_recv().expect("the reply");

        output.pause();
        assert_eq!(client.try_recv().expect("told to hold"), Downlink::Pause);
        *link.speaking_until.lock() = None;
        assert!(output.is_speaking(), "a held reply is not finished");

        output.resume();
        assert_eq!(client.try_recv().expect("told to go on"), Downlink::Resume);
        assert!(output.is_speaking(), "the rest of it is still to play");
        assert!(output.can_pause());

        output.pause();
        output.stop();
        assert!(!output.is_speaking(), "a stop ends a held reply too");
    }

    /// `Finished` is set when the device reports nothing left, so a browser
    /// that never reports would strand every reply. The two ways it can end —
    /// the client saying so, and the client disappearing — both have to close
    /// it, because the second one produces no message at all.
    #[test]
    fn a_reply_stops_speaking_when_the_client_says_so_or_when_it_vanishes() {
        use speaker::Output;

        let link = Arc::new(BrowserLink::new());
        let _client = link.subscribe();
        let output = BrowserOutput::new(link.clone());

        output.play(&audio(30.0)).expect("sent");
        assert!(output.is_speaking());
        link.note_playback_ended();
        assert!(!output.is_speaking(), "the client finished early");

        output.play(&audio(30.0)).expect("sent");
        assert!(output.is_speaking());
        link.disconnect();
        assert!(!output.is_speaking(), "a tab that closed is not speaking");
    }

    /// A reply queued for a client that has already gone must fail rather than
    /// be reported spoken: `Failed` is surfaced, and a false `Finished` is the
    /// voice assistant claiming it said something nobody heard.
    #[test]
    fn a_reply_for_a_client_that_left_is_a_failure_not_a_silent_success() {
        use speaker::Output;

        let link = Arc::new(BrowserLink::new());
        let output = BrowserOutput::new(link.clone());
        // No subscriber: every receiver is gone, which is what a closed socket
        // leaves behind once its task has returned.
        let error = output.play(&audio(1.0)).expect_err("nobody can play it");

        assert!(
            error.contains("gone"),
            "the reason names the missing client"
        );
        assert!(!output.is_speaking());
    }

    /// The runtime polls `connected` once a tick and disarms on false. That is
    /// the only channel a closed socket has, so the endpoint must report it.
    #[test]
    fn the_endpoint_reports_a_closed_socket_as_a_lost_owner() {
        let endpoints = BrowserEndpoints::default();
        let link = endpoints.connect("browser-a");
        let mut endpoint = BrowserVoiceEndpoint::new(
            link.clone(),
            Arc::new(SilentTranscriber),
            None,
            transcribe::VoiceGates::default(),
        );

        assert!(endpoint.connected());
        assert_eq!(
            endpoint.drain().expect("no capture failure"),
            [] as [f32; 0]
        );

        endpoints.disconnect("browser-a", &link);
        assert!(!endpoint.connected());
    }
}
