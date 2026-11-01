#[cfg(feature = "desktop")]
mod browser;
#[cfg(feature = "desktop")]
mod command_channel;
#[cfg(feature = "desktop")]
pub(crate) mod commands;
pub(crate) mod extension;
#[cfg(feature = "desktop")]
mod extension_process;
#[cfg(feature = "desktop")]
mod legacy;
#[cfg(feature = "desktop")]
mod liveness;
#[cfg(feature = "desktop")]
pub(crate) mod manager;
mod page_errors;
mod payload;
mod source;
#[cfg(feature = "desktop")]
pub(crate) mod tauri_commands;
#[cfg(feature = "desktop")]
pub(crate) use legacy::*;
#[cfg(all(test, unix))]
mod demo;
pub(crate) mod discovery;
mod hints;
#[cfg(feature = "desktop")]
pub(crate) mod launcher;
pub(crate) mod send;

mod captures;
#[cfg(feature = "desktop")]
mod source_cdp;

pub(crate) mod mcp;
