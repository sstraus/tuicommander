//! Fakes shared by the tests of every module (`test-support` feature for
//! other crates).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use futures::future::{BoxFuture, FutureExt};

use crate::model::{TransportError, WireRequest, WireResponse};
use crate::ports::{Clock, Transport};

type Reply = Result<WireResponse, TransportError>;

#[derive(Default)]
struct TransportState {
    requests: Vec<WireRequest>,
    replies: VecDeque<Reply>,
}

/// Records every request and answers from a queue; an empty queue answers
/// `TransportError::Other`. Clones share state.
#[derive(Clone, Default)]
pub struct FakeTransport(Arc<Mutex<TransportState>>);

impl FakeTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_reply(&self, reply: Reply) {
        self.0.lock().unwrap().replies.push_back(reply);
    }

    #[must_use]
    pub fn requests(&self) -> Vec<WireRequest> {
        self.0.lock().unwrap().requests.clone()
    }
}

impl Transport for FakeTransport {
    fn send(&self, req: WireRequest) -> BoxFuture<'static, Reply> {
        let mut state = self.0.lock().unwrap();
        state.requests.push(req);
        let reply = state
            .replies
            .pop_front()
            .unwrap_or_else(|| Err(TransportError::Other("no reply queued".into())));
        std::future::ready(reply).boxed()
    }
}

struct ClockState {
    base_instant: Instant,
    base_wall: SystemTime,
    elapsed: Duration,
}

/// Time moves only through `advance`. Clones share state.
#[derive(Clone)]
pub struct FakeClock(Arc<Mutex<ClockState>>);

impl FakeClock {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(ClockState {
            base_instant: Instant::now(),
            base_wall: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            elapsed: Duration::ZERO,
        })))
    }

    pub fn advance(&self, by: Duration) {
        self.0.lock().unwrap().elapsed += by;
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let s = self.0.lock().unwrap();
        s.base_instant + s.elapsed
    }

    fn wall(&self) -> SystemTime {
        let s = self.0.lock().unwrap();
        s.base_wall + s.elapsed
    }
}
