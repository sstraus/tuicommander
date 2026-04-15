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
use tokio::sync::{broadcast, Notify};

use crate::state::AppState;
use super::tools;

// ── Constants ─────────────────────────────────────────────────

const MAX_ITERATIONS: usize = 20;
const LOOP_TIMEOUT: Duration = Duration::from_secs(300); // 5 min
const MAX_IDENTICAL_CALLS: usize = 3;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT_PER_MINUTE: usize = 30;
const RATE_LIMIT_PER_SESSION: usize = 200;

// ── Active agents registry ────────────────────────────────────

lazy_static::lazy_static! {
    /// Global map of session_id → active agent handle.
    /// Prevents duplicate loops on the same session.
    pub(crate) static ref ACTIVE_AGENTS: DashMap<String, AgentHandle> = DashMap::new();
}

/// Handle to a running agent loop — used for pause/resume/cancel.
pub(crate) struct AgentHandle {
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<RwLock<AgentState>>,
    pub pause_notify: Arc<Notify>,
    pub event_tx: broadcast::Sender<AgentLoopEvent>,
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
    Paused { session_id: String },
    Resumed { session_id: String },
    RateLimited { session_id: String, wait_ms: u64 },
    Error { session_id: String, message: String },
    Completed { session_id: String, iterations: usize, reason: String },
}

// ── Rate limiter ──────────────────────────────────────────────

struct RateLimiter {
    window: VecDeque<tokio::time::Instant>,
    total: usize,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            window: VecDeque::new(),
            total: 0,
        }
    }

    /// Check if we can make another call. Returns Ok or the wait duration.
    fn check(&mut self) -> Result<(), Duration> {
        let now = tokio::time::Instant::now();

        // Session limit
        if self.total >= RATE_LIMIT_PER_SESSION {
            return Err(Duration::ZERO); // permanent for this session
        }

        // Sliding window
        while let Some(&front) = self.window.front() {
            if now.duration_since(front) > RATE_WINDOW {
                self.window.pop_front();
            } else {
                break;
            }
        }

        if self.window.len() >= RATE_LIMIT_PER_MINUTE {
            let oldest = self.window.front().unwrap();
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

// ── Input queue ───────────────────────────────────────────────

/// Buffers user input while the agent is controlling the session.
/// Flushed between tool calls.
pub(crate) struct InputQueue {
    queue: Mutex<Vec<String>>,
}

impl InputQueue {
    pub(crate) fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn push(&self, input: String) {
        self.queue.lock().push(input);
    }

    fn drain(&self) -> Vec<String> {
        let mut q = self.queue.lock();
        std::mem::take(&mut *q)
    }

    fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

// ── System prompt ─────────────────────────────────────────────

fn build_system_prompt(session_id: &str) -> String {
    format!(
        "You are an AI agent controlling a terminal session (id: {session_id}). \
         You can observe the terminal via read_screen and get_context, \
         send commands via send_input, send special keys via send_key, \
         and wait for output via wait_for. \
         \n\nAlways observe before acting. Prefer targeted, minimal commands. \
         When a task is complete, stop calling tools and summarize what you did."
    )
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

    let handle = AgentHandle {
        cancel: cancel.clone(),
        state: agent_state.clone(),
        pause_notify: pause_notify.clone(),
        event_tx: event_tx.clone(),
    };
    ACTIVE_AGENTS.insert(session_id.clone(), handle);

    // Spawn the loop
    let sid = session_id.clone();
    tokio::spawn(async move {
        let result = run_loop(
            state,
            sid.clone(),
            user_goal,
            cancel,
            agent_state.clone(),
            pause_notify,
            event_tx.clone(),
        )
        .await;

        match result {
            Ok(reason) => {
                *agent_state.write() = AgentState::Completed;
                let _ = event_tx.send(AgentLoopEvent::Completed {
                    session_id: sid.clone(),
                    iterations: 0, // logged inside run_loop
                    reason,
                });
            }
            Err(e) => {
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
    entry.cancel.store(true, Ordering::Relaxed);
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

/// The actual ReAct loop.
async fn run_loop(
    state: Arc<AppState>,
    session_id: String,
    user_goal: String,
    cancel: Arc<AtomicBool>,
    agent_state: Arc<RwLock<AgentState>>,
    pause_notify: Arc<Notify>,
    event_tx: broadcast::Sender<AgentLoopEvent>,
) -> Result<String, String> {
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
        .unwrap()
        .iter()
        .map(|t| {
            let mut tool = Tool::new(t["name"].as_str().unwrap());
            if let Some(desc) = t["description"].as_str() {
                tool = tool.with_description(desc);
            }
            if let Some(schema) = t.get("inputSchema") {
                tool = tool.with_schema(schema.clone());
            }
            tool
        })
        .collect();

    let chat_options = ChatOptions::default().with_capture_tool_calls(true);

    // Build initial request
    let mut chat_req = ChatRequest::default()
        .with_system(build_system_prompt(&session_id))
        .with_tools(genai_tools.clone())
        .append_message(ChatMessage::user(user_goal));

    let mut rate_limiter = RateLimiter::new();
    let mut repetition = RepetitionDetector::new();
    let deadline = tokio::time::Instant::now() + LOOP_TIMEOUT;

    for iteration in 0..MAX_ITERATIONS {
        // Check cancellation
        if cancel.load(Ordering::Relaxed) {
            return Ok("cancelled".into());
        }

        // Check timeout
        if tokio::time::Instant::now() >= deadline {
            return Ok("timeout".into());
        }

        // Check pause
        while *agent_state.read() == AgentState::Paused {
            // Wait for resume or check cancel every 100ms
            tokio::select! {
                _ = pause_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok("cancelled".into());
            }
        }

        // Rate limiting
        if let Err(wait) = rate_limiter.check() {
            if wait == Duration::ZERO {
                return Ok("session_rate_limit".into());
            }
            let _ = event_tx.send(AgentLoopEvent::RateLimited {
                session_id: session_id.clone(),
                wait_ms: wait.as_millis() as u64,
            });
            tokio::time::sleep(wait).await;
            if cancel.load(Ordering::Relaxed) {
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
                            return Err(format!("Stream error at iteration {iteration}: {e}"));
                        }
                        None => break,
                        _ => {} // Start, ReasoningChunk, ToolCallChunk, etc.
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok("cancelled".into());
                    }
                }
            }
        }

        // No tool calls → model is done
        if tool_calls.is_empty() {
            return Ok("end_turn".into());
        }

        // Append assistant message with tool calls to conversation
        chat_req = chat_req.append_message(tool_calls.clone());

        // Execute tool calls
        for tc in &tool_calls {
            // Repetition detection
            let sig = format!("{}:{}", tc.fn_name, tc.fn_arguments);
            if repetition.record(&sig) {
                return Ok(format!("repetition_detected: {}", tc.fn_name));
            }

            let _ = event_tx.send(AgentLoopEvent::ToolCall {
                session_id: session_id.clone(),
                tool_name: tc.fn_name.clone(),
                args: tc.fn_arguments.clone(),
            });

            let result = tools::dispatch(&state, &tc.fn_name, &tc.fn_arguments).await;

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

    Ok("max_iterations".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── RateLimiter ────────────────────────────────────────────

    #[test]
    fn rate_limiter_allows_initial_call() {
        let mut rl = RateLimiter::new();
        assert!(rl.check().is_ok());
    }

    #[test]
    fn rate_limiter_blocks_after_burst() {
        let mut rl = RateLimiter::new();
        for _ in 0..RATE_LIMIT_PER_MINUTE {
            rl.check().unwrap();
            rl.record();
        }
        assert!(rl.check().is_err());
    }

    #[test]
    fn rate_limiter_session_limit() {
        let mut rl = RateLimiter::new();
        rl.total = RATE_LIMIT_PER_SESSION;
        let err = rl.check().unwrap_err();
        assert_eq!(err, Duration::ZERO);
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

    // ── InputQueue ─────────────────────────────────────────────

    #[test]
    fn input_queue_push_and_drain() {
        let q = InputQueue::new();
        assert!(q.is_empty());
        q.push("hello".into());
        q.push("world".into());
        assert!(!q.is_empty());
        let items = q.drain();
        assert_eq!(items, vec!["hello", "world"]);
        assert!(q.is_empty());
    }

    #[test]
    fn input_queue_drain_is_empty_after() {
        let q = InputQueue::new();
        q.push("x".into());
        q.drain();
        assert_eq!(q.drain().len(), 0);
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
            iterations: 5,
            reason: "end_turn".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("\"iterations\":5"));
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
}
