//! Shared retry suppression and one asynchronous notice per service outage.
use super::{Speech, SpeechAudio, SpeechCancel, SpeechError};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

const COOL_DOWN: Duration = Duration::from_secs(300);
pub type OutageNotice = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Default)]
pub struct Rejections(Mutex<Option<Outage>>);

struct Outage {
    service: &'static str,
    until: Instant,
    reason: String,
}

impl Rejections {
    pub fn reason(&self, service: &str) -> Option<String> {
        self.reason_at(service, Instant::now())
    }

    fn reason_at(&self, service: &str, now: Instant) -> Option<String> {
        let mut outage = self.0.lock();
        if outage.as_ref().is_some_and(|outage| now >= outage.until) {
            *outage = None;
        }
        outage
            .as_ref()
            .filter(|outage| outage.service == service)
            .map(|outage| outage.reason.clone())
    }

    fn rejected(&self, service: &'static str, status: u16) -> (String, bool) {
        let mut slot = self.0.lock();
        if let Some(outage) = slot.as_ref()
            && outage.service == service
            && Instant::now() < outage.until
        {
            return (outage.reason.clone(), false);
        }
        let reason = format!(
            "The speech service is unavailable ({service}, HTTP {status}); do not call voice speak again; tell the user in text. A five-minute cool-down is active."
        );
        *slot = Some(Outage {
            service,
            until: Instant::now() + COOL_DOWN,
            reason: reason.clone(),
        });
        (reason, true)
    }
}

/// Shares service health across queue rebuilds and reports asynchronous rejection.
pub struct GuardedSpeech {
    engine: Arc<dyn Speech>,
    rejections: Arc<Rejections>,
    service: &'static str,
    notice: Option<OutageNotice>,
}

impl GuardedSpeech {
    pub fn new(
        engine: Arc<dyn Speech>,
        rejections: Arc<Rejections>,
        service: &'static str,
        notice: Option<OutageNotice>,
    ) -> Self {
        Self {
            engine,
            rejections,
            service,
            notice,
        }
    }
}

impl Speech for GuardedSpeech {
    fn synthesize(
        &self,
        text: &str,
        voice: &str,
        cancel: &SpeechCancel,
    ) -> Result<SpeechAudio, SpeechError> {
        if cancel.is_cancelled() {
            return Err(SpeechError::Cancelled);
        }
        if let Some(reason) = self.rejections.reason(self.service) {
            return Err(SpeechError::Failed(reason));
        }
        match self.engine.synthesize(text, voice, cancel) {
            Err(SpeechError::Rejected { status }) => {
                let (reason, first) = self.rejections.rejected(self.service, status);
                if first && let Some(notice) = &self.notice {
                    notice(reason.clone());
                }
                Err(SpeechError::Failed(reason))
            }
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Rejecting(Arc<AtomicUsize>);
    impl Speech for Rejecting {
        fn synthesize(
            &self,
            _: &str,
            _: &str,
            _: &SpeechCancel,
        ) -> Result<SpeechAudio, SpeechError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(SpeechError::Rejected { status: 403 })
        }
    }

    #[test]
    fn transient_failure_does_not_suppress_the_next_request() {
        // catches: an ordinary network failure incorrectly disables speech.
        struct Offline(Arc<AtomicUsize>);
        impl Speech for Offline {
            fn synthesize(
                &self,
                _: &str,
                _: &str,
                _: &SpeechCancel,
            ) -> Result<SpeechAudio, SpeechError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(SpeechError::Failed("offline".into()))
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let health = Arc::new(Rejections::default());
        let engine = GuardedSpeech::new(
            Arc::new(Offline(calls.clone())),
            health.clone(),
            "Microsoft Edge",
            None,
        );
        for _ in 0..2 {
            assert!(
                engine
                    .synthesize("hello", "", &SpeechCancel::new())
                    .is_err()
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(health.reason("Microsoft Edge").is_none());
    }

    #[test]
    fn rejection_sends_one_notice_and_suppresses_calls_across_queue_rebuilds_until_expiry() {
        // catches: async failure silently retries, or rebuilding the queue clears its cooldown.
        let calls = Arc::new(AtomicUsize::new(0));
        let notices = Arc::new(Mutex::new(Vec::new()));
        let health = Arc::new(Rejections::default());
        for _ in 0..3 {
            let notices = notices.clone();
            let engine = GuardedSpeech::new(
                Arc::new(Rejecting(calls.clone())),
                health.clone(),
                "Microsoft Edge",
                Some(Arc::new(move |text| notices.lock().push(text))),
            );
            let error = engine
                .synthesize("hello", "", &SpeechCancel::new())
                .unwrap_err();
            assert!(error.to_string().contains("HTTP 403"));
            assert!(!error.to_string().contains("clock"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(notices.lock().len(), 1);
        assert_eq!(
            health.reason("Microsoft Edge").as_ref(),
            notices.lock().first()
        );
        assert!(health.reason("External speech engine").is_none());
        assert!(
            health
                .reason_at("Microsoft Edge", Instant::now() + Duration::from_secs(301))
                .is_none()
        );
        let collected = notices.clone();
        let engine = GuardedSpeech::new(
            Arc::new(Rejecting(calls.clone())),
            health,
            "Microsoft Edge",
            Some(Arc::new(move |text| collected.lock().push(text))),
        );
        assert!(
            engine
                .synthesize("try after expiry", "", &SpeechCancel::new())
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(notices.lock().len(), 2);
    }
}
