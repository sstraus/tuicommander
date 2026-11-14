use super::neutral_effects::{EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralExecutionCommand as Command, NeutralResult, NeutralStatus};
use super::neutral_execution_tests::{fixture, step};
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance, HandlerRef};
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Notify;

struct Context {
    depth: usize,
}
impl HostContext for Context {
    fn for_subflow(
        &self,
        _: &crate::workflows::data_reference::ReferenceInputs,
    ) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new(Context {
            depth: self.depth + 1,
        }))
    }
}
pub(super) struct Host {
    denied: AtomicBool,
    calls: AtomicUsize,
    admitted: parking_lot::Mutex<Vec<BTreeMap<String, Value>>>,
    invalid_result: bool,
    known_failure: bool,
    depths: parking_lot::Mutex<Vec<usize>>,
    deny_child: bool,
}
impl Host {
    pub(super) fn new(denied: bool, invalid_result: bool) -> Arc<Self> {
        Arc::new(Self {
            denied: AtomicBool::new(denied),
            calls: AtomicUsize::new(0),
            admitted: Default::default(),
            invalid_result,
            known_failure: false,
            depths: Default::default(),
            deny_child: false,
        })
    }
}
impl HostAdmission for Host {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
    ) -> Result<(), String> {
        Ok(())
    }
    fn dispatch(
        &self,
        context: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        self.admitted.lock().push(inputs.clone());
        let depth = (context as &dyn std::any::Any)
            .downcast_ref::<Context>()
            .unwrap()
            .depth;
        self.depths.lock().push(depth);
        if depth > 0 && self.deny_child {
            return Err("derived child context refused".into());
        }
        if self.denied.load(Ordering::Acquire) {
            Err("current host refused".into())
        } else {
            Ok(())
        }
    }
}
impl NeutralHost for Host {
    fn prepare(
        &self,
        _: &NeutralInvocation,
    ) -> Result<super::neutral_effects::EffectPreparation, String> {
        Ok(super::neutral_effects::EffectPreparation {
            model_tools: None,
            mutation: false,
            resources: Default::default(),
            approval: None,
        })
    }
    fn context(&self, _: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new(Context { depth: 0 }))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        registry.handlers.insert(
            HandlerRef {
                name: "host.increment".into(),
                version: 1,
            },
            self,
        );
        registry
            .register_reusable_handler(
                HandlerRef {
                    name: "host.subflow".into(),
                    version: 1,
                },
                crate::workflows::neutral_admission::ReusableHandlerKind::Subflow,
                self,
            )
            .unwrap();
        registry
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let invocation = effect.invocation;
        self.calls.fetch_add(1, Ordering::AcqRel);
        let next = invocation.inputs["value"].as_i64().unwrap() + 1;
        let invalid = self.invalid_result;
        let known_failure = self.known_failure;
        Box::pin(async move {
            Ok(EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Completed {
                    result: if known_failure {
                        NeutralResult::Failed {
                            diagnostic: "owned producer definitively failed".into(),
                        }
                    } else {
                        NeutralResult::Completed {
                            outcome: if invalid {
                                "not-declared".into()
                            } else {
                                "ok".into()
                            },
                            outputs: BTreeMap::from([("next".into(), json!(next))]),
                        }
                    },
                },
            })
        })
    }
}

// Catches: the production actor taking legacy story/backend paths for a neutral
// id, or admitting root field names instead of the actual projected invocation.
#[tokio::test]
async fn existing_actor_dispatches_resolved_values_and_finishes_without_domain_state() {
    let (_dir, owner, run, _) = fixture(30, false);
    let host = Host::new(false, false);
    owner.neutral_host(host.clone());
    // A missing AppState is deliberate: the neutral branch needs no domain
    // backend or native application to execute a host-owned operation.
    super::runtime::run_actor(
        Arc::downgrade(&owner),
        run.run.id.clone(),
        Arc::new(Notify::new()),
    )
    .await;
    let finished = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(
        finished.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(
        finished.outputs.unwrap().values,
        BTreeMap::from([("result".into(), json!(7))])
    );
    assert_eq!(
        *host.admitted.lock(),
        vec![BTreeMap::from([("value".into(), json!(6))])]
    );
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    // Recovery lists identities without inspecting payloads; completed actors
    // exit after their own replay, rather than gating shared-owner startup.
    assert!(
        owner
            .store
            .neutral_recovery_run_ids()
            .unwrap()
            .contains(&run.run.id)
    );
}

// Catches: a persisted prepared attempt acting as a permit after host revocation,
// and a malformed adapter result leaving a permanently running activation.
#[tokio::test]
async fn current_admission_refuses_and_invalid_effect_result_retains_uncertainty() {
    for (denied, invalid) in [(true, false), (false, true)] {
        let (_dir, owner, run, at) = fixture(30, false);
        let run = step(&owner, &run, "enter", at);
        let run = step(&owner, &run, "prepared", at);
        let host = Host::new(denied, invalid);
        owner.neutral_host(host.clone());
        if invalid {
            let notify = Arc::new(Notify::new());
            assert!(owner.dispatch_neutral(&run, &notify).unwrap());
            notify.notified().await;
            let uncertain = owner.store.neutral_execution(&run.run.id).unwrap();
            assert_eq!(uncertain.status, NeutralStatus::Running);
            assert!(uncertain.attempt.as_ref().unwrap().result.is_none());
            assert!(matches!(
                uncertain
                    .effects
                    .values()
                    .next()
                    .unwrap()
                    .observation
                    .as_ref()
                    .unwrap()
                    .outcome,
                EffectOutcome::Uncertain { .. }
            ));
        } else {
            super::runtime::run_actor(
                Arc::downgrade(&owner),
                run.run.id.clone(),
                Arc::new(Notify::new()),
            )
            .await;
            let finished = owner.store.neutral_execution(&run.run.id).unwrap();
            assert!(matches!(finished.status, NeutralStatus::Failed { .. }));
            assert!(finished.attempt.unwrap().result.is_some());
        }
        assert_eq!(
            host.calls.load(Ordering::Acquire),
            if denied { 0 } else { 1 }
        );
        assert_eq!(host.admitted.lock().len(), 1);
    }
}

// Catches: restart blindly dispatching a previously claimed effect, cancellation
// accepting a late callback, and restart enumeration omitting neutral runs.
#[tokio::test]
async fn restart_keeps_dispatch_claim_inspectable_and_never_reinvokes_it() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "prepared", at);
    let fence = run.attempt.as_ref().unwrap().fence.clone();
    let run = owner
        .neutral_command_at(
            &run.run.id,
            "claimed",
            run.sequence,
            Command::Dispatch {
                fence: fence.clone(),
            },
            at,
        )
        .unwrap();
    drop(owner);
    let owner = super::runtime::RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let host = Host::new(false, false);
    owner.neutral_host(host.clone());
    assert_eq!(
        owner.store.neutral_recovery_run_ids().unwrap(),
        vec![run.run.id.clone()]
    );
    assert!(
        !owner
            .dispatch_neutral(&run, &Arc::new(Notify::new()))
            .unwrap()
    );
    assert_eq!(host.calls.load(Ordering::Acquire), 0);
    assert!(host.admitted.lock().is_empty());
    let cancelled = owner
        .neutral_command(&run.run.id, "cancel", run.sequence, Command::Cancel)
        .unwrap();
    assert!(
        owner
            .neutral_command(
                &run.run.id,
                "late",
                cancelled.sequence,
                Command::Complete {
                    fence,
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([("next".into(), json!(7))])
                }
            )
            .is_err()
    );
    assert_eq!(
        owner.store.neutral_execution(&run.run.id).unwrap(),
        cancelled
    );
}

// Catches: treating a reusable descriptor as an external whole-scheduler task,
// skipping current child admission, or deriving child rights from labels/data.
#[tokio::test]
async fn subflow_frames_replay_same_owner_and_readmit_typed_child_arguments() {
    for deny_child in [false, true] {
        let (_dir, owner, run, _) = subflow_fixture(false, 1, false);
        let host = Arc::new(Host {
            denied: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            admitted: Default::default(),
            invalid_result: false,
            known_failure: false,
            depths: Default::default(),
            deny_child,
        });
        owner.neutral_host(host.clone());
        super::runtime::run_actor(
            Arc::downgrade(&owner),
            run.run.id.clone(),
            Arc::new(Notify::new()),
        )
        .await;
        let finished = owner.store.neutral_execution(&run.run.id).unwrap();
        assert_eq!(*host.depths.lock(), vec![0, 1]);
        assert_eq!(
            *host.admitted.lock(),
            vec![
                BTreeMap::from([("value".into(), json!(6))]),
                BTreeMap::from([("value".into(), json!(6))])
            ]
        );
        assert_eq!(
            host.calls.load(Ordering::Acquire),
            if deny_child { 0 } else { 1 }
        );
        if deny_child {
            assert!(matches!(finished.status, NeutralStatus::Failed { .. }));
        } else {
            assert_eq!(
                finished.status,
                NeutralStatus::Completed {
                    outcome: "done".into()
                }
            );
            assert_eq!(
                finished.outputs.as_ref().unwrap().values["result"],
                json!(7)
            );
        }
        assert_eq!(
            owner.store.neutral_execution(&run.run.id).unwrap(),
            finished
        );
    }
}

// Catches: scoped prepared values bypassing actual actor admission, or the
// actor stranding additional ready items/iterations after the first callback.
#[tokio::test]
async fn actor_invokes_collection_items_and_loop_carried_values_through_host_port() {
    for kind in ["collection", "loop"] {
        let (_dir, owner, run, _) = super::neutral_scope_tests::fixture(kind, 2);
        let host = Host::new(false, false);
        owner.neutral_host(host.clone());
        super::runtime::run_actor(
            Arc::downgrade(&owner),
            run.run.id.clone(),
            Arc::new(Notify::new()),
        )
        .await;
        let finished = owner.store.neutral_execution(&run.run.id).unwrap();
        assert_eq!(
            finished.status,
            NeutralStatus::Completed {
                outcome: "done".into()
            }
        );
        if kind == "collection" {
            let items = finished.outputs.as_ref().unwrap().values["items"]
                .as_array()
                .unwrap();
            assert_eq!(
                items
                    .iter()
                    .map(|item| item["outputs"]["next"].clone())
                    .collect::<Vec<_>>(),
                vec![json!(31), json!(11), json!(21)]
            );
            let mut admitted = host
                .admitted
                .lock()
                .iter()
                .map(|inputs| inputs["value"].as_i64().unwrap())
                .collect::<Vec<_>>();
            admitted.sort_unstable();
            assert_eq!(admitted, vec![10, 20, 30]);
        } else {
            assert_eq!(finished.outputs.as_ref().unwrap().values["value"], json!(2));
            assert_eq!(
                host.admitted
                    .lock()
                    .iter()
                    .map(|inputs| inputs["value"].clone())
                    .collect::<Vec<_>>(),
                vec![json!(0), json!(1)]
            );
        }
    }
}

fn subflow_fixture(
    collection: bool,
    depth: u32,
    failing_fork: bool,
) -> (
    tempfile::TempDir,
    Arc<super::runtime::RuntimeOwner>,
    super::neutral_execution::NeutralExecution,
    i64,
) {
    subflow_fixture_with_ancestor(collection, depth, failing_fork, false)
}

pub(super) fn subflow_fixture_with_ancestor(
    collection: bool,
    depth: u32,
    failing_fork: bool,
    uncertain_ancestor: bool,
) -> (
    tempfile::TempDir,
    Arc<super::runtime::RuntimeOwner>,
    super::neutral_execution::NeutralExecution,
    i64,
) {
    use crate::workflows::catalog::{
        DescriptorId, FieldSchema, ObjectSchema, ReusableTarget, ValueType, content_digest,
    };
    use crate::workflows::data_binding::DataSource;
    use crate::workflows::fork_scope::{BranchContract, ForkContract};
    use crate::workflows::neutral_control::{ControlGraph, ControlNode};
    use crate::workflows::neutral_publication::{
        NeutralDefinition, PublicationPin, PublicationRef,
    };
    super::neutral_execution_tests::fixture_with_concurrency(
        100,
        false,
        2,
        |root, catalog, workflows| {
            let mut native = catalog.get(&root.instances[0].descriptor).unwrap().clone();
            if collection {
                native.id = DescriptorId {
                    name: "indexed-leaf".into(),
                    ..native.id
                };
                native.inputs.fields.insert(
                    "index".into(),
                    FieldSchema {
                        value_type: ValueType::Integer,
                        required: true,
                        default: None,
                    },
                );
                catalog.register(native.clone()).unwrap();
            }
            let instance = ActivityInstance {
                descriptor: native.id.clone(),
                ..root.instances[0].clone()
            };
            let mut child = NeutralDefinition {
                control: ControlGraph {
                    nodes: BTreeMap::from([
                        ("start".into(), ControlNode::Start),
                        ("body".into(), ControlNode::Activity { activity: 0 }),
                        (
                            "end".into(),
                            ControlNode::End {
                                outcome: "ok".into(),
                            },
                        ),
                    ]),
                    edges: BTreeMap::from([
                        (
                            "start".into(),
                            BTreeMap::from([("next".into(), "body".into())]),
                        ),
                        ("body".into(), BTreeMap::from([("ok".into(), "end".into())])),
                    ]),
                },
                inputs: native.inputs.clone(),
                subject: None,
                outputs: native.outputs.clone(),
                outcomes: native.outcomes.clone(),
                instances: vec![instance],
                input_bindings: BTreeMap::new(),
                conditions: BTreeMap::new(),
                collections: BTreeMap::new(),
                loops: BTreeMap::new(),
                forks: BTreeMap::new(),
                subflows: Vec::new(),
                output_bindings: BTreeMap::from([
                    (
                        "next".into(),
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
            if failing_fork || uncertain_ancestor {
                child.instances.push(child.instances[0].clone());
                let identity = child
                    .inputs
                    .fields
                    .keys()
                    .map(|field| {
                        (
                            field.clone(),
                            DataSource::WorkflowInput {
                                field: field.clone(),
                            },
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                child.input_bindings.insert(0, identity.clone());
                let mut dependent = identity;
                if !uncertain_ancestor {
                    dependent.insert(
                        "value".into(),
                        DataSource::ActivityOutput {
                            activity: 0,
                            field: "next".into(),
                        },
                    );
                }
                child.input_bindings.insert(1, dependent);
                child.forks.insert(
                    "fork".into(),
                    ForkContract {
                        branches: BTreeMap::from([
                            (
                                "a".into(),
                                BranchContract {
                                    activity: 0,
                                    required: true,
                                },
                            ),
                            (
                                "b".into(),
                                BranchContract {
                                    activity: 1,
                                    required: true,
                                },
                            ),
                        ]),
                        max_concurrency: 2,
                    },
                );
                child.control.nodes.insert(
                    "body".into(),
                    ControlNode::Fork {
                        scope: "fork".into(),
                    },
                );
                child.control.edges.insert(
                    "body".into(),
                    BTreeMap::from([
                        ("complete".into(), "end".into()),
                        ("blocked".into(), "end".into()),
                    ]),
                );
                child.outputs = ObjectSchema::default();
                child.output_bindings.clear();
            }
            let handlers = [
                native.handler.clone(),
                HandlerRef {
                    name: "host.subflow".into(),
                    version: 1,
                },
            ]
            .into();
            for level in 0..depth {
                let id = format!("child-{level}");
                let published = workflows
                    .save_neutral_publication("/project", &id, 0, child.clone(), catalog, &handlers)
                    .unwrap();
                let mut wrapper = native.clone();
                wrapper.id = DescriptorId {
                    name: format!("invoke-child-{level}"),
                    ..wrapper.id
                };
                wrapper.handler = HandlerRef {
                    name: "host.subflow".into(),
                    version: 1,
                };
                wrapper.outputs = child.outputs.clone();
                wrapper.target = Some(ReusableTarget::Subflow {
                    publication: PublicationPin {
                        reference: PublicationRef {
                            project: "/project".into(),
                            id,
                            revision: 1,
                        },
                        sha256: content_digest(&published).unwrap(),
                    },
                });
                catalog.register(wrapper.clone()).unwrap();
                root.instances[0].descriptor = wrapper.id.clone();
                if level + 1 < depth {
                    child.instances = vec![ActivityInstance {
                        descriptor: wrapper.id,
                        ..child.instances[0].clone()
                    }];
                    child.forks.clear();
                    child.input_bindings.clear();
                    child
                        .control
                        .nodes
                        .insert("body".into(), ControlNode::Activity { activity: 0 });
                    child
                        .control
                        .edges
                        .insert("body".into(), BTreeMap::from([("ok".into(), "end".into())]));
                }
            }
            if collection {
                root.conditions.clear();
                root.input_bindings.clear();
                root.outcomes = ["done".into()].into();
                root.collections.insert(
                    "items".into(),
                    crate::workflows::collection::ForEach {
                        collection: DataSource::Constant {
                            value: json!([6, 8]),
                        },
                        item_type: ValueType::Integer,
                        body: 0,
                        item_input: "value".into(),
                        index_input: "index".into(),
                        max_items: 2,
                        max_concurrency: 2,
                        empty: crate::workflows::collection::EmptyPolicy::Complete,
                        failure: crate::workflows::collection::FailurePolicy::Collect,
                    },
                );
                root.control = ControlGraph {
                    nodes: BTreeMap::from([
                        ("start".into(), ControlNode::Start),
                        (
                            "body".into(),
                            ControlNode::Collection {
                                scope: "items".into(),
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
                            BTreeMap::from([("next".into(), "body".into())]),
                        ),
                        (
                            "body".into(),
                            BTreeMap::from([
                                ("complete".into(), "end".into()),
                                ("blocked".into(), "end".into()),
                            ]),
                        ),
                    ]),
                };
                root.outputs = ObjectSchema {
                    fields: BTreeMap::from([(
                        "items".into(),
                        FieldSchema {
                            value_type: ValueType::Array,
                            required: true,
                            default: None,
                        },
                    )]),
                };
                root.output_bindings = BTreeMap::from([(
                    "items".into(),
                    DataSource::ScopeOutput {
                        scope: "items".into(),
                        field: "items".into(),
                    },
                )]);
            }
            if uncertain_ancestor {
                root.instances.push(ActivityInstance {
                    descriptor: native.id.clone(),
                    ..root.instances[0].clone()
                });
                root.input_bindings = [
                    (
                        0,
                        BTreeMap::from([(
                            "value".into(),
                            DataSource::Constant { value: json!(20) },
                        )]),
                    ),
                    (
                        1,
                        BTreeMap::from([(
                            "value".into(),
                            DataSource::WorkflowInput {
                                field: "initial".into(),
                            },
                        )]),
                    ),
                ]
                .into();
                root.conditions.clear();
                root.outputs = ObjectSchema::default();
                root.output_bindings.clear();
                root.outcomes = ["done".into()].into();
                root.forks.insert(
                    "outer".into(),
                    ForkContract {
                        branches: BTreeMap::from([
                            (
                                "a-native".into(),
                                BranchContract {
                                    activity: 1,
                                    required: true,
                                },
                            ),
                            (
                                "b-subflow".into(),
                                BranchContract {
                                    activity: 0,
                                    required: true,
                                },
                            ),
                        ]),
                        max_concurrency: 2,
                    },
                );
                root.control = ControlGraph {
                    nodes: BTreeMap::from([
                        ("start".into(), ControlNode::Start),
                        (
                            "body".into(),
                            ControlNode::Fork {
                                scope: "outer".into(),
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
                            BTreeMap::from([("next".into(), "body".into())]),
                        ),
                        (
                            "body".into(),
                            BTreeMap::from([
                                ("complete".into(), "end".into()),
                                ("blocked".into(), "end".into()),
                            ]),
                        ),
                    ]),
                };
            }
        },
    )
}

// Catches: prepared parent slots consuming child capacity and preventing the
// failed-producer skip/join path from settling under nested subflow frames.
#[tokio::test]
async fn actor_collection_nested_subflow_settles_failed_fork_without_capacity_deadlock() {
    for (depth, failing_fork) in [(2, false), (2, true), (4, false)] {
        let (_dir, owner, run, _) = subflow_fixture(true, depth, failing_fork);
        let mut host = Host::new(false, false);
        Arc::get_mut(&mut host).unwrap().known_failure = failing_fork;
        owner.neutral_host(host.clone());
        super::runtime::run_actor(
            Arc::downgrade(&owner),
            run.run.id.clone(),
            Arc::new(Notify::new()),
        )
        .await;
        let finished = owner.store.neutral_execution(&run.run.id).unwrap();
        assert_eq!(
            finished.status,
            NeutralStatus::Completed {
                outcome: "done".into()
            }
        );
        assert_eq!(host.calls.load(Ordering::Acquire), 2);
        let items = finished.outputs.as_ref().unwrap().values["items"]
            .as_array()
            .unwrap();
        assert_eq!(items.len(), 2);
        if !failing_fork {
            assert_eq!(
                items
                    .iter()
                    .map(|item| item["outputs"]["next"].clone())
                    .collect::<Vec<_>>(),
                vec![json!(7), json!(9)]
            );
        } else {
            assert!(owner.store.neutral_events_after(&run.run.id, 0, 500).unwrap().iter().any(|event| matches!(&event.kind, super::neutral::NeutralEventKind::Execution { decision, .. } if matches!(decision.as_ref(), super::neutral_execution::NeutralDecision::Scope { action: super::neutral_scopes::ScopeDecision::Skip { .. } }))));
        }
    }
}

// Catches: a transient event-write error becoming a durable Abort/Failure rather
// than leaving the same cursor retryable after the real storage error clears.
#[test]
fn neutral_actor_storage_write_failure_does_not_become_workflow_failure() {
    let (_dir, owner, run, _) = fixture(30, false);
    owner.store.connect().unwrap().execute_batch("CREATE TRIGGER refuse_daemon_step BEFORE INSERT ON neutral_workflow_events WHEN NEW.command_id LIKE 'daemon:neutral-step:%' BEGIN SELECT RAISE(FAIL, 'injected storage write failure'); END;").unwrap();
    assert!(owner.drive_neutral_turn(&run.run.id).is_err());
    assert_eq!(owner.store.neutral_execution(&run.run.id).unwrap(), run);
    owner
        .store
        .connect()
        .unwrap()
        .execute_batch("DROP TRIGGER refuse_daemon_step;")
        .unwrap();
    let (ready, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    assert_eq!(ready.status, NeutralStatus::Running);
    assert!(ready.attempt.is_some());
}
