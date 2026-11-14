//! Workflow duration ceiling, shared without domain scheduling policy.

/// Seven days, in seconds.
pub(crate) const MAX_DURATION_SECS: u32 = 604_800;
pub(crate) const MAX_DURATION_MS: u64 = MAX_DURATION_SECS as u64 * 1_000;
