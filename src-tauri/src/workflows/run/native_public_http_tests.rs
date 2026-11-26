use super::super::NativeCaller;
use super::super::native_test_support::Fixture;
use super::native_public_support::{answer, attach, call, commit_request, ready, request};
use crate::workflows::neutral_api::model::NeutralAction;
use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use std::net::SocketAddr;
use tower::ServiceExt;

// Catches: remote unauthenticated start gaining owner rights, or a verified
// session cookie not reaching the native binding through the real HTTP stack.
#[tokio::test]
async fn native_public_http_start_requires_verified_remote_credentials() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    attach(&f);
    let start = request(&f, "workspace.pin", json!({}));
    *f.state.session_token.write() = "native-fixture-owner".into();
    for verified in [false, true] {
        let mut request = Request::builder()
            .method("POST")
            .uri(format!(
                "/workflows/neutral/action?path={}",
                f.context.workspace_path().display()
            ))
            .header("host", "localhost")
            .header("content-type", "application/json");
        if verified {
            request = request.header("cookie", "tui-session=native-fixture-owner");
        }
        let mut request = request
            .body(Body::from(
                json!({"action":"start","request":start}).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 1234))));
        let response = crate::mcp_http::build_router(f.state.clone(), true, true)
            .oneshot(request)
            .await
            .unwrap();
        if !verified {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        } else {
            assert_eq!(response.status(), StatusCode::OK);
            let value: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            let run = value["value"]["snapshot"]["run"]["id"].as_str().unwrap();
            let host = f.state.workflow_runtime.neutral_catalog_host().unwrap();
            host.authorize_answer(
                &f.owner.store,
                run,
                Some(&super::super::NativeCaller::Owner),
            )
            .unwrap();
            host.resume_run(
                &f.owner.store,
                run,
                Some(&super::super::NativeCaller::Owner),
                f.context.workspace_path().to_str().unwrap(),
            )
            .unwrap();
        }
    }
}

// Catches: a start that names a publication revision the project never published
// fails on a wrapped or earlier error, so HTTP still answers 500 instead of 404.
#[tokio::test]
async fn native_public_http_start_of_unknown_publication_answers_404() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    attach(&f);
    let mut start = request(&f, "workspace.pin", json!({}));
    start.publication.revision += 7;
    *f.state.session_token.write() = "native-fixture-owner".into();
    let mut request = Request::builder()
        .method("POST")
        .uri(format!(
            "/workflows/neutral/action?path={}",
            f.context.workspace_path().display()
        ))
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("cookie", "tui-session=native-fixture-owner")
        .body(Body::from(
            json!({"action":"start","request":start}).to_string(),
        ))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 1234))));
    let response = crate::mcp_http::build_router(f.state.clone(), true, true)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND, "{value}");
    assert!(
        value["error"]
            .as_str()
            .unwrap()
            .starts_with("neutral publication not found: "),
        "{value}"
    );
}

// Catches: an answer to a question whose revision moved on is a refusal, but a
// wrapped or reworded error makes the real router answer 500 instead of 409,
// and a client retries it as a server fault.
#[tokio::test]
async fn native_public_http_stale_answer_answers_409_and_changes_nothing() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    attach(&f);
    let run = call(
        &f,
        NeutralAction::Start {
            request: request(&f, "commit.create", commit_request()),
        },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let waiting = f.dispatch(&ready(&f, run)).await;
    *f.state.session_token.write() = "native-fixture-owner".into();
    let mut http = Request::builder()
        .method("POST")
        .uri(format!(
            "/workflows/neutral/action?path={}",
            f.context.workspace_path().display()
        ))
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("cookie", "tui-session=native-fixture-owner")
        .body(Body::from(
            serde_json::to_string(&answer(&waiting, 2)).unwrap(),
        ))
        .unwrap();
    http.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 1234))));
    let response = crate::mcp_http::build_router(f.state.clone(), true, true)
        .oneshot(http)
        .await
        .unwrap();
    let status = response.status();
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{value}");
    assert_eq!(
        value["error"],
        "neutral approval is expired, stale or already answered"
    );
    let after = f.owner.store.neutral_execution(&waiting.run.id).unwrap();
    assert_eq!(after.sequence, waiting.sequence);
    assert!(after.effects.values().next().unwrap().answer.is_none());
}
