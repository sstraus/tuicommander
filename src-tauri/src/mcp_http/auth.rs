use crate::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cookie name used to persist the session after successful Basic Auth.
/// The browser sends cookies automatically in fetch() calls (unlike stored Basic Auth),
/// which is why we need this: JS API calls would otherwise fail with 401 every time.
pub(crate) const SESSION_COOKIE: &str = "tui-session";

/// Failed header digests retained for one IP and one rate-limit window.
const MAX_CACHED_FAILURES_PER_IP: usize = 64;

type CredentialDigest = [u8; 32];

/// One IP's admission state. The mutex covers only state transitions; bcrypt
/// always runs after it is released.
pub(crate) struct AuthRateLimit {
    state: Mutex<AuthRateLimitState>,
    verifying_changed: tokio::sync::Notify,
}

struct AuthRateLimitState {
    attempts: u32,
    window_start: Instant,
    config_digest: CredentialDigest,
    failed: VecDeque<CredentialDigest>,
    verifying: HashSet<CredentialDigest>,
}

impl AuthRateLimit {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(AuthRateLimitState {
                attempts: 0,
                window_start: Instant::now(),
                config_digest: [0; 32],
                failed: VecDeque::new(),
                verifying: HashSet::new(),
            }),
            verifying_changed: tokio::sync::Notify::new(),
        }
    }
}

/// Owns one credential's in-progress verification slot. A request can be
/// cancelled while bcrypt runs, so dropping this guard must release waiters.
struct AuthAttemptGuard {
    limit: Arc<AuthRateLimit>,
    credential: CredentialDigest,
    finished: bool,
}

impl AuthAttemptGuard {
    fn new(limit: Arc<AuthRateLimit>, credential: CredentialDigest) -> Self {
        Self {
            limit,
            credential,
            finished: false,
        }
    }

    fn finish(mut self, failed: bool) {
        finish_auth_attempt(&self.limit, self.credential, failed);
        self.finished = true;
    }
}

impl Drop for AuthAttemptGuard {
    fn drop(&mut self) {
        if !self.finished {
            release_verifying_slot(&self.limit, self.credential);
        }
    }
}

enum AuthAdmission {
    Verify,
    CachedFailure,
    Limited(Duration),
    Wait,
}

/// Result of checking Basic Auth credentials against a config.
pub(super) enum AuthResult {
    /// Credentials are valid
    Ok,
    /// Missing Authorization header
    MissingHeader,
    /// Credentials are invalid (wrong user, wrong password, bad format)
    Invalid,
    /// Auth not configured (no username/password in config)
    NotConfigured,
}

/// Validate a Basic Auth header value against expected credentials.
/// Pure function for testability. NOTE: calls bcrypt::verify — CPU-intensive.
/// Always call this from spawn_blocking in async contexts.
pub(super) fn validate_basic_auth(
    auth_header: Option<&str>,
    expected_username: &str,
    expected_password_hash: &str,
) -> AuthResult {
    if expected_username.is_empty() || expected_password_hash.is_empty() {
        return AuthResult::NotConfigured;
    }

    let Some(auth_value) = auth_header else {
        return AuthResult::MissingHeader;
    };

    let Some(encoded) = auth_value.strip_prefix("Basic ") else {
        return AuthResult::Invalid;
    };

    let Ok(decoded_bytes) =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
    else {
        return AuthResult::Invalid;
    };

    let Ok(decoded) = String::from_utf8(decoded_bytes) else {
        return AuthResult::Invalid;
    };

    let Some((username, password)) = decoded.split_once(':') else {
        return AuthResult::Invalid;
    };

    if username != expected_username {
        return AuthResult::Invalid;
    }

    match bcrypt::verify(password, expected_password_hash) {
        Ok(true) => AuthResult::Ok,
        _ => AuthResult::Invalid,
    }
}

/// Constant-time byte-slice comparison. `mcp_http` serves non-loopback clients
/// (LAN/relay/Tailscale), so comparing secrets with `==` — which short-circuits
/// on the first mismatching byte — is a timing side-channel an attacker could
/// use to recover the session token/cookie byte-by-byte. Always scans the full
/// length of both inputs; never exits early on a byte mismatch. The upfront
/// length check is not itself a secret-dependent branch (input lengths are
/// attacker-visible regardless), so it doesn't reintroduce the leak this
/// guards against.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Check whether the request carries a valid session cookie.
/// This is the fast path — avoids bcrypt on every API call after the first auth.
fn has_valid_session_cookie(req: &Request<axum::body::Body>, session_token: &str) -> bool {
    has_valid_session_cookie_header(req.headers(), session_token)
}

fn has_valid_session_cookie_header(headers: &HeaderMap, session_token: &str) -> bool {
    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("{SESSION_COOKIE}={session_token}");
    cookie_header
        .split(';')
        .map(str::trim)
        .any(|c| ct_eq(c.as_bytes(), expected.as_bytes()))
}

/// Check whether the request carries a valid `?token=<session_token>` query param.
/// This is the primary auth method for remote devices: the QR code URL includes the token,
/// and scanning it authenticates the device (a session cookie is then set for subsequent calls).
fn has_valid_url_token(req: &Request<axum::body::Body>, session_token: &str) -> bool {
    has_valid_token_query(req.uri(), session_token)
}

/// Cookie-only credential check for binary upload endpoints. An empty token
/// never authorizes.
pub(crate) fn has_valid_session_token(headers: &HeaderMap, session_token: &str) -> bool {
    !session_token.is_empty() && has_valid_session_cookie_header(headers, session_token)
}

/// Upload endpoints authenticate with the session cookie only: a `?token=` URL
/// is logged by proxies. QR pairing and WebSocket routes keep the query form.
fn is_cookie_only_route(path: &str) -> bool {
    matches!(path, "/fs/upload-copy" | "/remote/update")
}

pub(crate) fn has_valid_token_query(uri: &axum::http::Uri, session_token: &str) -> bool {
    let query = uri.query().unwrap_or("");
    let expected = format!("token={session_token}");
    query
        .split('&')
        .any(|param| ct_eq(param.as_bytes(), expected.as_bytes()))
}

/// Build a Set-Cookie header value for the session token.
/// `max_age_secs` controls cookie lifetime (0 = session cookie that expires on browser close).
fn session_cookie_value(token: &str, max_age_secs: u64, secure: bool) -> String {
    // HttpOnly: JS cannot read the cookie (XSS protection)
    // SameSite=Strict: only sent on same-origin requests (stronger CSRF protection)
    // Path=/: valid for all routes
    // Secure: only sent over HTTPS (when TLS is active)
    let secure_flag = if secure { "; Secure" } else { "" };
    let base = format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/{secure_flag}");
    if max_age_secs > 0 {
        format!("{base}; Max-Age={max_age_secs}")
    } else {
        base // session cookie — expires when browser closes
    }
}

/// Check whether an IP address belongs to a private/LAN network.
/// Covers RFC1918 (10/8, 172.16/12, 192.168/16), CGNAT/Tailscale (100.64/10),
/// IPv6 ULA (fc00::/7), and IPv6 link-local (fe80::/10).
pub(crate) fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_ipv4(v4),
        IpAddr::V6(v6) => is_private_ipv6(v6),
    }
}

fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    // 10.0.0.0/8
    if o[0] == 10 {
        return true;
    }
    // 172.16.0.0/12
    if o[0] == 172 && (16..=31).contains(&o[1]) {
        return true;
    }
    // 192.168.0.0/16
    if o[0] == 192 && o[1] == 168 {
        return true;
    }
    // 100.64.0.0/10 (CGNAT / Tailscale)
    if o[0] == 100 && (64..=127).contains(&o[1]) {
        return true;
    }
    false
}

fn is_private_ipv6(ip: &Ipv6Addr) -> bool {
    let seg = ip.segments();
    // fc00::/7 — Unique Local Address (ULA)
    if (seg[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // fe80::/10 — Link-local
    if (seg[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    false
}

/// Check if a string IP address belongs to the Tailscale CGNAT range (100.64/10)
/// or the Tailscale IPv6 prefix (fd7a:115c:a1e0::/48).
pub(crate) fn is_tailscale_ip(ip_str: &str) -> bool {
    use std::net::IpAddr;
    let ip: IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return false,
    };
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (64..=127).contains(&o[1])
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

/// Basic Auth middleware that validates credentials against config.
///
/// Flow:
/// 1. Only login assets and CORS preflight bypass credential checks.
/// 2. Requests with a valid session cookie pass through (fast path — no bcrypt).
/// 3. Requests with a valid `Authorization: Basic` header pass through AND get
///    a session cookie set so subsequent JS fetch() calls are authenticated.
/// 4. Everything else → 401.
///
/// Why session cookies? Browsers store Basic Auth credentials for direct navigation
/// but do NOT send them in JS `fetch()` calls. The session cookie is sent automatically
/// with all same-origin fetch() calls, allowing the SPA to work after the initial auth.
pub async fn basic_auth_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Login assets and preflight must load without credentials. The outer
    // request boundary still validates their Host and Origin; the CORS layer
    // handles OPTIONS without running a protected handler. Neither public path
    // gets the `Authenticated` marker below.
    if is_public_login_route(req.method(), req.uri().path())
        || (req.method() == Method::OPTIONS
            && req
                .headers()
                .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD))
    {
        return next.run(req).await;
    }

    // Mark the request as authenticated for downstream route guards
    // (require_local_or_auth). Reaching a handler implies the request passed
    // one of the auth gates below (session cookie, URL
    // token, or Basic Auth); every failed path short-circuits with 401/429
    // here and never runs the handler, so the marker only ever propagates to
    // authenticated handler invocations. (Boss 2026-06-27: token-auth = full
    // trust across config + agent-spawn + prompt routes.)
    req.extensions_mut().insert(super::guards::Authenticated);

    let session_token = state.session_token.read().clone();
    let token_duration_secs = state
        .config
        .read()
        .services
        .auth
        .session_token_duration_secs;

    // Detect TLS for Secure cookie flag (dual-protocol injects Protocol extension)
    let is_tls = req
        .extensions()
        .get::<axum_server_dual_protocol::Protocol>()
        .is_some_and(|p| matches!(p, axum_server_dual_protocol::Protocol::Tls));

    // Fast path: valid session cookie skips bcrypt entirely.
    // The cookie is re-issued on every hit so the expiry slides: with an absolute
    // Max-Age a phone was logged out exactly `session_token_duration_secs` (1 day
    // by default) after scanning the QR, even while in constant use, and fell back
    // to the Basic Auth prompt because the SPA stores the token nowhere.
    if has_valid_session_cookie(&req, &session_token) {
        req.extensions_mut()
            .insert(super::guards::UserAuthenticated);
        let mut response = next.run(req).await;
        if let Ok(val) = session_cookie_value(&session_token, token_duration_secs, is_tls).parse() {
            response.headers_mut().insert(header::SET_COOKIE, val);
        }
        return response;
    }

    // Primary remote auth: valid ?token=<session_token> in URL.
    // The QR code embeds this token, so scanning it authenticates the device.
    // We set a session cookie so the SPA's subsequent fetch() calls are also authenticated.
    if !is_cookie_only_route(req.uri().path()) && has_valid_url_token(&req, &session_token) {
        req.extensions_mut()
            .insert(super::guards::UserAuthenticated);
        state.auth_rate_limits.remove(&addr.ip());
        let mut response = next.run(req).await;
        if let Ok(val) = session_cookie_value(&session_token, token_duration_secs, is_tls).parse() {
            response.headers_mut().insert(header::SET_COOKIE, val);
        }
        return response;
    }

    let (rate_max, rate_window_secs, username, hash) = {
        let config = state.config.read();
        (
            config.services.auth.auth_rate_limit_max,
            config.services.auth.auth_rate_limit_window_secs,
            config.services.auth.username.clone(),
            config.services.auth.password_hash.clone(),
        )
    };
    let client_ip = addr.ip();

    // Fallback: Basic Auth. Admission happens before bcrypt so a full IP
    // window remains a brute-force bound, while known stale credentials do
    // not consume it repeatedly.
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    // Requests that cannot verify credentials do not need rate state. Keeping
    // them out of the map prevents unauthenticated scans from retaining an IP
    // entry for the whole rate-limit window.
    if auth_header.is_none() || username.is_empty() || hash.is_empty() {
        // An app page navigation cannot rely on the Basic dialog:
        // an iOS home-screen app with a registered service worker never shows it
        // (story 1359). Send it to the in-app form instead. Without a configured
        // password the form could never succeed, so that case keeps the 401.
        if auth_header.is_none()
            && !username.is_empty()
            && !hash.is_empty()
            && is_app_page_navigation(&req)
        {
            return login_redirect(req.uri());
        }
        return unauthorized_response("Scan the QR code or authenticate with Basic Auth");
    }
    let result = match check_basic_credentials(
        &state,
        client_ip,
        auth_header.as_deref().unwrap_or_default(),
        &username,
        &hash,
        rate_max,
        rate_window_secs,
    )
    .await
    {
        BasicOutcome::Checked(result) => result,
        BasicOutcome::Limited(retry_after) => return rate_limited_response(retry_after),
    };

    match result {
        AuthResult::Ok => {
            req.extensions_mut()
                .insert(super::guards::UserAuthenticated);
            // A success supersedes every stale failure for this IP.
            state.auth_rate_limits.remove(&client_ip);
            let mut response = next.run(req).await;
            if let Ok(val) =
                session_cookie_value(&session_token, token_duration_secs, is_tls).parse()
            {
                response.headers_mut().insert(header::SET_COOKIE, val);
            }
            response
        }
        AuthResult::MissingHeader | AuthResult::NotConfigured => {
            unauthorized_response("Scan the QR code or authenticate with Basic Auth")
        }
        AuthResult::Invalid => {
            tracing::warn!(source = "auth", ip = %client_ip, "Failed auth attempt");
            unauthorized_response("Invalid credentials")
        }
    }
}

/// Page the unauthenticated mobile app is sent to.
const LOGIN_PAGE_PATH: &str = "/mobile/login";
/// Script of the login page. A separate file keeps the page free of inline code.
const LOGIN_SCRIPT_PATH: &str = "/mobile-login.js";
const LOGIN_API_PATH: &str = "/auth/login";
/// A credential pair is a few dozen bytes; anything larger is not a login.
const MAX_LOGIN_BODY_BYTES: usize = 4096;

fn is_public_login_route(method: &Method, path: &str) -> bool {
    match *method {
        Method::GET | Method::HEAD => path == LOGIN_PAGE_PATH || path == LOGIN_SCRIPT_PATH,
        Method::POST => path == LOGIN_API_PATH,
        _ => false,
    }
}

/// A browser page load of either app shell, as opposed to an API call or an asset.
fn is_app_page_navigation(req: &Request<axum::body::Body>) -> bool {
    let path = req.uri().path();
    *req.method() == Method::GET
        && (path == "/" || path == "/mobile" || path.starts_with("/mobile/"))
        && req
            .headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"))
}

/// Keep a post-login destination inside an app shell: anything else (another
/// origin, `//host`, a backslash trick, control characters) falls back to the
/// app root, so the form cannot be turned into an open redirect.
fn safe_next(next: Option<&str>) -> String {
    let next = next.unwrap_or_default();
    let in_app = next == "/"
        || next.starts_with("/?")
        || next == "/mobile"
        || next.starts_with("/mobile/")
        || next.starts_with("/mobile?");
    let hostile = next.starts_with("//")
        || next.contains('\\')
        || next.contains("://")
        || next.chars().any(char::is_control)
        || next.len() > 2048;
    if in_app && !hostile && !next.starts_with(LOGIN_PAGE_PATH) {
        next.to_string()
    } else {
        "/mobile".to_string()
    }
}

fn login_redirect(uri: &axum::http::Uri) -> Response {
    // Percent-encode everything but unreserved characters and `/`, so the path
    // and its query (a share-target link is `/mobile?shared=<key>`) travel as one
    // query value. `safe_next` still vets it after login.
    let mut encoded = String::new();
    let target = uri.path_and_query().map_or(uri.path(), |pq| pq.as_str());
    for byte in target.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                format!("{LOGIN_PAGE_PATH}?next={encoded}"),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
    )
        .into_response()
}

/// A browser sends `Origin` (and on a secure context `Sec-Fetch-Site`) on every
/// cross-site POST, and neither can be set by page script. Requiring a
/// same-origin signal keeps another site from submitting guesses through the
/// victim's browser. `Sec-Fetch-Site` is absent over plain HTTP on a LAN
/// address, so `Origin` must then match `Host`.
fn is_same_origin_post(headers: &HeaderMap) -> bool {
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        return site == "same-origin";
    }
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    match (origin, host) {
        (Some(origin), Some(host)) => origin
            .split_once("://")
            .is_some_and(|(_, authority)| authority.eq_ignore_ascii_case(host)),
        _ => false,
    }
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
}

/// A login failure answer. It deliberately carries no `WWW-Authenticate`: a
/// Basic challenge on a `fetch` would raise the native dialog the form replaces.
fn login_error(status: StatusCode, message: &'static str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct LoginBody {
    username: String,
    password: String,
    next: Option<String>,
}

/// `POST /auth/login`: trade a username and password for the session cookie.
///
/// The credential check is `check_basic_credentials`, the one the Basic
/// fallback uses, so the per-IP rate limit, the failed-credential cache and
/// bcrypt on a blocking thread apply here unchanged. Same-origin JSON only.
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<axum::body::Body>,
) -> Response {
    if !is_same_origin_post(req.headers()) {
        return login_error(StatusCode::FORBIDDEN, "Cross-origin login refused");
    }
    if !is_json_content_type(req.headers()) {
        return login_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/json",
        );
    }
    let is_tls = req
        .extensions()
        .get::<axum_server_dual_protocol::Protocol>()
        .is_some_and(|p| matches!(p, axum_server_dual_protocol::Protocol::Tls));
    let Ok(bytes) = axum::body::to_bytes(req.into_body(), MAX_LOGIN_BODY_BYTES).await else {
        return login_error(StatusCode::PAYLOAD_TOO_LARGE, "Login body too large");
    };
    let Ok(body) = serde_json::from_slice::<LoginBody>(&bytes) else {
        return login_error(StatusCode::BAD_REQUEST, "Malformed login body");
    };

    let (rate_max, rate_window_secs, token_duration_secs, username, hash) = {
        let config = state.config.read();
        (
            config.services.auth.auth_rate_limit_max,
            config.services.auth.auth_rate_limit_window_secs,
            config.services.auth.session_token_duration_secs,
            config.services.auth.username.clone(),
            config.services.auth.password_hash.clone(),
        )
    };
    if username.is_empty() || hash.is_empty() {
        return login_error(
            StatusCode::UNAUTHORIZED,
            "Password login is not configured; scan the QR code",
        );
    }

    use base64::Engine;
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", body.username, body.password));
    let client_ip = addr.ip();
    let result = match check_basic_credentials(
        &state,
        client_ip,
        &format!("Basic {credentials}"),
        &username,
        &hash,
        rate_max,
        rate_window_secs,
    )
    .await
    {
        BasicOutcome::Checked(result) => result,
        BasicOutcome::Limited(retry_after) => {
            let mut response = login_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many failed authentication attempts",
            );
            if let Ok(val) = (retry_after.as_secs() + 1).to_string().parse() {
                response.headers_mut().insert(header::RETRY_AFTER, val);
            }
            return response;
        }
    };

    match result {
        AuthResult::Ok => {
            state.auth_rate_limits.remove(&client_ip);
            let session_token = state.session_token.read().clone();
            let mut response = (
                [(header::CACHE_CONTROL, "no-store")],
                axum::Json(
                    serde_json::json!({ "ok": true, "next": safe_next(body.next.as_deref()) }),
                ),
            )
                .into_response();
            if let Ok(val) =
                session_cookie_value(&session_token, token_duration_secs, is_tls).parse()
            {
                response.headers_mut().insert(header::SET_COOKIE, val);
            }
            response
        }
        AuthResult::Invalid | AuthResult::MissingHeader | AuthResult::NotConfigured => {
            tracing::warn!(source = "auth", ip = %client_ip, "Failed login attempt");
            login_error(StatusCode::UNAUTHORIZED, "Invalid credentials")
        }
    }
}

/// Outcome of one Basic credential check behind the per-IP admission gate.
enum BasicOutcome {
    Checked(AuthResult),
    Limited(Duration),
}

/// Verify a `Basic` header behind the per-IP admission gate: the rate limit, the
/// failed-credential cache and bcrypt on a blocking thread. Shared by the Basic
/// fallback of `basic_auth_middleware` and by `POST /auth/login`, so both paths
/// spend the same brute-force budget.
async fn check_basic_credentials(
    state: &AppState,
    client_ip: IpAddr,
    auth_header: &str,
    username: &str,
    hash: &str,
    rate_max: u32,
    rate_window_secs: u64,
) -> BasicOutcome {
    let auth_header = Some(auth_header.to_string());
    let (username, hash) = (username.to_string(), hash.to_string());
    let config_digest = auth_config_digest(&username, &hash);
    let credential_digest = failed_credential_digest(auth_header.as_deref(), &config_digest);
    let limit = state
        .auth_rate_limits
        .entry(client_ip)
        .or_insert_with(|| Arc::new(AuthRateLimit::new()))
        .clone();

    let result = loop {
        let notified = limit.verifying_changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let admission = admit_auth_attempt(
            &limit,
            credential_digest,
            config_digest,
            rate_max,
            rate_window_secs,
        );
        match admission {
            AuthAdmission::CachedFailure => break AuthResult::Invalid,
            AuthAdmission::Limited(retry_after) => return BasicOutcome::Limited(retry_after),
            // A duplicate is already in bcrypt. Wait for the owner to finish
            // instead of polling the admission mutex.
            AuthAdmission::Wait => notified.await,
            AuthAdmission::Verify => {
                let attempt = AuthAttemptGuard::new(Arc::clone(&limit), credential_digest);
                // bcrypt::verify is CPU-intensive (~100ms). Run it on a blocking
                // thread to avoid stalling the single-threaded tokio runtime.
                let result = tokio::task::spawn_blocking({
                    let auth_header = auth_header.clone();
                    let username = username.clone();
                    let hash = hash.clone();
                    move || validate_basic_auth(auth_header.as_deref(), &username, &hash)
                })
                .await
                .unwrap_or_else(|e| {
                    tracing::error!(source = "auth", error = %e, "spawn_blocking for bcrypt panicked or was cancelled");
                    AuthResult::Invalid
                });
                attempt.finish(matches!(result, AuthResult::Invalid));
                break result;
            }
        }
    };
    BasicOutcome::Checked(result)
}

fn auth_config_digest(username: &str, password_hash: &str) -> CredentialDigest {
    let mut digest = Sha256::new();
    digest.update(b"tuicommander-auth-config-v1\0");
    digest.update(username.as_bytes());
    digest.update(b"\0");
    digest.update(password_hash.as_bytes());
    digest.finalize().into()
}

fn failed_credential_digest(
    auth_header: Option<&str>,
    config_digest: &CredentialDigest,
) -> CredentialDigest {
    let mut digest = Sha256::new();
    digest.update(b"tuicommander-failed-basic-v1\0");
    digest.update(config_digest);
    digest.update(b"\0");
    digest.update(auth_header.unwrap_or("").as_bytes());
    digest.finalize().into()
}

fn admit_auth_attempt(
    limit: &AuthRateLimit,
    credential: CredentialDigest,
    config: CredentialDigest,
    rate_max: u32,
    window_secs: u64,
) -> AuthAdmission {
    let window = Duration::from_secs(window_secs);
    let mut state = limit.state.lock();
    if state.window_start.elapsed() >= window || state.config_digest != config {
        state.attempts = 0;
        state.window_start = Instant::now();
        state.config_digest = config;
        state.failed.clear();
        state.verifying.clear();
    }
    if state.failed.contains(&credential) {
        return AuthAdmission::CachedFailure;
    }
    if state.verifying.contains(&credential) {
        return AuthAdmission::Wait;
    }
    if rate_max > 0 && state.attempts >= rate_max {
        return AuthAdmission::Limited(window.saturating_sub(state.window_start.elapsed()));
    }
    state.attempts += 1;
    state.verifying.insert(credential);
    AuthAdmission::Verify
}

fn finish_auth_attempt(limit: &AuthRateLimit, credential: CredentialDigest, failed: bool) {
    let mut state = limit.state.lock();
    state.verifying.remove(&credential);
    if failed && !state.failed.contains(&credential) {
        if state.failed.len() == MAX_CACHED_FAILURES_PER_IP {
            state.failed.pop_front();
        }
        state.failed.push_back(credential);
    }
    drop(state);
    limit.verifying_changed.notify_waiters();
}

fn release_verifying_slot(limit: &AuthRateLimit, credential: CredentialDigest) {
    limit.state.lock().verifying.remove(&credential);
    limit.verifying_changed.notify_waiters();
}

fn unauthorized_response(message: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"TUICommander\"")],
        message,
    )
        .into_response()
}

fn rate_limited_response(retry_after: Duration) -> Response {
    let retry_after = retry_after.as_secs() + 1;
    tracing::warn!(
        source = "auth",
        retry_after,
        "Rate limited — too many failed auth attempts"
    );
    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            (header::RETRY_AFTER, retry_after.to_string()),
            (
                header::WWW_AUTHENTICATE,
                "Basic realm=\"TUICommander\"".to_string(),
            ),
        ],
        "Too many failed authentication attempts",
    )
        .into_response()
}

/// Evict rate-limit entries whose window has fully elapsed. Called periodically
/// by the background reaper so the per-IP failure cache cannot outlive its TTL.
pub(super) fn sweep_expired_rate_limits(
    rate_limits: &dashmap::DashMap<std::net::IpAddr, Arc<AuthRateLimit>>,
    window_secs: u64,
) -> usize {
    let window = Duration::from_secs(window_secs);
    let before = rate_limits.len();
    rate_limits.retain(|_, limit| limit.state.lock().window_start.elapsed() < window);
    before - rate_limits.len()
}

/// Authenticate story and workflow credentials for story transition provenance.
/// Missing caller metadata (Unix sockets and in-process services) denotes a
/// local/unknown caller.
pub(super) async fn workflow_actor_middleware(
    State(state): State<Arc<AppState>>,
    caller: Option<axum::extract::Extension<ConnectInfo<SocketAddr>>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if !matches!(
        req.uri().path(),
        "/stories/action" | "/workflows/definition/action" | "/workflows/run/action"
    ) || req
        .extensions()
        .get::<super::guards::UserAuthenticated>()
        .is_some()
    {
        return next.run(req).await;
    }
    let token = state.session_token.read().clone();
    let credentials = has_valid_session_cookie(&req, &token)
        || has_valid_url_token(&req, &token)
        || req.headers().contains_key(header::AUTHORIZATION);
    if credentials {
        let addr = caller.map_or(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            |axum::extract::Extension(ConnectInfo(addr))| addr,
        );
        basic_auth_middleware(State(state), ConnectInfo(addr), req, next).await
    } else {
        next.run(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ct_eq tests ---

    #[test]
    fn ct_eq_matches_equal_slices() {
        assert!(ct_eq(b"same-token-value", b"same-token-value"));
    }

    #[test]
    fn ct_eq_rejects_different_content_same_length() {
        assert!(!ct_eq(b"token-aaaaaaaaaa", b"token-bbbbbbbbbb"));
    }

    #[test]
    fn ct_eq_rejects_different_length() {
        assert!(!ct_eq(b"short", b"much-longer-value"));
    }

    #[test]
    fn ct_eq_empty_slices_are_equal() {
        assert!(ct_eq(b"", b""));
    }

    // --- is_private_ip tests ---

    #[test]
    fn private_ipv4_rfc1918() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 255, 255, 255))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 68, 111))));
    }

    #[test]
    fn private_ipv4_cgnat_tailscale() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            100, 127, 255, 255
        ))));
    }

    #[test]
    fn public_ipv4_not_private() {
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));
    }

    #[test]
    fn private_ipv6_ula() {
        // fd00::1 — ULA
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfd00, 0, 0, 0, 0, 0, 0, 1
        ))));
        // fc00::1 — ULA
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn private_ipv6_link_local() {
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn public_ipv6_not_private() {
        assert!(!is_private_ip(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))));
    }

    #[test]
    fn session_cookie_with_max_age() {
        let cookie = session_cookie_value("abc-123", 86400, false);
        assert!(cookie.contains("tui-session=abc-123"));
        assert!(cookie.contains("Max-Age=86400"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn session_cookie_zero_duration_omits_max_age() {
        let cookie = session_cookie_value("abc-123", 0, false);
        assert!(cookie.contains("tui-session=abc-123"));
        assert!(!cookie.contains("Max-Age"));
        assert!(cookie.contains("HttpOnly"));
    }

    #[test]
    fn session_cookie_never_duration() {
        let cookie = session_cookie_value("tok", 31536000, false);
        assert!(cookie.contains("Max-Age=31536000"));
    }

    #[test]
    fn session_cookie_secure_flag_on_tls() {
        let cookie = session_cookie_value("tok", 86400, true);
        assert!(cookie.contains("; Secure"));
    }

    #[test]
    fn tailscale_ip_detection() {
        assert!(is_tailscale_ip("100.80.90.53"));
        assert!(is_tailscale_ip("100.64.0.1"));
        assert!(is_tailscale_ip("100.127.255.255"));
        assert!(!is_tailscale_ip("100.128.0.1"));
        assert!(!is_tailscale_ip("192.168.1.1"));
        assert!(is_tailscale_ip("fd7a:115c:a1e0::c601:5a3a"));
        assert!(!is_tailscale_ip("fe80::1"));
        assert!(!is_tailscale_ip("not-an-ip"));
    }

    #[test]
    fn valid_session_cookie_matches() {
        let req = Request::get("/")
            .header(header::COOKIE, "tui-session=my-token; other=val")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(has_valid_session_cookie(&req, "my-token"));
    }

    #[test]
    fn invalid_session_cookie_rejected() {
        let req = Request::get("/")
            .header(header::COOKIE, "tui-session=wrong-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!has_valid_session_cookie(&req, "correct-token"));
    }

    // --- validate_basic_auth tests ---

    fn basic_header(user: &str, pass: &str) -> String {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        format!("Basic {encoded}")
    }

    #[test]
    fn basic_auth_valid_credentials() {
        let hash = bcrypt::hash("secret123", 4).unwrap(); // cost=4 for fast tests
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("admin", "secret123")), "admin", &hash),
            AuthResult::Ok
        ));
    }

    #[test]
    fn basic_auth_wrong_password() {
        let hash = bcrypt::hash("correct", 4).unwrap();
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("admin", "wrong")), "admin", &hash),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_missing_header() {
        let hash = bcrypt::hash("pass", 4).unwrap();
        assert!(matches!(
            validate_basic_auth(None, "admin", &hash),
            AuthResult::MissingHeader
        ));
    }

    #[test]
    fn basic_auth_empty_config_not_configured() {
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("admin", "pass")), "", ""),
            AuthResult::NotConfigured
        ));
    }

    #[test]
    fn basic_auth_malformed_base64() {
        assert!(matches!(
            validate_basic_auth(Some("Basic !!!not-base64!!!"), "admin", "somehash"),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_wrong_username() {
        let hash = bcrypt::hash("pass", 4).unwrap();
        assert!(matches!(
            validate_basic_auth(Some(&basic_header("hacker", "pass")), "admin", &hash),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_no_colon_separator() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("nocolon");
        assert!(matches!(
            validate_basic_auth(Some(&format!("Basic {encoded}")), "admin", "somehash"),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn basic_auth_not_basic_scheme() {
        assert!(matches!(
            validate_basic_auth(Some("Bearer some-token"), "admin", "somehash"),
            AuthResult::Invalid
        ));
    }

    #[test]
    fn valid_url_token_matches() {
        let req = Request::get("/?token=abc&other=1")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(has_valid_url_token(&req, "abc"));
    }

    #[test]
    fn invalid_url_token_rejected() {
        let req = Request::get("/?token=wrong")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!has_valid_url_token(&req, "correct"));
    }

    // --- rate limiting tests ---

    #[test]
    fn repeated_failed_header_is_admitted_once_per_window() {
        let limit = AuthRateLimit::new();
        let config = auth_config_digest("boss", "hash");
        let credential = failed_credential_digest(Some("Basic stale"), &config);
        assert!(matches!(
            admit_auth_attempt(&limit, credential, config, 2, 300),
            AuthAdmission::Verify
        ));
        finish_auth_attempt(&limit, credential, true);
        for _ in 0..20 {
            assert!(matches!(
                admit_auth_attempt(&limit, credential, config, 2, 300),
                AuthAdmission::CachedFailure
            ));
        }
        assert_eq!(limit.state.lock().attempts, 1);
    }

    #[test]
    fn unseen_candidates_exhaust_the_ip_budget() {
        let limit = AuthRateLimit::new();
        let config = auth_config_digest("boss", "hash");
        for header in ["Basic wrong-a", "Basic wrong-b"] {
            let credential = failed_credential_digest(Some(header), &config);
            assert!(matches!(
                admit_auth_attempt(&limit, credential, config, 2, 300),
                AuthAdmission::Verify
            ));
            finish_auth_attempt(&limit, credential, true);
        }
        let correct = failed_credential_digest(Some("Basic correct"), &config);
        assert!(matches!(
            admit_auth_attempt(&limit, correct, config, 2, 300),
            AuthAdmission::Limited(_)
        ));
    }

    #[tokio::test]
    async fn dropping_a_verifying_request_releases_its_slot() {
        let limit = Arc::new(AuthRateLimit::new());
        let config = auth_config_digest("boss", "hash");
        let credential = failed_credential_digest(Some("Basic stale"), &config);
        assert!(matches!(
            admit_auth_attempt(&limit, credential, config, 2, 300),
            AuthAdmission::Verify
        ));

        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let request = tokio::spawn({
            let limit = Arc::clone(&limit);
            async move {
                let _slot = AuthAttemptGuard::new(limit, credential);
                entered_tx.send(()).unwrap();
                std::future::pending::<()>().await;
            }
        });
        entered_rx.await.unwrap();
        request.abort();
        let _ = request.await;

        assert!(matches!(
            admit_auth_attempt(&limit, credential, config, 2, 300),
            AuthAdmission::Verify
        ));
    }

    #[test]
    fn sweep_evicts_only_expired_rate_limits() {
        let map = dashmap::DashMap::new();
        let expired: IpAddr = "10.0.0.1".parse().unwrap();
        let fresh: IpAddr = "10.0.0.2".parse().unwrap();
        let expired_limit = Arc::new(AuthRateLimit::new());
        expired_limit.state.lock().window_start = Instant::now() - Duration::from_secs(301);
        map.insert(expired, expired_limit);
        map.insert(fresh, Arc::new(AuthRateLimit::new()));

        let removed = sweep_expired_rate_limits(&map, 300);

        assert_eq!(removed, 1, "only the expired entry should be evicted");
        assert!(map.get(&expired).is_none(), "expired entry gone");
        assert!(map.get(&fresh).is_some(), "fresh entry retained");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn sweep_empty_map_is_noop() {
        let map: dashmap::DashMap<IpAddr, Arc<AuthRateLimit>> = dashmap::DashMap::new();
        assert_eq!(sweep_expired_rate_limits(&map, 300), 0);
    }

    /// A phone authenticates once by scanning the QR and then never sends the
    /// token again — the SPA stores it nowhere, so the cookie is the whole
    /// session. With an absolute `Max-Age` the device was logged out exactly
    /// `session_token_duration_secs` after the scan (1 day by default) even
    /// while in daily use, and fell back to the Basic Auth prompt. Every
    /// cookie-authenticated request must therefore re-issue the cookie.
    #[tokio::test]
    async fn cookie_fast_path_slides_the_expiry() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .config
            .write()
            .services
            .auth
            .session_token_duration_secs = 86400;

        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                basic_auth_middleware,
            ));

        // A public address: neither the desktop loopback bypass nor the LAN
        // bypass may carry this request — only the cookie.
        let req = Request::get("/ping")
            .header(header::COOKIE, "tui-session=test-token")
            .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 51234))))
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("a cookie-authenticated request must refresh the cookie")
            .to_str()
            .unwrap();
        assert!(cookie.contains("tui-session=test-token"), "got {cookie}");
        assert!(cookie.contains("Max-Age=86400"), "got {cookie}");
    }

    #[tokio::test]
    async fn missing_basic_headers_do_not_create_rate_limit_entries() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&state),
                basic_auth_middleware,
            ));

        for host in 1..=32 {
            let response = app
                .clone()
                .oneshot(
                    Request::get("/ping")
                        .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, host], 51234))))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        assert!(state.auth_rate_limits.is_empty());
    }

    #[tokio::test]
    async fn successful_basic_logins_do_not_retain_rate_limit_entries() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut config = state.config.write();
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("correct", 4).unwrap();
        }
        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&state),
                basic_auth_middleware,
            ));

        for host in 1..=32 {
            let response = app
                .clone()
                .oneshot(
                    Request::get("/ping")
                        .header(header::AUTHORIZATION, basic_header("boss", "correct"))
                        .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, host], 51234))))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        assert!(state.auth_rate_limits.is_empty());
    }

    /// A browser may replay its stale Basic header while it waits for a new
    /// challenge. Those replays must not exhaust the whole IP budget before
    /// the user can provide the correct password.
    #[tokio::test]
    async fn stale_basic_header_is_challenged_once_then_correct_login_succeeds() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut config = state.config.write();
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("correct", 4).unwrap();
            config.services.auth.auth_rate_limit_max = 2;
            config.services.auth.auth_rate_limit_window_secs = 300;
        }

        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                basic_auth_middleware,
            ));
        let remote = ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 51234)));

        for _ in 0..20 {
            let response = app
                .clone()
                .oneshot(
                    Request::get("/ping")
                        .header(header::AUTHORIZATION, basic_header("boss", "wrong"))
                        .extension(remote)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(
                response.headers().get(header::WWW_AUTHENTICATE),
                Some(&header::HeaderValue::from_static(
                    "Basic realm=\"TUICommander\""
                ))
            );
        }

        let response = app
            .oneshot(
                Request::get("/ping")
                    .header(header::AUTHORIZATION, basic_header("boss", "correct"))
                    .extension(remote)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key(header::SET_COOKIE));
    }

    #[test]
    fn config_change_invalidates_cached_failures() {
        let limit = AuthRateLimit::new();
        let old_config = auth_config_digest("boss", "old-hash");
        let old_header = failed_credential_digest(Some("Basic old"), &old_config);
        assert!(matches!(
            admit_auth_attempt(&limit, old_header, old_config, 1, 300),
            AuthAdmission::Verify
        ));
        finish_auth_attempt(&limit, old_header, true);

        let new_config = auth_config_digest("boss", "new-hash");
        let new_header = failed_credential_digest(Some("Basic new"), &new_config);
        assert!(matches!(
            admit_auth_attempt(&limit, new_header, new_config, 1, 300),
            AuthAdmission::Verify
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_verification_does_not_strand_the_next_login() {
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut config = state.config.write();
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("correct", 12).unwrap();
            config.services.auth.auth_rate_limit_max = 10;
        }
        let app = axum::Router::new()
            .route("/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&state),
                basic_auth_middleware,
            ));
        let request = || {
            Request::get("/ping")
                .header(header::AUTHORIZATION, basic_header("boss", "wrong"))
                .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, 7], 51234))))
                .body(axum::body::Body::empty())
                .unwrap()
        };

        let first = tokio::spawn(app.clone().oneshot(request()));
        let ip: std::net::IpAddr = [203, 0, 113, 7].into();
        let setup_deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let verifying = state
                .auth_rate_limits
                .get(&ip)
                .is_some_and(|limit| !limit.state.lock().verifying.is_empty());
            if verifying {
                break;
            }
            assert!(
                std::time::Instant::now() < setup_deadline,
                "first request never reached bcrypt"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        first.abort();
        let _ = first.await;

        let second = tokio::time::timeout(Duration::from_secs(10), app.clone().oneshot(request()))
            .await
            .expect("a cancelled verification stranded the next login in Wait")
            .unwrap();
        assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
    }

    // --- in-app login (story 1359) ---

    const PUBLIC_IP: [u8; 4] = [203, 0, 113, 9];

    fn login_state(max_failures: u32) -> Arc<AppState> {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut config = state.config.write();
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("correct", 4).unwrap();
            config.services.auth.auth_rate_limit_max = max_failures;
            config.services.auth.auth_rate_limit_window_secs = 300;
            config.services.auth.session_token_duration_secs = 2_592_000;
        }
        state
    }

    fn login_app(state: &Arc<AppState>) -> axum::Router {
        axum::Router::new()
            .route("/auth/login", axum::routing::post(login_handler))
            .route("/mobile/login", axum::routing::get(|| async { "form" }))
            .route("/", axum::routing::get(|| async { "desktop app" }))
            .route("/mobile", axum::routing::get(|| async { "app" }))
            .route("/mobile/session/a", axum::routing::get(|| async { "app" }))
            .route("/api/ping", axum::routing::get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(state),
                basic_auth_middleware,
            ))
            .with_state(Arc::clone(state))
    }

    fn login_post() -> axum::http::request::Builder {
        Request::post("/auth/login")
            .header(header::HOST, "tuic.test:9876")
            .header(header::ORIGIN, "http://tuic.test:9876")
            .header(header::CONTENT_TYPE, "application/json")
            .extension(ConnectInfo(SocketAddr::from((PUBLIC_IP, 51234))))
    }

    fn credentials(user: &str, pass: &str, next: Option<&str>) -> String {
        serde_json::json!({ "username": user, "password": pass, "next": next }).to_string()
    }

    async fn send(app: &axum::Router, req: Request<axum::body::Body>) -> Response {
        use tower::ServiceExt;
        app.clone().oneshot(req).await.unwrap()
    }

    async fn json_of(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn nav(path: &str) -> axum::http::request::Builder {
        Request::get(path)
            .header(header::ACCEPT, "text/html,application/xhtml+xml")
            .extension(ConnectInfo(SocketAddr::from((PUBLIC_IP, 51234))))
    }

    #[test]
    fn only_the_login_page_script_and_post_are_public() {
        assert!(is_public_login_route(&Method::GET, "/mobile/login"));
        assert!(is_public_login_route(&Method::GET, "/mobile-login.js"));
        assert!(is_public_login_route(&Method::POST, "/auth/login"));
        // Plausible bug: a prefix match would expose the whole mobile app.
        assert!(!is_public_login_route(&Method::GET, "/mobile"));
        assert!(!is_public_login_route(
            &Method::GET,
            "/mobile/login/../session"
        ));
        assert!(!is_public_login_route(&Method::GET, "/auth/login"));
        assert!(!is_public_login_route(&Method::POST, "/mobile/login"));
        assert!(!is_public_login_route(&Method::DELETE, "/auth/login"));
    }

    #[test]
    fn safe_next_keeps_only_in_app_destinations() {
        assert_eq!(safe_next(Some("/")), "/");
        assert_eq!(safe_next(Some("/?view=tablet")), "/?view=tablet");
        assert_eq!(safe_next(Some("/mobile/session/a")), "/mobile/session/a");
        assert_eq!(safe_next(Some("/mobile?shared=k")), "/mobile?shared=k");
        for hostile in [
            "https://evil.test/mobile",
            "//evil.test",
            "/mobile/..\\evil",
            "/mobile/\r\nSet-Cookie: x=1",
            "/api/sessions",
            "/mobile/login?next=/mobile",
            "mobile",
            "",
        ] {
            assert_eq!(safe_next(Some(hostile)), "/mobile", "{hostile:?}");
        }
        assert_eq!(safe_next(None), "/mobile");
    }

    #[tokio::test]
    async fn unauthenticated_page_navigation_lands_on_the_login_page() {
        let app = login_app(&login_state(5));
        let response = send(
            &app,
            nav("/mobile/session/a")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/mobile/login?next=/mobile/session/a"
        );
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
    }

    /// Catches: desktop-mode iPad navigation receives a Basic challenge instead of the form.
    #[tokio::test]
    async fn root_html_navigation_uses_form_and_login_returns_to_root() {
        let app = login_app(&login_state(5));
        let response = send(
            &app,
            nav("/?view=tablet")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/mobile/login?next=/%3Fview%3Dtablet"
        );
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
        let response = send(
            &app,
            login_post()
                .body(credentials("boss", "correct", Some("/?view=tablet")).into())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_of(response).await["next"], "/?view=tablet");
    }

    /// Catches: root API-style fetches follow a redirect and parse login HTML as data.
    #[tokio::test]
    async fn root_non_html_request_keeps_basic_challenge() {
        let app = login_app(&login_state(5));
        let request = Request::get("/")
            .header(header::ACCEPT, "application/json")
            .extension(ConnectInfo(SocketAddr::from((PUBLIC_IP, 51234))))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = send(&app, request).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
        assert!(!response.headers().contains_key(header::LOCATION));
    }

    /// The API contract is unchanged: a 401 with the Basic challenge, never a
    /// redirect a `fetch` would follow into an HTML page.
    #[tokio::test]
    async fn api_and_non_html_requests_keep_the_401_challenge() {
        let app = login_app(&login_state(5));
        let api = send(
            &app,
            nav("/api/ping").body(axum::body::Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
        assert!(api.headers().contains_key(header::WWW_AUTHENTICATE));

        let json_fetch = Request::get("/mobile")
            .header(header::ACCEPT, "application/json")
            .extension(ConnectInfo(SocketAddr::from((PUBLIC_IP, 51234))))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = send(&app, json_fetch).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// Without a configured password the form could never succeed.
    #[tokio::test]
    async fn navigation_keeps_the_401_when_no_password_is_configured() {
        let state = login_state(5);
        state.config.write().services.auth.password_hash.clear();
        let app = login_app(&state);
        let response = send(
            &app,
            nav("/mobile").body(axum::body::Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_page_loads_without_a_session() {
        let app = login_app(&login_state(5));
        let response = send(
            &app,
            nav("/mobile/login")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn successful_login_sets_the_sliding_cookie_and_returns_the_next_path() {
        let state = login_state(5);
        let app = login_app(&state);
        let body = credentials("boss", "correct", Some("/mobile/session/a"));
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let token = state.session_token.read().clone();
        assert!(cookie.contains(&format!("tui-session={token}")), "{cookie}");
        assert!(cookie.contains("Max-Age=2592000"), "{cookie}");
        assert_eq!(json_of(response).await["next"], "/mobile/session/a");
    }

    /// Plausible bug: the form is an open redirect after login.
    #[tokio::test]
    async fn successful_login_never_returns_an_off_app_destination() {
        let app = login_app(&login_state(5));
        let body = credentials("boss", "correct", Some("https://evil.test/"));
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        assert_eq!(json_of(response).await["next"], "/mobile");
    }

    #[tokio::test]
    async fn login_with_wrong_credentials_is_401_without_a_basic_challenge_or_cookie() {
        let app = login_app(&login_state(5));
        let body = credentials("boss", "wrong", None);
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }

    #[tokio::test]
    async fn login_is_refused_when_no_password_is_configured() {
        let state = login_state(5);
        state.config.write().services.auth.password_hash.clear();
        let app = login_app(&state);
        let body = credentials("boss", "correct", None);
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }

    #[tokio::test]
    async fn cross_origin_login_is_refused_before_any_credential_check() {
        let state = login_state(5);
        let app = login_app(&state);
        let body = credentials("boss", "correct", None);
        let mut foreign_origin = login_post().body(body.clone().into()).unwrap();
        foreign_origin
            .headers_mut()
            .insert(header::ORIGIN, "http://evil.test".parse().unwrap());
        assert_eq!(
            send(&app, foreign_origin).await.status(),
            StatusCode::FORBIDDEN
        );

        let mut cross_site = login_post().body(body.clone().into()).unwrap();
        cross_site
            .headers_mut()
            .insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert_eq!(send(&app, cross_site).await.status(), StatusCode::FORBIDDEN);

        let mut no_origin = login_post().body(body.clone().into()).unwrap();
        no_origin.headers_mut().remove(header::ORIGIN);
        assert_eq!(send(&app, no_origin).await.status(), StatusCode::FORBIDDEN);

        // A refused cross-origin POST must not have spent the IP's budget.
        assert!(state.auth_rate_limits.is_empty());

        let mut same_site = login_post().body(body.clone().into()).unwrap();
        same_site
            .headers_mut()
            .insert("sec-fetch-site", "same-origin".parse().unwrap());
        assert_eq!(send(&app, same_site).await.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_accepts_only_json() {
        let app = login_app(&login_state(5));
        let form = "username=boss&password=correct";
        let mut req = login_post().body(form.into()).unwrap();
        req.headers_mut().insert(
            header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        assert_eq!(
            send(&app, req).await.status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[tokio::test]
    async fn login_rejects_malformed_and_oversized_bodies() {
        let app = login_app(&login_state(5));
        let bad = login_post().body("{\"username\":1}".into()).unwrap();
        assert_eq!(send(&app, bad).await.status(), StatusCode::BAD_REQUEST);

        let huge = "x".repeat(MAX_LOGIN_BODY_BYTES + 1);
        let big = login_post().body(huge.into()).unwrap();
        assert_eq!(
            send(&app, big).await.status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    /// The login path spends the same per-IP budget as the Basic fallback: once
    /// it is exhausted even the correct password gets 429 with `Retry-After`.
    #[tokio::test]
    async fn login_failures_exhaust_the_ip_budget() {
        let app = login_app(&login_state(2));
        for attempt in ["wrong-a", "wrong-b"] {
            let body = credentials("boss", attempt, None);
            let response = send(&app, login_post().body(body.into()).unwrap()).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let body = credentials("boss", "correct", None);
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(header::RETRY_AFTER));
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }

    /// Failures made through the Basic fallback and through the form share one
    /// budget; neither path is a second allowance for a brute-force run.
    #[tokio::test]
    async fn basic_and_form_failures_share_the_ip_budget() {
        let app = login_app(&login_state(2));
        for attempt in ["wrong-a", "wrong-b"] {
            let req = Request::get("/api/ping")
                .header(header::AUTHORIZATION, basic_header("boss", attempt))
                .extension(ConnectInfo(SocketAddr::from((PUBLIC_IP, 51234))))
                .body(axum::body::Body::empty())
                .unwrap();
            assert_eq!(send(&app, req).await.status(), StatusCode::UNAUTHORIZED);
        }
        let body = credentials("boss", "correct", None);
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    /// The cookie the form sets must authenticate the next request.
    #[tokio::test]
    async fn the_login_cookie_authenticates_the_app() {
        let state = login_state(5);
        let app = login_app(&state);
        let body = credentials("boss", "correct", None);
        let response = send(&app, login_post().body(body.into()).unwrap()).await;
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        let pair = cookie.split(';').next().unwrap().to_string();
        let req = nav("/mobile")
            .header(header::COOKIE, pair)
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(send(&app, req).await.status(), StatusCode::OK);
    }
    mod login_boundaries {
        //! Adversarial tests for the in-app login (story 1359-57b4), written by the critic.
        //! Each test names the plausible bug it catches.

        use super::*;
        use base64::Engine;
        use tower::ServiceExt;

        const IP: [u8; 4] = [198, 51, 100, 7];

        fn state_with(max_failures: u32, duration_secs: u64) -> Arc<AppState> {
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            {
                let mut config = state.config.write();
                config.services.auth.username = "boss".to_string();
                config.services.auth.password_hash = bcrypt::hash("co:rrect", 4).unwrap();
                config.services.auth.auth_rate_limit_max = max_failures;
                config.services.auth.auth_rate_limit_window_secs = 300;
                config.services.auth.session_token_duration_secs = duration_secs;
            }
            state
        }

        fn app(state: &Arc<AppState>) -> axum::Router {
            axum::Router::new()
                .route("/auth/login", axum::routing::post(login_handler))
                .route("/mobile/login", axum::routing::get(|| async { "form" }))
                .route("/mobile", axum::routing::get(|| async { "app" }))
                .route("/mobile/session/a", axum::routing::get(|| async { "app" }))
                .layer(axum::middleware::from_fn_with_state(
                    Arc::clone(state),
                    basic_auth_middleware,
                ))
                .with_state(Arc::clone(state))
        }

        fn post_from(ip: [u8; 4]) -> axum::http::request::Builder {
            Request::post("/auth/login")
                .header(header::HOST, "tuic.test:9876")
                .header(header::ORIGIN, "http://tuic.test:9876")
                .header(header::CONTENT_TYPE, "application/json")
                .extension(ConnectInfo(SocketAddr::from((ip, 40000))))
        }

        fn body(user: &str, pass: &str) -> axum::body::Body {
            serde_json::json!({ "username": user, "password": pass })
                .to_string()
                .into()
        }

        async fn send(app: &axum::Router, req: Request<axum::body::Body>) -> Response {
            app.clone().oneshot(req).await.unwrap()
        }

        fn nav(path: &str) -> axum::http::request::Builder {
            Request::get(path)
                .header(header::ACCEPT, "text/html")
                .extension(ConnectInfo(SocketAddr::from((IP, 40000))))
        }

        fn empty() -> axum::body::Body {
            axum::body::Body::empty()
        }

        /// Plausible bug: `login_redirect` encodes only `uri.path()`, so a deep link with
        /// a query (the share target opens `/mobile?shared=<key>`) loses it at login.
        #[tokio::test]
        async fn redirect_to_login_keeps_the_query_of_the_deep_link() {
            let app = app(&state_with(5, 3600));
            let response = send(&app, nav("/mobile?shared=abc").body(empty()).unwrap()).await;
            assert_eq!(response.status(), StatusCode::FOUND);
            let location = response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                location.contains("shared%3Dabc"),
                "query dropped: {location}"
            );
        }

        /// Plausible bug: the per-IP budget keyed on a client-supplied header, so a
        /// rotating `X-Forwarded-For` mints a fresh budget per guess.
        #[tokio::test]
        async fn forwarded_for_header_does_not_open_a_second_budget() {
            let app = app(&state_with(2, 3600));
            for (i, guess) in ["a", "b"].iter().enumerate() {
                let mut req = post_from(IP).body(body("boss", guess)).unwrap();
                req.headers_mut()
                    .insert("x-forwarded-for", format!("10.9.9.{i}").parse().unwrap());
                assert_eq!(send(&app, req).await.status(), StatusCode::UNAUTHORIZED);
            }
            let mut req = post_from(IP).body(body("boss", "co:rrect")).unwrap();
            req.headers_mut()
                .insert("x-forwarded-for", "10.9.9.200".parse().unwrap());
            req.headers_mut()
                .insert("x-real-ip", "10.9.9.201".parse().unwrap());
            assert_eq!(
                send(&app, req).await.status(),
                StatusCode::TOO_MANY_REQUESTS
            );
        }

        /// Plausible bug: admission counted after bcrypt finishes, so a burst of
        /// concurrent guesses all get verified before the budget closes.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_guesses_never_exceed_the_budget() {
            let app = app(&state_with(3, 3600));
            let mut set = tokio::task::JoinSet::new();
            for i in 0..12 {
                let app = app.clone();
                set.spawn(async move {
                    let req = post_from(IP)
                        .body(body("boss", &format!("wrong-{i}")))
                        .unwrap();
                    app.oneshot(req).await.unwrap().status()
                });
            }
            let mut verified = 0;
            let mut limited = 0;
            while let Some(status) = set.join_next().await {
                match status.unwrap() {
                    StatusCode::UNAUTHORIZED => verified += 1,
                    StatusCode::TOO_MANY_REQUESTS => limited += 1,
                    other => panic!("unexpected {other}"),
                }
            }
            assert_eq!(verified, 3, "verified {verified}, limited {limited}");
            assert_eq!(limited, 9);
        }

        /// Plausible bug: cookie flags dropped on the login path (no HttpOnly, no
        /// SameSite=Strict, or no Secure under TLS).
        #[tokio::test]
        async fn login_cookie_flags_plain_and_tls() {
            let app = app(&state_with(5, 3600));
            let plain = send(&app, post_from(IP).body(body("boss", "co:rrect")).unwrap()).await;
            assert_eq!(plain.status(), StatusCode::OK);
            let cookie = plain
                .headers()
                .get(header::SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            assert!(
                cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
                "{cookie}"
            );
            assert!(cookie.contains("Path=/"), "{cookie}");
            assert!(
                !cookie.contains("Secure"),
                "plain HTTP must not set Secure: {cookie}"
            );

            let tls = send(
                &app,
                post_from(IP)
                    .extension(axum_server_dual_protocol::Protocol::Tls)
                    .body(body("boss", "co:rrect"))
                    .unwrap(),
            )
            .await;
            let cookie = tls
                .headers()
                .get(header::SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(cookie.contains("Secure"), "{cookie}");
        }

        /// Plausible bug: sliding renewal uses a different (larger) lifetime than the
        /// configured one, or `0` (session cookie) gets a Max-Age.
        #[tokio::test]
        async fn sliding_cookie_never_exceeds_the_configured_lifetime() {
            let state = state_with(5, 3600);
            let app = app(&state);
            let login = send(&app, post_from(IP).body(body("boss", "co:rrect")).unwrap()).await;
            let pair = login
                .headers()
                .get(header::SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_string();
            for _ in 0..3 {
                let hit = send(
                    &app,
                    nav("/mobile")
                        .header(header::COOKIE, pair.clone())
                        .body(empty())
                        .unwrap(),
                )
                .await;
                let renewed = hit
                    .headers()
                    .get(header::SET_COOKIE)
                    .unwrap()
                    .to_str()
                    .unwrap();
                assert!(renewed.contains("Max-Age=3600"), "{renewed}");
            }
            state
                .config
                .write()
                .services
                .auth
                .session_token_duration_secs = 0;
            let hit = send(
                &app,
                nav("/mobile")
                    .header(header::COOKIE, pair)
                    .body(empty())
                    .unwrap(),
            )
            .await;
            assert!(
                !hit.headers()
                    .get(header::SET_COOKIE)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .contains("Max-Age")
            );
        }

        /// Plausible bug: Sec-Fetch-Site accepted for `same-site`/`none`, or an Origin
        /// whose port differs from Host accepted (a sibling-port page on the same host).
        #[tokio::test]
        async fn only_exact_same_origin_signals_are_accepted() {
            let state = state_with(5, 3600);
            let app = app(&state);
            for site in ["same-site", "none", "cross-site"] {
                let mut req = post_from(IP).body(body("boss", "co:rrect")).unwrap();
                req.headers_mut()
                    .insert("sec-fetch-site", site.parse().unwrap());
                assert_eq!(
                    send(&app, req).await.status(),
                    StatusCode::FORBIDDEN,
                    "{site}"
                );
            }
            let mut other_port = post_from(IP).body(body("boss", "co:rrect")).unwrap();
            other_port
                .headers_mut()
                .insert(header::ORIGIN, "http://tuic.test:9999".parse().unwrap());
            assert_eq!(send(&app, other_port).await.status(), StatusCode::FORBIDDEN);
            let mut null_origin = post_from(IP).body(body("boss", "co:rrect")).unwrap();
            null_origin
                .headers_mut()
                .insert(header::ORIGIN, "null".parse().unwrap());
            assert_eq!(
                send(&app, null_origin).await.status(),
                StatusCode::FORBIDDEN
            );
            assert!(
                state.auth_rate_limits.is_empty(),
                "refused POSTs must not spend budget"
            );
        }

        /// Plausible bug: Content-Type compared as an exact string, so the charset
        /// parameter browsers/fetch libraries add is refused; or a `text/plain`
        /// simple-request body is accepted (CORS-preflight-free CSRF).
        #[tokio::test]
        async fn content_type_parameters_ok_but_simple_request_types_refused() {
            let app = app(&state_with(5, 3600));
            let mut ok = post_from(IP).body(body("boss", "co:rrect")).unwrap();
            ok.headers_mut().insert(
                header::CONTENT_TYPE,
                "application/json; charset=utf-8".parse().unwrap(),
            );
            assert_eq!(send(&app, ok).await.status(), StatusCode::OK);
            for ct in ["text/plain", "application/jsonx", "multipart/form-data"] {
                let mut req = post_from(IP).body(body("boss", "co:rrect")).unwrap();
                req.headers_mut()
                    .insert(header::CONTENT_TYPE, ct.parse().unwrap());
                assert_eq!(
                    send(&app, req).await.status(),
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "{ct}"
                );
            }
        }

        /// Plausible bug: the Basic header is built as `user:pass` and split at the first
        /// colon, so a password that contains `:` can never log in via the form.
        /// (The fixture password is `co:rrect`.)
        #[tokio::test]
        async fn password_containing_a_colon_logs_in() {
            let app = app(&state_with(5, 3600));
            let response = send(&app, post_from(IP).body(body("boss", "co:rrect")).unwrap()).await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        /// Plausible bug: hostile `next` forms that start with `/mobile` but leave the app
        /// (userinfo `@`, sibling host label, NEL/tab control characters, huge value).
        #[test]
        fn safe_next_rejects_prefix_lookalikes() {
            let long = format!("/mobile/{}", "a".repeat(3000));
            for hostile in [
                "/mobile@evil.test",
                "/mobile.evil.test",
                "/mobilex",
                "/mobile/\u{85}x",
                "/mobile/\tx",
                long.as_str(),
            ] {
                assert_eq!(safe_next(Some(hostile)), "/mobile", "{hostile:?}");
            }
        }

        /// Plausible bug: after a token rotation the stale cookie gets the 401 dead end
        /// again instead of the login redirect; or a wrong Basic header on a navigation is
        /// redirected (hiding the challenge and the budget it spends).
        #[tokio::test]
        async fn stale_cookie_redirects_but_wrong_basic_keeps_the_challenge() {
            let app = app(&state_with(5, 3600));
            let stale = send(
                &app,
                nav("/mobile/session/a")
                    .header(header::COOKIE, "tui-session=old-token")
                    .body(empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(stale.status(), StatusCode::FOUND);

            let wrong = base64::engine::general_purpose::STANDARD.encode("boss:nope");
            let basic = send(
                &app,
                nav("/mobile")
                    .header(header::AUTHORIZATION, format!("Basic {wrong}"))
                    .body(empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(basic.status(), StatusCode::UNAUTHORIZED);
            assert!(basic.headers().contains_key(header::WWW_AUTHENTICATE));
        }

        /// Plausible bug: a non-string `next`, or an empty credential pair, panics or
        /// is treated as a success.
        #[tokio::test]
        async fn degenerate_login_bodies_never_succeed() {
            let app = app(&state_with(5, 3600));
            let bad_next = r#"{"username":"boss","password":"co:rrect","next":5}"#;
            assert_eq!(
                send(&app, post_from(IP).body(bad_next.into()).unwrap())
                    .await
                    .status(),
                StatusCode::BAD_REQUEST
            );
            let empty_pair = send(&app, post_from(IP).body(body("", "")).unwrap()).await;
            assert_eq!(empty_pair.status(), StatusCode::UNAUTHORIZED);
            assert!(!empty_pair.headers().contains_key(header::SET_COOKIE));
        }

        /// Plausible bug: now that the redirect carries the query, an `&`, `#`-less
        /// delimiter or `//` inside it escapes the `next` value and adds a parameter or an
        /// off-app destination to the login URL.
        #[tokio::test]
        async fn redirect_with_a_hostile_query_stays_one_next_value() {
            let app = app(&state_with(5, 3600));
            let response = send(
                &app,
                nav("/mobile?x=1&next=//evil.test&y=%5C%5Cevil")
                    .body(empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::FOUND);
            let location = response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap();
            let query = location
                .strip_prefix("/mobile/login?next=")
                .expect(location);
            assert!(
                !query.contains('&') && !query.contains('?') && !query.contains('\\'),
                "{location}"
            );
        }

        /// Plausible bug: `safe_next` trusts a query that smuggles an absolute URL or a
        /// scheme-relative one after a legitimate `/mobile?` prefix.
        #[test]
        fn safe_next_rejects_urls_inside_the_query() {
            for hostile in [
                "/mobile?next=https://evil.test",
                "/mobile?x=1&u=http://evil.test/",
                "/mobile?x=\\\\evil.test",
                "//evil.test/mobile?x=1",
                "/\\evil.test",
            ] {
                assert_eq!(safe_next(Some(hostile)), "/mobile", "{hostile:?}");
            }
            assert_eq!(safe_next(Some("/mobile?shared=abc")), "/mobile?shared=abc");
        }
    }
}
