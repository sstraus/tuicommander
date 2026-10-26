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
pub(crate) mod completion;
pub(crate) mod templates;

pub(crate) mod context;
pub(crate) mod notifications;

pub(crate) mod worktrees;

pub(crate) mod create;

pub(crate) mod event_intake;
pub(crate) mod publish;
