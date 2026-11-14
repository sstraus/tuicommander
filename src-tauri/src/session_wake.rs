//! The shared transition from a retained or standby session to one live PTY.
use crate::{AppState, suspended_sessions};
use axum::Json;
use std::sync::Arc;

/// Resolve durable addresses without treating external ACP peers as terminals.
pub(crate) fn retained(
    reference: &str,
) -> Result<Option<suspended_sessions::SuspendedSession>, String> {
    let matches: Vec<_> = suspended_sessions::list()?
        .into_values()
        .filter(|record| {
            record.tuic_session == reference
                || record.session_id == reference
                || record.alias.as_deref() == Some(reference)
        })
        .collect();
    if matches.len() > 1 {
        return Err("Ambiguous suspended session reference".into());
    }
    Ok(matches.into_iter().next())
}

/// The suspend lock serializes wake and suspend; the live map is checked inside it.
/// A second caller therefore receives the first caller's PTY, not a second spawn.
pub(crate) fn ensure_live(state: &Arc<AppState>, reference: &str) -> Result<String, String> {
    let _guard = state.suspend_lock.lock();
    let resolved = state
        .resolve_session_ref_checked(reference)?
        .unwrap_or_else(|| reference.into());
    if state.session_maps.sessions.contains_key(&resolved) {
        #[cfg(unix)]
        crate::pty::wake_session(state, &resolved)?;
        return Ok(resolved);
    }
    let record = retained(reference)?.ok_or("Session not found")?;
    let identity = record.tuic_session.as_str();
    if !std::path::Path::new(&record.cwd).is_dir() {
        return Err(format!(
            "Resume working directory is unavailable: {}",
            record.cwd
        ));
    }
    let command = if record.agent_type.is_some() {
        Some(
            crate::session_resume::resolve(&record.resume_request())
                .cmd
                .ok_or("no resumable agent session")?,
        )
    } else {
        None
    };
    if state.session_maps.sessions.len() >= crate::MAX_CONCURRENT_SESSIONS {
        return Err("Max concurrent sessions reached".into());
    }
    let session_id = crate::mcp_http::session::spawn_pty_session(
        state.clone(),
        record.shell.clone(),
        Some(record.cwd.clone()),
        24,
        80,
        record.worktree.clone(),
        crate::mcp_http::session::RequestedIdentity {
            session_id: None,
            tuic_session: Some(identity.into()),
            alias: record.alias.clone(),
            startup_command: command,
            agent_type: record.agent_type.clone(),
            restored: true,
        },
    )
    .map_err(|(_, Json(error))| error.to_string())?;
    if let Err(error) = suspended_sessions::remove(identity) {
        crate::pty::close_pty_core_with_reason(state, &session_id, false, "resume_storage_failed");
        return Err(error);
    }

    state.hibernated_peers.remove(identity);
    Ok(session_id)
}
