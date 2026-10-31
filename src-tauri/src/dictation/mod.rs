pub use tuic_dictation::audio;
pub(crate) mod adapters;
mod asset_download;
pub mod browser;
pub mod commands;
mod edge_voices;
pub use tuic_dictation::continuous;
pub use tuic_dictation::corrections;
pub use tuic_dictation::echo;
pub mod fn_key_monitor;
#[allow(unused_imports)] // Keep the old module path for callers outside this adapter.
pub use tuic_dictation::language;
pub use tuic_dictation::loudness;
pub use tuic_dictation::model;
mod model_download;
pub mod ownership;
pub use tuic_dictation::permission;
pub use tuic_dictation::speaker;
pub use tuic_dictation::speech;
pub use tuic_dictation::streaming;
pub use tuic_dictation::transcribe;
#[allow(unused_imports)] // Keep the old module path for callers outside this adapter.
pub use tuic_dictation::vad;

use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;
use std::time::Instant;

pub struct CaptureStopRequest {
    pub at: Instant,
    pub source: &'static str,
}

/// Seconds between idle-unload sweeps.
const IDLE_UNLOAD_SWEEP_SECS: u64 = 30;

/// Frees the Whisper model when it has been idle; see
/// [`DictationState::unload_idle_transcriber`].
pub fn spawn_idle_unload_sweeper(app: tauri::AppHandle) {
    use tauri::Manager;
    std::thread::Builder::new()
        .name("dictation-idle-unload".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(IDLE_UNLOAD_SWEEP_SECS));
                if let Some(dictation) = app.try_state::<DictationState>() {
                    dictation.idle_unload_tick(Instant::now());
                }
            }
        })
        .expect("spawn dictation-idle-unload");
}

/// Shared dictation state accessible from Tauri commands.
/// Tauri's `.manage()` wraps this in `Arc` internally, so we don't double-wrap.
pub struct DictationState {
    pub audio: Mutex<Option<audio::AudioCapture>>,
    /// Name of the model currently loaded in `transcriber_arc` (e.g. "large-v3-turbo")
    pub active_model: Mutex<Option<String>>,
    pub corrections: Arc<Mutex<corrections::TextCorrector>>,
    pub recording: AtomicBool,
    /// Native Fn state survives a blocked or reloaded WebView.
    pub fn_down: AtomicBool,
    #[cfg(target_os = "macos")]
    pub fn_release_watch_active: AtomicBool,
    pub fn_capture: AtomicBool,
    pub native_release_pending: AtomicBool,
    pub stop_request: Mutex<Option<CaptureStopRequest>>,
    pub processing: Arc<AtomicBool>,
    /// Active streaming session (None when not streaming).
    pub streaming: Mutex<Option<streaming::StreamingSession>>,
    /// Arc-wrapped transcriber for sharing with the streaming thread.
    pub transcriber_arc: Mutex<Option<Arc<dyn transcribe::Transcriber>>>,
    /// Last moment the transcriber was loaded or seen held by a user (streaming
    /// thread, hands-free endpoint). The idle unload counts from here.
    transcriber_last_seen: Mutex<Instant>,
    /// Concatenation of all streaming partials (for accuracy comparison logging).
    pub accumulated_partials: Arc<Mutex<String>>,
    /// Hands-free mode. Separate from `recording` on purpose: push-to-talk and
    /// hands-free are different modes and neither arms the other.
    ///
    /// `Arc` because the runtime thread drives it between ticks.
    pub hands_free: Arc<Mutex<continuous::HandsFree>>,
    /// The thread driving hands-free capture while it is armed.
    pub hands_free_runtime: Mutex<Option<continuous::HandsFreeRuntime>>,
    /// The microphone hands-free captures from. Deliberately **not** `audio`:
    /// that one belongs to push-to-talk, and a shared slot would let either
    /// mode close the other's device.
    ///
    /// It stays on this side of the thread boundary because a `cpal::Stream` is
    /// `!Send` — the runtime only ever sees the buffer handle.
    pub hands_free_audio: Mutex<Option<audio::AudioCapture>>,
    /// Liveness of the desktop audio endpoint. Cleared when that endpoint is
    /// released, which is how the runtime learns its owner is gone.
    pub hands_free_owner_alive: Arc<AtomicBool>,
    /// Acoustic echo cancellation, shared by the two threads that must agree
    /// about it: the hands-free runtime cleans capture with it, and the reply
    /// queue's [`echo::FarEndTap`] tells it what is being played.
    ///
    /// One instance for the life of the app rather than one per arm. The
    /// canceller's delay estimator is state worth keeping — rebuilding it every
    /// time the mode is armed would make the first second of every session the
    /// worst one.
    pub echo: Arc<Mutex<echo::EchoGuard>>,
    /// The downloadable speech assets, and the engines loaded from them.
    ///
    /// One for the app rather than one per reply: it is what serialises
    /// replacing a language against speaking it, which only works if both
    /// sides go through the same instance.
    pub speech: Arc<speech::library::SpeechLibrary>,
    /// The reply queue for the armed conversation.
    ///
    /// `None` whenever hands-free is not armed, and that is the answer the
    /// voice capability gives a model that calls it then: speech belongs to a
    /// conversation, not to the application. Built on arm because it needs a
    /// loaded engine and an open audio device, both of which are worth holding
    /// only while somebody is listening.
    ///
    /// `Arc` because the hands-free capture loop holds the same slot as its
    /// barge-in port: it must interrupt whatever is speaking *now*, which
    /// under Auto is a voice that did not exist when the loop started.
    pub speaker: Arc<Mutex<Option<speaker::Armed>>>,
    /// The device a voice preview is playing on, held so the sound lasts past
    /// the call that started it. A conversation reply stops it before it is
    /// queued, so the two never play over each other. Always taken after
    /// `speaker`, never before.
    pub preview: Mutex<Option<Arc<dyn speaker::Output>>>,
    /// Where an utterance transition goes once a conversation is armed.
    ///
    /// Held here rather than passed in because the two places that build a
    /// reply queue — arming, and the first `speak` under Auto — have no
    /// `AppHandle` between them. Installed once at startup by the one caller
    /// that does. `None` in a test and in the headless build: a speaker nobody
    /// watches is the ordinary case for a unit test, not a misconfiguration.
    pub utterance_observer: Mutex<Option<Arc<dyn speaker::UtteranceObserver>>>,
    /// Browser and remote clients holding an audio socket, by owner id.
    ///
    /// Lives here rather than in `AppState` because it is only ever reached
    /// from the two dictation paths that already take this state: the socket
    /// handler that registers a client, and arming, which looks one up. Empty
    /// on a desktop-only install, which is the ordinary case.
    pub browser_endpoints: Arc<browser::BrowserEndpoints>,
    /// Whether this instance owns dictation. Unset until startup claims it, so
    /// a test or the headless build, which never claim, behave as the owner.
    ownership: std::sync::OnceLock<ownership::Ownership>,
}

impl DictationState {
    /// Claim dictation for this instance's config directory, once, at startup.
    /// Keeps the lock for the life of the state. Logs the outcome so a second
    /// instance leaves evidence of who owned what.
    pub fn claim_ownership(&self, config_dir: &std::path::Path) {
        let claimed = self
            .ownership
            .get_or_init(|| ownership::Ownership::acquire(config_dir));
        tracing::info!(
            source = "dictation",
            owner = claimed.is_owner(),
            lock = %claimed.path().display(),
            "Dictation ownership: {}",
            if claimed.is_owner() { "owner" } else { "non-owner, another instance holds the lock" }
        );
    }

    /// True unless another instance holds this config directory's dictation lock.
    pub fn is_owner(&self) -> bool {
        self.ownership
            .get()
            .is_none_or(ownership::Ownership::is_owner)
    }

    /// `Err` with the user-facing refusal when another instance owns dictation.
    pub fn ensure_owner(&self) -> Result<(), String> {
        if self.is_owner() {
            Ok(())
        } else {
            Err(ownership::OWNED_ELSEWHERE.to_string())
        }
    }

    /// Stop microphone capture at the native event edge, before WebView delivery.
    /// The IPC stop later joins streaming and transcribes the retained audio.
    #[cfg(target_os = "macos")]
    pub fn request_native_stop(&self, source: &'static str) {
        if !self.recording.load(Ordering::Acquire) || !self.fn_capture.load(Ordering::Acquire) {
            return;
        }
        self.native_release_pending.store(true, Ordering::Release);
        let requested = Instant::now();
        *self.stop_request.lock() = Some(CaptureStopRequest {
            at: requested,
            source,
        });
        let mut audio = self.audio.lock();
        if let Some(capture) = audio.as_mut() {
            capture.stop_stream();
            tracing::info!(
                source = "dictation",
                trigger = source,
                latency_ms = requested.elapsed().as_millis(),
                "Native capture stop executed"
            );
        } else {
            tracing::info!(
                source = "dictation",
                trigger = source,
                "Native capture stop pending microphone start"
            );
        }
    }

    pub fn new() -> Self {
        Self {
            audio: Mutex::new(None),
            active_model: Mutex::new(None),
            corrections: Arc::new(Mutex::new(corrections::TextCorrector::load_or_default())),
            recording: AtomicBool::new(false),
            fn_down: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            fn_release_watch_active: AtomicBool::new(false),
            fn_capture: AtomicBool::new(false),
            native_release_pending: AtomicBool::new(false),
            stop_request: Mutex::new(None),
            processing: Arc::new(AtomicBool::new(false)),
            streaming: Mutex::new(None),
            transcriber_arc: Mutex::new(None),
            transcriber_last_seen: Mutex::new(Instant::now()),
            accumulated_partials: Arc::new(Mutex::new(String::new())),
            hands_free: Arc::new(Mutex::new(continuous::HandsFree::new(
                commands::default_hold_back_ms().into(),
            ))),
            hands_free_runtime: Mutex::new(None),
            hands_free_audio: Mutex::new(None),
            hands_free_owner_alive: Arc::new(AtomicBool::new(false)),
            echo: Arc::new(Mutex::new(echo::install())),
            speech: Arc::new(speech::library::SpeechLibrary::new()),
            speaker: Arc::new(Mutex::new(None)),
            preview: Mutex::new(None),
            utterance_observer: Mutex::new(None),
            browser_endpoints: Arc::new(browser::BrowserEndpoints::default()),
            ownership: std::sync::OnceLock::new(),
        }
    }

    /// Store a freshly loaded transcriber, with both slots already locked by the
    /// caller (`ensure_transcriber` holds them across the load).
    pub(crate) fn install_transcriber_locked(
        &self,
        transcriber_slot: &mut Option<Arc<dyn transcribe::Transcriber>>,
        model_slot: &mut Option<String>,
        transcriber: Arc<dyn transcribe::Transcriber>,
        model_name: &str,
    ) {
        *transcriber_slot = Some(transcriber);
        *model_slot = Some(model_name.to_string());
        *self.transcriber_last_seen.lock() = Instant::now();
    }

    /// Release the transcriber when nothing has used it for `idle`.
    /// Returns whether it was released.
    ///
    /// "Used" means some other holder has a clone (the streaming thread, a
    /// hands-free or browser endpoint): a sweep that sees one restarts the idle
    /// clock, so the period counts from the last time the model was seen in use.
    /// Locks in the order `ensure_transcriber` takes them.
    pub(crate) fn unload_idle_transcriber(&self, now: Instant, idle: std::time::Duration) -> bool {
        let mut slot = self.transcriber_arc.lock();
        let Some(transcriber) = slot.as_ref() else {
            return false;
        };
        let mut last_seen = self.transcriber_last_seen.lock();
        if Arc::strong_count(transcriber) > 1 {
            *last_seen = now;
            return false;
        }
        if now.saturating_duration_since(*last_seen) < idle {
            return false;
        }
        drop(last_seen);
        *slot = None;
        *self.active_model.lock() = None;
        true
    }

    /// One sweep with the configured period (`0` keeps the model loaded).
    /// Reads the config only while a model is loaded, so an idle app pays no
    /// file read.
    pub(crate) fn idle_unload_tick(&self, now: Instant) {
        if self.transcriber_arc.lock().is_none() {
            return;
        }
        let minutes = commands::get_dictation_config().model_idle_unload_minutes;
        if minutes == 0 {
            return;
        }
        let idle = std::time::Duration::from_secs(u64::from(minutes) * 60);
        if self.unload_idle_transcriber(now, idle) {
            tracing::info!(
                source = "dictation",
                idle_minutes = minutes,
                "Whisper model unloaded after idle period"
            );
        }
    }

    /// Clean shutdown: stop the streaming thread, release audio, then drop the
    /// transcriber. Order matters — the streaming thread holds an Arc clone of
    /// the transcriber, so we must join it before the WhisperContext can be freed.
    pub fn shutdown(&self) {
        // 0. Release the desktop audio endpoint and disarm hands-free.
        //    The disarm is done here rather than left to the runtime: dropping
        //    the runtime stops its thread, so racing it for the owner flag
        //    would leave the mode armed on some runs and not others.
        self.hands_free_owner_alive
            .store(false, std::sync::atomic::Ordering::Release);
        self.hands_free
            .lock()
            .disarm(continuous::DisarmReason::OwnerDisconnected);
        *self.hands_free_runtime.lock() = None;
        *self.hands_free_audio.lock() = None;
        // 0b. Stop talking. Before the engine goes away below: dropping the
        //     queue cancels the reply in flight, and an `ort::Session` freed
        //     under a thread still rendering from it is undefined rather than
        //     merely abrupt.
        *self.speaker.lock() = None;
        // 1. Stop audio capture (upstream source)
        *self.audio.lock() = None;
        // 2. Stop + join the streaming thread (Drop impl signals stop flag)
        //    This releases the thread's Arc<dyn Transcriber> clone.
        *self.streaming.lock() = None;
        // 3. Now safe to drop the transcriber — no other Arc holders remain.
        *self.transcriber_arc.lock() = None;
        // 4. Let go of the speech graphs. Separate from the transcriber and
        //    last because it waits for any synthesis in flight: a reply being
        //    spoken as the app closes finishes its sentence rather than being
        //    cut off mid-word by a freed ONNX session.
        self.speech.shutdown();
    }
}

#[cfg(test)]
mod idle_unload_tests {
    use super::*;
    use std::sync::Weak;
    use std::time::Duration;

    const IDLE: Duration = Duration::from_secs(300);

    struct Silent;

    impl transcribe::Transcriber for Silent {
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

    /// Loads a fake model the way `ensure_transcriber` does and returns a probe
    /// that reads whether the model's memory is still alive: the observable
    /// effect of the unload, independent of any slot the state keeps.
    fn loaded(state: &DictationState) -> Weak<dyn transcribe::Transcriber> {
        let model: Arc<dyn transcribe::Transcriber> = Arc::new(Silent);
        let probe = Arc::downgrade(&model);
        let mut slot = state.transcriber_arc.lock();
        let mut name = state.active_model.lock();
        state.install_transcriber_locked(&mut slot, &mut name, model, "large-v3-turbo");
        probe
    }

    #[test]
    fn a_model_nobody_uses_is_freed_once_the_idle_period_has_passed() {
        let state = DictationState::new();
        let t0 = Instant::now();
        let probe = loaded(&state);

        assert!(!state.unload_idle_transcriber(t0 + IDLE - Duration::from_secs(1), IDLE));
        assert!(probe.upgrade().is_some(), "still inside the idle period");

        assert!(state.unload_idle_transcriber(Instant::now() + IDLE, IDLE));
        assert!(probe.upgrade().is_none(), "the model memory must be freed");
        assert_eq!(
            *state.active_model.lock(),
            None,
            "the next dictation must see no loaded model and reload it"
        );
    }

    #[test]
    fn a_model_held_by_a_running_session_is_never_freed_and_idle_counts_from_release() {
        let state = DictationState::new();
        let probe = loaded(&state);
        // The streaming thread / hands-free endpoint holds its own clone.
        let held = state.transcriber_arc.lock().clone().unwrap();
        let far = Instant::now() + IDLE * 10;

        assert!(!state.unload_idle_transcriber(far, IDLE), "in use");
        assert!(probe.upgrade().is_some());

        drop(held);
        // Released just now (as of `far`): a full idle period must pass again.
        assert!(!state.unload_idle_transcriber(far + IDLE - Duration::from_secs(1), IDLE));
        assert!(probe.upgrade().is_some());
        assert!(state.unload_idle_transcriber(far + IDLE, IDLE));
        assert!(probe.upgrade().is_none());
    }

    #[test]
    fn sweeping_with_nothing_loaded_does_nothing() {
        let state = DictationState::new();
        assert!(!state.unload_idle_transcriber(Instant::now() + IDLE * 10, IDLE));
    }

    #[test]
    fn a_model_can_be_loaded_again_after_it_was_freed() {
        let state = DictationState::new();
        let first = loaded(&state);
        assert!(state.unload_idle_transcriber(Instant::now() + IDLE, IDLE));
        assert!(first.upgrade().is_none());

        let second = loaded(&state);
        assert!(second.upgrade().is_some());
        assert_eq!(state.active_model.lock().as_deref(), Some("large-v3-turbo"));
    }

    #[test]
    fn the_sweep_reads_the_configured_period_and_zero_keeps_the_model() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let write = |minutes: u32| {
            std::fs::write(
                dir.path().join("dictation-config.json"),
                format!(r#"{{"model_idle_unload_minutes": {minutes}}}"#),
            )
            .unwrap();
        };
        let state = DictationState::new();
        let probe = loaded(&state);
        let after_an_hour = Instant::now() + Duration::from_secs(3600);

        write(0);
        state.idle_unload_tick(after_an_hour);
        assert!(probe.upgrade().is_some(), "0 means never unload");

        write(90);
        state.idle_unload_tick(after_an_hour);
        assert!(probe.upgrade().is_some(), "an hour is inside 90 minutes");

        write(1);
        state.idle_unload_tick(after_an_hour);
        assert!(probe.upgrade().is_none(), "an hour is past 1 minute");
    }
}
