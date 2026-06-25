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

// ── LLM retry/backoff ─────────────────────────────────────────

/// Max retry attempts for a single transient LLM call (excludes the first try).
pub(crate) const MAX_LLM_RETRIES: u32 = 4;
const BACKOFF_BASE_MS: u64 = 500;
const BACKOFF_CAP: Duration = Duration::from_secs(16);

// ── History compaction ────────────────────────────────────────

/// Number of most-recent tool-result messages kept verbatim during compaction.
pub(crate) const COMPACT_KEEP_RECENT_TOOL_RESULTS: usize = 6;
/// Tool-result bodies below this byte size are never elided (not worth it).
const MIN_ELIDE_BYTES: usize = 512;
/// Default prompt-token budget that triggers compaction. Conservative; small
/// local models (<32k context) should lower it via config.
/// DEFERRED (2026-06-25) — derive from per-model context window instead of a
/// fixed default once the provider registry exposes context sizes.
pub(crate) const DEFAULT_COMPACT_THRESHOLD_TOKENS: usize = 100_000;

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
            | "call_tool" | "spawn_session" | "watch_for" => has_write = true,
            "search_files" | "search_code" | "list_files" | "search_tools" => has_search = true,
            "read_screen"
            | "read_file"
            | "get_state"
            | "get_context"
            | "list_sessions"
            | "get_agent_status"
            | "get_command_history"
            | "explain_last_failure"
            | "get_error_fixes"
            | "list_watches"
            | "cancel_watch"
            | "search_scrollback"
            | "get_hyperlinks"
            | "get_semantic_zones" => has_read = true,
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

// ---------------------------------------------------------------------------
// LLM retry classification + backoff
// ---------------------------------------------------------------------------

/// Classify a genai error as transient (worth retrying): HTTP 429 / 5xx, or a
/// network/transport failure. Non-429 4xx and request-shape errors are fatal.
pub(crate) fn is_transient_llm_error(e: &genai::Error) -> bool {
    use genai::Error as E;
    match e {
        E::HttpError { status, .. } => status.as_u16() == 429 || status.is_server_error(),
        E::WebModelCall { webc_error, .. } | E::WebAdapterCall { webc_error, .. } => {
            is_transient_webc(webc_error)
        }
        E::WebStream { .. } => true, // mid-stream network drop
        _ => false,
    }
}

fn is_transient_webc(e: &genai::webc::Error) -> bool {
    use genai::webc::Error as W;
    match e {
        W::ResponseFailedStatus { status, .. } => {
            status.as_u16() == 429 || status.is_server_error()
        }
        W::Reqwest(_) => true, // timeout / connect / transport
        _ => false,
    }
}

/// Exponential backoff (base 500ms, ×2 per attempt) capped at `BACKOFF_CAP`,
/// plus 0..250ms jitter. `attempt` is 1-based.
pub(crate) fn retry_backoff(attempt: u32) -> Duration {
    let exp = BACKOFF_BASE_MS.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1)));
    let base = Duration::from_millis(exp).min(BACKOFF_CAP);
    // Cheap jitter (no rand dep): 0..250ms from wall-clock nanos.
    let jitter = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        % 250) as u64;
    base + Duration::from_millis(jitter)
}

// ---------------------------------------------------------------------------
// Deterministic history compaction
// ---------------------------------------------------------------------------

pub(crate) struct CompactionStats {
    pub elided: usize,
}

/// Rough token estimate from in-memory byte size (≈4 bytes/token). Fallback for
/// providers that don't return usage.
pub(crate) fn estimate_tokens(msgs: &[genai::chat::ChatMessage]) -> usize {
    msgs.iter().map(|m| m.size()).sum::<usize>() / 4
}

/// Elide the bodies of tool-result messages older than the last `keep_recent`
/// tool results, replacing each with a short stub. Preserves message count and
/// `call_id` so tool_use/tool_result pairing stays valid (Anthropic requires
/// every tool_use to have a matching tool_result).
pub(crate) fn compact_history(
    msgs: &mut [genai::chat::ChatMessage],
    keep_recent: usize,
) -> CompactionStats {
    use genai::chat::{ChatRole, ContentPart, MessageContent};

    let tool_idxs: Vec<usize> = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == ChatRole::Tool)
        .map(|(i, _)| i)
        .collect();
    let cutoff = tool_idxs.len().saturating_sub(keep_recent);
    let mut elided = 0;

    for &i in &tool_idxs[..cutoff] {
        let parts = std::mem::take(&mut msgs[i].content).into_parts();
        let new_parts: Vec<ContentPart> = parts
            .into_iter()
            .map(|p| match p {
                ContentPart::ToolResponse(mut tr) if tr.content.len() >= MIN_ELIDE_BYTES => {
                    let n = tr.content.len();
                    tr.content =
                        format!("[elided {n} bytes of earlier tool output to fit context]");
                    elided += 1;
                    ContentPart::ToolResponse(tr)
                }
                other => other,
            })
            .collect();
        msgs[i].content = MessageContent::from_parts(new_parts);
    }

    CompactionStats { elided }
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

    // ── Transient error classification ─────────────────────────

    fn http_err(code: u16) -> genai::Error {
        genai::Error::HttpError {
            status: reqwest::StatusCode::from_u16(code).unwrap(),
            canonical_reason: "test".into(),
            body: "{}".into(),
        }
    }

    #[test]
    fn transient_http_429_and_5xx() {
        assert!(is_transient_llm_error(&http_err(429)));
        assert!(is_transient_llm_error(&http_err(500)));
        assert!(is_transient_llm_error(&http_err(503)));
    }

    #[test]
    fn transient_http_4xx_non_429_is_fatal() {
        assert!(!is_transient_llm_error(&http_err(400)));
        assert!(!is_transient_llm_error(&http_err(401)));
        assert!(!is_transient_llm_error(&http_err(404)));
    }

    #[test]
    fn transient_webc_status() {
        let mk = |code: u16| genai::webc::Error::ResponseFailedStatus {
            status: reqwest::StatusCode::from_u16(code).unwrap(),
            body: "x".into(),
            headers: Box::new(reqwest::header::HeaderMap::new()),
        };
        assert!(is_transient_webc(&mk(429)));
        assert!(is_transient_webc(&mk(502)));
        assert!(!is_transient_webc(&mk(401)));
    }

    // ── Backoff ────────────────────────────────────────────────

    #[test]
    fn backoff_is_monotonic_and_capped() {
        // Base component (minus jitter) must grow then cap. Compare floors.
        let floor = |a: u32| {
            Duration::from_millis(
                BACKOFF_BASE_MS.saturating_mul(2u64.saturating_pow(a.saturating_sub(1))),
            )
            .min(BACKOFF_CAP)
        };
        assert!(floor(1) < floor(2));
        assert!(floor(2) < floor(3));
        assert_eq!(floor(10), BACKOFF_CAP);
        // Actual value never exceeds cap + max jitter.
        assert!(retry_backoff(10) <= BACKOFF_CAP + Duration::from_millis(250));
        assert!(retry_backoff(1) >= Duration::from_millis(BACKOFF_BASE_MS));
    }

    // ── Compaction ─────────────────────────────────────────────

    fn tool_msg(call_id: &str, body_len: usize) -> genai::chat::ChatMessage {
        use genai::chat::{MessageContent, ToolResponse};
        let body = "x".repeat(body_len);
        genai::chat::ChatMessage::tool(MessageContent::from_tool_responses(vec![
            ToolResponse::new(call_id, body),
        ]))
    }

    fn tool_body(msg: &genai::chat::ChatMessage) -> String {
        msg.content.tool_responses()[0].content.clone()
    }

    #[test]
    fn compact_elides_old_tool_bodies_preserving_pairing() {
        let mut msgs = vec![
            genai::chat::ChatMessage::user("fix the build"),
            tool_msg("call_0", 4096),
            tool_msg("call_1", 4096),
            tool_msg("call_2", 4096),
            tool_msg("call_3", 4096),
            tool_msg("call_4", 4096),
            tool_msg("call_5", 4096),
            tool_msg("call_6", 4096),
            tool_msg("call_7", 4096),
        ];
        let len_before = msgs.len();
        let stats = compact_history(&mut msgs, 6);

        // 8 tool results, keep 6 → 2 oldest elided.
        assert_eq!(stats.elided, 2);
        // Message count unchanged (pairing preserved).
        assert_eq!(msgs.len(), len_before);
        // Oldest two elided, call_id preserved.
        assert!(tool_body(&msgs[1]).contains("elided"));
        assert_eq!(msgs[1].content.tool_responses()[0].call_id, "call_0");
        assert!(tool_body(&msgs[2]).contains("elided"));
        // Recent six intact.
        assert_eq!(tool_body(&msgs[3]).len(), 4096);
        assert_eq!(tool_body(&msgs[8]).len(), 4096);
        // User message untouched.
        assert_eq!(msgs[0].role, genai::chat::ChatRole::User);
    }

    #[test]
    fn compact_skips_small_bodies() {
        let mut msgs = vec![tool_msg("a", 10), tool_msg("b", 10), tool_msg("c", 10)];
        // Even though all are "old" (keep_recent 0), small bodies aren't elided.
        let stats = compact_history(&mut msgs, 0);
        assert_eq!(stats.elided, 0);
        assert_eq!(tool_body(&msgs[0]).len(), 10);
    }

    #[test]
    fn compact_noop_when_under_keep_recent() {
        let mut msgs = vec![tool_msg("a", 4096), tool_msg("b", 4096)];
        let stats = compact_history(&mut msgs, 6);
        assert_eq!(stats.elided, 0);
        assert_eq!(tool_body(&msgs[0]).len(), 4096);
    }

    #[test]
    fn estimate_tokens_scales_with_size() {
        let small = vec![tool_msg("a", 40)];
        let big = vec![tool_msg("a", 4000)];
        assert!(estimate_tokens(&big) > estimate_tokens(&small));
    }
}
