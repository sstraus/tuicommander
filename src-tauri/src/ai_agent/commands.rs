//! Tauri IPC commands for the agent loop API.

use std::sync::Arc;
use tauri::State;

use crate::state::AppState;
use super::engine::{self, ACTIVE_AGENTS};
use super::knowledge::{OutcomeClass, SessionKnowledge};
use super::tui_detect::TerminalMode;

/// Start an agent loop on a terminal session.
#[tauri::command]
pub(crate) async fn start_agent_loop(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    goal: String,
) -> Result<String, String> {
    let state = Arc::clone(&state);
    let app_handle = state.app_handle.read().clone();
    let mut rx = engine::start_agent_loop(state, session_id.clone(), goal).await?;

    // Bridge broadcast events to Tauri's emit system so the frontend can
    // subscribe via `listen("agent-loop-event", ...)`.
    if let Some(handle) = app_handle {
        tokio::spawn(async move {
            use tauri::Emitter;
            while let Ok(event) = rx.recv().await {
                let _ = handle.emit("agent-loop-event", &event);
            }
        });
    }

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

/// Approve or reject a pending destructive command from the agent.
/// Resolves the oneshot channel that the engine is blocking on.
#[tauri::command]
pub(crate) async fn approve_agent_action(
    session_id: String,
    approved: bool,
) -> Result<String, String> {
    let entry = ACTIVE_AGENTS.get(&session_id)
        .ok_or_else(|| format!("No active agent on session {session_id}"))?;
    let tx = entry.approval_tx.lock().take()
        .ok_or_else(|| format!("No pending approval on session {session_id}"))?;
    tx.send(approved)
        .map_err(|_| format!("Approval channel closed for session {session_id}"))?;
    Ok(format!(
        "Action {} for session {session_id}",
        if approved { "approved" } else { "rejected" }
    ))
}

/// Compact outcome shape for the frontend session-knowledge bar. Omits full
/// snippet + cwd history; those are only needed by the agent loop's context
/// injection.
#[derive(serde::Serialize, Clone, Debug)]
pub(crate) struct OutcomeSummary {
    pub timestamp: u64,
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub kind: String,
    pub error_type: Option<String>,
}

/// Shape consumed by `SessionKnowledgeBar.tsx`. Lightweight — no full
/// `SessionKnowledge` fields that the UI doesn't need yet.
#[derive(serde::Serialize, Clone, Debug)]
pub(crate) struct SessionKnowledgeSummary {
    pub session_id: String,
    pub commands_count: usize,
    pub recent_outcomes: Vec<OutcomeSummary>,
    pub recent_errors: Vec<OutcomeSummary>,
    pub tui_mode: Option<String>,
    pub tui_apps_seen: Vec<String>,
}

impl SessionKnowledgeSummary {
    pub(crate) fn from_knowledge(session_id: &str, k: &SessionKnowledge) -> Self {
        let recent_outcomes: Vec<OutcomeSummary> = k
            .commands
            .iter()
            .rev()
            .take(5)
            .map(outcome_summary)
            .collect();
        let recent_errors: Vec<OutcomeSummary> = k
            .commands
            .iter()
            .rev()
            .filter(|c| matches!(c.classification, OutcomeClass::Error { .. }))
            .take(5)
            .map(outcome_summary)
            .collect();
        let tui_mode = match &k.terminal_mode {
            TerminalMode::Shell => None,
            TerminalMode::FullscreenTui { app_hint, depth } => Some(match app_hint {
                Some(a) => format!("{a} (depth {depth})"),
                None => format!("fullscreen (depth {depth})"),
            }),
        };
        let mut apps: Vec<String> = k.tui_apps_seen.iter().cloned().collect();
        apps.sort();
        Self {
            session_id: session_id.to_string(),
            commands_count: k.commands.len(),
            recent_outcomes,
            recent_errors,
            tui_mode,
            tui_apps_seen: apps,
        }
    }
}

fn outcome_summary(c: &super::knowledge::CommandOutcome) -> OutcomeSummary {
    let (kind, error_type) = match &c.classification {
        OutcomeClass::Success => ("success", None),
        OutcomeClass::Error { error_type } => ("error", Some(error_type.clone())),
        OutcomeClass::TuiLaunched { .. } => ("tui_launched", None),
        OutcomeClass::Timeout => ("timeout", None),
        OutcomeClass::UserCancelled => ("user_cancelled", None),
        OutcomeClass::Inferred => ("inferred", None),
    };
    OutcomeSummary {
        timestamp: c.timestamp,
        command: c.command.clone(),
        exit_code: c.exit_code,
        duration_ms: c.duration_ms,
        kind: kind.to_string(),
        error_type,
    }
}

/// Return a frontend-friendly summary of the session's accumulated knowledge.
/// Returns an empty summary if no commands have been recorded yet.
#[tauri::command]
pub(crate) async fn get_session_knowledge(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<SessionKnowledgeSummary, String> {
    let entry = state.session_knowledge.get(&session_id);
    let summary = match entry {
        Some(e) => SessionKnowledgeSummary::from_knowledge(&session_id, &e.lock()),
        None => SessionKnowledgeSummary {
            session_id: session_id.clone(),
            commands_count: 0,
            recent_outcomes: Vec::new(),
            recent_errors: Vec::new(),
            tui_mode: None,
            tui_apps_seen: Vec::new(),
        },
    };
    Ok(summary)
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

    #[test]
    fn session_knowledge_summary_from_empty() {
        let k = SessionKnowledge::new();
        let s = SessionKnowledgeSummary::from_knowledge("s1", &k);
        assert_eq!(s.commands_count, 0);
        assert!(s.recent_outcomes.is_empty());
        assert!(s.recent_errors.is_empty());
        assert!(s.tui_mode.is_none());
    }

    #[test]
    fn session_knowledge_summary_collects_recent_outcomes_and_errors() {
        use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass};
        let mut k = SessionKnowledge::new();
        for i in 0..8 {
            k.record(CommandOutcome {
                timestamp: i,
                command: format!("cmd{i}"),
                cwd: "/".into(),
                exit_code: Some(0),
                output_snippet: String::new(),
                classification: if i % 2 == 0 {
                    OutcomeClass::Success
                } else {
                    OutcomeClass::Error {
                        error_type: "npm_error".into(),
                    }
                },
                duration_ms: 1,
            });
        }
        let s = SessionKnowledgeSummary::from_knowledge("s1", &k);
        assert_eq!(s.commands_count, 8);
        assert_eq!(s.recent_outcomes.len(), 5);
        assert_eq!(s.recent_outcomes[0].command, "cmd7");
        assert!(s
            .recent_errors
            .iter()
            .all(|o| o.error_type.as_deref() == Some("npm_error")));
    }

    #[test]
    fn session_knowledge_summary_reports_tui_mode() {
        let mut k = SessionKnowledge::new();
        k.terminal_mode = TerminalMode::FullscreenTui {
            app_hint: Some("vim".into()),
            depth: 1,
        };
        let s = SessionKnowledgeSummary::from_knowledge("s1", &k);
        assert_eq!(s.tui_mode.as_deref(), Some("vim (depth 1)"));
    }
}
