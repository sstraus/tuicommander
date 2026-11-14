//! Neutral, versioned workflow activity contracts.
//!
//! Capability names are requirements, not grants. This module performs no
//! authority checks and does not execute handlers.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "002-eeb6 contract; first consumer 004-2809")
)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct DescriptorId {
    pub namespace: String,
    pub name: String,
    pub version: u32,
}

impl DescriptorId {
    fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.namespace) || !valid_name(&self.name) || self.version == 0 {
            return Err("descriptor identity requires ASCII names and a nonzero version".into());
        }
        Ok(())
    }
}

// Separators used in diagnostics are excluded; names are never normalized.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct HandlerRef {
    pub name: String,
    pub version: u32,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct DescriptorPin {
    pub id: DescriptorId,
    pub sha256: String,
}

/// A flat set of descriptor pins, not a transitive workflow closure or permit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogClosure {
    pins: BTreeSet<DescriptorPin>,
}

impl CatalogClosure {
    /// Derive pins from the actual instances, never from supplied pin claims.
    pub fn from_instances(
        catalog: &WorkflowCatalog,
        instances: &[ActivityInstance],
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<Self, String> {
        let mut pins = BTreeSet::new();
        for instance in instances {
            let pin = catalog.pin(&instance.descriptor)?;
            let descriptor = catalog.resolve_pinned(&pin, handlers)?;
            if !descriptor
                .execution_policies
                .contains(&instance.execution_policy)
            {
                return Err(format!(
                    "unsupported execution policy: {}",
                    instance.execution_policy
                ));
            }
            descriptor.config.resolve(&instance.config, "config")?;
            pins.insert(pin);
        }
        Ok(Self { pins })
    }

    pub fn pins(&self) -> &BTreeSet<DescriptorPin> {
        &self.pins
    }

    /// Recheck each serialized claim against current catalog bytes and handlers.
    /// This alone cannot establish that a caller omitted no instances.
    pub fn resolve<'a>(
        &self,
        catalog: &'a WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<Vec<&'a ActivityDescriptor>, String> {
        self.pins
            .iter()
            .map(|pin| catalog.resolve_pinned(pin, handlers))
            .collect()
    }

    /// Check both pin contents and exact coverage of the supplied instances.
    pub fn verify_instances(
        &self,
        catalog: &WorkflowCatalog,
        instances: &[ActivityInstance],
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<(), String> {
        self.resolve(catalog, handlers)?;
        if *self != Self::from_instances(catalog, instances, handlers)? {
            return Err("descriptor pin set does not match instances".into());
        }
        Ok(())
    }
}

// Sort every JSON object recursively, including objects in default values, even
// when another dependency enables serde_json's preserve_order feature.
fn canonicalize(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, mut value) in entries {
                canonicalize(&mut value);
                map.insert(key, value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(canonicalize),
        _ => {}
    }
}

pub(super) fn content_digest<T: Serialize>(content: &T) -> Result<String, String> {
    let mut value = serde_json::to_value(content).map_err(|e| format!("encode content: {e}"))?;
    canonicalize(&mut value);
    let bytes = serde_json::to_vec(&value).map_err(|e| format!("encode content: {e}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn descriptor_digest(descriptor: &ActivityDescriptor) -> Result<String, String> {
    content_digest(descriptor)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    WorkspaceReference,
    ArtifactReference,
    CredentialReference,
    Null,
    String,
    Boolean,
    Integer,
    Number,
    Object,
    Array,
}

impl ValueType {
    pub(super) fn matches(&self, value: &Value) -> bool {
        match self {
            Self::WorkspaceReference | Self::ArtifactReference | Self::CredentialReference => {
                super::data_reference::DataReference::parse(value)
                    .is_ok_and(|reference| reference.value_type() == *self)
            }
            Self::Null => value.is_null(),
            Self::String => value.is_string(),
            Self::Boolean => value.is_boolean(),
            Self::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
            Self::Number => value.is_number(),
            Self::Object => value.is_object(),
            Self::Array => value.is_array(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldSchema {
    pub value_type: ValueType,
    pub required: bool,
    pub default: Option<Value>,
}

/// A deliberately shallow object schema; nested values remain opaque.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectSchema {
    pub fields: BTreeMap<String, FieldSchema>,
}

impl ObjectSchema {
    pub(super) fn check_schema(&self, context: &str) -> Result<(), String> {
        for (name, field) in &self.fields {
            if name.trim().is_empty() {
                return Err(format!("{context}: empty field name"));
            }
            // Option<Value> uses JSON null for no default in the stored contract.
            // Explicit null values are supported, but null defaults would not
            // survive a descriptor roundtrip with the same meaning.
            if field.default.as_ref().is_some_and(Value::is_null) {
                return Err(format!(
                    "{context}.{name}: null defaults are unsupported; bind explicit null"
                ));
            }
            if let Some(value) = &field.default
                && !field.value_type.matches(value)
            {
                return Err(format!("{context}.{name}: default has wrong type"));
            }
        }
        Ok(())
    }

    pub(super) fn resolve(
        &self,
        values: &BTreeMap<String, Value>,
        context: &str,
    ) -> Result<BTreeMap<String, Value>, String> {
        for name in values.keys() {
            if !self.fields.contains_key(name) {
                return Err(format!("{context}.{name}: unknown field"));
            }
        }
        let mut resolved = BTreeMap::new();
        for (name, field) in &self.fields {
            let value = values.get(name).or(field.default.as_ref());
            match value {
                Some(value) if field.value_type.matches(value) => {
                    resolved.insert(name.clone(), value.clone());
                }
                Some(_) => return Err(format!("{context}.{name}: wrong type")),
                None if field.required => {
                    return Err(format!("{context}.{name}: required field missing"));
                }
                None => {}
            }
        }
        Ok(resolved)
    }
}

/// Typed subject contract. Absence of a subject is distinct from an empty object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubjectSchema {
    pub required: bool,
    pub fields: ObjectSchema,
}

impl SubjectSchema {
    pub(super) fn check_binding(&self, workflow: Option<&SubjectSchema>) -> Result<(), String> {
        let Some(workflow) = workflow else {
            return if self.required {
                Err("operation subject is required".into())
            } else {
                Ok(())
            };
        };
        if self.required && !workflow.required {
            return Err("operation subject requires a required workflow subject".into());
        }
        for (name, source) in &workflow.fields.fields {
            let target = self
                .fields
                .fields
                .get(name)
                .ok_or_else(|| format!("operation subject.{name}: unknown field"))?;
            if source.value_type != target.value_type
                && !(source.value_type == ValueType::Integer
                    && target.value_type == ValueType::Number)
            {
                return Err(format!("operation subject.{name}: incompatible type"));
            }
        }
        for (name, target) in &self.fields.fields {
            if target.required && target.default.is_none() {
                match workflow.fields.fields.get(name) {
                    Some(source) if source.required || source.default.is_some() => (),
                    _ => {
                        return Err(format!(
                            "operation subject.{name}: required field not guaranteed"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn resolve(
        &self,
        subject: Option<&BTreeMap<String, Value>>,
    ) -> Result<Option<BTreeMap<String, Value>>, String> {
        match subject {
            None if self.required => Err("operation subject is required".into()),
            None => Ok(None),
            Some(subject) => self.fields.resolve(subject, "operation subject").map(Some),
        }
    }
}

/// User-defined execution target. Schemas and outcomes stay on the descriptor,
/// never in instance configuration. The host still owns the named handler.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReusableTarget {
    Script {
        invocation: super::command::Invocation,
    },
    Subflow {
        publication: super::neutral_publication::PublicationPin,
    },
    Http {
        connector: super::connector::ConnectorContract,
        plugin: String,
        base: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActivityDescriptor {
    pub id: DescriptorId,
    pub purpose: String,
    /// A host-resolved handler reference, not an authorization decision.
    pub handler: HandlerRef,
    pub target: Option<ReusableTarget>,
    pub config: ObjectSchema,
    pub inputs: ObjectSchema,
    pub subject: Option<SubjectSchema>,
    pub outputs: ObjectSchema,
    pub outcomes: BTreeSet<String>,
    pub required_capabilities: BTreeSet<String>,
    pub execution_policies: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActivityInstance {
    pub descriptor: DescriptorId,
    pub config: BTreeMap<String, Value>,
    pub execution_policy: String,
}

/// Resolved data only. Its public fields are forgeable; it is not a dispatch permit.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedInstance {
    pub descriptor: ActivityDescriptor,
    pub config: BTreeMap<String, Value>,
    pub inputs: BTreeMap<String, Value>,
}

#[derive(Clone, Default)]
pub struct WorkflowCatalog {
    descriptors: BTreeMap<DescriptorId, ActivityDescriptor>,
}

impl WorkflowCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: ActivityDescriptor) -> Result<(), String> {
        let id = &descriptor.id;
        id.validate()?;
        if descriptor.purpose.trim().is_empty()
            || !valid_name(&descriptor.handler.name)
            || descriptor.handler.version == 0
        {
            return Err("descriptor needs a purpose and a valid versioned handler".into());
        }
        if descriptor.outcomes.is_empty()
            || descriptor.outcomes.iter().any(|s| s.trim().is_empty())
            || descriptor.execution_policies.is_empty()
            || descriptor
                .execution_policies
                .iter()
                .any(|s| s.trim().is_empty())
            || descriptor
                .required_capabilities
                .iter()
                .any(|s| s.trim().is_empty())
        {
            return Err("descriptor has empty outcomes, policies or capability names".into());
        }
        if let Some(ReusableTarget::Script { invocation }) = &descriptor.target {
            // Reuse the existing explicit inline shell contract, not mutable paths
            // or executable-byte attestation. Later adapters own execution policy.
            match invocation {
                super::command::Invocation::Shell {
                    program,
                    script,
                    version,
                } if *version == 1
                    && !program.is_empty()
                    && !script.is_empty()
                    && program.len() <= 32_768
                    && script.len() <= 32_768
                    && !program.contains('\0')
                    && !script.contains('\0') => {}
                _ => {
                    return Err(
                        "reusable script requires a bounded version 1 inline shell invocation"
                            .into(),
                    );
                }
            }
        }
        if let Some(ReusableTarget::Subflow { publication }) = &descriptor.target {
            if publication.reference.revision <= 0
                || publication.reference.id.is_empty()
                || !crate::fs::is_absolute_on_any_platform(&publication.reference.project)
            {
                return Err("invalid reusable subflow reference".into());
            }
            validate_digest(&publication.sha256)?;
        }
        descriptor.config.check_schema("config")?;
        descriptor.inputs.check_schema("inputs")?;
        if let Some(subject) = &descriptor.subject {
            subject.fields.check_schema("operation subject")?;
        }
        descriptor.outputs.check_schema("outputs")?;
        if self.descriptors.contains_key(id) {
            return Err(format!(
                "duplicate descriptor {}:{}@{}",
                id.namespace, id.name, id.version
            ));
        }
        self.descriptors.insert(id.clone(), descriptor);
        Ok(())
    }

    pub(crate) fn descriptors(&self) -> impl Iterator<Item = &ActivityDescriptor> {
        self.descriptors.values()
    }

    pub fn get(&self, id: &DescriptorId) -> Option<&ActivityDescriptor> {
        self.descriptors.get(id)
    }

    /// Pin the current descriptor body; the returned digest is a content claim.
    pub fn pin(&self, id: &DescriptorId) -> Result<DescriptorPin, String> {
        id.validate()?;
        let descriptor = self.get(id).ok_or_else(|| {
            format!(
                "missing descriptor {}:{}@{}",
                id.namespace, id.name, id.version
            )
        })?;
        Ok(DescriptorPin {
            id: id.clone(),
            sha256: descriptor_digest(descriptor)?,
        })
    }

    /// Verify a pin against current catalog content and exact handler availability.
    /// This is not an authority or freshness check.
    pub fn resolve_pinned(
        &self,
        pin: &DescriptorPin,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<&ActivityDescriptor, String> {
        pin.id.validate()?;
        validate_digest(&pin.sha256)?;
        let descriptor = self.get(&pin.id).ok_or_else(|| {
            format!(
                "missing descriptor {}:{}@{}",
                pin.id.namespace, pin.id.name, pin.id.version
            )
        })?;
        if descriptor_digest(descriptor)? != pin.sha256 {
            return Err(format!(
                "descriptor content mismatch for {}:{}@{}",
                pin.id.namespace, pin.id.name, pin.id.version
            ));
        }
        if !handlers.contains(&descriptor.handler) {
            return Err(format!(
                "handler unavailable: {}@{}",
                descriptor.handler.name, descriptor.handler.version
            ));
        }
        Ok(descriptor)
    }

    /// Validate adapter output before a future runtime consumes it. Unknown
    /// outcomes and missing required fields are protocol failures, not success.
    pub fn validate_output(
        &self,
        pin: &DescriptorPin,
        handlers: &BTreeSet<HandlerRef>,
        outcome: &str,
        values: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let descriptor = self.resolve_pinned(pin, handlers)?;
        if !descriptor.outcomes.contains(outcome) {
            return Err(format!("unknown outcome: {outcome}"));
        }
        descriptor.outputs.resolve(values, "outputs")
    }

    /// Validate against the caller's current host handler inventory. This only
    /// checks availability: the host must separately check current authority.
    pub fn validate_instance(
        &self,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        selected_outcomes: &BTreeSet<String>,
        available_handlers: &BTreeSet<HandlerRef>,
    ) -> Result<ValidatedInstance, String> {
        let descriptor = self.get(&instance.descriptor).ok_or_else(|| {
            format!(
                "missing descriptor {}:{}@{}",
                instance.descriptor.namespace,
                instance.descriptor.name,
                instance.descriptor.version
            )
        })?;
        if !available_handlers.contains(&descriptor.handler) {
            return Err(format!(
                "handler unavailable: {}@{}",
                descriptor.handler.name, descriptor.handler.version
            ));
        }
        if !descriptor
            .execution_policies
            .contains(&instance.execution_policy)
        {
            return Err(format!(
                "unsupported execution policy: {}",
                instance.execution_policy
            ));
        }
        if let Some(outcome) = selected_outcomes.difference(&descriptor.outcomes).next() {
            return Err(format!("unknown outcome: {outcome}"));
        }
        Ok(ValidatedInstance {
            descriptor: descriptor.clone(),
            config: descriptor.config.resolve(&instance.config, "config")?,
            inputs: descriptor.inputs.resolve(inputs, "inputs")?,
        })
    }
}

pub(super) fn validate_digest(digest: &str) -> Result<(), String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("malformed descriptor SHA-256 pin".into());
    }
    Ok(())
}
