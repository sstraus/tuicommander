//! Browser-origin and DNS-rebinding checks for the TCP listener, before auth.

use super::auth::is_private_ip;
#[cfg(test)]
use super::build_router;
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

const APP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost",
    "http://127.0.0.1",
    "http://127.0.0.1:1421",
    "http://localhost:1421",
];

/// This machine's own names (lowercase): the hostname and its `.local` form. A
/// MagicDNS short name is the hostname, so peers reach the daemon by it.
fn own_hostnames() -> Vec<String> {
    #[cfg(unix)]
    let raw = {
        let mut buf = [0u8; 256];
        // SAFETY: buf is valid for buf.len() bytes; gethostname NUL-terminates on success.
        let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
        ok.then(|| {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            String::from_utf8_lossy(&buf[..end]).into_owned()
        })
    };
    #[cfg(not(unix))]
    let raw = std::env::var("COMPUTERNAME").ok();
    let Some(raw) = raw else {
        return Vec::new();
    };
    let host = raw.trim().trim_end_matches(".local").to_ascii_lowercase();
    if host.is_empty() {
        return Vec::new();
    }
    vec![format!("{host}.local"), host]
}

/// Rejection log keys kept before the set restarts; bounds memory against a
/// client that varies its Host or Origin on every request.
const MAX_LOGGED_REJECTIONS: usize = 256;

/// Longest Host or Origin value written to the log.
const MAX_LOGGED_VALUE: usize = 200;

pub(super) struct RequestBoundary {
    local_ips: Vec<String>,
    state: Arc<AppState>,
    logged: parking_lot::Mutex<HashSet<(&'static str, String, String)>>,
}

/// An Origin is the page's scheme and authority, with the scheme's default port
/// left out. A Host header may carry that port, and hosts compare ignoring case.
fn same_origin_authority(origin: &str, host: &str) -> bool {
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    let default_port = match scheme {
        "http" => ":80",
        "https" => ":443",
        _ => return false,
    };
    let bare = |authority: &str| {
        let authority = authority.to_ascii_lowercase();
        authority
            .strip_suffix(default_port)
            .map_or_else(|| authority.clone(), str::to_string)
    };
    bare(authority) == bare(host)
}

/// A top-level page load. The browser sets these headers itself and a page
/// cannot read the response of a navigation, so a link from another site or
/// app may open the mobile app.
fn is_document_navigation(req: &Request<axum::body::Body>) -> bool {
    let header_is = |name: &str, value: &str| req.headers().get(name).is_some_and(|v| v == value);
    matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) && header_is("sec-fetch-mode", "navigate")
        && header_is("sec-fetch-dest", "document")
}

fn loggable(value: Option<&HeaderValue>) -> String {
    let text = value.map_or_else(
        || "-".to_string(),
        |v| v.to_str().unwrap_or("<non-ascii>").to_string(),
    );
    format!(
        "{:?}",
        text.chars().take(MAX_LOGGED_VALUE).collect::<String>()
    )
}

impl RequestBoundary {
    pub(super) fn new(state: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            local_ips: crate::get_local_ips_impl(&state)
                .into_iter()
                .map(|e| e.ip)
                .collect(),
            state,
            logged: Default::default(),
        })
    }

    fn allowed_host(&self, headers: &HeaderMap, uri: &Uri) -> Option<String> {
        // Multiple Host headers and malformed authorities must not be interpreted
        // differently by a proxy and by our handler. Never trust forwarded headers.
        // HTTP/2 carries the authority in the URI and sends no Host header.
        let host = match headers.get_all(header::HOST).iter().count() {
            0 => uri.authority()?.as_str(),
            1 => headers.get(header::HOST)?.to_str().ok()?,
            _ => return None,
        };
        let authority = host.parse::<axum::http::uri::Authority>().ok()?;
        if authority.as_str().contains('@') {
            return None;
        }
        let name = authority
            .host()
            .trim_start_matches('[')
            .trim_end_matches(']');
        let allowed = name.eq_ignore_ascii_case("localhost")
            || name
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback() || is_private_ip(&ip))
            || self.local_ips.iter().any(|ip| ip == name)
            // Read per request: macOS changes the hostname with the network.
            || own_hostnames().iter().any(|h| name.eq_ignore_ascii_case(h))
            || matches!(&*self.state.tailscale_state.read(),
                crate::tailscale::TailscaleState::Running { fqdn, .. }
                    if name.eq_ignore_ascii_case(fqdn)
                        || fqdn
                            .split('.')
                            .next()
                            .is_some_and(|short| name.eq_ignore_ascii_case(short)));
        allowed.then(|| host.to_string())
    }

    fn allowed_origin(&self, origin: &HeaderValue, host: &str) -> bool {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        APP_ORIGINS.contains(&origin) || same_origin_authority(origin, host)
    }

    /// Log a rejection once per (reason, host, origin): enough to name the
    /// failing client, never the URL, so a `?token=` cannot reach the log.
    fn log_rejection(&self, reason: &'static str, req: &Request<axum::body::Body>) {
        // HTTP/2 carries the authority in the URI and sends no Host header.
        let host = match req.headers().get(header::HOST) {
            Some(host) => loggable(Some(host)),
            None => format!("{:?}", req.uri().authority().map(|a| a.as_str())),
        };
        let origin = loggable(req.headers().get(header::ORIGIN));
        {
            let mut logged = self.logged.lock();
            if logged.len() >= MAX_LOGGED_REJECTIONS {
                logged.clear();
            }
            if !logged.insert((reason, host.clone(), origin.clone())) {
                return;
            }
        }
        tracing::warn!(
            source = "mcp_http",
            reason,
            host,
            origin,
            sec_fetch_site = loggable(req.headers().get("sec-fetch-site")),
            sec_fetch_mode = loggable(req.headers().get("sec-fetch-mode")),
            peer = ?req.extensions().get::<axum::extract::ConnectInfo<SocketAddr>>().map(|c| c.0),
            "Request rejected before auth"
        );
    }

    pub(super) fn cors(self: &Arc<Self>) -> CorsLayer {
        let boundary = Arc::clone(self);
        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(move |origin, parts| {
                origin
                    .to_str()
                    .is_ok_and(|origin| APP_ORIGINS.contains(&origin))
                    || boundary
                        .allowed_host(&parts.headers, &parts.uri)
                        .is_some_and(|host| boundary.allowed_origin(origin, &host))
            }))
            .allow_credentials(true)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::PATCH,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
    }
}

pub(super) async fn check(
    State(boundary): State<Arc<RequestBoundary>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(host) = boundary.allowed_host(req.headers(), req.uri()) else {
        boundary.log_rejection("untrusted-host", &req);
        return (StatusCode::FORBIDDEN, "Untrusted Host").into_response();
    };
    let origins = req.headers().get_all(header::ORIGIN);
    let reason = if origins.iter().count() > 1 {
        Some("multiple-origins")
    } else if origins
        .iter()
        .any(|origin| !boundary.allowed_origin(origin, &host))
    {
        Some("origin-not-host")
    } else if req
        .headers()
        .get("sec-fetch-site")
        .is_some_and(|site| site == "cross-site")
        && !req
            .headers()
            .get(header::ORIGIN)
            .and_then(|origin| origin.to_str().ok())
            .is_some_and(|origin| APP_ORIGINS.contains(&origin))
        && (!is_document_navigation(&req) || req.headers().contains_key(header::ORIGIN))
    {
        Some("cross-site")
    } else {
        None
    };
    if let Some(reason) = reason {
        boundary.log_rejection(reason, &req);
        return (StatusCode::FORBIDDEN, "Untrusted Origin").into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn state() -> Arc<AppState> {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.session_token.write() = "boundary-token".into();
        state
    }

    fn request(
        path: &str,
        host: &str,
        origin: Option<&str>,
        token: bool,
        peer: [u8; 4],
    ) -> Request<Body> {
        let uri = if token {
            format!("{path}?token=boundary-token")
        } else {
            path.into()
        };
        let mut req = Request::post(uri)
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json")
            .extension(ConnectInfo(SocketAddr::from((peer, 12345))))
            .body(Body::from("{}"))
            .unwrap();
        if let Some(origin) = origin {
            req.headers_mut()
                .insert(header::ORIGIN, origin.parse().unwrap());
        }
        req
    }

    /// Catches a drive-by page reaching privileged handlers even with a valid token.
    #[tokio::test]
    async fn foreign_origin_cannot_reach_debug_or_session_write() {
        let app = super::super::build_router(state(), true, true);
        for path in ["/debug/invoke_js", "/sessions/missing/write"] {
            let response = app
                .clone()
                .oneshot(request(
                    path,
                    "127.0.0.1:9876",
                    Some("https://evil.example"),
                    true,
                    [127, 0, 0, 1],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        }
    }

    /// Catches DNS rebinding bypassing origin checks with an attacker-controlled Host.
    #[tokio::test]
    async fn rebinding_host_cannot_reach_debug_or_session_write() {
        let app = super::super::build_router(state(), true, true);
        for path in ["/debug/invoke_js", "/sessions/missing/write"] {
            let response = app
                .clone()
                .oneshot(request(
                    path,
                    "attacker.example:9876",
                    Some("http://attacker.example:9876"),
                    true,
                    [127, 0, 0, 1],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        }
    }

    /// Catches #1535: the daemon answering 403 "Untrusted Host" to its own hostname
    /// (how a desktop reaches it over MagicDNS), while a foreign name stays rejected.
    #[tokio::test]
    async fn own_hostname_is_trusted_and_foreign_name_is_not() {
        let hostname = own_hostnames().pop().expect("machine has a hostname");
        let app = super::super::build_remote_router(state());
        for (host, expected) in [
            (format!("{hostname}:9877"), StatusCode::OK),
            (format!("{hostname}.local:9877"), StatusCode::OK),
            ("attacker.example:9877".to_string(), StatusCode::FORBIDDEN),
        ] {
            let req = Request::get("/health")
                .header(header::HOST, &host)
                .extension(ConnectInfo(SocketAddr::from(([100, 64, 0, 3], 12345))))
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                expected,
                "{host}"
            );
        }
    }

    /// Catches TCP loopback or the legacy LAN preference granting unauthenticated writes.
    #[tokio::test]
    async fn local_http_still_requires_credentials() {
        let state = state();
        state.config.write().services.auth.lan_auth_bypass = true;
        let app = super::super::build_router(state, true, true);
        for peer in [[127, 0, 0, 1], [192, 168, 1, 12]] {
            for path in ["/debug/invoke_js", "/sessions/missing/write"] {
                let response = app
                    .clone()
                    .oneshot(request(path, "127.0.0.1:9876", None, false, peer))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::UNAUTHORIZED,
                    "{path}: {peer:?}"
                );
            }
        }
    }

    /// Catches hardening breaking the existing WebView, native HTTP and remote PWA clients.
    #[tokio::test]
    async fn legitimate_clients_with_token_reach_real_handlers() {
        let app = super::super::build_router(state(), true, true);
        for (origin, host, peer) in [
            (Some("tauri://localhost"), "127.0.0.1:9876", [127, 0, 0, 1]),
            (
                Some("http://tauri.localhost"),
                "127.0.0.1:9876",
                [127, 0, 0, 1],
            ),
            (
                Some("http://127.0.0.1:1421"),
                "127.0.0.1:9876",
                [127, 0, 0, 1],
            ),
            (None, "127.0.0.1:9876", [127, 0, 0, 1]),
            (None, "100.64.0.2:9876", [100, 64, 0, 3]),
            (
                Some("https://192.168.1.2:9876"),
                "192.168.1.2:9876",
                [192, 168, 1, 3],
            ),
        ] {
            let mut req = request("/api/auth/session-token", host, origin, true, peer);
            *req.method_mut() = axum::http::Method::GET;
            if origin == Some("tauri://localhost") {
                req.headers_mut()
                    .insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
            }
            let response = app.clone().oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{origin:?} {host}");
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["token"], "boundary-token");
        }
    }

    /// Catches accepting opaque origins, absent/duplicate hosts or cross-site GETs.
    #[tokio::test]
    async fn malformed_browser_requests_never_reach_handlers() {
        let app = super::super::build_router(state(), true, true);
        for case in [
            "missing-host",
            "duplicate-host",
            "opaque-origin",
            "cross-site",
        ] {
            let mut req = request(
                "/api/auth/session-token",
                "127.0.0.1:9876",
                None,
                true,
                [127, 0, 0, 1],
            );
            *req.method_mut() = axum::http::Method::GET;
            match case {
                "missing-host" => {
                    req.headers_mut().remove(header::HOST);
                }
                "duplicate-host" => {
                    req.headers_mut()
                        .append(header::HOST, HeaderValue::from_static("attacker.example"));
                }
                "opaque-origin" => {
                    req.headers_mut()
                        .insert(header::ORIGIN, HeaderValue::from_static("null"));
                }
                "cross-site" => {
                    req.headers_mut()
                        .insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
                }
                _ => unreachable!(),
            }
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                StatusCode::FORBIDDEN,
                "{case}"
            );
        }
    }

    fn token_get(host: &str, origin: Option<&str>) -> Request<Body> {
        let mut req = request(
            "/api/auth/session-token",
            host,
            origin,
            true,
            [100, 64, 0, 3],
        );
        *req.method_mut() = axum::http::Method::GET;
        req
    }

    /// Catches #untrusted-origin: a Host that spells the same origin differently
    /// (case, or the scheme's default port) answered 403 "Untrusted Origin", while
    /// a different port or scheme stays rejected.
    #[tokio::test]
    async fn origin_matches_host_ignoring_case_and_default_port() {
        let app = super::super::build_router(state(), true, true);
        for (host, origin, expected) in [
            ("LOCALHOST:9876", "http://localhost:9876", StatusCode::OK),
            ("192.168.1.2:80", "http://192.168.1.2", StatusCode::OK),
            ("192.168.1.2:443", "https://192.168.1.2", StatusCode::OK),
            (
                "192.168.1.2:9876",
                "http://192.168.1.2:9877",
                StatusCode::FORBIDDEN,
            ),
            (
                "192.168.1.2:9876",
                "http://192.168.1.2:80",
                StatusCode::FORBIDDEN,
            ),
            (
                "192.168.1.2:80",
                "https://192.168.1.2",
                StatusCode::FORBIDDEN,
            ),
            (
                "192.168.1.2:9876",
                "ftp://192.168.1.2:9876",
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(token_get(host, Some(origin)))
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{host} {origin}");
        }
    }

    /// Catches a link from another site or app (Sec-Fetch-Site: cross-site, no
    /// Origin) answering "Untrusted Origin" instead of opening the app, and the
    /// exemption widening to fetches, frames or writes.
    #[tokio::test]
    async fn cross_site_navigation_opens_the_app_but_cross_site_fetch_does_not() {
        let app = super::super::build_router(state(), true, true);
        for (method, mode, dest, expected_forbidden) in [
            ("GET", "navigate", "document", false),
            ("HEAD", "navigate", "document", false),
            ("GET", "cors", "empty", true),
            ("GET", "no-cors", "image", true),
            ("GET", "navigate", "iframe", true),
            ("POST", "navigate", "document", true),
        ] {
            let mut req = token_get("100.64.0.2:9876", None);
            *req.method_mut() = method.parse().unwrap();
            req.headers_mut()
                .insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
            req.headers_mut()
                .insert("sec-fetch-mode", HeaderValue::from_static(mode));
            req.headers_mut()
                .insert("sec-fetch-dest", HeaderValue::from_static(dest));
            let status = app.clone().oneshot(req).await.unwrap().status();
            assert_eq!(
                status == StatusCode::FORBIDDEN,
                expected_forbidden,
                "{method} {mode} {dest}"
            );
        }
    }

    /// Catches the Tailscale short name (MagicDNS) answering "Untrusted Host" once
    /// the macOS hostname no longer equals the node name, and a foreign single
    /// label being accepted with it.
    #[tokio::test]
    async fn tailscale_short_name_is_trusted_while_running() {
        let state = state();
        *state.tailscale_state.write() = crate::tailscale::TailscaleState::Running {
            fqdn: "node-x.tail1.ts.net".into(),
            https_enabled: false,
        };
        let app = super::super::build_router(state, true, true);
        for (host, expected) in [
            ("node-x:9876", StatusCode::OK),
            ("node-x.tail1.ts.net:9876", StatusCode::OK),
            ("other:9876", StatusCode::FORBIDDEN),
        ] {
            let status = app
                .clone()
                .oneshot(token_get(host, None))
                .await
                .unwrap()
                .status();
            assert_eq!(status, expected, "{host}");
        }
    }

    #[derive(Clone, Default)]
    struct LogSink(Arc<parking_lot::Mutex<Vec<u8>>>);

    impl std::io::Write for LogSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
        type Writer = LogSink;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Catches a rejection that leaves no trace (the next "Untrusted Origin" stays
    /// undiagnosable), a repeat flooding the log, and the `?token=` of the request
    /// URL reaching it.
    #[tokio::test]
    async fn rejection_is_logged_once_per_reason_host_and_origin_without_the_token() {
        let sink = LogSink::default();
        let _guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(sink.clone())
                .with_ansi(false)
                .finish(),
        );
        let app = super::super::build_router(state(), true, true);
        for origin in [
            "http://evil.example",
            "http://evil.example",
            "http://evil.example",
            "http://other.example",
        ] {
            let response = app
                .clone()
                .oneshot(token_get("100.64.0.2:9876", Some(origin)))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let log = String::from_utf8(sink.0.lock().clone()).unwrap();
        assert_eq!(
            log.matches("Request rejected before auth").count(),
            2,
            "{log}"
        );
        assert!(log.contains("origin-not-host"), "{log}");
        assert!(log.contains("100.64.0.2:9876"), "{log}");
        assert!(log.contains("evil.example"), "{log}");
        assert!(!log.contains("boundary-token"), "{log}");
    }

    /// Catches putting auth/health/preflight outside the origin boundary on the daemon.
    #[tokio::test]
    async fn daemon_health_and_preflight_cannot_escape_boundary() {
        let app = super::super::build_remote_router(state());
        for method in [axum::http::Method::GET, axum::http::Method::OPTIONS] {
            let req = Request::builder()
                .method(method)
                .uri("/health")
                .header(header::HOST, "attacker.example")
                .header(header::ORIGIN, "http://attacker.example")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                StatusCode::FORBIDDEN
            );
        }
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/sessions/missing/write")
            .header(header::HOST, "127.0.0.1:9876")
            .header(header::ORIGIN, "tauri://localhost")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "tauri://localhost"
        );
    }
    mod origin_boundaries {
        //! Adversarial tests for the local API origin boundary (story 1456-351c), written by the critic.
        //! Each test names the plausible bug it catches.

        use super::*;
        use axum::body::Body;
        use axum::extract::ConnectInfo;
        use std::net::SocketAddr;
        use tower::ServiceExt;

        fn state() -> Arc<AppState> {
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            *state.session_token.write() = "boundary-token".into();
            state
        }

        fn get(
            uri: &str,
            host: Option<&str>,
            origin: Option<&str>,
            peer: [u8; 4],
        ) -> Request<Body> {
            let mut builder = Request::get(uri);
            if let Some(host) = host {
                builder = builder.header(header::HOST, host);
            }
            if let Some(origin) = origin {
                builder = builder.header(header::ORIGIN, origin);
            }
            builder
                .extension(ConnectInfo(SocketAddr::from((peer, 4000))))
                .body(Body::empty())
                .unwrap()
        }

        /// Catches HTTP/2 (ALPN `h2` is advertised on the Tailscale HTTPS listener): hyper hands
        /// the handler a request whose authority lives in the URI and has NO `Host` header, so
        /// requiring the header locks every HTTPS mobile PWA / browser out with 403 "Untrusted Host".
        #[tokio::test]
        async fn http2_request_authority_in_uri_without_host_header_is_accepted() {
            let app = super::super::build_router(state(), true, true);
            let response = app
                .oneshot(get(
                    "https://100.64.0.2:9876/api/auth/session-token?token=boundary-token",
                    None,
                    Some("https://100.64.0.2:9876"),
                    [100, 64, 0, 3],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        /// Catches the h2 fallback above trusting a hostile URI authority: rebinding over h2
        /// must still be rejected.
        #[tokio::test]
        async fn http2_hostile_uri_authority_without_host_header_is_rejected() {
            let app = super::super::build_router(state(), true, true);
            let response = app
                .oneshot(get(
                    "https://attacker.example:9876/debug/invoke_js?token=boundary-token",
                    None,
                    Some("https://attacker.example:9876"),
                    [127, 0, 0, 1],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }

        /// Catches suffix/prefix/parse confusion in the Host allow-list (rebinding names that
        /// merely contain a loopback token, 0.0.0.0, userinfo tricks, mapped IPv6, trailing dot).
        #[tokio::test]
        async fn hostile_host_spellings_are_rejected_on_any_path() {
            let app = super::super::build_router(state(), true, true);
            for host in [
                "127.0.0.1.attacker.example:9876",
                "localhost.attacker.example:9876",
                "attacker.example:9876@127.0.0.1",
                "127.0.0.1@attacker.example",
                "0.0.0.0:9876",
                "[::ffff:127.0.0.1]:9876",
                "localhost.:9876",
                "",
            ] {
                let response = app
                    .clone()
                    .oneshot(get(
                        "/no/such/route?token=boundary-token",
                        Some(host),
                        None,
                        [127, 0, 0, 1],
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{host:?}");
            }
        }

        /// Catches an Origin that shares a prefix with the Host, a different port on the same
        /// loopback host (another local web server), or a different scheme being accepted.
        #[tokio::test]
        async fn near_miss_origins_are_rejected_even_with_valid_host_and_token() {
            let app = super::super::build_router(state(), true, true);
            for origin in [
                "http://127.0.0.1:9876.attacker.example",
                "http://127.0.0.1:3000",
                "http://localhost:3000",
                "http://127.0.0.1:9876/",
                "ftp://127.0.0.1:9876",
                "null",
            ] {
                let response = app
                    .clone()
                    .oneshot(get(
                        "/api/auth/session-token?token=boundary-token",
                        Some("127.0.0.1:9876"),
                        Some(origin),
                        [127, 0, 0, 1],
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{origin}");
            }
        }

        /// Catches a WebSocket upgrade or CORS preflight from a foreign page reaching a handler
        /// (browsers always send Origin on both).
        #[tokio::test]
        async fn foreign_origin_upgrade_and_preflight_are_rejected() {
            let app = super::super::build_router(state(), true, true);
            let mut upgrade = get(
                "/api/auth/session-token?token=boundary-token",
                Some("127.0.0.1:9876"),
                Some("https://evil.example"),
                [127, 0, 0, 1],
            );
            upgrade
                .headers_mut()
                .insert(header::UPGRADE, HeaderValue::from_static("websocket"));
            upgrade
                .headers_mut()
                .insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
            assert_eq!(
                app.clone().oneshot(upgrade).await.unwrap().status(),
                StatusCode::FORBIDDEN
            );

            let preflight = Request::builder()
                .method("OPTIONS")
                .uri("/sessions/x/write")
                .header(header::HOST, "127.0.0.1:9876")
                .header(header::ORIGIN, "https://evil.example")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4000))))
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(preflight).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert!(
                !response
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            );
        }
    }
}
