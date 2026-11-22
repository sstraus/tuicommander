use super::*;

pub(super) async fn post_action(
    State(state): State<Arc<AppState>>,
    caller: Option<Extension<ConnectInfo<SocketAddr>>>,
    auth: Option<Extension<guards::Authenticated>>,
    user: Option<Extension<guards::UserAuthenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(action): Json<crate::workflows::neutral_api::model::NeutralAction>,
) -> Response {
    let addr = caller.map_or(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        |Extension(ConnectInfo(addr))| addr,
    );
    if let Some(response) = progress_auth(&addr, auth.is_some()) {
        return response;
    }
    let authority = guards::CallerAuthority::from_request(addr, user.map(|Extension(user)| user));
    if matches!(
        &action,
        crate::workflows::neutral_api::model::NeutralAction::ReplayTrigger { .. }
    ) && !authority.is_owner()
    {
        return (StatusCode::FORBIDDEN,Json(serde_json::json!({"error":"trigger replay requires authenticated owner authority"}))).into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::workflows::neutral_api::neutral_action(
            &state,
            &q.path,
            action,
            authority
                .is_owner()
                .then_some(crate::workflows::NativeCaller::Owner),
        )
    })
    .await
    .map_err(|e| format!("neutral workflow task failed: {e}"))
    .and_then(|r| r);
    neutral_result(result)
}

/// A caller refusal answers its 4xx; every other failure stays a 500 fault.
/// The body is the unchanged `{"error": text}` that MCP also returns.
fn neutral_result(
    result: Result<crate::workflows::neutral_api::model::NeutralReply, String>,
) -> Response {
    use crate::workflows::neutral_api::refusal::{Refusal, classify};
    match result {
        Err(error) => match classify(&error) {
            Some(kind) => (
                match kind {
                    Refusal::NotFound => StatusCode::NOT_FOUND,
                    Refusal::Conflict => StatusCode::CONFLICT,
                    Refusal::Invalid => StatusCode::BAD_REQUEST,
                    Refusal::Forbidden => StatusCode::FORBIDDEN,
                },
                Json(serde_json::json!({"error": error})),
            )
                .into_response(),
            None => err_500(&error),
        },
        result => json_result(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    // Catches: HTTP admission without verified credentials manufactures replay
    // owner authority, while a genuinely authenticated owner is rejected.
    #[tokio::test]
    async fn neutral_transport_replay_requires_verified_owner_not_transport_admission() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = project.path().canonicalize().unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        for verified in [false, true] {
            let mut request=Request::builder().method("POST").uri(format!("/workflows/neutral/action?path={}",path.display())).header("content-type","application/json").body(Body::from(serde_json::json!({"action":"replay_trigger","binding_id":"binding","resource":"resource","generation":"generation","replay_id":"replay"}).to_string())).unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 1234))));
            request.extensions_mut().insert(guards::Authenticated);
            if verified {
                request.extensions_mut().insert(guards::UserAuthenticated);
            }
            let response = build_router(state.clone(), false, true)
                .oneshot(request)
                .await
                .unwrap();
            if !verified {
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
                let value: serde_json::Value = serde_json::from_slice(
                    &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                )
                .unwrap();
                assert_eq!(
                    value["error"],
                    "trigger replay requires authenticated owner authority"
                );
            } else {
                assert_ne!(response.status(), StatusCode::FORBIDDEN);
                let value: serde_json::Value = serde_json::from_slice(
                    &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                )
                .unwrap();
                assert_eq!(value["error"], "native catalog not installed");
            }
        }
    }
    async fn post_neutral(
        state: &Arc<AppState>,
        path: &std::path::Path,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri(format!("/workflows/neutral/action?path={}", path.display()))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1234))));
        request.extensions_mut().insert(guards::Authenticated);
        let response = build_router(state.clone(), false, true)
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        (status, value)
    }

    // Catches: a caller refusal (unknown id, stale revision, bad input) answers
    // 500, so a client cannot tell it from a server fault and may retry it.
    #[tokio::test]
    async fn neutral_transport_refusals_answer_4xx_with_unchanged_text() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = project.path().canonicalize().unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let definition = serde_json::json!({"control":{"nodes":{"start":{"type":"start"},"end":{"type":"end","outcome":"done"}},"edges":{"start":{"next":"end"}}},"inputs":{"fields":{}},"outputs":{"fields":{}},"outcomes":["done"],"subject":null,"instances":[],"input_bindings":{},"conditions":{},"collections":{},"loops":{},"forks":{},"output_bindings":{},"subflows":[]});
        let (status, draft) = post_neutral(
            &state,
            &path,
            serde_json::json!({"action":"create_draft","name":"d","definition":definition}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{draft}");
        let id = draft["value"]["id"].as_str().unwrap().to_owned();
        let cases = [
            (
                "unknown draft",
                serde_json::json!({"action":"get_draft","id":"missing"}),
                StatusCode::NOT_FOUND,
                "neutral draft not found in project",
            ),
            (
                "unknown run",
                serde_json::json!({"action":"inspect","run_id":"missing","after_sequence":0,"limit":10}),
                StatusCode::NOT_FOUND,
                "read neutral run identity: Query returned no rows",
            ),
            (
                "stale draft revision",
                serde_json::json!({"action":"update_draft","id":id,"expected_revision":99,"name":"n","definition":definition}),
                StatusCode::CONFLICT,
                "neutral draft revision changed or project does not own draft",
            ),
            (
                "bad cursor",
                serde_json::json!({"action":"inspect","run_id":"missing","after_sequence":0,"limit":0}),
                StatusCode::BAD_REQUEST,
                "invalid neutral event cursor or limit",
            ),
            (
                "empty draft name",
                serde_json::json!({"action":"create_draft","name":" ","definition":definition}),
                StatusCode::BAD_REQUEST,
                "neutral draft requires a bounded nonempty name",
            ),
        ];
        for (name, body, expected, text) in cases {
            let (status, value) = post_neutral(&state, &path, body).await;
            assert_eq!(status, expected, "{name}: {value}");
            assert_eq!(value["error"], text, "{name}");
        }
    }

    // Catches: the browser run list action is unmapped or requires an execution host just to read history.
    #[tokio::test]
    async fn neutral_transport_lists_project_runs_without_an_installed_host() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = project.path().canonicalize().unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let (status, value) =
            post_neutral(&state, &path, serde_json::json!({"action":"list_runs"})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value, serde_json::json!({"type":"runs","value":[]}));
    }
}
