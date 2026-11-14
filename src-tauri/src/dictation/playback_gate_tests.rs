// These are state/queue regressions, not acoustic echo tests.
struct GateTestSpeech(std::sync::mpsc::Sender<()>);
impl speech::Speech for GateTestSpeech {
    fn synthesize(
        &self,
        _: &str,
        _: &str,
        _: &speech::SpeechCancel,
    ) -> Result<speech::SpeechAudio, speech::SpeechError> {
        self.0.send(()).unwrap();
        Ok(speech::SpeechAudio {
            samples: vec![0.1; 160],
            sample_rate: 16_000,
        })
    }
}
struct GateTestOutput(
    std::sync::mpsc::Sender<()>,
    Option<Arc<dyn speaker::Output>>,
);
impl speaker::Output for GateTestOutput {
    fn play(&self, audio: &speech::SpeechAudio) -> Result<(), String> {
        if let Some(inner) = &self.1 {
            inner.play(audio)?;
        }
        self.0.send(()).unwrap();
        Ok(())
    }
    fn stop(&self) {}
    fn is_speaking(&self) -> bool {
        false
    }
}
struct GateTestEndpoint(Vec<f32>);
impl continuous::VoiceEndpoint for GateTestEndpoint {
    fn drain(&mut self) -> Result<Vec<f32>, String> {
        Ok(std::mem::take(&mut self.0))
    }
    fn connected(&self) -> bool {
        true
    }
    fn transcribe(&self, _: &[f32]) -> Result<continuous::Transcript, String> {
        Ok(continuous::Transcript {
            text: "a complete turn".into(),
            language: Some("it".into()),
        })
    }
}
struct GateTestTarget;
impl continuous::TargetProbe for GateTestTarget {
    fn accepts(&self, _: &str) -> bool {
        true
    }
}
struct GateTestSink<'a>(&'a std::sync::mpsc::Receiver<()>);
impl continuous::VoiceSink for GateTestSink<'_> {
    fn write(&self, _: &str, _: &str) -> Result<continuous::VoiceWrite, String> {
        assert!(
            self.0
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err(),
            "playback started before submit completed"
        );
        Ok(continuous::VoiceWrite::Written)
    }
}

/// Catches: a synthesized reply reaches output while the segmenter is active,
/// during hold-back, or before the submit call returns.
fn playback_waits_case(browser_owner: bool, cancel: bool, mixed_batch: bool) {
    let (dictation, _engine_gate, _config) = armed_with_a_voice("session-a");
    let (rendered_tx, rendered) = std::sync::mpsc::channel();
    let (played_tx, played) = std::sync::mpsc::channel();
    if mixed_batch {
        let mut mode = continuous::HandsFree::new(0);
        mode.arm("session-a", "desktop", true).unwrap();
        *dictation.hands_free.lock() = mode;
    }
    let generation = dictation.hands_free.lock().generation();
    let browser = browser_owner.then(|| {
        let link = dictation.browser_endpoints.connect("gate-browser");
        let output = open_reply_output(&dictation, Some("gate-browser")).unwrap();
        let receiver = link.subscribe();
        (link, output, receiver)
    });
    let output = Arc::new(GateTestOutput(
        played_tx,
        browser.as_ref().map(|(_, output, _)| output.clone()),
    ));
    dictation.speaker.lock().as_mut().unwrap().speaker = Arc::new(speaker::Speaker::new(
        Arc::new(GateTestSpeech(rendered_tx)),
        speech_far_end(output, &dictation),
        generation,
    ));
    let echo = Arc::new(parking_lot::Mutex::new(echo::EchoGuard::new(Box::new(
        echo::PassThrough,
    ))));
    // No interruption port: this isolates admission to output from the separately
    // covered policy that cancels old replies on an accepted barge-in.
    let mut capture = continuous::Capture::new(
        continuous::SegmenterConfig::default(),
        60_000,
        0,
        echo,
        None,
    );
    let mut endpoint = GateTestEndpoint(vec![0.1; 3_200]);
    if mixed_batch {
        endpoint.0.extend(vec![0.0; 24_000]);
        endpoint.0.extend(vec![0.1; 3_200]);
    }
    let sink = GateTestSink(&played);
    continuous::tick(
        &mut capture,
        &dictation.hands_free,
        &mut endpoint,
        &GateTestTarget,
        &sink,
        0,
    );
    assert_eq!(
        *dictation.hands_free.lock().phase(),
        if mixed_batch {
            continuous::Phase::Delivered
        } else {
            continuous::Phase::Capturing
        }
    );
    let reply = speak(&dictation, Caller::Model("session-a"), "reply", None).unwrap();
    rendered
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("synthesis reached its end");
    assert!(
        played
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err(),
        "playback started during active near-end speech"
    );
    assert!(
        dictation
            .echo
            .lock()
            .snapshot()
            .last_render_age_ms
            .is_none(),
        "held reply must not become a phantom echo reference"
    );
    if mixed_batch {
        endpoint.0 = vec![0.0; 32_000];
        continuous::tick(
            &mut capture,
            &dictation.hands_free,
            &mut endpoint,
            &GateTestTarget,
            &sink,
            2_000,
        );
        played
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("playback starts after the last open phrase submits");
        assert_eq!(dictation.hands_free.lock().delivered_turns(), 2);
        return;
    }
    if cancel {
        dictation.speaker.lock().as_ref().unwrap().speaker.hush();
    }
    endpoint.0 = vec![0.0; 32_000];
    continuous::tick(
        &mut capture,
        &dictation.hands_free,
        &mut endpoint,
        &GateTestTarget,
        &sink,
        2_000,
    );
    assert!(dictation.hands_free.lock().pending_text().is_some());
    assert!(
        played
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err(),
        "playback started before the held transcript was submitted"
    );
    continuous::tick(
        &mut capture,
        &dictation.hands_free,
        &mut endpoint,
        &GateTestTarget,
        &sink,
        10_000,
    );
    if cancel {
        let slot = dictation.speaker.lock();
        assert_eq!(
            slot.as_ref()
                .unwrap()
                .speaker
                .utterance(reply.utterance_id.parse().unwrap()),
            Some(speaker::Utterance::Interrupted)
        );
        assert!(
            played
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err(),
            "cancelled held reply played after submit"
        );
    } else {
        played
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("playback starts after submit");
    }
    assert_eq!(dictation.hands_free.lock().delivered_turns(), 1);
}

#[test]
fn reply_playback_waits_for_near_end_and_completed_submit() {
    playback_waits_case(false, false, false);
}

/// Catches: browser output bypassing the shared input/submission gate.
#[test]
fn browser_reply_playback_waits_for_near_end_and_completed_submit() {
    playback_waits_case(true, false, false);
}

/// Catches: a rendered reply escaping hush while waiting for input to submit.
#[test]
fn cancelled_reply_waiting_for_submit_never_reaches_output() {
    playback_waits_case(false, true, false);
}

/// Catches: the gate holding an ordinary idle conversation indefinitely.
#[test]
fn reply_playback_without_pending_input_starts_immediately() {
    let (dictation, _engine_gate, _config) = armed_with_a_voice("session-a");
    let (rendered_tx, _rendered) = std::sync::mpsc::channel();
    let (played_tx, played) = std::sync::mpsc::channel();
    let generation = dictation.hands_free.lock().generation();
    dictation.speaker.lock().as_mut().unwrap().speaker = Arc::new(speaker::Speaker::new(
        Arc::new(GateTestSpeech(rendered_tx)),
        Arc::new(GateTestOutput(played_tx, None)),
        generation,
    ));
    speak(&dictation, Caller::Model("session-a"), "reply", None).unwrap();
    played
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("idle playback starts without a capture tick or submit");
}

/// Catches: submitting a closed phrase erasing a new open phrase in the same batch.
#[test]
fn reply_playback_waits_for_the_last_open_phrase_in_a_mixed_batch() {
    playback_waits_case(false, false, true);
}

/// Catches: an empty first transcript admitting output before the remaining
/// closed utterance in the same capture batch has been transcribed/submitted.
#[test]
fn empty_first_transcript_does_not_release_reply_before_remaining_batch_submits() {
    closed_batch_playback_case("", "the real question", false);
}

/// Catches: rejected speech releasing playback before a later addressed turn.
#[test]
fn rejected_first_transcript_does_not_release_reply_before_remaining_batch_submits() {
    closed_batch_playback_case("across the room", "computer the real question", true);
}

/// Catches: the batch admission bit stranding playback after all speech is empty.
#[test]
fn empty_closed_batch_releases_reply_after_transcription() {
    closed_batch_playback_case("", "", false);
}

fn closed_batch_playback_case(first: &str, second: &str, activation: bool) {
    use speaker::StartPlayback;
    struct Endpoint<'a> {
        samples: Vec<f32>,
        passes: std::sync::atomic::AtomicUsize,
        transcripts: [&'a str; 2],
    }
    impl continuous::VoiceEndpoint for Endpoint<'_> {
        fn drain(&mut self) -> Result<Vec<f32>, String> {
            Ok(std::mem::take(&mut self.samples))
        }
        fn connected(&self) -> bool {
            true
        }
        fn transcribe(&self, _: &[f32]) -> Result<continuous::Transcript, String> {
            let pass = self
                .passes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(continuous::Transcript {
                text: self.transcripts[pass].into(),
                language: Some("it".into()),
            })
        }
    }
    struct ResumeProbe {
        gate: continuous::PlaybackGate,
        output: GateTestOutput,
    }
    impl continuous::Interruptible for ResumeProbe {
        fn pause(&self) {}
        fn resume(&self) {
            // Model a renderer scheduled exactly at the resume boundary. This
            // is the actual output admission adapter, not a copied predicate.
            let _ = self.gate.try_play(
                &self.output,
                &speech::SpeechAudio {
                    samples: vec![0.1; 160],
                    sample_rate: 16_000,
                },
            );
        }
        fn hush(&self) {}
    }
    struct Sink;
    impl continuous::VoiceSink for Sink {
        fn write(&self, _: &str, _: &str) -> Result<continuous::VoiceWrite, String> {
            Ok(continuous::VoiceWrite::Written)
        }
    }
    let (dictation, _engine_gate, _config) = armed_with_a_voice("session-a");
    if activation {
        let mut mode = dictation.hands_free.lock();
        mode.disarm(continuous::DisarmReason::Manual);
        mode.set_activation("computer", 30_000);
        mode.arm("session-a", "desktop", true).unwrap();
    }
    let (played_tx, played) = std::sync::mpsc::channel();
    let generation = dictation.hands_free.lock().generation();
    let interrupt = Arc::new(ResumeProbe {
        gate: continuous::PlaybackGate::new(Arc::clone(&dictation.hands_free), generation),
        output: GateTestOutput(played_tx, None),
    });
    let mut capture = continuous::Capture::new(
        continuous::SegmenterConfig::default(),
        60_000,
        0,
        Arc::new(parking_lot::Mutex::new(echo::EchoGuard::new(Box::new(
            echo::PassThrough,
        )))),
        Some(interrupt.clone()),
    );
    let mut samples = vec![0.1; 3_200];
    samples.extend(vec![0.0; 24_000]);
    samples.extend(vec![0.1; 3_200]);
    samples.extend(vec![0.0; 24_000]);
    let mut endpoint = Endpoint {
        samples,
        passes: std::sync::atomic::AtomicUsize::new(0),
        transcripts: [first, second],
    };
    continuous::tick(
        &mut capture,
        &dictation.hands_free,
        &mut endpoint,
        &GateTestTarget,
        &Sink,
        0,
    );
    assert_eq!(endpoint.passes.load(std::sync::atomic::Ordering::SeqCst), 2);
    if activation {
        assert_eq!(dictation.hands_free.lock().dropped_turns(), 1);
    }
    assert!(
        played.try_recv().is_err(),
        "reply reached output while captured utterances still awaited transcription and submit"
    );
    if second.is_empty() {
        continuous::Interruptible::resume(interrupt.as_ref());
        assert!(
            played.try_recv().is_ok(),
            "empty batch must release playback"
        );
    }
}
