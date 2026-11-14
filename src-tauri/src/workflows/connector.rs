//! Shared metadata for backend-installed HTTP, client and tool operations.
//! References describe work; current host grants remain separate.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "009 connector awaits host composition in 008")
)]

use super::catalog::ObjectSchema;
use super::data_reference::DataReference;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectorContract {
    pub id: String,
    pub authentication: Option<DataReference>,
    pub resources: ObjectSchema,
    pub pagination: Option<CursorPagination>,
    pub rate_limit: RateLimit,
    pub callback: Option<CallbackCorrelation>,
    pub reconciliation: Reconciliation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CursorPagination {
    pub query_parameter: String,
    pub items_pointer: String,
    pub cursor_pointer: String,
    pub output: String,
    pub max_pages: u32,
    pub max_items: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RateLimit {
    /// Retain a failure and Retry-After hint; never repeat a possible write.
    FailWithRetryAfter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CallbackCorrelation {
    /// Host-installed safe header receives the precommitted effect digest.
    pub header: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Reconciliation {
    Unavailable,
    /// Installed service adapter observes the original effect; never repeats a write.
    ReadOnlyServiceObservation,
    /// Callback ingestion must submit Observe with the original EffectKey.
    OriginalEffectCallback,
}

impl ConnectorContract {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("connector identity is empty".into());
        }
        self.resources.check_schema("connector resources")?;
        if let Some(reference) = &self.authentication {
            let value = serde_json::to_value(reference).map_err(|e| e.to_string())?;
            if !matches!(
                DataReference::parse(&value)?,
                DataReference::Credential { .. }
            ) {
                return Err("connector authentication requires a credential reference".into());
            }
        }
        if let Some(p) = &self.pagination
            && (p.max_pages == 0
                || p.max_pages > 100
                || p.max_items == 0
                || p.max_items > 100_000
                || p.query_parameter.is_empty()
                || p.query_parameter.len() > 128
                || p.output.is_empty()
                || p.items_pointer.len() > 1024
                || p.cursor_pointer.len() > 1024
                || !p.items_pointer.starts_with('/')
                || !p.cursor_pointer.starts_with('/'))
        {
            return Err("invalid bounded cursor pagination contract".into());
        }
        if let Some(callback) = &self.callback {
            safe_header(&callback.header)?;
        }
        if self.reconciliation == Reconciliation::OriginalEffectCallback && self.callback.is_none()
        {
            return Err("callback reconciliation requires correlation".into());
        }
        Ok(())
    }
}

/// Values can fill declared data slots, including argv slots of installed
/// clients. They cannot select a destination, executable or mutation grant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundedValue {
    Literal { value: Value },
    Input { field: String, max_bytes: usize },
}

impl BoundedValue {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Literal { value } => {
                bounded(value, 65_536)?;
            }
            Self::Input { field, max_bytes }
                if !field.is_empty() && *max_bytes > 0 && *max_bytes <= 65_536 => {}
            _ => return Err("invalid bounded input field".into()),
        }
        Ok(())
    }

    pub(crate) fn resolve(&self, inputs: &BTreeMap<String, Value>) -> Result<Value, String> {
        self.validate()?;
        let (value, max) = match self {
            Self::Literal { value } => (value, 65_536),
            Self::Input { field, max_bytes } => (
                inputs.get(field).ok_or("connector input is absent")?,
                *max_bytes,
            ),
        };
        bounded(value, max)?;
        Ok(value.clone())
    }
}

fn bounded(value: &Value, max: usize) -> Result<(), String> {
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > max {
        return Err("connector value exceeds byte bound".into());
    }
    Ok(())
}

pub(crate) fn safe_header(name: &str) -> Result<(), String> {
    let lower = name.to_ascii_lowercase();
    if !(matches!(
        lower.as_str(),
        "accept" | "content-type" | "if-match" | "if-none-match"
    ) || lower.starts_with("x-"))
        || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
    {
        return Err("connector header is not a safe data header".into());
    }
    Ok(())
}
