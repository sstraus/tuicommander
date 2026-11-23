use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures::executor::block_on;
use futures::future::{BoxFuture, FutureExt};
use http::header::{AUTHORIZATION, ETAG, SET_COOKIE};
use http::{HeaderMap, HeaderValue, Method};
use url::Url;

use crate::cache::{Cache, CacheKey, CachePolicy};
use crate::l2::{ColdStore, L2, StoredEntry};
use crate::model::*;
use crate::ports::Clock;
use crate::testing::{FakeClock, FakeTransport};

#[derive(Default)]
struct FakeStore {
    rows: Mutex<HashMap<CacheKey, StoredEntry>>,
    batches: Mutex<Vec<usize>>,
    hang_writes: bool,
}

impl ColdStore for FakeStore {
    fn get(&self, key: &CacheKey, _now: SystemTime) -> BoxFuture<'static, Option<StoredEntry>> {
        let r = self.rows.lock().unwrap().get(key).cloned();
        async move { r }.boxed()
    }
    fn put_batch(&self, rows: Vec<StoredEntry>) -> BoxFuture<'static, ()> {
        if self.hang_writes {
            return futures::future::pending().boxed();
        }
        self.batches.lock().unwrap().push(rows.len());
        let mut m = self.rows.lock().unwrap();
        for r in rows {
            m.insert(r.key.clone(), r);
        }
        async {}.boxed()
    }
    fn delete_batch(&self, keys: Vec<CacheKey>) -> BoxFuture<'static, ()> {
        let mut m = self.rows.lock().unwrap();
        for k in keys {
            m.remove(&k);
        }
        async {}.boxed()
    }
    fn purge_account(&self, account_id: &str) -> BoxFuture<'static, ()> {
        self.rows
            .lock()
            .unwrap()
            .retain(|k, _| k.account.account_id != account_id);
        async {}.boxed()
    }
    fn evict_to(&self, _cap: u64, _idle: Duration, _now: SystemTime) -> BoxFuture<'static, ()> {
        async {}.boxed()
    }
}

fn get(acct: &str, path: &str) -> GatewayRequest {
    GatewayRequest {
        provider: Provider::GitHub,
        account: AccountKey {
            account_id: acct.into(),
            api_base: Url::parse("https://api.github.com").unwrap(),
        },
        method: Method::GET,
        url: Url::parse(&format!("https://api.github.com{path}")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        effect: Effect::Read,
        channel: Channel::Visible,
        freshness: Freshness::Default,
        caller: CallerId("test"),
        graphql: None,
    }
}

fn ok(etag: &str, body: &'static str) -> WireResponse {
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, HeaderValue::from_str(etag).unwrap());
    WireResponse {
        status: 200,
        headers,
        body: Bytes::from_static(body.as_bytes()),
    }
}

const POLICY: CachePolicy = CachePolicy {
    ttl_base: Duration::from_secs(60),
    cacheable: true,
};

fn tier(store: &Arc<FakeStore>, clock: &FakeClock) -> (Cache, Arc<L2>) {
    let l2 = Arc::new(L2::new(store.clone(), Arc::new(clock.clone())));
    (
        Cache::with_l2(Arc::new(clock.clone()), 1 << 20, l2.clone()),
        l2,
    )
}

#[test]
fn l1_miss_reads_l2_body_and_validator_after_restart() {
    let store = Arc::new(FakeStore::default());
    let clock = FakeClock::new();
    let t = FakeTransport::new();
    let (cache, l2) = tier(&store, &clock);
    t.push_reply(Ok(ok("\"v1\"", "payload")));
    block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    block_on(l2.flush());
    drop(cache);

    // A fresh process: empty L1, same store, 90 s later (entry stale).
    clock.advance(Duration::from_secs(90));
    let (cache, _l2) = tier(&store, &clock);
    t.push_reply(Ok(WireResponse {
        status: 304,
        headers: HeaderMap::new(),
        body: Bytes::new(),
    }));
    let r = block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    assert_eq!(r.source, Source::Revalidated);
    assert_eq!(r.body, Bytes::from_static(b"payload"));
    let sent = t.requests();
    assert_eq!(sent[1].headers.get("if-none-match").unwrap(), "\"v1\"");
}

#[test]
fn l2_fresh_entry_after_restart_is_served_without_the_network() {
    let store = Arc::new(FakeStore::default());
    let clock = FakeClock::new();
    let t = FakeTransport::new();
    let (cache, l2) = tier(&store, &clock);
    t.push_reply(Ok(ok("\"v1\"", "payload")));
    block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    block_on(l2.flush());
    clock.advance(Duration::from_secs(10));
    let (cache, _l2) = tier(&store, &clock);
    let r = block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    assert_eq!(r.source, Source::Cache);
    assert_eq!(t.requests().len(), 1);
}

#[test]
fn l2_write_behind_batches_and_never_blocks_the_request_path() {
    let store = Arc::new(FakeStore {
        hang_writes: true,
        ..FakeStore::default()
    });
    let clock = FakeClock::new();
    let (cache, _l2) = tier(&store, &clock);
    let t = FakeTransport::new();
    // The store never completes a write; every fetch must still return.
    for i in 0..10 {
        t.push_reply(Ok(ok("\"e\"", "b")));
        block_on(cache.fetch(&get("a", &format!("/p/{i}")), POLICY, &t)).unwrap();
    }

    let store = Arc::new(FakeStore::default());
    let (cache, l2) = tier(&store, &clock);
    for i in 0..10 {
        t.push_reply(Ok(ok("\"e\"", "b")));
        block_on(cache.fetch(&get("a", &format!("/p/{i}")), POLICY, &t)).unwrap();
    }
    assert!(
        store.batches.lock().unwrap().is_empty(),
        "nothing written before the flush"
    );
    block_on(l2.flush());
    assert_eq!(*store.batches.lock().unwrap(), vec![10]);
    block_on(l2.flush());
    assert_eq!(
        store.batches.lock().unwrap().len(),
        1,
        "an empty queue writes nothing"
    );
}

#[test]
fn l2_invalidated_entry_is_not_resurrected_from_the_store() {
    let store = Arc::new(FakeStore::default());
    let clock = FakeClock::new();
    let (cache, l2) = tier(&store, &clock);
    let t = FakeTransport::new();
    t.push_reply(Ok(ok("\"v1\"", "old")));
    block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    block_on(l2.flush());
    block_on(cache.invalidate(&get("a", "/x")));
    t.push_reply(Ok(ok("\"v2\"", "new")));
    let r = block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    assert_eq!(
        r.body,
        Bytes::from_static(b"new"),
        "pending tombstone hides the stored row"
    );
    block_on(l2.flush());
    assert!(!t.requests()[1].headers.contains_key("if-none-match"));
}

#[test]
fn l2_never_persists_credential_headers_or_credential_urls() {
    let store = Arc::new(FakeStore::default());
    let clock = FakeClock::new();
    let (cache, l2) = tier(&store, &clock);
    let t = FakeTransport::new();
    let mut resp = ok("\"v1\"", "payload");
    resp.headers
        .insert(AUTHORIZATION, HeaderValue::from_static("Bearer ghp_SECRET"));
    resp.headers
        .insert(SET_COOKIE, HeaderValue::from_static("session=SECRET"));
    resp.headers
        .insert("x-access-token", HeaderValue::from_static("SECRET"));
    resp.headers
        .insert("x-ratelimit-remaining", HeaderValue::from_static("42"));
    t.push_reply(Ok(resp));
    block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    t.push_reply(Ok(ok("\"v1\"", "payload")));
    block_on(cache.fetch(&get("a", "/y?access_token=SECRET"), POLICY, &t)).unwrap();
    block_on(l2.flush());

    let rows = store.rows.lock().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "a URL carrying a credential is not persisted"
    );
    let row = rows.values().next().unwrap();
    assert!(
        !row.headers.to_ascii_lowercase().contains("secret"),
        "{}",
        row.headers
    );
    assert!(row.headers.contains("x-ratelimit-remaining: 42"));
    assert!(row.headers.contains("etag"));
}

#[test]
fn l2_stores_wall_clock_times_and_compresses_the_body() {
    let store = Arc::new(FakeStore::default());
    let clock = FakeClock::new();
    let (cache, l2) = tier(&store, &clock);
    let t = FakeTransport::new();
    let mut resp = ok("\"v1\"", "x");
    resp.body = Bytes::from(vec![b'a'; 100_000]);
    t.push_reply(Ok(resp));
    block_on(cache.fetch(&get("a", "/x"), POLICY, &t)).unwrap();
    block_on(l2.flush());
    let rows = store.rows.lock().unwrap();
    let row = rows.values().next().unwrap();
    assert!(row.body_zstd.len() < 1_000);
    assert_eq!(row.fetched_at, clock.wall());
}

#[test]
fn l2_purge_account_drops_stored_and_pending_rows_of_that_account_only() {
    let store = Arc::new(FakeStore::default());
    let clock = FakeClock::new();
    let (cache, l2) = tier(&store, &clock);
    let t = FakeTransport::new();
    for (acct, path) in [("a", "/1"), ("b", "/1")] {
        t.push_reply(Ok(ok("\"e\"", "b")));
        block_on(cache.fetch(&get(acct, path), POLICY, &t)).unwrap();
    }
    block_on(l2.flush());
    t.push_reply(Ok(ok("\"e\"", "b")));
    block_on(cache.fetch(&get("a", "/2"), POLICY, &t)).unwrap(); // pending
    block_on(l2.purge_account("a"));
    block_on(l2.flush());
    let rows = store.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows.keys().next().unwrap().account.account_id, "b");
}
