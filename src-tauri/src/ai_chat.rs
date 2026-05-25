//! AI Chat backend — config, credentials, Ollama detection, and streaming.
//!
//! Separate from `llm_api.rs` (Smart Prompts) so Chat and Smart Prompts
//! can use different providers/models independently.
//! Config stored in `ai-chat.json`; API key in the unified credential vault.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::config::{load_json_config, save_json_config};
use crate::state::AppState;

pub(crate) const CONFIG_FILE: &str = "ai-chat.json";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AiChatConfig {
    /// Sampling temperature 0.0–1.0
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    // Legacy fields — read from old ai-chat.json for one-time migration to
    // provider registry, never written again.
    #[serde(default = "default_provider", skip_serializing)]
    pub provider: String,
    #[serde(default, skip_serializing)]
    pub model: String,
    #[serde(default, skip_serializing)]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing)]
    pub agent_model_overrides: Option<HashMap<crate::ai_agent::engine::ToolPhase, String>>,
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

impl Default for AiChatConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            provider: default_provider(),
            model: String::new(),
            base_url: None,
            agent_model_overrides: None,
        }
    }
}

impl AiChatConfig {
    pub fn is_configured(&self) -> bool {
        !self.provider.is_empty() && !self.model.is_empty()
    }

    /// Derive the effective base_url for the provider.
    pub fn effective_base_url(&self) -> Option<String> {
        if let Some(url) = &self.base_url
            && !url.is_empty()
        {
            // genai concatenates base_url + "chat/completions" — trailing slash required
            let url = if url.ends_with('/') {
                url.clone()
            } else {
                format!("{url}/")
            };
            return Some(url);
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

pub(crate) async fn detect_ollama(base: &str) -> OllamaStatus {
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
            };
        }
    };

    let tags: OllamaTagsResponse = resp
        .json()
        .await
        .unwrap_or(OllamaTagsResponse { models: vec![] });

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

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_ai_chat_config() -> AiChatConfig {
    load_json_config(CONFIG_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_ai_chat_config(config: AiChatConfig) -> Result<(), String> {
    save_json_config(CONFIG_FILE, &config)
}

/// Assemble terminal context string for use by conversation_engine.rs.
/// Returns the formatted system section (terminal state + recent output).
pub(crate) fn assemble_terminal_context_for_engine(
    state: &crate::state::AppState,
    session_id: &str,
) -> String {
    let ctx = assemble_terminal_context(state, session_id, 150);
    let mut section = ctx.to_system_section();

    // Prefer OSC 133 block context; fall back to VtLogBuffer output already in section.
    if let Some(entry) = state.session_knowledge.get(session_id) {
        let knowledge = entry.lock();
        if let Some(block_ctx) = assemble_block_context(&knowledge, DEFAULT_CONTEXT_BUDGET) {
            // Replace the VtLogBuffer "Recent Terminal Output" with structured blocks.
            if let Some(pos) = section.find("\n### Recent Terminal Output\n") {
                section.truncate(pos);
                section.push('\n');
            }
            section.push_str(&block_ctx);
        }
    }

    section
}

// ---------------------------------------------------------------------------
// Conversation persistence
// ---------------------------------------------------------------------------

// Persistence types live in `ai_agent::conversation` so L2 tool-call
// extensions sit next to the agent code. L1 keeps the same import path.
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use crate::ai_agent::conversation::{ChatMessage, Conversation, ConversationMeta};

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

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn list_conversations() -> Result<Vec<ConversationMeta>, String> {
    #[derive(serde::Deserialize)]
    struct MetaOnly {
        meta: ConversationMeta,
    }

    let dir = conversations_dir()?;
    let mut metas = Vec::new();
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("Failed to read conversations dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match std::fs::read_to_string(&path) {
                Ok(data) => match serde_json::from_str::<MetaOnly>(&data) {
                    Ok(wrapper) => metas.push(wrapper.meta),
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "Failed to parse conversation metadata")
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to read conversation file")
                }
            }
        }
    }
    metas.sort_by_key(|a| std::cmp::Reverse(a.updated));
    Ok(metas)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_conversation(id: String) -> Result<Conversation, String> {
    crate::ai_agent::knowledge::validate_file_stem(&id)?;
    let dir = conversations_dir()?;
    let path = dir.join(format!("{id}.json"));
    let data =
        std::fs::read_to_string(&path).map_err(|_| format!("Conversation not found: {id}"))?;
    let conv: Conversation =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse conversation: {e}"))?;
    Ok(conv)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_conversation(mut conversation: Conversation) -> Result<(), String> {
    crate::ai_agent::knowledge::validate_file_stem(&conversation.meta.id)?;
    conversation.sanitize_for_persist();
    let dir = conversations_dir()?;
    let path = dir.join(format!("{}.json", conversation.meta.id));
    let data = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize conversation: {e}"))?;
    crate::config::persist_atomic(&path, data.as_bytes())
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn delete_conversation(id: String) -> Result<(), String> {
    crate::ai_agent::knowledge::validate_file_stem(&id)?;
    let dir = conversations_dir()?;
    let path = dir.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete conversation: {e}"))?;
    }
    Ok(())
}

#[cfg_attr(feature = "desktop", tauri::command)]
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

/// Per-million-token pricing: (input_usd, output_usd, cached_input_usd).
/// Cached input price = standard input * cache_discount (typically 0.10 for Anthropic, 0.50 for OpenAI).
#[allow(dead_code)]
struct ModelPricing {
    input_per_m: f64,
    output_per_m: f64,
    cached_input_per_m: f64,
}

#[allow(dead_code)]
fn model_pricing(model: &str) -> Option<ModelPricing> {
    // Match by prefix to handle version suffixes (e.g. claude-sonnet-4-5-20241022)
    let m = model.to_lowercase();
    // Anthropic
    if m.contains("claude-opus-4") || m.contains("claude-opus-5") {
        return Some(ModelPricing {
            input_per_m: 15.0,
            output_per_m: 75.0,
            cached_input_per_m: 1.50,
        });
    }
    if m.contains("claude-sonnet-4")
        || m.contains("claude-3-7-sonnet")
        || m.contains("claude-3-5-sonnet")
        || m.contains("claude-sonnet-4-5")
    {
        return Some(ModelPricing {
            input_per_m: 3.0,
            output_per_m: 15.0,
            cached_input_per_m: 0.30,
        });
    }
    if m.contains("claude-3-5-haiku") || m.contains("claude-haiku-4") {
        return Some(ModelPricing {
            input_per_m: 0.80,
            output_per_m: 4.0,
            cached_input_per_m: 0.08,
        });
    }
    if m.contains("claude-3-opus") {
        return Some(ModelPricing {
            input_per_m: 15.0,
            output_per_m: 75.0,
            cached_input_per_m: 1.50,
        });
    }
    if m.contains("claude-3-haiku") {
        return Some(ModelPricing {
            input_per_m: 0.25,
            output_per_m: 1.25,
            cached_input_per_m: 0.03,
        });
    }
    // OpenAI
    if m.contains("gpt-4o-mini") {
        return Some(ModelPricing {
            input_per_m: 0.15,
            output_per_m: 0.60,
            cached_input_per_m: 0.075,
        });
    }
    if m.contains("gpt-4o") {
        return Some(ModelPricing {
            input_per_m: 2.50,
            output_per_m: 10.0,
            cached_input_per_m: 1.25,
        });
    }
    if m.contains("gpt-4-turbo") || m.contains("gpt-4-1106") || m.contains("gpt-4-0125") {
        return Some(ModelPricing {
            input_per_m: 10.0,
            output_per_m: 30.0,
            cached_input_per_m: 5.0,
        });
    }
    if m.contains("o3-mini") || m.contains("o1-mini") {
        return Some(ModelPricing {
            input_per_m: 1.10,
            output_per_m: 4.40,
            cached_input_per_m: 0.55,
        });
    }
    if m.contains("o1") || m.contains("o3") {
        return Some(ModelPricing {
            input_per_m: 15.0,
            output_per_m: 60.0,
            cached_input_per_m: 7.50,
        });
    }
    None
}

/// Estimate cost in USD for a completion. Returns None if the model is not in the pricing table.
#[allow(dead_code)]
pub(crate) fn estimate_cost_usd(
    model: &str,
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    cached_tokens: Option<i32>,
) -> Option<f64> {
    let pricing = model_pricing(model)?;
    let prompt = prompt_tokens.unwrap_or(0) as f64;
    let completion = completion_tokens.unwrap_or(0) as f64;
    let cached = cached_tokens.unwrap_or(0) as f64;
    let uncached_input = (prompt - cached).max(0.0);
    let cost = (uncached_input * pricing.input_per_m
        + cached * pricing.cached_input_per_m
        + completion * pricing.output_per_m)
        / 1_000_000.0;
    Some(cost)
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
    /// Current terminal mode (Shell vs FullscreenTui with app hint + depth).
    /// When a TUI (vim, lazygit, htop, …) is in the alternate screen buffer,
    /// the model must prefer key-level input (q, ctrl+c) over shell commands.
    terminal_mode: Option<crate::ai_agent::tui_detect::TerminalMode>,
}

impl TerminalContext {
    fn to_system_section(&self) -> String {
        use crate::ai_agent::tui_detect::TerminalMode;

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
        if let Some(TerminalMode::FullscreenTui { app_hint, depth }) = &self.terminal_mode {
            let hint = app_hint.as_deref().unwrap_or("unknown");
            s.push_str(&format!(
                "**Terminal mode:** fullscreen TUI (app: {hint}, depth: {depth}) — \
                 suggest keystrokes (e.g. `q`, `:q`, `ctrl+c`) instead of shell commands; \
                 shell commands will not be interpreted until the TUI exits.\n"
            ));
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

/// Build structured context from OSC 133 CommandOutcome blocks.
///
/// Selects the most recent blocks that fit within `budget` characters.
/// Returns `None` when no blocks are recorded (caller falls back to VtLogBuffer).
pub(crate) fn assemble_block_context(
    knowledge: &crate::ai_agent::knowledge::SessionKnowledge,
    budget: usize,
) -> Option<String> {
    if knowledge.commands.is_empty() {
        return None;
    }

    // Collect blocks from most-recent to oldest, respecting the budget.
    let mut blocks: Vec<String> = Vec::new();
    let mut used = 0usize;

    for outcome in knowledge.commands.iter().rev() {
        let block = format_command_block(outcome);
        if used + block.len() > budget && !blocks.is_empty() {
            break;
        }
        used += block.len();
        blocks.push(block);
    }

    if blocks.is_empty() {
        return None;
    }

    blocks.reverse();
    let mut out = String::with_capacity(used + 64);
    out.push_str("### Recent Commands (OSC 133)\n\n");
    for block in blocks {
        out.push_str(&block);
    }
    Some(out)
}

fn format_command_block(outcome: &crate::ai_agent::knowledge::CommandOutcome) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();
    let exit = match outcome.exit_code {
        Some(c) => c.to_string(),
        None => "?".to_string(),
    };
    let _ = writeln!(
        s,
        "[cmd: {}] [cwd: {}] [exit: {}] [duration: {}ms]",
        outcome.command.trim(),
        outcome.cwd,
        exit,
        outcome.duration_ms,
    );
    let snippet = outcome.output_snippet.trim();
    if !snippet.is_empty() {
        s.push_str("```\n");
        s.push_str(snippet);
        if !snippet.ends_with('\n') {
            s.push('\n');
        }
        s.push_str("```\n");
    }
    s.push('\n');
    s
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
        ctx.terminal_mode = ss.terminal_mode;
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
        let screen_rows: Vec<String> = buf.screen_rows().into_iter().collect();
        if output.is_empty() {
            for row in &screen_rows {
                let trimmed = row.trim_end();
                if !trimmed.is_empty() {
                    output.push_str(trimmed);
                    output.push('\n');
                }
            }
        }
        ctx.terminal_output = truncate_terminal_output(&output, DEFAULT_CONTEXT_BUDGET);

        // Enrich TUI mode with app_hint from visible screen if PTY hasn't
        // identified one yet (e.g. vim, lazygit, htop detected by signature).
        if let Some(crate::ai_agent::tui_detect::TerminalMode::FullscreenTui {
            app_hint: None,
            depth,
        }) = ctx.terminal_mode.clone()
            && let Some(app) = crate::ai_agent::tui_detect::detect_app_from_rows(&screen_rows)
        {
            ctx.terminal_mode = Some(crate::ai_agent::tui_detect::TerminalMode::FullscreenTui {
                app_hint: Some(app.to_string()),
                depth,
            });
        }
    }

    ctx
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
        // Legacy fields are skip_serializing — only temperature survives round-trip.
        let config = AiChatConfig {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20241022".to_string(),
            base_url: None,
            temperature: 0.5,
            agent_model_overrides: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: AiChatConfig = serde_json::from_str(&json).unwrap();
        // Legacy fields are not serialized, so loaded has defaults.
        assert_eq!(loaded.provider, "ollama");
        assert!(loaded.model.is_empty());
        assert!(loaded.base_url.is_none());
        assert!((loaded.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn serde_round_trip_with_base_url() {
        // base_url is skip_serializing — it's a legacy migration field.
        // Verify that old JSON with base_url can still be read (deserializes fine).
        let json = r#"{"temperature":0.3,"base_url":"https://my-llm.internal/v1/"}"#;
        let loaded: AiChatConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            loaded.base_url,
            Some("https://my-llm.internal/v1/".to_string())
        );
        assert!((loaded.temperature - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let loaded: AiChatConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(loaded.provider, "ollama");
        assert!(loaded.model.is_empty());
        assert!((loaded.temperature - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn model_overrides_round_trip() {
        let mut overrides = HashMap::new();
        overrides.insert(
            crate::ai_agent::engine::ToolPhase::Search,
            "anthropic/claude-haiku-3-5".to_string(),
        );
        overrides.insert(
            crate::ai_agent::engine::ToolPhase::Read,
            "anthropic/claude-haiku-3-5".to_string(),
        );
        // agent_model_overrides is skip_serializing (migration-only).
        // Verify old JSON with this field can still be deserialized.
        let json = r#"{"temperature":0.7,"provider":"openrouter","model":"anthropic/claude-sonnet-4-5","agent_model_overrides":{"search":"anthropic/claude-haiku-3-5","read":"anthropic/claude-haiku-3-5"}}"#;
        let loaded: AiChatConfig = serde_json::from_str(json).unwrap();
        let loaded_overrides = loaded.agent_model_overrides.unwrap();
        assert_eq!(loaded_overrides.len(), 2);
        assert_eq!(
            loaded_overrides[&crate::ai_agent::engine::ToolPhase::Search],
            "anthropic/claude-haiku-3-5"
        );
    }

    #[test]
    fn model_overrides_skipped_when_none() {
        let config = AiChatConfig {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("agent_model_overrides"));
    }

    #[test]
    fn missing_overrides_deserialize_as_none() {
        let loaded: AiChatConfig = serde_json::from_str("{}").unwrap();
        assert!(loaded.agent_model_overrides.is_none());
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
        let lines: Vec<String> = (0..100)
            .map(|i| format!("line {i}: some terminal output here"))
            .collect();
        let text = lines.join("\n");
        let result = truncate_terminal_output(&text, 500);
        assert!(result.len() < text.len());
        assert!(result.contains("[..."));
        assert!(result.contains("lines truncated ...]"));
    }

    #[test]
    fn truncate_25_75_split() {
        // 200 lines of ~20 chars each = ~4000 chars. Budget 1000 → must truncate.
        let lines: Vec<String> = (0..200)
            .map(|i| format!("line {:>3}: data here", i))
            .collect();
        let text = lines.join("\n");
        let result = truncate_terminal_output(&text, 1000);

        // Head should have fewer lines than tail
        let parts: Vec<&str> = result.split("[...").collect();
        assert_eq!(parts.len(), 2, "should have truncation marker");
        let head_part = parts[0];
        let tail_part = parts[1];
        // Head is roughly 25% of budget, tail 75%
        assert!(
            tail_part.len() > head_part.len(),
            "tail ({}) should be larger than head ({})",
            tail_part.len(),
            head_part.len()
        );
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
            terminal_mode: None,
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
    fn terminal_context_tui_mode_appears_in_system_section() {
        use crate::ai_agent::tui_detect::TerminalMode;
        let ctx = TerminalContext {
            terminal_mode: Some(TerminalMode::FullscreenTui {
                app_hint: Some("vim".to_string()),
                depth: 1,
            }),
            ..Default::default()
        };
        let section = ctx.to_system_section();
        assert!(section.contains("fullscreen TUI"));
        assert!(section.contains("vim"));
        assert!(section.contains("depth: 1"));
        assert!(section.contains("keystrokes"));
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

    // -- Cost estimation --

    #[test]
    fn estimate_cost_known_anthropic_model() {
        // claude-sonnet-4-5: $3.00/$15.00 per 1M tokens
        let cost = estimate_cost_usd("claude-sonnet-4-5-20241022", Some(1000), Some(500), None);
        let expected = (1000.0 * 3.0 + 500.0 * 15.0) / 1_000_000.0;
        assert!((cost.unwrap() - expected).abs() < 1e-9, "cost={:?}", cost);
    }

    #[test]
    fn estimate_cost_with_cached_tokens_reduces_input_cost() {
        // claude-sonnet: $3.00/$15.00 input/output, cached input = $0.30 (10%)
        let cost_no_cache =
            estimate_cost_usd("claude-sonnet-4-5-20241022", Some(1000), Some(500), None);
        // 800 cached of 1000 prompt — cached at 10%, uncached at 100%
        let cost_cached = estimate_cost_usd(
            "claude-sonnet-4-5-20241022",
            Some(1000),
            Some(500),
            Some(800),
        );
        assert!(
            cost_cached.unwrap() < cost_no_cache.unwrap(),
            "cached should be cheaper"
        );
    }

    #[test]
    fn estimate_cost_unknown_model_returns_none() {
        let cost = estimate_cost_usd("unknown-model-xyz", Some(1000), Some(500), None);
        assert!(cost.is_none());
    }

    // ── assemble_block_context ────────────────────────────────────────

    fn make_outcome(
        cmd: &str,
        cwd: &str,
        exit_code: Option<i32>,
        output: &str,
        duration_ms: u64,
    ) -> crate::ai_agent::knowledge::CommandOutcome {
        crate::ai_agent::knowledge::CommandOutcome {
            timestamp: 0,
            command: cmd.into(),
            cwd: cwd.into(),
            exit_code,
            output_snippet: output.into(),
            classification: crate::ai_agent::knowledge::OutcomeClass::Success,
            duration_ms,
            id: 0,
        }
    }

    #[test]
    fn block_context_empty_when_no_commands() {
        let k = crate::ai_agent::knowledge::SessionKnowledge::new();
        assert!(assemble_block_context(&k, 16_000).is_none());
    }

    #[test]
    fn block_context_contains_command_fields() {
        let mut k = crate::ai_agent::knowledge::SessionKnowledge::new();
        k.commands.push_back(make_outcome(
            "cargo test",
            "/repo",
            Some(0),
            "test passed",
            120,
        ));
        let out = assemble_block_context(&k, 16_000).unwrap();
        assert!(out.contains("[cmd: cargo test]"));
        assert!(out.contains("[cwd: /repo]"));
        assert!(out.contains("[exit: 0]"));
        assert!(out.contains("[duration: 120ms]"));
        assert!(out.contains("test passed"));
    }

    #[test]
    fn block_context_no_exit_code_shows_question_mark() {
        let mut k = crate::ai_agent::knowledge::SessionKnowledge::new();
        k.commands
            .push_back(make_outcome("sleep 10", "/", None, "", 0));
        let out = assemble_block_context(&k, 16_000).unwrap();
        assert!(out.contains("[exit: ?]"));
    }

    #[test]
    fn block_context_most_recent_last_in_output() {
        let mut k = crate::ai_agent::knowledge::SessionKnowledge::new();
        k.commands
            .push_back(make_outcome("first", "/", Some(0), "", 1));
        k.commands
            .push_back(make_outcome("second", "/", Some(0), "", 2));
        let out = assemble_block_context(&k, 16_000).unwrap();
        let pos_first = out.find("first").unwrap();
        let pos_second = out.find("second").unwrap();
        assert!(
            pos_first < pos_second,
            "oldest command should appear before newest"
        );
    }

    #[test]
    fn block_context_respects_budget() {
        let mut k = crate::ai_agent::knowledge::SessionKnowledge::new();
        // Add many large blocks
        for i in 0..50 {
            k.commands.push_back(make_outcome(
                &format!("cmd-{i}"),
                "/",
                Some(0),
                &"x".repeat(500),
                10,
            ));
        }
        let budget = 5_000;
        let out = assemble_block_context(&k, budget).unwrap();
        assert!(out.len() <= budget + 200, "output should stay near budget");
        // Most recent commands should be present
        assert!(out.contains("cmd-49"));
    }

    #[test]
    fn block_context_empty_snippet_omits_code_block() {
        let mut k = crate::ai_agent::knowledge::SessionKnowledge::new();
        k.commands
            .push_back(make_outcome("ls", "/", Some(0), "", 5));
        let out = assemble_block_context(&k, 16_000).unwrap();
        assert!(!out.contains("```"), "no code fence for empty output");
    }
}
