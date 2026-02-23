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

/// Check whether the request carries a valid `?token=<session_token>` query param.
/// This is the primary auth method for remote devices: the QR code URL includes the token,
/// and scanning it authenticates the device (a session cookie is then set for subsequent calls).
fn has_valid_url_token(req: &Request<axum::body::Body>, session_token: &str) -> bool {
    let query = req.uri().query().unwrap_or("");
    let expected = format!("token={session_token}");
    query.split('&').any(|param| param == expected)
}

/// Build a Set-Cookie header value for the session token.
/// `max_age_secs` controls cookie lifetime (0 = session cookie that expires on browser close).
fn session_cookie_value(token: &str, max_age_secs: u64) -> String {
    // HttpOnly: JS cannot read the cookie (XSS protection)
    // SameSite=Strict: only sent on same-origin requests (stronger CSRF protection)
    // Path=/: valid for all routes
    let base = format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/");
    if max_age_secs > 0 {
        format!("{base}; Max-Age={max_age_secs}")
    } else {
        base // session cookie — expires when browser closes
    }
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

    let session_token = state.session_token.read().clone();
    let token_duration_secs = state.config.read().session_token_duration_secs;

    // Fast path: valid session cookie skips bcrypt entirely
    if has_valid_session_cookie(&req, &session_token) {
        return next.run(req).await;
    }

    // Primary remote auth: valid ?token=<session_token> in URL.
    // The QR code embeds this token, so scanning it authenticates the device.
    // We set a session cookie so the SPA's subsequent fetch() calls are also authenticated.
    if has_valid_url_token(&req, &session_token) {
        let mut response = next.run(req).await;
        if let Ok(val) = session_cookie_value(&session_token, token_duration_secs).parse() {
            response.headers_mut().insert(header::SET_COOKIE, val);
        }
        return response;
    }

    // Fallback: Basic Auth (if username+password are configured)
    let (username, hash) = {
        let config = state.config.read();
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

    // bcrypt::verify is CPU-intensive (~100ms). Run it on a blocking thread to
    // avoid stalling the single-threaded tokio runtime for the entire server.
    let result = tokio::task::spawn_blocking(move || {
        validate_basic_auth(auth_header.as_deref(), &username, &hash)
    })
    .await
    .unwrap_or(AuthResult::Invalid);

    match result {
        AuthResult::Ok => {
            // Set session cookie so subsequent JS fetch() calls are authenticated
            let mut response = next.run(req).await;
            if let Ok(val) = session_cookie_value(&session_token, token_duration_secs).parse() {
                response.headers_mut().insert(header::SET_COOKIE, val);
            }
            response
        }
        AuthResult::MissingHeader | AuthResult::NotConfigured => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"TUICommander\"")],
            "Scan the QR code or authenticate with Basic Auth",
        )
            .into_response(),
        AuthResult::Invalid => {
            (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_with_max_age() {
        let cookie = session_cookie_value("abc-123", 86400);
        assert!(cookie.contains("tui-session=abc-123"));
        assert!(cookie.contains("Max-Age=86400"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[test]
    fn session_cookie_zero_duration_omits_max_age() {
        let cookie = session_cookie_value("abc-123", 0);
        assert!(cookie.contains("tui-session=abc-123"));
        assert!(!cookie.contains("Max-Age"));
        assert!(cookie.contains("HttpOnly"));
    }

    #[test]
    fn session_cookie_never_duration() {
        let cookie = session_cookie_value("tok", 31536000);
        assert!(cookie.contains("Max-Age=31536000"));
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
}
