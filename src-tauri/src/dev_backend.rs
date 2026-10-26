//! Debug-only launch identity for Vite's Cargo dep-info staleness check.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static PROCESS_START: OnceLock<SystemTime> = OnceLock::new();

/// Record entry before context generation and application setup can block.
pub fn record_process_start() {
    let _ = PROCESS_START.set(SystemTime::now());
}

pub(crate) fn identity() -> serde_json::Value {
    let exe = std::env::current_exe().ok();
    // make dev's macOS runner executes a protected copy outside Cargo's target.
    // The runner supplies the original path; never guess an mbx target view.
    let build_exe = std::env::var_os("TUIC_DEV_BUILD_EXE")
        .map(std::path::PathBuf::from)
        .or_else(|| exe.clone());
    let bridge = crate::agent_mcp::locate_bridge_binary().map(|bridge| {
        if bridge.parent() == exe.as_deref().and_then(std::path::Path::parent)
            && let Some(dir) = build_exe.as_deref().and_then(std::path::Path::parent)
            && let Some(name) = bridge.file_name()
        {
            dir.join(name)
        } else {
            bridge
        }
    });
    serde_json::json!({
        "exe": exe,
        "build_exe": build_exe,
        "bridge_exe": bridge,
        "started_at_ms": PROCESS_START.get()
            .and_then(|start| start.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis()),
        "git_hash": env!("BUILD_GIT_HASH"),
    })
}
