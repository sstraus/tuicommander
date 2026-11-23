//! One wire exchange and the state it feeds: the half of the facade that
//! touches the budget book, failure memory, burn rate and transport.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use http::HeaderMap;

use crate::budget::BudgetBook;
use crate::cache::{Cache, CachePolicy, Fetched};
use crate::failure::{FailureMemory, LimitHit, LimitKey};
use crate::headers;
use crate::invalidate::Signal;
use crate::model::{
    BudgetSnapshot, Effect, GatewayError, GatewayRequest, GatewayResponse, LimitReason,
};
use crate::policy::{self, BucketState, BurnTracker};
use crate::ports::{Clock, SignalSink, Transport};
use crate::routes::Route;

/// Bytes of a response body that header parsing may read (secondary-limit text).
const BODY_PREFIX: usize = 512;

pub(crate) type Flight = Result<Fetched, GatewayError>;

pub(crate) struct Core<T> {
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) transport: Arc<T>,
    pub(crate) cache: Cache,
    pub(crate) budgets: BudgetBook,
    pub(crate) failures: FailureMemory,
    pub(crate) burn: Mutex<HashMap<LimitKey, BurnTracker>>,
    pub(crate) sink: OnceLock<Arc<dyn SignalSink>>,
}

impl<T: Transport> Core<T> {
    fn limit_key(req: &GatewayRequest, route: &Route) -> LimitKey {
        (req.account.clone(), route.bucket.clone())
    }

    pub(crate) fn bucket_state(&self, req: &GatewayRequest, route: &Route) -> BucketState {
        self.state_of(&Self::limit_key(req, route))
    }

    pub(crate) fn state_of(&self, key: &LimitKey) -> BucketState {
        let wall = self.clock.wall();
        let burn = self.burn.lock().unwrap_or_else(|e| e.into_inner());
        BucketState {
            status: self.budgets.status(&key.0, &key.1),
            burn_per_sec: burn.get(key).map_or(0.0, |b| b.rate(wall)),
            now: wall,
            blocked_until: self.failures.limit_wait(key).map(|d| wall + d),
        }
    }

    pub(crate) fn respond(
        &self,
        req: &GatewayRequest,
        route: &Route,
        f: Fetched,
    ) -> GatewayResponse {
        let state = self.bucket_state(req, route);
        GatewayResponse {
            status: f.status,
            headers: f.headers,
            body: f.body,
            source: f.source,
            budget: BudgetSnapshot {
                bucket: route.bucket.clone(),
                limit: state.status.limit,
                remaining: state.status.remaining,
                reset: state.status.reset,
                pressure: policy::pressure(&state),
            },
        }
    }

    /// Teach burn rate, budget and failure memory what a billed response said.
    /// `Err` when the response is a limit answer that did not deliver a result.
    pub(crate) fn learn(
        &self,
        req: &GatewayRequest,
        route: &Route,
        status: u16,
        headers: &HeaderMap,
        prefix: &[u8],
    ) -> Result<(), GatewayError> {
        let key = Self::limit_key(req, route);
        let host = req.url.host_str().unwrap_or_default();
        let wall = self.clock.wall();
        self.burn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(key.clone())
            .or_default()
            .record(1, wall);

        let obs = headers::observe(req.provider, status, headers, prefix);
        if obs.limit.is_some() || obs.remaining.is_some() {
            self.budgets.update(
                &req.account,
                &route.bucket,
                obs.limit,
                obs.remaining,
                obs.reset,
            );
        }
        if let Some(reason) = obs.limited {
            let hit = LimitHit {
                retry_after: obs.retry_after,
                reset: obs.reset,
                reason,
            };
            let retry_at = self.failures.record_limit(&key, hit);
            // A 2xx that spent the last unit is a real answer (for a write, the
            // write happened): deliver it and let the hold stop the next call.
            if !spent_last_unit(status, reason, prefix) {
                return Err(GatewayError::RateLimited {
                    bucket: route.bucket.clone(),
                    retry_at,
                    reason,
                });
            }
        } else if (200..300).contains(&status) {
            self.failures.record_success(&key, host);
        }
        Ok(())
    }

    /// One wire exchange: reserve a unit, fetch, then teach budget and
    /// failure memory what the response said.
    pub(crate) async fn send(&self, req: &GatewayRequest, route: &Route) -> Flight {
        let reservation = self
            .budgets
            .reserve(&req.account, &route.bucket, 1)
            .map_err(|d| GatewayError::RateLimited {
                bucket: route.bucket.clone(),
                retry_at: self.clock.wall() + d.retry_after.unwrap_or(policy::LIMITED_RETRY),
                reason: LimitReason::Unknown,
            })?;
        if !reservation.delay.is_zero() {
            tokio::time::sleep(reservation.delay).await;
        }
        let policy = CachePolicy {
            ttl_base: route.ttl.base_ttl(),
            cacheable: req.identified_read(),
        };
        let host = req.url.host_str().unwrap_or_default();
        let fetched = match self
            .cache
            .fetch_tagged(req, policy, route, &*self.transport)
            .await
        {
            Ok(f) if f.billable => f,
            Ok(f) => {
                self.budgets.cancel(reservation); // a 304 or a cache hit spends nothing
                return Ok(f);
            }
            Err(e) => {
                self.budgets.cancel(reservation);
                if matches!(e, GatewayError::Transport(_)) {
                    self.failures.record_transport_failure(host);
                }
                return Err(e);
            }
        };
        self.budgets.complete(reservation);
        let prefix = &fetched.body[..fetched.body.len().min(BODY_PREFIX)];
        self.learn(req, route, fetched.status, &fetched.headers, prefix)?;
        if (200..300).contains(&fetched.status) {
            if req.effect == Effect::Write && route.effect == Effect::Write {
                let signal = Signal::from_write(req.account.clone(), route);
                self.cache.apply(&signal).await;
                if let Some(sink) = self.sink.get() {
                    sink.on_signal(&signal);
                }
            }
        }
        Ok(fetched)
    }
}

/// True for a 2xx that exhausted the bucket while answering normally. A body
/// that is itself the limit answer (GraphQL `RATE_LIMITED`) stays limited.
fn spent_last_unit(status: u16, reason: LimitReason, body: &[u8]) -> bool {
    (200..300).contains(&status)
        && reason == LimitReason::Primary
        && !headers::contains_ci(body, b"RATE_LIMITED")
}
