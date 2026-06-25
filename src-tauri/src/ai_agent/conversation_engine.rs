//! Unified conversation engine — single entry point for both chat and agent modes.
//!
//! Replaces the dual-engine architecture (stream_ai_chat + run_loop) with a single
//! `start_conversation()` that always includes tools and routes via `Autonomy`.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio::sync::{Notify, broadcast, oneshot};

use super::engine::{
    self, AgentState, LOOP_TIMEOUT, MAX_ITERATIONS, RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION,
    RateLimiter, RepetitionDetector, TOOL_DISPATCH_LIMIT_PER_MINUTE,
    TOOL_DISPATCH_LIMIT_PER_SESSION, classify_phase, compose_system_prompt, redact_json_values,
    select_model_for_phase,
};
use super::tools;
use crate::state::AppState;

// ── Autonomy ──────────────────────────────────────────────────

/// Controls tool approval and step limits for a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Autonomy {
    /// Approval required for destructive tools; bypass set skips per-tool prompts.
    #[default]
    Assisted,
    /// No approval gates; runs up to max_steps iterations.
    Autonomous,
}

// ── ConversationConfig ────────────────────────────────────────

pub(crate) struct ConversationConfig {
    pub autonomy: Autonomy,
    /// None = stop after first text response (chat-like). Some(n) = up to n iterations.
    pub max_steps: Option<usize>,
    pub temperature: f32,
    /// Override the main model from the provider registry.
    pub model_override: Option<String>,
    /// Tool names pre-approved for this session — bypass approval prompt.
    pub bypassed_tools: HashSet<String>,
    /// Extended-thinking effort (Opus 4.7+); gated by model capability.
    pub reasoning: ReasoningLevel,
    /// Prompt-token budget that triggers history compaction. None disables it.
    pub compact_after_tokens: Option<usize>,
}

/// User-facing reasoning effort. `Auto` enables a sensible default on capable
/// models; all levels are no-ops on models without extended thinking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReasoningLevel {
    #[default]
    Auto,
    Off,
    Low,
    Medium,
    High,
}

impl ReasoningLevel {
    pub(crate) fn from_opt(s: Option<&str>) -> Self {
        match s {
            Some("off") => Self::Off,
            Some("low") => Self::Low,
            Some("medium") => Self::Medium,
            Some("high") => Self::High,
            _ => Self::Auto,
        }
    }
}

/// Opus 4.7+ gained adaptive extended thinking (genai 0.6.3).
/// DEFERRED (2026-06-06) — extend match as new thinking-capable families ship.
fn supports_extended_thinking(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    ["opus-4-7", "opus-4-8", "opus-4-9"]
        .iter()
        .any(|t| m.contains(t))
}

/// Resolve the user's reasoning level + the active model into a genai effort.
/// Returns `None` when the model can't think or the user turned it off.
fn resolve_reasoning(level: ReasoningLevel, model: &str) -> Option<genai::chat::ReasoningEffort> {
    use genai::chat::ReasoningEffort;
    if !supports_extended_thinking(model) {
        return None;
    }
    match level {
        ReasoningLevel::Off => None,
        ReasoningLevel::Auto | ReasoningLevel::Medium => Some(ReasoningEffort::Medium),
        ReasoningLevel::Low => Some(ReasoningEffort::Low),
        ReasoningLevel::High => Some(ReasoningEffort::High),
    }
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            autonomy: Autonomy::Assisted,
            max_steps: None,
            temperature: 0.7,
            model_override: None,
            bypassed_tools: HashSet::new(),
            reasoning: ReasoningLevel::Auto,
            compact_after_tokens: Some(engine::DEFAULT_COMPACT_THRESHOLD_TOKENS),
        }
    }
}

// ── ConversationEvent ─────────────────────────────────────────

/// Unified event stream for conversation sessions.
/// DEFERRED (2026-05-07) — expand with richer event variants (progress, file-change) per terminal-watcher plan.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ConversationEvent {
    Thinking {
        iteration: usize,
    },
    TextChunk {
        text: String,
    },
    /// Streamed reasoning/thinking content (Opus 4.7+ extended thinking).
    ReasoningChunk {
        text: String,
    },
    ToolCall {
        tool_name: String,
        args: serde_json::Value,
    },
    ToolResult {
        tool_name: String,
        success: bool,
        output: String,
        duration_ms: u64,
    },
    NeedsApproval {
        tool_name: String,
        command: String,
        reason: String,
    },
    /// Tool was executed without prompting because it's in the bypass set.
    Bypassed {
        tool_name: String,
    },
    Paused,
    Resumed,
    RateLimited {
        wait_ms: u64,
    },
    /// A transient LLM error is being retried after `wait_ms` backoff.
    Retrying {
        attempt: u32,
        wait_ms: u64,
        reason: String,
    },
    /// History was compacted: `elided` old tool-result bodies were truncated
    /// because the request reached `before_tokens` prompt tokens.
    Compacted {
        elided: usize,
        before_tokens: usize,
    },
    Error {
        message: String,
    },
    Completed {
        reason: String,
        usage: Option<ConversationUsage>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ── Active conversations registry ─────────────────────────────

pub(crate) static ACTIVE_CONVERSATIONS: std::sync::LazyLock<DashMap<String, ConversationHandle>> =
    std::sync::LazyLock::new(DashMap::new);

pub(crate) struct ConversationHandle {
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<RwLock<AgentState>>,
    pub pause_notify: Arc<Notify>,
    pub event_tx: broadcast::Sender<ConversationEvent>,
    pub approval_tx: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
}

// ── System prompt ─────────────────────────────────────────────

/// Merged system prompt combining the agent tool documentation and the terminal
/// assistant code-block rules from the chat path.
fn build_base_system_prompt(session_id: &str) -> String {
    format!(
        "You are a terminal assistant and AI agent embedded in TUICommander (session: {session_id}).\n\
         You can see the user's terminal output and use tools to act on it.\n\n\
         ## Code block rules (for inline responses)\n\
         - Every fenced code block gets a ▶ Run button — put EXACTLY ONE command per block.\n\
         - Never combine multiple commands or alternatives in one block.\n\
         - Never put comments (lines starting with #) inside code blocks.\n\
         - Do NOT ask for confirmation before suggesting a command.\n\n\
         ## Terminal tools\n\
         - read_screen — observe terminal text + live shell_state/awaiting_input\n\
         - search_scrollback — regex-search the scrollback (screen + history)\n\
         - get_hyperlinks — list OSC 8 links (file://, https://) on the active screen\n\
         - get_semantic_zones — OSC 133 prompt/input/output zones on the active screen\n\
         - get_context — cheap orientation: shell state, cwd, git branch, last exit code\n\
         - get_command_history — recent commands with exit codes/durations (OSC 133)\n\
         - explain_last_failure — the last failed command + its captured output\n\
         - get_error_fixes — known error→fix correlations for this session\n\
         - send_input — type a command into the interactive shell\n\
         - send_key — send a special key (ctrl+c, enter, …)\n\
         - wait_for — wait until a regex appears or the screen stabilizes\n\
         - get_state — structured session metadata (cwd, git, shell state)\n\n\
         ## Filesystem tools\n\
         - read_file — read a file with line numbers (paginated, max 2000 lines)\n\
         - write_file — create or overwrite a file (atomic, creates dirs)\n\
         - edit_file — surgical search-and-replace\n\
         - list_files — glob-match files in the repo\n\
         - search_files — regex search across files with context lines\n\
         - run_command — run a shell command and capture stdout/stderr\n\n\
         ## Code search\n\
         - search_code — BM25 semantic search across the codebase\n\n\
         ## MCP bridge\n\
         - search_tools — discover available MCP upstream tools\n\
         - call_tool — invoke an MCP upstream tool by name\n\n\
         ## Multi-session orchestration\n\
         - list_sessions — enumerate active PTY sessions\n\
         - spawn_session — create a new PTY tab\n\
         - get_agent_status — query another agent's state\n\n\
         ## Reactive watches\n\
         - watch_for — arm a watch on this session (idle/busy/command_done/question/error/unseen/pattern); when it fires, a fresh agent runs your instructions (user-approved, bounded by max_fires/cooldown)\n\
         - list_watches — list watches armed on this session\n\
         - cancel_watch — cancel a watch by id\n\n\
         Always observe before acting. Prefer targeted, minimal commands. \
         When a task is complete, stop calling tools and summarize what you did."
    )
}

// ── Context assembly ──────────────────────────────────────────

/// Assemble terminal context using VtLogBuffer.
/// DEFERRED (2026-05-06) — will use OSC 133 blocks when available (story 1611-375b).
fn assemble_context(state: &AppState, session_id: &str) -> String {
    // Re-use the existing TerminalContext builder from ai_chat.
    // We call assemble_terminal_context with 150 lines (same default as before).
    // The result is formatted via TerminalContext::to_system_section().
    // Since that function is private to ai_chat, we call it via the public
    // assemble_terminal_context_section helper we expose below.
    crate::ai_chat::assemble_terminal_context_for_engine(state, session_id)
}

// ── Public entry point ────────────────────────────────────────

/// Start a unified conversation on `session_id`. Returns a broadcast receiver
/// for `ConversationEvent`s. The caller is responsible for bridging to a
/// Tauri Channel (step 5 will wrap this in a proper Tauri command).
pub(crate) async fn start_conversation(
    state: Arc<AppState>,
    session_id: String,
    message: String,
    config: ConversationConfig,
) -> Result<broadcast::Receiver<ConversationEvent>, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let conv_state = Arc::new(RwLock::new(AgentState::Running));
    let pause_notify = Arc::new(Notify::new());
    let (event_tx, event_rx) = broadcast::channel(256);
    let approval_tx: Arc<Mutex<Option<oneshot::Sender<bool>>>> = Arc::new(Mutex::new(None));

    let handle = ConversationHandle {
        cancel: cancel.clone(),
        state: conv_state.clone(),
        pause_notify: pause_notify.clone(),
        event_tx: event_tx.clone(),
        approval_tx: approval_tx.clone(),
    };

    use dashmap::mapref::entry::Entry;
    match ACTIVE_CONVERSATIONS.entry(session_id.clone()) {
        Entry::Occupied(_) => {
            return Err(format!(
                "Conversation already active on session {session_id}"
            ));
        }
        Entry::Vacant(v) => {
            v.insert(handle);
        }
    }

    if config.autonomy == Autonomy::Autonomous {
        state.unrestricted_sessions.insert(session_id.clone(), ());
    }

    let sid = session_id.clone();
    tokio::spawn(async move {
        let result = run_conversation(
            state.clone(),
            sid.clone(),
            message,
            config,
            cancel,
            conv_state.clone(),
            pause_notify,
            event_tx.clone(),
            approval_tx,
        )
        .await;

        state.unrestricted_sessions.remove(&sid);
        state.file_sandboxes.remove(&sid);
        ACTIVE_CONVERSATIONS.remove(&sid);

        match result {
            Ok(reason) => {
                *conv_state.write() = AgentState::Completed;
                let _ = event_tx.send(ConversationEvent::Completed {
                    reason,
                    usage: None,
                });
            }
            Err(e) => {
                tracing::error!(session_id = %sid, error = %e, "Conversation failed");
                *conv_state.write() = AgentState::Error;
                let _ = event_tx.send(ConversationEvent::Error { message: e });
            }
        }
    });

    Ok(event_rx)
}

/// Cancel an active conversation.
///
/// Idempotent: cancelling a session with no active conversation is a no-op
/// success — the desired end state (stopped) is already satisfied. This avoids
/// a false "No active conversation" error when the user clicks Stop in the
/// instant the conversation finishes on its own (the spawned task removes its
/// `ACTIVE_CONVERSATIONS` entry the moment the loop ends, so a Stop click that
/// races the natural completion would otherwise surface a scary error banner).
pub(crate) fn cancel_conversation(session_id: &str) -> Result<(), String> {
    if let Some(entry) = ACTIVE_CONVERSATIONS.get(session_id) {
        entry.cancel.store(true, Ordering::Release);
        *entry.state.write() = AgentState::Cancelled;
    }
    Ok(())
}

/// Pause an active conversation.
pub(crate) fn pause_conversation(session_id: &str) -> Result<(), String> {
    let entry = ACTIVE_CONVERSATIONS
        .get(session_id)
        .ok_or_else(|| format!("No active conversation on session {session_id}"))?;
    *entry.state.write() = AgentState::Paused;
    let _ = entry.event_tx.send(ConversationEvent::Paused);
    Ok(())
}

/// Resume a paused conversation.
pub(crate) fn resume_conversation(session_id: &str) -> Result<(), String> {
    let entry = ACTIVE_CONVERSATIONS
        .get(session_id)
        .ok_or_else(|| format!("No active conversation on session {session_id}"))?;
    *entry.state.write() = AgentState::Running;
    entry.pause_notify.notify_one();
    let _ = entry.event_tx.send(ConversationEvent::Resumed);
    Ok(())
}

/// Respond to a NeedsApproval event. `approved = true` runs the tool;
/// `false` rejects it and sends a rejection result back to the LLM.
pub(crate) fn approve_conversation_action(session_id: &str, approved: bool) -> Result<(), String> {
    let entry = ACTIVE_CONVERSATIONS
        .get(session_id)
        .ok_or_else(|| format!("No active conversation on session {session_id}"))?;
    if let Some(tx) = entry.approval_tx.lock().take() {
        let _ = tx.send(approved);
    }
    Ok(())
}

// ── Stream draining ───────────────────────────────────────────

/// Result of draining one streamed LLM response.
enum DrainOutcome {
    Done {
        text_buf: String,
        captured: Option<genai::chat::MessageContent>,
        usage: Option<genai::chat::Usage>,
    },
    Cancelled,
    Failed {
        /// Whether the error is worth retrying.
        transient: bool,
        /// Whether any content was already streamed to the UI (blocks retry to
        /// avoid duplicated text).
        emitted: bool,
        msg: String,
    },
}

/// Drain one streamed LLM response, emitting TextChunk/ReasoningChunk events as
/// content arrives. On a stream error, reports whether it's transient and
/// whether anything was already emitted so the caller can decide to retry.
async fn drain_stream(
    mut stream: genai::chat::ChatStream,
    event_tx: &broadcast::Sender<ConversationEvent>,
    cancel: &Arc<AtomicBool>,
) -> DrainOutcome {
    use futures_util::StreamExt;
    use genai::chat::ChatStreamEvent as GenaiStreamEvent;

    let mut text_buf = String::new();
    let mut captured = None;
    let mut usage = None;
    let mut emitted = false;

    loop {
        tokio::select! {
            event = stream.next() => {
                match event {
                    Some(Ok(GenaiStreamEvent::Chunk(chunk))) => {
                        emitted = true;
                        text_buf.push_str(&chunk.content);
                        let _ = event_tx.send(ConversationEvent::TextChunk { text: chunk.content });
                    }
                    Some(Ok(GenaiStreamEvent::ReasoningChunk(chunk))) => {
                        // Opus 4.7+ extended thinking. The matching thought-signature
                        // chunks are preserved via captured_content (see history append).
                        emitted = true;
                        let _ = event_tx.send(ConversationEvent::ReasoningChunk { text: chunk.content });
                    }
                    Some(Ok(GenaiStreamEvent::End(end))) => {
                        captured = end.captured_content;
                        usage = end.captured_usage;
                        break;
                    }
                    Some(Err(e)) => {
                        return DrainOutcome::Failed {
                            transient: engine::is_transient_llm_error(&e),
                            emitted,
                            msg: e.to_string(),
                        };
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if cancel.load(Ordering::Acquire) {
                    return DrainOutcome::Cancelled;
                }
            }
        }
    }

    DrainOutcome::Done {
        text_buf,
        captured,
        usage,
    }
}

// ── Internal loop ─────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn run_conversation(
    state: Arc<AppState>,
    session_id: String,
    initial_message: String,
    config: ConversationConfig,
    cancel: Arc<AtomicBool>,
    conv_state: Arc<RwLock<AgentState>>,
    pause_notify: Arc<Notify>,
    event_tx: broadcast::Sender<ConversationEvent>,
    approval_tx: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
) -> Result<String, String> {
    use genai::chat::{
        ChatMessage, ChatOptions, ChatRequest, ContentPart, Tool, ToolCall, ToolResponse,
    };

    // Create filesystem sandbox from the session's CWD so that file tools
    // (list_files, read_file, etc.) can resolve paths during the conversation.
    state
        .file_sandboxes
        .entry(session_id.clone())
        .or_try_insert_with(|| {
            let dir = state
                .sessions
                .get(&session_id)
                .and_then(|s| s.lock().cwd.clone())
                .ok_or("no cwd")?;
            super::sandbox::FileSandbox::new(&dir).map_err(|e| {
            tracing::warn!(session_id = %session_id, cwd = %dir, "FileSandbox init failed: {e}");
            e.to_string()
        })
        })
        .ok();

    // Resolve LLM from provider registry
    let registry = crate::provider_registry::load_registry();
    let resolved =
        crate::provider_registry::resolve_slot(&registry, crate::provider_registry::SlotName::Main)
            .map_err(|e| format!("AI not configured — {e}"))?;

    let llm_config = resolved.config;
    let api_key = resolved.api_key;

    // Model: registry main + per-phase overrides, or model_override if specified
    let base_model = config
        .model_override
        .as_deref()
        .unwrap_or(&llm_config.model)
        .to_string();

    // Build phase overrides from registry
    let model_overrides: std::collections::HashMap<engine::ToolPhase, String> = registry
        .phase_overrides
        .iter()
        .filter_map(|(phase, model_id)| {
            registry
                .models
                .iter()
                .find(|m| &m.id == model_id)
                .map(|m| (*phase, m.model_name.clone()))
        })
        .collect();

    let client = crate::llm_api::build_client(&llm_config, &api_key);

    // Build tools
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

    // chat_options is rebuilt per-iteration below: reasoning effort depends on the
    // per-phase model, which is only known inside the loop.

    // Build system prompt — merged from agent + chat rules
    let base_system_prompt = build_base_system_prompt(&session_id);
    let cross_session = super::context::build_cross_session_section(&state, &session_id);
    let mut last_context = assemble_context(&state, &session_id);
    let mut last_knowledge = super::context::build_knowledge_section(&state, &session_id);

    // Compose the full system prompt from the live terminal context + knowledge.
    // The context (shell_state, awaiting_input, recent output) is re-assembled
    // each iteration so the model never reasons about a stale iteration-0 snapshot.
    let compose = |context: &str, knowledge: Option<&str>| {
        compose_system_prompt(
            &format!("{base_system_prompt}\n\n{context}"),
            cross_session.as_deref(),
            knowledge,
        )
    };

    let mut chat_req = ChatRequest::default()
        .with_system(compose(&last_context, last_knowledge.as_deref()))
        .with_tools(genai_tools.clone())
        .append_message(ChatMessage::user(&initial_message));

    let mut rate_limiter = RateLimiter::new(RATE_LIMIT_PER_MINUTE, RATE_LIMIT_PER_SESSION);
    let mut tool_limiter = RateLimiter::new(
        TOOL_DISPATCH_LIMIT_PER_MINUTE,
        TOOL_DISPATCH_LIMIT_PER_SESSION,
    );
    let mut repetition = RepetitionDetector::new();
    let deadline = tokio::time::Instant::now() + LOOP_TIMEOUT;
    let mut last_tool_names: Vec<String> = Vec::new();
    let max_iterations = config.max_steps.unwrap_or(MAX_ITERATIONS);
    // When no max_steps configured: allow one tool-use round-trip, then stop.
    let is_single_response = config.max_steps.is_none();

    for iteration in 0..max_iterations {
        if cancel.load(Ordering::Acquire) {
            return Ok("cancelled".into());
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok("timeout".into());
        }

        // Refresh terminal context + knowledge each iteration so shell_state,
        // awaiting_input, and recent terminal output stay live across the loop.
        if iteration > 0 {
            let current_context = assemble_context(&state, &session_id);
            let current_knowledge = super::context::build_knowledge_section(&state, &session_id);
            if current_context != last_context || current_knowledge != last_knowledge {
                chat_req.system = Some(compose(&current_context, current_knowledge.as_deref()));
                last_context = current_context;
                last_knowledge = current_knowledge;
            }
        }

        // Pause check
        while *conv_state.read() == AgentState::Paused {
            tokio::select! {
                _ = pause_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            if cancel.load(Ordering::Acquire) {
                return Ok("cancelled".into());
            }
        }

        // Rate limit
        if let Err(wait) = rate_limiter.check() {
            if wait == Duration::ZERO {
                return Ok("session_rate_limit".into());
            }
            let _ = event_tx.send(ConversationEvent::RateLimited {
                wait_ms: wait.as_millis() as u64,
            });
            tokio::time::sleep(wait).await;
            if cancel.load(Ordering::Acquire) {
                return Ok("cancelled".into());
            }
        }
        rate_limiter.record();

        let _ = event_tx.send(ConversationEvent::Thinking { iteration });

        // Phase-based model selection
        let phase_refs: Vec<&str> = last_tool_names.iter().map(|s| s.as_str()).collect();
        let phase = classify_phase(&phase_refs);
        let model = select_model_for_phase(&base_model, &model_overrides, phase);

        // Reasoning effort depends on the active per-phase model.
        // capture_usage gives us the real prompt-token count (compaction trigger).
        let mut chat_options = ChatOptions::default()
            .with_capture_tool_calls(true)
            .with_capture_usage(true)
            .with_temperature(config.temperature.into());
        if let Some(effort) = resolve_reasoning(config.reasoning, model) {
            chat_options = chat_options
                .with_reasoning_effort(effort)
                .with_capture_reasoning_content(true);
        }

        // LLM call with bounded retry on transient errors (429/5xx/network).
        // We only retry when nothing was streamed yet, so the UI never sees
        // duplicated text.
        let mut attempt: u32 = 0;
        let (text_buf, captured, usage) = loop {
            let outcome = match client
                .exec_chat_stream(model, chat_req.clone(), Some(&chat_options))
                .await
            {
                Ok(resp) => drain_stream(resp.stream, &event_tx, &cancel).await,
                Err(e) => DrainOutcome::Failed {
                    transient: engine::is_transient_llm_error(&e),
                    emitted: false,
                    msg: e.to_string(),
                },
            };

            match outcome {
                DrainOutcome::Done {
                    text_buf,
                    captured,
                    usage,
                } => break (text_buf, captured, usage),
                DrainOutcome::Cancelled => return Ok("cancelled".into()),
                DrainOutcome::Failed {
                    transient,
                    emitted,
                    msg,
                } => {
                    if !(transient && !emitted && attempt < engine::MAX_LLM_RETRIES) {
                        return Err(format!("LLM stream error: {msg}"));
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Ok("timeout".into());
                    }
                    attempt += 1;
                    let wait = engine::retry_backoff(attempt);
                    let _ = event_tx.send(ConversationEvent::Retrying {
                        attempt,
                        wait_ms: wait.as_millis() as u64,
                        reason: msg,
                    });
                    tokio::select! {
                        _ = tokio::time::sleep(wait) => {}
                        _ = async {
                            while !cancel.load(Ordering::Acquire) {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        } => return Ok("cancelled".into()),
                    }
                }
            }
        };

        // Extract tool calls from the captured assistant content (clone so the
        // original stays intact for the signature-preserving history append below).
        let tool_calls: Vec<ToolCall> = captured
            .as_ref()
            .map(|c| {
                c.clone()
                    .into_parts()
                    .into_iter()
                    .filter_map(|p| match p {
                        ContentPart::ToolCall(tc) => Some(tc),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // On end_turn the conversation stops, so chat_req is never reused — no point
        // appending the final assistant message (it would be discarded).
        if tool_calls.is_empty() {
            return Ok("end_turn".into());
        }
        if is_single_response && iteration > 0 {
            return Ok("end_turn".into());
        }

        // Continuation: append the FULL captured assistant content so the thinking
        // block + its signature ride in the same assistant turn as the tool_use
        // (Anthropic requires this when extended thinking is on). Fall back to the
        // streamed text when nothing was captured.
        match captured {
            Some(content) => {
                chat_req = chat_req.append_message(ChatMessage::assistant(content));
            }
            None if !text_buf.is_empty() => {
                chat_req = chat_req.append_message(ChatMessage::assistant(text_buf));
            }
            None => {}
        }

        // Execute tool calls
        for tc in &tool_calls {
            if let Err(wait) = tool_limiter.check() {
                if wait == Duration::ZERO {
                    return Ok("tool_dispatch_session_limit".into());
                }
                let _ = event_tx.send(ConversationEvent::RateLimited {
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
                return Ok(format!("repetition_detected: {}", tc.fn_name));
            }

            let redacted_args = redact_json_values(&tc.fn_arguments);
            let _ = event_tx.send(ConversationEvent::ToolCall {
                tool_name: tc.fn_name.clone(),
                args: redacted_args,
            });

            let start = std::time::Instant::now();
            let mut result =
                tools::dispatch(&state, &session_id, &tc.fn_name, &tc.fn_arguments).await;

            if result.needs_approval {
                let reason = result.approval_reason.clone().unwrap_or_default();
                let command = result.approval_command.clone().unwrap_or_default();

                if config.autonomy == Autonomy::Autonomous {
                    result = tools::dispatch_approved(
                        &state,
                        &session_id,
                        &tc.fn_name,
                        &tc.fn_arguments,
                    )
                    .await;
                } else if config.bypassed_tools.contains(&tc.fn_name) {
                    let _ = event_tx.send(ConversationEvent::Bypassed {
                        tool_name: tc.fn_name.clone(),
                    });
                    result = tools::dispatch_approved(
                        &state,
                        &session_id,
                        &tc.fn_name,
                        &tc.fn_arguments,
                    )
                    .await;
                } else {
                    let _ = event_tx.send(ConversationEvent::NeedsApproval {
                        tool_name: tc.fn_name.clone(),
                        command: command.clone(),
                        reason: reason.clone(),
                    });

                    let (tx, rx) = oneshot::channel();
                    *approval_tx.lock() = Some(tx);

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
                        result = tools::dispatch_approved(
                            &state,
                            &session_id,
                            &tc.fn_name,
                            &tc.fn_arguments,
                        )
                        .await;
                    } else {
                        result = tools::ToolResult::err(format!("User rejected: {reason}"));
                    }
                }
            }

            let duration_ms = start.elapsed().as_millis() as u64;
            let _ = event_tx.send(ConversationEvent::ToolResult {
                tool_name: tc.fn_name.clone(),
                success: result.success,
                output: result.output.clone(),
                duration_ms,
            });

            let tool_resp = ToolResponse::new(tc.call_id.clone(), result.output);
            chat_req = chat_req.append_message(tool_resp);
        }

        last_tool_names = tool_calls.iter().map(|tc| tc.fn_name.clone()).collect();

        // Compaction: if the request we just sent reached the token budget,
        // elide old tool-result bodies so the next iteration stays bounded.
        // Uses the real prompt_tokens when the provider reports usage, else a
        // byte-size heuristic.
        if let Some(thr) = config.compact_after_tokens {
            let before = usage
                .as_ref()
                .and_then(|u| u.prompt_tokens)
                .map(|t| t as usize)
                .unwrap_or_else(|| engine::estimate_tokens(&chat_req.messages));
            if before > thr {
                let stats = engine::compact_history(
                    &mut chat_req.messages,
                    engine::COMPACT_KEEP_RECENT_TOOL_RESULTS,
                );
                if stats.elided > 0 {
                    let _ = event_tx.send(ConversationEvent::Compacted {
                        elided: stats.elided,
                        before_tokens: before,
                    });
                }
            }
        }
    }

    Ok("max_iterations".into())
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autonomy_default_is_assisted() {
        assert_eq!(ConversationConfig::default().autonomy, Autonomy::Assisted);
    }

    #[test]
    fn config_default_max_steps_is_none() {
        assert!(ConversationConfig::default().max_steps.is_none());
    }

    #[test]
    fn config_bypassed_tools_empty_by_default() {
        assert!(ConversationConfig::default().bypassed_tools.is_empty());
    }

    #[test]
    fn conversation_event_thinking_serializes() {
        let evt = ConversationEvent::Thinking { iteration: 3 };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"thinking\""));
        assert!(json.contains("\"iteration\":3"));
    }

    #[test]
    fn conversation_event_reasoning_chunk_serializes() {
        let evt = ConversationEvent::ReasoningChunk {
            text: "let me think".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"reasoning_chunk\""));
        assert!(json.contains("\"text\":\"let me think\""));
    }

    #[test]
    fn config_reasoning_default_is_auto() {
        assert_eq!(
            ConversationConfig::default().reasoning,
            ReasoningLevel::Auto
        );
    }

    #[test]
    fn reasoning_level_from_opt_maps_known_values() {
        assert_eq!(ReasoningLevel::from_opt(Some("off")), ReasoningLevel::Off);
        assert_eq!(ReasoningLevel::from_opt(Some("low")), ReasoningLevel::Low);
        assert_eq!(
            ReasoningLevel::from_opt(Some("medium")),
            ReasoningLevel::Medium
        );
        assert_eq!(ReasoningLevel::from_opt(Some("high")), ReasoningLevel::High);
        assert_eq!(ReasoningLevel::from_opt(None), ReasoningLevel::Auto);
        assert_eq!(
            ReasoningLevel::from_opt(Some("bogus")),
            ReasoningLevel::Auto
        );
    }

    #[test]
    fn supports_extended_thinking_gates_on_opus_47_plus() {
        assert!(supports_extended_thinking("claude-opus-4-7"));
        assert!(supports_extended_thinking("claude-opus-4-8"));
        assert!(supports_extended_thinking("CLAUDE-OPUS-4-8")); // case-insensitive
        assert!(!supports_extended_thinking("claude-opus-4-1"));
        assert!(!supports_extended_thinking("claude-sonnet-4-6"));
        assert!(!supports_extended_thinking("gpt-5"));
    }

    #[test]
    fn resolve_reasoning_off_and_unsupported_return_none() {
        use genai::chat::ReasoningEffort;
        // Off always disables, even on a capable model.
        assert!(resolve_reasoning(ReasoningLevel::Off, "claude-opus-4-8").is_none());
        // Any level is a no-op on a model without extended thinking.
        assert!(resolve_reasoning(ReasoningLevel::High, "gpt-5").is_none());
        // Auto maps to Medium on a capable model (ReasoningEffort has no PartialEq → matches!).
        assert!(matches!(
            resolve_reasoning(ReasoningLevel::Auto, "claude-opus-4-8"),
            Some(ReasoningEffort::Medium)
        ));
        assert!(matches!(
            resolve_reasoning(ReasoningLevel::Low, "claude-opus-4-7"),
            Some(ReasoningEffort::Low)
        ));
        assert!(matches!(
            resolve_reasoning(ReasoningLevel::High, "claude-opus-4-8"),
            Some(ReasoningEffort::High)
        ));
    }

    #[test]
    fn conversation_event_tool_call_serializes() {
        let evt = ConversationEvent::ToolCall {
            tool_name: "read_file".into(),
            args: serde_json::json!({"path": "src/main.rs"}),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"tool_name\":\"read_file\""));
    }

    #[test]
    fn conversation_event_bypassed_serializes() {
        let evt = ConversationEvent::Bypassed {
            tool_name: "send_input".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"bypassed\""));
        assert!(json.contains("\"tool_name\":\"send_input\""));
    }

    #[test]
    fn conversation_event_completed_serializes() {
        let evt = ConversationEvent::Completed {
            reason: "end_turn".into(),
            usage: None,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("\"reason\":\"end_turn\""));
    }

    #[test]
    fn conversation_event_needs_approval_serializes() {
        let evt = ConversationEvent::NeedsApproval {
            tool_name: "run_command".into(),
            command: "rm -rf /tmp/test".into(),
            reason: "destructive command".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"needs_approval\""));
    }

    #[test]
    fn cancel_missing_session_is_idempotent_ok() {
        // Cancel is "ensure stopped" — a session with no active conversation is
        // already stopped, so cancelling it succeeds rather than erroring. This
        // prevents a false error banner when Stop races natural completion.
        assert!(cancel_conversation("nonexistent-conv").is_ok());
    }

    #[test]
    fn pause_missing_session_errors() {
        assert!(pause_conversation("nonexistent-conv").is_err());
    }

    #[test]
    fn resume_missing_session_errors() {
        assert!(resume_conversation("nonexistent-conv").is_err());
    }

    #[test]
    fn approve_missing_session_errors() {
        assert!(approve_conversation_action("nonexistent-conv", true).is_err());
    }

    #[test]
    fn conversation_event_paused_serializes() {
        let json = serde_json::to_string(&ConversationEvent::Paused).unwrap();
        assert!(json.contains("\"type\":\"paused\""));
    }

    #[test]
    fn conversation_event_resumed_serializes() {
        let json = serde_json::to_string(&ConversationEvent::Resumed).unwrap();
        assert!(json.contains("\"type\":\"resumed\""));
    }

    #[test]
    fn conversation_event_rate_limited_serializes() {
        let evt = ConversationEvent::RateLimited { wait_ms: 2000 };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"rate_limited\""));
        assert!(json.contains("\"wait_ms\":2000"));
    }

    #[test]
    fn conversation_event_retrying_serializes() {
        let evt = ConversationEvent::Retrying {
            attempt: 2,
            wait_ms: 1000,
            reason: "HTTP 503".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"retrying\""));
        assert!(json.contains("\"attempt\":2"));
        assert!(json.contains("\"wait_ms\":1000"));
    }

    #[test]
    fn conversation_event_compacted_serializes() {
        let evt = ConversationEvent::Compacted {
            elided: 3,
            before_tokens: 120_000,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"compacted\""));
        assert!(json.contains("\"elided\":3"));
        assert!(json.contains("\"before_tokens\":120000"));
    }

    #[test]
    fn config_default_enables_compaction() {
        assert_eq!(
            ConversationConfig::default().compact_after_tokens,
            Some(engine::DEFAULT_COMPACT_THRESHOLD_TOKENS)
        );
    }

    #[test]
    fn conversation_event_error_serializes() {
        let evt = ConversationEvent::Error {
            message: "LLM timeout".into(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"LLM timeout\""));
    }

    #[test]
    fn conversation_event_tool_result_serializes() {
        let evt = ConversationEvent::ToolResult {
            tool_name: "run_command".into(),
            success: true,
            output: "ok".into(),
            duration_ms: 123,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"tool_result\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"duration_ms\":123"));
    }

    #[test]
    fn conversation_event_completed_with_usage_serializes() {
        let evt = ConversationEvent::Completed {
            reason: "end_turn".into(),
            usage: Some(ConversationUsage {
                input_tokens: 100,
                output_tokens: 50,
            }),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("\"input_tokens\":100"));
        assert!(json.contains("\"output_tokens\":50"));
    }

    #[test]
    fn bypassed_tool_check() {
        let mut config = ConversationConfig::default();
        config.bypassed_tools.insert("send_input".into());
        assert!(config.bypassed_tools.contains("send_input"));
        assert!(!config.bypassed_tools.contains("run_command"));
    }

    #[test]
    fn single_response_mode_has_no_max_steps() {
        let config = ConversationConfig {
            max_steps: None,
            ..Default::default()
        };
        assert!(config.max_steps.is_none());
    }

    #[test]
    fn autonomous_mode_sets_max_steps() {
        let config = ConversationConfig {
            autonomy: Autonomy::Autonomous,
            max_steps: Some(20),
            ..Default::default()
        };
        assert_eq!(config.max_steps, Some(20));
        assert_eq!(config.autonomy, Autonomy::Autonomous);
    }
}
