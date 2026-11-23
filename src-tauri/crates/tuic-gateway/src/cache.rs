//! L1 response cache: in memory, weighted by body bytes, with conditional
//! revalidation and an adaptive TTL.
//!
//! `CacheKey` and `Entry` hold only owned, plain data (the account is part of
//! the key) so the L2 store can persist them unchanged. L1 holds bodies
//! uncompressed; compression belongs to L2 (step 2.2b).

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::header::{ACCEPT, CACHE_CONTROL, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use http::{HeaderMap, HeaderValue};
use moka::future::Cache as Moka;
use moka::notification::RemovalCause;
use url::Url;

use crate::invalidate::TagIndex;
use crate::l2::L2;
use crate::model::{
    AccountKey, Freshness, GatewayError, GatewayRequest, GraphqlRead, L1_ONLY, Source, WireRequest,
    WireResponse,
};
use crate::ports::{Clock, Transport};
use crate::routes::Route;

/// Bodies above this size are never cached.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Each unchanged revalidation multiplies the TTL by this factor...
pub const TTL_GROWTH: f64 = 1.5;
/// ...up to this multiple of the base TTL.
pub const MAX_TTL_MULT: f64 = 8.0;
/// An entry nobody reads for this long is evicted.
const TIME_TO_IDLE: Duration = Duration::from_secs(24 * 3600);

/// Route-table verdict for one request, passed in as plain data.
#[derive(Debug, Clone, Copy)]
pub struct CachePolicy {
    pub ttl_base: Duration,
    pub cacheable: bool,
}

/// Everything that makes two reads the same read. The account is part of the
/// identity, so an entry can never be served to another account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub account: AccountKey,
    pub method: String,
    /// URL with its query pairs sorted and its fragment dropped.
    pub url: String,
    pub accept: String,
    pub graphql: Option<GraphqlRead>,
}

impl CacheKey {
    #[must_use]
    pub fn new(req: &GatewayRequest) -> Self {
        Self {
            account: req.account.clone(),
            method: req.method.to_string(),
            url: canonical_url(&req.url),
            accept: req
                .headers
                .get(ACCEPT)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned(),
            graphql: req.graphql.clone(),
        }
    }
}

fn canonical_url(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    let mut pairs: Vec<(String, String)> = u
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.is_empty() {
        u.set_query(None);
    } else {
        pairs.sort();
        u.query_pairs_mut().clear().extend_pairs(pairs);
    }
    u.into()
}

/// One cached response. `fetched_at` is monotonic time (L2 will persist wall
/// time and translate on load).
#[derive(Debug, Clone)]
pub struct Entry {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fetched_at: Instant,
    pub ttl_base: Duration,
    pub ttl_mult: f64,
}

impl Entry {
    pub(crate) fn is_fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.fetched_at) < self.ttl_base.mul_f64(self.ttl_mult)
    }
}

/// A served response plus what the caller needs for budget accounting.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub source: Source,
    /// False when no quota was spent: a cache hit or a revalidation 304.
    pub billable: bool,
}

pub struct Cache {
    clock: Arc<dyn Clock>,
    pub(crate) entries: Moka<CacheKey, Arc<Entry>>,
    pub(crate) index: Arc<TagIndex>,
    pub(crate) l2: Option<Arc<L2>>,
}

impl Cache {
    /// `max_bytes` caps the summed body size.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, max_bytes: u64) -> Self {
        let index = Arc::new(TagIndex::default());
        let evicted = Arc::clone(&index);
        let entries = Moka::builder()
            .max_capacity(max_bytes)
            .weigher(|_, e: &Arc<Entry>| u32::try_from(e.body.len()).unwrap_or(u32::MAX))
            .time_to_idle(TIME_TO_IDLE)
            .eviction_listener(move |k: Arc<CacheKey>, _, cause| {
                if matches!(cause, RemovalCause::Size | RemovalCause::Expired) {
                    evicted.remove(&k);
                }
            })
            .build();
        Self {
            clock,
            entries,
            index,
            l2: None,
        }
    }

    /// Like `new`, with a persistent tier consulted on an L1 miss.
    #[must_use]
    pub fn with_l2(clock: Arc<dyn Clock>, max_bytes: u64, l2: Arc<L2>) -> Self {
        Self {
            l2: Some(l2),
            ..Self::new(clock, max_bytes)
        }
    }

    pub async fn invalidate(&self, req: &GatewayRequest) {
        self.drop_key(&CacheKey::new(req)).await;
    }

    pub(crate) async fn drop_key(&self, key: &CacheKey) {
        self.index.remove(key);
        self.entries.invalidate(key).await;
        if let Some(l2) = &self.l2 {
            l2.remove(key);
        }
    }

    async fn store(&self, key: CacheKey, e: Entry, persist: bool) {
        if let (true, Some(l2)) = (persist, &self.l2) {
            l2.put(&key, &e);
        }
        self.entries.insert(key, Arc::new(e)).await;
    }

    /// L1 first; on a miss, L2 supplies the body and validators of an entry
    /// stored before a restart and warms L1 with it.
    async fn lookup(&self, key: &CacheKey) -> Option<Arc<Entry>> {
        if let Some(e) = self.entries.get(key).await {
            return Some(e);
        }
        let e = Arc::new(self.l2.as_ref()?.get(key).await?);
        self.entries.insert(key.clone(), e.clone()).await;
        Some(e)
    }

    /// The stored entry of a read, warming L1 from L2: what the facade needs
    /// for admission and stale-if-error. An entry served from here never
    /// passes through `fetch_tagged`, so it is indexed here: a later `Signal`
    /// must find an entry that came back from L2.
    pub(crate) async fn peek(&self, req: &GatewayRequest, route: &Route) -> Option<Arc<Entry>> {
        if !req.identified_read() {
            return None;
        }
        let key = CacheKey::new(req);
        let entry = self.lookup(&key).await?;
        self.index.add(&key, route);
        Some(entry)
    }

    /// TTL multiplier of the stored entry, if any.
    pub async fn entry_ttl_mult(&self, req: &GatewayRequest) -> Option<f64> {
        self.entries
            .get(&CacheKey::new(req))
            .await
            .map(|e| e.ttl_mult)
    }

    /// Serve from cache when fresh, revalidate when stale, else go to the
    /// transport. A 304 counts as a revalidation only when this call sent the
    /// validator itself; any other 304 is returned as a plain status.
    pub async fn fetch<T: Transport>(
        &self,
        req: &GatewayRequest,
        policy: CachePolicy,
        transport: &T,
    ) -> Result<Fetched, GatewayError> {
        self.fetch_indexed(req, policy, None, transport).await
    }

    /// `fetch`, indexing a stored entry under the route's tags so a `Signal`
    /// can find it.
    pub async fn fetch_tagged<T: Transport>(
        &self,
        req: &GatewayRequest,
        policy: CachePolicy,
        route: &Route,
        transport: &T,
    ) -> Result<Fetched, GatewayError> {
        self.fetch_indexed(req, policy, Some(route), transport)
            .await
    }

    async fn fetch_indexed<T: Transport>(
        &self,
        req: &GatewayRequest,
        policy: CachePolicy,
        route: Option<&Route>,
        transport: &T,
    ) -> Result<Fetched, GatewayError> {
        let cacheable = policy.cacheable && req.identified_read();
        let persist = !req.headers.contains_key(L1_ONLY);
        let key = CacheKey::new(req);
        let entry = if cacheable {
            self.lookup(&key).await
        } else {
            None
        };
        // An entry warmed from L2 after a restart has no tag index yet.
        if let (Some(route), Some(_)) = (route, &entry) {
            self.index.add(&key, route);
        }

        match (req.freshness, &entry) {
            (Freshness::CacheOnly, Some(e)) => return Ok(served(e, Source::Cache, false)),
            (Freshness::CacheOnly, None) => {
                return Err(GatewayError::Transport("cache miss".into()));
            }
            (Freshness::Default, Some(e)) if e.is_fresh(self.clock.now()) => {
                return Ok(served(e, Source::Cache, false));
            }
            _ => {}
        }

        let mut headers = req.headers.clone();
        let validator_sent = entry
            .as_deref()
            .is_some_and(|e| add_validator(&mut headers, e));
        let resp = transport
            .send(WireRequest {
                method: req.method.clone(),
                url: req.url.clone(),
                headers,
                body: req.body.clone(),
            })
            .await?;

        if resp.status == 304
            && validator_sent
            && let Some(old) = &entry
        {
            let e = self.refreshed(old, policy, &resp);
            self.store(key, e.clone(), persist).await;
            return Ok(served(&e, Source::Revalidated, false));
        }
        if cacheable {
            if resp.status == 200 && storable(&resp) {
                let e = self.new_entry(&resp, policy, entry.as_deref());
                if let Some(route) = route {
                    self.index.add(&key, route);
                }
                self.store(key, e, persist).await;
            } else if entry.is_some() {
                self.drop_key(&key).await;
            }
        }
        Ok(Fetched {
            status: resp.status,
            headers: resp.headers,
            body: resp.body,
            source: Source::Network,
            billable: true,
        })
    }

    /// A 304 carries no new body: keep the stored one, grow the TTL.
    fn refreshed(&self, old: &Entry, policy: CachePolicy, resp: &WireResponse) -> Entry {
        Entry {
            fetched_at: self.clock.now(),
            ttl_base: policy.ttl_base,
            ttl_mult: grow(old.ttl_mult),
            etag: header_str(&resp.headers, &ETAG).or_else(|| old.etag.clone()),
            last_modified: header_str(&resp.headers, &LAST_MODIFIED)
                .or_else(|| old.last_modified.clone()),
            ..old.clone()
        }
    }

    fn new_entry(&self, resp: &WireResponse, policy: CachePolicy, old: Option<&Entry>) -> Entry {
        let unchanged = old.is_some_and(|o| o.body == resp.body);
        Entry {
            status: resp.status,
            headers: resp.headers.clone(),
            body: resp.body.clone(),
            etag: header_str(&resp.headers, &ETAG),
            last_modified: header_str(&resp.headers, &LAST_MODIFIED),
            fetched_at: self.clock.now(),
            ttl_base: policy.ttl_base,
            ttl_mult: match old {
                Some(o) if unchanged => grow(o.ttl_mult),
                _ => 1.0,
            },
        }
    }
}

fn grow(mult: f64) -> f64 {
    (mult * TTL_GROWTH).min(MAX_TTL_MULT)
}

pub(crate) fn served(e: &Entry, source: Source, billable: bool) -> Fetched {
    Fetched {
        status: e.status,
        headers: e.headers.clone(),
        body: e.body.clone(),
        source,
        billable,
    }
}

fn storable(resp: &WireResponse) -> bool {
    resp.body.len() <= MAX_BODY_BYTES
        && !header_str(&resp.headers, &CACHE_CONTROL)
            .is_some_and(|v| v.to_ascii_lowercase().contains("no-store"))
}

fn header_str(h: &HeaderMap, name: &http::header::HeaderName) -> Option<String> {
    h.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned)
}

/// Adds the stored validator unless the caller set its own; true when this
/// call is the one sending a validator for the stored entry.
fn add_validator(headers: &mut HeaderMap, e: &Entry) -> bool {
    if headers.contains_key(IF_NONE_MATCH) || headers.contains_key(IF_MODIFIED_SINCE) {
        return false;
    }
    let (name, value) = match (&e.etag, &e.last_modified) {
        (Some(v), _) => (IF_NONE_MATCH, v),
        (None, Some(v)) => (IF_MODIFIED_SINCE, v),
        (None, None) => return false,
    };
    match HeaderValue::from_str(value) {
        Ok(v) => {
            headers.insert(name, v);
            true
        }
        Err(_) => false,
    }
}
