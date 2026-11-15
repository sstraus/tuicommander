//! Declared scope results and their ordinary data-dependency producers.
//! The run owner must fence attempts before exposing any scope snapshot.

use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use super::collection::Aggregate;
use super::data_binding::Datum;
use super::neutral_publication::NeutralDefinition;
use std::collections::BTreeMap;

pub(crate) type ScopeSchemas = BTreeMap<String, ObjectSchema>;
pub(crate) type ScopeValues = BTreeMap<String, BTreeMap<String, Datum>>;

pub(crate) fn schemas(definition: &NeutralDefinition) -> Result<ScopeSchemas, String> {
    let mut schemas = BTreeMap::new();
    for scope in definition.collections.keys() {
        let mut schema = Aggregate::summary_schema();
        schema
            .fields
            .insert("items".into(), field(ValueType::Array));
        insert(&mut schemas, scope, schema)?;
    }
    for scope in definition.forks.keys() {
        insert(
            &mut schemas,
            scope,
            ObjectSchema {
                fields: BTreeMap::from([
                    ("state".into(), field(ValueType::String)),
                    ("branches".into(), field(ValueType::Object)),
                ]),
            },
        )?;
    }
    for (scope, contract) in &definition.loops {
        insert(&mut schemas, scope, contract.carried.clone())?;
    }
    Ok(schemas)
}

fn field(value_type: ValueType) -> FieldSchema {
    FieldSchema {
        value_type,
        required: true,
        default: None,
    }
}

fn insert(schemas: &mut ScopeSchemas, scope: &str, schema: ObjectSchema) -> Result<(), String> {
    if scope.trim().is_empty() || schemas.insert(scope.into(), schema).is_some() {
        return Err("scope output requires a unique nonempty scope identity".into());
    }
    Ok(())
}

/// A scope output depends on all its bodies, not whichever finishes first.
/// This is declaration validation only, never completion or release evidence.
pub(crate) fn producers(definition: &NeutralDefinition, scope: &str) -> Result<Vec<usize>, String> {
    if let Some(contract) = definition.collections.get(scope) {
        return Ok(vec![contract.body]);
    }
    if let Some(contract) = definition.loops.get(scope) {
        return Ok(vec![contract.body]);
    }
    if let Some(contract) = definition.forks.get(scope) {
        return Ok(contract
            .branches
            .values()
            .map(|branch| branch.activity)
            .collect());
    }
    Err(format!("unknown scope output producer {scope}"))
}

/// The owner inserts only a settled snapshot. A pending scope has no entry, so
/// every reference to it resolves Unknown rather than exposing partial counts.
pub(crate) fn collection_snapshot(
    aggregate: &Aggregate,
) -> Result<BTreeMap<String, Datum>, String> {
    use super::collection::ItemState;
    use super::data_binding::DataState;
    use serde_json::json;
    let mut summary = aggregate.summary()?;
    let mut items = Vec::new();
    for item in &aggregate.items {
        let index = item.identity.index;
        let value = match &item.state {
            ItemState::Pending => return Err("aggregate contains a pending item".into()),
            ItemState::Failed { diagnostic } => {
                json!({"index":index,"state":"failed","diagnostic":diagnostic})
            }
            ItemState::Skipped { reason } => {
                json!({"index":index,"state":"skipped","reason":reason})
            }
            ItemState::Completed { outcome, outputs } => {
                let mut present = BTreeMap::new();
                let mut absent = Vec::new();
                for (name, datum) in outputs {
                    match &datum.state {
                        DataState::Present(value) => {
                            present.insert(name.clone(), value.clone());
                        }
                        DataState::Absent => absent.push(name.clone()),
                        DataState::Unknown => {
                            return Err("completed item has unknown output".into());
                        }
                    }
                }
                json!({"index":index,"state":"completed","outcome":outcome,"outputs":present,"absent":absent})
            }
        };
        items.push(value);
    }
    let provenance = summary.provenance["total"].clone();
    summary.values.insert("items".into(), json!(items));
    summary.provenance.insert("items".into(), provenance);
    Ok(snapshot(summary, &aggregate.identity))
}

pub(crate) fn loop_snapshot(
    contract: &super::loop_data::LoopContract,
    result: &super::loop_data::LoopResult,
) -> BTreeMap<String, Datum> {
    let mut fields = snapshot(result.fields.clone(), &result.identity);
    for name in contract.carried.fields.keys() {
        fields.entry(name.clone()).or_insert_with(|| Datum {
            state: super::data_binding::DataState::Absent,
            provenance: std::collections::BTreeSet::from([
                super::data_binding::DataOrigin::ScopeResult {
                    identity: result.identity.clone(),
                },
            ]),
        });
    }
    fields
}

pub(crate) fn fork_snapshot(
    join: &super::fork_scope::JoinSnapshot,
) -> Result<BTreeMap<String, Datum>, String> {
    use super::collection::AggregateState;
    use super::fork_scope::ArrivalState;
    use serde_json::json;
    let state = match join.state {
        AggregateState::Pending => return Err("join is pending".into()),
        AggregateState::Complete => "complete",
        AggregateState::Blocked => "blocked",
    };
    let branches: BTreeMap<_, _> = join
        .arrivals
        .iter()
        .map(|(branch, arrival)| {
            (
                branch.clone(),
                match arrival {
                    ArrivalState::Completed => "completed",
                    ArrivalState::Failed => "failed",
                    ArrivalState::Cancelled => "cancelled",
                    ArrivalState::Skipped => "skipped",
                },
            )
        })
        .collect();
    Ok(snapshot(
        super::data_binding::ResolvedInputs {
            values: BTreeMap::from([
                ("state".into(), json!(state)),
                ("branches".into(), json!(branches)),
            ]),
            provenance: BTreeMap::new(),
        },
        &join.identity,
    ))
}

fn snapshot(
    fields: super::data_binding::ResolvedInputs,
    identity: &super::collection::CollectionIdentity,
) -> BTreeMap<String, Datum> {
    use super::data_binding::{DataOrigin, DataState};
    fields
        .values
        .into_iter()
        .map(|(name, value)| {
            let mut provenance = fields.provenance.get(&name).cloned().unwrap_or_default();
            provenance.insert(DataOrigin::ScopeResult {
                identity: identity.clone(),
            });
            (
                name,
                Datum {
                    state: DataState::Present(value),
                    provenance,
                },
            )
        })
        .collect()
}
