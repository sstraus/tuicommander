//! Loopback TLS stand-in for `api.github.com`: controlled protocol fixtures, a
//! trace of every request that reached the wire, and switches for the quota
//! headers and for limit answers. Nothing here is captured account data.

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Request,
    http::{Response, StatusCode},
    routing::any,
};
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc, time::Instant};

/// Open issues of the fixture repository: two list pages of 100.
pub(super) const ISSUES: usize = 160;
/// Open pull requests of the fixture repository: one list page.
pub(super) const PULLS: usize = 30;
const PAGE: usize = 100;
const REPO: &str = "/repos/fixture/repository";
// Recorded by openssl req -x509 with a test-only api.github.com SAN.
const CERT: &[u8] = include_bytes!("../fixtures/github_volume/cert.pem");
const KEY: &[u8] = include_bytes!("../fixtures/github_volume/key.pem");

#[derive(Clone, Copy, Default)]
pub(super) struct Mode {
    /// `(limit, remaining)` of every answer; `None` is a free 5,000 bucket.
    pub quota: Option<(u32, u32)>,
    /// Epoch seconds sent as `x-ratelimit-reset`.
    pub reset_epoch: u64,
    /// `(status, retry-after seconds)` that answers every request.
    pub storm: Option<(u16, u64)>,
}

#[derive(Clone)]
pub(super) struct Receiver {
    started: Instant,
    trace: Arc<parking_lot::Mutex<Vec<Value>>>,
    mode: Arc<parking_lot::Mutex<Mode>>,
}

/// One open issue or pull request in the REST shape the trigger source parses.
pub(super) fn resource(pull: bool, number: usize) -> Value {
    let mut value: Value = serde_json::from_str(if pull {
        include_str!("../workflows/fixtures/github-pr-rest.json")
    } else {
        include_str!("../workflows/fixtures/github-issue-rest.json")
    })
    .unwrap();
    value["number"] = json!(number);
    value
}

/// Every open issue, then every open pull request, of the fixture repository.
pub(super) fn open_resources() -> (Vec<Value>, Vec<Value>) {
    (
        (1..=ISSUES).map(|n| resource(false, n)).collect(),
        (1..=PULLS).map(|n| resource(true, 1000 + n)).collect(),
    )
}

fn page_of(all: &[Value], query: &str) -> Vec<Value> {
    let page = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("page="))
        .and_then(|page| page.parse::<usize>().ok())
        .unwrap_or(1);
    all.iter()
        .skip((page.max(1) - 1) * PAGE)
        .take(PAGE)
        .cloned()
        .collect()
}

fn graphql_answer(query: &str) -> Value {
    if query.contains("viewer { login }") {
        json!({"data": {"viewer": {"login": "fixture"}}})
    } else if query.contains("query PRChecks") {
        json!({"data": {"repository": {"pullRequest": {"commits": {"nodes": [
            {"commit": {"statusCheckRollup": {"contexts": {"nodes": [
                {"__typename": "CheckRun", "name": "fixture-check", "status": "COMPLETED", "conclusion": "SUCCESS", "detailsUrl": ""}
            ]}}}}
        ]}}}}})
    } else {
        json!({"data": {
            "r0": {
                "mergeCommitAllowed": true, "squashMergeAllowed": true, "rebaseMergeAllowed": true,
                "pullRequests": {"nodes": [{
                    "number": 42, "title": "Controlled fixture", "state": "OPEN",
                    "url": "https://github.com/fixture/repository/pull/42",
                    "headRefName": "feature", "headRefOid": "a".repeat(40), "baseRefName": "main",
                    "isDraft": false, "author": {"login": "fixture"}, "labels": {"nodes": []},
                    "additions": 1, "deletions": 0, "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
                    "createdAt": "2026-10-18T00:00:00Z", "updatedAt": "2026-10-18T00:00:00Z",
                    "commits": {"totalCount": 1, "nodes": []},
                    "reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": false}}
                }]},
                "issues": {"nodes": []}
            },
            "rateLimit": {"cost": 1, "remaining": 4990, "resetAt": "2099-01-01T00:00:00Z"}
        }})
    }
}

/// The 200 answer of one request: its body, and its ETag when GitHub sends one.
/// The REST lists and items carry an ETag; search and GraphQL do not.
fn answer(path: &str, query: &str, graphql: &str) -> (Option<String>, Value) {
    let (issues, pulls) = open_resources();
    let tagged = |name: &str| Some(format!("\"{name}-{}\"", query.len()));
    match path {
        "/graphql" => (None, graphql_answer(graphql)),
        // Account verification of the plain-HTTP plugin account on the old tree.
        "/identity" => (None, json!({"name": "fixture"})),
        "/search/issues" => (
            None,
            json!({"total_count": ISSUES, "incomplete_results": false, "items": page_of(&issues, query)}),
        ),
        _ if path == format!("{REPO}/issues") => (tagged(query), json!(page_of(&issues, query))),
        _ if path == format!("{REPO}/pulls") => (tagged(query), json!(page_of(&pulls, query))),
        _ => match path
            .strip_prefix(&format!("{REPO}/issues/"))
            .and_then(|number| number.parse::<usize>().ok())
        {
            Some(number) => (Some(format!("\"item-{number}\"")), resource(false, number)),
            None => (None, json!({})),
        },
    }
}

impl Receiver {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            trace: Arc::default(),
            mode: Arc::default(),
        }
    }

    pub(super) fn set(&self, change: impl FnOnce(&mut Mode)) {
        change(&mut self.mode.lock());
    }

    pub(super) fn sends(&self) -> usize {
        self.trace.lock().len()
    }

    /// The requests received since the last call.
    pub(super) fn take(&self) -> Vec<Value> {
        std::mem::take(&mut self.trace.lock())
    }

    /// Every path and method goes to the one recording handler.
    pub(super) fn router(&self) -> Router {
        let endpoint = self.clone();
        Router::new().fallback(any(move |request| endpoint.clone().receive(request)))
    }

    async fn receive(self, request: Request) -> Response<Body> {
        let method = request.method().to_string();
        let path = request.uri().path().to_string();
        let query = request.uri().query().unwrap_or_default().to_string();
        // Scoped: a borrow of the request must not live across an await.
        let (validator, authorized) = {
            let header = |name: &str| {
                request
                    .headers()
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            };
            (header("if-none-match"), header("authorization").is_some())
        };
        let bytes = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
        let graphql = body["query"].as_str().unwrap_or_default();
        let mode = *self.mode.lock();
        let (status, etag, payload) = match mode.storm {
            Some((status, _)) => (
                status,
                None,
                json!({"message": "Controlled rate-limit response"}),
            ),
            None => {
                let (etag, payload) = answer(&path, &query, graphql);
                match (&etag, &validator) {
                    (Some(etag), Some(sent)) if etag == sent => {
                        (304, Some(etag.clone()), json!(null))
                    }
                    _ => (200, etag, payload),
                }
            }
        };
        self.trace.lock().push(json!({
            "elapsed_seconds": self.started.elapsed().as_secs_f64(),
            "method": method,
            "path": path,
            "kind": if path == "/graphql" { "GraphQL" } else { "REST" },
            "conditional": validator.is_some(),
            "authorized": authorized,
            "status": status,
        }));
        let mut response = Response::builder().status(StatusCode::from_u16(status).unwrap());
        if let Some((_, retry_after)) = mode.storm {
            // A secondary limit: `retry-after` and no quota headers.
            response = response.header("retry-after", retry_after);
        } else {
            let (limit, remaining) = mode.quota.unwrap_or((5000, 4990));
            let bucket = match path.as_str() {
                "/graphql" => "graphql",
                "/search/issues" => "search",
                _ => "core",
            };
            response = response
                .header("x-ratelimit-limit", limit)
                .header("x-ratelimit-remaining", remaining)
                .header("x-ratelimit-reset", mode.reset_epoch)
                .header("x-ratelimit-resource", bucket);
        }
        if let Some(etag) = etag {
            response = response.header("etag", etag);
        }
        if path.ends_with("/issues") && query.contains("page=1") && status == 200 {
            let next = format!("<https://api.github.com{path}?page=2>; rel=\"next\"");
            response = response.header("link", next);
        }
        let body = if status == 304 {
            Body::empty()
        } else {
            response = response.header("content-type", "application/json");
            Body::from(payload.to_string())
        };
        response.body(body).unwrap()
    }
}

pub(super) struct Server {
    pub receiver: Receiver,
    address: SocketAddr,
    handle: axum_server::Handle<SocketAddr>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Server {
    pub(super) async fn start() -> Self {
        rustls::crypto::ring::default_provider()
            .install_default()
            .unwrap_or_else(|_| assert!(rustls::crypto::CryptoProvider::get_default().is_some()));
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(CERT.to_vec(), KEY.to_vec())
            .await
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let receiver = Receiver::new();
        let router = receiver.router();
        let handle = axum_server::Handle::new();
        let task = tokio::spawn(
            axum_server::from_tcp_rustls(listener, tls)
                .unwrap()
                .handle(handle.clone())
                .serve(router.into_make_service()),
        );
        Self {
            receiver,
            address,
            handle,
            task,
        }
    }

    /// A client whose `api.github.com` is this receiver. Only the socket target
    /// changes: production URLs, methods, auth headers, parsing and the gateway
    /// stay as they are.
    pub(super) fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .tls_certs_only([reqwest::Certificate::from_pem(CERT).unwrap()])
            .resolve("api.github.com", self.address)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap()
    }

    pub(super) async fn stop(self) {
        self.handle.shutdown();
        self.task.await.unwrap().unwrap();
    }
}
