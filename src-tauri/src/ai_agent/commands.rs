//! Tauri IPC commands for the agent loop API.

use std::sync::Arc;
use tauri::State;

use crate::state::AppState;
use super::engine::{self, ACTIVE_AGENTS, LlmRuntime, TrustLevel};
use super::knowledge::{OutcomeClass, SessionKnowledge};
use super::tui_detect::TerminalMode;

/// Build the LLM runtime (provider/model/base_url/api key) that the agent
/// loop uses. Lives here — at the Tauri command boundary — so `engine.rs`
/// stays decoupled from `ai_chat` config and keyring I/O.
pub(crate) fn build_llm_runtime_for_scheduler() -> Result<LlmRuntime, String> {
    build_llm_runtime()
}

fn build_llm_runtime() -> Result<LlmRuntime, String> {
    let chat_config: crate::ai_chat::AiChatConfig =
        crate::config::load_json_config(crate::ai_chat::CONFIG_FILE);
    let api_key = if chat_config.provider == "ollama" {
        crate::ai_chat::read_api_key()?.unwrap_or_else(|| "ollama".to_string())
    } else {
        crate::ai_chat::read_api_key()?
            .ok_or_else(|| "No API key stored — add one in Settings > AI Chat".to_string())?
    };
    let config = crate::llm_api::LlmApiConfig {
        provider: chat_config.provider.clone(),
        model: chat_config.model.clone(),
        base_url: chat_config.effective_base_url(),
    };
    let model_overrides = chat_config.agent_model_overrides.clone().unwrap_or_default();
    Ok(LlmRuntime { config, api_key, model_overrides })
}

/// Start an agent loop on a terminal session.
#[tauri::command]
pub(crate) async fn start_agent_loop(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    goal: String,
    #[allow(unused_variables)]
    unrestricted: Option<bool>,
) -> Result<String, String> {
    let state = Arc::clone(&state);
    let app_handle = state.app_handle.read().clone();
    let runtime = build_llm_runtime()?;
    let trust_level = if unrestricted.unwrap_or(false) {
        TrustLevel::Unrestricted
    } else {
        TrustLevel::Standard
    };
    let mut rx =
        engine::start_agent_loop(state, session_id.clone(), goal, runtime, trust_level).await?;

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

/// One row of the knowledge history list. Compact enough to paginate hundreds
/// without loading every session's full command list.
#[derive(serde::Serialize, Clone, Debug)]
pub(crate) struct SessionListEntry {
    pub session_id: String,
    /// Timestamp of the most recent recorded outcome (or file mtime if the
    /// session never ran a command).
    pub last_activity: u64,
    pub commands_count: usize,
    pub errors_count: usize,
    pub last_cwd: Option<String>,
    pub tui_apps_seen: Vec<String>,
}

#[derive(serde::Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeListFilter {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub has_errors: Option<bool>,
    /// UNIX seconds — lower bound on last_activity (inclusive).
    #[serde(default)]
    pub since: Option<u64>,
}

/// List persisted sessions sorted by most recent activity. Scans
/// `ai-sessions/` on every call (no in-memory index yet) — acceptable up
/// to a few hundred files; upgrade path is a sidecar index if this becomes
/// a bottleneck.
#[tauri::command]
pub(crate) async fn list_knowledge_sessions(
    filter: Option<KnowledgeListFilter>,
    limit: Option<usize>,
) -> Result<Vec<SessionListEntry>, String> {
    let filter = filter.unwrap_or_default();
    let limit = limit.unwrap_or(100).min(500);
    let rows = tokio::task::spawn_blocking(move || scan_sessions(&filter, limit))
        .await
        .map_err(|e| format!("list task join error: {e}"))??;
    Ok(rows)
}

fn scan_sessions(filter: &KnowledgeListFilter, limit: usize) -> Result<Vec<SessionListEntry>, String> {
    use crate::ai_agent::knowledge as kb;
    let dir = crate::config::config_dir().join("ai-sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let needle = filter
        .text
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let mut rows: Vec<SessionListEntry> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(sid) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(k) = kb::load(sid) else { continue };
        let commands_count = k.commands.len();
        let errors_count = k
            .commands
            .iter()
            .filter(|c| matches!(c.classification, OutcomeClass::Error { .. }))
            .count();
        if filter.has_errors == Some(true) && errors_count == 0 {
            continue;
        }
        let last_activity = k
            .commands
            .iter()
            .map(|c| c.timestamp)
            .max()
            .unwrap_or_else(|| {
                entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            });
        if let Some(since) = filter.since
            && last_activity < since
        {
            continue;
        }
        if let Some(n) = &needle {
            let hit = k.commands.iter().any(|c| {
                c.command.to_lowercase().contains(n)
                    || c.output_snippet.to_lowercase().contains(n)
                    || c.semantic_intent
                        .as_deref()
                        .is_some_and(|s| s.to_lowercase().contains(n))
                    || match &c.classification {
                        OutcomeClass::Error { error_type } => error_type.to_lowercase().contains(n),
                        _ => false,
                    }
            });
            if !hit {
                continue;
            }
        }
        let last_cwd = k.cwd_history.front().map(|(p, _)| p.clone());
        let mut apps: Vec<String> = k.tui_apps_seen.iter().cloned().collect();
        apps.sort();
        rows.push(SessionListEntry {
            session_id: sid.to_string(),
            last_activity,
            commands_count,
            errors_count,
            last_cwd,
            tui_apps_seen: apps,
        });
    }
    rows.sort_by_key(|a| std::cmp::Reverse(a.last_activity));
    rows.truncate(limit);
    Ok(rows)
}

/// Detail of one command in the full-history view. Mirrors `CommandOutcome`
/// but with `kind`/`error_type` pre-extracted so the frontend does not need
/// to pattern-match on the tagged enum.
#[derive(serde::Serialize, Clone, Debug)]
pub(crate) struct HistoryCommand {
    pub id: u64,
    pub timestamp: u64,
    pub command: String,
    pub cwd: String,
    pub exit_code: Option<i32>,
    pub output_snippet: String,
    pub duration_ms: u64,
    pub kind: String,
    pub error_type: Option<String>,
    pub semantic_intent: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub(crate) struct SessionDetail {
    pub session_id: String,
    pub commands: Vec<HistoryCommand>,
    pub tui_apps_seen: Vec<String>,
    pub cwd_history: Vec<(String, u64)>,
}

/// Load the full command history for one session. Reads from disk if the
/// session is not currently active — covers the "inspect last week's
/// session" case from the story.
#[tauri::command]
pub(crate) async fn get_knowledge_session_detail(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<Option<SessionDetail>, String> {
    if let Some(entry) = state.session_knowledge.get(&session_id) {
        let k = entry.lock();
        return Ok(Some(to_detail(&session_id, &k)));
    }
    let sid_load = session_id.clone();
    let loaded = tokio::task::spawn_blocking(move || crate::ai_agent::knowledge::load(&sid_load))
        .await
        .map_err(|e| format!("load task join error: {e}"))?;
    Ok(loaded.map(|k| to_detail(&session_id, &k)))
}

fn to_detail(session_id: &str, k: &SessionKnowledge) -> SessionDetail {
    let commands = k.commands.iter().map(history_command).collect();
    let mut apps: Vec<String> = k.tui_apps_seen.iter().cloned().collect();
    apps.sort();
    SessionDetail {
        session_id: session_id.to_string(),
        commands,
        tui_apps_seen: apps,
        cwd_history: k.cwd_history.iter().cloned().collect(),
    }
}

fn history_command(c: &super::knowledge::CommandOutcome) -> HistoryCommand {
    let (kind, error_type) = match &c.classification {
        OutcomeClass::Success => ("success", None),
        OutcomeClass::Error { error_type } => ("error", Some(error_type.clone())),
        OutcomeClass::TuiLaunched { .. } => ("tui_launched", None),
        OutcomeClass::Timeout => ("timeout", None),
        OutcomeClass::UserCancelled => ("user_cancelled", None),
        OutcomeClass::Inferred => ("inferred", None),
    };
    HistoryCommand {
        id: c.id,
        timestamp: c.timestamp,
        command: c.command.clone(),
        cwd: c.cwd.clone(),
        exit_code: c.exit_code,
        output_snippet: c.output_snippet.clone(),
        duration_ms: c.duration_ms,
        kind: kind.to_string(),
        error_type,
        semantic_intent: c.semantic_intent.clone(),
    }
}

// ── Scheduler commands ──────────────────────────────────────────

#[tauri::command]
pub(crate) fn load_scheduler_config() -> super::scheduler::SchedulerConfig {
    super::scheduler::load_config()
}

#[tauri::command]
pub(crate) fn save_scheduler_config(
    config: super::scheduler::SchedulerConfig,
) -> Result<(), String> {
    super::scheduler::save_config(&config)
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
                id: 0,
                semantic_intent: None,
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
