use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{load_json_config, save_json_config};

const CONFIG_FILE: &str = "providers.json";
const SCHEMA_VERSION: u32 = 1;

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

#[allow(dead_code)] // Wired in story 1480 (consumers call resolve_slot)
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
    Chat,
    AgentDefault,
    AgentSearch,
    AgentRead,
    AgentWrite,
    Headless,
    Enrichment,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct Features {
    #[serde(default)]
    pub enrichment_enabled: bool,
}

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
    #[serde(default)]
    pub features: Features,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            providers: vec![],
            models: vec![],
            slots: HashMap::new(),
            features: Features::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Load / Save
// ---------------------------------------------------------------------------

pub(crate) fn load_registry() -> ProviderRegistry {
    load_json_config(CONFIG_FILE)
}

pub(crate) fn save_registry(registry: &ProviderRegistry) -> Result<(), String> {
    save_json_config(CONFIG_FILE, registry)
}

// ---------------------------------------------------------------------------
// Slot resolver
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[allow(dead_code)] // Wired in story 1480 (consumers call resolve_slot)
pub(crate) struct ResolvedSlot {
    pub config: crate::llm_api::LlmApiConfig,
    pub api_key: String,
    pub provider_type: ProviderType,
}

#[allow(dead_code)] // Wired in story 1480
pub(crate) fn resolve_slot(
    registry: &ProviderRegistry,
    slot: SlotName,
) -> Result<ResolvedSlot, String> {
    let model_id = registry
        .slots
        .get(&slot)
        .or_else(|| match slot {
            SlotName::AgentSearch | SlotName::AgentRead | SlotName::AgentWrite => {
                registry.slots.get(&SlotName::AgentDefault)
            }
            _ => None,
        })
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

#[tauri::command]
pub(crate) fn load_provider_registry() -> ProviderRegistry {
    load_registry()
}

#[tauri::command]
pub(crate) fn save_provider_registry(registry: ProviderRegistry) -> Result<(), String> {
    save_registry(&registry)
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
        assert_eq!(reg.schema_version, 1);
        assert!(reg.providers.is_empty());
        assert!(reg.models.is_empty());
        assert!(reg.slots.is_empty());
        assert!(!reg.features.enrichment_enabled);
    }

    #[test]
    fn serde_round_trip_empty_registry() {
        let reg = ProviderRegistry::default();
        let json = serde_json::to_string_pretty(&reg).unwrap();
        let loaded: ProviderRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert!(loaded.providers.is_empty());
        assert!(loaded.models.is_empty());
        assert!(loaded.slots.is_empty());
    }

    #[test]
    fn serde_round_trip_populated_registry() {
        let mut slots = HashMap::new();
        slots.insert(SlotName::Chat, "sonnet-4".to_string());
        slots.insert(SlotName::AgentDefault, "sonnet-4".to_string());
        slots.insert(SlotName::Enrichment, "haiku".to_string());

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
            features: Features { enrichment_enabled: true },
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
        assert_eq!(loaded.slots.get(&SlotName::Chat), Some(&"sonnet-4".to_string()));
        assert_eq!(loaded.slots.get(&SlotName::AgentDefault), Some(&"sonnet-4".to_string()));
        assert_eq!(loaded.slots.get(&SlotName::Enrichment), Some(&"haiku".to_string()));
        assert!(loaded.features.enrichment_enabled);
    }

    #[test]
    fn serde_missing_fields_use_defaults() {
        let json = "{}";
        let loaded: ProviderRegistry = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert!(loaded.providers.is_empty());
        assert!(loaded.models.is_empty());
        assert!(loaded.slots.is_empty());
        assert!(!loaded.features.enrichment_enabled);
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
            (SlotName::Chat, "chat"),
            (SlotName::AgentDefault, "agent_default"),
            (SlotName::AgentSearch, "agent_search"),
            (SlotName::AgentRead, "agent_read"),
            (SlotName::AgentWrite, "agent_write"),
            (SlotName::Headless, "headless"),
            (SlotName::Enrichment, "enrichment"),
        ];
        assert_eq!(variants.len(), 7, "All 7 slot names must be covered");

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
        map.insert(SlotName::Chat, "m1".to_string());
        map.insert(SlotName::AgentDefault, "m2".to_string());
        assert_eq!(map.get(&SlotName::Chat), Some(&"m1".to_string()));
        assert_eq!(map.get(&SlotName::AgentDefault), Some(&"m2".to_string()));
        assert!(map.get(&SlotName::Headless).is_none());
    }

    #[test]
    fn features_default() {
        let f = Features::default();
        assert!(!f.enrichment_enabled);
    }

    #[test]
    #[serial_test::serial]
    fn load_registry_returns_default_for_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let reg = load_registry();
        assert_eq!(reg.schema_version, 1);
        assert!(reg.providers.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn save_and_load_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let mut slots = HashMap::new();
        slots.insert(SlotName::Chat, "m1".to_string());

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
            features: Features { enrichment_enabled: true },
        };

        save_registry(&reg).unwrap();
        let loaded = load_registry();

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "p1");
        assert_eq!(loaded.models.len(), 1);
        assert_eq!(loaded.models[0].model_name, "gpt-4o");
        assert_eq!(loaded.models[0].tier, ModelTier::Premium);
        assert_eq!(loaded.slots.get(&SlotName::Chat), Some(&"m1".to_string()));
        assert!(loaded.features.enrichment_enabled);
    }

    // -- resolve_slot tests --

    fn test_registry() -> ProviderRegistry {
        let mut slots = HashMap::new();
        slots.insert(SlotName::Chat, "sonnet".to_string());
        slots.insert(SlotName::AgentDefault, "sonnet".to_string());
        slots.insert(SlotName::AgentSearch, "haiku".to_string());
        slots.insert(SlotName::Headless, "gpt4o".to_string());

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
            features: Features::default(),
        }
    }

    #[test]
    #[serial_test::serial]
    fn resolve_slot_chat() {
        crate::credentials::set(
            crate::credentials::Credential::Provider("anthropic-main"),
            "sk-ant-test",
        ).unwrap();

        let reg = test_registry();
        let resolved = resolve_slot(&reg, SlotName::Chat).unwrap();
        assert_eq!(resolved.config.model, "claude-sonnet-4-5-20241022");
        assert_eq!(resolved.api_key, "sk-ant-test");
        assert_eq!(resolved.provider_type, ProviderType::Anthropic);
        assert!(resolved.config.base_url.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn resolve_slot_agent_phase_fallback_to_default() {
        crate::credentials::set(
            crate::credentials::Credential::Provider("anthropic-main"),
            "sk-ant-test",
        ).unwrap();

        let reg = test_registry();
        // AgentRead not explicitly set → falls back to AgentDefault
        let resolved = resolve_slot(&reg, SlotName::AgentRead).unwrap();
        assert_eq!(resolved.config.model, "claude-sonnet-4-5-20241022");

        // AgentWrite not explicitly set → falls back to AgentDefault
        let resolved2 = resolve_slot(&reg, SlotName::AgentWrite).unwrap();
        assert_eq!(resolved2.config.model, "claude-sonnet-4-5-20241022");
    }

    #[test]
    #[serial_test::serial]
    fn resolve_slot_agent_search_uses_explicit() {
        crate::credentials::set(
            crate::credentials::Credential::Provider("anthropic-main"),
            "sk-ant-test",
        ).unwrap();

        let reg = test_registry();
        // AgentSearch IS explicitly set to haiku
        let resolved = resolve_slot(&reg, SlotName::AgentSearch).unwrap();
        assert_eq!(resolved.config.model, "claude-haiku-4-5-20241022");
    }

    #[test]
    #[serial_test::serial]
    fn resolve_slot_ollama_gets_default_base_url() {
        let mut reg = test_registry();
        reg.slots.insert(SlotName::Enrichment, "llama".to_string());

        let resolved = resolve_slot(&reg, SlotName::Enrichment).unwrap();
        assert_eq!(resolved.config.model, "llama3.2");
        assert_eq!(resolved.config.base_url.as_deref(), Some("http://localhost:11434/v1/"));
        assert_eq!(resolved.api_key, "local");
        assert_eq!(resolved.provider_type, ProviderType::Ollama);
    }

    #[test]
    fn resolve_slot_error_no_slot_configured() {
        let reg = test_registry();
        let result = resolve_slot(&reg, SlotName::Enrichment);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No model configured"));
    }

    #[test]
    fn resolve_slot_error_dangling_model_ref() {
        let mut reg = test_registry();
        reg.slots.insert(SlotName::Enrichment, "nonexistent".to_string());

        let result = resolve_slot(&reg, SlotName::Enrichment);
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
        reg.slots.insert(SlotName::Enrichment, "orphan".to_string());

        let result = resolve_slot(&reg, SlotName::Enrichment);
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
        reg.slots.insert(SlotName::Chat, "m1".to_string());

        let resolved = resolve_slot(&reg, SlotName::Chat).unwrap();
        assert_eq!(resolved.config.base_url.as_deref(), Some("http://my-llm:8080/v1/"));
    }
}
