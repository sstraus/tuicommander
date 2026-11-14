//! Shared HTTP request guards used by config, agent, and prompt routes.
//!
//! Sensitive routes (config mutation, process spawn, prompt execution) must be
//! reachable either from the loopback interface OR by a request that already
//! passed the auth middleware (`basic_auth_middleware`). A token-authenticated
//! remote device (LAN or Tailscale) is fully trusted; unauthenticated remote
//! callers are rejected. This module centralises that check so new routes
//! cannot accidentally ship without it.
//!
//! Boss decision 2026-06-27 (story 059): token-authenticated requests get full
//! trust across config + agent-spawn + prompt routes, superseding the earlier
//! "config-only / agent stays loopback" scope. Caveat surfaced to Boss: on
//! plain-HTTP LAN the token travels in cleartext; on Tailscale TLS it is fine.

use axum::Json;
use axum::http::StatusCode;
use std::net::SocketAddr;

/// Marker inserted into the request extensions by `basic_auth_middleware` for
/// every request that reaches a handler. Reaching a handler implies the request
/// passed an auth gate (desktop loopback bypass, LAN bypass, session cookie,
/// URL token, or Basic Auth) — failed paths short-circuit with 401/429 and
/// never run the handler. Route guards read this via an
/// `Option<Extension<Authenticated>>` extractor.
#[derive(Clone, Copy)]
pub(super) struct Authenticated;

/// Actual credential verification, distinct from transport admission.
#[derive(Clone, Copy)]
pub(super) struct UserAuthenticated;

/// Server-issued authority of one request that manages sessions, agents,
/// launches and mail.
///
/// Boss decision L03 (2026-11-01): an authenticated caller equals a loopback
/// caller. Loopback covers the local IPC transport, which presents itself as
/// 127.0.0.1. `verified` comes only from the [`UserAuthenticated`] marker, so a
/// transport admission without checked credentials is never owner evidence.
/// Ownership of tasks, launches and mail stays with the caller identity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CallerAuthority {
    addr: SocketAddr,
    verified: bool,
}

impl CallerAuthority {
    /// Authority of a request; `user` is its `UserAuthenticated` marker, if any.
    pub(super) fn from_request(addr: SocketAddr, user: Option<UserAuthenticated>) -> Self {
        Self {
            addr,
            verified: user.is_some(),
        }
    }

    pub(crate) fn addr(self) -> SocketAddr {
        self.addr
    }

    /// True for loopback callers and for callers with verified credentials.
    pub(crate) fn is_owner(self) -> bool {
        self.verified || self.addr.ip().is_loopback()
    }
}

/// A bare address carries no verified credentials: only loopback is owner.
impl From<SocketAddr> for CallerAuthority {
    fn from(addr: SocketAddr) -> Self {
        Self {
            addr,
            verified: false,
        }
    }
}

/// Reject requests that did not originate on the loopback interface.
///
/// Used by desktop-producer surfaces (e.g. desktop open-tab publication) that
/// must stay loopback-only even for authenticated remote callers. Returns `Err`
/// with a ready-to-serialize 403 response tuple.
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

/// Reject requests that are neither from the loopback interface nor
/// authenticated by the auth middleware.
///
/// `authenticated` is `true` when the handler extracted an `Authenticated`
/// marker (i.e. the request passed `basic_auth_middleware`). Returns `Err`
/// with a ready-to-serialize 403 response tuple. Callers typically use
/// `if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) { return resp.into_response(); }`.
pub(super) fn require_local_or_auth(
    addr: &SocketAddr,
    authenticated: bool,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if addr.ip().is_loopback() || authenticated {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "This endpoint requires loopback access or authentication"
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
        assert!(require_local_or_auth(&addr, false).is_ok());
    }

    #[test]
    fn accepts_ipv6_loopback() {
        let addr: SocketAddr = "[::1]:42".parse().unwrap();
        assert!(require_local_or_auth(&addr, false).is_ok());
    }

    #[test]
    fn accepts_authenticated_remote() {
        let addr: SocketAddr = "100.64.1.42:443".parse().unwrap();
        assert!(require_local_or_auth(&addr, true).is_ok());
    }

    #[test]
    fn rejects_unauthenticated_lan() {
        let addr: SocketAddr = "192.168.1.42:80".parse().unwrap();
        let err =
            require_local_or_auth(&addr, false).expect_err("unauthenticated LAN must be rejected");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn rejects_unauthenticated_public() {
        let addr: SocketAddr = "8.8.8.8:80".parse().unwrap();
        assert!(require_local_or_auth(&addr, false).is_err());
    }

    #[test]
    fn accepts_authenticated_public() {
        let addr: SocketAddr = "8.8.8.8:80".parse().unwrap();
        assert!(require_local_or_auth(&addr, true).is_ok());
    }

    #[test]
    fn localhost_only_accepts_loopback() {
        let v4: SocketAddr = "127.0.0.1:42".parse().unwrap();
        let v6: SocketAddr = "[::1]:42".parse().unwrap();
        assert!(localhost_only(&v4).is_ok());
        assert!(localhost_only(&v6).is_ok());
    }

    #[test]
    fn localhost_only_rejects_remote_even_authenticated() {
        // localhost_only ignores auth entirely — it is the strict RCE gate.
        let lan: SocketAddr = "192.168.1.42:80".parse().unwrap();
        let public: SocketAddr = "8.8.8.8:80".parse().unwrap();
        assert_eq!(
            localhost_only(&lan).expect_err("LAN must be rejected").0,
            StatusCode::FORBIDDEN
        );
        assert!(localhost_only(&public).is_err());
    }
}
