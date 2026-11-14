//! Diagnostic-only HTTP control plane, shared by desktop and headless.
//! No separate Tauri command: desktop operators use this same HTTP surface.
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum Control {
    Activate,
    Deactivate,
    Dump { path: String },
}

type Response = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn response(result: Result<Value, String>) -> Response {
    result.map(Json).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
    })
}
pub(crate) async fn status() -> Response {
    response(crate::heap_profile::status())
}
pub(crate) async fn control(Json(action): Json<Control>) -> Response {
    let result = tokio::task::spawn_blocking(move || match action {
        Control::Activate => crate::heap_profile::set_active(true),
        Control::Deactivate => crate::heap_profile::set_active(false),
        Control::Dump { path } => crate::heap_profile::dump(&path),
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    response(result)
}

#[cfg(all(test, not(feature = "heap-profile")))]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn unavailable_profiler_is_an_http_error_for_status_and_controls() {
        // Catches: the HTTP adapter wraps an unavailable backend as a 200.
        let router = axum::Router::new().route(
            "/diagnostics/heap",
            axum::routing::get(status).post(control),
        );
        for (method, body) in [
            ("GET", ""),
            ("POST", r#"{"action":"activate"}"#),
            ("POST", r#"{"action":"dump","path":"unused.heap"}"#),
        ] {
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri("/diagnostics/heap")
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(body["error"].as_str().unwrap().contains("unavailable"));
        }
    }
}
