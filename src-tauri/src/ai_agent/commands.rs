//! Tauri IPC commands for the agent loop API.

use std::sync::Arc;
use tauri::State;

use crate::state::AppState;
use super::engine::{self, ACTIVE_AGENTS, AgentState};

/// Start an agent loop on a terminal session.
#[tauri::command]
pub(crate) async fn start_agent_loop(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    goal: String,
) -> Result<String, String> {
    let state = Arc::clone(&state);
    let _rx = engine::start_agent_loop(state, session_id.clone(), goal).await?;
    Ok(format!("Agent started on session {session_id}"))
}

/// Cancel an active agent loop.
#[tauri::command]
pub(crate) async fn cancel_agent_loop(
    session_id: String,
) -> Result<String, String> {
    engine::cancel_agent_loop(&session_id)?;
    Ok(format!("Agent cancelled on session {session_id}"))
}

/// Pause an active agent loop.
#[tauri::command]
pub(crate) async fn pause_agent_loop(
    session_id: String,
) -> Result<String, String> {
    engine::pause_agent_loop(&session_id)?;
    Ok(format!("Agent paused on session {session_id}"))
}

/// Resume a paused agent loop.
#[tauri::command]
pub(crate) async fn resume_agent_loop(
    session_id: String,
) -> Result<String, String> {
    engine::resume_agent_loop(&session_id)?;
    Ok(format!("Agent resumed on session {session_id}"))
}

/// Get the status of an agent loop.
#[tauri::command]
pub(crate) async fn agent_loop_status(
    session_id: String,
) -> Result<serde_json::Value, String> {
    let entry = ACTIVE_AGENTS.get(&session_id);
    match entry {
        Some(handle) => {
            let state = *handle.state.read();
            Ok(serde_json::json!({
                "active": true,
                "state": state,
                "session_id": session_id,
            }))
        }
        None => Ok(serde_json::json!({
            "active": false,
            "state": null,
            "session_id": session_id,
        })),
    }
}

/// Approve a pending destructive command from the agent.
/// (Placeholder — will be wired to an approval channel in the engine.)
#[tauri::command]
pub(crate) async fn approve_agent_action(
    session_id: String,
    approved: bool,
) -> Result<String, String> {
    if !ACTIVE_AGENTS.contains_key(&session_id) {
        return Err(format!("No active agent on session {session_id}"));
    }
    // TODO: Wire to approval oneshot channel in engine when NeedsApproval
    // flow is implemented end-to-end.
    Ok(format!(
        "Action {} for session {session_id}",
        if approved { "approved" } else { "rejected" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_nonexistent_errors() {
        let result = engine::cancel_agent_loop("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn pause_nonexistent_errors() {
        let result = engine::pause_agent_loop("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn resume_nonexistent_errors() {
        let result = engine::resume_agent_loop("nonexistent");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn status_nonexistent_returns_inactive() {
        // Simulate what agent_loop_status does without Tauri State
        let entry = ACTIVE_AGENTS.get("nonexistent");
        assert!(entry.is_none());
    }

    #[test]
    fn active_agents_is_empty_initially() {
        // Fresh test — no agents should be active for random IDs
        assert!(!ACTIVE_AGENTS.contains_key("test-fresh-id-12345"));
    }
}
