//! Shared HTTP request guards used by config, agent, and prompt routes.
//!
//! Sensitive routes (config mutation, process spawn, prompt execution) must
//! only be reachable from the loopback interface, even when `remote_enabled`
//! binds the server to `0.0.0.0`. This module centralises that check so new
//! routes cannot accidentally ship without it.

use axum::Json;
use axum::http::StatusCode;
use std::net::SocketAddr;

/// Reject requests that did not originate on the loopback interface.
///
/// Returns `Err` with a ready-to-serialize 403 response tuple. Callers
/// typically use `if let Err(resp) = localhost_only(&addr) { return resp.into_response(); }`.
pub(super) fn localhost_only(
    addr: &SocketAddr,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if addr.ip().is_loopback() {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "This endpoint is only accessible from localhost"
            })),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ipv4_loopback() {
        let addr: SocketAddr = "127.0.0.1:42".parse().unwrap();
        assert!(localhost_only(&addr).is_ok());
    }

    #[test]
    fn accepts_ipv6_loopback() {
        let addr: SocketAddr = "[::1]:42".parse().unwrap();
        assert!(localhost_only(&addr).is_ok());
    }

    #[test]
    fn rejects_lan_address() {
        let addr: SocketAddr = "192.168.1.42:80".parse().unwrap();
        let err = localhost_only(&addr).expect_err("LAN must be rejected");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn rejects_public_ipv4() {
        let addr: SocketAddr = "8.8.8.8:80".parse().unwrap();
        assert!(localhost_only(&addr).is_err());
    }

    #[test]
    fn rejects_public_ipv6() {
        let addr: SocketAddr = "[2001:db8::1]:80".parse().unwrap();
        assert!(localhost_only(&addr).is_err());
    }
}
