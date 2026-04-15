//! AI Chat backend — config, keyring, and Ollama detection.
//!
//! Separate from `llm_api.rs` (Smart Prompts) so Chat and Smart Prompts
//! can use different providers/models independently.
//! Config stored in `ai-chat.json`; API key in OS keyring under a
//! distinct service name.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::{load_json_config, save_json_config};
use crate::llm_api;

const CONFIG_FILE: &str = "ai-chat.json";
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
                return Some(url.clone());
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

fn read_api_key() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
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

/// Quick connection test: send a minimal prompt and verify a response comes back.
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

    // Reuse llm_api's build_client — same genai crate, same pattern
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

    Ok(format!("Connection successful — model replied: {text}"))
}

// ---------------------------------------------------------------------------
// Conversation persistence
// ---------------------------------------------------------------------------

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    #[serde(default)]
    pub timestamp: u64, // unix millis
}

/// Metadata for a saved conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConversationMeta {
    pub id: String,
    pub title: String,
    /// Session ID of the attached terminal (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub created: u64,  // unix millis
    pub updated: u64,  // unix millis
    pub message_count: usize,
    /// Provider + model used
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

/// A full conversation with messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Conversation {
    pub meta: ConversationMeta,
    pub messages: Vec<ChatMessage>,
}

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
    let dir = conversations_dir()?;
    let mut metas = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read conversations dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(conv) = serde_json::from_str::<Conversation>(&data) {
                    metas.push(conv.meta);
                }
            }
        }
    }
    metas.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(metas)
}

#[tauri::command]
pub(crate) fn load_conversation(id: String) -> Result<Conversation, String> {
    let dir = conversations_dir()?;
    let path = dir.join(format!("{id}.json"));
    let data = std::fs::read_to_string(&path)
        .map_err(|_| format!("Conversation not found: {id}"))?;
    serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse conversation: {e}"))
}

#[tauri::command]
pub(crate) fn save_conversation(conversation: Conversation) -> Result<(), String> {
    let dir = conversations_dir()?;
    let path = dir.join(format!("{}.json", conversation.meta.id));
    let data = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize conversation: {e}"))?;
    std::fs::write(&path, data)
        .map_err(|e| format!("Failed to write conversation: {e}"))
}

#[tauri::command]
pub(crate) fn delete_conversation(id: String) -> Result<(), String> {
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
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "explain this error".to_string(),
            timestamp: 1713200000000,
        };
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
                ChatMessage {
                    role: "user".to_string(),
                    content: "why is CI failing?".to_string(),
                    timestamp: 1713200000000,
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: "The test suite has a flaky assertion...".to_string(),
                    timestamp: 1713200001000,
                },
            ],
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
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
                timestamp: now_millis(),
            }],
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
}
