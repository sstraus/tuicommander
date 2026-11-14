//! HTTP fixtures implement the recorded local contract, not a vendor API.
use super::neutral_execution_tests::fixture_with;
use super::neutral_http::{HttpConnector, HttpContext, HttpHost};
pub(super) use super::neutral_http_fixture_server::server;
use super::neutral_runtime::{NeutralHost, NeutralInvocation};
use super::runtime::RuntimeOwner;
use crate::workflows::catalog::*;
use crate::workflows::connector::*;
use crate::workflows::data_binding::DataSource;
use crate::workflows::http_operation::HttpOperation;
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) fn field(value_type: ValueType) -> FieldSchema {
    FieldSchema {
        value_type,
        required: true,
        default: None,
    }
}

pub(super) fn contract(paginated: bool) -> ConnectorContract {
    ConnectorContract {
        id: "local-recorded".into(),
        authentication: None,
        resources: ObjectSchema::default(),
        pagination: paginated.then(|| CursorPagination {
            query_parameter: "cursor".into(),
            items_pointer: "/items".into(),
            cursor_pointer: "/next".into(),
            output: "items".into(),
            max_pages: 3,
            max_items: 4,
        }),
        rate_limit: RateLimit::FailWithRetryAfter,
        callback: None,
        reconciliation: Reconciliation::Unavailable,
    }
}

pub(super) fn operation(base: &str, paginated: bool) -> HttpOperation {
    HttpOperation {
        method: "GET".into(),
        endpoint: format!("{base}records"),
        path: vec![],
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        response: if paginated {
            BTreeMap::new()
        } else {
            BTreeMap::from([("value".into(), "/value".into())])
        },
        max_response_bytes: 4096,
    }
}

pub(super) struct Fixture {
    pub _dir: tempfile::TempDir,
    pub owner: Arc<RuntimeOwner>,
    pub host: Arc<HttpHost>,
    pub invocation: NeutralInvocation,
    pub granted: Arc<AtomicBool>,
}

pub(super) fn fixture(
    state: Arc<crate::AppState>,
    base: &str,
    op: HttpOperation,
    contract: ConnectorContract,
) -> Fixture {
    let output = if contract.pagination.is_some() {
        ("items", ValueType::Array)
    } else {
        ("value", ValueType::Integer)
    };
    let descriptor = ActivityDescriptor {
        id: DescriptorId {
            namespace: "test".into(),
            name: "http".into(),
            version: 1,
        },
        purpose: "Read the documented local HTTP contract".into(),
        handler: HandlerRef {
            name: "installed.http".into(),
            version: 1,
        },
        target: None,
        config: ObjectSchema {
            fields: BTreeMap::from([("request".into(), field(ValueType::Object))]),
        },
        inputs: ObjectSchema {
            fields: BTreeMap::from([("initial".into(), field(ValueType::Integer))]),
        },
        subject: None,
        outputs: ObjectSchema {
            fields: if op.response.is_empty() && contract.pagination.is_none() {
                BTreeMap::new()
            } else {
                BTreeMap::from([(output.0.into(), field(output.1))])
            },
        },
        outcomes: BTreeSet::from(["complete".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    let binding = HttpConnector::new(
        descriptor,
        contract,
        "workflow-http-test".into(),
        base.into(),
    )
    .unwrap();
    let descriptor = binding.descriptor.clone();
    let granted = Arc::new(AtomicBool::new(true));
    let current = granted.clone();
    let host = Arc::new(HttpHost {
        approval: None,
        state,
        connectors: BTreeMap::from([(descriptor.handler.clone(), binding)]),
        authority: Arc::new(move |_| {
            Ok(HttpContext {
                network: if current.load(Ordering::Acquire) {
                    BTreeSet::from(["local-recorded".into()])
                } else {
                    BTreeSet::new()
                },
                writes: BTreeSet::from(["local-recorded".into()]),
            })
        }),
    });
    let instance = ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::from([("request".into(), serde_json::to_value(op).unwrap())]),
        execution_policy: "local".into(),
    };
    let (_dir, owner, run, _) = fixture_with(20, false, |definition, catalog, _| {
        catalog.register(descriptor.clone()).unwrap();
        definition.control = ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                ("work".into(), ControlNode::Activity { activity: 0 }),
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
                    BTreeMap::from([("next".into(), "work".into())]),
                ),
                (
                    "work".into(),
                    BTreeMap::from([("complete".into(), "end".into())]),
                ),
            ]),
        };
        definition.instances = vec![instance.clone()];
        definition.conditions.clear();
        definition.outputs = descriptor.outputs.clone();
        definition.outcomes = BTreeSet::from(["done".into()]);
        definition.input_bindings = BTreeMap::from([(
            0,
            BTreeMap::from([(
                "initial".into(),
                DataSource::WorkflowInput {
                    field: "initial".into(),
                },
            )]),
        )]);
        definition.output_bindings = descriptor
            .outputs
            .fields
            .keys()
            .map(|name| {
                (
                    name.clone(),
                    DataSource::ActivityOutput {
                        activity: 0,
                        field: name.clone(),
                    },
                )
            })
            .collect();
    });
    owner.neutral_host(host.clone());
    let (snapshot, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    let (_, attempt) = snapshot.pending_invocations().into_iter().next().unwrap();
    let invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: snapshot.publication.clone(),
        descriptor,
        instance,
        inputs: attempt.inputs.clone(),
        subject: None,
    };
    Fixture {
        _dir,
        owner,
        host,
        invocation,
        granted,
    }
}

pub(super) async fn run(f: &Fixture) -> super::neutral_execution::NeutralExecution {
    super::runtime::run_actor(
        Arc::downgrade(&f.owner),
        f.invocation.fence.run.clone(),
        Arc::new(tokio::sync::Notify::new()),
    )
    .await;
    f.owner
        .store
        .neutral_execution(&f.invocation.fence.run)
        .unwrap()
}

pub(super) fn claimed(f: &Fixture) -> super::neutral_effects::NeutralEffect {
    use super::neutral_effects::EffectCommand;
    let mut invocation = f.invocation.clone();
    invocation.instance.config = invocation
        .descriptor
        .config
        .resolve(&invocation.instance.config, "HTTP config")
        .unwrap();
    let effect = super::neutral_effects::NeutralEffect::prepare(
        invocation,
        f.host.prepare(&f.invocation).unwrap(),
    )
    .unwrap();
    f.owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Prepare {
                effect: Box::new(effect.clone()),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Dispatch {
                key: effect.key.clone(),
            },
        )
        .unwrap()
        .effect(&effect.key)
        .unwrap()
        .clone()
}
