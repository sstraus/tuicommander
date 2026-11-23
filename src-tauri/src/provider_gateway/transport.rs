//! `tuic_gateway::ports::Transport` over the app's shared pooled reqwest client.
//!
//! A request carrying `PLUGIN_EGRESS` goes through a second pooled client
//! instead: redirects off, because the plugin egress follows and re-admits each
//! hop itself, and a streamed body cap at the host maximum. A request carrying
//! `REDIRECTS` goes through a client with redirects off (a credential header
//! must not follow a provider-controlled redirect) or capped at
//! `LIMITED_REDIRECTS`; `TIMEOUT_MS` sets one
//! request's total timeout. None of the markers reaches the wire.

use std::time::Duration;

use bytes::Bytes;
use futures::future::{BoxFuture, FutureExt};
use futures_util::StreamExt;
use http::HeaderName;
use tuic_gateway::model::{L1_ONLY, TransportError, WireRequest, WireResponse, WireStream};
use tuic_gateway::ports::Transport;

use super::plugin_egress::{MAX_RESPONSE_BYTES, MAX_TIMEOUT};

/// Marks a request for the plugin-egress client; removed before the wire.
pub(crate) const PLUGIN_EGRESS: HeaderName = HeaderName::from_static("x-tuic-plugin-egress");

/// Redirect policy of one request, `none` or `limited`; removed before the wire.
pub(crate) const REDIRECTS: HeaderName = HeaderName::from_static("x-tuic-redirects");

/// The hops a `limited` request follows: the updater and the installers always
/// allowed five.
pub(crate) const LIMITED_REDIRECTS: usize = 5;

/// Total timeout of one request in milliseconds; removed before the wire.
pub(crate) const TIMEOUT_MS: HeaderName = HeaderName::from_static("x-tuic-timeout-ms");

const EGRESS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct ReqwestTransport {
    client: reqwest::Client,
    egress: reqwest::Client,
    no_redirect: reqwest::Client,
    limited: reqwest::Client,
}

impl ReqwestTransport {
    pub(crate) fn new(client: reqwest::Client) -> Self {
        let egress = reqwest::Client::builder()
            .connect_timeout(EGRESS_CONNECT_TIMEOUT)
            .timeout(MAX_TIMEOUT)
            // GitHub refuses REST requests without a User-Agent (403). A plugin
            // header still overrides this default per request.
            .user_agent(concat!("tuicommander-plugin/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build plugin egress HTTP client");
        let no_redirect = reqwest::Client::builder()
            .connect_timeout(crate::state::HTTP_CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build no-redirect HTTP client");
        let limited = reqwest::Client::builder()
            .connect_timeout(crate::state::HTTP_CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(LIMITED_REDIRECTS))
            .build()
            .expect("Failed to build limited-redirect HTTP client");
        Self {
            client,
            egress,
            no_redirect,
            limited,
        }
    }

    /// The request to send and whether it is a plugin-egress one, with every
    /// marker header consumed.
    fn prepare(&self, mut req: WireRequest) -> (reqwest::RequestBuilder, bool) {
        req.headers.remove(&L1_ONLY);
        let egress = req.headers.remove(&PLUGIN_EGRESS).is_some();
        let redirects = req.headers.remove(&REDIRECTS);
        let timeout = req
            .headers
            .remove(&TIMEOUT_MS)
            .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
            .map(Duration::from_millis);
        let client = if egress {
            &self.egress
        } else if redirects.as_ref().is_some_and(|v| v == "none") {
            &self.no_redirect
        } else if redirects.is_some() {
            &self.limited
        } else {
            &self.client
        };
        let mut builder = client.request(req.method, req.url).headers(req.headers);
        if let Some(body) = req.body {
            builder = builder.body(body);
        }
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        (builder, egress)
    }
}

/// An error with its `source()` chain, so the cause of a failed send (a refused
/// connection, a TLS failure) is not lost behind reqwest's generic text.
fn describe(e: &(dyn std::error::Error + 'static)) -> String {
    let mut msg = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    msg
}

fn classify(error: &reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else if error.is_connect() {
        TransportError::Connect(describe(error))
    } else {
        TransportError::Other(describe(error))
    }
}

/// Collect a body, stopping one byte past the cap. The plugin egress rejects a
/// body over the plugin's own limit; an oversized body is the plugin's limit
/// being met, not a failing host, so the transport reports no error for it.
async fn read_capped(response: reqwest::Response) -> Result<Bytes, TransportError> {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.map_err(|e| classify(&e))?);
        if body.len() > MAX_RESPONSE_BYTES {
            body.truncate(MAX_RESPONSE_BYTES + 1);
            break;
        }
    }
    Ok(body.into())
}

impl Transport for ReqwestTransport {
    fn send(&self, req: WireRequest) -> BoxFuture<'static, Result<WireResponse, TransportError>> {
        let (builder, egress) = self.prepare(req);
        async move {
            let response = builder.send().await.map_err(|e| classify(&e))?;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let body = if egress {
                read_capped(response).await?
            } else {
                response.bytes().await.map_err(|e| classify(&e))?
            };
            Ok(WireResponse {
                status,
                headers,
                body,
            })
        }
        .boxed()
    }

    fn send_stream(
        &self,
        req: WireRequest,
    ) -> BoxFuture<'static, Result<WireStream, TransportError>> {
        let (builder, _) = self.prepare(req);
        async move {
            let response = builder.send().await.map_err(|e| classify(&e))?;
            Ok(WireStream {
                status: response.status().as_u16(),
                headers: response.headers().clone(),
                body: response
                    .bytes_stream()
                    .map(|chunk| chunk.map_err(|e| classify(&e)))
                    .boxed(),
            })
        }
        .boxed()
    }
}
