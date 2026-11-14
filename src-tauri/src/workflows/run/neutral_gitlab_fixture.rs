//! Local protocol harness. Echoed writes are not live GitLab publication evidence.
use super::neutral_execution_tests::fixture_with;
use super::neutral_gitlab::{GitLabContext, GitLabHost};
use super::neutral_runtime::NeutralInvocation;
use super::runtime::RuntimeOwner;
use crate::workflows::catalog::ActivityInstance;
use crate::workflows::data_binding::DataSource;
use crate::workflows::gitlab::{GitLabInstall, Operation};
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct Fixture {
    pub _dir: tempfile::TempDir,
    pub owner: Arc<RuntimeOwner>,
    pub host: Arc<GitLabHost>,
    pub invocation: NeutralInvocation,
    pub granted: Arc<AtomicBool>,
}
pub(super) fn fixture(
    state: Arc<crate::AppState>,
    install: GitLabInstall,
    op: Operation,
    inputs: BTreeMap<String, Value>,
) -> Fixture {
    let descriptor = install.descriptor(op);
    let instance = ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    };
    let granted = Arc::new(AtomicBool::new(true));
    let current = granted.clone();
    let id = install.connector.id.clone();
    let host = Arc::new(GitLabHost {
        approval: None,
        state,
        install,
        authority: Arc::new(move |_| {
            Ok(GitLabContext {
                reads: if current.load(Ordering::Acquire) {
                    BTreeSet::from([id.clone()])
                } else {
                    BTreeSet::new()
                },
                writes: BTreeSet::from([id.clone()]),
            })
        }),
    });
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
                    descriptor
                        .outcomes
                        .iter()
                        .map(|outcome| (outcome.clone(), "end".into()))
                        .collect(),
                ),
            ]),
        };
        definition.instances = vec![instance.clone()];
        definition.conditions.clear();
        definition.outputs = descriptor.outputs.clone();
        definition.outcomes = BTreeSet::from(["done".into()]);
        definition.input_bindings = BTreeMap::from([(
            0,
            inputs
                .iter()
                .map(|(k, v)| (k.clone(), DataSource::Constant { value: v.clone() }))
                .collect(),
        )]);
        definition.output_bindings = BTreeMap::from([(
            "receipt".into(),
            DataSource::ActivityOutput {
                activity: 0,
                field: "receipt".into(),
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
pub(super) type Requests = Arc<Mutex<Vec<(String, String, Value)>>>;
pub(super) async fn server(
    dir: &std::path::Path,
    raw: Value,
    write_status: u16,
) -> (
    Arc<crate::AppState>,
    GitLabInstall,
    Requests,
    tokio::task::JoinHandle<()>,
) {
    use axum::{extract::Request, response::IntoResponse};
    let seen: Requests = Arc::default();
    let log = seen.clone();
    let app=axum::Router::new().fallback(move |req:Request| {let seen=log.clone();let raw=raw.clone();async move {
  let path=req.uri().path().to_string();let method=req.method().to_string();
  let bytes=axum::body::to_bytes(req.into_body(),65536).await.unwrap();
  let body=if bytes.is_empty(){Value::Null}else{serde_json::from_slice(&bytes).unwrap()};
  seen.lock().unwrap().push((method.clone(),path.clone(),body.clone()));
  if path.ends_with("/user") {return axum::Json(json!({"username":"fixture-user"})).into_response();}
  if method=="POST" {return (axum::http::StatusCode::from_u16(write_status).unwrap(),axum::Json(json!({"id":99,"body":body["body"],"project_id":1,"noteable_iid":1,"noteable_type":"MergeRequest"}))).into_response();}
  let payload = if raw.get("mr").is_some() && raw.get("pipeline").is_some() {
      if path.contains("/pipelines/") { raw["pipeline"].clone() } else { raw["mr"].clone() }
  } else { raw };
  axum::Json(payload).into_response()
 }});
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let plugin = dir.join("plugins/gitlab");
    std::fs::create_dir_all(&plugin).unwrap();
    let manifest = json!({"id":"gitlab","name":"GitLab protocol harness","version":"1.0.0","minAppVersion":"0.0.0","main":"main.js","capabilities":["net:http"],"allowedUrls":[format!("{origin}/api/v4/*")],"providers":[{"id":"gitlab","label":"GitLab","kinds":["bearer"],"verify":{"path":"/api/v4/user","loginPointer":"/username"}}]});
    std::fs::write(
        plugin.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    crate::plugins::register_loaded_plugin_impl(&state, "gitlab".into(), vec!["net:http".into()])
        .unwrap();
    // The test-support credential backend is an in-memory mock, never Boss's vault.
    let account = crate::accounts::add_account(
        crate::accounts::NewAccount {
            provider: "gitlab".into(),
            host: origin.clone(),
            kind: crate::accounts::AuthKind::Bearer,
            label: "Isolated protocol".into(),
            username: None,
        },
        "local-fixture-token".into(),
    )
    .await
    .unwrap();
    let install = GitLabInstall::new(format!("{origin}/api/v4/"), 1, account.account.id).unwrap();
    (state, install, seen, task)
}
