use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{load_json_config, save_json_config};

const CONFIG_FILE: &str = "providers.json";
const SCHEMA_VERSION: u32 = 3;

// ---------------------------------------------------------------------------
// Provider types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderType {
    Anthropic,
    OpenAi,
    Gemini,
    DeepSeek,
    Mistral,
    Fireworks,
    SambaNova,
    Moonshot,
    Xai,
    Zai,
    OpenRouter,
    Requesty,
    LiteLlm,
    Ollama,
    LmStudio,
    Bedrock,
    Vertex,
    Custom,
}

impl ProviderType {
    pub(crate) fn default_base_url(&self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("http://localhost:11434/v1/"),
            Self::LmStudio => Some("http://localhost:1234/v1/"),
            Self::OpenRouter => Some("https://openrouter.ai/api/v1/"),
            Self::DeepSeek => Some("https://api.deepseek.com/v1/"),
            Self::Mistral => Some("https://api.mistral.ai/v1/"),
            Self::Fireworks => Some("https://api.fireworks.ai/inference/v1/"),
            Self::SambaNova => Some("https://api.sambanova.ai/v1/"),
            Self::Moonshot => Some("https://api.moonshot.cn/v1/"),
            Self::Xai => Some("https://api.x.ai/v1/"),
            Self::Zai => Some("https://open.bigmodel.cn/api/paas/v4/"),
            Self::Requesty => Some("https://router.requesty.ai/v1/"),
            Self::LiteLlm => Some("http://localhost:4000/v1/"),
            Self::Anthropic | Self::OpenAi | Self::Gemini | Self::Bedrock | Self::Vertex | Self::Custom => None,
        }
    }

    pub(crate) fn needs_api_key(&self) -> bool {
        !matches!(self, Self::Ollama | Self::LmStudio | Self::LiteLlm)
    }

    #[allow(dead_code)] // Wired in story 1481 (Ollama detection + Tauri commands)
    pub(crate) fn uses_custom_url_routing(&self) -> bool {
        self.default_base_url().is_some() || matches!(self, Self::Custom)
    }
}

// ---------------------------------------------------------------------------
// Registry types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelTier {
    Economic,
    Standard,
    Premium,
}

fn default_tier() -> ModelTier {
    ModelTier::Standard
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelEntry {
    pub id: String,
    pub provider_id: String,
    pub model_name: String,
    #[serde(default = "default_tier")]
    pub tier: ModelTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SlotName {
    Main,
    Triage,
    Headless,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct Features {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderRegistry {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub providers: Vec<ProviderEntry>,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub slots: HashMap<SlotName, String>,
    /// Per-phase model overrides for the Main slot.
    /// Maps ToolPhase (search/read/write/plan) → model_id from the models list.
    /// Phases without an entry fall back to the Main slot model.
    #[serde(default)]
    pub phase_overrides: HashMap<crate::ai_agent::engine::ToolPhase, String>,
    #[serde(default)]
    pub features: Features,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            providers: vec![],
            models: vec![],
            slots: HashMap::new(),
            phase_overrides: HashMap::new(),
            features: Features::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Load / Save
// ---------------------------------------------------------------------------

pub(crate) fn load_registry() -> ProviderRegistry {
    let path = crate::config::config_dir().join(CONFIG_FILE);
    if !path.exists() {
        return migrate_from_legacy();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "Could not read config: {e}");
            return ProviderRegistry::default();
        }
    };
    let mut json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(path = %path.display(), "Corrupt config: {e}. Using defaults.");
            return ProviderRegistry::default();
        }
    };
    let version = json.get("schema_version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    if version < 2 {
        migrate_slots_v1_to_v2(&mut json);
    }
    if version < 3 {
        migrate_slots_v2_to_v3(&mut json);
    }
    match serde_json::from_value::<ProviderRegistry>(json) {
        Ok(mut reg) => {
            reg.schema_version = SCHEMA_VERSION;
            let _ = save_registry(&reg);
            reg
        }
        Err(e) => {
            tracing::error!("Failed to deserialize after migration: {e}. Using defaults.");
            ProviderRegistry::default()
        }
    }
}

fn migrate_slots_v1_to_v2(json: &mut serde_json::Value) {
    let slots = match json.get_mut("slots").and_then(|v| v.as_object_mut()) {
        Some(m) => m,
        None => return,
    };
    let renames = [
        ("agent_default", "agent_mid"),
        ("agent_search", "agent_low"),
        ("agent_read", "agent_low"),
        ("agent_write", "agent_high"),
    ];
    for (old, new) in renames {
        if let Some(val) = slots.remove(old) {
            slots.entry(new).or_insert(val);
        }
    }
    tracing::info!("Migrated providers.json slots from v1 to v2");
}

/// Migrate from 5-slot schema (chat/agent_mid/agent_low/agent_high/headless) to
/// 3-slot schema (main/triage/headless). agent_low/agent_high move to phase_overrides.
fn migrate_slots_v2_to_v3(json: &mut serde_json::Value) {
    let slots = match json.get_mut("slots").and_then(|v| v.as_object_mut()) {
        Some(m) => m,
        None => return,
    };

    // chat and agent_mid both map to main; prefer an explicitly-set main
    for old in ["chat", "agent_mid"] {
        if let Some(val) = slots.remove(old) {
            slots.entry("main").or_insert(val);
        }
    }

    // agent_low → phase_overrides.search + phase_overrides.read
    // agent_high → phase_overrides.write
    let low = slots.remove("agent_low");
    let high = slots.remove("agent_high");

    if low.is_some() || high.is_some() {
        let overrides = json
            .as_object_mut()
            .unwrap()
            .entry("phase_overrides")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .unwrap();

        if let Some(low_model) = low {
            overrides.entry("search").or_insert(low_model.clone());
            overrides.entry("read").or_insert(low_model);
        }
        if let Some(high_model) = high {
            overrides.entry("write").or_insert(high_model);
        }
    }

    tracing::info!("Migrated providers.json slots from v2 to v3 (3-slot schema)");
}

fn infer_provider_type(provider_str: &str) -> ProviderType {
    match provider_str {
        "anthropic" => ProviderType::Anthropic,
        "openai" => ProviderType::OpenAi,
        "gemini" => ProviderType::Gemini,
        "ollama" => ProviderType::Ollama,
        "openrouter" => ProviderType::OpenRouter,
        "deepseek" => ProviderType::DeepSeek,
        "mistral" => ProviderType::Mistral,
        _ => ProviderType::Custom,
    }
}

fn migrate_from_legacy() -> ProviderRegistry {
    use crate::ai_agent::engine::ToolPhase;

    let mut reg = ProviderRegistry::default();
    let chat_cfg: crate::ai_chat::AiChatConfig = load_json_config(crate::ai_chat::CONFIG_FILE);
    let llm_cfg: crate::llm_api::LlmApiConfig = load_json_config("llm-api.json");

    if chat_cfg.is_configured() {
        let ptype = infer_provider_type(&chat_cfg.provider);
        let provider_id = format!("{}-chat", chat_cfg.provider);

        reg.providers.push(ProviderEntry {
            id: provider_id.clone(),
            provider_type: ptype,
            label: format!("{} (migrated)", chat_cfg.provider),
            base_url: chat_cfg.base_url.clone(),
        });

        let model_id = format!("chat-{}", chat_cfg.model.replace(['/', '.'], "-"));
        reg.models.push(ModelEntry {
            id: model_id.clone(),
            provider_id: provider_id.clone(),
            model_name: chat_cfg.model.clone(),
            tier: ModelTier::Standard,
        });

        reg.slots.insert(SlotName::Main, model_id.clone());

        if let Ok(Some(key)) = crate::credentials::get(crate::credentials::Credential::AiChatApiKey) {
            let _ = crate::credentials::set(crate::credentials::Credential::Provider(&provider_id), &key);
        }

        if let Some(overrides) = &chat_cfg.agent_model_overrides {
            for (phase, override_model) in overrides {
                let override_id = format!("agent-{}", override_model.replace(['/', '.'], "-"));
                if !reg.models.iter().any(|m| m.model_name == *override_model) {
                    reg.models.push(ModelEntry {
                        id: override_id.clone(),
                        provider_id: provider_id.clone(),
                        model_name: override_model.clone(),
                        tier: ModelTier::Standard,
                    });
                }
                let mid = reg.models.iter().find(|m| m.model_name == *override_model)
                    .map(|m| m.id.clone()).unwrap_or(override_id);
                match phase {
                    ToolPhase::Search => { reg.phase_overrides.insert(ToolPhase::Search, mid.clone()); reg.phase_overrides.insert(ToolPhase::Read, mid); }
                    ToolPhase::Read => { reg.phase_overrides.insert(ToolPhase::Read, mid); }
                    ToolPhase::Write => { reg.phase_overrides.insert(ToolPhase::Write, mid); }
                    ToolPhase::Plan => {} // plan uses main slot
                }
            }
        }

    }

    if llm_cfg.is_configured() {
        let same_provider = chat_cfg.is_configured()
            && chat_cfg.provider == llm_cfg.provider
            && chat_cfg.effective_base_url() == llm_cfg.base_url;

        if same_provider {
            let existing_provider_id = format!("{}-chat", chat_cfg.provider);
            let headless_model_id = format!("headless-{}", llm_cfg.model.replace(['/', '.'], "-"));
            if !reg.models.iter().any(|m| m.model_name == llm_cfg.model) {
                reg.models.push(ModelEntry {
                    id: headless_model_id.clone(),
                    provider_id: existing_provider_id,
                    model_name: llm_cfg.model.clone(),
                    tier: ModelTier::Standard,
                });
            }
            let mid = reg.models.iter().find(|m| m.model_name == llm_cfg.model)
                .map(|m| m.id.clone()).unwrap_or(headless_model_id);
            reg.slots.insert(SlotName::Headless, mid);
        } else {
            let ptype = infer_provider_type(&llm_cfg.provider);
            let provider_id = format!("{}-headless", llm_cfg.provider);

            reg.providers.push(ProviderEntry {
                id: provider_id.clone(),
                provider_type: ptype,
                label: format!("{} (headless, migrated)", llm_cfg.provider),
                base_url: llm_cfg.base_url.clone(),
            });

            let model_id = format!("headless-{}", llm_cfg.model.replace(['/', '.'], "-"));
            reg.models.push(ModelEntry {
                id: model_id.clone(),
                provider_id: provider_id.clone(),
                model_name: llm_cfg.model.clone(),
                tier: ModelTier::Standard,
            });

            reg.slots.insert(SlotName::Headless, model_id);

            if let Ok(Some(key)) = crate::credentials::get(crate::credentials::Credential::LlmApiKey) {
                let _ = crate::credentials::set(crate::credentials::Credential::Provider(&provider_id), &key);
            }
        }
    }

    let _ = save_registry(&reg);
    reg
}

pub(crate) fn save_registry(registry: &ProviderRegistry) -> Result<(), String> {
    save_json_config(CONFIG_FILE, registry)
}

// ---------------------------------------------------------------------------
// Slot resolver
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct ResolvedSlot {
    pub config: crate::llm_api::LlmApiConfig,
    pub api_key: String,
    #[allow(dead_code)] // Wired in story 1481 (Tauri commands expose provider_type to UI)
    pub provider_type: ProviderType,
}

pub(crate) fn resolve_slot(
    registry: &ProviderRegistry,
    slot: SlotName,
) -> Result<ResolvedSlot, String> {
    let model_id = registry
        .slots
        .get(&slot)
        .ok_or_else(|| format!("No model configured for slot {slot:?}"))?;

    let model = registry
        .models
        .iter()
        .find(|m| &m.id == model_id)
        .ok_or_else(|| format!("Model '{model_id}' not found in registry"))?;

    let provider = registry
        .providers
        .iter()
        .find(|p| p.id == model.provider_id)
        .ok_or_else(|| {
            format!(
                "Provider '{}' not found for model '{}'",
                model.provider_id, model.id
            )
        })?;

    let base_url = provider
        .base_url
        .clone()
        .or_else(|| provider.provider_type.default_base_url().map(String::from));
    let base_url = base_url.map(|u| if u.ends_with('/') { u } else { format!("{u}/") });

    let api_key = crate::credentials::get(crate::credentials::Credential::Provider(&provider.id))?
        .unwrap_or_else(|| {
            if provider.provider_type.needs_api_key() {
                String::new()
            } else {
                "local".into()
            }
        });

    Ok(ResolvedSlot {
        config: crate::llm_api::LlmApiConfig {
            provider: format!("{:?}", provider.provider_type).to_lowercase(),
            model: model.model_name.clone(),
            base_url,
        },
        api_key,
        provider_type: provider.provider_type,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_provider_registry() -> ProviderRegistry {
    load_registry()
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_provider_registry(registry: ProviderRegistry) -> Result<(), String> {
    save_registry(&registry)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_provider_api_key_exists(provider_id: String) -> Result<bool, String> {
    crate::credentials::get(crate::credentials::Credential::Provider(&provider_id))
        .map(|v| v.is_some())
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_provider_api_key(provider_id: String, key: String) -> Result<(), String> {
    if key.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    crate::credentials::set(crate::credentials::Credential::Provider(&provider_id), &key)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn delete_provider_api_key(provider_id: String) -> Result<(), String> {
    crate::credentials::delete(crate::credentials::Credential::Provider(&provider_id))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn test_slot_connection(slot: SlotName) -> Result<String, String> {
    use genai::chat::{ChatMessage, ChatRequest};

    let registry = load_registry();
    let resolved = resolve_slot(&registry, slot)?;
    let client = crate::llm_api::build_client(&resolved.config, &resolved.api_key);

    let chat_req = ChatRequest::default()
        .with_system("Reply with exactly: OK")
        .append_message(ChatMessage::user("Test connection"));

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client.exec_chat(&resolved.config.model, chat_req, None),
    )
    .await
    .map_err(|_| "Connection timed out after 15s".to_string())?
    .map_err(|e| format!("Connection failed: {e}"))?;

    let text = result
        .first_text()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Ok(format!("Connection successful — model replied: {text}"))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn check_ollama_models(provider_id: String) -> crate::ai_chat::OllamaStatus {
    let registry = load_registry();
    let base = registry
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .and_then(|p| p.base_url.as_deref().map(String::from))
        .or_else(|| ProviderType::Ollama.default_base_url().map(String::from))
        .unwrap_or_else(|| "http://localhost:11434/v1/".to_string());
    crate::ai_chat::detect_ollama(&base).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_schema_version() {
        let reg = ProviderRegistry::default();
        assert_eq!(reg.schema_version, SCHEMA_VERSION);
        assert!(reg.providers.is_empty());
        assert!(reg.models.is_empty());
        assert!(reg.slots.is_empty());
    }

    #[test]
    fn serde_round_trip_empty_registry() {
        let reg = ProviderRegistry::default();
        let json = serde_json::to_string_pretty(&reg).unwrap();
        let loaded: ProviderRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert!(loaded.providers.is_empty());
        assert!(loaded.models.is_empty());
        assert!(loaded.slots.is_empty());
    }

    #[test]
    fn serde_round_trip_populated_registry() {
        use crate::ai_agent::engine::ToolPhase;
        let mut slots = HashMap::new();
        slots.insert(SlotName::Main, "sonnet-4".to_string());
        let mut phase_overrides = HashMap::new();
        phase_overrides.insert(ToolPhase::Search, "haiku".to_string());

        let reg = ProviderRegistry {
            schema_version: 1,
            providers: vec![
                ProviderEntry {
                    id: "anthropic-main".to_string(),
                    provider_type: ProviderType::Anthropic,
                    label: "Anthropic (personal)".to_string(),
                    base_url: None,
                },
                ProviderEntry {
                    id: "ollama-local".to_string(),
                    provider_type: ProviderType::Ollama,
                    label: "Ollama (local)".to_string(),
                    base_url: Some("http://localhost:11434/v1/".to_string()),
                },
            ],
            models: vec![
                ModelEntry {
                    id: "sonnet-4".to_string(),
                    provider_id: "anthropic-main".to_string(),
                    model_name: "claude-sonnet-4-5-20241022".to_string(),
                    tier: ModelTier::Standard,
                },
                ModelEntry {
                    id: "haiku".to_string(),
                    provider_id: "anthropic-main".to_string(),
                    model_name: "claude-haiku-4-5-20241022".to_string(),
                    tier: ModelTier::Economic,
                },
            ],
            slots,
            phase_overrides,
            features: Features {},
        };

        let json = serde_json::to_string_pretty(&reg).unwrap();
        let loaded: ProviderRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.providers.len(), 2);
        assert_eq!(loaded.providers[0].id, "anthropic-main");
        assert_eq!(loaded.providers[0].provider_type, ProviderType::Anthropic);
        assert!(loaded.providers[0].base_url.is_none());
        assert_eq!(loaded.providers[1].id, "ollama-local");
        assert_eq!(loaded.providers[1].provider_type, ProviderType::Ollama);
        assert_eq!(loaded.providers[1].base_url.as_deref(), Some("http://localhost:11434/v1/"));
        assert_eq!(loaded.models.len(), 2);
        assert_eq!(loaded.models[0].model_name, "claude-sonnet-4-5-20241022");
        assert_eq!(loaded.models[0].tier, ModelTier::Standard);
        assert_eq!(loaded.models[1].tier, ModelTier::Economic);
        assert_eq!(loaded.slots.get(&SlotName::Main), Some(&"sonnet-4".to_string()));
        assert_eq!(loaded.phase_overrides.get(&ToolPhase::Search), Some(&"haiku".to_string()));
    }

    #[test]
    fn serde_missing_fields_use_defaults() {
        let json = "{}";
        let loaded: ProviderRegistry = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert!(loaded.providers.is_empty());
        assert!(loaded.models.is_empty());
        assert!(loaded.slots.is_empty());
    }

    #[test]
    fn provider_type_all_variants_serde() {
        let variants = [
            (ProviderType::Anthropic, "anthropic"),
            (ProviderType::OpenAi, "open_ai"),
            (ProviderType::Gemini, "gemini"),
            (ProviderType::DeepSeek, "deep_seek"),
            (ProviderType::Mistral, "mistral"),
            (ProviderType::Fireworks, "fireworks"),
            (ProviderType::SambaNova, "samba_nova"),
            (ProviderType::Moonshot, "moonshot"),
            (ProviderType::Xai, "xai"),
            (ProviderType::Zai, "zai"),
            (ProviderType::OpenRouter, "open_router"),
            (ProviderType::Requesty, "requesty"),
            (ProviderType::LiteLlm, "lite_llm"),
            (ProviderType::Ollama, "ollama"),
            (ProviderType::LmStudio, "lm_studio"),
            (ProviderType::Bedrock, "bedrock"),
            (ProviderType::Vertex, "vertex"),
            (ProviderType::Custom, "custom"),
        ];
        assert_eq!(variants.len(), 18, "All 18 provider types must be covered");

        for (variant, expected_str) in &variants {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, format!("\"{}\"", expected_str), "serialize {:?}", variant);
            let back: ProviderType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, variant, "deserialize {}", expected_str);
        }
    }

    #[test]
    fn provider_type_default_base_urls() {
        assert_eq!(ProviderType::Ollama.default_base_url(), Some("http://localhost:11434/v1/"));
        assert_eq!(ProviderType::LmStudio.default_base_url(), Some("http://localhost:1234/v1/"));
        assert_eq!(ProviderType::OpenRouter.default_base_url(), Some("https://openrouter.ai/api/v1/"));
        assert_eq!(ProviderType::DeepSeek.default_base_url(), Some("https://api.deepseek.com/v1/"));
        assert_eq!(ProviderType::Mistral.default_base_url(), Some("https://api.mistral.ai/v1/"));
        assert_eq!(ProviderType::Fireworks.default_base_url(), Some("https://api.fireworks.ai/inference/v1/"));
        assert_eq!(ProviderType::SambaNova.default_base_url(), Some("https://api.sambanova.ai/v1/"));
        assert_eq!(ProviderType::Moonshot.default_base_url(), Some("https://api.moonshot.cn/v1/"));
        assert_eq!(ProviderType::Xai.default_base_url(), Some("https://api.x.ai/v1/"));
        assert_eq!(ProviderType::Zai.default_base_url(), Some("https://open.bigmodel.cn/api/paas/v4/"));
        assert_eq!(ProviderType::Requesty.default_base_url(), Some("https://router.requesty.ai/v1/"));
        assert_eq!(ProviderType::LiteLlm.default_base_url(), Some("http://localhost:4000/v1/"));
        assert!(ProviderType::Anthropic.default_base_url().is_none());
        assert!(ProviderType::OpenAi.default_base_url().is_none());
        assert!(ProviderType::Gemini.default_base_url().is_none());
        assert!(ProviderType::Bedrock.default_base_url().is_none());
        assert!(ProviderType::Vertex.default_base_url().is_none());
        assert!(ProviderType::Custom.default_base_url().is_none());
    }

    #[test]
    fn provider_type_needs_api_key() {
        assert!(!ProviderType::Ollama.needs_api_key());
        assert!(!ProviderType::LmStudio.needs_api_key());
        assert!(!ProviderType::LiteLlm.needs_api_key());
        assert!(ProviderType::Anthropic.needs_api_key());
        assert!(ProviderType::OpenAi.needs_api_key());
        assert!(ProviderType::Gemini.needs_api_key());
        assert!(ProviderType::OpenRouter.needs_api_key());
        assert!(ProviderType::Custom.needs_api_key());
        assert!(ProviderType::Bedrock.needs_api_key());
    }

    #[test]
    fn provider_type_uses_custom_url_routing() {
        assert!(ProviderType::Ollama.uses_custom_url_routing());
        assert!(ProviderType::LmStudio.uses_custom_url_routing());
        assert!(ProviderType::OpenRouter.uses_custom_url_routing());
        assert!(ProviderType::Custom.uses_custom_url_routing());
        assert!(!ProviderType::Anthropic.uses_custom_url_routing());
        assert!(!ProviderType::OpenAi.uses_custom_url_routing());
        assert!(!ProviderType::Gemini.uses_custom_url_routing());
    }

    #[test]
    fn slot_name_all_variants_serde() {
        let variants = [
            (SlotName::Main, "main"),
            (SlotName::Triage, "triage"),
            (SlotName::Headless, "headless"),
        ];
        assert_eq!(variants.len(), 3, "All 3 slot names must be covered");

        for (variant, expected_str) in &variants {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, format!("\"{}\"", expected_str));
            let back: SlotName = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, variant);
        }
    }

    #[test]
    fn model_tier_serde() {
        let variants = [
            (ModelTier::Economic, "economic"),
            (ModelTier::Standard, "standard"),
            (ModelTier::Premium, "premium"),
        ];
        for (variant, expected_str) in &variants {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, format!("\"{}\"", expected_str));
            let back: ModelTier = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, variant);
        }
    }

    #[test]
    fn model_entry_default_tier() {
        let json = r#"{"id":"m1","provider_id":"p1","model_name":"gpt-4o"}"#;
        let model: ModelEntry = serde_json::from_str(json).unwrap();
        assert_eq!(model.tier, ModelTier::Standard);
    }

    #[test]
    fn provider_entry_base_url_skipped_when_none() {
        let entry = ProviderEntry {
            id: "test".to_string(),
            provider_type: ProviderType::Anthropic,
            label: "Test".to_string(),
            base_url: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("base_url"));
    }

    #[test]
    fn slot_names_usable_as_hash_keys() {
        let mut map = HashMap::new();
        map.insert(SlotName::Main, "m1".to_string());
        map.insert(SlotName::Triage, "m2".to_string());
        assert_eq!(map.get(&SlotName::Main), Some(&"m1".to_string()));
        assert_eq!(map.get(&SlotName::Triage), Some(&"m2".to_string()));
        assert!(map.get(&SlotName::Headless).is_none());
    }

    #[test]
    #[serial_test::serial]
    fn load_registry_returns_default_for_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let reg = load_registry();
        assert_eq!(reg.schema_version, SCHEMA_VERSION);
        assert!(reg.providers.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn save_and_load_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let mut slots = HashMap::new();
        slots.insert(SlotName::Main, "m1".to_string());

        let reg = ProviderRegistry {
            schema_version: 1,
            providers: vec![ProviderEntry {
                id: "p1".to_string(),
                provider_type: ProviderType::OpenAi,
                label: "OpenAI".to_string(),
                base_url: None,
            }],
            models: vec![ModelEntry {
                id: "m1".to_string(),
                provider_id: "p1".to_string(),
                model_name: "gpt-4o".to_string(),
                tier: ModelTier::Premium,
            }],
            slots,
            phase_overrides: HashMap::new(),
            features: Features {},
        };

        save_registry(&reg).unwrap();
        let loaded = load_registry();

        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "p1");
        assert_eq!(loaded.models.len(), 1);
        assert_eq!(loaded.models[0].model_name, "gpt-4o");
        assert_eq!(loaded.models[0].tier, ModelTier::Premium);
        assert_eq!(loaded.slots.get(&SlotName::Main), Some(&"m1".to_string()));
    }

    // -- resolve_slot tests --

    fn test_registry() -> ProviderRegistry {
        use crate::ai_agent::engine::ToolPhase;
        let mut slots = HashMap::new();
        slots.insert(SlotName::Main, "sonnet".to_string());
        slots.insert(SlotName::Headless, "gpt4o".to_string());
        let mut phase_overrides = HashMap::new();
        phase_overrides.insert(ToolPhase::Search, "haiku".to_string());
        phase_overrides.insert(ToolPhase::Read, "haiku".to_string());
        phase_overrides.insert(ToolPhase::Write, "gpt4o".to_string());

        ProviderRegistry {
            schema_version: 1,
            providers: vec![
                ProviderEntry {
                    id: "anthropic-main".to_string(),
                    provider_type: ProviderType::Anthropic,
                    label: "Anthropic".to_string(),
                    base_url: None,
                },
                ProviderEntry {
                    id: "openai-main".to_string(),
                    provider_type: ProviderType::OpenAi,
                    label: "OpenAI".to_string(),
                    base_url: None,
                },
                ProviderEntry {
                    id: "ollama-local".to_string(),
                    provider_type: ProviderType::Ollama,
                    label: "Ollama".to_string(),
                    base_url: None,
                },
            ],
            models: vec![
                ModelEntry {
                    id: "sonnet".to_string(),
                    provider_id: "anthropic-main".to_string(),
                    model_name: "claude-sonnet-4-5-20241022".to_string(),
                    tier: ModelTier::Standard,
                },
                ModelEntry {
                    id: "haiku".to_string(),
                    provider_id: "anthropic-main".to_string(),
                    model_name: "claude-haiku-4-5-20241022".to_string(),
                    tier: ModelTier::Economic,
                },
                ModelEntry {
                    id: "gpt4o".to_string(),
                    provider_id: "openai-main".to_string(),
                    model_name: "gpt-4o".to_string(),
                    tier: ModelTier::Premium,
                },
                ModelEntry {
                    id: "llama".to_string(),
                    provider_id: "ollama-local".to_string(),
                    model_name: "llama3.2".to_string(),
                    tier: ModelTier::Standard,
                },
            ],
            slots,
            phase_overrides,
            features: Features::default(),
        }
    }

    #[test]
    #[serial_test::serial]
    fn resolve_slot_main() {
        crate::credentials::set(
            crate::credentials::Credential::Provider("anthropic-main"),
            "sk-ant-test",
        ).unwrap();

        let reg = test_registry();
        let resolved = resolve_slot(&reg, SlotName::Main).unwrap();
        assert_eq!(resolved.config.model, "claude-sonnet-4-5-20241022");
        assert_eq!(resolved.api_key, "sk-ant-test");
        assert_eq!(resolved.provider_type, ProviderType::Anthropic);
        assert!(resolved.config.base_url.is_none());
    }

    #[test]
    fn phase_overrides_in_registry() {
        use crate::ai_agent::engine::ToolPhase;
        let reg = test_registry();
        assert_eq!(reg.phase_overrides.get(&ToolPhase::Search), Some(&"haiku".to_string()));
        assert_eq!(reg.phase_overrides.get(&ToolPhase::Write), Some(&"gpt4o".to_string()));
        assert!(reg.phase_overrides.get(&ToolPhase::Plan).is_none());
    }

    #[test]
    #[serial_test::serial]
    fn resolve_slot_ollama_gets_default_base_url() {
        let mut reg = test_registry();
        reg.slots.insert(SlotName::Headless, "llama".to_string());

        let resolved = resolve_slot(&reg, SlotName::Headless).unwrap();
        assert_eq!(resolved.config.model, "llama3.2");
        assert_eq!(resolved.config.base_url.as_deref(), Some("http://localhost:11434/v1/"));
        assert_eq!(resolved.api_key, "local");
        assert_eq!(resolved.provider_type, ProviderType::Ollama);
    }

    #[test]
    fn resolve_slot_error_no_slot_configured() {
        let mut reg = test_registry();
        reg.slots.remove(&SlotName::Headless);
        let result = resolve_slot(&reg, SlotName::Headless);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No model configured"));
    }

    #[test]
    fn resolve_slot_error_dangling_model_ref() {
        let mut reg = test_registry();
        reg.slots.insert(SlotName::Headless, "nonexistent".to_string());

        let result = resolve_slot(&reg, SlotName::Headless);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found in registry"));
    }

    #[test]
    fn resolve_slot_error_dangling_provider_ref() {
        let mut reg = test_registry();
        reg.models.push(ModelEntry {
            id: "orphan".to_string(),
            provider_id: "deleted-provider".to_string(),
            model_name: "orphan-model".to_string(),
            tier: ModelTier::Standard,
        });
        reg.slots.insert(SlotName::Headless, "orphan".to_string());

        let result = resolve_slot(&reg, SlotName::Headless);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Provider 'deleted-provider' not found"));
    }

    #[test]
    fn resolve_slot_base_url_trailing_slash_normalization() {
        let mut reg = ProviderRegistry::default();
        reg.providers.push(ProviderEntry {
            id: "custom".to_string(),
            provider_type: ProviderType::Custom,
            label: "Custom".to_string(),
            base_url: Some("http://my-llm:8080/v1".to_string()),
        });
        reg.models.push(ModelEntry {
            id: "m1".to_string(),
            provider_id: "custom".to_string(),
            model_name: "my-model".to_string(),
            tier: ModelTier::Standard,
        });
        reg.slots.insert(SlotName::Main, "m1".to_string());

        let resolved = resolve_slot(&reg, SlotName::Main).unwrap();
        assert_eq!(resolved.config.base_url.as_deref(), Some("http://my-llm:8080/v1/"));
    }

    // -- migration tests --

    fn write_json(dir: &std::path::Path, filename: &str, value: &impl serde::Serialize) {
        let json = serde_json::to_string_pretty(value).unwrap();
        std::fs::write(dir.join(filename), json).unwrap();
    }

    #[test]
    #[serial_test::serial]
    fn migrate_empty_configs_produces_empty_registry() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let reg = load_registry();
        assert_eq!(reg.schema_version, SCHEMA_VERSION);
        assert!(reg.providers.is_empty());
        assert!(reg.models.is_empty());
        assert!(reg.slots.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn load_registry_migrates_v1_slot_keys_to_v3() {
        use crate::ai_agent::engine::ToolPhase;
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let v1_json = serde_json::json!({
            "schema_version": 1,
            "providers": [{
                "id": "anthropic-main",
                "type": "anthropic",
                "label": "Anthropic"
            }],
            "models": [{
                "id": "sonnet",
                "provider_id": "anthropic-main",
                "model_name": "claude-sonnet-4-5-20241022",
                "tier": "standard"
            }],
            "slots": {
                "chat": "sonnet",
                "agent_default": "sonnet",
                "agent_search": "sonnet",
                "agent_write": "sonnet"
            },
            "features": {}
        });
        let path = dir.path().join(CONFIG_FILE);
        std::fs::write(&path, serde_json::to_string_pretty(&v1_json).unwrap()).unwrap();

        let reg = load_registry();
        assert_eq!(reg.schema_version, SCHEMA_VERSION);
        // v1 chat/agent_default → v2 chat/agent_mid → v3 main
        assert_eq!(reg.slots.get(&SlotName::Main), Some(&"sonnet".to_string()));
        // v1 agent_search → v2 agent_low → v3 phase_overrides.search/read
        assert_eq!(reg.phase_overrides.get(&ToolPhase::Search), Some(&"sonnet".to_string()));
        // v1 agent_write → v2 agent_high → v3 phase_overrides.write
        assert_eq!(reg.phase_overrides.get(&ToolPhase::Write), Some(&"sonnet".to_string()));
        assert!(reg.slots.get(&SlotName::Headless).is_none());
    }

    #[test]
    fn migrate_slots_v1_to_v2_preserves_existing_new_keys() {
        let mut json = serde_json::json!({
            "slots": {
                "agent_default": "model-a",
                "agent_mid": "model-b"
            }
        });
        migrate_slots_v1_to_v2(&mut json);
        let slots = json["slots"].as_object().unwrap();
        assert_eq!(slots.get("agent_mid").unwrap(), "model-b");
        assert!(slots.get("agent_default").is_none());
    }

    #[test]
    fn migrate_slots_v1_to_v2_collapses_search_and_read_to_low() {
        let mut json = serde_json::json!({
            "slots": {
                "agent_search": "model-a",
                "agent_read": "model-b"
            }
        });
        migrate_slots_v1_to_v2(&mut json);
        let slots = json["slots"].as_object().unwrap();
        assert_eq!(slots.get("agent_low").unwrap(), "model-a");
        assert!(slots.get("agent_search").is_none());
        assert!(slots.get("agent_read").is_none());
    }

    #[test]
    #[serial_test::serial]
    fn migrate_ai_chat_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // Write raw legacy JSON (provider/model are skip_serializing in current AiChatConfig)
        std::fs::write(
            dir.path().join("ai-chat.json"),
            r#"{"temperature":0.7,"provider":"anthropic","model":"claude-sonnet-4-5-20241022"}"#,
        ).unwrap();

        crate::credentials::set(crate::credentials::Credential::AiChatApiKey, "sk-ant-old").unwrap();

        let reg = load_registry();
        assert_eq!(reg.providers.len(), 1);
        assert_eq!(reg.providers[0].provider_type, ProviderType::Anthropic);
        assert_eq!(reg.providers[0].id, "anthropic-chat");
        assert_eq!(reg.models.len(), 1);
        assert_eq!(reg.models[0].model_name, "claude-sonnet-4-5-20241022");
        assert!(reg.slots.contains_key(&SlotName::Main));

        let migrated_key = crate::credentials::get(
            crate::credentials::Credential::Provider("anthropic-chat")
        ).unwrap();
        assert_eq!(migrated_key, Some("sk-ant-old".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn migrate_llm_api_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let llm = crate::llm_api::LlmApiConfig {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: None,
        };
        write_json(dir.path(), "llm-api.json", &llm);

        crate::credentials::set(crate::credentials::Credential::LlmApiKey, "sk-oai-old").unwrap();

        let reg = load_registry();
        assert_eq!(reg.providers.len(), 1);
        assert_eq!(reg.providers[0].provider_type, ProviderType::OpenAi);
        assert_eq!(reg.providers[0].id, "openai-headless");
        assert_eq!(reg.models.len(), 1);
        assert!(reg.slots.contains_key(&SlotName::Headless));
        assert!(!reg.slots.contains_key(&SlotName::Main));

        let migrated_key = crate::credentials::get(
            crate::credentials::Credential::Provider("openai-headless")
        ).unwrap();
        assert_eq!(migrated_key, Some("sk-oai-old".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn migrate_both_same_provider_deduplicates() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        std::fs::write(
            dir.path().join("ai-chat.json"),
            r#"{"temperature":0.7,"provider":"openai","model":"gpt-4o"}"#,
        ).unwrap();
        let llm = crate::llm_api::LlmApiConfig {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: None,
        };
        write_json(dir.path(), "llm-api.json", &llm);

        let reg = load_registry();
        // Same provider, different models — one provider, two models
        assert_eq!(reg.providers.len(), 1);
        assert_eq!(reg.models.len(), 2);
        assert!(reg.slots.contains_key(&SlotName::Main));
        assert!(reg.slots.contains_key(&SlotName::Headless));
    }

    #[test]
    #[serial_test::serial]
    fn migrate_both_different_providers() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        std::fs::write(
            dir.path().join("ai-chat.json"),
            r#"{"temperature":0.7,"provider":"anthropic","model":"claude-sonnet-4-5-20241022"}"#,
        ).unwrap();
        let llm = crate::llm_api::LlmApiConfig {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: None,
        };
        write_json(dir.path(), "llm-api.json", &llm);

        let reg = load_registry();
        assert_eq!(reg.providers.len(), 2);
        assert_eq!(reg.models.len(), 2);
        assert!(reg.slots.contains_key(&SlotName::Main));
        assert!(reg.slots.contains_key(&SlotName::Headless));
    }

    #[test]
    #[serial_test::serial]
    fn migrate_with_agent_model_overrides() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        std::fs::write(
            dir.path().join("ai-chat.json"),
            r#"{"temperature":0.7,"provider":"anthropic","model":"claude-sonnet-4-5-20241022","agent_model_overrides":{"search":"claude-haiku-4-5-20241022","write":"claude-sonnet-4-5-20241022"}}"#,
        ).unwrap();

        use crate::ai_agent::engine::ToolPhase;
        let reg = load_registry();
        // Search override → phase_overrides.search + phase_overrides.read
        assert!(reg.phase_overrides.contains_key(&ToolPhase::Search));
        assert!(reg.phase_overrides.contains_key(&ToolPhase::Read));
        // Write override uses the same model as default — phase_overrides.write set
        assert!(reg.phase_overrides.contains_key(&ToolPhase::Write));
        // Haiku model should be added
        assert!(reg.models.iter().any(|m| m.model_name == "claude-haiku-4-5-20241022"));
    }

    #[test]
    #[serial_test::serial]
    fn migrate_persists_providers_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        std::fs::write(
            dir.path().join("ai-chat.json"),
            r#"{"temperature":0.7,"provider":"anthropic","model":"claude-sonnet-4-5-20241022"}"#,
        ).unwrap();

        let _ = load_registry();
        // providers.json should now exist
        assert!(dir.path().join("providers.json").exists());
        // Second load should read from file, not migrate again
        let reg2 = load_registry();
        assert_eq!(reg2.providers.len(), 1);
    }
}
