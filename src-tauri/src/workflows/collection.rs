//! Bounded collection snapshots and ordered results, not a scheduling owner.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "003-e678 contract; first consumer 004-2809")
)]

use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use super::data_binding::{DataOrigin, DataSource, DataState, Datum, ResolvedInputs};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EmptyPolicy {
    Complete,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailurePolicy {
    Collect,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForEach {
    pub collection: DataSource,
    pub item_type: ValueType,
    pub body: usize,
    pub item_input: String,
    pub index_input: String,
    pub max_items: u32,
    pub max_concurrency: u32,
    pub empty: EmptyPolicy,
    pub failure: FailurePolicy,
}

/// Supplied by the execution owner. Re-expansion must retain the same immutable
/// source snapshot; a later traversal has a different generation.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectionIdentity {
    pub run: String,
    pub scope: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemIdentity {
    pub collection: CollectionIdentity,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectionItem {
    pub identity: ItemIdentity,
    pub inputs: BTreeMap<String, Datum>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Expansion {
    identity: CollectionIdentity,
    items: Vec<CollectionItem>,
}

impl Expansion {
    pub(crate) fn items(&self) -> &[CollectionItem] {
        &self.items
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ItemState {
    Pending,
    Completed {
        outcome: String,
        outputs: BTreeMap<String, Datum>,
    },
    Failed {
        diagnostic: String,
    },
    Skipped {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemResult {
    pub identity: ItemIdentity,
    pub state: ItemState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AggregateState {
    Pending,
    Complete,
    Blocked,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Aggregate {
    pub identity: CollectionIdentity,
    pub state: AggregateState,
    pub items: Vec<ItemResult>,
}

impl ForEach {
    pub(crate) fn validate(
        &self,
        workflow: &ObjectSchema,
        outputs: &[&ObjectSchema],
        body_inputs: &ObjectSchema,
        scopes: &super::scope_data::ScopeSchemas,
    ) -> Result<(), String> {
        if self.max_items == 0 || self.max_concurrency == 0 || self.max_concurrency > self.max_items
        {
            return Err("collection requires positive item/concurrency bounds".into());
        }
        if self
            .collection
            .schema(workflow, outputs, scopes)?
            .value_type
            != ValueType::Array
        {
            return Err("collection source must be an array".into());
        }
        if self.item_input == self.index_input {
            return Err("collection item and index inputs must differ".into());
        }
        for (name, kind) in [
            (&self.item_input, &self.item_type),
            (&self.index_input, &ValueType::Integer),
        ] {
            if body_inputs.fields.get(name).map(|f| &f.value_type) != Some(kind) {
                return Err(format!(
                    "collection scoped input {name} has incompatible type"
                ));
            }
        }
        if self.collection.reads_activity(self.body) {
            return Err("collection body cannot produce its own source".into());
        }
        // Constants can be checked completely before publication.
        if let DataSource::Constant { value } = &self.collection {
            self.check_items(value)?;
        }
        Ok(())
    }

    fn check_items<'a>(&self, value: &'a Value) -> Result<&'a [Value], String> {
        let values = value
            .as_array()
            .ok_or("collection source is not an array")?;
        if values.len() > self.max_items as usize {
            return Err("collection exceeds max-items".into());
        }
        if values.iter().any(|value| !self.item_type.matches(value)) {
            return Err("collection item has wrong type".into());
        }
        Ok(values)
    }

    pub(crate) fn expand(
        &self,
        identity: CollectionIdentity,
        workflow: &BTreeMap<String, Datum>,
        outputs: &BTreeMap<usize, BTreeMap<String, Datum>>,
        scopes: &super::scope_data::ScopeValues,
    ) -> Result<Expansion, String> {
        if identity.run.is_empty() || identity.scope.is_empty() || identity.generation == 0 {
            return Err("collection requires run/scope/generation identity".into());
        }
        let source = self.collection.resolve(workflow, outputs, scopes);
        let values = match &source.state {
            DataState::Unknown => return Err("collection source is unknown".into()),
            DataState::Absent => return Err("collection source is absent".into()),
            DataState::Present(value) => self.check_items(value)?,
        };
        let items = values
            .iter()
            .enumerate()
            .map(|(index, value)| CollectionItem {
                identity: ItemIdentity {
                    collection: identity.clone(),
                    index: index as u32,
                },
                inputs: BTreeMap::from([
                    (
                        self.item_input.clone(),
                        Datum {
                            state: DataState::Present(value.clone()),
                            provenance: source.provenance.clone(),
                        },
                    ),
                    (
                        self.index_input.clone(),
                        Datum {
                            state: DataState::Present(json!(index)),
                            provenance: source.provenance.clone(),
                        },
                    ),
                ]),
            })
            .collect();
        Ok(Expansion { identity, items })
    }

    /// A pure snapshot fold. It cannot release joins or persist callbacks; the
    /// durable owner must fence activation/attempt identity before calling it.
    // DEFERRED (2026-11-10) — 004 must carry and check each item's current
    // activation/attempt owner; ItemIdentity alone cannot reject a replaced attempt.
    pub(crate) fn aggregate(
        &self,
        expansion: &Expansion,
        results: &[ItemResult],
        schema: &ObjectSchema,
        outcomes: &BTreeSet<String>,
    ) -> Result<Aggregate, String> {
        let mut items: Vec<_> = expansion
            .items
            .iter()
            .map(|item| ItemResult {
                identity: item.identity.clone(),
                state: ItemState::Pending,
            })
            .collect();
        let mut seen = BTreeSet::new();
        for result in results {
            if result.identity.collection != expansion.identity {
                return Err("collection result belongs to another run/scope/generation".into());
            }
            let index = result.identity.index as usize;
            if !seen.insert(index) {
                return Err("duplicate collection item result".into());
            }
            let slot = items
                .get_mut(index)
                .ok_or("collection result index out of range")?;
            if let ItemState::Completed { outcome, outputs } = &result.state {
                if !outcomes.contains(outcome) {
                    return Err("collection item has unknown outcome".into());
                }
                let mut present = BTreeMap::new();
                for (name, datum) in outputs {
                    if !schema.fields.contains_key(name) {
                        return Err("collection item has unknown output field".into());
                    }
                    match &datum.state {
                        DataState::Unknown => {
                            return Err("completed item has unknown output".into());
                        }
                        DataState::Absent => {}
                        DataState::Present(value) => {
                            present.insert(name.clone(), value.clone());
                        }
                    }
                }
                // A completed snapshot must contain required outputs even when
                // the descriptor has defaults. Optional omissions stay absent.
                for (name, field) in &schema.fields {
                    if field.required && !present.contains_key(name) {
                        return Err(format!("collection result.{name}: required output absent"));
                    }
                }
                schema.resolve(&present, "collection result")?;
                let mut fields = outputs.clone();
                for name in schema.fields.keys() {
                    fields.entry(name.clone()).or_insert_with(|| Datum {
                        state: DataState::Absent,
                        provenance: BTreeSet::new(),
                    });
                }
                slot.state = ItemState::Completed {
                    outcome: outcome.clone(),
                    outputs: fields,
                };
            } else {
                slot.state = result.state.clone();
            }
        }
        let state = if items.iter().any(|item| item.state == ItemState::Pending) {
            AggregateState::Pending
        } else if (items.is_empty() && self.empty == EmptyPolicy::Blocked)
            || (self.failure == FailurePolicy::Blocked
                && items.iter().any(|item| {
                    matches!(
                        item.state,
                        ItemState::Failed { .. } | ItemState::Skipped { .. }
                    )
                }))
        {
            AggregateState::Blocked
        } else {
            AggregateState::Complete
        };
        Ok(Aggregate {
            identity: expansion.identity.clone(),
            state,
            items,
        })
    }
}

impl Aggregate {
    const COUNT_FIELDS: [&str; 4] = ["total", "completed", "failed", "skipped"];

    pub(crate) fn summary_schema() -> ObjectSchema {
        let mut fields = BTreeMap::new();
        for name in Self::COUNT_FIELDS {
            fields.insert(
                name.into(),
                FieldSchema {
                    value_type: ValueType::Integer,
                    required: true,
                    default: None,
                },
            );
        }
        fields.insert(
            "state".into(),
            FieldSchema {
                value_type: ValueType::String,
                required: true,
                default: None,
            },
        );
        ObjectSchema { fields }
    }

    /// Final typed metadata for ordinary predicates. Pending results cannot be
    /// normalized into final counts or a false/success branch.
    pub(crate) fn summary(&self) -> Result<ResolvedInputs, String> {
        if self.state == AggregateState::Pending {
            return Err("aggregate is pending".into());
        }
        let mut values: BTreeMap<String, Value> = Self::COUNT_FIELDS
            .into_iter()
            .map(|name| (name.into(), json!(0)))
            .collect();
        values.insert("total".into(), json!(self.items.len()));
        values.insert(
            "state".into(),
            json!(match self.state {
                AggregateState::Complete => "complete",
                AggregateState::Blocked => "blocked",
                AggregateState::Pending => unreachable!(),
            }),
        );
        let mut provenance = BTreeSet::from([DataOrigin::ScopeResult {
            identity: self.identity.clone(),
        }]);
        for item in &self.items {
            let key = match &item.state {
                ItemState::Completed { outputs, .. } => {
                    for datum in outputs.values() {
                        provenance.extend(datum.provenance.iter().cloned());
                    }
                    "completed"
                }
                ItemState::Failed { .. } => "failed",
                ItemState::Skipped { .. } => "skipped",
                ItemState::Pending => return Err("aggregate contains a pending item".into()),
            };
            let count = values[key].as_u64().expect("summary count is an integer");
            values.insert(key.into(), json!(count + 1));
        }
        let values = Self::summary_schema().resolve(&values, "aggregate summary")?;
        let provenance = values
            .keys()
            .map(|name| (name.clone(), provenance.clone()))
            .collect();
        Ok(ResolvedInputs { values, provenance })
    }
}
