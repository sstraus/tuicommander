//! Counters for every call (R6): the complete benchmark. A cache hit is a
//! call like any other, so each `execute` records exactly one outcome.
//!
//! Labels are static call-site names, account ids, bucket names and route
//! templates. Never a URL, query value, header or error text: diagnostics must
//! not be able to leak a credential.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::model::{BucketId, Channel, Pressure, Provider, Source};

/// Request records kept for diagnostics.
pub const RING_CAPACITY: usize = 2_000;
/// Latency samples kept per key for the percentiles.
const SAMPLES_PER_KEY: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MetricKey {
    pub caller: &'static str,
    pub provider: ProviderLabel,
    pub account: String,
    pub bucket: String,
    pub template: &'static str,
}

/// `Provider` has no `Ord`; the snapshot sorts by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderLabel(u8);

impl From<Provider> for ProviderLabel {
    fn from(p: Provider) -> Self {
        Self(p as u8)
    }
}

impl ProviderLabel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self.0 {
            x if x == Provider::GitHub as u8 => "github",
            x if x == Provider::GitLab as u8 => "gitlab",
            x if x == Provider::Jira as u8 => "jira",
            _ => "generic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Served { source: Source, status: u16 },
    Shed,
    RateLimited,
    TransportError,
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
pub struct Record {
    pub at: SystemTime,
    pub channel: Channel,
    pub outcome: Outcome,
    pub bytes_in: u64,
    pub units: u32,
    pub latency: Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counters {
    /// Every `execute`, whatever its outcome.
    pub calls: u64,
    /// Requests that crossed the wire and got an answer (network or 304).
    pub sent: u64,
    pub network_2xx: u64,
    pub network_other: u64,
    pub revalidated: u64,
    pub cache_hits: u64,
    pub stale_served: u64,
    pub shed: u64,
    pub rate_limited: u64,
    pub transport_errors: u64,
    pub cancelled: u64,
    pub bytes_in: u64,
    pub units: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
}

#[derive(Default)]
struct Cell {
    counters: Counters,
    samples: VecDeque<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    pub at: SystemTime,
    pub caller: &'static str,
    pub template: &'static str,
    pub channel: Channel,
    pub outcome: Outcome,
    pub units: u32,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub counters: Vec<(MetricKey, Counters)>,
    pub recent: Vec<RequestRecord>,
}

/// Quota state of one bucket, for diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct BucketReport {
    pub account: String,
    pub bucket: BucketId,
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<SystemTime>,
    pub in_flight: u32,
    pub pressure: Pressure,
    pub burn_per_sec: f64,
}

#[derive(Default)]
struct State {
    cells: HashMap<MetricKey, Cell>,
    ring: VecDeque<RequestRecord>,
}

#[derive(Default)]
pub struct Metrics(Mutex<State>);

impl Metrics {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, key: MetricKey, rec: Record) {
        let ms = u64::try_from(rec.latency.as_millis()).unwrap_or(u64::MAX);
        let mut st = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if st.ring.len() == RING_CAPACITY {
            st.ring.pop_front();
        }
        st.ring.push_back(RequestRecord {
            at: rec.at,
            caller: key.caller,
            template: key.template,
            channel: rec.channel,
            outcome: rec.outcome,
            units: rec.units,
            latency_ms: ms,
        });
        let cell = st.cells.entry(key).or_default();
        count(&mut cell.counters, &rec);
        if cell.samples.len() == SAMPLES_PER_KEY {
            cell.samples.pop_front();
        }
        cell.samples.push_back(ms);
    }

    /// Counters sorted by key, percentiles filled in.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let st = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut counters: Vec<_> = st
            .cells
            .iter()
            .map(|(k, c)| {
                let mut counters = c.counters.clone();
                counters.p50_ms = percentile(&c.samples, 50);
                counters.p95_ms = percentile(&c.samples, 95);
                (k.clone(), counters)
            })
            .collect();
        counters.sort_by(|a, b| a.0.cmp(&b.0));
        Snapshot {
            counters,
            recent: st.ring.iter().cloned().collect(),
        }
    }

    /// Starts a benchmark window.
    pub fn reset(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = State::default();
    }
}

fn count(c: &mut Counters, rec: &Record) {
    c.calls += 1;
    c.bytes_in += rec.bytes_in;
    c.units += u64::from(rec.units);
    match rec.outcome {
        Outcome::Served { source, status } => match source {
            Source::Network => {
                c.sent += 1;
                if (200..300).contains(&status) {
                    c.network_2xx += 1;
                } else {
                    c.network_other += 1;
                }
            }
            Source::Revalidated => {
                c.sent += 1;
                c.revalidated += 1;
            }
            Source::Cache => c.cache_hits += 1,
            Source::Stale => c.stale_served += 1,
        },
        Outcome::Shed => c.shed += 1,
        Outcome::RateLimited => c.rate_limited += 1,
        Outcome::TransportError => c.transport_errors += 1,
        Outcome::Cancelled => c.cancelled += 1,
    }
}

fn percentile(samples: &VecDeque<u64>, pct: usize) -> u64 {
    let mut v: Vec<u64> = samples.iter().copied().collect();
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[((v.len() - 1) * pct).div_ceil(100)]
}
