//! A barge-in ends a reply, not the hands-free conversation's need to hear an outage.
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tuic_dictation::speech::rejection::{GuardedSpeech, Rejections};
use tuic_dictation::speech::{Speech, SpeechAudio, SpeechCancel, SpeechError};

struct RejectedDuringBargeIn(Arc<AtomicUsize>);

impl Speech for RejectedDuringBargeIn {
    fn synthesize(
        &self,
        _: &str,
        _: &str,
        cancel: &SpeechCancel,
    ) -> Result<SpeechAudio, SpeechError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        // The user starts the next turn while the service's HTTP response arrives.
        cancel.cancel();
        Err(SpeechError::Rejected { status: 403 })
    }
}

#[test]
fn interrupted_reply_auth_rejection_still_warns_the_active_conversation() {
    // catches: barge-in consumes the only outage notice while activating the cooldown.
    let calls = Arc::new(AtomicUsize::new(0));
    let notices = Arc::new(Mutex::new(Vec::new()));
    let collected = notices.clone();
    let health = Arc::new(Rejections::default());
    let engine = GuardedSpeech::new(
        Arc::new(RejectedDuringBargeIn(calls.clone())),
        health,
        "Microsoft Edge",
        Some(Arc::new(move |notice| collected.lock().push(notice))),
    );
    assert!(
        engine
            .synthesize("first reply", "", &SpeechCancel::new())
            .is_err()
    );
    assert!(
        engine
            .synthesize("next turn", "", &SpeechCancel::new())
            .is_err()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "cooldown suppresses another service request"
    );
    let notices = notices.lock();
    assert_eq!(
        notices.len(),
        1,
        "an armed conversation must receive the outage notice even when the rejected reply was interrupted"
    );
    assert!(notices[0].contains("do not call voice speak again"));
    assert!(notices[0].contains("tell the user in text"));
}
