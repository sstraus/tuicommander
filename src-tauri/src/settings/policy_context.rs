//! Host-origin identity is never part of a model-deserialized request.
use super::super::request::{Error, Failure};
use std::time::Duration;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct CallerKey {
    pub connection: String,
    pub origin: &'static str,
}
/// Constructed by the server transport, never deserialized from a model payload.
///
/// An authenticated remote caller has the same settings authority as loopback
/// (L07): it may read, propose and confirm. Approval still needs the enabled
/// capability, a real human answer and a proposal bound to this caller,
/// fingerprint and expiry.
#[derive(Clone)]
pub struct Caller {
    pub(super) key: CallerKey,
    enabled: bool,
    /// Set only by the Tauri settings command; no request field can claim it.
    pub(super) owner_ui: bool,
}
impl Caller {
    pub fn native() -> Self {
        Self {
            key: CallerKey {
                connection: "native".into(),
                origin: "ipc",
            },
            enabled: true,
            owner_ui: false,
        }
    }
    /// The owner's own settings window, reached only through the native IPC command.
    pub fn owner_ui() -> Self {
        Self {
            owner_ui: true,
            ..Self::native()
        }
    }
    pub fn http(connection: String) -> Self {
        let enabled = !connection.is_empty();
        Self {
            key: CallerKey {
                connection,
                origin: "http",
            },
            enabled,
            owner_ui: false,
        }
    }
    /// Reaching the MCP handler already proves loopback or authentication:
    /// the TCP router always runs `basic_auth_middleware`, and the Unix socket
    /// and named pipe routers serve only local clients.
    pub fn mcp(connection: String, settings_enabled: bool) -> Self {
        let enabled = settings_enabled && !connection.is_empty();
        Self {
            key: CallerKey {
                connection,
                origin: "mcp",
            },
            enabled,
            owner_ui: false,
        }
    }
    pub fn check(&self) -> Result<(), Failure> {
        if !self.enabled {
            return Err(Error::new("settings", "unavailable"));
        }
        Ok(())
    }
}

/// Backend-built redacted text for the existing confirmation dialog.
#[derive(Clone)]
pub struct Preview {
    pub title: String,
    pub message: String,
    pub(crate) remaining: Duration,
}
/// Bounded audit contains identifiers and outcomes only, never values or dynamic keys.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    pub connection: String,
    pub origin: &'static str,
    pub target: super::super::request::Target,
    pub store: String,
    pub changed_ids: Vec<String>,
    pub outcome: String,
}
