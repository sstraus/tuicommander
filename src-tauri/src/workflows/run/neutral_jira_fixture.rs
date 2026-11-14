//! Local transport replay of recorded Cloud responses; constructed faults are explicit.
use super::neutral_effects::{EffectCommand, NeutralEffect};
use super::neutral_execution_tests::fixture_with;
use super::neutral_http::HttpContext;
pub(super) use super::neutral_http_fixture_server::server;
use super::neutral_jira::JiraHost;
use super::neutral_runtime::{NeutralHost, NeutralInvocation};
use super::runtime::RuntimeOwner;
use crate::workflows::catalog::*;
use crate::workflows::data_binding::DataSource;
use crate::workflows::jira::*;
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn cloud() -> Value {
    serde_json::from_str(include_str!("../../../../plugins/jira/fixtures/cloud.json")).unwrap()
}
pub(super) fn fields() -> JiraFields {
    JiraFields {
        summary: None,
        description: None,
        labels: None,
        priority_id: None,
        assignee_account_id: None,
        custom: BTreeMap::new(),
    }
}
pub(super) fn reference() -> JiraIssueRef {
    JiraIssueRef {
        account: "fixture".into(),
        id: "10004".into(),
    }
}
pub(super) fn inputs(request: &JiraRequest) -> BTreeMap<String, Value> {
    BTreeMap::from([("request".into(), serde_json::to_value(request).unwrap())])
}
pub(super) struct Fixture {
    pub _dir: tempfile::TempDir,
    pub owner: Arc<RuntimeOwner>,
    pub host: Arc<JiraHost>,
    pub invocation: NeutralInvocation,
    pub writes: Arc<AtomicBool>,
}
pub(super) fn fixture(state: Arc<crate::AppState>, base: &str, request: JiraRequest) -> Fixture {
    let mut binding = JiraBinding::new(
        base.into(),
        "workflow-http-test".into(),
        "fixture".into(),
        "10001".into(),
        request.operation(),
    )
    .unwrap();
    // Credential-free local replay does not claim account authentication evidence.
    binding.contract.authentication = None;
    binding
        .descriptor
        .config
        .fields
        .get_mut("connector")
        .unwrap()
        .default
        .as_mut()
        .unwrap()["contract"]["authentication"] = Value::Null;
    let descriptor = binding.descriptor.clone();
    let id = binding.contract.id.clone();
    let writes = Arc::new(AtomicBool::new(true));
    let grants = writes.clone();
    let host = Arc::new(JiraHost {
        approval: None,
        state,
        bindings: BTreeMap::from([(descriptor.handler.clone(), binding)]),
        authority: Arc::new(move |_| {
            Ok(HttpContext {
                network: BTreeSet::from([id.clone()]),
                writes: if grants.load(Ordering::Acquire) {
                    BTreeSet::from([id.clone()])
                } else {
                    BTreeSet::new()
                },
            })
        }),
    });
    let instance = ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    };
    let request = serde_json::to_value(request).unwrap();
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
        definition.subject = None;
        definition.outputs = descriptor.outputs.clone();
        definition.outcomes = BTreeSet::from(["done".into()]);
        definition.input_bindings = BTreeMap::from([(
            0,
            BTreeMap::from([("request".into(), DataSource::Constant { value: request })]),
        )]);
        definition.output_bindings = BTreeMap::from([(
            "result".into(),
            DataSource::ActivityOutput {
                activity: 0,
                field: "result".into(),
            },
        )]);
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
        writes,
    }
}
pub(super) fn claimed(f: &Fixture) -> NeutralEffect {
    let mut invocation = f.invocation.clone();
    invocation.instance.config = invocation
        .descriptor
        .config
        .resolve(&invocation.instance.config, "Jira config")
        .unwrap();
    let effect =
        NeutralEffect::prepare(invocation, f.host.prepare(&f.invocation).unwrap()).unwrap();
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
pub(super) fn outputs(observation: super::neutral_effects::EffectObservation) -> Value {
    let super::neutral_effects::EffectOutcome::Completed {
        result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
    } = observation.outcome
    else {
        panic!("not completed")
    };
    outputs["result"].clone()
}

/// Constructed recovery responses use the precommitted digest in documented slots.
pub(super) async fn recovery(
    dir: &std::path::Path,
    request: JiraRequest,
    replies: Vec<Value>,
) -> (
    Fixture,
    NeutralEffect,
    tokio::task::JoinHandle<()>,
    Arc<std::sync::Mutex<Vec<String>>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/", listener.local_addr().unwrap());
    let (state, _, setup) = server(dir, vec![]).await;
    setup.await.unwrap();
    state.config.write().plugin_http_destinations = vec![format!("{base}*")];
    let f = fixture(state, &base, request);
    let effect = claimed(&f);
    let replies = replies
        .into_iter()
        .map(|v| {
            serde_json::from_str::<Value>(&v.to_string().replace("<effect>", &effect.key.digest))
                .unwrap()
        })
        .collect::<std::collections::VecDeque<_>>();
    let replies = Arc::new(std::sync::Mutex::new(replies));
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = requests.clone();
    let router = axum::Router::new().fallback(axum::routing::any(
        move |request: axum::extract::Request| {
            let replies = replies.clone();
            let seen = seen.clone();
            async move {
                let label = format!("{} {}", request.method(), request.uri());
                let body = axum::body::to_bytes(request.into_body(), 65_536)
                    .await
                    .unwrap();
                seen.lock().unwrap().push(format!(
                    "{label}\n{}",
                    String::from_utf8(body.to_vec()).unwrap()
                ));
                axum::Json(
                    replies
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("unexpected duplicate request"),
                )
            }
        },
    ));
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (f, effect, task, requests)
}
