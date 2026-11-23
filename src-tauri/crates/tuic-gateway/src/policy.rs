//! Pressure levels (R4) and channel admission (R3/R5). Pure: no clock, no I/O.

use std::time::{Duration, SystemTime};

use crate::budget::BucketStatus;
use crate::model::{Channel, Freshness, Pressure};

pub const LEVELS: [(f64, Pressure); 4] = [
    (0.50, Pressure::Free),
    (0.20, Pressure::Watch),
    (0.05, Pressure::Tight),
    (0.0, Pressure::Critical),
];
pub const BURN_WINDOW: Duration = Duration::from_secs(300);
pub const SHED_RETRY: Duration = Duration::from_secs(30);
pub const LIMITED_RETRY: Duration = Duration::from_secs(60);

/// Everything `pressure` and `admit` read about one (account, bucket).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BucketState {
    pub status: BucketStatus,
    /// Units per second, from `BurnTracker::rate`.
    pub burn_per_sec: f64,
    pub now: SystemTime,
    /// Failure memory: no request may be sent before this instant.
    pub blocked_until: Option<SystemTime>,
}

/// What the cache holds for the request key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheView {
    pub fresh: bool,
    pub stale: bool,
    pub validator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Network,
    Conditional,
    ServeCache,
    Shed(SystemTime),
    Limited(SystemTime),
}

/// EWMA of units per second with a `BURN_WINDOW` time constant.
#[derive(Debug, Clone, Copy)]
pub struct BurnTracker {
    rate: f64,
    at: Option<SystemTime>,
}

impl BurnTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate: 0.0,
            at: None,
        }
    }

    pub fn record(&mut self, units: u32, now: SystemTime) {
        self.rate = self.rate(now) + f64::from(units) / BURN_WINDOW.as_secs_f64();
        self.at = Some(now);
    }

    #[must_use]
    pub fn rate(&self, now: SystemTime) -> f64 {
        let Some(at) = self.at else { return 0.0 };
        let dt = now.duration_since(at).unwrap_or_default();
        self.rate * (-dt.as_secs_f64() / BURN_WINDOW.as_secs_f64()).exp()
    }
}

impl Default for BurnTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn raised(level: Pressure) -> Pressure {
    match level {
        Pressure::Free => Pressure::Watch,
        Pressure::Watch => Pressure::Tight,
        _ => Pressure::Critical,
    }
}

/// Level of one bucket. Thresholds: Free above 50 %, Watch from 20 %, Tight
/// from 5 %, Critical above 0. Exhausted means remaining 0 or failure memory.
/// A reset in the past means the bucket refilled. A burn that empties the
/// bucket before the reset raises the level by one, never to Exhausted.
#[must_use]
pub fn pressure(state: &BucketState) -> Pressure {
    let BucketState {
        status,
        burn_per_sec,
        now,
        blocked_until,
    } = *state;
    if blocked_until.is_some_and(|t| t > now) {
        return Pressure::Exhausted;
    }
    let until_reset = match status.reset {
        Some(reset) => match reset.duration_since(now) {
            Ok(d) if !d.is_zero() => Some(d),
            _ => return Pressure::Free,
        },
        None => None,
    };
    let Some(remaining) = status.remaining else {
        return Pressure::Free;
    };
    if remaining == 0 {
        return Pressure::Exhausted;
    }
    let Some(limit) = status.limit.filter(|l| *l > 0) else {
        return Pressure::Free;
    };
    let ratio = f64::from(remaining) / f64::from(limit);
    let level = LEVELS
        .iter()
        .enumerate()
        .find(|(i, (edge, _))| {
            if *i == 0 {
                ratio > *edge
            } else {
                ratio >= *edge
            }
        })
        .map_or(Pressure::Critical, |(_, (_, level))| *level);
    match until_reset {
        Some(d) if burn_per_sec > 0.0 && f64::from(remaining) / burn_per_sec < d.as_secs_f64() => {
            raised(level)
        }
        _ => level,
    }
}

/// TTL multiplier the cache applies at this level (plan table).
#[must_use]
pub fn ttl_multiplier(level: Pressure, channel: Channel) -> u32 {
    match (channel, level) {
        (Channel::Interactive, _) => 1,
        (Channel::Background, Pressure::Watch) => 2,
        (_, Pressure::Tight) => 4,
        _ => 1,
    }
}

fn retry_at(state: &BucketState, fallback: Duration) -> SystemTime {
    let blocked = state.blocked_until.filter(|t| *t > state.now);
    // The reset only matters when the bucket is empty or nothing else blocks.
    let reset = state
        .status
        .reset
        .filter(|_| blocked.is_none() || state.status.remaining == Some(0));
    let wait = [reset, blocked]
        .into_iter()
        .flatten()
        .filter(|t| *t > state.now)
        .max();
    wait.unwrap_or(state.now + fallback)
}

/// Decide how one request may proceed. `Force` is treated as Interactive and
/// skips freshness, but an exhausted bucket (or failure memory) never sends:
/// it serves any cached entry, else `Limited`. `CacheOnly` never sends.
/// `ServeCache` on a stale entry at Watch means stale-while-revalidate: the
/// cache layer revalidates through a Background admission.
#[must_use]
pub fn admit(
    state: &BucketState,
    channel: Channel,
    freshness: Freshness,
    cache: CacheView,
) -> Admission {
    if freshness == Freshness::CacheOnly {
        return Admission::ServeCache;
    }
    let level = pressure(state);
    let any = cache.fresh || cache.stale;
    if level == Pressure::Exhausted {
        return if any {
            Admission::ServeCache
        } else {
            Admission::Limited(retry_at(state, LIMITED_RETRY))
        };
    }
    let force = freshness == Freshness::Force;
    let channel = if force { Channel::Interactive } else { channel };
    if cache.fresh && !force {
        return Admission::ServeCache;
    }
    let send = if cache.validator {
        Admission::Conditional
    } else {
        Admission::Network
    };
    let shed = Admission::Shed(retry_at(state, SHED_RETRY));
    match (channel, level) {
        (Channel::Interactive, Pressure::Critical) if state.status.in_flight > 0 => shed,
        (Channel::Interactive, _) => send,
        (Channel::Visible, Pressure::Free) => send,
        (Channel::Visible, Pressure::Watch) if cache.stale => Admission::ServeCache,
        (Channel::Visible, Pressure::Watch) => send,
        (Channel::Visible, Pressure::Tight) if cache.validator => send,
        (Channel::Visible, Pressure::Tight) if cache.stale => Admission::ServeCache,
        (Channel::Visible, Pressure::Tight) => send,
        (Channel::Visible, _) if cache.stale => Admission::ServeCache,
        (Channel::Background, Pressure::Free | Pressure::Watch) => send,
        (Channel::Background, Pressure::Tight) if cache.validator => send,
        _ => shed,
    }
}
