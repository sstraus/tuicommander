//! Bucket state per `(AccountKey, BucketId)`: provider headers are the truth
//! when present; local token buckets cover the limits headers do not show
//! (GHES, search per-minute caps). Admission reserves in-flight units so
//! concurrent callers never all see the same remaining. Write spacing hands
//! out FIFO send slots. Time comes only from the `Clock` port.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::model::{AccountKey, BucketId};
use crate::ports::Clock;

/// Token bucket: `capacity` tokens, refilled evenly over `per`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalLimit {
    pub capacity: u32,
    pub per: Duration,
}

/// Static per-bucket limits the headers do not show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BucketConfig {
    pub local: Option<LocalLimit>,
    /// Minimum spacing between consecutive sends (writes are FIFO).
    pub write_gap: Option<Duration>,
}

/// Admission refused. `retry_after` is `None` when no reset is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    pub retry_after: Option<Duration>,
}

/// Units held for one admitted request. Pass it to `complete` or `cancel`.
#[derive(Debug)]
#[must_use]
pub struct Reservation {
    key: Key,
    units: u32,
    /// Wait this long before sending (write spacing); zero for reads.
    pub delay: Duration,
}

/// What the bucket currently believes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BucketStatus {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<SystemTime>,
    pub in_flight: u32,
}

type Key = (AccountKey, BucketId);

struct Tokens {
    cfg: LocalLimit,
    tokens: f64,
    at: Instant,
}

impl Tokens {
    fn rate(&self) -> f64 {
        f64::from(self.cfg.capacity) / self.cfg.per.as_secs_f64()
    }

    fn refill(&mut self, now: Instant) {
        let gained = now.saturating_duration_since(self.at).as_secs_f64() * self.rate();
        self.tokens = (self.tokens + gained).min(f64::from(self.cfg.capacity));
        self.at = now;
    }
}

#[derive(Default)]
struct Bucket {
    limit: Option<u32>,
    header_remaining: Option<u32>,
    reset: Option<SystemTime>,
    tokens: Option<Tokens>,
    write_gap: Option<Duration>,
    next_write: Option<Instant>,
    in_flight: u32,
}

impl Bucket {
    /// Header remaining, unless its window already reset (then it is stale).
    fn header_remaining(&self, wall: SystemTime) -> Option<u32> {
        let stale = self.reset.is_some_and(|r| r <= wall);
        self.header_remaining.filter(|_| !stale)
    }
}

pub struct BudgetBook {
    clock: Arc<dyn Clock>,
    buckets: Mutex<HashMap<Key, Bucket>>,
}

impl BudgetBook {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn configure(&self, account: &AccountKey, bucket: &BucketId, cfg: BucketConfig) {
        let now = self.clock.now();
        let mut map = self.buckets.lock().unwrap();
        let b = map.entry((account.clone(), bucket.clone())).or_default();
        b.write_gap = cfg.write_gap;
        b.tokens = cfg.local.map(|cfg| Tokens {
            cfg,
            tokens: f64::from(cfg.capacity),
            at: now,
        });
    }

    /// Provider headers override any local estimate. In-flight units stay
    /// reserved on top of the server's remaining.
    pub fn update(
        &self,
        account: &AccountKey,
        bucket: &BucketId,
        limit: Option<u32>,
        remaining: Option<u32>,
        reset: Option<SystemTime>,
    ) {
        let mut map = self.buckets.lock().unwrap();
        let b = map.entry((account.clone(), bucket.clone())).or_default();
        b.limit = limit.or(b.limit);
        b.header_remaining = remaining;
        b.reset = reset;
    }

    /// Admit `units` or say when to retry. Admission reserves the units.
    pub fn reserve(
        &self,
        account: &AccountKey,
        bucket: &BucketId,
        units: u32,
    ) -> Result<Reservation, Denied> {
        let (now, wall) = (self.clock.now(), self.clock.wall());
        let key = (account.clone(), bucket.clone());
        let mut map = self.buckets.lock().unwrap();
        let b = map.entry(key.clone()).or_default();

        if let Some(rem) = b.header_remaining(wall)
            && rem.saturating_sub(b.in_flight) < units
        {
            let retry_after = b.reset.and_then(|r| r.duration_since(wall).ok());
            return Err(Denied { retry_after });
        }
        if let Some(t) = b.tokens.as_mut() {
            t.refill(now);
            let missing = f64::from(units) - t.tokens;
            if missing > 0.0 {
                return Err(Denied {
                    retry_after: Some(Duration::from_secs_f64(missing / t.rate())),
                });
            }
            t.tokens -= f64::from(units);
        }
        b.in_flight += units;
        let mut delay = Duration::ZERO;
        if let Some(gap) = b.write_gap {
            let slot = b.next_write.map_or(now, |n| n.max(now));
            b.next_write = Some(slot + gap);
            delay = slot - now;
        }
        Ok(Reservation { key, units, delay })
    }

    /// The request was sent: its units leave in-flight and count as spent.
    pub fn complete(&self, r: Reservation) {
        let mut map = self.buckets.lock().unwrap();
        if let Some(b) = map.get_mut(&r.key) {
            b.in_flight = b.in_flight.saturating_sub(r.units);
            b.header_remaining = b.header_remaining.map(|v| v.saturating_sub(r.units));
        }
    }

    /// The request never left: units return to the bucket.
    pub fn cancel(&self, r: Reservation) {
        let mut map = self.buckets.lock().unwrap();
        if let Some(b) = map.get_mut(&r.key) {
            b.in_flight = b.in_flight.saturating_sub(r.units);
            if let Some(t) = b.tokens.as_mut() {
                t.tokens = (t.tokens + f64::from(r.units)).min(f64::from(t.cfg.capacity));
            }
        }
    }

    /// Forget what the headers said about one account (a new login may be a
    /// different identity with its own quota). Local limits stay configured.
    pub fn forget_account(&self, account: &AccountKey) {
        let mut map = self.buckets.lock().unwrap();
        for (_, b) in map.iter_mut().filter(|(k, _)| &k.0 == account) {
            b.limit = None;
            b.header_remaining = None;
            b.reset = None;
        }
    }

    /// Every bucket seen so far.
    pub fn keys(&self) -> Vec<(AccountKey, BucketId)> {
        self.buckets.lock().unwrap().keys().cloned().collect()
    }

    pub fn status(&self, account: &AccountKey, bucket: &BucketId) -> BucketStatus {
        let (now, wall) = (self.clock.now(), self.clock.wall());
        let mut map = self.buckets.lock().unwrap();
        let b = map.entry((account.clone(), bucket.clone())).or_default();
        let remaining = match b.header_remaining(wall) {
            Some(rem) => Some(rem),
            None => b.tokens.as_mut().map(|t| {
                t.refill(now);
                t.tokens.floor() as u32
            }),
        };
        BucketStatus {
            limit: b.limit.or(b.tokens.as_ref().map(|t| t.cfg.capacity)),
            remaining,
            reset: b.reset,
            in_flight: b.in_flight,
        }
    }
}
