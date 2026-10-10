use crate::mcp_http::{build_router, tests::test_state};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

// Catches: missing automation endpoints returning the SPA HTML with HTTP 200.
#[tokio::test]
async fn unknown_automation_endpoint_returns_json_404_instead_of_spa_success() {
    let response = build_router(test_state(), false, true)
        .oneshot(
            Request::post("/automations/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"error":"no such endpoint: /automations/missing"})
    );
}
