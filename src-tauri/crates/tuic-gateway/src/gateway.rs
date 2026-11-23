//! The facade: one `execute()` for every outbound request. It classifies the
//! route, asks policy for admission, serves from cache or goes to the wire
//! through the coalescer, feeds what the response taught back into the
//! budget and failure memory, and counts the outcome.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::budget::BudgetBook;
use crate::cache::{Cache, CacheKey, Entry, served};
use crate::coalesce::Coalescer;
use crate::exchange::{Core, Flight};
use crate::failure::{self, FailureMemory, Jitter};
use crate::invalidate::Signal;
use crate::metrics::{BucketReport, MetricKey, Metrics, Outcome, Record};
use crate::model::{
    AccountKey, Freshness, GatewayError, GatewayRequest, GatewayResponse, LimitReason, Source,
};
use crate::policy::{self, Admission, CacheView};
use crate::ports::{Clock, SignalSink, Transport};
use crate::routes::{self, Route};

/// An entry older than this is never served as stale.
pub const STALE_MAX: Duration = Duration::from_secs(24 * 3600);

pub struct Gateway<T: Transport> {
    pub(crate) core: Arc<Core<T>>,
    flights: Coalescer<CacheKey, Flight>,
    pub(crate) metrics: Arc<Metrics>,
}

impl<T: Transport> Gateway<T> {
    #[must_use]
    pub fn new(
        clock: Arc<dyn Clock>,
        transport: Arc<T>,
        cache: Cache,
        jitter: Arc<dyn Jitter>,
    ) -> Self {
        Self {
            core: Arc::new(Core {
                budgets: BudgetBook::new(clock.clone()),
                failures: FailureMemory::new(clock.clone(), jitter),
                burn: Mutex::default(),
                sink: OnceLock::new(),
                clock,
                transport,
                cache,
            }),
            flights: Coalescer::new(),
            metrics: Arc::new(Metrics::new()),
        }
    }

    #[must_use]
    pub fn budgets(&self) -> &BudgetBook {
        &self.core.budgets
    }

    #[must_use]
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Quota state of every bucket seen so far, sorted by account and bucket.
    #[must_use]
    pub fn bucket_snapshots(&self) -> Vec<BucketReport> {
        let mut out: Vec<_> = self
            .core
            .budgets
            .keys()
            .into_iter()
            .map(|key| {
                let st = self.core.state_of(&key);
                BucketReport {
                    account: key.0.account_id,
                    bucket: key.1,
                    limit: st.status.limit,
                    remaining: st.status.remaining,
                    reset: st.status.reset,
                    in_flight: st.status.in_flight,
                    pressure: policy::pressure(&st),
                    burn_per_sec: st.burn_per_sec,
                }
            })
            .collect();
        out.sort_by(|a, b| (&a.account, &a.bucket.0).cmp(&(&b.account, &b.bucket.0)));
        out
    }

    /// Where a successful write's signal goes after the cache applied it.
    /// Set once, at construction time of the app; a second call is ignored.
    pub fn set_signal_sink(&self, sink: Arc<dyn SignalSink>) {
        let _ = self.core.sink.set(sink);
    }

    /// External signals (poller update, logout) drop cached entries.
    pub async fn invalidate(&self, signal: &Signal) {
        self.core.cache.apply(signal).await;
    }

    /// A new login: forget the account's limits and quota, and drop its cache.
    pub async fn reset_account(&self, account: &AccountKey) {
        self.core.failures.forget_account(account);
        self.core.budgets.forget_account(account);
        self.invalidate(&Signal::Logout {
            account: account.clone(),
        })
        .await;
    }

    pub async fn execute(&self, req: GatewayRequest) -> Result<GatewayResponse, GatewayError> {
        let start = self.core.clock.now();
        let route = routes::classify(req.provider, &req.account, &req.method, &req.url);
        let result = self.run(&req, &route).await;
        self.record(&req, &route, &result, start);
        result
    }

    async fn run(
        &self,
        req: &GatewayRequest,
        route: &Route,
    ) -> Result<GatewayResponse, GatewayError> {
        let core = &self.core;
        let cached = core.cache.peek(req, route).await;
        let view = cache_view(cached.as_deref(), core.clock.now());
        let state = core.bucket_state(req, route);
        let bucket = route.bucket.clone();
        match policy::admit(&state, req.channel, req.freshness, view) {
            Admission::ServeCache => {
                let source = if view.fresh || req.freshness == Freshness::CacheOnly {
                    Source::Cache
                } else {
                    Source::Stale
                };
                let hit = cached.ok_or_else(|| GatewayError::Transport("cache miss".into()))?;
                Ok(core.respond(req, route, served(&hit, source, false)))
            }
            Admission::Shed(retry_at) => {
                let level = policy::pressure(&state);
                self.fallback(
                    req,
                    route,
                    cached.as_deref(),
                    GatewayError::Shed {
                        bucket,
                        retry_at,
                        level,
                    },
                )
            }
            Admission::Limited(retry_at) => {
                let reason = LimitReason::Unknown;
                self.fallback(
                    req,
                    route,
                    cached.as_deref(),
                    GatewayError::RateLimited {
                        bucket,
                        retry_at,
                        reason,
                    },
                )
            }
            Admission::Network | Admission::Conditional => {
                let host = req.url.host_str().unwrap_or_default();
                if core.failures.host_wait(host).is_some() {
                    let err = GatewayError::Transport("host is backing off".into());
                    return self.fallback(req, route, cached.as_deref(), err);
                }
                match self.fly_with_retry(req, route).await {
                    Ok(f) if f.status >= 500 => {
                        let err = GatewayError::Transport(format!("status {}", f.status));
                        self.fallback(req, route, cached.as_deref(), err)
                            .or_else(|_| Ok(core.respond(req, route, f)))
                    }
                    Ok(f) => Ok(core.respond(req, route, f)),
                    Err(e) => self.fallback(req, route, cached.as_deref(), e),
                }
            }
        }
    }

    /// A short `Retry-After` on an interactive read is waited out and sent
    /// once more, skipping admission: the limit it just recorded is the wait
    /// being served. A second refusal surfaces.
    async fn fly_with_retry(&self, req: &GatewayRequest, route: &Route) -> Flight {
        let first = self.fly(req, route).await;
        let Err(GatewayError::RateLimited { retry_at, .. }) = &first else {
            return first;
        };
        let wait = retry_at.duration_since(self.core.clock.wall()).ok();
        match failure::single_retry(req.effect, req.channel, wait, false) {
            Some(d) => {
                tokio::time::sleep(d).await;
                self.fly(req, route).await
            }
            None => first,
        }
    }

    /// Reads share one flight per key; writes never do.
    async fn fly(&self, req: &GatewayRequest, route: &Route) -> Flight {
        let core = Arc::clone(&self.core);
        let (r, rt) = (req.clone(), route.clone());
        let work = move || async move { core.send(&r, &rt).await };
        if !req.identified_read() {
            return work().await;
        }
        self.flights
            .run(CacheKey::new(req), work)
            .await
            .map_err(|_| GatewayError::Cancelled)?
    }

    /// Stale-if-error: a transport failure, a 5xx, a limit or a shed serves
    /// the cached entry when it is at most 24 h old.
    fn fallback(
        &self,
        req: &GatewayRequest,
        route: &Route,
        cached: Option<&Entry>,
        err: GatewayError,
    ) -> Result<GatewayResponse, GatewayError> {
        let recoverable = matches!(
            err,
            GatewayError::Transport(_)
                | GatewayError::RateLimited { .. }
                | GatewayError::Shed { .. }
        );
        let age_ok = |e: &Entry| {
            self.core
                .clock
                .now()
                .saturating_duration_since(e.fetched_at)
                <= STALE_MAX
        };
        match cached {
            Some(e) if recoverable && age_ok(e) => {
                Ok(self
                    .core
                    .respond(req, route, served(e, Source::Stale, false)))
            }
            _ => Err(err),
        }
    }

    fn record(
        &self,
        req: &GatewayRequest,
        route: &Route,
        result: &Result<GatewayResponse, GatewayError>,
        start: Instant,
    ) {
        let (outcome, bytes_in, units) = match result {
            Ok(r) => {
                let net = r.source == Source::Network;
                let outcome = Outcome::Served {
                    source: r.source,
                    status: r.status,
                };
                (
                    outcome,
                    if net { r.body.len() as u64 } else { 0 },
                    u32::from(net),
                )
            }
            Err(GatewayError::Shed { .. }) => (Outcome::Shed, 0, 0),
            Err(GatewayError::RateLimited { .. }) => (Outcome::RateLimited, 0, 0),
            Err(GatewayError::Transport(_)) => (Outcome::TransportError, 0, 0),
            Err(GatewayError::Cancelled) => (Outcome::Cancelled, 0, 0),
        };
        let key = metric_key(req, route);
        let latency = self.core.clock.now().saturating_duration_since(start);
        let at = self.core.clock.wall();
        let rec = Record {
            at,
            channel: req.channel,
            outcome,
            bytes_in,
            units,
            latency,
        };
        self.metrics.record(key, rec);
    }
}

pub(crate) fn metric_key(req: &GatewayRequest, route: &Route) -> MetricKey {
    MetricKey {
        caller: req.caller.0,
        provider: req.provider.into(),
        account: req.account.account_id.clone(),
        bucket: route.bucket.0.clone(),
        template: route.template,
    }
}

fn cache_view(entry: Option<&Entry>, now: Instant) -> CacheView {
    let Some(e) = entry else {
        return CacheView {
            fresh: false,
            stale: false,
            validator: false,
        };
    };
    let fresh = e.is_fresh(now);
    CacheView {
        fresh,
        stale: !fresh && now.saturating_duration_since(e.fetched_at) <= STALE_MAX,
        validator: e.etag.is_some() || e.last_modified.is_some(),
    }
}
