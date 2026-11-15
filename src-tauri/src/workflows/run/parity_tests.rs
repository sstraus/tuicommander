use super::super as run;
use super::parity_support::*;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::*;
use run::native_test_support::Fixture;
use run::neutral_execution::NeutralStatus;
use serde_json::json;

// Catches: a real script's mapped request is lost at the native child boundary,
// or the child's artifact receipt never returns through the public run owner.
#[tokio::test]
async fn public_script_maps_native_artifact_request_and_returns_pinned_subflow_output() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let store = WorkflowStore::open().unwrap();
    let mut transform = script(
        "artifact-request",
        schema(json!({"path":field("string")})),
        schema(json!({"request":field("object")})),
        "data=dict(request=dict(paths=[q['inputs']['path']]))",
    );
    transform.subject = Some(SubjectSchema {
        required: true,
        fields: schema(json!({"workspace":field("workspace_reference")})),
    });
    let catalog = save(&f, &store, transform.clone());
    let native = catalog
        .get(&DescriptorId {
            namespace: "native".into(),
            name: "artifact.pin".into(),
            version: 1,
        })
        .unwrap()
        .clone();
    let mut child = graph(&[native]);
    child.input_bindings = serde_json::from_value(
        json!({"0":{"request":{"source":"workflow_input","field":"request"}}}),
    )
    .unwrap();
    child.output_bindings = serde_json::from_value(
        json!({"receipt":{"source":"activity_output","activity":0,"field":"receipt"}}),
    )
    .unwrap();
    let child = publish(&f, &store, &catalog, "pin-child", child);
    let call = subflow(&child, "pin-subflow");
    let catalog = save(&f, &store, call.clone());
    let mut root = graph(&[transform, call]);
    root.subject = child.definition.subject.clone();
    root.control = serde_json::from_value(json!({"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},"child":{"type":"activity","activity":1},"end":{"type":"end","outcome":"done"}},
        "edges":{"start":{"next":"work"},"work":{"ok":"child"},"child":{"done":"end"}}})).unwrap();
    root.input_bindings = serde_json::from_value(json!({"0":{"path":{"source":"workflow_input","field":"path"}},"1":{"request":{"source":"activity_output","activity":0,"field":"request"}}})).unwrap();
    root.output_bindings = serde_json::from_value(
        json!({"receipt":{"source":"activity_output","activity":1,"field":"receipt"}}),
    )
    .unwrap();
    let publication = publish(&f, &store, &catalog, "script-native-parent", root);
    let finished = run(
        &f,
        &publication,
        json!({"path":"artifact.txt"}),
        Some(json!({"workspace":{"reference":"workspace","id":f.context.workspace_path()}})),
    )
    .await;
    assert_eq!(
        finished.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        },
        "{finished:?}"
    );
    let receipt = &finished.outputs.as_ref().unwrap().values["receipt"];
    let blob = receipt["blobs"]["artifact.txt"].as_str().unwrap();
    assert_eq!(
        run::check::git_output(f.context.workspace_path(), &["cat-file", "-p", blob]).unwrap(),
        "seed"
    );
    assert_eq!(
        receipt["commit"],
        json!(f.context.workspace.as_ref().unwrap().baseline_commit)
    );
    assert!(finished.effects.values().all(|e| e.settled()));
}

// Catches: pagination stops at expansion instead of executing pinned per-item
// subflows, or child returns cannot feed the ordered aggregate's condition.
// The responder is a documented local contract, never live-provider evidence.
#[tokio::test]
async fn public_paginated_http_runs_bounded_subflows_and_branches_on_ordered_aggregate() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let mut f = Fixture::new();
    let (state, base, server) = run::neutral_http_fixture_server::server(
        config.path(),
        vec![
            (200, json!({"items":[30,10],"next":"page2"}), None),
            (200, json!({"items":[20],"next":null}), None),
        ],
    )
    .await;
    let workspace = f.context.workspace_path().to_owned();
    f.state = state;
    f.host = run::native_host::NativeHost::install(&f.state).unwrap();
    f.context = run::NativeContext::capture(&f.state, run::NativeCaller::Owner, &workspace, vec![])
        .unwrap();
    f.owner.neutral_host(f.host.clone());
    let store = WorkflowStore::open().unwrap();
    let transform = script(
        "page-item",
        schema(json!({"value":field("integer"),"index":field("integer")})),
        schema(json!({"next":field("integer")})),
        "data=dict(next=q['inputs']['value']+1)",
    );
    let catalog = save(&f, &store, transform.clone());
    let mut child = graph(&[transform]);
    child.input_bindings=serde_json::from_value(json!({"0":{"value":{"source":"workflow_input","field":"value"},"index":{"source":"workflow_input","field":"index"}}})).unwrap();
    child.output_bindings = serde_json::from_value(
        json!({"next":{"source":"activity_output","activity":0,"field":"next"}}),
    )
    .unwrap();
    let child = publish(&f, &store, &catalog, "page-child", child);
    let call = subflow(&child, "page-subflow");
    save(&f, &store, call.clone());
    let mut http = run::native_family_tests::http();
    http.target = Some(ReusableTarget::Http {
        connector: run::neutral_http_fixture::contract(true),
        plugin: "workflow-http-test".into(),
        base: base.clone(),
    });
    http.outputs = schema(json!({"items":field("array")}));
    http.config.fields.get_mut("request").unwrap().default =
        Some(serde_json::to_value(run::neutral_http_fixture::operation(&base, true)).unwrap());
    let catalog = save(&f, &store, http.clone());
    let mut root = graph(&[http, call]);
    root.outputs = schema(json!({"items":field("array")}));
    root.outcomes = ["done".into(), "blocked".into()].into();
    root.control=serde_json::from_value(json!({"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},"each":{"type":"collection","scope":"pages"},"check":{"type":"condition","condition":"all"},"end":{"type":"end","outcome":"done"},"blocked":{"type":"end","outcome":"blocked"}},
        "edges":{"start":{"next":"work"},"work":{"complete":"each"},"each":{"complete":"check","blocked":"blocked"},"check":{"true":"end","false":"blocked","unknown":"blocked"}}})).unwrap();
    root.collections=serde_json::from_value(json!({"pages":{"collection":{"source":"activity_output","activity":0,"field":"items"},"item_type":"integer","body":1,"item_input":"value","index_input":"index","max_items":4,"max_concurrency":2,"empty":"complete","failure":"collect"}})).unwrap();
    root.conditions=serde_json::from_value(json!({"all":{"operator":"compare","comparison":"equal","left":{"source":"scope_output","scope":"pages","field":"completed"},"right":{"source":"constant","value":3}}})).unwrap();
    root.output_bindings = serde_json::from_value(
        json!({"items":{"source":"scope_output","scope":"pages","field":"items"}}),
    )
    .unwrap();
    let publication = publish(&f, &store, &catalog, "pages-parent", root);
    let finished = run(&f, &publication, json!({}), None).await;
    assert_eq!(
        finished.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        },
        "{finished:?}"
    );
    let items = finished.outputs.as_ref().unwrap().values["items"]
        .as_array()
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i["outputs"]["next"].clone())
            .collect::<Vec<_>>(),
        vec![json!(31), json!(11), json!(21)]
    );
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /api/records?cursor=page2 HTTP/1.1\r\n"));
}
