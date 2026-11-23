//! Failure memory and the single retry layer. Replaces the scattered
//! `GitHubCircuitBreaker` back-off: one place decides how long a rate-limited
//! bucket or a failing host stays closed, and whether a request may be resent.
//!
//! Inputs are plain values (status-derived `LimitHit`, effect, channel); header
//! parsing lives elsewhere. Time comes from `Clock`, jitter from `Jitter`, so
//! every decision is deterministic in tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::model::{AccountKey, BucketId, Channel, Effect, LimitReason};
use crate::ports::Clock;

/// Consecutive transport failures before a host opens (`github.rs` `CIRCUIT_BREAKER_THRESHOLD`).
pub const TRANSPORT_THRESHOLD: u32 = 3;
/// Initial host back-off (`CIRCUIT_BREAKER_BASE_MS`).
pub const TRANSPORT_BASE: Duration = Duration::from_millis(5_000);
/// Host back-off cap (`CIRCUIT_BREAKER_MAX_MS`).
pub const TRANSPORT_MAX: Duration = Duration::from_millis(300_000);
/// Host back-off multiplier (`CIRCUIT_BREAKER_MULTIPLIER`).
pub const TRANSPORT_MULTIPLIER: f64 = 2.0;

/// Wait when a limit carries neither `retry-after` nor a usable reset.
pub const LIMIT_DEFAULT_WAIT: Duration = Duration::from_secs(60);
/// Cap on the repeated-hit back-off.
pub const LIMIT_MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);
/// Jitter factor range applied to repeated hits.
pub const JITTER_MIN: f64 = 0.7;
pub const JITTER_MAX: f64 = 1.3;
/// A Read is resent only when the provider asks for at most this wait.
pub const RETRY_AFTER_MAX: Duration = Duration::from_secs(5);

pub type LimitKey = (AccountKey, BucketId);

/// Source of jitter: a sample in `[0, 1]`.
pub trait Jitter: Send + Sync + 'static {
    fn unit(&self) -> f64;
}

/// Always the same sample. For tests.
pub struct FixedJitter(pub f64);

impl Jitter for FixedJitter {
    fn unit(&self) -> f64 {
        self.0
    }
}

/// xorshift64 sequence; seed from wall time in production.
pub struct SeededJitter(Mutex<u64>);

impl SeededJitter {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(Mutex::new(seed | 1))
    }
}

impl Jitter for SeededJitter {
    fn unit(&self) -> f64 {
        let mut s = self.0.lock().unwrap();
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        (*s >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A provider refusal, reduced to plain inputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitHit {
    pub retry_after: Option<Duration>,
    pub reset: Option<SystemTime>,
    pub reason: LimitReason,
}

#[derive(Default)]
struct LimitState {
    hits: u32,
    until: Option<Instant>,
}

#[derive(Default)]
struct HostState {
    failures: u32,
    until: Option<Instant>,
}

#[derive(Default)]
struct State {
    limits: HashMap<LimitKey, LimitState>,
    hosts: HashMap<String, HostState>,
}

pub struct FailureMemory {
    clock: Arc<dyn Clock>,
    jitter: Arc<dyn Jitter>,
    state: Mutex<State>,
}

impl FailureMemory {
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, jitter: Arc<dyn Jitter>) -> Self {
        Self {
            clock,
            jitter,
            state: Mutex::default(),
        }
    }

    /// Records a rate-limit refusal and returns the wall time to retry at.
    /// Order: `retry-after`, then a future `reset`, then 60 s. The second and
    /// later consecutive hits double and jitter that wait (never below it),
    /// capped at 15 min unless the provider itself asked for longer.
    pub fn record_limit(&self, key: &LimitKey, hit: LimitHit) -> SystemTime {
        let now = self.clock.now();
        let wall = self.clock.wall();
        let stated = hit
            .retry_after
            .or_else(|| hit.reset.and_then(|r| r.duration_since(wall).ok()))
            .filter(|d| !d.is_zero())
            .unwrap_or(LIMIT_DEFAULT_WAIT);

        let mut st = self.state.lock().unwrap();
        let entry = st.limits.entry(key.clone()).or_default();
        entry.hits = entry.hits.saturating_add(1);
        let wait = if entry.hits == 1 {
            stated
        } else {
            let exp = i32::try_from(entry.hits - 1).unwrap_or(i32::MAX);
            let factor =
                JITTER_MIN + (JITTER_MAX - JITTER_MIN) * self.jitter.unit().clamp(0.0, 1.0);
            let backoff = stated.as_secs_f64() * 2f64.powi(exp) * factor;
            let capped = Duration::try_from_secs_f64(backoff)
                .unwrap_or(LIMIT_MAX_BACKOFF)
                .min(LIMIT_MAX_BACKOFF);
            capped.max(stated)
        };
        entry.until = Some(now + wait);
        wall + wait
    }

    /// Drop the limit memory of every bucket of one account.
    pub fn forget_account(&self, account: &AccountKey) {
        self.state
            .lock()
            .unwrap()
            .limits
            .retain(|key, _| &key.0 != account);
    }

    /// Remaining closed time of a bucket, if any.
    pub fn limit_wait(&self, key: &LimitKey) -> Option<Duration> {
        let st = self.state.lock().unwrap();
        remaining(st.limits.get(key)?.until, self.clock.now())
    }

    /// Records one transport failure. The host opens from the third in a row:
    /// 5 s, doubling per further failure, capped at 5 min.
    pub fn record_transport_failure(&self, host: &str) {
        let now = self.clock.now();
        let mut st = self.state.lock().unwrap();
        let entry = st.hosts.entry(host.to_owned()).or_default();
        entry.failures = entry.failures.saturating_add(1);
        if entry.failures >= TRANSPORT_THRESHOLD {
            let n = i32::try_from(entry.failures - TRANSPORT_THRESHOLD).unwrap_or(i32::MAX);
            let ms = TRANSPORT_BASE.as_secs_f64() * TRANSPORT_MULTIPLIER.powi(n);
            let wait = Duration::try_from_secs_f64(ms)
                .unwrap_or(TRANSPORT_MAX)
                .min(TRANSPORT_MAX);
            entry.until = Some(now + wait);
        }
    }

    /// Remaining closed time of a host, if any.
    pub fn host_wait(&self, host: &str) -> Option<Duration> {
        let st = self.state.lock().unwrap();
        remaining(st.hosts.get(host)?.until, self.clock.now())
    }

    /// A good response clears the bucket's limit memory and the host's failures.
    pub fn record_success(&self, key: &LimitKey, host: &str) {
        let mut st = self.state.lock().unwrap();
        st.limits.remove(key);
        st.hosts.remove(host);
    }
}

fn remaining(until: Option<Instant>, now: Instant) -> Option<Duration> {
    until?.checked_duration_since(now).filter(|d| !d.is_zero())
}

/// The only retry rule. Returns how long to wait before resending, or `None`.
/// A Write is never resent; a Read is resent once, only for an Interactive
/// caller and only when the provider asked for at most 5 s.
#[must_use]
pub fn single_retry(
    effect: Effect,
    channel: Channel,
    retry_after: Option<Duration>,
    already_retried: bool,
) -> Option<Duration> {
    if effect == Effect::Write || channel != Channel::Interactive || already_retried {
        return None;
    }
    retry_after.filter(|d| *d <= RETRY_AFTER_MAX)
}
