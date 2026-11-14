use super::neutral::{NeutralLimits, NeutralStartRequest};
use super::neutral_execution::{
    NeutralExecution, NeutralExecutionCommand as Command, NeutralStatus,
};
use super::runtime::RuntimeOwner;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ValueType, WorkflowCatalog, content_digest,
};
use crate::workflows::condition::{Comparison, Condition};
use crate::workflows::data_binding::{DataSource, DataState};
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use crate::workflows::neutral_publication::{NeutralDefinition, PublicationRef};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn integer() -> FieldSchema {
    FieldSchema {
        value_type: ValueType::Integer,
        required: true,
        default: None,
    }
}

pub(super) fn fixture(
    max_activations: u32,
    unknown_first: bool,
) -> (
    tempfile::TempDir,
    std::sync::Arc<RuntimeOwner>,
    NeutralExecution,
    i64,
) {
    fixture_with(max_activations, unknown_first, |_, _, _| {})
}

pub(super) fn fixture_with(
    max_activations: u32,
    unknown_first: bool,
    change: impl FnOnce(&mut NeutralDefinition, &mut WorkflowCatalog, &WorkflowStore),
) -> (
    tempfile::TempDir,
    std::sync::Arc<RuntimeOwner>,
    NeutralExecution,
    i64,
) {
    fixture_with_concurrency(max_activations, unknown_first, 1, change)
}

pub(super) fn fixture_with_concurrency(
    max_activations: u32,
    unknown_first: bool,
    concurrency: u32,
    change: impl FnOnce(&mut NeutralDefinition, &mut WorkflowCatalog, &WorkflowStore),
) -> (
    tempfile::TempDir,
    std::sync::Arc<RuntimeOwner>,
    NeutralExecution,
    i64,
) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let workflows = WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let descriptor = DescriptorId {
        namespace: "test".into(),
        name: "increment".into(),
        version: 1,
    };
    let handler = HandlerRef {
        name: "host.increment".into(),
        version: 1,
    };
    let mut catalog = WorkflowCatalog::new();
    catalog
        .register(ActivityDescriptor {
            id: descriptor.clone(),
            purpose: "Increment a typed value".into(),
            handler: handler.clone(),
            target: None,
            config: ObjectSchema::default(),
            inputs: ObjectSchema {
                fields: BTreeMap::from([("value".into(), integer())]),
            },
            subject: None,
            outputs: ObjectSchema {
                fields: BTreeMap::from([
                    ("next".into(), integer()),
                    (
                        "note".into(),
                        FieldSchema {
                            value_type: ValueType::String,
                            required: false,
                            default: None,
                        },
                    ),
                ]),
            },
            outcomes: BTreeSet::from(["ok".into()]),
            required_capabilities: BTreeSet::new(),
            execution_policies: BTreeSet::from(["local".into()]),
        })
        .unwrap();
    let mut definition = NeutralDefinition {
        control: ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                ("work".into(), ControlNode::Activity { activity: 0 }),
                (
                    "condition".into(),
                    ControlNode::Condition {
                        condition: "is-seven".into(),
                    },
                ),
                ("wait".into(), ControlNode::Wait { duration_ms: 5 }),
                (
                    "done".into(),
                    ControlNode::End {
                        outcome: "done".into(),
                    },
                ),
                (
                    "reject".into(),
                    ControlNode::End {
                        outcome: "reject".into(),
                    },
                ),
                (
                    "unknown".into(),
                    ControlNode::End {
                        outcome: "unknown".into(),
                    },
                ),
            ]),
            edges: BTreeMap::from([
                (
                    "start".into(),
                    BTreeMap::from([("next".into(), "work".into())]),
                ),
                (
                    "work".into(),
                    BTreeMap::from([("ok".into(), "condition".into())]),
                ),
                (
                    "condition".into(),
                    BTreeMap::from([
                        ("true".into(), "wait".into()),
                        ("false".into(), "reject".into()),
                        ("unknown".into(), "unknown".into()),
                    ]),
                ),
                (
                    "wait".into(),
                    BTreeMap::from([("next".into(), "done".into())]),
                ),
            ]),
        },
        inputs: ObjectSchema {
            fields: BTreeMap::from([("initial".into(), integer())]),
        },
        subject: None,
        outputs: ObjectSchema {
            fields: BTreeMap::from([
                ("result".into(), integer()),
                (
                    "note".into(),
                    FieldSchema {
                        value_type: ValueType::String,
                        required: false,
                        default: None,
                    },
                ),
            ]),
        },
        outcomes: BTreeSet::from(["done".into(), "reject".into(), "unknown".into()]),
        instances: vec![ActivityInstance {
            descriptor,
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        }],
        input_bindings: BTreeMap::from([(
            0,
            BTreeMap::from([(
                "value".into(),
                DataSource::WorkflowInput {
                    field: "initial".into(),
                },
            )]),
        )]),
        conditions: BTreeMap::from([(
            "is-seven".into(),
            Condition::Compare {
                comparison: Comparison::Equal,
                left: DataSource::ActivityOutput {
                    activity: 0,
                    field: "next".into(),
                },
                right: DataSource::Constant { value: json!(7) },
            },
        )]),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        subflows: vec![],
        output_bindings: BTreeMap::from([
            (
                "result".into(),
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
    };
    if unknown_first {
        definition
            .control
            .edges
            .get_mut("start")
            .unwrap()
            .insert("next".into(), "condition".into());
        definition
            .control
            .edges
            .get_mut("condition")
            .unwrap()
            .insert("true".into(), "work".into());
        definition
            .control
            .edges
            .get_mut("work")
            .unwrap()
            .insert("ok".into(), "wait".into());
        definition.outputs = ObjectSchema::default();
        definition.output_bindings.clear();
    }
    change(&mut definition, &mut catalog, &workflows);
    let mut handlers = BTreeSet::from([handler]);
    handlers.extend(
        definition
            .instances
            .iter()
            .map(|instance| catalog.get(&instance.descriptor).unwrap().handler.clone()),
    );
    let publication = workflows
        .save_neutral_publication("/project", "execution", 0, definition, &catalog, &handlers)
        .unwrap();
    let request = NeutralStartRequest {
        publication: PublicationRef {
            project: "/project".into(),
            id: publication.id,
            revision: publication.revision,
        },
        request_id: "start".into(),
        inputs: BTreeMap::from([("initial".into(), json!(6))]),
        subject: None,
        limits: NeutralLimits {
            max_activations,
            max_concurrency: concurrency,
            max_duration_ms: 60_000,
        },
    };
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let run = owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    let execution = owner.store.neutral_execution(&run.id).unwrap();
    let at = execution.deadline_ms - 60_000;
    (dir, owner, execution, at)
}

pub(super) fn step(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    id: &str,
    at: i64,
) -> NeutralExecution {
    owner
        .neutral_command_at(&run.run.id, id, run.sequence, Command::Step, at)
        .unwrap()
}

// Catches: replay repinning definitions, restarting the wait, losing mapped
// outputs, or repeated completion overwriting the original immutable result.
#[test]
fn neutral_execution_replays_selected_path_results_and_wait_after_owner_restart() {
    let (dir, owner, mut run, at) = fixture(20, false);
    run = step(&owner, &run, "start-step", at);
    run = step(&owner, &run, "prepare", at + 1);
    let attempt = run.attempt.as_ref().unwrap();
    assert_eq!(attempt.inputs, BTreeMap::from([("value".into(), json!(6))]));
    let command = Command::Complete {
        fence: attempt.fence.clone(),
        outcome: "ok".into(),
        outputs: BTreeMap::from([("next".into(), json!(7))]),
    };
    let receipt = owner
        .neutral_command_at(&run.run.id, "result", run.sequence, command.clone(), at + 2)
        .unwrap();
    assert_eq!(
        receipt.activity_outputs[&0]["note"].state,
        DataState::Absent
    );
    let changed = Command::Complete {
        fence: receipt.attempt.as_ref().unwrap().fence.clone(),
        outcome: "ok".into(),
        outputs: BTreeMap::from([("next".into(), json!(9))]),
    };
    assert!(
        owner
            .neutral_command_at(
                &run.run.id,
                "result",
                receipt.sequence,
                changed.clone(),
                at + 2
            )
            .unwrap_err()
            .contains("reused")
    );
    assert!(
        owner
            .neutral_command_at(
                &run.run.id,
                "replace-result",
                receipt.sequence,
                changed,
                at + 2
            )
            .unwrap_err()
            .contains("immutable")
    );
    run = step(&owner, &receipt, "route-result", at + 3);
    run = step(&owner, &run, "condition", at + 4);
    assert_eq!(run.node, "wait");
    run = step(&owner, &run, "arm", at + 5);
    assert_eq!(run.wait_due_ms, Some(at + 10));
    assert_eq!(step(&owner, &run, "not-due", at + 9), run);
    drop(owner);
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
    run = step(&owner, &run, "due", at + 10);
    run = step(&owner, &run, "finish", at + 11);
    assert_eq!(
        run.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(
        run.outputs.as_ref().unwrap().values,
        BTreeMap::from([("result".into(), json!(7))])
    );
    assert_eq!(
        owner
            .neutral_command_at(&run.run.id, "result", 0, command.clone(), at + 20)
            .unwrap(),
        receipt
    );
    assert!(
        owner
            .neutral_command_at(&run.run.id, "late-result", run.sequence, command, at + 20)
            .unwrap_err()
            .contains("terminal")
    );
    let history = owner
        .store
        .neutral_events_after(&run.run.id, 0, 500)
        .unwrap();
    assert_eq!(
        history
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        (1..=run.sequence).collect::<Vec<_>>()
    );
    assert!(history.iter().any(|event| matches!(&event.kind, super::neutral::NeutralEventKind::Execution { decision, .. } if matches!(decision.as_ref(), super::neutral_execution::NeutralDecision::Route { outcome, provenance } if outcome == "true" && !provenance.is_empty()))));
    assert_eq!(
        owner.store.neutral_events_after(&run.run.id, 3, 1).unwrap()[0].command_id,
        "result"
    );
    assert!(
        owner
            .store
            .neutral_events_after(&run.run.id, 0, 501)
            .is_err()
    );
}

// Catches: stale/foreign activation reports, invalid output or outcome advancing
// control, command-key payload replacement, and completion after cancellation.
#[test]
fn neutral_execution_fences_results_and_retains_cancelled_history() {
    let (_dir, owner, run, at) = fixture(20, false);
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "prepare", at);
    let fence = run.attempt.as_ref().unwrap().fence.clone();
    let valid = Command::Complete {
        fence: fence.clone(),
        outcome: "ok".into(),
        outputs: BTreeMap::from([("next".into(), json!(7))]),
    };
    let mut invalid = Vec::new();
    for field in 0..4 {
        let mut bad = fence.clone();
        match field {
            0 => bad.run = "other".into(),
            1 => bad.execution = "other".into(),
            2 => bad.activation += 1,
            _ => bad.attempt += 1,
        }
        invalid.push(Command::Complete {
            fence: bad,
            outcome: "ok".into(),
            outputs: BTreeMap::from([("next".into(), json!(7))]),
        });
    }
    invalid.push(Command::Complete {
        fence: fence.clone(),
        outcome: "success".into(),
        outputs: BTreeMap::from([("next".into(), json!(7))]),
    });
    invalid.push(Command::Complete {
        fence: fence.clone(),
        outcome: "ok".into(),
        outputs: BTreeMap::new(),
    });
    invalid.push(Command::Complete {
        fence,
        outcome: "ok".into(),
        outputs: BTreeMap::from([("next".into(), json!("wrong"))]),
    });
    for (index, command) in invalid.into_iter().enumerate() {
        assert!(
            owner
                .neutral_command_at(
                    &run.run.id,
                    &format!("invalid-{index}"),
                    run.sequence,
                    command,
                    at
                )
                .is_err()
        );
        assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
    }
    let cancelled = owner
        .neutral_command(&run.run.id, "cancel", run.sequence, Command::Cancel)
        .unwrap();
    assert_eq!(cancelled.status, NeutralStatus::Cancelled);
    assert!(
        owner
            .neutral_command_at(&run.run.id, "late", cancelled.sequence, valid, at)
            .unwrap_err()
            .contains("terminal")
    );
    assert!(
        owner
            .neutral_command_at(&run.run.id, "cancel", cancelled.sequence, Command::Step, at)
            .unwrap_err()
            .contains("reused")
    );
    assert_eq!(
        owner.store.neutral_execution(&run.run.id).unwrap(),
        cancelled
    );
}

// Catches: Unknown selecting false/success, unpersisted limit exhaustion, and
// an expired run accepting a new result rather than preserving terminal history.
#[test]
fn neutral_execution_routes_unknown_and_records_generic_limits() {
    let (_dir, owner, run, at) = fixture(20, true);
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "unknown", at);
    assert_eq!(run.node, "unknown");
    let run = step(&owner, &run, "finish", at);
    assert_eq!(
        run.status,
        NeutralStatus::Completed {
            outcome: "unknown".into()
        }
    );
    let (_dir, owner, run, at) = fixture(1, false);
    let run = step(&owner, &run, "limit", at);
    assert_eq!(
        run.status,
        NeutralStatus::Limited {
            reason: "neutral activation limit exhausted".into()
        }
    );
    assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
    let (_dir, owner, run, _at) = fixture(20, false);
    let run = step(&owner, &run, "expire", run.deadline_ms);
    assert_eq!(run.status, NeutralStatus::Expired);
    assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
    assert!(super::neutral_execution::checked_deadline(i64::MAX - 4, 5).is_err());
    assert_eq!(
        super::neutral_execution::checked_deadline(i64::MAX - 5, 5).unwrap(),
        i64::MAX
    );
}

// Catches: concurrent writers advancing twice from one sequence, or replay
// trusting a rehashed decision instead of checking its command and predecessor.
#[test]
fn neutral_execution_serializes_decisions_and_rejects_corrupt_history() {
    let (_dir, owner, run, at) = fixture(20, false);
    let barrier = std::sync::Barrier::new(2);
    let receipts = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    owner
                        .neutral_command_at(
                            &run.run.id,
                            "same-step",
                            run.sequence,
                            Command::Step,
                            at,
                        )
                        .unwrap()
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(receipts[0], receipts[1]);
    assert_eq!(receipts[0].sequence, 2);
    assert!(
        owner
            .neutral_command_at(&run.run.id, "stale-step", 1, Command::Step, at)
            .unwrap_err()
            .contains("sequence")
    );
    let conn = owner.store.connect().unwrap();
    let document: String = conn
        .query_row(
            "SELECT event_json FROM neutral_workflow_events WHERE run_id=?1 AND sequence=2",
            [&run.run.id],
            |row| row.get(0),
        )
        .unwrap();
    let mut event: super::neutral::NeutralEvent = serde_json::from_str(&document).unwrap();
    if let super::neutral::NeutralEventKind::Execution { decision, .. } = &mut event.kind {
        **decision = super::neutral_execution::NeutralDecision::Cancel;
    }
    conn.execute(
        "UPDATE neutral_workflow_events SET event_json=?2,sha256=?3 WHERE run_id=?1 AND sequence=2",
        rusqlite::params![
            run.run.id,
            serde_json::to_string(&event).unwrap(),
            content_digest(&event).unwrap()
        ],
    )
    .unwrap();
    assert!(
        owner
            .store
            .neutral_execution(&run.run.id)
            .unwrap_err()
            .contains("decision")
    );
}
