//! Scheduled agent automation definitions and persistence.
//!
//! Boundary with workflows: an automation is one scheduled agent prompt with
//! its own definitions and `automation_runs` history. A workflow with a
//! `schedule` trigger is a composed run in `workflow_runs`. Both share one
//! timer (this module's runtime tick, which calls
//! `workflows::scheduled_trigger::tick`) and one cron parser (`schedule`).

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
