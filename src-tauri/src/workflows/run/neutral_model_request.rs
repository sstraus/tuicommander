//! Literal, strict request contract for the mediated ACP activity.
use crate::acp::EgoAcpConfig;
use crate::workflows::catalog::ObjectSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelRequest {
    pub profile: String,
    pub model: String,
    pub effort: String,
    pub instructions: String,
    pub context: Vec<String>,
    pub output_schema: ObjectSchema,
    pub limits: ModelLimits,
    pub unavailable_policy: UnavailablePolicy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelLimits {
    pub timeout_ms: u64,
    pub max_context_bytes: usize,
    pub max_output_bytes: usize,
}

/// Unsupported policy spellings fail deserialization instead of choosing a model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum UnavailablePolicy {
    Fail,
}

impl ModelRequest {
    pub fn validate(&self, runtime: &EgoAcpConfig) -> Result<(), String> {
        if self.profile.trim().is_empty()
            || self.profile != runtime.profile
            || !runtime.executable.is_absolute()
            || runtime.ai_chat
            || !runtime.master_prompt.is_empty()
        {
            return Err("model unavailable: exact isolated runtime/profile required".into());
        }
        if [&self.model, &self.effort]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err("model unavailable: exact model and effort required".into());
        }
        if !(1..=300_000).contains(&self.limits.timeout_ms)
            || !(1..=1_048_576).contains(&self.limits.max_context_bytes)
            || !(1..=1_048_576).contains(&self.limits.max_output_bytes)
        {
            return Err("model limits exceed the supported bounds".into());
        }
        let size = self
            .context
            .iter()
            .try_fold(self.instructions.len(), |n, item| {
                n.checked_add(item.len())
                    .ok_or("model context size overflow")
            })?;
        if size > self.limits.max_context_bytes {
            return Err("model context exceeds its declared byte limit".into());
        }
        self.output_schema.check_schema("model output")
    }

    pub fn prompt(&self) -> Result<String, String> {
        // Explicit fields preserve literal context boundaries; no shell/template expansion.
        // output() parses the whole reply as JSON, and real models fence JSON in Markdown
        // unless told not to (live run 2026-11-13).
        serde_json::to_string(&serde_json::json!({
            "instructions": self.instructions,
            "context": self.context,
            "response": "Return exactly one JSON object matching output_schema as raw JSON text: no Markdown code fence and no other text",
            "output_schema": self.output_schema,
        }))
        .map_err(|e| e.to_string())
    }

    pub fn output(&self, text: &str) -> Result<BTreeMap<String, Value>, String> {
        if text.len() > self.limits.max_output_bytes {
            return Err("model output exceeds its declared byte limit".into());
        }
        let object: BTreeMap<String, Value> = serde_json::from_str(text)
            .map_err(|e| format!("model output must be a complete JSON object: {e}"))?;
        self.output_schema.resolve(&object, "model output")
    }
}
