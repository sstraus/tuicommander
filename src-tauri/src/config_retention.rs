//! Retention ages of closed and dead records, read by the database cleanup job.

use serde::{Deserialize, Serialize};

/// Days each store keeps a closed or dead record; 0 keeps it forever. Open
/// records are never deleted. Rules: `docs/backend/persistence.md`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub(crate) struct RetentionConfig {
    /// Progress entries, except a blocked entry that is still its terminal's newest.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) progress_days: u32,
    /// Finished automation runs and their deliveries.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) automation_run_days: u32,
    /// Peer mail of a recipient with no new mail and nothing unacknowledged.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) mail_days: u32,
    /// Tunnel audit events.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) tunnel_audit_days: u32,
    /// Daily MCP usage counts.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) mcp_usage_days: u32,
    /// Managed launch records that reached `created` or `blocked`.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) launch_days: u32,
    /// Config directories under `instances/` that no running process holds.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) instance_days: u32,
    /// Per-session capability and background-wake files of sessions no longer live.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) session_file_days: u32,
    /// PTY captures and frame workload recordings.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) capture_days: u32,
    /// Installed MCP bridge revisions other than the current one.
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Used by the next database cleanup.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) bridge_revision_days: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            progress_days: 30,
            automation_run_days: 90,
            mail_days: 14,
            tunnel_audit_days: 30,
            mcp_usage_days: 90,
            launch_days: 30,
            instance_days: 3,
            session_file_days: 30,
            capture_days: 14,
            bridge_revision_days: 7,
        }
    }
}
