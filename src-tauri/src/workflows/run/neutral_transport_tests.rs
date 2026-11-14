use super::*;
use crate::workflows::{catalog::*, neutral_api::model::*};
use serde_json::{Value, json};

use super::neutral_transport_fixture::{call, source, state};

// Catches: HTTP buffers numeric mapping keys incorrectly, changes reply casing,
// loses editor metadata, accepts stale writes or crosses project ownership.
#[tokio::test]
async fn neutral_transport_draft_roundtrip_preserves_mapping_keys_and_project_revision() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let (project, _source, definition) = source();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = state();
    let input = json!({"action":"create_draft","name":"neutral","definition":definition,"editor":{"labels":{"work":"Classifier"},"presets":{}}});
    let request = Request::builder()
        .method("POST")
        .uri(format!("/workflows/neutral/action?path={path}"))
        .header("content-type", "application/json")
        .body(Body::from(input.to_string()))
        .unwrap();
    let response = crate::mcp_http::build_router(state.clone(), false, true)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let reply: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["type"], "draft");
    assert_eq!(
        reply["value"]["definition"]["input_bindings"]["0"]["value"]["field"],
        "value"
    );
    let id = reply["value"]["id"].as_str().unwrap();
    assert_eq!(
        call(&state, &path, json!({"action":"get_draft","id":id})).unwrap(),
        reply
    );
    let updated=call(&state,&path,json!({"action":"update_draft","id":id,"expected_revision":1,"name":"renamed","definition":definition,"editor":{"labels":{"work":"Reviewer"}}})).unwrap();
    assert_eq!(updated["value"]["revision"], 2);
    assert!(call(&state,&path,json!({"action":"update_draft","id":id,"expected_revision":1,"name":"stale","definition":definition})).is_err());
    let foreign = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    assert!(
        call(
            &state,
            foreign.path().to_str().unwrap(),
            json!({"action":"get_draft","id":id})
        )
        .is_err()
    );
    assert!(
        call(&state, &path, json!({"action":"catalog"}))
            .unwrap_err()
            .contains("native catalog not installed")
    );
    assert!(
        serde_json::from_value::<NeutralAction>(json!({"action":"catalog","grants":["write"]}))
            .is_err()
    );
}

// Catches: client labels grant native rights, publication drops internal graphs,
// cloning changes pinned content, preview executes effects, or mapping errors lose their link.
#[tokio::test]
async fn neutral_transport_catalog_publication_preview_keep_native_authority() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let (project, source, definition) = source();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = state();
    state
        .workflow_runtime
        .install_neutral_catalog_host(source.clone());
    let catalog = call(&state, &path, json!({"action":"catalog"})).unwrap();
    assert_eq!(
        catalog["value"]["activities"][0]["id"],
        json!(source.template.descriptor.id)
    );
    let draft=call(&state,&path,json!({"action":"create_draft","name":"operation","definition":definition,"editor":{"labels":{"work":"Reviewer"}}})).unwrap();
    let id = draft["value"]["id"].as_str().unwrap();
    source.denied.store(true, Ordering::Release);
    assert!(
        call(
            &state,
            &path,
            json!({"action":"publish","id":id,"expected_revision":1})
        )
        .unwrap_err()
        .contains("native authority refused")
    );
    source.denied.store(false, Ordering::Release);
    let pinned = call(
        &state,
        &path,
        json!({"action":"publish","id":id,"expected_revision":1}),
    )
    .unwrap();
    assert_eq!(pinned["value"]["definition"], json!(definition));
    assert_eq!(pinned["value"]["editor"]["labels"]["work"], "Reviewer");
    let discovered = call(&state, &path, json!({"action":"catalog"})).unwrap();
    assert_eq!(discovered["value"]["metanodes"], json!([pinned["value"]]));

    assert!(
        call(
            &state,
            &path,
            json!({"action":"publish","id":id,"expected_revision":1})
        )
        .is_err()
    );
    let changed=call(&state,&path,json!({"action":"update_draft","id":id,"expected_revision":1,"name":"operation","definition":definition,"editor":{"labels":{"work":"Writer"}}})).unwrap();
    assert_eq!(
        content_digest(&changed["value"]["definition"]).unwrap(),
        content_digest(&pinned["value"]["definition"]).unwrap()
    );
    let pinned_again = call(
        &state,
        &path,
        json!({"action":"get_publication","id":id,"revision":1}),
    )
    .unwrap();
    assert_eq!(pinned_again, pinned);
    let clone = call(
        &state,
        &path,
        json!({"action":"clone_publication","id":id,"revision":1,"name":"copy"}),
    )
    .unwrap();
    assert_ne!(clone["value"]["id"], id);
    assert_eq!(clone["value"]["definition"], json!(definition));
    let preview = call(
        &state,
        &path,
        json!({"action":"preview","definition":definition,"inputs":{"value":7},"subject":null}),
    )
    .unwrap();
    assert_eq!(
        preview["value"]["activities"][0]["inputs"]["value"]["state"]["value"],
        7
    );
    assert_eq!(
        preview["value"]["activities"][0]["effect_intent"]["mutation"],
        false
    );
    assert_eq!(source.calls.load(Ordering::Acquire), 0);
    let mut invalid = json!(definition);
    invalid["input_bindings"]["0"]["value"] = json!({"source":"constant","value":"wrong type"});
    let validation = call(
        &state,
        &path,
        json!({"action":"validate","definition":invalid}),
    )
    .unwrap();
    assert_eq!(validation["value"]["valid"], false);
    assert!(
        validation["value"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["path"] == "/definition/input_bindings/0/value")
    );
    if std::env::var_os("TUIC_NEUTRAL_FIXTURE_CAPTURE").is_some() {
        println!(
            "WER015_AUTHORING_FIXTURE={}",
            json!({"provenance":"Observed neutral authoring services with internal host.increment fixture; not production native provider or external service evidence","project":path,"catalog":discovered,"draft":draft,"updated_draft":changed,"publication":pinned,"clone":clone,"preview":preview,"validation":validation})
        );
    }
}

// Catches: production host installation misses the existing owner, neutral wait
// and result commits emit no lifecycle cursor, or inspectors lose retained output.
#[tokio::test]
async fn neutral_transport_owner_emits_wait_and_result_cursors_and_retains_inspection() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let (project, source, definition) = source();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = state();
    let owner = RuntimeOwner::acquire(&config.path().join("workflow_runs.sqlite3")).unwrap();
    *owner.state.lock() = Arc::downgrade(&state);
    *state.workflow_runtime.owner.lock() = Some(owner.clone());
    *state.workflow_runtime.state.lock() = Arc::downgrade(&state);
    state
        .workflow_runtime
        .install_neutral_catalog_host(source.clone());
    let draft = call(
        &state,
        &path,
        json!({"action":"create_draft","name":"run","definition":definition}),
    )
    .unwrap();
    let id = draft["value"]["id"].as_str().unwrap();
    call(
        &state,
        &path,
        json!({"action":"publish","id":id,"expected_revision":1}),
    )
    .unwrap();
    let mut bus = state.event_bus.subscribe();
    let request = json!({"publication":{"project":path,"id":id,"revision":1},"request_id":"start-once","inputs":{"value":7},"subject":null,"limits":{"max_activations":20,"max_concurrency":1,"max_duration_ms":60000}});
    let started = call(&state, &path, json!({"action":"start","request":request})).unwrap();
    let run_id = started["value"]["snapshot"]["run"]["id"].as_str().unwrap();
    let finished = loop {
        let event = bus.recv().await.unwrap();
        if let crate::state::AppEvent::WorkflowRunChanged { repo_path, payload } = event {
            assert_eq!(repo_path, path);
            assert_eq!(payload["runId"], run_id);
            let view = call(
                &state,
                &path,
                json!({"action":"inspect","run_id":run_id,"after_sequence":0,"limit":500}),
            )
            .unwrap();
            if view["value"]["snapshot"]["status"]["type"] == "completed" {
                break view;
            }
        }
    };
    assert_eq!(
        finished["value"]["snapshot"]["outputs"]["values"]["answer"],
        8
    );
    let events = &finished["value"]["events"];
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"]["decision"]["type"] == "arm_wait")
    );
    assert_eq!(source.calls.load(Ordering::Acquire), 1);
    let retry = call(&state, &path, json!({"action":"start","request":request})).unwrap();
    assert_eq!(retry["value"]["snapshot"]["run"]["id"], run_id);
    assert_eq!(source.calls.load(Ordering::Acquire), 1);
    let page = call(
        &state,
        &path,
        json!({"action":"events","run_id":run_id,"after_sequence":1,"limit":1}),
    )
    .unwrap();
    assert_eq!(page["value"][0]["sequence"], 2);
    if std::env::var_os("TUIC_NEUTRAL_FIXTURE_CAPTURE").is_some() {
        let catalog = call(&state, &path, json!({"action":"catalog"})).unwrap();
        let publication = call(
            &state,
            &path,
            json!({"action":"get_publication","id":id,"revision":1}),
        )
        .unwrap();
        println!(
            "WER015_ENGINE_FIXTURE={}",
            json!({"provenance":"Observed real neutral engine ledger with internal host.increment fixture; not production native provider or external service evidence","project":path,"catalog":catalog,"publication":publication,"inspection":finished})
        );
    }
}
