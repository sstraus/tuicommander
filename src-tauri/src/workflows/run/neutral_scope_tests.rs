use super::neutral_execution::{
    NeutralAttemptFence, NeutralExecution, NeutralExecutionCommand as Command, NeutralStatus,
};
use super::neutral_execution_tests::{fixture_with, fixture_with_concurrency, step};
use super::runtime::RuntimeOwner;
use crate::workflows::catalog::{DescriptorId, FieldSchema, ObjectSchema, ValueType};
use crate::workflows::collection::{EmptyPolicy, FailurePolicy, ForEach};
use crate::workflows::condition::{Comparison, Condition};
use crate::workflows::data_binding::{DataSource, DataState};
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn field(kind: ValueType, required: bool) -> FieldSchema {
    FieldSchema {
        value_type: kind,
        required,
        default: None,
    }
}

pub(super) fn fixture(
    kind: &str,
    concurrency: u32,
) -> (
    tempfile::TempDir,
    std::sync::Arc<RuntimeOwner>,
    NeutralExecution,
    i64,
) {
    fixture_with_concurrency(40, false, concurrency, |definition, catalog, _| {
        let mut descriptor = catalog
            .get(&definition.instances[0].descriptor)
            .unwrap()
            .clone();
        descriptor.id = DescriptorId {
            name: "scope-body".into(),
            ..descriptor.id
        };
        descriptor.inputs = ObjectSchema {
            fields: BTreeMap::from([("value".into(), field(ValueType::Integer, true))]),
        };
        if kind == "collection" {
            descriptor
                .inputs
                .fields
                .insert("index".into(), field(ValueType::Integer, true));
        }
        descriptor
            .inputs
            .fields
            .insert("note".into(), field(ValueType::String, false));
        definition.instances[0].descriptor = descriptor.id.clone();
        catalog.register(descriptor).unwrap();
        definition.outputs = ObjectSchema::default();
        definition.output_bindings.clear();
        definition.input_bindings.clear();
        definition.conditions.clear();
        definition.outcomes = ["done".into()].into();
        let (node, ports) = match kind {
            "collection" => {
                definition.collections.insert(
                    "scope".into(),
                    ForEach {
                        collection: DataSource::Constant {
                            value: json!([30, 10, 20]),
                        },
                        item_type: ValueType::Integer,
                        body: 0,
                        item_input: "value".into(),
                        index_input: "index".into(),
                        max_items: 3,
                        max_concurrency: 2,
                        empty: EmptyPolicy::Complete,
                        failure: FailurePolicy::Collect,
                    },
                );
                definition
                    .outputs
                    .fields
                    .insert("items".into(), field(ValueType::Array, true));
                definition.output_bindings.insert(
                    "items".into(),
                    DataSource::ScopeOutput {
                        scope: "scope".into(),
                        field: "items".into(),
                    },
                );
                (
                    ControlNode::Collection {
                        scope: "scope".into(),
                    },
                    vec!["complete", "blocked"],
                )
            }
            "loop" => {
                definition.loops.insert(
                    "scope".into(),
                    crate::workflows::loop_data::LoopContract {
                        body: 0,
                        max_iterations: 3,
                        carried: ObjectSchema {
                            fields: BTreeMap::from([
                                ("value".into(), field(ValueType::Integer, true)),
                                ("note".into(), field(ValueType::String, false)),
                            ]),
                        },
                        initial: BTreeMap::from([
                            ("value".into(), DataSource::Constant { value: json!(0) }),
                            (
                                "note".into(),
                                DataSource::Constant {
                                    value: json!("initial"),
                                },
                            ),
                        ]),
                        result: BTreeMap::from([
                            (
                                "value".into(),
                                DataSource::ActivityOutput {
                                    activity: 0,
                                    field: "next".into(),
                                },
                            ),
                            (
                                "note".into(),
                                DataSource::ActivityOutput {
                                    activity: 0,
                                    field: "note".into(),
                                },
                            ),
                        ]),
                    },
                );
                definition.conditions.insert(
                    "repeat".into(),
                    Condition::Compare {
                        comparison: Comparison::Less,
                        left: DataSource::ScopeOutput {
                            scope: "scope".into(),
                            field: "value".into(),
                        },
                        right: DataSource::Constant { value: json!(2) },
                    },
                );
                definition
                    .outputs
                    .fields
                    .insert("value".into(), field(ValueType::Integer, true));
                definition.output_bindings.insert(
                    "value".into(),
                    DataSource::ScopeOutput {
                        scope: "scope".into(),
                        field: "value".into(),
                    },
                );
                (
                    ControlNode::Loop {
                        scope: "scope".into(),
                        condition: "repeat".into(),
                    },
                    vec!["complete", "exhausted", "unknown", "failed"],
                )
            }
            _ => unreachable!(),
        };
        definition.control = ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                ("scope".into(), node),
                (
                    "end".into(),
                    ControlNode::End {
                        outcome: "done".into(),
                    },
                ),
            ]),
            edges: BTreeMap::from([
                (
                    "start".into(),
                    BTreeMap::from([("next".into(), "scope".into())]),
                ),
                (
                    "scope".into(),
                    ports
                        .into_iter()
                        .map(|port| (port.into(), "end".into()))
                        .collect(),
                ),
            ]),
        };
    })
}

fn complete(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    fence: NeutralAttemptFence,
    value: i64,
    at: i64,
) -> NeutralExecution {
    owner
        .neutral_command_at(
            &run.run.id,
            &format!("complete:{}", fence.activation),
            run.sequence,
            Command::Complete {
                fence,
                outcome: "ok".into(),
                outputs: BTreeMap::from([("next".into(), json!(value))]),
            },
            at,
        )
        .unwrap()
}

// Catches: completion-order aggregates, scope/run concurrency bypass, accepting a
// foreign generation/item, and duplicate join release after owner restart.
#[test]
fn collection_owner_fences_results_and_replays_ordered_join_once() {
    let (dir, owner, run, at) = fixture("collection", 2);
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "expand", at);
    let run = step(&owner, &run, "first", at);
    let run = step(&owner, &run, "second", at);
    assert_eq!(step(&owner, &run, "bounded", at), run);
    let first = run.active_scope.as_ref().unwrap().slots["0"]
        .attempt
        .as_ref()
        .unwrap();
    assert_eq!(first.inputs["value"], json!(30));
    let mut foreign = first.fence.clone();
    foreign.scope.as_mut().unwrap().identity.generation += 1;
    assert!(
        owner
            .neutral_command_at(
                &run.run.id,
                "foreign",
                run.sequence,
                Command::Complete {
                    fence: foreign,
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([("next".into(), json!(99))])
                },
                at
            )
            .is_err()
    );
    let first = first.fence.clone();
    let second = run.active_scope.as_ref().unwrap().slots["1"]
        .attempt
        .as_ref()
        .unwrap()
        .fence
        .clone();
    let run = complete(&owner, &run, second, 11, at);
    let run = complete(&owner, &run, first, 31, at);
    let run = step(&owner, &run, "third", at);
    let third = run.active_scope.as_ref().unwrap().slots["2"]
        .attempt
        .as_ref()
        .unwrap()
        .fence
        .clone();
    let run = complete(&owner, &run, third, 21, at);
    assert!(
        !run.scope_values.contains_key("scope"),
        "pending scope is not published"
    );
    let run = step(&owner, &run, "join", at);
    let items = match &run.scope_values["scope"]["items"].state {
        DataState::Present(Value::Array(items)) => items,
        _ => panic!("ordered items missing"),
    };
    assert_eq!(
        items
            .iter()
            .map(|item| item["outputs"]["next"].clone())
            .collect::<Vec<_>>(),
        vec![json!(31), json!(11), json!(21)]
    );
    drop(owner);
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
    let routed = step(&owner, &run, "release", at);
    let finished = step(&owner, &routed, "finish", at);
    assert_eq!(
        finished.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(
        finished.outputs.unwrap().values["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        owner
            .neutral_command_at(&run.run.id, "join", 0, Command::Step, at)
            .unwrap(),
        run
    );
    let (_dir, serial_owner, serial, at) = fixture("collection", 1);
    let serial = step(&serial_owner, &serial, "enter", at);
    let serial = step(&serial_owner, &serial, "open", at);
    let serial = step(&serial_owner, &serial, "first", at);
    assert_eq!(
        step(&serial_owner, &serial, "run-bound", at),
        serial,
        "run concurrency one overrides scope concurrency two"
    );
}

// Catches: a loop condition seeing the old/missing carried result, an omitted
// optional field inheriting its prior value, and a late iteration overwriting it.
#[test]
fn loop_owner_tests_current_carried_snapshot_and_rejects_previous_iteration() {
    let (_dir, owner, run, at) = fixture("loop", 1);
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "initial", at);
    let run = step(&owner, &run, "prepare-one", at);
    let one = run.active_scope.as_ref().unwrap().slots["1"]
        .attempt
        .as_ref()
        .unwrap();
    assert_eq!(one.inputs["value"], json!(0));
    assert_eq!(one.inputs["note"], json!("initial"));
    let old = one.fence.clone();
    let run = complete(&owner, &run, old.clone(), 1, at);
    let run = step(&owner, &run, "repeat", at);
    assert_eq!(
        run.scope_values["scope"]["value"].state,
        DataState::Present(json!(1))
    );
    let run = step(&owner, &run, "prepare-two", at);
    let two = run.active_scope.as_ref().unwrap().slots["2"]
        .attempt
        .as_ref()
        .unwrap();
    assert_eq!(two.inputs, BTreeMap::from([("value".into(), json!(1))]));
    let run = complete(&owner, &run, two.fence.clone(), 2, at);
    assert!(
        owner
            .neutral_command_at(
                &run.run.id,
                "old-result",
                run.sequence,
                Command::Complete {
                    fence: old,
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([("next".into(), json!(88))])
                },
                at
            )
            .is_err()
    );
    let run = step(&owner, &run, "settle", at);
    assert_eq!(
        run.active_scope.as_ref().unwrap().settled.as_deref(),
        Some("complete")
    );
    assert_eq!(run.scope_values["scope"]["note"].state, DataState::Absent);
    let run = step(&owner, &run, "route", at);
    let run = step(&owner, &run, "finish", at);
    assert_eq!(
        run.outputs.unwrap().values,
        BTreeMap::from([("value".into(), json!(2))])
    );
}

// Catches: declaring a fork joined while a required branch is pending/failed,
// or resolving a sibling binding from a stale/root output instead of its owner.
#[test]
fn fork_owner_waits_for_current_producers_and_retains_failed_arrival() {
    use crate::workflows::fork_scope::{BranchContract, ForkContract};
    let (_dir, owner, run, at) = fixture_with(30, false, |definition, _, _| {
        definition.instances.push(definition.instances[0].clone());
        definition.input_bindings.insert(
            1,
            BTreeMap::from([(
                "value".into(),
                DataSource::ActivityOutput {
                    activity: 0,
                    field: "next".into(),
                },
            )]),
        );
        definition.conditions.clear();
        definition.outputs = ObjectSchema::default();
        definition.output_bindings.clear();
        definition.outcomes = ["done".into()].into();
        definition.forks.insert(
            "fork".into(),
            ForkContract {
                branches: BTreeMap::from([
                    (
                        "z-producer".into(),
                        BranchContract {
                            activity: 0,
                            required: true,
                        },
                    ),
                    (
                        "a-consumer".into(),
                        BranchContract {
                            activity: 1,
                            required: true,
                        },
                    ),
                ]),
                max_concurrency: 2,
            },
        );
        definition.control = ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                (
                    "fork".into(),
                    ControlNode::Fork {
                        scope: "fork".into(),
                    },
                ),
                (
                    "end".into(),
                    ControlNode::End {
                        outcome: "done".into(),
                    },
                ),
            ]),
            edges: BTreeMap::from([
                (
                    "start".into(),
                    BTreeMap::from([("next".into(), "fork".into())]),
                ),
                (
                    "fork".into(),
                    BTreeMap::from([
                        ("complete".into(), "end".into()),
                        ("blocked".into(), "end".into()),
                    ]),
                ),
            ]),
        };
    });
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "open", at);
    let run = step(&owner, &run, "producer", at);
    assert!(
        run.active_scope.as_ref().unwrap().slots["a-consumer"]
            .attempt
            .is_none()
    );
    let fence = run.active_scope.as_ref().unwrap().slots["z-producer"]
        .attempt
        .as_ref()
        .unwrap()
        .fence
        .clone();
    let run = complete(&owner, &run, fence, 7, at);
    let run = step(&owner, &run, "consumer", at);
    let attempt = run.active_scope.as_ref().unwrap().slots["a-consumer"]
        .attempt
        .as_ref()
        .unwrap();
    assert_eq!(attempt.inputs["value"], json!(7));
    assert!(!run.scope_values.contains_key("fork"));
    let run = owner
        .neutral_command_at(
            &run.run.id,
            "failure",
            run.sequence,
            Command::Fail {
                fence: attempt.fence.clone(),
                diagnostic: "host failed".into(),
            },
            at,
        )
        .unwrap();
    let run = step(&owner, &run, "join", at);
    assert_eq!(
        run.active_scope.as_ref().unwrap().settled.as_deref(),
        Some("blocked")
    );
    assert_eq!(
        run.scope_values["fork"]["state"].state,
        DataState::Present(json!("blocked"))
    );
    let run = step(&owner, &run, "release", at);
    let run = step(&owner, &run, "end", at);
    assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
}
