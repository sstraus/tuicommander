//! MCP Proxy Hub — connects TUIC to upstream MCP servers.
//!
//! TUIC acts as both an MCP server (to downstream clients like Claude Code)
//! and an MCP client (to upstream servers). Tools from all connected upstreams
//! are aggregated and exposed via TUIC's single `/mcp` endpoint, prefixed with
//! `{upstream_name}__` to avoid namespace collisions.

pub(crate) mod http_client;
pub(crate) mod stdio_client;
pub(crate) mod registry;
