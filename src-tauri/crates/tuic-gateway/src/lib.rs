//! Provider gateway: pure ports and policy for every outbound GitHub, GitLab
//! and Jira request. No I/O lives here; `Transport` and `Clock` are the only
//! doors to the outside world, so policy is testable without a network or
//! wall-clock time.
//!
//! Add one `pub mod <name>;` line per module, in alphabetical order.

pub mod budget;
pub mod cache;
pub mod coalesce;
pub mod exchange;
pub mod failure;
pub mod gateway;
pub mod headers;
pub mod invalidate;
pub mod l2;
pub mod metrics;
pub mod model;
pub mod policy;
pub mod ports;
pub mod routes;
pub mod stream;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

#[cfg(test)]
mod tests;
