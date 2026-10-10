//! Thin automation adapter shared by desktop and headless HTTP routers.
use super::*;

#[derive(serde::Deserialize)]
pub(super) struct AutomationRequest {
    input: serde_json::Value,
}

pub(super) async fn post_action(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AutomationRequest>,
) -> Response {
    json_result(crate::automations::api::execute(request.input, state).await)
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "desktop"))]
mod critic_tests;
