//! L2: the persistent, compressed second tier behind L1. The port
//! (`ColdStore`) is implemented by a SQLite adapter in the app crate.
//! Writes are queued and flushed in batches off the request path; wall-clock
//! times are stored, never `Instant`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{HeaderMap, HeaderName, HeaderValue};

use crate::cache::{CacheKey, Entry, MAX_BODY_BYTES};
use crate::ports::Clock;

/// One persisted row. The body is zstd-compressed; `headers` holds
/// `name: value` lines with credential headers already removed.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEntry {
    pub key: CacheKey,
    pub status: u16,
    pub headers: String,
    pub body_zstd: Bytes,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fetched_at: SystemTime,
    pub ttl_base: Duration,
    pub ttl_mult: f64,
    pub last_used: SystemTime,
}

/// Persistent store port. `now` is the caller's wall clock.
pub trait ColdStore: Send + Sync + 'static {
    /// Reads a row and marks it used at `now`.
    fn get(&self, key: &CacheKey, now: SystemTime) -> BoxFuture<'static, Option<StoredEntry>>;
    fn put_batch(&self, rows: Vec<StoredEntry>) -> BoxFuture<'static, ()>;
    fn delete_batch(&self, keys: Vec<CacheKey>) -> BoxFuture<'static, ()>;
    fn purge_account(&self, account_id: &str) -> BoxFuture<'static, ()>;
    /// Drops rows idle longer than `max_idle`, then least recently used rows
    /// until the stored bytes fit `cap_bytes`.
    fn evict_to(
        &self,
        cap_bytes: u64,
        max_idle: Duration,
        now: SystemTime,
    ) -> BoxFuture<'static, ()>;
}

/// Disk budget for the persistent tier.
pub const CAP_BYTES: u64 = 256 * 1024 * 1024;
/// A row nobody reads for this long is purged.
pub const MAX_IDLE: Duration = Duration::from_secs(7 * 24 * 3600);
/// How often the background task writes the queue.
pub const FLUSH_EVERY: Duration = Duration::from_secs(2);
/// The background task runs eviction once per this many flushes.
const EVICT_EVERY_N_FLUSHES: u32 = 150;
/// The write queue is best-effort: past these bounds new writes are dropped.
const MAX_PENDING_ROWS: usize = 1024;
const MAX_PENDING_BYTES: usize = 16 * 1024 * 1024;
const ZSTD_LEVEL: i32 = 3;

/// Header and query-parameter names that may carry a credential.
const SECRET_MARKERS: [&str; 7] = [
    "auth",
    "cookie",
    "token",
    "secret",
    "key",
    "password",
    "credential",
];

fn looks_secret(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|m| n.contains(m))
}

/// A URL that carries a credential is never persisted.
fn url_has_credential(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return true;
    };
    !u.username().is_empty()
        || u.password().is_some()
        || u.query_pairs().any(|(k, _)| looks_secret(&k))
}

fn encode_headers(headers: &HeaderMap) -> String {
    let mut out = String::new();
    for (name, value) in headers {
        if looks_secret(name.as_str()) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            out.push_str(name.as_str());
            out.push_str(": ");
            out.push_str(v);
            out.push('\n');
        }
    }
    out
}

fn decode_headers(text: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    for line in text.lines() {
        if let Some((n, v)) = line.split_once(": ")
            && let (Ok(n), Ok(v)) = (
                HeaderName::from_bytes(n.as_bytes()),
                HeaderValue::from_str(v),
            )
        {
            map.append(n, v);
        }
    }
    map
}

struct Row {
    key: CacheKey,
    entry: Entry,
    fetched_at: SystemTime,
}

#[derive(Default)]
struct Pending {
    generation: u64,
    /// `None` is a tombstone: the key was invalidated.
    rows: HashMap<CacheKey, (u64, Option<Row>)>,
}

impl Pending {
    fn bump(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }
}

pub struct L2 {
    store: Arc<dyn ColdStore>,
    clock: Arc<dyn Clock>,
    pending: Mutex<Pending>,
    /// Serialises flushes with purges, so a flush cannot re-add a purged row.
    io: tokio::sync::Mutex<()>,
}

impl L2 {
    #[must_use]
    pub fn new(store: Arc<dyn ColdStore>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store,
            clock,
            pending: Mutex::default(),
            io: tokio::sync::Mutex::new(()),
        }
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues a write; never touches the store.
    pub fn put(&self, key: &CacheKey, entry: &Entry) {
        if url_has_credential(&key.url) {
            return;
        }
        let age = self.clock.now().saturating_duration_since(entry.fetched_at);
        let fetched_at = self
            .clock
            .wall()
            .checked_sub(age)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut p = self.pending();
        let queued: usize = p
            .rows
            .values()
            .filter_map(|(_, r)| r.as_ref())
            .map(|r| r.entry.body.len())
            .sum();
        if p.rows.len() >= MAX_PENDING_ROWS || queued + entry.body.len() > MAX_PENDING_BYTES {
            return;
        }
        let generation = p.bump();
        let row = Row {
            key: key.clone(),
            entry: entry.clone(),
            fetched_at,
        };
        p.rows.insert(key.clone(), (generation, Some(row)));
    }

    /// Queues a delete; the key stops resolving at once.
    pub fn remove(&self, key: &CacheKey) {
        let mut p = self.pending();
        let generation = p.bump();
        p.rows.insert(key.clone(), (generation, None));
    }

    pub async fn get(&self, key: &CacheKey) -> Option<Entry> {
        if let Some((_, row)) = self.pending().rows.get(key) {
            return row.as_ref().map(|r| r.entry.clone());
        }
        let stored = self.store.get(key, self.clock.wall()).await?;
        self.decode(stored)
    }

    fn decode(&self, s: StoredEntry) -> Option<Entry> {
        let body = zstd::bulk::decompress(&s.body_zstd, MAX_BODY_BYTES).ok()?;
        let age = self
            .clock
            .wall()
            .duration_since(s.fetched_at)
            .unwrap_or_default();
        Some(Entry {
            status: s.status,
            headers: decode_headers(&s.headers),
            body: Bytes::from(body),
            etag: s.etag,
            last_modified: s.last_modified,
            fetched_at: self.clock.now().checked_sub(age)?,
            ttl_base: s.ttl_base,
            ttl_mult: s.ttl_mult,
        })
    }

    fn encode(&self, row: &Row) -> Option<StoredEntry> {
        let e = &row.entry;
        let body_zstd = zstd::bulk::compress(&e.body, ZSTD_LEVEL).ok()?;
        Some(StoredEntry {
            key: row.key.clone(),
            status: e.status,
            headers: encode_headers(&e.headers),
            body_zstd: Bytes::from(body_zstd),
            etag: e.etag.clone(),
            last_modified: e.last_modified.clone(),
            fetched_at: row.fetched_at,
            ttl_base: e.ttl_base,
            ttl_mult: e.ttl_mult,
            last_used: self.clock.wall(),
        })
    }

    /// Writes the queue as one delete batch and one put batch.
    pub async fn flush(&self) {
        let _io = self.io.lock().await;
        let mut seen = Vec::new();
        let mut deletes = Vec::new();
        let mut rows = Vec::new();
        {
            let p = self.pending();
            for (key, (generation, row)) in &p.rows {
                seen.push((key.clone(), *generation));
                match row {
                    Some(r) => rows.extend(self.encode(r)),
                    None => deletes.push(key.clone()),
                }
            }
        }
        if seen.is_empty() {
            return;
        }
        if !deletes.is_empty() {
            self.store.delete_batch(deletes).await;
        }
        if !rows.is_empty() {
            self.store.put_batch(rows).await;
        }
        let mut p = self.pending();
        for (key, generation) in seen {
            if p.rows.get(&key).is_some_and(|(g, _)| *g == generation) {
                p.rows.remove(&key);
            }
        }
    }

    /// Account logout: forget its queued and stored rows.
    pub async fn purge_account(&self, account_id: &str) {
        let _io = self.io.lock().await;
        self.pending()
            .rows
            .retain(|k, _| k.account.account_id != account_id);
        self.store.purge_account(account_id).await;
    }

    pub async fn evict(&self, cap_bytes: u64, max_idle: Duration) {
        self.store
            .evict_to(cap_bytes, max_idle, self.clock.wall())
            .await;
    }

    /// Flushes every `FLUSH_EVERY`, and enforces the size cap and the idle
    /// limit at start and every `EVICT_EVERY_N_FLUSHES` flushes.
    pub fn spawn_maintenance(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = 0u32;
            loop {
                if tick.is_multiple_of(EVICT_EVERY_N_FLUSHES) {
                    this.evict(CAP_BYTES, MAX_IDLE).await;
                }
                tick = tick.wrapping_add(1);
                tokio::time::sleep(FLUSH_EVERY).await;
                this.flush().await;
            }
        })
    }
}
