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
    json_result(result)
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
}
