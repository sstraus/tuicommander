//! PTY read API for plugins.
//!
//! Exposes read-only access to VT100 buffer contents (visible screen + recent
//! scrollback) for plugins that need to react to terminal state — e.g. reading
//! the agent's last reply to verify its content.
//!
//! Gated by the `pty:read` capability.

use crate::AppState;
use std::sync::Arc;

/// Maximum lines requestable via `plugin_read_session_output`.
const MAX_LINES: usize = 2000;
/// Default lines returned when `max_lines` is omitted.
const DEFAULT_LINES: usize = 200;

/// Read the VT100-decoded contents of a PTY session.
///
/// Returns the last `max_lines` of scrollback concatenated with the current
/// visible screen rows, joined by newlines. Alternate-screen agents (Claude
/// Code, Codex, Ink-based TUIs) only produce screen rows — scrollback is
/// empty for those sessions.
///
/// Requires the `pty:read` capability. Returns an error if the session does
/// not exist (closed or never created).
#[tauri::command]
pub async fn plugin_read_session_output(
    session_id: String,
    max_lines: Option<usize>,
    plugin_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(&state, &plugin_id, "pty:read")?;

    let vt_log = state
        .vt_log_buffers
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;

    let buf = vt_log.lock();
    let limit = max_lines.unwrap_or(DEFAULT_LINES).min(MAX_LINES);

    let total = buf.total_lines();
    let offset = total.saturating_sub(limit);
    let (log_lines, _) = buf.lines_since_owned(offset, limit);

    let screen: Vec<String> = buf
        .screen_rows()
        .into_iter()
        .filter(|r| !r.is_empty())
        .collect();

    let mut all: Vec<String> = log_lines.iter().map(|ll| ll.text()).collect();
    all.extend(screen);
    Ok(all.join("\n"))
}
