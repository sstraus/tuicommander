use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::executor::block_on;
use http::header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH};
use http::{HeaderMap, HeaderValue, Method};
use url::Url;

use crate::cache::{Cache, CachePolicy, MAX_BODY_BYTES, MAX_TTL_MULT};
use crate::model::*;
use crate::testing::{FakeClock, FakeTransport};

fn account(id: &str) -> AccountKey {
    AccountKey {
        account_id: id.into(),
        api_base: Url::parse("https://api.github.com").unwrap(),
    }
}

fn request(acct: &str, method: Method, effect: Effect) -> GatewayRequest {
    GatewayRequest {
        provider: Provider::GitHub,
        account: account(acct),
        method,
        url: Url::parse("https://api.github.com/repos/o/r/pulls/1").unwrap(),
        headers: HeaderMap::new(),
        body: None,
        effect,
        channel: Channel::Visible,
        freshness: Freshness::Default,
        caller: CallerId("test"),
        graphql: None,
    }
}

fn get(acct: &str) -> GatewayRequest {
    request(acct, Method::GET, Effect::Read)
}

fn reply(status: u16, etag: Option<&str>, body: &'static str) -> WireResponse {
    let mut headers = HeaderMap::new();
    if let Some(e) = etag {
        headers.insert(ETAG, HeaderValue::from_str(e).unwrap());
    }
    WireResponse {
        status,
        headers,
        body: Bytes::from_static(body.as_bytes()),
    }
}

const POLICY: CachePolicy = CachePolicy {
    ttl_base: Duration::from_secs(60),
    cacheable: true,
};

fn setup() -> (Cache, FakeClock, FakeTransport) {
    let clock = FakeClock::new();
    (
        Cache::new(Arc::new(clock.clone()), 1 << 20),
        clock,
        FakeTransport::new(),
    )
}

fn fetch(
    cache: &Cache,
    req: &GatewayRequest,
    policy: CachePolicy,
    t: &FakeTransport,
) -> crate::cache::Fetched {
    block_on(cache.fetch(req, policy, t)).unwrap()
}

#[test]
fn fresh_hit_sends_nothing() {
    let (cache, clock, t) = setup();
    t.push_reply(Ok(reply(200, Some("\"a\""), "body")));
    fetch(&cache, &get("a"), POLICY, &t);
    clock.advance(Duration::from_secs(59));
    let hit = fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(hit.source, Source::Cache);
    assert_eq!(hit.body, Bytes::from_static(b"body"));
    assert_eq!(t.requests().len(), 1);
    assert!(!hit.billable);
}

#[test]
fn stale_github_get_sends_if_none_match_and_304_spends_no_core_unit() {
    let (cache, clock, t) = setup();
    t.push_reply(Ok(reply(200, Some("\"a\""), "body")));
    assert!(fetch(&cache, &get("a"), POLICY, &t).billable);
    clock.advance(Duration::from_secs(61));
    t.push_reply(Ok(reply(304, None, "")));
    let r = fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(r.source, Source::Revalidated);
    assert_eq!(r.status, 200);
    assert_eq!(r.body, Bytes::from_static(b"body"));
    assert!(!r.billable);
    assert_eq!(t.requests()[1].headers.get(IF_NONE_MATCH).unwrap(), "\"a\"");
}

#[test]
fn status_304_without_validator_is_a_plain_status() {
    let (cache, _clock, t) = setup();
    // No cached entry: nothing was sent as a validator.
    t.push_reply(Ok(reply(304, None, "")));
    let r = fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(r.status, 304);
    assert_eq!(r.source, Source::Network);
    assert!(r.billable);
    assert!(t.requests()[0].headers.get(IF_NONE_MATCH).is_none());
    // It was not stored as a hit.
    t.push_reply(Ok(reply(200, None, "real")));
    let r = fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(r.source, Source::Network);
    assert_eq!(r.body, Bytes::from_static(b"real"));
}

#[test]
fn unchanged_revalidations_grow_ttl_to_cap_and_change_resets_it() {
    let (cache, clock, t) = setup();
    t.push_reply(Ok(reply(200, Some("\"a\""), "body")));
    fetch(&cache, &get("a"), POLICY, &t);
    let mut mult = 1.0;
    for _ in 0..10 {
        clock.advance(Duration::from_secs(60 * 8 + 1));
        t.push_reply(Ok(reply(304, None, "")));
        fetch(&cache, &get("a"), POLICY, &t);
        mult = f64::min(mult * 1.5, MAX_TTL_MULT);
        assert_eq!(block_on(cache.entry_ttl_mult(&get("a"))), Some(mult));
    }
    assert_eq!(mult, MAX_TTL_MULT);
    // A 200 with the same body also counts as unchanged.
    clock.advance(Duration::from_secs(60 * 8 + 1));
    t.push_reply(Ok(reply(200, Some("\"b\""), "body")));
    fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(
        block_on(cache.entry_ttl_mult(&get("a"))),
        Some(MAX_TTL_MULT)
    );
    // A changed body resets to x1.
    clock.advance(Duration::from_secs(60 * 8 + 1));
    t.push_reply(Ok(reply(200, Some("\"c\""), "new body")));
    fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(block_on(cache.entry_ttl_mult(&get("a"))), Some(1.0));
}

#[test]
fn cache_never_crosses_accounts() {
    let (cache, _clock, t) = setup();
    t.push_reply(Ok(reply(200, Some("\"a\""), "private-a")));
    fetch(&cache, &get("a"), POLICY, &t);
    t.push_reply(Ok(reply(200, Some("\"b\""), "private-b")));
    let b = fetch(&cache, &get("b"), POLICY, &t);
    assert_eq!(b.source, Source::Network);
    assert_eq!(b.body, Bytes::from_static(b"private-b"));
    assert!(t.requests()[1].headers.get(IF_NONE_MATCH).is_none());
}

#[test]
fn writes_and_large_bodies_are_not_cached() {
    let (cache, _clock, t) = setup();
    let w = request("a", Method::POST, Effect::Write);
    t.push_reply(Ok(reply(200, Some("\"a\""), "w")));
    t.push_reply(Ok(reply(200, Some("\"a\""), "w")));
    fetch(&cache, &w, POLICY, &t);
    assert_eq!(fetch(&cache, &w, POLICY, &t).source, Source::Network);

    let big: &'static str = Box::leak("x".repeat(MAX_BODY_BYTES + 1).into_boxed_str());
    t.push_reply(Ok(reply(200, Some("\"a\""), big)));
    t.push_reply(Ok(reply(200, Some("\"a\""), big)));
    fetch(&cache, &get("a"), POLICY, &t);
    assert_eq!(fetch(&cache, &get("a"), POLICY, &t).source, Source::Network);

    let off = CachePolicy {
        cacheable: false,
        ..POLICY
    };
    t.push_reply(Ok(reply(200, None, "n")));
    t.push_reply(Ok(reply(200, None, "n")));
    fetch(&cache, &get("c"), off, &t);
    assert_eq!(fetch(&cache, &get("c"), off, &t).source, Source::Network);

    let mut no_store = reply(200, None, "n");
    no_store
        .headers
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    t.push_reply(Ok(no_store.clone()));
    t.push_reply(Ok(no_store));
    let mut other = get("d");
    other.url = Url::parse("https://api.github.com/x").unwrap();
    fetch(&cache, &other, POLICY, &t);
    assert_eq!(fetch(&cache, &other, POLICY, &t).source, Source::Network);
}

// Catches: a body-carrying read (Jira `POST search`) cached under a key that
// ignores its body, so a second JQL is answered with the first one's issues.
#[test]
fn read_with_a_body_is_never_cached() {
    let (cache, _clock, t) = setup();
    let mut search = request("a", Method::POST, Effect::Read);
    search.body = Some(Bytes::from_static(b"{\"jql\":\"one\"}"));
    t.push_reply(Ok(reply(200, Some("\"a\""), "one")));
    t.push_reply(Ok(reply(200, Some("\"a\""), "two")));
    fetch(&cache, &search, POLICY, &t);
    search.body = Some(Bytes::from_static(b"{\"jql\":\"two\"}"));
    let second = fetch(&cache, &search, POLICY, &t);
    assert_eq!(second.source, Source::Network);
    assert_eq!(second.body, Bytes::from_static(b"two"));
}
