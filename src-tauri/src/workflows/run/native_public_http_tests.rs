use super::super::native_test_support::Fixture;
use super::native_public_support::{attach, request};
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
