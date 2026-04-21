//! LLM API integration for Smart Prompts "api" execution mode.
//!
//! Uses the `genai` crate for multi-provider chat completions.
//! API keys are stored in the platform-specific secure store (OS keyring).
//! Config (provider/model/base_url) stored in `llm-api.json`.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::{load_json_config, save_json_config};

const CONFIG_FILE: &str = "llm-api.json";
use crate::credentials::Credential;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct LlmApiConfig {
    /// Provider identifier: "openai", "anthropic", "gemini", "openrouter", "ollama", "custom"
    #[serde(default)]
    pub provider: String,
    /// Model name (e.g. "gpt-4o-mini", "claude-sonnet-4-5-20241022")
    #[serde(default)]
    pub model: String,
    /// Custom base URL — only used for openrouter, ollama, and custom providers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

impl LlmApiConfig {
    fn is_configured(&self) -> bool {
        !self.provider.is_empty() && !self.model.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Keyring helpers
// ---------------------------------------------------------------------------

fn read_api_key() -> Result<Option<String>, String> {
    crate::credentials::get(Credential::LlmApiKey)
}

fn save_api_key(key: &str) -> Result<(), String> {
    crate::credentials::set(Credential::LlmApiKey, key)
}

fn delete_api_key() -> Result<(), String> {
    crate::credentials::delete(Credential::LlmApiKey)
}

// ---------------------------------------------------------------------------
// genai client builder
// ---------------------------------------------------------------------------

pub(crate) fn build_client(config: &LlmApiConfig, api_key: &str) -> genai::Client {
    use genai::resolver::{AuthData, AuthResolver};

    let has_custom_url = config
        .base_url
        .as_ref()
        .is_some_and(|u| !u.is_empty());

    if has_custom_url {
        // Custom base URL (OpenRouter, Ollama, custom) — use ServiceTargetResolver
        // which overrides both endpoint and auth in one resolver.
        use genai::adapter::AdapterKind;
        use genai::resolver::{Endpoint, ServiceTargetResolver};
        use genai::ServiceTarget;
        use genai::ModelIden;

        let url = config.base_url.clone().unwrap();
        let key = api_key.to_string();
        let target_resolver = ServiceTargetResolver::from_resolver_fn(
            move |service_target: ServiceTarget| -> Result<ServiceTarget, genai::resolver::Error>
            {
                let ServiceTarget { model, .. } = service_target;
                let endpoint = Endpoint::from_owned(url.clone());
                let auth = AuthData::from_single(key.clone());
                // Route through the OpenAI adapter (works for OpenAI-compatible APIs)
                let model = ModelIden::new(AdapterKind::OpenAI, model.model_name);
                Ok(ServiceTarget {
                    endpoint,
                    auth,
                    model,
                })
            },
        );
        genai::Client::builder()
            .with_service_target_resolver(target_resolver)
            .build()
    } else {
        // Standard provider — genai auto-detects from model name prefix.
        // We just provide the API key via AuthResolver.
        let key = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(move |_model_iden| {
            Ok(Some(AuthData::from_single(key.clone())))
        });
        genai::Client::builder()
            .with_auth_resolver(auth_resolver)
            .build()
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn load_llm_api_config() -> LlmApiConfig {
    load_json_config(CONFIG_FILE)
}

#[tauri::command]
pub(crate) fn save_llm_api_config(config: LlmApiConfig) -> Result<(), String> {
    save_json_config(CONFIG_FILE, &config)
}

#[tauri::command]
pub(crate) fn has_llm_api_key() -> Result<bool, String> {
    read_api_key().map(|k| k.is_some())
}

#[tauri::command]
pub(crate) fn save_llm_api_key(key: String) -> Result<(), String> {
    if key.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    save_api_key(&key)
}

#[tauri::command]
pub(crate) fn delete_llm_api_key() -> Result<(), String> {
    delete_api_key()
}

/// Execute a Smart Prompt via direct LLM API call.
/// Returns the model's response text.
#[tauri::command]
pub(crate) async fn execute_api_prompt(
    system_prompt: Option<String>,
    content: String,
    timeout_ms: u64,
) -> Result<String, String> {
    use genai::chat::{ChatMessage, ChatRequest};

    if content.is_empty() {
        return Err("Prompt content must not be empty".to_string());
    }

    let config: LlmApiConfig = load_json_config(CONFIG_FILE);
    if !config.is_configured() {
        return Err("LLM API not configured — set provider and model in Settings > Agents".to_string());
    }

    let api_key = read_api_key()?
        .ok_or_else(|| "No API key stored — add one in Settings > Agents".to_string())?;

    let client = build_client(&config, &api_key);

    let mut chat_req = ChatRequest::default();
    if let Some(sys) = &system_prompt
        && !sys.is_empty()
    {
        chat_req = chat_req.with_system(sys.as_str());
    }
    chat_req = chat_req.append_message(ChatMessage::user(content));

    let duration = Duration::from_millis(timeout_ms.min(60_000));

    let result = tokio::time::timeout(duration, client.exec_chat(&config.model, chat_req, None))
        .await
        .map_err(|_| format!("Timed out after {}s", duration.as_secs()))?
        .map_err(map_genai_error)?;

    result
        .first_text()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Model returned an empty response".to_string())
}

/// Test the LLM API connection with a minimal completion.
#[tauri::command]
pub(crate) async fn test_llm_api() -> Result<String, String> {
    execute_api_prompt(
        Some("Reply with exactly: OK".to_string()),
        "Test connection".to_string(),
        15_000,
    )
    .await
    .map(|response| format!("Connection successful — model replied: {response}"))
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn map_genai_error(err: genai::Error) -> String {
    map_genai_error_str(&err.to_string())
}

fn map_genai_error_str(msg: &str) -> String {
    if msg.contains("401") || msg.contains("Unauthorized") || msg.contains("invalid_api_key") {
        "Authentication failed — check your API key in Settings > Agents".to_string()
    } else if msg.contains("429") || msg.contains("rate") {
        "Rate limit exceeded — wait a moment and try again".to_string()
    } else if msg.contains("404") || msg.contains("model_not_found") {
        format!("Model not found — check the model name in Settings > Agents. Error: {msg}")
    } else if msg.contains("timeout") || msg.contains("connect") {
        format!("Network error — check your connection. Error: {msg}")
    } else {
        format!("LLM API error: {msg}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_is_unconfigured() {
        let config = LlmApiConfig::default();
        assert!(!config.is_configured());
    }

    #[test]
    fn config_with_provider_and_model_is_configured() {
        let config = LlmApiConfig {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: None,
        };
        assert!(config.is_configured());
    }

    #[test]
    fn config_serde_round_trip() {
        let config = LlmApiConfig {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20241022".to_string(),
            base_url: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let loaded: LlmApiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.provider, "anthropic");
        assert_eq!(loaded.model, "claude-sonnet-4-5-20241022");
        assert!(loaded.base_url.is_none());
    }

    #[test]
    fn config_with_base_url_serde_round_trip() {
        let config = LlmApiConfig {
            provider: "openrouter".to_string(),
            model: "openai/gpt-4o-mini".to_string(),
            base_url: Some("https://openrouter.ai/api/v1/".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let loaded: LlmApiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.provider, "openrouter");
        assert_eq!(
            loaded.base_url,
            Some("https://openrouter.ai/api/v1/".to_string())
        );
    }

    #[test]
    fn missing_fields_use_defaults() {
        let loaded: LlmApiConfig = serde_json::from_str("{}").unwrap();
        assert!(!loaded.is_configured());
        assert!(loaded.provider.is_empty());
        assert!(loaded.model.is_empty());
        assert!(loaded.base_url.is_none());
    }

    #[test]
    fn base_url_skipped_when_none() {
        let config = LlmApiConfig {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            base_url: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("base_url"));
    }

    #[test]
    fn error_mapping_auth_keyword() {
        // Test that our error mapper recognizes auth-related error strings
        let msg = map_genai_error_str("401 Unauthorized");
        assert!(msg.contains("Authentication failed"));
    }

    #[test]
    fn error_mapping_rate_limit() {
        let msg = map_genai_error_str("429 rate limit exceeded");
        assert!(msg.contains("Rate limit"));
    }

    #[test]
    fn error_mapping_model_not_found() {
        let msg = map_genai_error_str("404 model_not_found");
        assert!(msg.contains("Model not found"));
    }
}
