use super::*;
#[cfg(unix)]
use crate::PtySession;
use crate::mcp_http::mcp_transport;
use parking_lot::Mutex;
use portable_pty::CommandBuilder;
#[cfg(unix)]
use portable_pty::PtySize;
#[cfg(unix)]
use std::sync::atomic::AtomicBool;
include!("mcp_transport_transport_tests.rs");
#[cfg(unix)]
#[path = "mcp_transport_reconnect_fixture_tests.rs"]
mod reconnect_fixture_tests;
include!("mcp_transport_catalogue_tests.rs");
include!("mcp_transport_peer_tests.rs");
#[cfg(unix)]
include!("mcp_transport_peer_identity_tests.rs");
include!("mcp_transport_session_agent_tests.rs");
include!("mcp_transport_literal_read_tests.rs");
#[cfg(unix)]
include!("suspended_sessions_tests.rs");
include!("mcp_transport_ancillary_tests.rs");
#[cfg(unix)]
include!("mcp_transport_registration_tests.rs");
#[cfg(unix)]
include!("agent_capabilities_tests.rs");
#[cfg(unix)]
include!("caller_role_tests.rs");
#[cfg(unix)]
include!("tool_usage_tests.rs");
#[cfg(feature = "settings-ipc-test")]
#[path = "settings_mock_ipc_tests.rs"]
mod settings_mock_ipc_tests;
include!("mcp_transport_neutral_tests.rs");
