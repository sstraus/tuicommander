//! A bounded wake hint for the existing grid serializer, not another send path.
use parking_lot::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_millis(100);
const BYPASS_SENDS: usize = 4;
const MIN_SPACING: Duration = Duration::from_millis(8);

#[derive(Default)]
pub(crate) struct InputFramePriority {
    inner: Mutex<PriorityState>,
    ticker: OnceLock<std::thread::Thread>,
}

#[derive(Default)]
struct PriorityState {
    generation: u64,
    expires: Option<Instant>,
    output: u64,
    pending: bool,
    // Rolling quota: new input must not replenish it during an output flood.
    sent: [Option<Instant>; BYPASS_SENDS],
}

#[derive(Clone, Copy)]
pub(super) struct PriorityRequest {
    output: u64,
    pub(super) ready_at: Instant,
}

impl PriorityState {
    fn request(&self, now: Instant, last_sent: Option<Instant>) -> Option<PriorityRequest> {
        if !self.pending || now >= self.expires? {
            return None;
        }
        let mut ready_at = now;
        if let Some(last) = last_sent {
            ready_at = ready_at.max(last + MIN_SPACING);
        }
        if let Some(oldest) = self.sent[0] {
            ready_at = ready_at.max(oldest + WINDOW);
        }
        (ready_at < self.expires?).then_some(PriorityRequest {
            output: self.output,
            ready_at,
        })
    }
}

impl InputFramePriority {
    pub(super) fn bind_ticker(&self) {
        let _ = self.ticker.set(std::thread::current());
    }

    pub(super) fn arm(&self, now: Instant) {
        let mut state = self.inner.lock();
        state.generation = state.generation.wrapping_add(1).max(1);
        state.expires = Some(now + WINDOW);
        state.pending = false;
    }

    pub(super) fn generation(&self) -> u64 {
        self.inner.lock().generation
    }

    /// Called only after the reader applied a chunk that began after input.
    pub(super) fn applied(&self, generation: u64, now: Instant) {
        let mut state = self.inner.lock();
        if generation == 0
            || generation != state.generation
            || state.expires.is_none_or(|end| now >= end)
        {
            return;
        }
        state.output = state.output.wrapping_add(1);
        let wake = !state.pending;
        state.pending = true;
        let eligible = state
            .request(now, None)
            .is_some_and(|request| request.ready_at <= now);
        drop(state);
        if wake
            && eligible
            && let Some(ticker) = self.ticker.get()
        {
            // unpark retains one token if the reader wins the park race.
            ticker.unpark();
        }
    }

    pub(super) fn request(
        &self,
        now: Instant,
        last_sent: Option<Instant>,
    ) -> Option<PriorityRequest> {
        self.inner.lock().request(now, last_sent)
    }

    /// Consume quota only after the shared serializer cut a frame. A newer
    /// reader notification must survive a send of the preceding snapshot.
    pub(super) fn sent(
        &self,
        request: PriorityRequest,
        now: Instant,
        bypassed: bool,
    ) -> Option<Instant> {
        let mut state = self.inner.lock();
        if state.output == request.output {
            state.pending = false;
        }
        if bypassed {
            state.sent.rotate_left(1);
            state.sent[BYPASS_SENDS - 1] = Some(now);
        }
        // Newer reader work can arrive while the old request is pending, so
        // applied() deliberately coalesces its wake. Keep its bounded retry on
        // the existing ticker rather than stranding it until a periodic tick.
        state.request(now, Some(now)).map(|next| next.ready_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_input_chunk_cannot_spend_the_echo_priority() {
        let priority = InputFramePriority::default();
        let now = Instant::now();
        let before = priority.generation();
        priority.arm(now);
        priority.applied(before, now);
        assert!(priority.request(now, None).is_none());
        priority.applied(priority.generation(), now);
        assert!(priority.request(now, None).is_some());
    }

    #[test]
    fn repeated_input_cannot_replenish_the_rolling_send_budget() {
        let priority = InputFramePriority::default();
        let now = Instant::now();
        for ms in [0, 8, 16, 24] {
            let at = now + Duration::from_millis(ms);
            priority.arm(at);
            priority.applied(priority.generation(), at);
            let request = priority.request(at, None).unwrap();
            assert_eq!(request.ready_at, at);
            priority.sent(request, at, true);
        }
        let at = now + Duration::from_millis(32);
        priority.arm(at);
        priority.applied(priority.generation(), at);
        assert_eq!(priority.request(at, None).unwrap().ready_at, now + WINDOW);
    }

    #[test]
    fn expired_priority_cannot_bypass_background_floors() {
        let priority = InputFramePriority::default();
        let now = Instant::now();
        priority.arm(now);
        priority.applied(priority.generation(), now);
        assert!(priority.request(now + WINDOW, None).is_none());
    }

    /// Catches: a retained retry bypassing spacing, replenishing spent credits,
    /// or resurrecting a hint whose remaining window cannot admit another send.
    #[test]
    fn retained_send_retry_preserves_spacing_quota_and_expiry() {
        let mut failures = Vec::new();
        for (spent, at_ms, expected_ms) in [
            (false, 32, Some(40)),
            (true, 32, Some(100)),
            (false, 99, None),
        ] {
            let priority = InputFramePriority::default();
            let start = Instant::now();
            priority.arm(start);
            if spent {
                for ms in [0, 8, 16, 24] {
                    let at = start + Duration::from_millis(ms);
                    priority.applied(priority.generation(), at);
                    let request = priority.request(at, None).unwrap();
                    priority.sent(request, at, true);
                }
            }
            let at = start + Duration::from_millis(at_ms);
            if spent {
                // A new keystroke extends expiry, but not the rolling quota.
                priority.arm(at);
            }
            priority.applied(priority.generation(), at);
            let previous = priority.request(at, None).unwrap();
            priority.applied(priority.generation(), at);
            let actual = priority.sent(previous, at, false);
            let expected = expected_ms.map(|ms| start + Duration::from_millis(ms));
            if actual != expected {
                failures.push(format!(
                    "spent={spent}, send at {at_ms}ms: expected {expected:?}, got {actual:?}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn priority_coalesces_output_and_preserves_newer_applied_damage() {
        let priority = InputFramePriority::default();
        let now = Instant::now();
        priority.arm(now);
        let generation = priority.generation();
        priority.applied(generation, now);
        let request = priority.request(now, Some(now)).unwrap();
        assert_eq!(request.ready_at, now + MIN_SPACING);
        priority.applied(generation, now);
        // Catches: preserving newer pending work but losing its ticker retry
        // after its wake was coalesced into the send still in progress.
        assert_eq!(
            priority.sent(request, now, true),
            Some(now + Duration::from_millis(8))
        );
        assert!(priority.request(now, None).is_some());
        let newer = priority.request(now, None).unwrap();
        assert_eq!(priority.sent(newer, now + MIN_SPACING, true), None);
        assert!(priority.request(now + MIN_SPACING, None).is_none());
    }
}
