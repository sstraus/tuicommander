use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use http::header::{AUTHORIZATION, ETAG};
use http::{HeaderMap, HeaderValue, Method};
use tuic_gateway::cache::{Cache, CacheKey, CachePolicy};
use tuic_gateway::l2::{ColdStore, L2, StoredEntry};
use tuic_gateway::model::*;
use tuic_gateway::testing::{FakeClock, FakeTransport};
use url::Url;

use super::SqliteColdStore;

const DAY: Duration = Duration::from_secs(86_400);

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap()
}

fn account(id: &str) -> AccountKey {
    AccountKey {
        account_id: id.into(),
        api_base: Url::parse("https://api.github.com").unwrap(),
    }
}

fn key(acct: &str, path: &str) -> CacheKey {
    CacheKey {
        account: account(acct),
        method: "GET".into(),
        url: format!("https://api.github.com{path}"),
        accept: String::new(),
        graphql: None,
    }
}

fn row(acct: &str, path: &str, body_len: usize, used: SystemTime) -> StoredEntry {
    StoredEntry {
        key: key(acct, path),
        status: 200,
        headers: "etag: \"v1\"\n".into(),
        body_zstd: Bytes::from(vec![7u8; body_len]),
        etag: Some("\"v1\"".into()),
        last_modified: None,
        fetched_at: used,
        ttl_base: Duration::from_secs(60),
        ttl_mult: 1.5,
        last_used: used,
    }
}

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

async fn present(store: &SqliteColdStore, acct: &str, path: &str) -> bool {
    store.get(&key(acct, path), now()).await.is_some()
}

#[tokio::test]
async fn l2_sqlite_row_survives_reopen_with_body_validators_and_wall_times() {
    let dir = temp_dir();
    let path = dir.path().join("provider_cache.sqlite3");
    let store = SqliteColdStore::open(&path).unwrap();
    let mut r = row("a", "/x", 100, now());
    r.last_modified = Some("Sat, 01 Jan 2028 00:00:00 GMT".into());
    store.put_batch(vec![r.clone()]).await;
    drop(store);
    let store = SqliteColdStore::open(&path).unwrap();
    assert_eq!(store.get(&key("a", "/x"), now()).await, Some(r));
}

#[tokio::test]
async fn l2_sqlite_put_replaces_the_row_of_the_same_key() {
    let dir = temp_dir();
    let store = SqliteColdStore::open(&dir.path().join("c.sqlite3")).unwrap();
    store.put_batch(vec![row("a", "/x", 10, now())]).await;
    let mut newer = row("a", "/x", 20, now());
    newer.etag = Some("\"v2\"".into());
    store.put_batch(vec![newer.clone()]).await;
    assert_eq!(store.get(&key("a", "/x"), now()).await, Some(newer));
    store.delete_batch(vec![key("a", "/x")]).await;
    assert!(!present(&store, "a", "/x").await);
}

#[tokio::test]
async fn l2_sqlite_size_cap_evicts_least_recently_used_first() {
    let dir = temp_dir();
    let store = SqliteColdStore::open(&dir.path().join("c.sqlite3")).unwrap();
    let t = |s| now() - Duration::from_secs(s);
    store
        .put_batch(vec![
            row("a", "/old", 1000, t(300)),
            row("a", "/mid", 1000, t(200)),
            row("a", "/new", 1000, t(100)),
        ])
        .await;
    store.get(&key("a", "/old"), t(10)).await; // a read refreshes recency
    store.evict_to(2500, 7 * DAY, now()).await;
    assert!(present(&store, "a", "/old").await);
    assert!(
        !present(&store, "a", "/mid").await,
        "least recently used goes first"
    );
    assert!(present(&store, "a", "/new").await);
}

#[tokio::test]
async fn l2_sqlite_rows_unused_for_seven_days_are_purged() {
    let dir = temp_dir();
    let store = SqliteColdStore::open(&dir.path().join("c.sqlite3")).unwrap();
    store
        .put_batch(vec![
            row("a", "/stale", 10, now() - 8 * DAY),
            row("a", "/live", 10, now() - DAY),
        ])
        .await;
    store.evict_to(u64::MAX, 7 * DAY, now()).await;
    assert!(!present(&store, "a", "/stale").await);
    assert!(present(&store, "a", "/live").await);
}

#[tokio::test]
async fn l2_sqlite_account_logout_purges_its_rows() {
    let dir = temp_dir();
    let store = SqliteColdStore::open(&dir.path().join("c.sqlite3")).unwrap();
    store
        .put_batch(vec![row("a", "/1", 10, now()), row("b", "/1", 10, now())])
        .await;
    store.purge_account("a").await;
    assert!(!present(&store, "a", "/1").await);
    assert!(present(&store, "b", "/1").await);
}

#[tokio::test]
async fn l2_sqlite_schema_version_change_deletes_the_file() {
    let dir = temp_dir();
    let path = dir.path().join("c.sqlite3");
    let store = SqliteColdStore::open(&path).unwrap();
    store.put_batch(vec![row("a", "/1", 10, now())]).await;
    drop(store);
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.pragma_update(None, "user_version", 99).unwrap();
    drop(raw);
    let store = SqliteColdStore::open(&path).unwrap();
    assert!(
        !present(&store, "a", "/1").await,
        "a cache of another layout is dropped, not refused"
    );
    store.put_batch(vec![row("a", "/2", 10, now())]).await;
    assert!(present(&store, "a", "/2").await);
}

#[cfg(unix)]
#[tokio::test]
async fn l2_sqlite_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir();
    let path = dir.path().join("c.sqlite3");
    drop(SqliteColdStore::open(&path).unwrap());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// Catches: a token or Authorization header reaching the cache file.
#[tokio::test]
async fn l2_sqlite_no_token_or_authorization_header_is_ever_written() {
    let dir = temp_dir();
    let path = dir.path().join("provider_cache.sqlite3");
    let clock = FakeClock::new();
    let store = Arc::new(SqliteColdStore::open(&path).unwrap());
    let l2 = Arc::new(L2::new(store, Arc::new(clock.clone())));
    let cache = Cache::with_l2(Arc::new(clock), 1 << 20, l2.clone());
    let t = FakeTransport::new();
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, HeaderValue::from_static("\"v1\""));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer ghp_SECRETTOKEN"),
    );
    headers.insert(
        "x-access-token",
        HeaderValue::from_static("ghp_SECRETTOKEN"),
    );
    headers.insert("x-ratelimit-remaining", HeaderValue::from_static("4242"));
    t.push_reply(Ok(WireResponse {
        status: 200,
        headers,
        body: Bytes::from_static(b"{}"),
    }));
    let req = GatewayRequest {
        provider: Provider::GitHub,
        account: account("a"),
        method: Method::GET,
        url: Url::parse("https://api.github.com/user").unwrap(),
        headers: HeaderMap::new(),
        body: None,
        effect: Effect::Read,
        channel: Channel::Visible,
        freshness: Freshness::Default,
        caller: CallerId("test"),
        graphql: None,
    };
    let policy = CachePolicy {
        ttl_base: Duration::from_secs(60),
        cacheable: true,
    };
    cache.fetch(&req, policy, &t).await.unwrap();
    l2.flush().await;
    drop((cache, l2));

    let mut bytes = std::fs::read(&path).unwrap();
    if let Ok(wal) = std::fs::read(path.with_extension("sqlite3-wal")) {
        bytes.extend(wal);
    }
    let has = |needle: &str| {
        bytes
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
    };
    assert!(
        has("x-ratelimit-remaining"),
        "control: the headers were written"
    );
    assert!(!has("SECRETTOKEN"));
    assert!(!has("authorization"));
    assert!(!has("x-access-token"));
}
