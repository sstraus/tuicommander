//! Agent loop engine — ReAct pattern (Reason → Act → Observe → repeat).
//!
//! Assembles context, calls the LLM with tools, dispatches tool calls,
//! feeds results back, and re-enters until the model stops requesting tools
//! or a termination condition is met.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::Serialize;

use super::tools;

// ── Constants ─────────────────────────────────────────────────

pub(crate) const MAX_ITERATIONS: usize = 20;
pub(crate) const LOOP_TIMEOUT: Duration = Duration::from_secs(300); // 5 min
pub(crate) const MAX_IDENTICAL_CALLS: usize = 3;
pub(crate) const RATE_WINDOW: Duration = Duration::from_secs(60);
pub(crate) const RATE_LIMIT_PER_MINUTE: usize = 30;
pub(crate) const RATE_LIMIT_PER_SESSION: usize = 200;
pub(crate) const TOOL_DISPATCH_LIMIT_PER_MINUTE: usize = 60;
pub(crate) const TOOL_DISPATCH_LIMIT_PER_SESSION: usize = 500;

// ── Active agents registry ────────────────────────────────────

#[allow(dead_code)]
pub(crate) static ACTIVE_AGENTS: std::sync::LazyLock<DashMap<String, AgentHandle>> =
    std::sync::LazyLock::new(DashMap::new);

/// Handle to a running agent loop — used for cancel and status queries.
#[allow(dead_code)]
pub(crate) struct AgentHandle {
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<RwLock<AgentState>>,
}

// ── Agent state ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Running,
    Paused,
    Completed,
    Cancelled,
    Error,
}

// ── Rate limiter ──────────────────────────────────────────────

pub(crate) struct RateLimiter {
    window: VecDeque<tokio::time::Instant>,
    total: usize,
    per_minute: usize,
    per_session: usize,
}

impl RateLimiter {
    pub(crate) fn new(per_minute: usize, per_session: usize) -> Self {
        Self {
            window: VecDeque::new(),
            total: 0,
            per_minute,
            per_session,
        }
    }

    /// Check if we can make another call. Returns Ok or the wait duration.
    pub(crate) fn check(&mut self) -> Result<(), Duration> {
        let now = tokio::time::Instant::now();

        if self.total >= self.per_session {
            return Err(Duration::ZERO);
        }

        while let Some(&front) = self.window.front() {
            if now.duration_since(front) > RATE_WINDOW {
                self.window.pop_front();
            } else {
                break;
            }
        }

        if self.window.len() >= self.per_minute {
            let oldest = self
                .window
                .front()
                .expect("window non-empty after len check");
            let wait = RATE_WINDOW
                .checked_sub(now.duration_since(*oldest))
                .unwrap_or(Duration::ZERO);
            return Err(wait);
        }

        Ok(())
    }

    pub(crate) fn record(&mut self) {
        self.window.push_back(tokio::time::Instant::now());
        self.total += 1;
    }
}

// ── Repetition detector ───────────────────────────────────────

pub(crate) struct RepetitionDetector {
    recent_calls: VecDeque<String>,
}

impl RepetitionDetector {
    pub(crate) fn new() -> Self {
        Self {
            recent_calls: VecDeque::new(),
        }
    }

    /// Record a tool call signature and return true if it's a repetition.
    pub(crate) fn record(&mut self, signature: &str) -> bool {
        self.recent_calls.push_back(signature.to_string());
        if self.recent_calls.len() > MAX_IDENTICAL_CALLS {
            self.recent_calls.pop_front();
        }

        self.recent_calls.len() == MAX_IDENTICAL_CALLS
            && self.recent_calls.iter().all(|s| s == signature)
    }
}

// ── System prompt ─────────────────────────────────────────────

/// Concatenate the static base prompt with optional cross-session memory and
/// per-session knowledge sections (each separated by a blank line).
/// `cross_session` is injected once at session start and never refreshed.
/// `knowledge` is refreshed every iteration.
pub(crate) fn compose_system_prompt(
    base: &str,
    cross_session: Option<&str>,
    knowledge: Option<&str>,
) -> String {
    let mut out = base.to_string();
    if let Some(cs) = cross_session {
        out.push_str("\n\n");
        out.push_str(cs);
    }
    if let Some(k) = knowledge {
        out.push_str("\n\n");
        out.push_str(k);
    }
    out
}

#[allow(dead_code)]
fn build_system_prompt(session_id: &str) -> String {
    format!(
        "You are an AI agent controlling a terminal session (id: {session_id}).\n\n\
         ## Terminal tools\n\
         - read_screen / get_context — observe terminal state\n\
         - send_input — type a command into the interactive shell\n\
         - send_key — send a special key (ctrl+c, enter, …)\n\
         - wait_for — wait until a regex appears or the screen stabilizes\n\
         - get_state — structured session metadata (cwd, git, shell state)\n\n\
         ## Filesystem tools\n\
         - read_file — read a file with line numbers (paginated, max 2000 lines)\n\
         - write_file — create or overwrite a file (atomic, creates dirs)\n\
         - edit_file — surgical search-and-replace; include enough context in old_string for uniqueness\n\
         - list_files — glob-match files in the repo (e.g. `src/**/*.rs`)\n\
         - search_files — regex search across files with context lines\n\
         - run_command — run a shell command and capture stdout/stderr (not interactive)\n\n\
         ## Code search\n\
         - search_code — BM25 semantic search across the codebase by query string\n\n\
         ## MCP bridge\n\
         - search_tools — discover available MCP upstream tools (by keyword)\n\
         - call_tool — invoke an MCP upstream tool by name with arguments\n\n\
         ## Multi-session orchestration (unrestricted mode only)\n\
         - list_sessions — enumerate all active PTY sessions\n\
         - spawn_session — create a new PTY tab (returns session_id)\n\
         - get_agent_status — query another agent's state (running/paused/completed/etc.)\n\
         In unrestricted mode, send_input/send_key/read_screen accept any session_id, \
         enabling orchestration of other terminals (e.g. launching Claude Code in a new tab \
         and driving it via send_input).\n\n\
         ## When to use which\n\
         - **read_file vs read_screen**: use read_file for structured file access; \
         use read_screen only for interactive terminal output (TUI apps, command results already on screen).\n\
         - **run_command vs send_input**: use run_command when you need captured, parseable output \
         (builds, tests, grep). Use send_input for interactive programs that need a live PTY.\n\
         - **Edit workflow**: read_file first, then edit_file with enough surrounding context \
         in old_string to ensure uniqueness.\n\n\
         Always observe before acting. Prefer targeted, minimal commands. \
         When a task is complete, stop calling tools and summarize what you did."
    )
}

// ── JSON value redaction ─────────────────────────────────────

/// Recursively apply `redact_secrets` to all string values in a JSON value.
pub(crate) fn redact_json_values(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => serde_json::Value::String(tools::redact_secrets(s)),
        serde_json::Value::Object(map) => {
            let redacted: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), redact_json_values(v)))
                .collect();
            serde_json::Value::Object(redacted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(redact_json_values).collect())
        }
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tool phase classification — routes tool calls to cost-appropriate models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolPhase {
    Plan,
    Search,
    Read,
    Write,
}

pub(crate) fn classify_phase(tool_names: &[&str]) -> ToolPhase {
    let mut has_write = false;
    let mut has_search = false;
    let mut has_read = false;
    for &name in tool_names {
        match name {
            "write_file" | "edit_file" | "send_input" | "send_key" | "run_command"
            | "call_tool" | "spawn_session" => has_write = true,
            "search_files" | "search_code" | "list_files" | "search_tools" => has_search = true,
            "read_screen" | "read_file" | "get_state" | "get_context" | "list_sessions"
            | "get_agent_status" => has_read = true,
            _ => {}
        }
    }
    if has_write {
        ToolPhase::Write
    } else if has_search {
        ToolPhase::Search
    } else if has_read {
        ToolPhase::Read
    } else {
        ToolPhase::Plan
    }
}

pub(crate) fn select_model_for_phase<'a>(
    base: &'a str,
    overrides: &'a std::collections::HashMap<ToolPhase, String>,
    phase: ToolPhase,
) -> &'a str {
    match overrides.get(&phase) {
        Some(m) => m.as_str(),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── RateLimiter ────────────────────────────────────────────

    #[test]
    fn rate_limiter_allows_initial_call() {
        let mut rl = RateLimiter::new(RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION);
        assert!(rl.check().is_ok());
    }

    #[test]
    fn rate_limiter_blocks_after_burst() {
        let mut rl = RateLimiter::new(RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION);
        for _ in 0..RATE_LIMIT_PER_MINUTE {
            rl.check().unwrap();
            rl.record();
        }
        assert!(rl.check().is_err());
    }

    #[test]
    fn rate_limiter_session_limit() {
        let mut rl = RateLimiter::new(RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION);
        rl.total = RATE_LIMIT_PER_SESSION;
        let err = rl.check().unwrap_err();
        assert_eq!(err, Duration::ZERO);
    }

    #[test]
    fn tool_dispatch_limiter_separate_from_llm() {
        let mut llm = RateLimiter::new(RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION);
        let mut tool = RateLimiter::new(
            TOOL_DISPATCH_LIMIT_PER_MINUTE,
            TOOL_DISPATCH_LIMIT_PER_SESSION,
        );
        // Tool limiter allows more per minute than LLM limiter
        assert!(TOOL_DISPATCH_LIMIT_PER_MINUTE > RATE_LIMIT_PER_MINUTE);
        for _ in 0..RATE_LIMIT_PER_MINUTE {
            llm.record();
            tool.record();
        }
        assert!(llm.check().is_err());
        assert!(tool.check().is_ok());
    }

    // ── RepetitionDetector ─────────────────────────────────────

    #[test]
    fn repetition_detects_triple() {
        let mut rd = RepetitionDetector::new();
        assert!(!rd.record("read_screen:{\"session_id\":\"a\"}"));
        assert!(!rd.record("read_screen:{\"session_id\":\"a\"}"));
        assert!(rd.record("read_screen:{\"session_id\":\"a\"}"));
    }

    #[test]
    fn repetition_resets_on_different_call() {
        let mut rd = RepetitionDetector::new();
        assert!(!rd.record("read_screen:a"));
        assert!(!rd.record("read_screen:a"));
        assert!(!rd.record("send_input:b")); // different → resets
        assert!(!rd.record("send_input:b"));
    }

    #[test]
    fn repetition_no_false_positive_on_two() {
        let mut rd = RepetitionDetector::new();
        assert!(!rd.record("same"));
        assert!(!rd.record("same"));
        // Only two — not yet a repetition
    }

    // ── AgentState ─────────────────────────────────────────────

    #[test]
    fn agent_state_serializes() {
        let json = serde_json::to_string(&AgentState::Running).unwrap();
        assert_eq!(json, "\"running\"");
        let json = serde_json::to_string(&AgentState::Paused).unwrap();
        assert_eq!(json, "\"paused\"");
    }

    // ── ACTIVE_AGENTS registry ─────────────────────────────────

    #[test]
    fn active_agents_rejects_duplicate() {
        let sid = "test-dup-check";
        let cancel = Arc::new(AtomicBool::new(false));
        ACTIVE_AGENTS.insert(
            sid.to_string(),
            AgentHandle {
                cancel,
                state: Arc::new(RwLock::new(AgentState::Running)),
            },
        );
        assert!(ACTIVE_AGENTS.contains_key(sid));
        // Cleanup
        ACTIVE_AGENTS.remove(sid);
    }

    // ── System prompt ──────────────────────────────────────────

    #[test]
    fn system_prompt_contains_session_id() {
        let prompt = build_system_prompt("my-session");
        assert!(prompt.contains("my-session"));
    }

    // ── Constants sanity ───────────────────────────────────────

    #[test]
    fn constants_are_reasonable() {
        assert!(MAX_ITERATIONS > 0 && MAX_ITERATIONS <= 100);
        assert!(LOOP_TIMEOUT.as_secs() >= 60);
        assert!(MAX_IDENTICAL_CALLS >= 2);
        assert!(RATE_LIMIT_PER_MINUTE > 0);
        assert!(RATE_LIMIT_PER_SESSION > RATE_LIMIT_PER_MINUTE);
    }

    // ── redact_json_values ────────────────────────────────────

    #[test]
    fn redact_json_string_value() {
        let val = json!({"key": "sk-abcdefghijklmnopqrstuvwxyz12345"});
        let redacted = redact_json_values(&val);
        assert!(redacted["key"].as_str().unwrap().contains("[REDACTED]"));
    }

    #[test]
    fn redact_json_nested() {
        let val = json!({"outer": {"inner": "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijk0"}});
        let redacted = redact_json_values(&val);
        let inner = redacted["outer"]["inner"].as_str().unwrap();
        assert!(inner.contains("[REDACTED]"));
    }

    #[test]
    fn redact_json_array() {
        let val = json!(["safe text", "Bearer secret-token-value"]);
        let redacted = redact_json_values(&val);
        assert!(redacted[1].as_str().unwrap().contains("[REDACTED]"));
        assert_eq!(redacted[0].as_str().unwrap(), "safe text");
    }

    #[test]
    fn redact_json_preserves_non_string() {
        let val = json!({"count": 42, "active": true});
        let redacted = redact_json_values(&val);
        assert_eq!(redacted["count"], 42);
        assert_eq!(redacted["active"], true);
    }

    // ── Knowledge injection ───────────────────────────────────

    #[test]
    fn system_prompt_appends_knowledge_when_present() {
        use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass};
        use crate::state::tests_support::make_test_app_state;

        let state = make_test_app_state();
        let sid = "test-knowledge-inject";

        state.record_outcome(
            sid,
            CommandOutcome {
                timestamp: 1,
                command: "cargo build".into(),
                cwd: "/project".into(),
                exit_code: Some(1),
                output_snippet: "error[E0308]: mismatched types".into(),
                classification: OutcomeClass::Error {
                    error_type: "rust_compilation".into(),
                },
                duration_ms: 500,
                id: 0,
            },
        );

        let base = build_system_prompt(sid);
        let knowledge = crate::ai_agent::context::build_knowledge_section(&state, sid);
        let system_prompt = compose_system_prompt(&base, None, knowledge.as_deref());

        assert!(system_prompt.contains(sid));
        assert!(system_prompt.contains("Session Knowledge"));
        assert!(system_prompt.contains("cargo build"));
    }

    #[test]
    fn system_prompt_unchanged_without_knowledge() {
        use crate::state::tests_support::make_test_app_state;

        let state = make_test_app_state();
        let sid = "test-no-knowledge";

        let base = build_system_prompt(sid);
        let knowledge = crate::ai_agent::context::build_knowledge_section(&state, sid);
        let system_prompt = compose_system_prompt(&base, None, knowledge.as_deref());

        assert_eq!(system_prompt, base);
    }

    // #1384-0dca: knowledge mutated mid-loop must be reflected in subsequent
    // iterations' system prompt. Mirrors the change-detection block in
    // `run_loop` (refresh only when the rendered section differs).
    #[test]
    fn system_prompt_refreshes_when_knowledge_mutates_between_iterations() {
        use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass};
        use crate::state::tests_support::make_test_app_state;

        let state = make_test_app_state();
        let sid = "test-knowledge-refresh";
        let base = build_system_prompt(sid);

        // Iteration 0: no knowledge yet → base prompt only.
        let mut last = crate::ai_agent::context::build_knowledge_section(&state, sid);
        let prompt_iter0 = compose_system_prompt(&base, None, last.as_deref());
        assert_eq!(
            prompt_iter0, base,
            "iter 0 should equal base when no knowledge"
        );
        assert!(last.is_none());

        // Tool call mid-loop records a command outcome (Error so it surfaces
        // in `build_context_summary`'s Recent Errors section).
        state.record_outcome(
            sid,
            CommandOutcome {
                timestamp: 1,
                command: "npm run build".into(),
                cwd: "/repo".into(),
                exit_code: Some(1),
                output_snippet: "TypeError: foo".into(),
                classification: OutcomeClass::Error {
                    error_type: "node_runtime".into(),
                },
                duration_ms: 10,
                id: 0,
            },
        );

        // Iteration 1: refresh detects the diff and rebuilds the prompt.
        let current = crate::ai_agent::context::build_knowledge_section(&state, sid);
        assert_ne!(current, last, "knowledge must differ after record_outcome");
        let prompt_iter1 = compose_system_prompt(&base, None, current.as_deref());
        assert_ne!(prompt_iter1, prompt_iter0, "iter 1 prompt must change");
        assert!(prompt_iter1.contains("npm run build"));
        assert!(prompt_iter1.contains("Session Knowledge"));
        last = current;

        // Iteration 2: no further change → no rebuild needed.
        let still_current = crate::ai_agent::context::build_knowledge_section(&state, sid);
        assert_eq!(still_current, last, "stable knowledge must compare equal");
    }

    // ── ToolPhase & model selection ──────────────────────────────

    #[test]
    fn tool_phase_serializes() {
        assert_eq!(
            serde_json::to_string(&ToolPhase::Plan).unwrap(),
            r#""plan""#
        );
        assert_eq!(
            serde_json::to_string(&ToolPhase::Search).unwrap(),
            r#""search""#
        );
        assert_eq!(
            serde_json::to_string(&ToolPhase::Read).unwrap(),
            r#""read""#
        );
        assert_eq!(
            serde_json::to_string(&ToolPhase::Write).unwrap(),
            r#""write""#
        );
    }

    #[test]
    fn classify_phase_no_tools_is_plan() {
        assert_eq!(classify_phase(&[]), ToolPhase::Plan);
    }

    #[test]
    fn classify_phase_read_tools() {
        assert_eq!(classify_phase(&["read_screen"]), ToolPhase::Read);
        assert_eq!(classify_phase(&["read_file"]), ToolPhase::Read);
        assert_eq!(classify_phase(&["get_state"]), ToolPhase::Read);
        assert_eq!(classify_phase(&["get_context"]), ToolPhase::Read);
        assert_eq!(classify_phase(&["list_sessions"]), ToolPhase::Read);
        assert_eq!(classify_phase(&["get_agent_status"]), ToolPhase::Read);
    }

    #[test]
    fn classify_phase_search_tools() {
        assert_eq!(classify_phase(&["search_files"]), ToolPhase::Search);
        assert_eq!(classify_phase(&["search_code"]), ToolPhase::Search);
        assert_eq!(classify_phase(&["list_files"]), ToolPhase::Search);
        assert_eq!(classify_phase(&["search_tools"]), ToolPhase::Search);
    }

    #[test]
    fn classify_phase_write_tools() {
        assert_eq!(classify_phase(&["write_file"]), ToolPhase::Write);
        assert_eq!(classify_phase(&["edit_file"]), ToolPhase::Write);
        assert_eq!(classify_phase(&["send_input"]), ToolPhase::Write);
        assert_eq!(classify_phase(&["run_command"]), ToolPhase::Write);
        assert_eq!(classify_phase(&["spawn_session"]), ToolPhase::Write);
    }

    #[test]
    fn classify_phase_mixed_prefers_write() {
        assert_eq!(
            classify_phase(&["read_file", "edit_file"]),
            ToolPhase::Write
        );
        assert_eq!(
            classify_phase(&["search_files", "send_input"]),
            ToolPhase::Write
        );
    }

    #[test]
    fn classify_phase_mixed_read_search_prefers_search() {
        assert_eq!(
            classify_phase(&["read_screen", "search_files"]),
            ToolPhase::Search
        );
    }

    #[test]
    fn select_model_no_overrides_returns_default() {
        let base = "anthropic/claude-sonnet-4-5";
        let overrides = std::collections::HashMap::new();
        assert_eq!(
            select_model_for_phase(base, &overrides, ToolPhase::Plan),
            base
        );
        assert_eq!(
            select_model_for_phase(base, &overrides, ToolPhase::Write),
            base
        );
    }

    #[test]
    fn select_model_with_override() {
        let base = "anthropic/claude-sonnet-4-5";
        let mut overrides = std::collections::HashMap::new();
        overrides.insert(ToolPhase::Search, "anthropic/claude-haiku-3-5".to_string());
        overrides.insert(ToolPhase::Read, "anthropic/claude-haiku-3-5".to_string());

        assert_eq!(
            select_model_for_phase(base, &overrides, ToolPhase::Search),
            "anthropic/claude-haiku-3-5"
        );
        assert_eq!(
            select_model_for_phase(base, &overrides, ToolPhase::Read),
            "anthropic/claude-haiku-3-5"
        );
        assert_eq!(
            select_model_for_phase(base, &overrides, ToolPhase::Write),
            base
        );
        assert_eq!(
            select_model_for_phase(base, &overrides, ToolPhase::Plan),
            base
        );
    }
}
