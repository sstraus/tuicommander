//! Explicit top-level data projections. Values and provenance are data, not grants.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "003-e678 contract; first consumer 004-2809")
)]

use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DataSource {
    WorkflowInput {
        field: String,
    },
    /// Ordinal in the immutable published instance vector, not a completion order.
    ActivityOutput {
        activity: usize,
        field: String,
    },
    ScopeOutput {
        scope: String,
        field: String,
    },
    Constant {
        value: Value,
    },
    /// A JSON pointer inside one field. Nested values are opaque in the shallow
    /// schema, so the projection declares the type it requires.
    Projection {
        from: Box<DataSource>,
        pointer: String,
        value_type: ValueType,
    },
    /// A typed object built from named member sources. No expressions.
    Object {
        fields: BTreeMap<String, DataSource>,
    },
}

const MAX_POINTER_BYTES: usize = 512;

impl DataSource {
    /// Every base source this value is built from, in declaration order.
    pub(crate) fn leaves(&self) -> Vec<&DataSource> {
        match self {
            Self::Projection { from, .. } => from.leaves(),
            Self::Object { fields } => fields.values().flat_map(Self::leaves).collect(),
            source => vec![source],
        }
    }

    /// True when any base source is an output of `activity`.
    pub(crate) fn reads_activity(&self, activity: usize) -> bool {
        self.leaves().iter().any(
            |leaf| matches!(leaf, Self::ActivityOutput { activity: producer, .. } if *producer == activity),
        )
    }

    pub(crate) fn schema(
        &self,
        workflow: &ObjectSchema,
        outputs: &[&ObjectSchema],
        scopes: &super::scope_data::ScopeSchemas,
    ) -> Result<FieldSchema, String> {
        match self {
            Self::WorkflowInput { field } => workflow
                .fields
                .get(field)
                .cloned()
                .ok_or_else(|| format!("unknown workflow input {field}")),
            Self::ActivityOutput { activity, field } => outputs
                .get(*activity)
                .and_then(|schema| schema.fields.get(field))
                .cloned()
                .ok_or_else(|| format!("unknown activity output {activity}.{field}")),
            Self::ScopeOutput { scope, field } => scopes
                .get(scope)
                .and_then(|schema| schema.fields.get(field))
                .cloned()
                .ok_or_else(|| format!("unknown scope output {scope}.{field}")),
            Self::Projection {
                from,
                pointer,
                value_type,
            } => {
                if !matches!(
                    **from,
                    Self::WorkflowInput { .. }
                        | Self::ActivityOutput { .. }
                        | Self::ScopeOutput { .. }
                ) {
                    return Err("projection reads only an input or output field".into());
                }
                if !pointer.starts_with('/') || pointer.len() > MAX_POINTER_BYTES {
                    return Err("projection needs a non-empty JSON pointer".into());
                }
                let base = from.schema(workflow, outputs, scopes)?;
                if !matches!(base.value_type, ValueType::Object | ValueType::Array) {
                    return Err("projection requires an object or array field".into());
                }
                Ok(FieldSchema {
                    value_type: value_type.clone(),
                    required: base.required,
                    default: None,
                })
            }
            Self::Object { fields } => {
                for (name, member) in fields {
                    member
                        .schema(workflow, outputs, scopes)
                        .map_err(|e| format!("object.{name}: {e}"))?;
                }
                Ok(FieldSchema {
                    value_type: ValueType::Object,
                    required: true,
                    default: None,
                })
            }
            Self::Constant { value } => Ok(FieldSchema {
                value_type: match value {
                    Value::Null => ValueType::Null,
                    Value::Bool(_) => ValueType::Boolean,
                    Value::String(_) => ValueType::String,
                    Value::Number(n) if n.as_i64().is_some() || n.as_u64().is_some() => {
                        ValueType::Integer
                    }
                    Value::Number(_) => ValueType::Number,
                    Value::Array(_) => ValueType::Array,
                    Value::Object(_) => super::data_reference::DataReference::parse(value)
                        .map(|reference| reference.value_type())
                        .unwrap_or(ValueType::Object),
                },
                required: true,
                default: None,
            }),
        }
    }

    pub(crate) fn resolve(
        &self,
        workflow: &BTreeMap<String, Datum>,
        outputs: &BTreeMap<usize, BTreeMap<String, Datum>>,
        scopes: &super::scope_data::ScopeValues,
    ) -> Datum {
        let (mut datum, origin) = match self {
            DataSource::Projection {
                from,
                pointer,
                value_type,
            } => {
                let base = from.resolve(workflow, outputs, scopes);
                // A missing path is absent and a wrong type is unknown: never a default.
                let state = match base.state {
                    DataState::Present(value) => match value.pointer(pointer) {
                        Some(found) if value_type.matches(found) => {
                            DataState::Present(found.clone())
                        }
                        Some(_) => DataState::Unknown,
                        None => DataState::Absent,
                    },
                    other => other,
                };
                return Datum {
                    state,
                    provenance: base.provenance,
                };
            }
            DataSource::Object { fields } => {
                let mut object = serde_json::Map::new();
                let mut provenance = BTreeSet::new();
                let mut unknown = false;
                for (name, member) in fields {
                    let datum = member.resolve(workflow, outputs, scopes);
                    provenance.extend(datum.provenance);
                    match datum.state {
                        DataState::Unknown => unknown = true,
                        DataState::Absent => {}
                        DataState::Present(value) => {
                            object.insert(name.clone(), value);
                        }
                    }
                }
                return Datum {
                    state: if unknown {
                        DataState::Unknown
                    } else {
                        DataState::Present(Value::Object(object))
                    },
                    provenance,
                };
            }
            DataSource::WorkflowInput { field } => (
                workflow.get(field).cloned().unwrap_or(Datum {
                    state: DataState::Absent,
                    provenance: BTreeSet::new(),
                }),
                DataOrigin::WorkflowInput {
                    field: field.clone(),
                },
            ),
            DataSource::ActivityOutput { activity, field } => (
                outputs
                    .get(activity)
                    .map(|fields| {
                        fields.get(field).cloned().unwrap_or(Datum {
                            state: DataState::Absent,
                            provenance: BTreeSet::new(),
                        })
                    })
                    .unwrap_or(Datum {
                        state: DataState::Unknown,
                        provenance: BTreeSet::new(),
                    }),
                DataOrigin::ActivityOutput {
                    activity: *activity,
                    field: field.clone(),
                },
            ),
            DataSource::ScopeOutput { scope, field } => (
                scopes
                    .get(scope)
                    .map(|fields| {
                        fields.get(field).cloned().unwrap_or(Datum {
                            state: DataState::Absent,
                            provenance: BTreeSet::new(),
                        })
                    })
                    .unwrap_or(Datum {
                        state: DataState::Unknown,
                        provenance: BTreeSet::new(),
                    }),
                DataOrigin::ScopeOutput {
                    scope: scope.clone(),
                    field: field.clone(),
                },
            ),
            DataSource::Constant { value } => (
                Datum {
                    state: DataState::Present(value.clone()),
                    provenance: BTreeSet::new(),
                },
                DataOrigin::Constant,
            ),
        };
        datum.provenance.insert(origin);
        datum
    }
}

pub(crate) type InputBindings = BTreeMap<String, DataSource>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DataOrigin {
    WorkflowInput {
        field: String,
    },
    ActivityOutput {
        activity: usize,
        field: String,
    },
    ScopeOutput {
        scope: String,
        field: String,
    },
    Constant,
    Default,
    ScopeResult {
        identity: super::collection::CollectionIdentity,
    },
}

/// Unknown means the producer has not supplied a result. Absent means a known
/// result omitted this field. Present(null) and Present([]) are neither state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum DataState {
    Unknown,
    Absent,
    Present(Value),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Datum {
    pub state: DataState,
    pub provenance: BTreeSet<DataOrigin>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolvedInputs {
    pub values: BTreeMap<String, Value>,
    pub provenance: BTreeMap<String, BTreeSet<DataOrigin>>,
}

fn compatible(source: &ValueType, target: &ValueType) -> bool {
    source == target || (*source == ValueType::Integer && *target == ValueType::Number)
}

/// Validate only explicit projections; no expression evaluation or authority.
pub(crate) fn validate_bindings(
    bindings: &InputBindings,
    workflow: &ObjectSchema,
    outputs: &[&ObjectSchema],
    target: &ObjectSchema,
    activity: usize,
    scopes: &super::scope_data::ScopeSchemas,
) -> Result<(), String> {
    for (name, source) in bindings {
        let dest = target
            .fields
            .get(name)
            .ok_or_else(|| format!("binding.{name}: unknown target field"))?;
        if source.reads_activity(activity) {
            return Err(format!("binding.{name}: self dependency"));
        }
        let field = source
            .schema(workflow, outputs, scopes)
            .map_err(|e| format!("binding.{name}: {e}"))?;
        if !compatible(&field.value_type, &dest.value_type) {
            let kind = if matches!(source, DataSource::Constant { .. }) {
                "constant"
            } else {
                "field type"
            };
            return Err(format!("binding.{name}: incompatible {kind}"));
        }
        // Workflow defaults are normalized before projection. An optional output
        // may be absent in a producer snapshot, even when its schema has a default.
        let guaranteed = field.required
            || (matches!(source, DataSource::WorkflowInput { .. }) && field.default.is_some());
        if dest.required && dest.default.is_none() && !guaranteed {
            return Err(format!("binding.{name}: required field not guaranteed"));
        }
    }
    for (name, field) in &target.fields {
        if field.required && field.default.is_none() && !bindings.contains_key(name) {
            return Err(format!("binding.{name}: required target is unbound"));
        }
    }
    Ok(())
}

/// Resolve against an immutable producer snapshot. A missing producer is unknown,
/// not an omitted field, and cannot be masked by a target default.
pub(crate) fn resolve_bindings(
    bindings: &InputBindings,
    workflow: &BTreeMap<String, Datum>,
    outputs: &BTreeMap<usize, BTreeMap<String, Datum>>,
    target: &ObjectSchema,
    scopes: &super::scope_data::ScopeValues,
) -> Result<ResolvedInputs, String> {
    let mut values = BTreeMap::new();
    let mut provenance = BTreeMap::new();
    for (name, source) in bindings {
        if !target.fields.contains_key(name) {
            return Err(format!("binding.{name}: unknown target field"));
        }
        let datum = source.resolve(workflow, outputs, scopes);
        match datum.state {
            DataState::Unknown => return Err(format!("binding.{name}: source is unknown")),
            DataState::Absent => {}
            DataState::Present(value) => {
                values.insert(name.clone(), value);
                provenance.insert(name.clone(), datum.provenance);
            }
        }
    }
    let resolved = target.resolve(&values, "bound inputs")?;
    for name in resolved.keys() {
        provenance
            .entry(name.clone())
            .or_insert_with(|| BTreeSet::from([DataOrigin::Default]));
    }
    Ok(ResolvedInputs {
        values: resolved,
        provenance,
    })
}
