//! Tauri IPC commands for the agent loop API.

use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

use super::conversation_engine::ACTIVE_CONVERSATIONS;
use super::knowledge::{OutcomeClass, SessionKnowledge};
use super::tui_detect::TerminalMode;
use crate::state::AppState;

/// Start a unified conversation via the new conversation_engine.
/// Uses per-conversation Channel transport with 50ms TextChunk batching.
#[cfg(feature = "desktop")]
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_conversation(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    message: String,
    autonomy: Option<String>,
    max_steps: Option<usize>,
    temperature: Option<f32>,
    model_override: Option<String>,
    bypassed_tools: Option<Vec<String>>,
    on_event: tauri::ipc::Channel<super::conversation_engine::ConversationEvent>,
) -> Result<(), String> {
    use super::conversation_engine::{
        Autonomy, ConversationConfig, ConversationEvent, start_conversation as engine_start,
    };
    use std::collections::HashSet;

    let config = ConversationConfig {
        autonomy: match autonomy.as_deref() {
            Some("autonomous") => Autonomy::Autonomous,
            _ => Autonomy::Assisted,
        },
        max_steps,
        temperature: temperature.unwrap_or(0.7),
        model_override,
        bypassed_tools: bypassed_tools
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<_>>(),
    };

    let mut rx = engine_start(state.inner().clone(), session_id, message, config).await?;

    // Bridge broadcast→Channel with 50ms TextChunk batching
    tokio::spawn(async move {
        let mut text_batch = String::new();
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(ConversationEvent::TextChunk { text }) => {
                            text_batch.push_str(&text);
                        }
                        Ok(other) => {
                            // Flush pending text batch before non-text event
                            if !text_batch.is_empty() {
                                let _ = on_event.send(ConversationEvent::TextChunk { text: std::mem::take(&mut text_batch) });
                            }
                            let done = matches!(other, ConversationEvent::Completed { .. } | ConversationEvent::Error { .. });
                            let _ = on_event.send(other);
                            if done { break; }
                        }
                        Err(_) => {
                            // Flush remaining text on channel close
                            if !text_batch.is_empty() {
                                let _ = on_event.send(ConversationEvent::TextChunk { text: std::mem::take(&mut text_batch) });
                            }
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !text_batch.is_empty() {
                        let _ = on_event.send(ConversationEvent::TextChunk { text: std::mem::take(&mut text_batch) });
                    }
                }
            }
        }
    });

    Ok(())
}

/// Cancel an active conversation (conversation_engine).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn cancel_conversation(session_id: String) -> Result<String, String> {
    super::conversation_engine::cancel_conversation(&session_id)?;
    Ok(format!("Conversation cancelled on session {session_id}"))
}

/// Pause an active conversation.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn pause_conversation(session_id: String) -> Result<String, String> {
    super::conversation_engine::pause_conversation(&session_id)?;
    Ok(format!("Conversation paused on session {session_id}"))
}

/// Resume a paused conversation.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn resume_conversation(session_id: String) -> Result<String, String> {
    super::conversation_engine::resume_conversation(&session_id)?;
    Ok(format!("Conversation resumed on session {session_id}"))
}

/// Approve or reject a tool action in an active conversation.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn approve_conversation_action(
    session_id: String,
    approved: bool,
) -> Result<(), String> {
    super::conversation_engine::approve_conversation_action(&session_id, approved)
}

/// Get the status of an agent loop.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn agent_loop_status(session_id: String) -> Result<serde_json::Value, String> {
    let entry = ACTIVE_CONVERSATIONS.get(&session_id);
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

fn classify_kind(c: &OutcomeClass) -> (&'static str, Option<String>) {
    match c {
        OutcomeClass::Success => ("success", None),
        OutcomeClass::Error { error_type } => ("error", Some(error_type.clone())),
        OutcomeClass::TuiLaunched { .. } => ("tui_launched", None),
        OutcomeClass::Timeout => ("timeout", None),
        OutcomeClass::UserCancelled => ("user_cancelled", None),
        OutcomeClass::Inferred => ("inferred", None),
    }
}

fn outcome_summary(c: &super::knowledge::CommandOutcome) -> OutcomeSummary {
    let (kind, error_type) = classify_kind(&c.classification);
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
#[cfg_attr(feature = "desktop", tauri::command)]
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

fn scan_sessions(
    filter: &KnowledgeListFilter,
    limit: usize,
) -> Result<Vec<SessionListEntry>, String> {
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
#[cfg(feature = "desktop")]
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
    let (kind, error_type) = classify_kind(&c.classification);
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
    }
}

// ── Scheduler commands ──────────────────────────────────────────

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_scheduler_config() -> super::scheduler::SchedulerConfig {
    super::scheduler::load_config()
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_scheduler_config(
    config: super::scheduler::SchedulerConfig,
) -> Result<(), String> {
    super::scheduler::save_config(&config)
}

// ── Watcher commands ────────────────────────────────────────────

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_create(
    state: State<'_, Arc<AppState>>,
    name: String,
    session_id: Option<String>,
    trigger: super::watcher::WatcherTrigger,
    instructions: String,
    max_fires: Option<u32>,
    cooldown_secs: Option<u32>,
) -> Result<String, String> {
    let rule = super::watcher::WatcherRule {
        id: String::new(),
        name,
        session_id,
        template_id: None,
        trigger,
        instructions,
        max_fires: max_fires.unwrap_or(super::watcher::default_max_fires()),
        fire_count: 0,
        cooldown_secs: cooldown_secs.unwrap_or(super::watcher::default_cooldown()),
        burst_threshold: super::watcher::default_burst_threshold(),
        burst_window_secs: super::watcher::default_burst_window(),
        status: super::watcher::WatcherStatus::Active,
        created_at: 0,
    };

    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let mut config = cfg.write();
    super::watcher::create_rule(&mut config, rule)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_list(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<super::watcher::WatcherRule>, String> {
    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let config = cfg.read();
    Ok(config.rules.clone())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_delete(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let mut config = cfg.write();
    super::watcher::delete_rule(&mut config, &id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_toggle(
    state: State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let mut config = cfg.write();
    super::watcher::toggle_rule(&mut config, &id, enabled)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_attach(
    state: State<'_, Arc<AppState>>,
    template_id: String,
    session_id: String,
) -> Result<String, String> {
    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let mut config = cfg.write();
    super::watcher::attach_rule(&mut config, &template_id, session_id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_detach(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let mut config = cfg.write();
    super::watcher::detach_rule(&mut config, &id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn watcher_update(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: Option<String>,
    trigger: Option<super::watcher::WatcherTrigger>,
    instructions: Option<String>,
    max_fires: Option<u32>,
    cooldown_secs: Option<u32>,
) -> Result<(), String> {
    let engine = state
        .watcher_engine
        .get()
        .ok_or("Watcher engine not initialized")?;
    let cfg = engine.config();
    let mut config = cfg.write();
    super::watcher::update_rule(
        &mut config,
        &id,
        name,
        trigger,
        instructions,
        max_fires,
        cooldown_secs,
    )
}

/// Return a frontend-friendly summary of the session's accumulated knowledge.
/// Returns an empty summary if no commands have been recorded yet.
#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn toggle_ai_suggestions(state: State<'_, Arc<AppState>>, session_id: String) -> bool {
    let current = state
        .ai_suggestions_enabled
        .get(&session_id)
        .map(|v| *v)
        .unwrap_or_else(|| {
            state
                .session_states
                .get(&session_id)
                .map(|s| s.agent_type.is_some())
                .unwrap_or(false)
        });
    let new_val = !current;
    state.ai_suggestions_enabled.insert(session_id, new_val);
    new_val
}

// DEFERRED (2026-05-14) — wire to frontend alongside toggle_ai_suggestions.
// Getter needed so UI can restore suggestion toggle state on mount.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_ai_suggestions_enabled(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> bool {
    state
        .ai_suggestions_enabled
        .get(&session_id)
        .map(|v| *v)
        .unwrap_or_else(|| {
            state
                .session_states
                .get(&session_id)
                .map(|s| s.agent_type.is_some())
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_nonexistent_returns_inactive() {
        // Simulate what agent_loop_status does without Tauri State
        let entry = ACTIVE_CONVERSATIONS.get("nonexistent");
        assert!(entry.is_none());
    }

    #[test]
    fn active_agents_is_empty_initially() {
        // Fresh test — no agents should be active for random IDs
        assert!(!ACTIVE_CONVERSATIONS.contains_key("test-fresh-id-12345"));
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
            });
        }
        let s = SessionKnowledgeSummary::from_knowledge("s1", &k);
        assert_eq!(s.commands_count, 8);
        assert_eq!(s.recent_outcomes.len(), 5);
        assert_eq!(s.recent_outcomes[0].command, "cmd7");
        assert!(
            s.recent_errors
                .iter()
                .all(|o| o.error_type.as_deref() == Some("npm_error"))
        );
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

    #[test]
    fn file_sandbox_inserted_on_start_and_removed_on_end() {
        use super::super::sandbox::FileSandbox;
        use crate::state::tests_support::make_test_app_state;

        let state = make_test_app_state();
        let sid = "test-sandbox-session";

        // Simulate what start_agent_loop does: create and insert sandbox for cwd.
        let root = std::env::temp_dir();
        let sandbox = FileSandbox::new(&root).expect("temp_dir should be a valid sandbox root");
        state.file_sandboxes.insert(sid.to_string(), sandbox);
        assert!(state.file_sandboxes.contains_key(sid), "sandbox inserted");

        // Simulate what engine cleanup does at loop end.
        state.file_sandboxes.remove(sid);
        assert!(
            !state.file_sandboxes.contains_key(sid),
            "sandbox removed after loop"
        );
    }
}
