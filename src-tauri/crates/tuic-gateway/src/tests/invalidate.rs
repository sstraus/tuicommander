use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::executor::block_on;
use http::{HeaderMap, Method};
use url::Url;

use crate::cache::{Cache, CachePolicy};
use crate::invalidate::Signal;
use crate::model::*;
use crate::routes::{Tag, classify};
use crate::testing::{FakeClock, FakeTransport};

const POLICY: CachePolicy = CachePolicy {
    ttl_base: Duration::from_secs(60),
    cacheable: true,
};

fn account(id: &str, base: &str) -> AccountKey {
    AccountKey {
        account_id: id.into(),
        api_base: Url::parse(base).unwrap(),
    }
}

fn gh(id: &str) -> AccountKey {
    account(id, "https://api.github.com")
}

fn request(acct: &AccountKey, method: Method, path: &str, freshness: Freshness) -> GatewayRequest {
    let url = acct.api_base.join(path).unwrap();
    let route = classify(Provider::GitHub, acct, &method, &url);
    GatewayRequest {
        provider: Provider::GitHub,
        account: acct.clone(),
        method,
        url,
        headers: HeaderMap::new(),
        body: None,
        effect: route.effect,
        channel: Channel::Visible,
        freshness,
        caller: CallerId("test"),
        graphql: None,
    }
}

fn setup() -> Cache {
    Cache::new(Arc::new(FakeClock::new()), 1 << 20)
}

fn cache_it(cache: &Cache, acct: &AccountKey, path: &str) {
    let req = request(acct, Method::GET, path, Freshness::Default);
    let route = classify(Provider::GitHub, acct, &req.method, &req.url);
    let t = FakeTransport::new();
    t.push_reply(Ok(WireResponse {
        status: 200,
        headers: HeaderMap::new(),
        body: Bytes::from_static(b"x"),
    }));
    block_on(cache.fetch_tagged(&req, POLICY, &route, &t)).unwrap();
}

fn is_cached(cache: &Cache, acct: &AccountKey, path: &str) -> bool {
    let req = request(acct, Method::GET, path, Freshness::CacheOnly);
    block_on(cache.fetch(&req, POLICY, &FakeTransport::new())).is_ok()
}

fn write_signal(acct: &AccountKey, method: Method, path: &str) -> Signal {
    let req = request(acct, method, path, Freshness::Default);
    let route = classify(Provider::GitHub, acct, &req.method, &req.url);
    Signal::from_write(acct.clone(), &route)
}

#[test]
fn patch_issue_invalidates_item_list_and_search_of_that_repo_only() {
    let cache = setup();
    let a = gh("a");
    let other_acct = gh("b");
    let item = "/repos/o/r/issues/5";
    let list = "/repos/o/r/issues";
    let search = "/search/issues?q=x";
    let other_item = "/repos/o/r/issues/6";
    let other_repo = "/repos/o/other/issues";
    for p in [item, list, search, other_item, other_repo] {
        cache_it(&cache, &a, p);
    }
    cache_it(&cache, &other_acct, search);
    cache_it(&cache, &other_acct, list);

    block_on(cache.apply(&write_signal(&a, Method::PATCH, item)));

    assert!(!is_cached(&cache, &a, item), "item stays stale");
    assert!(!is_cached(&cache, &a, list), "list stays stale");
    assert!(!is_cached(&cache, &a, search), "search stays stale");
    assert!(is_cached(&cache, &a, other_item), "sibling item wiped");
    assert!(is_cached(&cache, &a, other_repo), "other repo wiped");
    assert!(
        is_cached(&cache, &other_acct, search),
        "other account wiped"
    );
    assert!(is_cached(&cache, &other_acct, list), "other account wiped");
}

#[test]
fn signal_invalidates_repo_collection_tags() {
    let cache = setup();
    let a = gh("a");
    let (list, pulls, item, other) = (
        "/repos/o/r/issues",
        "/repos/o/r/pulls",
        "/repos/o/r/issues/5",
        "/repos/o/x/issues",
    );
    for p in [list, pulls, item, other, "/search/issues?q=x"] {
        cache_it(&cache, &a, p);
    }

    block_on(cache.apply(&Signal::RepoChanged {
        account: a.clone(),
        tags: vec![Tag::repo("a", "O", "R")],
    }));

    assert!(!is_cached(&cache, &a, list));
    assert!(!is_cached(&cache, &a, pulls));
    assert!(is_cached(&cache, &a, item), "item is not a collection tag");
    assert!(is_cached(&cache, &a, other));
    assert!(is_cached(&cache, &a, "/search/issues?q=x"), "search kept");
}

#[test]
fn logout_drops_account_entries() {
    let cache = setup();
    let (a, b) = (gh("a"), gh("b"));
    let ghes = account("a", "https://ghe.example/api/v3/");
    for p in ["/repos/o/r/issues", "/search/issues?q=x", "/user"] {
        cache_it(&cache, &a, p);
        cache_it(&cache, &b, p);
    }
    cache_it(&cache, &ghes, "/repos/o/r/issues");

    block_on(cache.apply(&Signal::Logout { account: a.clone() }));

    for p in ["/repos/o/r/issues", "/search/issues?q=x", "/user"] {
        assert!(!is_cached(&cache, &a, p));
        assert!(is_cached(&cache, &b, p), "other account wiped");
    }
    assert!(
        is_cached(&cache, &ghes, "/repos/o/r/issues"),
        "other host wiped"
    );
}

#[test]
fn invalidate_is_idempotent_and_reindexes_after_refetch() {
    let cache = setup();
    let a = gh("a");
    let item = "/repos/o/r/issues/5";
    cache_it(&cache, &a, item);
    let sig = write_signal(&a, Method::PATCH, item);
    block_on(cache.apply(&sig));
    block_on(cache.apply(&sig));
    cache_it(&cache, &a, item);
    assert!(is_cached(&cache, &a, item));
    block_on(cache.apply(&sig));
    assert!(
        !is_cached(&cache, &a, item),
        "re-cached entry lost its tags"
    );
}
