//! Plugin egress: admission and the send path of `plugin_http`.
//!
//! `admit` applies the plugin perimeter in a fixed order: capability `net:http`,
//! the manifest from disk, owner destinations, then either the account path
//! (provider consent, credential, host binding, `Authorization` override block)
//! or the credentialless path (scheme, private-host declaration, `allowed_urls`).
//! Only an admitted request reaches `Gateway::execute`, so budgets, cache and
//! coalescing never run ahead of admission.
//!
//! The send path uses a pooled client with redirects off. This module follows
//! each redirect itself and admits every hop under the same rules.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use http::header::{
    AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION,
    PROXY_AUTHORIZATION, TRANSFER_ENCODING, WWW_AUTHENTICATE,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use tuic_gateway::model::{
    AccountKey, CallerId, Channel, Effect, Freshness, GatewayError, GatewayRequest,
    GatewayResponse, L1_ONLY, Provider, Source,
};
use url::Url;

use super::transport::PLUGIN_EGRESS;

use crate::plugin_http::HttpResponse;
use crate::plugin_http::account::{self, AccountAuth};

/// Request header a plugin sets to ask for a fresh read; removed in `admit`.
const FRESHNESS: &str = "x-tuic-freshness";

#[path = "plugin_egress_rules.rs"]
pub(crate) mod rules;

/// Host maximum response body (10 MiB).
pub(crate) const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Host maximum request time and the default.
pub(crate) const MAX_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum redirect hops to follow.
const MAX_REDIRECTS: usize = 5;

/// Label of every plugin request in gateway metrics.
const CALLER: CallerId = CallerId("plugin");

/// Bucket owner of a request that carries no account.
const ANONYMOUS: &str = "plugin-anonymous";

/// Limits a plugin may ask for; the host maxima cap them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FetchOptions {
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) max_response_bytes: Option<u64>,
}

impl FetchOptions {
    pub(crate) fn timeout(self) -> Duration {
        self.timeout_ms
            .map_or(MAX_TIMEOUT, |ms| Duration::from_millis(ms).min(MAX_TIMEOUT))
    }

    pub(crate) fn max_bytes(self) -> usize {
        self.max_response_bytes.map_or(MAX_RESPONSE_BYTES, |bytes| {
            usize::try_from(bytes).map_or(MAX_RESPONSE_BYTES, |b| b.min(MAX_RESPONSE_BYTES))
        })
    }
}

/// What a plugin asks for.
pub(crate) struct EgressRequest {
    pub(crate) url: String,
    pub(crate) method: Option<String>,
    pub(crate) headers: Option<HashMap<String, String>>,
    pub(crate) body: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) plugin_id: String,
    pub(crate) options: FetchOptions,
    pub(crate) declared: Declared,
}

/// What a host call site says about itself. A plugin cannot: only host code
/// builds this. None keeps the method-and-credential rule and the plugin label.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Declared {
    pub(crate) effect: Option<Effect>,
    pub(crate) caller: Option<CallerId>,
    pub(crate) freshness: Option<Freshness>,
}

impl Declared {
    /// A workflow connector: its reads decide what an effect does next, and
    /// reconcile observes a write that just landed, so they never take a cached
    /// answer (the gateway still limits and coalesces them).
    pub(crate) fn workflow(read: bool, caller: &'static str) -> Self {
        Self {
            effect: Some(if read { Effect::Read } else { Effect::Write }),
            caller: Some(CallerId(caller)),
            freshness: Some(Freshness::Force),
        }
    }
}

/// A request that passed admission, with the data its redirect hops need.
struct Admitted {
    url: Url,
    method: Method,
    headers: HeaderMap,
    body: Option<Bytes>,
    allowed_urls: Vec<String>,
    owner_urls: Vec<String>,
    auth: Option<AccountAuth>,
    effect: Option<Effect>,
    caller: CallerId,
    freshness: Freshness,
}

fn parse_url(url: &str) -> Result<Url, String> {
    Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))
}

/// The admission checks, in order. Any refusal stops the request before the
/// gateway sees it.
async fn admit(state: &Arc<crate::AppState>, request: EgressRequest) -> Result<Admitted, String> {
    let plugin_id = &request.plugin_id;
    crate::plugins::check_plugin_capability(state, plugin_id, "net:http")?;

    // The on-disk manifest is the source of truth, never a caller-supplied
    // list: otherwise a scoped plugin could widen its own allowlist.
    let manifest = crate::plugins::read_single_manifest(plugin_id)?;
    // Owner configuration, never a request field: the plugin cannot widen it.
    let owner_urls = state.config.read().plugin_http_destinations.clone();
    // An account request is bound by the account host rule instead: the user
    // chose that site when they created the account (a LAN Jira DC included).
    let auth = match request.account.as_deref() {
        Some(id) => Some(account::resolve(state, &manifest, id).await?),
        None => None,
    };
    match &auth {
        Some(auth) => auth.check_url(&request.url)?,
        None => rules::validate_destination(&request.url, &manifest.allowed_urls, &owner_urls)?,
    }

    let method_str = request.method.as_deref().unwrap_or("GET");
    let method: Method = method_str
        .parse()
        .map_err(|_| format!("Invalid HTTP method: {method_str}"))?;
    let mut headers = HeaderMap::new();
    for (key, value) in request.headers.iter().flatten() {
        if auth.is_some() && key.eq_ignore_ascii_case("authorization") {
            return Err("An account request must not set Authorization".into());
        }
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| format!("Invalid HTTP header name: {key}"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| format!("Invalid value for HTTP header {key}"))?;
        headers.insert(name, value);
    }
    if let Some(auth) = &auth {
        headers.insert(AUTHORIZATION, auth.header());
    }
    // A TUIC-internal request, never a provider header: it asks the cache for a
    // fresh read and is removed before the request goes out.
    let requested = match headers.remove(FRESHNESS) {
        Some(value) if value.as_bytes().eq_ignore_ascii_case(b"force") => Freshness::Force,
        _ => Freshness::Default,
    };
    let freshness = request.declared.freshness.unwrap_or(requested);
    Ok(Admitted {
        url: parse_url(&request.url)?,
        method,
        headers,
        body: request.body.map(Bytes::from),
        allowed_urls: manifest.allowed_urls,
        owner_urls,
        auth,
        effect: request.declared.effect,
        caller: request.declared.caller.unwrap_or(CALLER),
        freshness,
    })
}

/// Make one plugin request: admit it, then send it through the gateway.
pub(crate) async fn fetch(
    state: &Arc<crate::AppState>,
    request: EgressRequest,
) -> Result<HttpResponse, String> {
    let options = request.options;
    let admitted = admit(state, request).await?;
    let gateway = &state.gateway.gateway;
    tokio::time::timeout(
        options.timeout(),
        admitted.send(gateway, options.max_bytes()),
    )
    .await
    .map_err(|_| "HTTP request failed: timed out".to_string())?
}

/// One answer of the gateway, or the synthetic 429 that stands for its refusal.
enum Reply {
    Wire(GatewayResponse),
    Limited(SystemTime),
}

fn provider_of(name: &str) -> Provider {
    match name {
        "github" => Provider::GitHub,
        "gitlab" => Provider::GitLab,
        "jira" => Provider::Jira,
        _ => Provider::Generic,
    }
}

/// Same origin as `url`, nothing else: the bucket of an account-less request.
fn origin_of(url: &Url) -> Url {
    let mut origin = url.clone();
    origin.set_path("/");
    origin.set_query(None);
    origin.set_fragment(None);
    let _ = origin.set_username("");
    let _ = origin.set_password(None);
    origin
}

fn cross_host(next: &Url, previous: &Url) -> bool {
    next.host_str() != previous.host_str()
        || next.port_or_known_default() != previous.port_or_known_default()
        || next.scheme() != previous.scheme()
}

impl Admitted {
    /// A redirect hop obeys the rule the first URL passed: under the account
    /// base for an account request, else the credentialless destination rule.
    fn admit_hop(&self, next: &Url) -> Result<(), String> {
        if self
            .auth
            .as_ref()
            .is_some_and(|auth| account::is_under(next, &auth.base()))
        {
            return Ok(());
        }
        rules::validate_destination(next.as_str(), &self.allowed_urls, &self.owner_urls)
    }

    async fn send(
        mut self,
        gateway: &super::AppGateway,
        max_bytes: usize,
    ) -> Result<HttpResponse, String> {
        let mut credentialed = self.auth.is_some();
        for hop in 0..=MAX_REDIRECTS {
            let reply = self.call(gateway, credentialed).await?;
            let Reply::Wire(response) = &reply else {
                return self.answer(reply, max_bytes);
            };
            let Some(next) = redirect_target(response, &self.url) else {
                return self.answer(reply, max_bytes);
            };
            if hop == MAX_REDIRECTS {
                return Err("HTTP request failed: too many redirects".into());
            }
            self.admit_hop(&next)
                .map_err(|e| format!("HTTP request failed: error following redirect: {e}"))?;
            if cross_host(&next, &self.url) {
                for name in [AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, WWW_AUTHENTICATE] {
                    self.headers.remove(name);
                }
                self.headers.remove("cookie2");
                credentialed = false;
            }
            self.rewrite_for(response.status);
            self.url = next;
        }
        unreachable!("the last hop returns above")
    }

    /// A 301 or 302 answer to a POST, and a 303 answer to anything but HEAD,
    /// becomes a bodiless GET; 307 and 308 repeat the request.
    fn rewrite_for(&mut self, status: u16) {
        let to_get = match status {
            301 | 302 => self.method == Method::POST,
            303 => self.method != Method::HEAD,
            _ => false,
        };
        if to_get {
            self.method = Method::GET;
            self.body = None;
            for name in [
                CONTENT_TYPE,
                CONTENT_LENGTH,
                CONTENT_ENCODING,
                TRANSFER_ENCODING,
            ] {
                self.headers.remove(name);
            }
        }
    }

    async fn call(&self, gateway: &super::AppGateway, credentialed: bool) -> Result<Reply, String> {
        let (provider, account) = match (&self.auth, credentialed) {
            (Some(auth), true) => (
                provider_of(auth.provider()),
                AccountKey {
                    account_id: auth.account_id().to_owned(),
                    api_base: auth.base(),
                },
            ),
            _ => (
                Provider::Generic,
                AccountKey {
                    account_id: ANONYMOUS.into(),
                    api_base: origin_of(&self.url),
                },
            ),
        };
        // Only a credentialed read is cached: the cache key does not cover
        // caller-chosen headers, so another request's headers could be served.
        let cacheable = credentialed && matches!(self.method, Method::GET | Method::HEAD);
        let mut headers = self.headers.clone();
        headers.insert(PLUGIN_EGRESS, HeaderValue::from_static("1"));
        // A body may echo the credential, which only `answer` redacts: keep it
        // out of the persistent tier.
        headers.insert(L1_ONLY, HeaderValue::from_static("1"));
        let request = GatewayRequest {
            provider,
            account,
            method: self.method.clone(),
            url: self.url.clone(),
            headers,
            body: self.body.clone(),
            effect: self.effect.unwrap_or(if cacheable {
                Effect::Read
            } else {
                Effect::Write
            }),
            channel: Channel::Interactive,
            freshness: self.freshness,
            caller: self.caller,
            graphql: None,
        };
        match gateway.execute(request).await {
            Ok(response) => Ok(Reply::Wire(response)),
            Err(
                GatewayError::RateLimited { retry_at, .. } | GatewayError::Shed { retry_at, .. },
            ) => Ok(Reply::Limited(retry_at)),
            Err(GatewayError::Transport(message)) => Err(format!("HTTP request failed: {message}")),
            Err(GatewayError::Cancelled) => Err("HTTP request cancelled".into()),
        }
    }

    /// The plugin's view of an answer: the credential redacted from headers and
    /// body, the cache source named.
    fn answer(&self, reply: Reply, max_bytes: usize) -> Result<HttpResponse, String> {
        let redact = |text: String| match &self.auth {
            Some(auth) => auth.redact(text),
            None => text,
        };
        let response = match reply {
            Reply::Wire(response) => response,
            Reply::Limited(retry_at) => {
                let wait = retry_at
                    .duration_since(SystemTime::now())
                    .unwrap_or_default()
                    .as_secs_f64()
                    .ceil()
                    .max(1.0);
                return Ok(HttpResponse {
                    status: 429,
                    headers: HashMap::from([
                        ("x-tuic-gateway".into(), "rate_limited".into()),
                        ("retry-after".into(), format!("{wait:.0}")),
                    ]),
                    body: String::new(),
                });
            }
        };
        if response.body.len() > max_bytes {
            return Err(format!(
                "Response body exceeds maximum size (> {max_bytes} bytes)"
            ));
        }
        let mut headers: HashMap<String, String> = response
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), redact(v.to_str().unwrap_or("").to_string())))
            .collect();
        let source = match response.source {
            Source::Network => None,
            Source::Cache => Some("hit"),
            Source::Revalidated => Some("revalidated"),
            Source::Stale => Some("stale"),
        };
        if let Some(source) = source {
            headers.insert("x-tuic-cache".into(), source.into());
        }
        Ok(HttpResponse {
            status: response.status,
            headers,
            body: redact(String::from_utf8_lossy(&response.body).to_string()),
        })
    }
}

/// The URL a redirect answer points at; none for any other answer, or for a
/// redirect without a usable `Location`, which the plugin then sees as is.
fn redirect_target(response: &GatewayResponse, from: &Url) -> Option<Url> {
    if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    let location = response.headers.get(LOCATION)?.to_str().ok()?;
    from.join(location).ok()
}
