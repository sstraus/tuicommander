use super::*;
use serde_json::json;

async fn read_tabs(state: Arc<AppState>, cwd: &str) -> (axum::http::StatusCode, serde_json::Value) {
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use tower::ServiceExt;
    let request = Request::builder()
        .uri(format!(
            "/repo/open-markdown-tabs?token=test-markdown-token&cwd={}",
            url::form_urlencoded::byte_serialize(cwd.as_bytes()).collect::<String>()
        ))
        .header("Host", "127.0.0.1:9876")
        .extension(ConnectInfo(
            "100.64.0.2:443".parse::<std::net::SocketAddr>().unwrap(),
        ))
        .body(Body::empty())
        .unwrap();
    let response = crate::mcp_http::build_router(state, true, false)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn menu(
    tabs: Vec<DesktopMarkdownTab>,
    cwd: &str,
    config: serde_json::Value,
) -> serde_json::Value {
    let cfg = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(cfg.path().to_path_buf());
    crate::config::replace_repositories_for_test(config).unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    *state.session_token.write() = "test-markdown-token".into();
    publish(&state, tabs).unwrap();
    let (status, result) = read_tabs(state.clone(), cwd).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        read_tabs(state, "/unregistered").await.0,
        axum::http::StatusCode::FORBIDDEN
    );
    result
}

fn config() -> serde_json::Value {
    json!({"repos": {"/repo": {"workspaces": {"feature": {"worktreePath": "/worktrees/feature"}}}, "/repo/nested": {}, "/other": {}}})
}
fn tab(id: &str, repo: &str, file: &str, root: Option<&str>) -> DesktopMarkdownTab {
    DesktopMarkdownTab {
        id: id.into(),
        repo_path: repo.into(),
        file_path: file.into(),
        fs_root: root.map(str::to_owned),
        opener_tuic_session: None,
    }
}

// Catches: leaking another repository or accidentally assigning a nested session to its parent.
#[tokio::test]
#[serial_test::serial]
async fn menu_scopes_titles_and_relative_paths_to_the_deepest_registered_repository() {
    let tabs = vec![
        tab("a", "/repo", "docs/guide.md", None),
        tab("b", "/other", "private.md", None),
        tab("c", "/repo/nested", "README.md", None),
    ];
    let list = menu(tabs.clone(), "/repo/src", config()).await;
    assert_eq!(
        list,
        json!([{"id":"a","title":"guide.md","relative_path":"docs/guide.md","absolute_path":"/repo/docs/guide.md","fs_root":"/repo"}])
    );
    let nested = menu(tabs, "/repo/nested/src", config()).await;
    assert_eq!(
        nested
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["c"]
    );
}

// Catches: linked-worktree sessions missing their repository tabs or opening a different checkout's file.
#[tokio::test]
#[serial_test::serial]
async fn worktree_tabs_keep_their_own_file_root_in_the_mobile_viewer_address() {
    let tabs = vec![
        tab("main", "/repo", "README.md", None),
        tab(
            "feature",
            "/repo",
            "/worktrees/feature/docs/guide.md",
            Some("/worktrees/feature"),
        ),
    ];
    let list = menu(tabs, "/worktrees/feature/src", config()).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[1]["absolute_path"], "/worktrees/feature/docs/guide.md");
    assert_eq!(list[1]["relative_path"], "docs/guide.md");
    assert_eq!(list[1]["fs_root"], "/worktrees/feature");
}

// Catches: exposing traversal, sibling-repository files, unregistered roots, or non-Markdown tabs.
#[tokio::test]
#[serial_test::serial]
async fn menu_omits_files_outside_registered_roots_and_non_markdown_files() {
    let tabs = vec![
        tab("a", "/repo", "../secret.md", None),
        tab("b", "/repo", "/repo-other/secret.md", None),
        tab("c", "/repo", "src/main.ts", None),
        tab("d", "/repo", "secret.md", Some("/unregistered")),
        tab("e", "/unregistered", "secret.md", None),
        tab("ok", "/repo", "docs/UPPER.MD", None),
    ];
    let list = menu(tabs, "/repo", config()).await;
    assert_eq!(
        list.as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["ok"]
    );
}

// Catches: host-only absolute path checks losing Windows tabs when the backend runs on Unix.
#[tokio::test]
#[serial_test::serial]
async fn windows_file_paths_have_relative_menu_labels_and_absolute_viewer_addresses() {
    let cfg = json!({"repos": {"C:\\repo": {}}});
    let tabs = vec![tab("a", "C:\\repo", "C:\\repo\\docs\\guide.md", None)];
    let list = menu(tabs, "C:\\repo\\src", cfg).await;
    assert_eq!(list[0]["relative_path"], "docs/guide.md");
    assert_eq!(list[0]["absolute_path"], "C:/repo/docs/guide.md");
}

// Catches: closes leaving a stale snapshot, missing invalidation, or content leaking into the event.
#[test]
fn desktop_open_and_close_publish_invalidation_and_replace_the_snapshot() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut events = state.event_bus.subscribe();
    let tabs = vec![tab("a", "/repo", "guide.md", None)];
    publish(&state, tabs.clone()).unwrap();
    let event = events.try_recv().unwrap();
    assert_eq!(
        crate::mcp_http::sse_routes::event_type_name_for_test(&event),
        "open-markdown-tabs-changed"
    );
    assert_eq!(
        crate::mcp_http::sse_routes::event_payload_for_test(&event),
        json!({})
    );
    assert_eq!(
        select_tabs(&state.open_markdown_tabs.read(), "/repo", &config())
            .unwrap()
            .len(),
        1
    );
    publish(&state, tabs).unwrap();
    assert!(
        events.try_recv().is_err(),
        "unchanged publications must not emit redundant events"
    );
    publish(&state, vec![]).unwrap();
    assert!(matches!(
        events.try_recv().unwrap(),
        AppEvent::OpenMarkdownTabsChanged
    ));
    assert_eq!(
        select_tabs(&state.open_markdown_tabs.read(), "/repo", &config()).unwrap(),
        [] as [crate::open_markdown_tabs::OpenMarkdownTab; 0]
    );
}

// Catches: the mobile menu loses the document opener and targets the session browsing the menu.
#[test]
fn mobile_menu_retains_the_desktop_document_opener() {
    let mut document = tab("review", "/repo", "review.md", None);
    document.opener_tuic_session = Some("tuic-b".into());
    let tabs = select_tabs(&[document], "/repo", &config()).unwrap();
    assert_eq!(
        serde_json::to_value(&tabs).unwrap()[0]["opener_tuic_session"],
        "tuic-b"
    );
}
