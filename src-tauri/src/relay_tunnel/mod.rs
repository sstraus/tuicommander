//! Desktop-to-remote-daemon tunnel through the cloud relay (story 2085-721d).
//!
//! The relay forwards opaque bytes. Everything the two endpoints say to each
//! other is sealed under a Noise channel keyed by a pairing secret that the
//! relay never sees (`secure`). The pairing material travels between the two
//! endpoints as a bundle (`bundle`), not through the relay.
//!
//! Design: `docs/design/2085-relay-remote-daemon.md`.

// DEFERRED (2026-10-22) — the modules are wired into the daemon agent and the
// desktop transport by the next commits of story 2085-721d; until then only
// their tests use them.
#![allow(dead_code)]

pub(crate) mod bridge;
pub(crate) mod bundle;
pub(crate) mod daemon;
pub(crate) mod desktop;
pub(crate) mod frames;
pub(crate) mod mux;
pub(crate) mod secure;
pub(crate) mod service;
pub(crate) mod ws_link;
