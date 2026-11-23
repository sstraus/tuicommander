//! Sends that do not go through the REST client: the OAuth endpoints, account
//! verification, release and update downloads, CI logs. They are not cacheable
//! API reads, but they still spend a budget, feed the failure memory and show
//! up in the per-caller metrics.
//!
//! A request is never cached: it is sent as `Effect::Write`, which the gateway
//! neither caches nor coalesces. The callers keep what they had before the
//! gateway — their redirect policy and their timeout — as marker headers the
//! transport consumes.

use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::HeaderValue;
use tuic_gateway::model::{
    AccountKey, Effect, Freshness, GatewayError, GatewayRequest, Provider, StreamResponse,
};
use url::Url;

use super::transport::{REDIRECTS, TIMEOUT_MS};
use super::{AppGateway, ProviderGateway};
use crate::github::gateway::{Origin, into_response, refusal_message};

/// The default total timeout of one send, as the shared client's.
const DEFAULT_TIMEOUT: Duration = crate::state::HTTP_REQUEST_TIMEOUT;

static SHARED: OnceLock<Arc<AppGateway>> = OnceLock::new();

/// Builds requests only; the gateway's transport sends them.
static BUILDER: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// The client to build a request with, to hand to `Direct::send` or
/// `Direct::stream`. Sending it would bypass the gateway.
pub(crate) fn builder_client() -> &'static reqwest::Client {
    &BUILDER
}

/// A request to hand to `Direct::send` or `Direct::stream`.
pub(crate) fn builder(
    method: reqwest::Method,
    url: impl reqwest::IntoUrl,
) -> reqwest::RequestBuilder {
    BUILDER.request(method, url)
}

/// The gateway the app registered at start-up, so a call site far from
/// `AppState` shares its budgets. Without one (a unit test that never built an
/// `AppState`) a private gateway answers, never the bare client.
pub(crate) fn shared() -> Arc<AppGateway> {
    SHARED
        .get_or_init(|| {
            ProviderGateway::build(
                crate::state::build_http_client(),
                Arc::new(super::SystemClock),
                None,
            )
            .gateway
        })
        .clone()
}

/// Register the app's gateway; the first one wins.
pub(super) fn register(gateway: &Arc<AppGateway>) {
    let _ = SHARED.set(gateway.clone());
}

#[derive(Clone, Copy)]
enum Redirects {
    Default,
    Never,
    Limited,
}

/// How one send differs from the default: who it is for, whose budget it
/// spends, whether it follows redirects and how long it may take.
#[derive(Clone, Copy)]
pub(crate) struct Direct<'a> {
    origin: Origin,
    provider: Provider,
    account: &'a str,
    redirects: Redirects,
    redact_url: bool,
    timeout: Duration,
}

impl<'a> Direct<'a> {
    pub(crate) fn new(origin: Origin, provider: Provider, account: &'a str) -> Self {
        Self {
            origin,
            provider,
            account,
            redirects: Redirects::Default,
            redact_url: false,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// A credential header must not follow a provider-controlled redirect.
    pub(crate) fn no_redirect(mut self) -> Self {
        self.redirects = Redirects::Never;
        self
    }

    /// Follow at most `transport::LIMITED_REDIRECTS` hops.
    pub(crate) fn limited_redirects(mut self) -> Self {
        self.redirects = Redirects::Limited;
        self
    }

    /// The URL is a bearer credential (a pre-signed link): no failure message
    /// quotes it.
    pub(crate) fn redact_url(mut self) -> Self {
        self.redact_url = true;
        self
    }

    pub(crate) fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, String> {
        self.send_via(&shared(), request).await
    }

    pub(crate) async fn stream(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, String> {
        self.stream_via(&shared(), request).await
    }

    pub(crate) async fn send_via(
        &self,
        gateway: &AppGateway,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, String> {
        let (wire, failure) = self.wire(request)?;
        let response = gateway
            .execute(wire)
            .await
            .map_err(|e| failure.describe(&e))?;
        into_response(response)
    }

    /// Like `send`, with the body left to stream: nothing is buffered or cached.
    pub(crate) async fn stream_via(
        &self,
        gateway: &AppGateway,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, String> {
        let (wire, failure) = self.wire(request)?;
        let opened = gateway
            .execute_stream(wire)
            .await
            .map_err(|e| failure.describe(&e))?;
        Ok(streamed(opened))
    }

    fn wire(&self, request: reqwest::RequestBuilder) -> Result<(GatewayRequest, Failure), String> {
        let request = request
            .build()
            .map_err(|e| format!("Invalid request: {}", e.without_url()))?;
        let url = request.url().clone();
        let failure = Failure {
            label: url.host_str().unwrap_or("remote host").to_owned(),
            url: self.redact_url.then(|| url.to_string()),
        };
        let mut headers = request.headers().clone();
        match self.redirects {
            Redirects::Default => {}
            Redirects::Never => {
                headers.insert(REDIRECTS, HeaderValue::from_static("none"));
            }
            Redirects::Limited => {
                headers.insert(REDIRECTS, HeaderValue::from_static("limited"));
            }
        }
        headers.insert(
            TIMEOUT_MS,
            HeaderValue::from(u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX)),
        );
        let body = request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .map(Bytes::copy_from_slice);
        let mut api_base: Url = url.clone();
        api_base.set_path("/");
        api_base.set_query(None);
        Ok((
            GatewayRequest {
                provider: self.provider,
                account: AccountKey {
                    account_id: self.account.to_owned(),
                    api_base,
                },
                method: request.method().clone(),
                url,
                headers,
                body,
                effect: Effect::Write,
                channel: self.origin.channel,
                freshness: Freshness::Default,
                caller: self.origin.caller,
                graphql: None,
            },
            failure,
        ))
    }
}

/// Words a refusal for the user. A transport error quotes its URL; a send that
/// asked for `redact_url` never lets it reach the message.
struct Failure {
    label: String,
    url: Option<String>,
}

impl Failure {
    fn describe(&self, error: &GatewayError) -> String {
        let message = refusal_message(&self.label, error);
        match &self.url {
            Some(url) => message.replace(url, "<url>"),
            None => message,
        }
    }
}

/// The `Content-Length` a streamed response declared. A streamed body has no
/// exact size hint, so `Response::content_length` cannot answer for it.
pub(crate) fn declared_length(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

/// A `reqwest::Response` over the unread body, so a caller keeps reading it
/// the way it did before the gateway.
fn streamed(opened: StreamResponse) -> reqwest::Response {
    let body = reqwest::Body::wrap_stream(
        opened
            .body
            .map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string()))),
    );
    let mut rebuilt = http::Response::new(body);
    *rebuilt.status_mut() = http::StatusCode::from_u16(opened.status).unwrap_or_default();
    *rebuilt.headers_mut() = opened.headers;
    reqwest::Response::from(rebuilt)
}
