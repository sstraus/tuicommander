use crate::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;
use std::sync::Arc;

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
/// Pure function for testability.
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

/// Basic Auth middleware that validates credentials against config.
/// Localhost connections (127.0.0.1, ::1) bypass authentication.
pub async fn basic_auth_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Localhost bypass: no auth needed for local connections
    if addr.ip().is_loopback() {
        return next.run(req).await;
    }

    let config = state.config.read().unwrap().clone();
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match validate_basic_auth(
        auth_header,
        &config.remote_access_username,
        &config.remote_access_password_hash,
    ) {
        AuthResult::Ok => next.run(req).await,
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
