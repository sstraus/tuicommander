use crate::{
    automations::api,
    mcp_http::{build_router, tests::test_state},
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

// Catches: HTTP envelope drift, validation bypass, or dispatch on a non-owning backend.
#[tokio::test]
async fn automation_http_matches_shared_ipc_results_and_errors() {
    let state = test_state();
    for input in [
        json!({"action":"preset","preset":{"kind":"daily","hour":9,"minute":30}}),
        json!({"action":"preview","cron":"0 9 * * *","timezone":"UTC","count":0}),
        json!({"action":"list","unknown":true}),
        json!({"action":"run_now","id":"a"}),
    ] {
        let expected = api::execute(input.clone(), state.clone()).await;
        let response = build_router(state.clone(), false, true)
            .oneshot(
                Request::post("/automations/action")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"input": input}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        match expected {
            Ok(value) => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body, value);
                assert_eq!(body, json!({"cron":"30 9 * * *"}));
            }
            Err(error) => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body, json!({"error": error}));
            }
        }
    }
}
