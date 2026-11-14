//! Typed references contain host-resolved IDs, not credentials or grants.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "003-e678 contract; first consumer 004-2809")
)]

use super::catalog::{ObjectSchema, ValueType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "reference", rename_all = "snake_case", deny_unknown_fields)]
pub enum DataReference {
    Workspace { id: String },
    Artifact { id: String, revision: String },
    Credential { id: String },
}

impl DataReference {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let reference: Self = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid data reference: {e}"))?;
        let id = match &reference {
            Self::Workspace { id } | Self::Artifact { id, .. } | Self::Credential { id } => id,
        };
        if id.trim().is_empty() {
            return Err("data reference requires a host identifier".into());
        }
        if let Self::Artifact { revision, .. } = &reference
            && revision.trim().is_empty()
        {
            return Err("artifact reference requires a revision".into());
        }
        Ok(reference)
    }

    pub(crate) fn value_type(&self) -> ValueType {
        match self {
            Self::Workspace { .. } => ValueType::WorkspaceReference,
            Self::Artifact { .. } => ValueType::ArtifactReference,
            Self::Credential { .. } => ValueType::CredentialReference,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReferenceInputs {
    pub fields: BTreeMap<String, DataReference>,
}

impl ReferenceInputs {
    pub(crate) fn resolve(
        schema: &ObjectSchema,
        values: &BTreeMap<String, Value>,
    ) -> Result<Self, String> {
        let values = schema.resolve(values, "subflow inputs")?;
        let mut fields = BTreeMap::new();
        for (name, field) in &schema.fields {
            if matches!(
                field.value_type,
                ValueType::WorkspaceReference
                    | ValueType::ArtifactReference
                    | ValueType::CredentialReference
            ) && let Some(value) = values.get(name)
            {
                fields.insert(name.clone(), DataReference::parse(value)?);
            }
        }
        Ok(Self { fields })
    }
}
