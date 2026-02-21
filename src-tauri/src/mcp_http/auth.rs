use crate::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;
use std::sync::Arc;

/// Cookie name used to persist the session after successful Basic Auth.
/// The browser sends cookies automatically in fetch() calls (unlike stored Basic Auth),
/// which is why we need this: JS API calls would otherwise fail with 401 every time.
const SESSION_COOKIE: &str = "tui-session";

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

/// Check whether the request carries a valid session cookie.
/// This is the fast path — avoids bcrypt on every API call after the first auth.
fn has_valid_session_cookie(req: &Request<axum::body::Body>, session_token: &str) -> bool {
    let cookie_header = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("{SESSION_COOKIE}={session_token}");
    cookie_header
        .split(';')
        .map(str::trim)
        .any(|c| c == expected)
}

/// Build a Set-Cookie header value for the session token.
fn session_cookie_value(token: &str) -> String {
    // HttpOnly: JS cannot read the cookie (XSS protection)
    // SameSite=Lax: sent on same-origin requests and top-level navigation
    // Path=/: valid for all routes
    // Max-Age=86400: 24 hours (auto-re-auth after a day)
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400")
}

/// Basic Auth middleware that validates credentials against config.
///
/// Flow:
/// 1. Localhost connections bypass auth (local Tauri app).
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
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Localhost bypass: local Tauri app connections don't need auth
    if addr.ip().is_loopback() {
        return next.run(req).await;
    }

    // Fast path: valid session cookie skips bcrypt entirely
    if has_valid_session_cookie(&req, &state.session_token) {
        return next.run(req).await;
    }

    // Read credentials and auth header (owned, for move into spawn_blocking)
    let (username, hash) = {
        let config = state.config.read().unwrap();
        (
            config.remote_access_username.clone(),
            config.remote_access_password_hash.clone(),
        )
    };
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let session_token = state.session_token.clone();

    // bcrypt::verify is CPU-intensive (~100ms). Run it on a blocking thread to
    // avoid stalling the single-threaded tokio runtime for the entire server.
    let result = tokio::task::spawn_blocking(move || {
        validate_basic_auth(auth_header.as_deref(), &username, &hash)
    })
    .await
    .unwrap_or(AuthResult::Invalid);

    match result {
        AuthResult::Ok => {
            // Set session cookie so the SPA's JS fetch() calls are authenticated
            let mut response = next.run(req).await;
            if let Ok(val) = session_cookie_value(&session_token).parse() {
                response.headers_mut().insert(header::SET_COOKIE, val);
            }
            response
        }
        AuthResult::MissingHeader => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"TUI Commander\"")],
            "Authentication required",
        )
            .into_response(),
        AuthResult::NotConfigured => {
            (StatusCode::UNAUTHORIZED, "Authentication not configured").into_response()
        }
        AuthResult::Invalid => {
            (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response()
        }
    }
}
