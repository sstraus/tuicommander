//! Agent loop engine — ReAct pattern (Reason → Act → Observe → repeat).
//!
//! Assembles context, calls the LLM with tools, dispatches tool calls,
//! feeds results back, and re-enters until the model stops requesting tools
//! or a termination condition is met.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio::sync::{broadcast, oneshot, Notify};

use crate::state::AppState;
use super::tools;

// ── Constants ─────────────────────────────────────────────────

const MAX_ITERATIONS: usize = 20;
const LOOP_TIMEOUT: Duration = Duration::from_secs(300); // 5 min
const MAX_IDENTICAL_CALLS: usize = 3;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT_PER_MINUTE: usize = 30;
const RATE_LIMIT_PER_SESSION: usize = 200;
const TOOL_DISPATCH_LIMIT_PER_MINUTE: usize = 60;
const TOOL_DISPATCH_LIMIT_PER_SESSION: usize = 500;

// ── Active agents registry ────────────────────────────────────

pub(crate) static ACTIVE_AGENTS: std::sync::LazyLock<DashMap<String, AgentHandle>> =
    std::sync::LazyLock::new(DashMap::new);

/// Handle to a running agent loop — used for pause/resume/cancel.
pub(crate) struct AgentHandle {
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<RwLock<AgentState>>,
    pub pause_notify: Arc<Notify>,
    pub event_tx: broadcast::Sender<AgentLoopEvent>,
    /// Oneshot sender for pending approval — set when NeedsApproval fires,
    /// consumed by approve_agent_action.
    pub approval_tx: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
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

// ── Agent loop events ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentLoopEvent {
    Started { session_id: String },
    Thinking { session_id: String, iteration: usize },
    TextChunk { session_id: String, text: String },
    ToolCall { session_id: String, tool_name: String, args: serde_json::Value },
    ToolResult { session_id: String, tool_name: String, success: bool, output: String },
    NeedsApproval { session_id: String, tool_name: String, command: String, reason: String },
    Paused { session_id: String },
    Resumed { session_id: String },
    RateLimited { session_id: String, wait_ms: u64 },
    Error { session_id: String, message: String },
    Completed { session_id: String, reason: String },
}

// ── Rate limiter ──────────────────────────────────────────────

struct RateLimiter {
    window: VecDeque<tokio::time::Instant>,
    total: usize,
    per_minute: usize,
    per_session: usize,
}

impl RateLimiter {
    fn new(per_minute: usize, per_session: usize) -> Self {
        Self {
            window: VecDeque::new(),
            total: 0,
            per_minute,
            per_session,
        }
    }

    /// Check if we can make another call. Returns Ok or the wait duration.
    fn check(&mut self) -> Result<(), Duration> {
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
            let oldest = self.window.front().expect("window non-empty after len check");
            let wait = RATE_WINDOW - now.duration_since(*oldest);
            return Err(wait);
        }

        Ok(())
    }

    fn record(&mut self) {
        self.window.push_back(tokio::time::Instant::now());
        self.total += 1;
    }
}

// ── Repetition detector ───────────────────────────────────────

struct RepetitionDetector {
    recent_calls: VecDeque<String>,
}

impl RepetitionDetector {
    fn new() -> Self {
        Self {
            recent_calls: VecDeque::new(),
        }
    }

    /// Record a tool call signature and return true if it's a repetition.
    fn record(&mut self, signature: &str) -> bool {
        self.recent_calls.push_back(signature.to_string());
        if self.recent_calls.len() > MAX_IDENTICAL_CALLS {
            self.recent_calls.pop_front();
        }

        self.recent_calls.len() == MAX_IDENTICAL_CALLS
            && self.recent_calls.iter().all(|s| s == signature)
    }
}

// ── System prompt ─────────────────────────────────────────────

/// Concatenate the static base prompt with an optional knowledge section
/// (separated by a blank line). Kept as a single function so initial-build
/// and per-iteration refresh produce byte-identical output.
fn compose_system_prompt(base: &str, knowledge: Option<&str>) -> String {
    match knowledge {
        Some(k) => format!("{base}\n\n{k}"),
        None => base.to_string(),
    }
}

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
fn redact_json_values(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => {
            serde_json::Value::String(tools::redact_secrets(s))
        }
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

// ── Core loop ─────────────────────────────────────────────────

/// Start the agent loop for a session. Returns error if already active.
pub(crate) async fn start_agent_loop(
    state: Arc<AppState>,
    session_id: String,
    user_goal: String,
) -> Result<broadcast::Receiver<AgentLoopEvent>, String> {
    // Reject duplicate
    if ACTIVE_AGENTS.contains_key(&session_id) {
        return Err(format!("Agent already active on session {session_id}"));
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let agent_state = Arc::new(RwLock::new(AgentState::Running));
    let pause_notify = Arc::new(Notify::new());
    let (event_tx, event_rx) = broadcast::channel(256);

    let approval_tx: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(None));
    let handle = AgentHandle {
        cancel: cancel.clone(),
        state: agent_state.clone(),
        pause_notify: pause_notify.clone(),
        event_tx: event_tx.clone(),
        approval_tx: approval_tx.clone(),
    };
    ACTIVE_AGENTS.insert(session_id.clone(), handle);

    // Spawn the loop
    let sid = session_id.clone();
    tokio::spawn(async move {
        let result = run_loop(
            state,
            sid.clone(),
            user_goal,
            LoopHandles {
                cancel,
                agent_state: agent_state.clone(),
                pause_notify,
                event_tx: event_tx.clone(),
                approval_tx,
            },
        )
        .await;

        match result {
            Ok(reason) => {
                *agent_state.write() = AgentState::Completed;
                let _ = event_tx.send(AgentLoopEvent::Completed {
                    session_id: sid.clone(),
                    reason,
                });
            }
            Err(e) => {
                tracing::error!(session_id = %sid, error = %e, "Agent loop failed");
                *agent_state.write() = AgentState::Error;
                let _ = event_tx.send(AgentLoopEvent::Error {
                    session_id: sid.clone(),
                    message: e,
                });
            }
        }
        ACTIVE_AGENTS.remove(&sid);
    });

    Ok(event_rx)
}

/// Cancel an active agent loop.
pub(crate) fn cancel_agent_loop(session_id: &str) -> Result<(), String> {
    let entry = ACTIVE_AGENTS.get(session_id)
        .ok_or_else(|| format!("No active agent on session {session_id}"))?;
    entry.cancel.store(true, Ordering::Release);
    *entry.state.write() = AgentState::Cancelled;
    Ok(())
}

/// Pause an active agent loop.
pub(crate) fn pause_agent_loop(session_id: &str) -> Result<(), String> {
    let entry = ACTIVE_AGENTS.get(session_id)
        .ok_or_else(|| format!("No active agent on session {session_id}"))?;
    *entry.state.write() = AgentState::Paused;
    let _ = entry.event_tx.send(AgentLoopEvent::Paused {
        session_id: session_id.to_string(),
    });
    Ok(())
}

/// Resume a paused agent loop.
pub(crate) fn resume_agent_loop(session_id: &str) -> Result<(), String> {
    let entry = ACTIVE_AGENTS.get(session_id)
        .ok_or_else(|| format!("No active agent on session {session_id}"))?;
    *entry.state.write() = AgentState::Running;
    entry.pause_notify.notify_one();
    let _ = entry.event_tx.send(AgentLoopEvent::Resumed {
        session_id: session_id.to_string(),
    });
    Ok(())
}

/// Coordination handles shared between the spawner and the loop.
struct LoopHandles {
    cancel: Arc<AtomicBool>,
    agent_state: Arc<RwLock<AgentState>>,
    pause_notify: Arc<Notify>,
    event_tx: broadcast::Sender<AgentLoopEvent>,
    approval_tx: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
}

/// The actual ReAct loop.
async fn run_loop(
    state: Arc<AppState>,
    session_id: String,
    user_goal: String,
    h: LoopHandles,
) -> Result<String, String> {
    let LoopHandles { cancel, agent_state, pause_notify, event_tx, approval_tx } = h;
    use futures_util::StreamExt;
    use genai::chat::{
        ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent as GenaiStreamEvent,
        ContentPart, Tool, ToolResponse,
    };

    let _ = event_tx.send(AgentLoopEvent::Started {
        session_id: session_id.clone(),
    });

    // Build LLM client
    let config: crate::ai_chat::AiChatConfig = crate::config::load_json_config(crate::ai_chat::CONFIG_FILE);
    let api_key = if config.provider == "ollama" {
        crate::ai_chat::read_api_key()?.unwrap_or_else(|| "ollama".to_string())
    } else {
        crate::ai_chat::read_api_key()?
            .ok_or_else(|| "No API key stored — add one in Settings > AI Chat".to_string())?
    };
    let llm_config = crate::llm_api::LlmApiConfig {
        provider: config.provider.clone(),
        model: config.model.clone(),
        base_url: config.effective_base_url(),
    };
    let client = crate::llm_api::build_client(&llm_config, &api_key);

    // Build tools for genai
    let tool_defs = tools::tool_definitions();
    let genai_tools: Vec<Tool> = tool_defs
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|t| {
            let name = t["name"].as_str()?;
            let mut tool = Tool::new(name);
            if let Some(desc) = t["description"].as_str() {
                tool = tool.with_description(desc);
            }
            if let Some(schema) = t.get("inputSchema") {
                tool = tool.with_schema(schema.clone());
            }
            Some(tool)
        })
        .collect();

    let chat_options = ChatOptions::default().with_capture_tool_calls(true);

    // Base system prompt is constant per session; knowledge is appended and
    // refreshed every iteration so tool calls that mutate session knowledge
    // (mid-loop writes, terminal_mode flips, command outcomes) are reflected
    // in subsequent LLM turns instead of being frozen at iteration 0.
    let base_system_prompt = build_system_prompt(&session_id);
    let mut last_knowledge: Option<String> =
        super::context::build_knowledge_section(&state, &session_id);
    let mut chat_req = ChatRequest::default()
        .with_system(compose_system_prompt(&base_system_prompt, last_knowledge.as_deref()))
        .with_tools(genai_tools.clone())
        .append_message(ChatMessage::user(user_goal));

    let mut rate_limiter = RateLimiter::new(RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION);
    let mut tool_limiter = RateLimiter::new(TOOL_DISPATCH_LIMIT_PER_MINUTE, TOOL_DISPATCH_LIMIT_PER_SESSION);
    let mut repetition = RepetitionDetector::new();
    let deadline = tokio::time::Instant::now() + LOOP_TIMEOUT;

    for iteration in 0..MAX_ITERATIONS {
        if cancel.load(Ordering::Acquire) {
            tracing::info!(session_id, "Agent loop cancelled");
            return Ok("cancelled".into());
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(session_id, "Agent loop timed out");
            return Ok("timeout".into());
        }

        // Refresh knowledge section every iteration: tool calls executed at
        // the end of the previous iteration may have updated SessionKnowledge
        // (cwd, command outcomes, TUI mode). Only re-write the system prompt
        // when content actually changed to avoid pointless allocations.
        if iteration > 0 {
            let current = super::context::build_knowledge_section(&state, &session_id);
            if current != last_knowledge {
                chat_req.system =
                    Some(compose_system_prompt(&base_system_prompt, current.as_deref()));
                last_knowledge = current;
            }
        }

        // Check pause
        while *agent_state.read() == AgentState::Paused {
            // Wait for resume or check cancel every 100ms
            tokio::select! {
                _ = pause_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            if cancel.load(Ordering::Acquire) {
                return Ok("cancelled".into());
            }
        }

        if let Err(wait) = rate_limiter.check() {
            if wait == Duration::ZERO {
                tracing::warn!(session_id, "LLM session rate limit reached");
                return Ok("session_rate_limit".into());
            }
            tracing::debug!(session_id, wait_ms = wait.as_millis() as u64, "LLM rate limit, waiting");
            let _ = event_tx.send(AgentLoopEvent::RateLimited {
                session_id: session_id.clone(),
                wait_ms: wait.as_millis() as u64,
            });
            tokio::time::sleep(wait).await;
            if cancel.load(Ordering::Acquire) {
                return Ok("cancelled".into());
            }
        }
        rate_limiter.record();

        let _ = event_tx.send(AgentLoopEvent::Thinking {
            session_id: session_id.clone(),
            iteration,
        });

        // Stream the LLM turn
        let stream_resp = client
            .exec_chat_stream(&config.model, chat_req.clone(), Some(&chat_options))
            .await
            .map_err(|e| format!("LLM stream error: {e}"))?;

        let mut stream = stream_resp.stream;
        let mut tool_calls = Vec::new();
        let mut text_buf = String::new();
        // Collect ContentParts for assistant message reconstruction
        let mut assistant_parts: Vec<ContentPart> = Vec::new();

        loop {
            tokio::select! {
                event = stream.next() => {
                    match event {
                        Some(Ok(GenaiStreamEvent::Chunk(chunk))) => {
                            text_buf.push_str(&chunk.content);
                            let _ = event_tx.send(AgentLoopEvent::TextChunk {
                                session_id: session_id.clone(),
                                text: chunk.content,
                            });
                        }
                        Some(Ok(GenaiStreamEvent::End(end))) => {
                            // Extract tool calls from captured content
                            if let Some(content) = end.captured_content {
                                for part in content.into_parts() {
                                    match part {
                                        ContentPart::ToolCall(tc) => {
                                            tool_calls.push(tc);
                                        }
                                        other => {
                                            assistant_parts.push(other);
                                        }
                                    }
                                }
                            }
                            break;
                        }
                        Some(Err(e)) => {
                            tracing::error!(session_id, iteration, error = %e, "LLM stream error");
                            return Err(format!("Stream error at iteration {iteration}: {e}"));
                        }
                        None => break,
                        _ => {} // Start, ReasoningChunk, ToolCallChunk, etc.
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if cancel.load(Ordering::Acquire) {
                        return Ok("cancelled".into());
                    }
                }
            }
        }

        if tool_calls.is_empty() {
            tracing::info!(session_id, iteration, "Agent completed (end_turn)");
            return Ok("end_turn".into());
        }

        // Append assistant message with tool calls to conversation
        chat_req = chat_req.append_message(tool_calls.clone());

        // Execute tool calls
        for tc in &tool_calls {
            if let Err(wait) = tool_limiter.check() {
                if wait == Duration::ZERO {
                    tracing::warn!(session_id, "Tool dispatch session limit reached");
                    return Ok("tool_dispatch_session_limit".into());
                }
                tracing::debug!(session_id, wait_ms = wait.as_millis() as u64, "Tool dispatch rate limit, waiting");
                let _ = event_tx.send(AgentLoopEvent::RateLimited {
                    session_id: session_id.clone(),
                    wait_ms: wait.as_millis() as u64,
                });
                tokio::time::sleep(wait).await;
                if cancel.load(Ordering::Acquire) {
                    return Ok("cancelled".into());
                }
            }
            tool_limiter.record();

            // Repetition detection
            let sig = format!("{}:{}", tc.fn_name, tc.fn_arguments);
            if repetition.record(&sig) {
                tracing::warn!(session_id, tool = %tc.fn_name, "Repetition detected, stopping");
                return Ok(format!("repetition_detected: {}", tc.fn_name));
            }

            let redacted_args = redact_json_values(&tc.fn_arguments);
            let _ = event_tx.send(AgentLoopEvent::ToolCall {
                session_id: session_id.clone(),
                tool_name: tc.fn_name.clone(),
                args: redacted_args,
            });

            tracing::debug!(session_id, tool = %tc.fn_name, "Dispatching tool");
            let mut result = tools::dispatch(&state, &session_id, &tc.fn_name, &tc.fn_arguments).await;

            // Approval flow: pause for user confirmation, then re-dispatch
            if result.needs_approval {
                let reason = result.approval_reason.clone().unwrap_or_default();
                let command = result.approval_command.clone().unwrap_or_default();

                let _ = event_tx.send(AgentLoopEvent::NeedsApproval {
                    session_id: session_id.clone(),
                    tool_name: tc.fn_name.clone(),
                    command: command.clone(),
                    reason: reason.clone(),
                });

                let (tx, rx) = oneshot::channel();
                *approval_tx.lock() = Some(tx);

                // Wait for approval or cancellation
                let approved = tokio::select! {
                    res = rx => res.unwrap_or(false),
                    _ = async {
                        while !cancel.load(Ordering::Acquire) {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    } => false,
                };

                *approval_tx.lock() = None;

                if cancel.load(Ordering::Acquire) {
                    return Ok("cancelled".into());
                }

                if approved {
                    result = tools::dispatch_approved(&state, &session_id, &tc.fn_name, &tc.fn_arguments).await;
                } else {
                    result = tools::ToolResult::err(format!("User rejected: {reason}"));
                }
            }

            let _ = event_tx.send(AgentLoopEvent::ToolResult {
                session_id: session_id.clone(),
                tool_name: tc.fn_name.clone(),
                success: result.success,
                output: result.output.clone(),
            });

            // Append tool response
            let tool_resp = ToolResponse::new(
                tc.call_id.clone(),
                result.output,
            );
            chat_req = chat_req.append_message(tool_resp);
        }
    }

    tracing::warn!(session_id, "Agent hit max iterations");
    Ok("max_iterations".into())
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
        let mut tool = RateLimiter::new(TOOL_DISPATCH_LIMIT_PER_MINUTE, TOOL_DISPATCH_LIMIT_PER_SESSION);
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

    // ── AgentLoopEvent ─────────────────────────────────────────

    #[test]
    fn event_started_serializes() {
        let evt = AgentLoopEvent::Started { session_id: "s1".into() };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"started\""));
        assert!(json.contains("\"session_id\":\"s1\""));
    }

    #[test]
    fn event_tool_call_serializes() {
        let evt = AgentLoopEvent::ToolCall {
            session_id: "s1".into(),
            tool_name: "read_screen".into(),
            args: json!({"session_id": "s1"}),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"tool_name\":\"read_screen\""));
    }

    #[test]
    fn event_completed_serializes() {
        let evt = AgentLoopEvent::Completed {
            session_id: "s1".into(),
            reason: "end_turn".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("\"reason\":\"end_turn\""));
    }

    #[test]
    fn event_error_serializes() {
        let evt = AgentLoopEvent::Error {
            session_id: "s1".into(),
            message: "LLM timeout".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"error\""));
    }

    // ── ACTIVE_AGENTS registry ─────────────────────────────────

    #[test]
    fn active_agents_rejects_duplicate() {
        let sid = "test-dup-check";
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, _) = broadcast::channel(16);
        ACTIVE_AGENTS.insert(sid.to_string(), AgentHandle {

            cancel,
            state: Arc::new(RwLock::new(AgentState::Running)),
            pause_notify: Arc::new(Notify::new()),
            event_tx: tx,
            approval_tx: Arc::new(Mutex::new(None)),
        });
        assert!(ACTIVE_AGENTS.contains_key(sid));
        // Cleanup
        ACTIVE_AGENTS.remove(sid);
    }

    #[test]
    fn cancel_missing_session_errors() {
        assert!(cancel_agent_loop("nonexistent").is_err());
    }

    #[test]
    fn pause_missing_session_errors() {
        assert!(pause_agent_loop("nonexistent").is_err());
    }

    #[test]
    fn resume_missing_session_errors() {
        assert!(resume_agent_loop("nonexistent").is_err());
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

    // ── Approval channel ──────────────────────────────────────

    #[test]
    fn approval_tx_starts_none() {
        let atx: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(None));
        assert!(atx.lock().is_none());
    }

    #[tokio::test]
    async fn approval_channel_approve_path() {
        let (tx, rx) = oneshot::channel::<bool>();
        let atx: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(Some(tx)));
        // Simulate approve_agent_action
        let sender = atx.lock().take().unwrap();
        sender.send(true).unwrap();
        assert!(rx.await.unwrap());
    }

    #[tokio::test]
    async fn approval_channel_reject_path() {
        let (tx, rx) = oneshot::channel::<bool>();
        let atx: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(Some(tx)));
        let sender = atx.lock().take().unwrap();
        sender.send(false).unwrap();
        assert!(!rx.await.unwrap());
    }

    #[tokio::test]
    async fn approval_channel_dropped_sender_returns_false() {
        let (tx, rx) = oneshot::channel::<bool>();
        drop(tx);
        assert!(rx.await.is_err());
    }

    #[test]
    fn approval_handle_take_clears() {
        let (tx, _rx) = oneshot::channel::<bool>();
        let atx: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(Some(tx)));
        let taken = atx.lock().take();
        assert!(taken.is_some());
        assert!(atx.lock().is_none());
    }

    #[test]
    fn needs_approval_event_serializes() {
        let evt = AgentLoopEvent::NeedsApproval {
            session_id: "s1".into(),
            tool_name: "send_input".into(),
            command: "rm -rf /tmp".into(),
            reason: "destructive command".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"needs_approval\""));
        assert!(json.contains("\"tool_name\":\"send_input\""));
        assert!(json.contains("\"command\":\"rm -rf /tmp\""));
        assert!(json.contains("\"reason\":\"destructive command\""));
    }

    #[test]
    fn approve_action_no_pending_errors() {
        let sid = "test-no-pending";
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, _) = broadcast::channel(16);
        let atx = Arc::new(Mutex::new(None));
        ACTIVE_AGENTS.insert(sid.to_string(), AgentHandle {

            cancel,
            state: Arc::new(RwLock::new(AgentState::Running)),
            pause_notify: Arc::new(Notify::new()),
            event_tx: tx,
            approval_tx: atx.clone(),
        });
        // No pending approval — take returns None
        assert!(atx.lock().take().is_none());
        ACTIVE_AGENTS.remove(sid);
    }

    #[tokio::test]
    async fn approve_action_resolves_channel() {
        let sid = "test-approve-resolve";
        let cancel = Arc::new(AtomicBool::new(false));
        let (etx, _) = broadcast::channel(16);
        let (otx, orx) = oneshot::channel::<bool>();
        let atx = Arc::new(Mutex::new(Some(otx)));
        ACTIVE_AGENTS.insert(sid.to_string(), AgentHandle {

            cancel,
            state: Arc::new(RwLock::new(AgentState::Running)),
            pause_notify: Arc::new(Notify::new()),
            event_tx: etx,
            approval_tx: atx.clone(),
        });
        // Simulate approve_agent_action
        let sender = atx.lock().take().unwrap();
        sender.send(true).unwrap();
        assert!(orx.await.unwrap());
        ACTIVE_AGENTS.remove(sid);
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
                semantic_intent: None,
            },
        );

        let base = build_system_prompt(sid);
        let knowledge = crate::ai_agent::context::build_knowledge_section(&state, sid);
        let system_prompt = compose_system_prompt(&base, knowledge.as_deref());

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
        let system_prompt = compose_system_prompt(&base, knowledge.as_deref());

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
        let prompt_iter0 = compose_system_prompt(&base, last.as_deref());
        assert_eq!(prompt_iter0, base, "iter 0 should equal base when no knowledge");
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
                semantic_intent: None,
            },
        );

        // Iteration 1: refresh detects the diff and rebuilds the prompt.
        let current = crate::ai_agent::context::build_knowledge_section(&state, sid);
        assert_ne!(current, last, "knowledge must differ after record_outcome");
        let prompt_iter1 = compose_system_prompt(&base, current.as_deref());
        assert_ne!(prompt_iter1, prompt_iter0, "iter 1 prompt must change");
        assert!(prompt_iter1.contains("npm run build"));
        assert!(prompt_iter1.contains("Session Knowledge"));
        last = current;

        // Iteration 2: no further change → no rebuild needed.
        let still_current = crate::ai_agent::context::build_knowledge_section(&state, sid);
        assert_eq!(still_current, last, "stable knowledge must compare equal");
    }
}
