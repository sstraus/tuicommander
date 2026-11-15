//! Closed, typed predicates over the same immutable sources as input bindings.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "003-e678 contract; first consumer 004-2809")
)]

use super::catalog::{ObjectSchema, ValueType};
use super::data_binding::{DataOrigin, DataSource, DataState, Datum};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Comparison {
    Equal,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operator", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Condition {
    Exists {
        value: DataSource,
    },
    Compare {
        comparison: Comparison,
        left: DataSource,
        right: DataSource,
    },
    Contains {
        collection: DataSource,
        value: DataSource,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Decision {
    True,
    False,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Evaluation {
    pub decision: Decision,
    pub provenance: BTreeSet<DataOrigin>,
}

fn numeric(kind: &ValueType) -> bool {
    matches!(kind, ValueType::Integer | ValueType::Number)
}

impl Condition {
    pub(crate) fn validate(
        &self,
        workflow: &ObjectSchema,
        outputs: &[&ObjectSchema],
        scopes: &super::scope_data::ScopeSchemas,
    ) -> Result<(), String> {
        match self {
            Self::Exists { value } => {
                value.schema(workflow, outputs, scopes)?;
            }
            Self::Contains { collection, value } => {
                if collection.schema(workflow, outputs, scopes)?.value_type != ValueType::Array {
                    return Err("membership requires an array".into());
                }
                value.schema(workflow, outputs, scopes)?;
            }
            Self::Compare {
                comparison,
                left,
                right,
            } => {
                let a = left.schema(workflow, outputs, scopes)?.value_type;
                let b = right.schema(workflow, outputs, scopes)?.value_type;
                let numbers = numeric(&a) && numeric(&b);
                if a != b && !numbers {
                    return Err("condition operands have incompatible types".into());
                }
                if *comparison != Comparison::Equal && !numbers && a != ValueType::String {
                    return Err("ordered comparison requires numbers or strings".into());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn evaluate(
        &self,
        workflow: &BTreeMap<String, Datum>,
        outputs: &BTreeMap<usize, BTreeMap<String, Datum>>,
        scopes: &super::scope_data::ScopeValues,
    ) -> Result<Evaluation, String> {
        let resolve = |source: &DataSource| source.resolve(workflow, outputs, scopes);
        let (decision, provenance) = match self {
            Self::Exists { value } => {
                let datum = resolve(value);
                let decision = match datum.state {
                    DataState::Unknown => Decision::Unknown,
                    DataState::Absent => Decision::False,
                    DataState::Present(_) => Decision::True,
                };
                (decision, datum.provenance)
            }
            Self::Compare {
                comparison,
                left,
                right,
            } => {
                let a = resolve(left);
                let b = resolve(right);
                let provenance = a.provenance.union(&b.provenance).cloned().collect();
                let decision = match (a.state, b.state) {
                    (DataState::Present(a), DataState::Present(b)) => {
                        let yes = if *comparison == Comparison::Equal {
                            equal(&a, &b)?
                        } else {
                            let order = ordered(&a, &b)?;
                            match comparison {
                                Comparison::Equal => unreachable!(),
                                Comparison::Less => order == Ordering::Less,
                                Comparison::LessOrEqual => order != Ordering::Greater,
                                Comparison::Greater => order == Ordering::Greater,
                                Comparison::GreaterOrEqual => order != Ordering::Less,
                            }
                        };
                        if yes { Decision::True } else { Decision::False }
                    }
                    _ => Decision::Unknown,
                };
                (decision, provenance)
            }
            Self::Contains { collection, value } => {
                let a = resolve(collection);
                let b = resolve(value);
                let provenance = a.provenance.union(&b.provenance).cloned().collect();
                let decision = match (a.state, b.state) {
                    (DataState::Present(Value::Array(items)), DataState::Present(value)) => {
                        // Array elements are opaque in the bounded shallow schema.
                        // Different JSON kinds are simply not members, not errors.
                        let found = items
                            .iter()
                            .any(|item| equal(item, &value).unwrap_or(false));
                        if found {
                            Decision::True
                        } else {
                            Decision::False
                        }
                    }
                    (DataState::Present(_), DataState::Present(_)) => {
                        return Err("membership requires an array".into());
                    }
                    _ => Decision::Unknown,
                };
                (decision, provenance)
            }
        };
        Ok(Evaluation {
            decision,
            provenance,
        })
    }
}

fn equal(a: &Value, b: &Value) -> Result<bool, String> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => Ok(compare_numbers(a, b) == Ordering::Equal),
        _ if std::mem::discriminant(a) == std::mem::discriminant(b) => Ok(a == b),
        _ => Err("condition operands have incompatible types".into()),
    }
}

fn ordered(a: &Value, b: &Value) -> Result<Ordering, String> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => Ok(compare_numbers(a, b)),
        (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
        _ => Err("ordered comparison requires numbers or strings".into()),
    }
}

// Never round two large integer identities through f64. For a mixed comparison,
// distinct floating values preserve ordering; when conversion ties, compare the
// original integer to the exact integral float. JSON numbers are finite.
fn compare_numbers(a: &Number, b: &Number) -> Ordering {
    let integer = |n: &Number| {
        n.as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
    };
    let mixed = |integer: i128, float: f64| {
        let approximate = integer as f64;
        match approximate.partial_cmp(&float).expect("finite JSON number") {
            Ordering::Equal => integer.cmp(&(float as i128)),
            order => order,
        }
    };
    match (integer(a), integer(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(a), None) => mixed(a, b.as_f64().expect("JSON number")),
        (None, Some(b)) => mixed(b, a.as_f64().expect("JSON number")).reverse(),
        (None, None) => a
            .as_f64()
            .unwrap()
            .partial_cmp(&b.as_f64().unwrap())
            .expect("finite JSON numbers"),
    }
}
