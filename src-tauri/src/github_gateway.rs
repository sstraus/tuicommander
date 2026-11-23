//! The GitHub client's one path to the wire. Every REST and GraphQL send goes
//! through `AppState.gateway`, so one budget per (account, bucket) sees all
//! traffic and a limit learned on one path blocks the others.

use bytes::Bytes;
use reqwest::header::HeaderMap;
use tuic_gateway::invalidate::Signal;
use tuic_gateway::model::{
    AccountKey, CallerId, Channel, Effect, Freshness, GatewayError, GatewayRequest,
    GatewayResponse, Provider,
};
use tuic_gateway::ports::SignalSink;
use tuic_gateway::routes::Tag;
use url::Url;

use crate::github::BranchPrStatus;
use crate::github_account::GitHubAccount;
use crate::state::{AppState, GitCache, GitCacheState};

/// Buckets that pace the poller's cadence.
const POLLER_BUCKETS: [&str; 2] = ["graphql", "core"];

/// Who sends a request and how urgent it is. Every send names one, so the
/// gateway's per-caller view and its load shedding see real traffic.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Origin {
    pub caller: CallerId,
    pub channel: Channel,
}

impl Origin {
    /// A user action: never shed.
    pub(crate) const fn interactive(caller: &'static str) -> Self {
        Self {
            caller: CallerId(caller),
            channel: Channel::Interactive,
        }
    }

    /// A read a visible panel waits on.
    pub(crate) const fn visible(caller: &'static str) -> Self {
        Self {
            caller: CallerId(caller),
            channel: Channel::Visible,
        }
    }

    /// Polling and monitoring: the first traffic shed under pressure.
    pub(crate) const fn background(caller: &'static str) -> Self {
        Self {
            caller: CallerId(caller),
            channel: Channel::Background,
        }
    }
}

pub(super) fn account_key(account: &GitHubAccount) -> Result<AccountKey, String> {
    let api_base = Url::parse(&account.host.rest_base())
        .map_err(|e| format!("Invalid GitHub API base for {}: {e}", account.host.as_str()))?;
    Ok(AccountKey {
        account_id: account.id.clone(),
        api_base,
    })
}

/// One request through the gateway. A GET or HEAD is a cacheable read; every
/// other method, and every GraphQL POST, is a write that never caches.
pub(super) async fn execute(
    state: &AppState,
    account: &GitHubAccount,
    origin: Origin,
    method: reqwest::Method,
    url: Url,
    headers: HeaderMap,
    body: Option<Bytes>,
) -> Result<GatewayResponse, GatewayError> {
    let effect = if matches!(method, reqwest::Method::GET | reqwest::Method::HEAD) {
        Effect::Read
    } else {
        Effect::Write
    };
    let account = account_key(account).map_err(GatewayError::Transport)?;
    state
        .gateway
        .gateway
        .execute(GatewayRequest {
            provider: Provider::GitHub,
            account,
            method,
            url,
            headers,
            body,
            effect,
            channel: origin.channel,
            freshness: Freshness::Default,
            caller: origin.caller,
            graphql: None,
        })
        .await
}

/// The typed refusal the UI reads, identical over IPC and HTTP: `rate-limit:`
/// followed by `{kind, bucket, retry_at (epoch ms), message}`. The prefix stays
/// first so the poller and the breaker keep classifying on it.
fn limit_message(
    kind: &str,
    bucket: &str,
    retry_at: std::time::SystemTime,
    message: String,
) -> String {
    let retry_at = retry_at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let payload = serde_json::json!({
        "kind": kind,
        "bucket": bucket,
        "retry_at": retry_at,
        "message": message,
    });
    format!("rate-limit:{payload}")
}

/// The user-facing text of a refusal. A limit or a shed starts with
/// `rate-limit:`, the classification the poller and the UI key on.
pub(super) fn error_message(error: &GatewayError) -> String {
    match error {
        GatewayError::Transport(message) => format!("GitHub API request failed: {message}"),
        GatewayError::Cancelled => "GitHub API request cancelled".to_string(),
        limit => refusal_message("GitHub", limit),
    }
}

/// The text of a limit, a shed or a failure, naming `label` as the provider.
pub(crate) fn refusal_message(label: &str, error: &GatewayError) -> String {
    let wait = |at: std::time::SystemTime| {
        at.duration_since(std::time::SystemTime::now())
            .map_or(0, |d| d.as_secs())
    };
    match error {
        GatewayError::RateLimited {
            bucket, retry_at, ..
        } => limit_message(
            "rate_limited",
            &bucket.0,
            *retry_at,
            format!(
                "{label} {} limit reached, retry in {}s",
                bucket.0,
                wait(*retry_at)
            ),
        ),
        GatewayError::Shed {
            bucket,
            retry_at,
            level,
        } => limit_message(
            "shed",
            &bucket.0,
            *retry_at,
            format!(
                "{label} {} quota is {level:?}, retry in {}s",
                bucket.0,
                wait(*retry_at)
            ),
        ),
        GatewayError::Transport(message) => format!("{label} request failed: {message}"),
        GatewayError::Cancelled => format!("{label} request cancelled"),
    }
}

/// Hand a gateway response back as the `reqwest::Response` the REST callers
/// already read their own status and body from.
pub(crate) fn into_response(response: GatewayResponse) -> Result<reqwest::Response, String> {
    let mut rebuilt = axum::http::Response::builder()
        .status(response.status)
        .body(reqwest::Body::from(response.body))
        .map_err(|e| format!("Failed to rebuild GitHub response: {e}"))?;
    *rebuilt.headers_mut() = response.headers;
    Ok(reqwest::Response::from(rebuilt))
}

/// The most constrained quota the poller must pace for: the `graphql` and
/// `core` buckets of every account. `u32::MAX` means no constraint is known.
pub(super) fn min_rate_budget(state: &AppState) -> u32 {
    state
        .gateway
        .gateway
        .bucket_snapshots()
        .iter()
        .filter(|b| POLLER_BUCKETS.contains(&b.bucket.0.as_str()))
        .filter_map(|b| b.remaining)
        .min()
        .unwrap_or(u32::MAX)
}

/// The account's GraphQL bucket is closed: a poll of it would only be refused.
pub(super) fn graphql_closed(state: &AppState, account: &GitHubAccount) -> bool {
    use tuic_gateway::model::Pressure;
    state.gateway.gateway.bucket_snapshots().iter().any(|b| {
        b.account == account.id && b.bucket.0 == "graphql" && b.pressure == Pressure::Exhausted
    })
}

/// A bucket of any account is refusing sends, with a status line for the UI.
pub(crate) fn backoff_status(state: &AppState) -> Option<String> {
    use tuic_gateway::model::Pressure;
    state
        .gateway
        .gateway
        .bucket_snapshots()
        .into_iter()
        .find(|b| b.pressure == Pressure::Exhausted)
        .map(|b| {
            format!(
                "rate-limit: GitHub {} bucket of {} is backing off",
                b.bucket.0, b.account
            )
        })
}

/// A new login may be another identity: forget the account's limits and cache.
pub(crate) async fn reset_account(state: &AppState, account: &GitHubAccount) {
    if let Ok(key) = account_key(account) {
        state.gateway.gateway.reset_account(&key).await;
    }
}

/// Cache the parsed PR status of `path` and remember which repository tag it
/// derives from, so a write the gateway sees can drop it.
pub(crate) fn cache_status(
    state: &AppState,
    account: &GitHubAccount,
    owner: &str,
    repo: &str,
    path: &str,
    statuses: Vec<BranchPrStatus>,
) {
    let tag = Tag::repo(&account.id, owner, repo);
    state
        .git_cache
        .github_status_repos
        .insert(path.to_string(), tag.as_str().to_string());
    state
        .git_cache
        .github_status
        .insert(path.to_string(), std::sync::Arc::new(statuses));
}

/// `github_status` is the poller's parsed view of PRs, not an HTTP response, so
/// the gateway cache cannot own it. A successful write drops the entries of the
/// repositories it touched: the next read re-parses instead of showing the
/// state from before the write.
pub(crate) struct StatusInvalidator {
    status: GitCache<Vec<BranchPrStatus>>,
    repos: std::sync::Arc<dashmap::DashMap<String, String>>,
}

impl StatusInvalidator {
    pub(crate) fn new(cache: &GitCacheState) -> Self {
        Self {
            status: cache.github_status.clone(),
            repos: cache.github_status_repos.clone(),
        }
    }
}

impl SignalSink for StatusInvalidator {
    fn on_signal(&self, signal: &Signal) {
        let Signal::Wrote { tags, .. } = signal else {
            return;
        };
        let stale: Vec<String> = self
            .repos
            .iter()
            .filter(|e| tags.iter().any(|t| t.as_str() == e.value()))
            .map(|e| e.key().clone())
            .collect();
        for path in stale {
            self.status.invalidate(&path);
            self.repos.remove(&path);
        }
    }
}
