//! AI Chat backend — config, keyring, Ollama detection, and streaming.
//!
//! Separate from `llm_api.rs` (Smart Prompts) so Chat and Smart Prompts
//! can use different providers/models independently.
//! Config stored in `ai-chat.json`; API key in OS keyring under a
//! distinct service name.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::config::{load_json_config, save_json_config};
use crate::llm_api;
use crate::state::AppState;

pub(crate) const CONFIG_FILE: &str = "ai-chat.json";
const KEYRING_SERVICE: &str = "tuicommander-ai-chat";
const KEYRING_USER: &str = "api-key";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AiChatConfig {
    /// Provider identifier: "ollama", "anthropic", "openai", "openrouter", "custom"
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Model name (e.g. "qwen2.5:7b", "claude-sonnet-4-5-20241022")
    #[serde(default)]
    pub model: String,
    /// Custom base URL — pre-filled per provider, editable for custom
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Sampling temperature 0.0–1.0
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Max terminal context lines injected per turn
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_context_lines() -> u32 {
    150
}

impl Default for AiChatConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: String::new(),
            base_url: None,
            temperature: default_temperature(),
            context_lines: default_context_lines(),
        }
    }
}

impl AiChatConfig {
    pub fn is_configured(&self) -> bool {
        !self.provider.is_empty() && !self.model.is_empty()
    }

    /// Derive the effective base_url for the provider.
    pub fn effective_base_url(&self) -> Option<String> {
        if let Some(url) = &self.base_url {
            if !url.is_empty() {
                // genai concatenates base_url + "chat/completions" — trailing slash required
                let url = if url.ends_with('/') { url.clone() } else { format!("{url}/") };
                return Some(url);
            }
        }
        // Default URLs for known providers that need one
        match self.provider.as_str() {
            "ollama" => Some("http://localhost:11434/v1/".to_string()),
            "openrouter" => Some("https://openrouter.ai/api/v1/".to_string()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Keyring helpers
// ---------------------------------------------------------------------------

pub(crate) fn read_api_key() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key.trim().to_string())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read AI Chat API key: {e}")),
    }
}

fn store_api_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("Failed to save AI Chat API key: {e}"))
}

fn remove_api_key() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete AI Chat API key: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Ollama detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OllamaModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OllamaStatus {
    pub available: bool,
    pub models: Vec<OllamaModel>,
}

/// Response shape from GET /api/tags
#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagEntry>,
}

#[derive(Deserialize)]
struct OllamaTagEntry {
    name: String,
    #[serde(default)]
    size: u64,
}

async fn detect_ollama(base: &str) -> OllamaStatus {
    let url = base.trim_end_matches('/').trim_end_matches("/v1");
    let tags_url = format!("{url}/api/tags");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap_or_default();

    let resp = match client.get(&tags_url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            return OllamaStatus {
                available: false,
                models: vec![],
            }
        }
    };

    let tags: OllamaTagsResponse = resp.json().await.unwrap_or(OllamaTagsResponse {
        models: vec![],
    });

    OllamaStatus {
        available: true,
        models: tags
            .models
            .into_iter()
            .map(|t| OllamaModel {
                name: t.name,
                size: t.size,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn load_ai_chat_config() -> AiChatConfig {
    load_json_config(CONFIG_FILE)
}

#[tauri::command]
pub(crate) fn save_ai_chat_config(config: AiChatConfig) -> Result<(), String> {
    save_json_config(CONFIG_FILE, &config)
}

#[tauri::command]
pub(crate) fn has_ai_chat_api_key() -> Result<bool, String> {
    read_api_key().map(|k| k.is_some())
}

#[tauri::command]
pub(crate) fn save_ai_chat_api_key(key: String) -> Result<(), String> {
    if key.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    store_api_key(&key)
}

#[tauri::command]
pub(crate) fn delete_ai_chat_api_key() -> Result<(), String> {
    remove_api_key()
}

/// Check whether Ollama is running and list available models.
#[tauri::command]
pub(crate) async fn check_ollama_status() -> OllamaStatus {
    let config: AiChatConfig = load_json_config(CONFIG_FILE);
    let base = config
        .effective_base_url()
        .unwrap_or_else(|| "http://localhost:11434/v1/".to_string());
    detect_ollama(&base).await
}

/// Quick connection test: first validate the API key, then send a minimal completion.
#[tauri::command]
pub(crate) async fn test_ai_chat_connection() -> Result<String, String> {
    let config: AiChatConfig = load_json_config(CONFIG_FILE);
    if !config.is_configured() {
        return Err(
            "AI Chat not configured — set provider and model in Settings > AI Chat".to_string(),
        );
    }

    // Ollama doesn't need an API key; others do
    let api_key = if config.provider == "ollama" {
        read_api_key()?.unwrap_or_else(|| "ollama".to_string())
    } else {
        read_api_key()?.ok_or_else(|| {
            "No API key stored — add one in Settings > AI Chat".to_string()
        })?
    };

    // Step 1: lightweight key validation via provider-specific endpoint
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let base = config.effective_base_url().unwrap_or_default();
    let key_check = match config.provider.as_str() {
        "openrouter" => {
            let url = format!("{}auth/key", if base.is_empty() { "https://openrouter.ai/api/v1/" } else { &base });
            Some(http.get(&url).bearer_auth(&api_key).send().await)
        }
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/models";
            Some(http.get(url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .send().await)
        }
        "openai" => {
            let url = format!("{}models", if base.is_empty() { "https://api.openai.com/v1/" } else { &base });
            Some(http.get(&url).bearer_auth(&api_key).send().await)
        }
        "ollama" => {
            let url = format!("{}models", if base.is_empty() { "http://localhost:11434/v1/" } else { &base });
            Some(http.get(&url).send().await)
        }
        _ => {
            // Custom/unknown provider — skip key check, go straight to completion
            None
        }
    };

    // For known providers, verify the key check response
    if let Some(key_check) = key_check {
        match key_check {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
                    return Err("API key is invalid or expired".to_string());
                }
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    return Err("Rate limited — try again in a moment".to_string());
                }
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("Key check failed (HTTP {}): {}", status.as_u16(), &body[..body.len().min(200)]));
                }
            }
            Err(e) => {
                return Err(format!("Cannot reach {} API: {e}", config.provider));
            }
        }
    }

    // Step 2: actual completion test to verify model works
    let llm_config = llm_api::LlmApiConfig {
        provider: config.provider.clone(),
        model: config.model.clone(),
        base_url: config.effective_base_url(),
    };

    let client = llm_api::build_client(&llm_config, &api_key);

    use genai::chat::{ChatMessage, ChatRequest};
    let chat_req =
        ChatRequest::default()
            .with_system("Reply with exactly: OK")
            .append_message(ChatMessage::user("Test connection"));

    let result =
        tokio::time::timeout(Duration::from_secs(15), client.exec_chat(&config.model, chat_req, None))
            .await
            .map_err(|_| "Connection timed out after 15s".to_string())?
            .map_err(|e| format!("Connection failed: {e}"))?;

    let text = result
        .first_text()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Ok(format!("Key valid — model replied: {text}"))
}

// ---------------------------------------------------------------------------
// Conversation persistence
// ---------------------------------------------------------------------------

// Persistence types live in `ai_agent::conversation` so L2 tool-call
// extensions sit next to the agent code. L1 keeps the same import path.
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use crate::ai_agent::conversation::{
    migrate_to_current, ChatMessage, Conversation, ConversationMeta,
};

const CONVERSATIONS_DIR: &str = "ai-chat-conversations";

/// Get the conversations directory, creating it if needed.
fn conversations_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::config::config_dir().join(CONVERSATIONS_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create conversations dir: {e}"))?;
    }
    Ok(dir)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[tauri::command]
pub(crate) fn list_conversations() -> Result<Vec<ConversationMeta>, String> {
    #[derive(serde::Deserialize)]
    struct MetaOnly { meta: ConversationMeta }

    let dir = conversations_dir()?;
    let mut metas = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read conversations dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match std::fs::read_to_string(&path) {
                Ok(data) => match serde_json::from_str::<MetaOnly>(&data) {
                    Ok(wrapper) => metas.push(wrapper.meta),
                    Err(e) => tracing::warn!(path = %path.display(), error = %e, "Failed to parse conversation metadata"),
                },
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "Failed to read conversation file"),
            }
        }
    }
    metas.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(metas)
}

#[tauri::command]
pub(crate) fn load_conversation(id: String) -> Result<Conversation, String> {
    crate::ai_agent::knowledge::validate_file_stem(&id)?;
    let dir = conversations_dir()?;
    let path = dir.join(format!("{id}.json"));
    let data = std::fs::read_to_string(&path)
        .map_err(|_| format!("Conversation not found: {id}"))?;
    let mut conv: Conversation = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse conversation: {e}"))?;
    migrate_to_current(&mut conv);
    Ok(conv)
}

#[tauri::command]
pub(crate) fn save_conversation(mut conversation: Conversation) -> Result<(), String> {
    crate::ai_agent::knowledge::validate_file_stem(&conversation.meta.id)?;
    conversation.sanitize_for_persist();
    let dir = conversations_dir()?;
    let path = dir.join(format!("{}.json", conversation.meta.id));
    let data = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize conversation: {e}"))?;
    crate::config::persist_atomic(&path, data.as_bytes())
}

#[tauri::command]
pub(crate) fn delete_conversation(id: String) -> Result<(), String> {
    crate::ai_agent::knowledge::validate_file_stem(&id)?;
    let dir = conversations_dir()?;
    let path = dir.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete conversation: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn new_conversation_id() -> String {
    use std::fmt::Write;
    let ts = now_millis();
    let rand: u32 = rand::random();
    let mut id = String::with_capacity(20);
    let _ = write!(id, "{ts:x}-{rand:04x}");
    id
}

// ---------------------------------------------------------------------------
// Streaming chat types
// ---------------------------------------------------------------------------

/// Events sent to the frontend via `tauri::ipc::Channel`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub(crate) enum ChatStreamEvent {
    Chunk { text: String },
    End { full_text: String },
    Error { message: String },
}

/// Default context budget in characters (~4K tokens).
const DEFAULT_CONTEXT_BUDGET: usize = 16_000;

/// Truncate terminal output to `max_chars` using 25% head + 75% tail split.
/// Inserts a `[... N lines truncated ...]` marker in the middle.
pub(crate) fn truncate_terminal_output(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let head_budget = max_chars / 4;
    let tail_budget = max_chars - head_budget;

    // Split into lines for clean truncation at line boundaries.
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();

    // Collect head lines within budget
    let mut head_len = 0;
    let mut head_end = 0;
    for (i, line) in lines.iter().enumerate() {
        let cost = line.len() + 1; // +1 for newline
        if head_len + cost > head_budget && i > 0 {
            break;
        }
        head_len += cost;
        head_end = i + 1;
    }

    // Collect tail lines within budget (scan backwards)
    let mut tail_len = 0;
    let mut tail_start = total_lines;
    for i in (0..total_lines).rev() {
        let cost = lines[i].len() + 1;
        if tail_len + cost > tail_budget && tail_start < total_lines {
            break;
        }
        tail_len += cost;
        tail_start = i;
    }

    // Ensure no overlap
    if tail_start <= head_end {
        // Not enough to truncate meaningfully — return raw slice
        let mut result = String::with_capacity(max_chars + 40);
        result.push_str(&text[..max_chars]);
        result.push_str("\n[... truncated ...]");
        return result;
    }

    let truncated_count = tail_start - head_end;
    let mut result = String::with_capacity(head_len + tail_len + 40);
    for line in &lines[..head_end] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!("[... {truncated_count} lines truncated ...]\n"));
    for (i, line) in lines[tail_start..].iter().enumerate() {
        result.push_str(line);
        if i < lines.len() - tail_start - 1 {
            result.push('\n');
        }
    }
    result
}

/// Assembled terminal context for the AI system prompt.
#[derive(Debug, Default)]
struct TerminalContext {
    terminal_output: String,
    shell_state: Option<String>,
    cwd: Option<String>,
    agent_type: Option<String>,
    agent_intent: Option<String>,
    awaiting_input: bool,
}

impl TerminalContext {
    fn to_system_section(&self) -> String {
        let mut s = String::with_capacity(self.terminal_output.len() + 256);
        s.push_str("## Terminal Context\n\n");

        if let Some(ref state) = self.shell_state {
            s.push_str(&format!("**Shell state:** {state}\n"));
        }
        if let Some(ref cwd) = self.cwd {
            s.push_str(&format!("**Working directory:** {cwd}\n"));
        }
        if let Some(ref agent) = self.agent_type {
            s.push_str(&format!("**Agent:** {agent}\n"));
        }
        if let Some(ref intent) = self.agent_intent {
            s.push_str(&format!("**Current task:** {intent}\n"));
        }
        if self.awaiting_input {
            s.push_str("**Status:** Awaiting user input\n");
        }

        if !self.terminal_output.is_empty() {
            s.push_str("\n### Recent Terminal Output\n\n```\n");
            s.push_str(&self.terminal_output);
            if !self.terminal_output.ends_with('\n') {
                s.push('\n');
            }
            s.push_str("```\n");
        }
        s
    }
}

/// Build context from a terminal session's VtLogBuffer and state.
fn assemble_terminal_context(
    state: &AppState,
    session_id: &str,
    context_lines: u32,
) -> TerminalContext {
    let mut ctx = TerminalContext::default();

    // Session state
    if let Some(ss) = state.session_state_with_shell(session_id) {
        ctx.shell_state = ss.shell_state;
        ctx.agent_type = ss.agent_type;
        ctx.agent_intent = ss.agent_intent;
        ctx.awaiting_input = ss.awaiting_input;
    }

    // CWD from PtySession
    if let Some(sess) = state.sessions.get(session_id) {
        let sess = sess.lock();
        ctx.cwd = sess.cwd.clone();
    }

    // Terminal output from VtLogBuffer
    if let Some(buf_entry) = state.vt_log_buffers.get(session_id) {
        let buf = buf_entry.lock();
        let lines = buf.lines();
        let n = context_lines as usize;
        let skip = lines.len().saturating_sub(n);
        let mut output = String::new();
        for line in lines.iter().skip(skip) {
            let text: String = line.text();
            let trimmed = text.trim_end();
            if !trimmed.is_empty() {
                output.push_str(trimmed);
                output.push('\n');
            }
        }
        // Also include current screen rows for the freshest state
        if output.is_empty() {
            for row in buf.screen_rows() {
                let trimmed = row.trim_end();
                if !trimmed.is_empty() {
                    output.push_str(trimmed);
                    output.push('\n');
                }
            }
        }
        ctx.terminal_output = truncate_terminal_output(&output, DEFAULT_CONTEXT_BUDGET);
    }

    ctx
}

const SYSTEM_PROMPT_PREFIX: &str = "\
You are a helpful terminal assistant embedded in TUICommander. \
You can see the user's terminal output and help them understand errors, \
debug issues, explain commands, and suggest next steps. \
Be concise and practical. When suggesting commands, use fenced code blocks. \
Do not repeat terminal output back unless highlighting a specific line.";

fn build_system_prompt(ctx: &TerminalContext) -> String {
    let mut prompt = String::with_capacity(SYSTEM_PROMPT_PREFIX.len() + 256);
    prompt.push_str(SYSTEM_PROMPT_PREFIX);
    prompt.push_str("\n\n");
    prompt.push_str(&ctx.to_system_section());
    prompt
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex as TokioMutex;

lazy_static::lazy_static! {
    static ref ACTIVE_CHATS: TokioMutex<HashMap<String, Arc<AtomicBool>>> =
        TokioMutex::new(HashMap::new());
}

// ---------------------------------------------------------------------------
// Streaming command
// ---------------------------------------------------------------------------

/// Input message from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StreamChatMessage {
    pub role: String,
    pub content: String,
}

#[tauri::command]
pub(crate) async fn stream_ai_chat(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    messages: Vec<StreamChatMessage>,
    chat_id: String,
    on_event: tauri::ipc::Channel<ChatStreamEvent>,
) -> Result<(), String> {
    let config: AiChatConfig = load_json_config(CONFIG_FILE);
    if !config.is_configured() {
        let _ = on_event.send(ChatStreamEvent::Error {
            message: "AI Chat not configured — set provider and model in Settings > AI Chat"
                .to_string(),
        });
        return Ok(());
    }

    // Resolve API key
    let api_key = if config.provider == "ollama" {
        read_api_key()?.unwrap_or_else(|| "ollama".to_string())
    } else {
        match read_api_key()? {
            Some(k) => k,
            None => {
                let _ = on_event.send(ChatStreamEvent::Error {
                    message: "No API key stored — add one in Settings > AI Chat".to_string(),
                });
                return Ok(());
            }
        }
    };

    // Assemble terminal context
    let ctx = assemble_terminal_context(&state, &session_id, config.context_lines);
    let mut system_prompt = build_system_prompt(&ctx);
    if let Some(section) = crate::ai_agent::context::build_knowledge_section(&state, &session_id) {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&section);
    }

    // Build genai request
    let llm_config = llm_api::LlmApiConfig {
        provider: config.provider.clone(),
        model: config.model.clone(),
        base_url: config.effective_base_url(),
    };
    let client = llm_api::build_client(&llm_config, &api_key);

    use genai::chat::{ChatMessage as GenaiMessage, ChatRequest};
    let mut chat_req = ChatRequest::default().with_system(system_prompt);

    for msg in &messages {
        match msg.role.as_str() {
            "user" => chat_req = chat_req.append_message(GenaiMessage::user(&msg.content)),
            "assistant" => {
                chat_req = chat_req.append_message(GenaiMessage::assistant(&msg.content))
            }
            _ => {}
        }
    }

    // Set up cancellation
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = ACTIVE_CHATS.lock().await;
        active.insert(chat_id.clone(), cancelled.clone());
    }

    // Stream
    let result = stream_with_batching(
        client,
        &config.model,
        chat_req,
        &on_event,
        &cancelled,
    )
    .await;

    // Cleanup cancellation token
    {
        let mut active = ACTIVE_CHATS.lock().await;
        active.remove(&chat_id);
    }

    if let Err(e) = result {
        let _ = on_event.send(ChatStreamEvent::Error {
            message: e.to_string(),
        });
    }

    Ok(())
}

/// Stream LLM response with ~50ms chunk batching to avoid IPC saturation.
async fn stream_with_batching(
    client: genai::Client,
    model: &str,
    chat_req: genai::chat::ChatRequest,
    on_event: &tauri::ipc::Channel<ChatStreamEvent>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use genai::chat::ChatStreamEvent as GenaiStreamEvent;

    let stream_resp = client
        .exec_chat_stream(model, chat_req, None)
        .await
        .map_err(|e| format!("Failed to start stream: {e}"))?;

    let mut stream = stream_resp.stream;
    let mut full_text = String::new();
    let mut batch_buf = String::new();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        // Check cancellation at top of each iteration
        if cancelled.load(Ordering::Relaxed) {
            if !batch_buf.is_empty() {
                let _ = on_event.send(ChatStreamEvent::Chunk { text: batch_buf.clone() });
                full_text.push_str(&batch_buf);
            }
            let _ = on_event.send(ChatStreamEvent::End { full_text });
            return Ok(());
        }

        tokio::select! {
            _ = interval.tick() => {
                if !batch_buf.is_empty() {
                    let _ = on_event.send(ChatStreamEvent::Chunk { text: batch_buf.clone() });
                    full_text.push_str(&batch_buf);
                    batch_buf.clear();
                }
            }
            event = stream.next() => {
                match event {
                    Some(Ok(GenaiStreamEvent::Chunk(chunk))) => {
                        batch_buf.push_str(&chunk.content);
                    }
                    Some(Ok(GenaiStreamEvent::End(_))) => {
                        if !batch_buf.is_empty() {
                            let _ = on_event.send(ChatStreamEvent::Chunk { text: batch_buf.clone() });
                            full_text.push_str(&batch_buf);
                        }
                        let _ = on_event.send(ChatStreamEvent::End { full_text });
                        return Ok(());
                    }
                    Some(Err(e)) => {
                        if !batch_buf.is_empty() {
                            let _ = on_event.send(ChatStreamEvent::Chunk { text: batch_buf.clone() });
                            full_text.push_str(&batch_buf);
                        }
                        return Err(format!("Stream error: {e}"));
                    }
                    None => {
                        if !batch_buf.is_empty() {
                            let _ = on_event.send(ChatStreamEvent::Chunk { text: batch_buf.clone() });
                            full_text.push_str(&batch_buf);
                        }
                        let _ = on_event.send(ChatStreamEvent::End { full_text });
                        return Ok(());
                    }
                    _ => {} // Start, ReasoningChunk, etc.
                }
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn cancel_ai_chat(chat_id: String) -> Result<(), String> {
    let active = ACTIVE_CHATS.lock().await;
    if let Some(flag) = active.get(&chat_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_ollama() {
        let config = AiChatConfig::default();
        assert_eq!(config.provider, "ollama");
        assert!(config.model.is_empty());
        assert!(!config.is_configured());
    }

    #[test]
    fn configured_when_provider_and_model_set() {
        let config = AiChatConfig {
            provider: "ollama".to_string(),
            model: "qwen2.5:7b".to_string(),
            ..Default::default()
        };
        assert!(config.is_configured());
    }

    #[test]
    fn effective_base_url_ollama_default() {
        let config = AiChatConfig {
            provider: "ollama".to_string(),
            base_url: None,
            ..Default::default()
        };
        assert_eq!(
            config.effective_base_url(),
            Some("http://localhost:11434/v1/".to_string())
        );
    }

    #[test]
    fn effective_base_url_custom_override() {
        let config = AiChatConfig {
            provider: "ollama".to_string(),
            base_url: Some("http://10.0.0.5:11434/v1/".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config.effective_base_url(),
            Some("http://10.0.0.5:11434/v1/".to_string())
        );
    }

    #[test]
    fn effective_base_url_anthropic_none() {
        let config = AiChatConfig {
            provider: "anthropic".to_string(),
            base_url: None,
            ..Default::default()
        };
        assert!(config.effective_base_url().is_none());
    }

    #[test]
    fn effective_base_url_openrouter_default() {
        let config = AiChatConfig {
            provider: "openrouter".to_string(),
            base_url: None,
            ..Default::default()
        };
        assert_eq!(
            config.effective_base_url(),
            Some("https://openrouter.ai/api/v1/".to_string())
        );
    }

    #[test]
    fn serde_round_trip_minimal() {
        let config = AiChatConfig {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20241022".to_string(),
            base_url: None,
            temperature: 0.5,
            context_lines: 200,
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: AiChatConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.provider, "anthropic");
        assert_eq!(loaded.model, "claude-sonnet-4-5-20241022");
        assert!(loaded.base_url.is_none());
        assert!((loaded.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(loaded.context_lines, 200);
    }

    #[test]
    fn serde_round_trip_with_base_url() {
        let config = AiChatConfig {
            provider: "custom".to_string(),
            model: "my-model".to_string(),
            base_url: Some("https://my-llm.internal/v1/".to_string()),
            temperature: 0.3,
            context_lines: 100,
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: AiChatConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.base_url,
            Some("https://my-llm.internal/v1/".to_string())
        );
    }

    #[test]
    fn missing_fields_use_defaults() {
        let loaded: AiChatConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(loaded.provider, "ollama");
        assert!(loaded.model.is_empty());
        assert!((loaded.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(loaded.context_lines, 150);
    }

    #[test]
    fn base_url_skipped_when_none() {
        let config = AiChatConfig {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            base_url: None,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("base_url"));
    }

    #[test]
    fn ollama_status_serializes() {
        let status = OllamaStatus {
            available: true,
            models: vec![
                OllamaModel {
                    name: "qwen2.5:7b".to_string(),
                    size: 4_000_000_000,
                },
                OllamaModel {
                    name: "llama3.3:8b".to_string(),
                    size: 5_000_000_000,
                },
            ],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("qwen2.5:7b"));
        assert!(json.contains("\"available\":true"));
    }

    #[test]
    fn ollama_tags_response_deserializes() {
        let json = r#"{"models":[{"name":"qwen2.5:7b","size":4000000000},{"name":"llama3.3:8b","size":5000000000}]}"#;
        let resp: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);
        assert_eq!(resp.models[0].name, "qwen2.5:7b");
    }

    #[test]
    fn ollama_tags_response_empty() {
        let json = r#"{"models":[]}"#;
        let resp: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.models.is_empty());
    }

    // -- Conversation persistence tests --

    #[test]
    fn chat_message_serde_round_trip() {
        let msg = ChatMessage::text("user", "explain this error", 1713200000000);
        let json = serde_json::to_string(&msg).unwrap();
        let loaded: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.role, "user");
        assert_eq!(loaded.content, "explain this error");
        assert_eq!(loaded.timestamp, 1713200000000);
    }

    #[test]
    fn chat_message_default_timestamp() {
        let json = r#"{"role":"assistant","content":"hello"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.timestamp, 0);
    }

    #[test]
    fn conversation_meta_serde_round_trip() {
        let meta = ConversationMeta {
            id: "abc123".to_string(),
            title: "Debug build error".to_string(),
            session_id: Some("sess-42".to_string()),
            created: 1713200000000,
            updated: 1713200060000,
            message_count: 4,
            provider: "ollama".to_string(),
            model: "qwen2.5:7b".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let loaded: ConversationMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.id, "abc123");
        assert_eq!(loaded.session_id, Some("sess-42".to_string()));
        assert_eq!(loaded.message_count, 4);
    }

    #[test]
    fn conversation_meta_optional_session_id_skipped() {
        let meta = ConversationMeta {
            id: "x".to_string(),
            title: "t".to_string(),
            session_id: None,
            created: 0,
            updated: 0,
            message_count: 0,
            provider: String::new(),
            model: String::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("session_id"));
    }

    #[test]
    fn conversation_full_serde_round_trip() {
        let conv = Conversation {
            meta: ConversationMeta {
                id: "conv-1".to_string(),
                title: "Fix CI".to_string(),
                session_id: None,
                created: 1713200000000,
                updated: 1713200060000,
                message_count: 2,
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-5-20241022".to_string(),
            },
            messages: vec![
                ChatMessage::text("user", "why is CI failing?", 1713200000000),
                ChatMessage::text(
                    "assistant",
                    "The test suite has a flaky assertion...",
                    1713200001000,
                ),
            ],
            schema_version: crate::ai_agent::conversation::CURRENT_SCHEMA_VERSION,
        };
        let json = serde_json::to_string_pretty(&conv).unwrap();
        let loaded: Conversation = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.meta.id, "conv-1");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].role, "assistant");
    }

    /// Direct file-based CRUD test — bypasses global config override to
    /// avoid races with other tests sharing the `CONFIG_DIR_OVERRIDE` mutex.
    #[test]
    fn conversation_file_crud() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join(CONVERSATIONS_DIR);
        std::fs::create_dir_all(&conv_dir).unwrap();

        let id = new_conversation_id();
        assert!(!id.is_empty());

        let conv = Conversation {
            meta: ConversationMeta {
                id: id.clone(),
                title: "Test conversation".to_string(),
                session_id: Some("sess-1".to_string()),
                created: now_millis(),
                updated: now_millis(),
                message_count: 1,
                provider: "ollama".to_string(),
                model: "qwen2.5:7b".to_string(),
            },
            messages: vec![ChatMessage::text("user", "hello", now_millis())],
            schema_version: crate::ai_agent::conversation::CURRENT_SCHEMA_VERSION,
        };

        // Save
        let path = conv_dir.join(format!("{id}.json"));
        let data = serde_json::to_string_pretty(&conv).unwrap();
        std::fs::write(&path, &data).unwrap();

        // List by scanning directory
        let entries: Vec<_> = std::fs::read_dir(&conv_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .collect();
        assert_eq!(entries.len(), 1);

        // Load
        let loaded_data = std::fs::read_to_string(&path).unwrap();
        let loaded: Conversation = serde_json::from_str(&loaded_data).unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "hello");
        assert_eq!(loaded.meta.id, id);

        // Delete
        std::fs::remove_file(&path).unwrap();
        let entries: Vec<_> = std::fs::read_dir(&conv_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn load_nonexistent_conversation_errors() {
        // Use the Tauri command directly via config override
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let result = load_conversation("does-not-exist".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // -- Truncation tests --

    #[test]
    fn truncate_short_text_unchanged() {
        let text = "line1\nline2\nline3";
        let result = truncate_terminal_output(text, 1000);
        assert_eq!(result, text);
    }

    #[test]
    fn truncate_preserves_line_boundaries() {
        // Create text that exceeds budget
        let lines: Vec<String> = (0..100).map(|i| format!("line {i}: some terminal output here")).collect();
        let text = lines.join("\n");
        let result = truncate_terminal_output(&text, 500);
        assert!(result.len() < text.len());
        assert!(result.contains("[..."));
        assert!(result.contains("lines truncated ...]"));
    }

    #[test]
    fn truncate_25_75_split() {
        // 200 lines of ~20 chars each = ~4000 chars. Budget 1000 → must truncate.
        let lines: Vec<String> = (0..200).map(|i| format!("line {:>3}: data here", i)).collect();
        let text = lines.join("\n");
        let result = truncate_terminal_output(&text, 1000);

        // Head should have fewer lines than tail
        let parts: Vec<&str> = result.split("[...").collect();
        assert_eq!(parts.len(), 2, "should have truncation marker");
        let head_part = parts[0];
        let tail_part = parts[1];
        // Head is roughly 25% of budget, tail 75%
        assert!(tail_part.len() > head_part.len(),
            "tail ({}) should be larger than head ({})", tail_part.len(), head_part.len());
    }

    #[test]
    fn truncate_includes_first_and_last_lines() {
        let lines: Vec<String> = (0..200).map(|i| format!("LINE-{i:03}")).collect();
        let text = lines.join("\n");
        let result = truncate_terminal_output(&text, 500);
        assert!(result.contains("LINE-000"), "should contain first line");
        assert!(result.contains("LINE-199"), "should contain last line");
    }

    #[test]
    fn truncate_empty_text() {
        let result = truncate_terminal_output("", 1000);
        assert_eq!(result, "");
    }

    #[test]
    fn truncate_exact_budget() {
        let text = "abcde";
        let result = truncate_terminal_output(text, 5);
        assert_eq!(result, "abcde");
    }

    // -- ChatStreamEvent serialization tests --

    #[test]
    fn chat_stream_event_chunk_serializes() {
        let event = ChatStreamEvent::Chunk { text: "hello".to_string() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"chunk""#), "json: {json}");
        assert!(json.contains("hello"), "json: {json}");
    }

    #[test]
    fn chat_stream_event_end_serializes() {
        let event = ChatStreamEvent::End { full_text: "full response".to_string() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"end""#), "json: {json}");
        assert!(json.contains("full response"), "json: {json}");
    }

    #[test]
    fn chat_stream_event_error_serializes() {
        let event = ChatStreamEvent::Error { message: "connection failed".to_string() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"error""#), "json: {json}");
        assert!(json.contains("connection failed"), "json: {json}");
    }

    // -- Context assembly tests --

    #[test]
    fn terminal_context_to_system_section_full() {
        let ctx = TerminalContext {
            terminal_output: "$ cargo build\nerror[E0308]: mismatched types".to_string(),
            shell_state: Some("idle".to_string()),
            cwd: Some("/home/user/project".to_string()),
            agent_type: Some("claude-code".to_string()),
            agent_intent: Some("fixing build errors".to_string()),
            awaiting_input: false,
        };
        let section = ctx.to_system_section();
        assert!(section.contains("**Shell state:** idle"));
        assert!(section.contains("**Working directory:** /home/user/project"));
        assert!(section.contains("**Agent:** claude-code"));
        assert!(section.contains("**Current task:** fixing build errors"));
        assert!(section.contains("cargo build"));
        assert!(section.contains("error[E0308]"));
        assert!(!section.contains("Awaiting user input"));
    }

    #[test]
    fn terminal_context_to_system_section_minimal() {
        let ctx = TerminalContext::default();
        let section = ctx.to_system_section();
        assert!(section.contains("## Terminal Context"));
        assert!(!section.contains("```")); // no code block when no output
    }

    #[test]
    fn terminal_context_awaiting_input() {
        let ctx = TerminalContext {
            awaiting_input: true,
            ..Default::default()
        };
        let section = ctx.to_system_section();
        assert!(section.contains("Awaiting user input"));
    }

    #[test]
    fn build_system_prompt_includes_prefix_and_context() {
        let ctx = TerminalContext {
            terminal_output: "$ ls\nfile.rs".to_string(),
            shell_state: Some("idle".to_string()),
            ..Default::default()
        };
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.starts_with("You are a helpful terminal assistant"));
        assert!(prompt.contains("## Terminal Context"));
        assert!(prompt.contains("file.rs"));
    }

    // -- StreamChatMessage deserialization --

    #[test]
    fn stream_chat_message_deserializes() {
        let json = r#"{"role":"user","content":"explain this error"}"#;
        let msg: StreamChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "explain this error");
    }
}
