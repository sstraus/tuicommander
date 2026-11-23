//! Shared types for every gateway module. Later steps import these; they do
//! not redefine them.

use std::time::SystemTime;

use bytes::Bytes;
use futures::stream::BoxStream;
use http::{HeaderMap, HeaderName, Method};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    GitHub,
    GitLab,
    Jira,
    Generic,
}

/// One credential identity on one API host.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountKey {
    pub account_id: String,
    pub api_base: Url,
}

/// Rate-limit bucket name within a provider (`core`, `search`, `graphql`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BucketId(pub String);

/// Caller priority (R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Interactive,
    Visible,
    Background,
}

/// Explicit: a Jira `POST search/jql` is a `Read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    Read,
    Write,
}

/// `Force` is the refresh button (R5); it never bypasses an exhausted bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Freshness {
    Default,
    Force,
    CacheOnly,
}

/// Static label of the call site: `"tasks.list"`, `"plugin:github-work"`, ...
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallerId(pub &'static str);

/// Identity of a cacheable GraphQL read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GraphqlRead {
    pub query_hash: String,
    /// Canonical (key-sorted) JSON of the variables.
    pub variables: String,
}

/// Quota pressure, ordered from free to exhausted (R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pressure {
    Free,
    Watch,
    Tight,
    Critical,
    Exhausted,
}

/// Why a provider refused a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitReason {
    Primary,
    Secondary,
    Burst,
    Quota,
    PerIssue,
    Unknown,
}

/// Bucket state attached to a response.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetSnapshot {
    pub bucket: BucketId,
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<SystemTime>,
    pub pressure: Pressure,
}

/// A request carrying this header is cached in memory only, never in the
/// persistent tier. The adapter strips it before the wire.
pub const L1_ONLY: HeaderName = HeaderName::from_static("x-tuic-l1-only");

#[derive(Debug, Clone)]
pub struct GatewayRequest {
    pub provider: Provider,
    pub account: AccountKey,
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub effect: Effect,
    pub channel: Channel,
    pub freshness: Freshness,
    pub caller: CallerId,
    pub graphql: Option<GraphqlRead>,
}

impl GatewayRequest {
    /// A read whose cache key and flight identify it. The key covers no body
    /// except a GraphQL read's identity, so any other read that carries a body
    /// (a Jira `POST search`) is neither cached nor coalesced.
    #[must_use]
    pub fn identified_read(&self) -> bool {
        self.effect == Effect::Read && (self.body.is_none() || self.graphql.is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Network,
    Cache,
    Revalidated,
    Stale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GatewayResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub source: Source,
    pub budget: BudgetSnapshot,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum GatewayError {
    /// The provider said no.
    #[error("rate limited on {bucket:?} until {retry_at:?} ({reason:?})")]
    RateLimited {
        bucket: BucketId,
        retry_at: SystemTime,
        reason: LimitReason,
    },
    /// The gateway said "not now".
    #[error("shed on {bucket:?} at {level:?}, retry at {retry_at:?}")]
    Shed {
        bucket: BucketId,
        retry_at: SystemTime,
        level: Pressure,
    },
    #[error("transport: {0}")]
    Transport(String),
    #[error("cancelled")]
    Cancelled,
}

/// What actually crosses the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct WireRequest {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WireResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
}

/// A response whose body has not been read yet.
pub struct WireStream {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: BoxStream<'static, Result<Bytes, TransportError>>,
}

/// What `execute_stream` hands back: the unread body and the bucket state.
pub struct StreamResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: BoxStream<'static, Result<Bytes, TransportError>>,
    pub budget: BudgetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

impl From<TransportError> for GatewayError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e.to_string())
    }
}
