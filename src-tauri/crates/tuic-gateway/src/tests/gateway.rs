use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures::future::{BoxFuture, FutureExt};
use http::header::{AUTHORIZATION, ETAG};
use http::{HeaderMap, HeaderValue, Method};
use url::Url;

use crate::cache::{Cache, CacheKey};
use crate::failure::FixedJitter;
use crate::gateway::Gateway;
use crate::l2::{ColdStore, L2, StoredEntry};
use crate::model::*;
use crate::testing::{FakeClock, FakeTransport};

const HOUR: Duration = Duration::from_secs(3600);

pub(super) fn account() -> AccountKey {
    AccountKey {
        account_id: "work".into(),
        api_base: Url::parse("https://api.github.com").unwrap(),
    }
}

pub(super) fn request(method: Method, path: &str, caller: &'static str) -> GatewayRequest {
    let effect = if method == Method::GET {
        Effect::Read
    } else {
        Effect::Write
    };
    GatewayRequest {
        provider: Provider::GitHub,
        account: account(),
        method,
        url: Url::parse(&format!("https://api.github.com{path}")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        effect,
        channel: Channel::Visible,
        freshness: Freshness::Default,
        caller: CallerId(caller),
        graphql: None,
    }
}

pub(super) fn get(path: &str) -> GatewayRequest {
    request(Method::GET, path, "test")
}

pub(super) fn wire(
    status: u16,
    rate: Option<(u32, u32, u64)>,
    etag: Option<&str>,
    body: &'static str,
) -> WireResponse {
    let mut headers = HeaderMap::new();
    if let Some((limit, remaining, reset_in)) = rate {
        let reset = 1_700_000_000 + reset_in;
        for (k, v) in [
            ("x-ratelimit-limit", limit.to_string()),
            ("x-ratelimit-remaining", remaining.to_string()),
            ("x-ratelimit-reset", reset.to_string()),
        ] {
            headers.insert(k, HeaderValue::from_str(&v).unwrap());
        }
    }
    if let Some(e) = etag {
        headers.insert(ETAG, HeaderValue::from_str(e).unwrap());
    }
    WireResponse {
        status,
        headers,
        body: Bytes::from_static(body.as_bytes()),
    }
}

pub(super) fn setup() -> (Gateway<FakeTransport>, FakeClock, FakeTransport) {
    let (clock, transport) = (FakeClock::new(), FakeTransport::new());
    let cache = Cache::new(Arc::new(clock.clone()), 1 << 20);
    let gw = Gateway::new(
        Arc::new(clock.clone()),
        Arc::new(transport.clone()),
        cache,
        Arc::new(FixedJitter(0.5)),
    );
    (gw, clock, transport)
}

#[tokio::test]
async fn stale_if_error_serves_cached_body_as_stale_with_budget() {
    let (gw, clock, t) = setup();
    t.push_reply(Ok(wire(
        200,
        Some((5000, 4000, 7200)),
        Some("\"v1\""),
        "cached",
    )));
    gw.execute(get("/repos/o/r/pulls/1")).await.unwrap();

    clock.advance(HOUR); // stale, but inside the 24 h window
    t.push_reply(Err(TransportError::Timeout));
    let r = gw.execute(get("/repos/o/r/pulls/1")).await.unwrap();
    assert_eq!(r.source, Source::Stale);
    assert_eq!(r.body, Bytes::from_static(b"cached"));
    assert_eq!(r.budget.bucket, BucketId("core".into()));
    assert_eq!(r.budget.remaining, Some(4000));
    assert_eq!(r.budget.limit, Some(5000));

    // A 5xx recovers the same way; past 24 h nothing is served.
    t.push_reply(Ok(wire(503, None, None, "down")));
    let r = gw.execute(get("/repos/o/r/pulls/1")).await.unwrap();
    assert_eq!(r.source, Source::Stale);
    clock.advance(24 * HOUR);
    t.push_reply(Err(TransportError::Timeout));
    let e = gw.execute(get("/repos/o/r/pulls/1")).await.unwrap_err();
    assert!(matches!(e, GatewayError::Transport(_)), "{e:?}");
}

/// Primes `core` to Tight (10 of 100 left, reset in 1000 s) through one read.
async fn tight(gw: &Gateway<FakeTransport>, t: &FakeTransport) {
    t.push_reply(Ok(wire(200, Some((100, 10, 1000)), None, "x")));
    gw.execute(get("/repos/o/r/pulls/9")).await.unwrap();
}

#[tokio::test]
async fn shed_returns_typed_error_with_retry_at() {
    let (gw, _clock, t) = setup();
    tight(&gw, &t).await;

    let mut bg = request(Method::GET, "/repos/o/r/pulls/2", "poller");
    bg.channel = Channel::Background;
    let e = gw.execute(bg).await.unwrap_err();
    let reset = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_001_000);
    match e {
        GatewayError::Shed {
            bucket,
            retry_at,
            level,
        } => {
            assert_eq!(bucket, BucketId("core".into()));
            assert_eq!(retry_at, reset);
            assert_eq!(level, Pressure::Tight);
        }
        other => panic!("expected Shed, got {other:?}"),
    }
    assert_eq!(t.requests().len(), 1, "a shed call never reaches the wire");
    let counted = gw.metrics().snapshot();
    let poller = counted
        .counters
        .iter()
        .find(|(k, _)| k.caller == "poller")
        .unwrap();
    assert_eq!(poller.1.shed, 1);
}

#[tokio::test]
async fn metrics_count_every_outcome_per_caller() {
    let (gw, clock, t) = setup();
    let a = |p: &str| request(Method::GET, p, "tasks");
    t.push_reply(Ok(wire(200, Some((100, 90, 1000)), Some("\"v1\""), "body")));
    gw.execute(a("/repos/o/r/pulls/1")).await.unwrap(); // network
    let hit = gw.execute(a("/repos/o/r/pulls/1")).await.unwrap(); // cache
    assert_eq!(hit.source, Source::Cache);
    gw.execute(request(Method::GET, "/repos/o/r/pulls/1", "sidebar"))
        .await
        .unwrap(); // cache, other caller

    clock.advance(HOUR);
    t.push_reply(Ok(wire(304, None, None, "")));
    let r = gw.execute(a("/repos/o/r/pulls/1")).await.unwrap(); // revalidated
    assert_eq!(r.source, Source::Revalidated);
    clock.advance(HOUR);
    t.push_reply(Ok(wire(503, None, None, "")));
    let r = gw.execute(a("/repos/o/r/pulls/1")).await.unwrap(); // stale
    assert_eq!(r.source, Source::Stale);
    gw.execute(a("/repos/o/r/pulls/2")).await.unwrap_err(); // transport: no reply queued
    let mut limited = wire(429, None, None, "");
    limited
        .headers
        .insert("retry-after", HeaderValue::from_static("60"));
    t.push_reply(Ok(limited));
    let e = gw.execute(a("/repos/o/r/pulls/3")).await.unwrap_err();
    assert!(matches!(e, GatewayError::RateLimited { .. }), "{e:?}");

    let snap = gw.metrics().snapshot();
    let of = |caller: &str| {
        snap.counters
            .iter()
            .find(|(k, _)| k.caller == caller)
            .unwrap()
            .1
            .clone()
    };
    let tasks = of("tasks");
    assert_eq!(tasks.calls, 6);
    assert_eq!(
        (tasks.sent, tasks.network_2xx, tasks.revalidated),
        (2, 1, 1)
    );
    assert_eq!((tasks.cache_hits, tasks.stale_served), (1, 1));
    assert_eq!((tasks.transport_errors, tasks.rate_limited), (1, 1));
    assert_eq!((tasks.bytes_in, tasks.units), (4, 1));
    let sidebar = of("sidebar");
    assert_eq!((sidebar.calls, sidebar.cache_hits), (1, 1));
    assert_eq!(snap.recent.len(), 7);
}

#[tokio::test]
async fn metrics_never_contain_tokens_or_query_values() {
    let (gw, _clock, t) = setup();
    let mut req = get("/repos/o/r/pulls/1?access_token=QSECRET&state=open");
    req.headers
        .insert(AUTHORIZATION, HeaderValue::from_static("Bearer ghp_SECRET"));
    t.push_reply(Ok(wire(200, None, None, "ok")));
    gw.execute(req.clone()).await.unwrap();
    t.push_reply(Err(TransportError::Other(
        "failed https://x/?access_token=QSECRET".into(),
    )));
    let mut other = req.clone();
    other.url = Url::parse("https://api.github.com/other?access_token=QSECRET").unwrap();
    gw.execute(other).await.unwrap_err();

    let dump = format!("{:?}", gw.metrics().snapshot());
    for leak in [
        "ghp_SECRET",
        "QSECRET",
        "access_token",
        "state=open",
        "Bearer",
    ] {
        assert!(!dump.contains(leak), "{leak} leaked into {dump}");
    }
    assert!(dump.contains("/repos/{owner}/{repo}/pulls/{number}"));
}

#[derive(Default)]
struct Rows(Mutex<HashMap<CacheKey, StoredEntry>>);

impl ColdStore for Rows {
    fn get(&self, key: &CacheKey, _now: SystemTime) -> BoxFuture<'static, Option<StoredEntry>> {
        let r = self.0.lock().unwrap().get(key).cloned();
        async move { r }.boxed()
    }
    fn put_batch(&self, rows: Vec<StoredEntry>) -> BoxFuture<'static, ()> {
        let mut m = self.0.lock().unwrap();
        for r in rows {
            m.insert(r.key.clone(), r);
        }
        async {}.boxed()
    }
    fn delete_batch(&self, keys: Vec<CacheKey>) -> BoxFuture<'static, ()> {
        let mut m = self.0.lock().unwrap();
        for k in keys {
            m.remove(&k);
        }
        async {}.boxed()
    }
    fn purge_account(&self, _account_id: &str) -> BoxFuture<'static, ()> {
        async {}.boxed()
    }
    fn evict_to(&self, _c: u64, _i: Duration, _n: SystemTime) -> BoxFuture<'static, ()> {
        async {}.boxed()
    }
}

#[tokio::test]
async fn write_after_l2_restore_drops_the_l2_row() {
    let store = Arc::new(Rows::default());
    let process = |clock: &FakeClock| {
        let l2 = Arc::new(L2::new(store.clone(), Arc::new(clock.clone())));
        let cache = Cache::with_l2(Arc::new(clock.clone()), 1 << 20, l2.clone());
        (cache, l2)
    };
    let clock = FakeClock::new();
    let transport = FakeTransport::new();
    let gateway = |cache: Cache, t: &FakeTransport| {
        Gateway::new(
            Arc::new(clock.clone()),
            Arc::new(t.clone()),
            cache,
            Arc::new(FixedJitter(0.5)),
        )
    };

    let (cache, l2) = process(&clock);
    transport.push_reply(Ok(wire(200, None, Some("\"v1\""), "pr")));
    gateway(cache, &transport)
        .execute(get("/repos/o/r/pulls/1"))
        .await
        .unwrap();
    l2.flush().await;
    assert_eq!(store.0.lock().unwrap().len(), 1);

    // A restart: empty L1, the row only in the store.
    let restarted = FakeTransport::new();
    let (cache, l2) = process(&clock);
    let gw = gateway(cache, &restarted);
    let warm = gw.execute(get("/repos/o/r/pulls/1")).await.unwrap();
    assert_eq!(warm.source, Source::Cache);
    assert_eq!(restarted.requests().len(), 0);

    restarted.push_reply(Ok(wire(200, None, None, "{}")));
    gw.execute(request(Method::PATCH, "/repos/o/r/pulls/1", "tasks"))
        .await
        .unwrap();
    l2.flush().await;
    assert!(
        store.0.lock().unwrap().is_empty(),
        "the write left the L2 row behind"
    );
}

fn limited(retry_after: &'static str) -> WireResponse {
    let mut r = wire(429, None, None, "");
    r.headers
        .insert("retry-after", HeaderValue::from_static(retry_after));
    r
}

#[tokio::test(start_paused = true)]
async fn transient_failure_retries_once_then_surfaces() {
    let (gw, _clock, t) = setup();
    let mut req = get("/repos/o/r/pulls/1");
    req.channel = Channel::Interactive;

    t.push_reply(Ok(limited("2")));
    t.push_reply(Ok(wire(200, None, None, "second try")));
    let r = gw.execute(req.clone()).await.unwrap();
    assert_eq!(r.body, Bytes::from_static(b"second try"));
    assert_eq!(t.requests().len(), 2);

    // Still limited after the one retry: surfaces, never a third send.
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(limited("2")));
    t.push_reply(Ok(limited("2")));
    t.push_reply(Ok(wire(200, None, None, "never sent")));
    let e = gw.execute(req).await.unwrap_err();
    assert!(matches!(e, GatewayError::RateLimited { .. }), "{e:?}");
    assert_eq!(t.requests().len(), 2);

    // Not interactive, or a long wait: no retry.
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(limited("2")));
    gw.execute(get("/repos/o/r/pulls/1")).await.unwrap_err();
    assert_eq!(t.requests().len(), 1);
}

#[tokio::test]
async fn bucket_snapshot_reports_limit_remaining_in_flight() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(wire(200, Some((100, 40, 1000)), None, "x")));
    gw.execute(get("/repos/o/r/pulls/1")).await.unwrap();
    let held = gw
        .budgets()
        .reserve(&account(), &BucketId("core".into()), 2)
        .unwrap();

    let snap = gw.bucket_snapshots();
    assert_eq!(snap.len(), 1);
    let b = &snap[0];
    assert_eq!((b.account.as_str(), b.bucket.0.as_str()), ("work", "core"));
    assert_eq!(
        (b.limit, b.remaining, b.in_flight),
        (Some(100), Some(40), 2)
    );
    assert_eq!(b.pressure, Pressure::Watch);
    assert!(b.burn_per_sec > 0.0);
    drop(held);
}

/// A new login may be another identity: the limit memory and the header quota
/// of the account are dropped, another account is untouched.
#[tokio::test]
async fn reset_account_clears_limit_memory_and_quota_of_that_account_only() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(limited("600")));
    gw.execute(get("/repos/o/r/pulls/1")).await.unwrap_err();
    let blocked = gw.execute(get("/repos/o/r/pulls/2")).await.unwrap_err();
    assert!(
        matches!(blocked, GatewayError::RateLimited { .. }),
        "{blocked:?}"
    );
    assert_eq!(t.requests().len(), 1);

    let other = AccountKey {
        account_id: "other".into(),
        api_base: account().api_base,
    };
    gw.reset_account(&other).await;
    let still = gw.execute(get("/repos/o/r/pulls/2")).await.unwrap_err();
    assert!(
        matches!(still, GatewayError::RateLimited { .. }),
        "{still:?}"
    );

    gw.reset_account(&account()).await;
    t.push_reply(Ok(wire(200, None, None, "ok")));
    gw.execute(get("/repos/o/r/pulls/2")).await.unwrap();
    assert_eq!(t.requests().len(), 2);
}
