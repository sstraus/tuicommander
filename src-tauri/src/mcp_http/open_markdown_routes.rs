use crate::{
    AppState,
    open_markdown_tabs::{self, DesktopMarkdownTab},
};
use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};

#[derive(Deserialize)]
pub(super) struct ListQuery {
    cwd: String,
}
#[derive(Deserialize)]
pub(super) struct PublishBody {
    tabs: Vec<DesktopMarkdownTab>,
}

pub(super) async fn get_tabs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err(error) = super::validate_repo_path(&query.cwd) {
        return error.into_response();
    }
    match open_markdown_tabs::list(state, query.cwd).await {
        Ok(tabs) => Json(tabs).into_response(),
        Err(error) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    }
}

/// Publication is a local desktop operation; remote clients can only read the scoped list.
pub(super) async fn put_tabs(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<PublishBody>,
) -> Response {
    if let Err(error) = super::guards::localhost_only(&addr) {
        return error.into_response();
    }
    super::json_result(open_markdown_tabs::publish(&state, body.tabs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    // Catches: a remote client replacing the authoritative desktop snapshot via the publication twin.
    #[tokio::test]
    async fn open_markdown_publication_refuses_remote_writes() {
        let state = super::super::tests::test_state();
        let response = put_tabs(
            State(state),
            ConnectInfo("100.64.0.2:443".parse().unwrap()),
            Json(PublishBody { tabs: vec![] }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // Catches: a new route bypassing auth or serving an unregistered directory's tabs.
    #[tokio::test]
    #[serial_test::serial]
    async fn open_markdown_read_uses_existing_auth_and_registered_repository_scope() {
        let cfg = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(cfg.path().to_path_buf());
        crate::config::replace_repositories_for_test(
            serde_json::json!({"repos":{"/repo":{},"/other":{}}}),
        )
        .unwrap();
        let state = super::super::tests::test_state();
        open_markdown_tabs::publish(
            &state,
            vec![
                DesktopMarkdownTab {
                    id: "visible".into(),
                    repo_path: "/repo".into(),
                    file_path: "docs/guide.md".into(),
                    fs_root: None,
                    opener_tuic_session: None,
                },
                DesktopMarkdownTab {
                    id: "foreign".into(),
                    repo_path: "/other".into(),
                    file_path: "secret.md".into(),
                    fs_root: None,
                    opener_tuic_session: None,
                },
            ],
        )
        .unwrap();
        *state.session_token.write() = "test-markdown-token".into();
        let router = super::super::build_router(state, true, false);
        for (path, token, expected) in [
            (
                "/repo/open-markdown-tabs?cwd=%2Frepo",
                false,
                StatusCode::UNAUTHORIZED,
            ),
            ("/repo/open-markdown-tabs?cwd=%2Frepo", true, StatusCode::OK),
            (
                "/repo/open-markdown-tabs?cwd=%2Felsewhere",
                true,
                StatusCode::FORBIDDEN,
            ),
        ] {
            let uri = if token {
                format!("{path}&token=test-markdown-token")
            } else {
                path.into()
            };
            let request = Request::builder()
                .uri(uri)
                .header("Host", "127.0.0.1:9876")
                .extension(ConnectInfo("100.64.0.2:443".parse::<SocketAddr>().unwrap()))
                .body(Body::empty())
                .unwrap();
            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected, "{path}, authenticated={token}");
            if expected == StatusCode::OK {
                let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let tabs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    tabs,
                    serde_json::json!([{"id":"visible","title":"guide.md","relative_path":"docs/guide.md","absolute_path":"/repo/docs/guide.md","fs_root":"/repo"}])
                );
            }
        }
    }
}
