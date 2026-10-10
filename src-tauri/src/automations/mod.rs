//! Scheduled agent automation definitions and persistence.

pub mod definitions;
pub mod model;
pub mod precheck;
pub mod schedule;

#[cfg(test)]
mod tests;

pub mod run;
pub mod store;

pub mod scheduler;

#[cfg(test)]
mod mcp_tests;

pub mod actions;
pub mod mcp;

pub(crate) mod dispatcher;
pub(crate) mod runtime;

pub(crate) mod api;
