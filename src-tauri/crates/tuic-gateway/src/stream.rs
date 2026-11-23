//! `execute_stream`: a large body (a release download) that must not be held
//! in memory or cached. Admission, the budget, failure memory and the metrics
//! apply; the cache, coalescing and stale-if-error do not.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use futures::stream::{BoxStream, Stream, StreamExt};
use http::HeaderMap;

use crate::gateway::{Gateway, metric_key};
use crate::metrics::{MetricKey, Metrics, Outcome, Record};
use crate::model::{
    BudgetSnapshot, Channel, GatewayError, GatewayRequest, LimitReason, StreamResponse,
    TransportError, WireRequest,
};
use crate::policy::{self, Admission, CacheView};
use crate::ports::{Clock, Transport};
use crate::routes::{self, Route};

impl<T: Transport> Gateway<T> {
    /// One uncached send whose body the caller reads as it arrives. The call is
    /// counted once, when the body ends or is dropped, with the bytes read.
    pub async fn execute_stream(
        &self,
        req: GatewayRequest,
    ) -> Result<StreamResponse, GatewayError> {
        let start = self.core.clock.now();
        let route = routes::classify(req.provider, &req.account, &req.method, &req.url);
        let key = metric_key(&req, &route);
        match self.open_stream(&req, &route).await {
            Ok((status, headers, body)) => {
                let budget = self.budget_of(&req, &route);
                let tally = Tally {
                    metrics: self.metrics.clone(),
                    clock: self.core.clock.clone(),
                    key,
                    channel: req.channel,
                    status,
                    start,
                    bytes: 0,
                    failed: false,
                };
                Ok(StreamResponse {
                    status,
                    headers,
                    body: Counted { inner: body, tally }.boxed(),
                    budget,
                })
            }
            Err(error) => {
                let outcome = match &error {
                    GatewayError::Shed { .. } => Outcome::Shed,
                    GatewayError::RateLimited { .. } => Outcome::RateLimited,
                    GatewayError::Transport(_) => Outcome::TransportError,
                    GatewayError::Cancelled => Outcome::Cancelled,
                };
                self.metrics.record(
                    key,
                    Record {
                        at: self.core.clock.wall(),
                        channel: req.channel,
                        outcome,
                        bytes_in: 0,
                        units: 0,
                        latency: self.core.clock.now().saturating_duration_since(start),
                    },
                );
                Err(error)
            }
        }
    }

    async fn open_stream(
        &self,
        req: &GatewayRequest,
        route: &Route,
    ) -> Result<
        (
            u16,
            HeaderMap,
            BoxStream<'static, Result<Bytes, TransportError>>,
        ),
        GatewayError,
    > {
        let core = &self.core;
        let state = core.bucket_state(req, route);
        let none = CacheView {
            fresh: false,
            stale: false,
            validator: false,
        };
        match policy::admit(&state, req.channel, req.freshness, none) {
            Admission::Network | Admission::Conditional => {}
            Admission::Shed(retry_at) => {
                return Err(GatewayError::Shed {
                    bucket: route.bucket.clone(),
                    retry_at,
                    level: policy::pressure(&state),
                });
            }
            Admission::Limited(retry_at) => {
                return Err(GatewayError::RateLimited {
                    bucket: route.bucket.clone(),
                    retry_at,
                    reason: LimitReason::Unknown,
                });
            }
            Admission::ServeCache => return Err(GatewayError::Transport("cache miss".into())),
        }
        let host = req.url.host_str().unwrap_or_default();
        if core.failures.host_wait(host).is_some() {
            return Err(GatewayError::Transport("host is backing off".into()));
        }
        let reservation = core
            .budgets
            .reserve(&req.account, &route.bucket, 1)
            .map_err(|d| GatewayError::RateLimited {
                bucket: route.bucket.clone(),
                retry_at: core.clock.wall() + d.retry_after.unwrap_or(policy::LIMITED_RETRY),
                reason: LimitReason::Unknown,
            })?;
        if !reservation.delay.is_zero() {
            tokio::time::sleep(reservation.delay).await;
        }
        let wire = WireRequest {
            method: req.method.clone(),
            url: req.url.clone(),
            headers: req.headers.clone(),
            body: req.body.clone(),
        };
        let opened = match core.transport.send_stream(wire).await {
            Ok(opened) => opened,
            Err(error) => {
                core.budgets.cancel(reservation);
                core.failures.record_transport_failure(host);
                return Err(error.into());
            }
        };
        core.budgets.complete(reservation);
        core.learn(req, route, opened.status, &opened.headers, &[])?;
        Ok((opened.status, opened.headers, opened.body))
    }

    fn budget_of(&self, req: &GatewayRequest, route: &Route) -> BudgetSnapshot {
        let state = self.core.bucket_state(req, route);
        BudgetSnapshot {
            bucket: route.bucket.clone(),
            limit: state.status.limit,
            remaining: state.status.remaining,
            reset: state.status.reset,
            pressure: policy::pressure(&state),
        }
    }
}

/// What the metrics record of one stream, written once on drop.
struct Tally {
    metrics: Arc<Metrics>,
    clock: Arc<dyn Clock>,
    key: MetricKey,
    channel: Channel,
    status: u16,
    start: Instant,
    bytes: u64,
    failed: bool,
}

impl Drop for Tally {
    fn drop(&mut self) {
        let outcome = if self.failed {
            Outcome::TransportError
        } else {
            Outcome::Served {
                source: crate::model::Source::Network,
                status: self.status,
            }
        };
        self.metrics.record(
            self.key.clone(),
            Record {
                at: self.clock.wall(),
                channel: self.channel,
                outcome,
                bytes_in: self.bytes,
                units: 1,
                latency: self.clock.now().saturating_duration_since(self.start),
            },
        );
    }
}

struct Counted {
    inner: BoxStream<'static, Result<Bytes, TransportError>>,
    tally: Tally,
}

impl Stream for Counted {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let polled = self.inner.poll_next_unpin(cx);
        match &polled {
            Poll::Ready(Some(Ok(chunk))) => self.tally.bytes += chunk.len() as u64,
            Poll::Ready(Some(Err(_))) => self.tally.failed = true,
            _ => {}
        }
        polled
    }
}
