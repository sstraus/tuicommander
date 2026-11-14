//! Explicit paged editor reads on the authenticated remote (phone) router.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "editor-page-token";
const PAGE: u64 = 1024 * 1024;

struct Fixture {
    _config: tempfile::TempDir,
    repo: tempfile::TempDir,
    outside: tempfile::TempDir,
}

/// One registered repository plus a directory outside every registered root.
fn fixture() -> (Fixture, impl Drop) {
    let root = crate::test_support::test_temp_root();
    let config = tempfile::TempDir::new_in(&root).unwrap();
    let repo = tempfile::TempDir::new_in(&root).unwrap();
    let outside = tempfile::TempDir::new_in(&root).unwrap();
    let guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let repo_path = repo.path().to_str().unwrap();
    std::fs::write(
        config.path().join("repositories.json"),
        json!({ "repos": { repo_path: { "path": repo_path, "branches": {} } } }).to_string(),
    )
    .unwrap();
    (
        Fixture {
            _config: config,
            repo,
            outside,
        },
        guard,
    )
}

fn authed_state() -> Arc<crate::state::AppState> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    *state.session_token.write() = TOKEN.into();
    state.config.write().services.auth.lan_auth_bypass = false;
    state
}

fn remote_router() -> Router {
    crate::mcp_http::build_remote_router(authed_state())
}

async fn get(
    app: &Router,
    route: &str,
    query: &[(&str, &str)],
    authed: bool,
) -> (StatusCode, Value) {
    let mut encoded = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in query {
        encoded.append_pair(name, value);
    }
    let mut request = Request::get(format!("{route}?{}", encoded.finish()))
        .header("host", "localhost")
        .body(Body::empty())
        .unwrap();
    if authed {
        request
            .headers_mut()
            .insert("cookie", format!("tui-session={TOKEN}").parse().unwrap());
    }
    // A phone on the LAN, never loopback.
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [203, 0, 113, 5],
            5555,
        ))));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}

fn repo_query<'a>(repo: &'a Path, file: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("repoPath", repo.to_str().unwrap()), ("file", file)]
}

/// Read every page from offset 0, as the editor does, and return the joined bytes.
async fn read_all_pages(app: &Router, route: &str, base: &[(&str, &str)]) -> Vec<u8> {
    let mut joined = Vec::new();
    let mut pages = 0;
    loop {
        let offset = joined.len().to_string();
        let mut query = base.to_vec();
        query.push(("offset", &offset));
        let (status, page) = get(app, route, &query, true).await;
        assert_eq!(status, StatusCode::OK, "page at {offset}: {page}");
        assert_eq!(page["offset"], json!(joined.len()));
        assert!(
            page["bytesRead"].as_u64().unwrap() <= PAGE,
            "page bound: {}",
            page["bytesRead"]
        );
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(page["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(bytes.len() as u64, page["bytesRead"].as_u64().unwrap());
        joined.extend_from_slice(&bytes);
        pages += 1;
        if !page["hasMore"].as_bool().unwrap() {
            assert_eq!(
                page["totalSize"],
                json!(joined.len()),
                "final page ends at EOF"
            );
            break;
        }
        assert!(pages < 64, "paging must terminate");
    }
    joined
}

// Catches: the phone cannot open a file the desktop editor supports, because the
// remote editor routes ignore an explicit page request and refuse at the 10 MB cap.
#[tokio::test]
#[serial_test::serial]
async fn remote_editor_explicit_large_read_returns_pages() {
    let (fx, _config_guard) = fixture();
    let app = remote_router();
    // 12 MiB + 3: above the whole-read cap, and a UTF-8 sequence split by the
    // first page boundary so the bytes must survive paging untouched.
    let mut content = vec![b'a'; 12 * 1024 * 1024];
    content[(PAGE as usize - 1)..=(PAGE as usize)].copy_from_slice("é".as_bytes());
    content.extend_from_slice(b"end");
    let file = fx.repo.path().join("big.log");
    std::fs::write(&file, &content).unwrap();

    // Without an explicit page request the standard whole-read cap still applies.
    let (status, body) = get(
        &app,
        "/fs/read-editor",
        &repo_query(fx.repo.path(), "big.log"),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"].as_str().unwrap().contains("too large"),
        "{body}"
    );

    let repo_pages = read_all_pages(
        &app,
        "/fs/read-editor",
        &repo_query(fx.repo.path(), "big.log"),
    )
    .await;
    assert!(
        repo_pages == content,
        "repo pages must rebuild the exact file"
    );

    let external = [("path", file.to_str().unwrap())];
    let external_pages = read_all_pages(&app, "/fs/read-editor-external", &external).await;
    assert!(
        external_pages == content,
        "external pages must rebuild the exact file"
    );

    // A browser connected to the desktop router gets the same page contract.
    let desktop = crate::mcp_http::build_router(authed_state(), true, false);
    let mut first = repo_query(fx.repo.path(), "big.log");
    first.push(("offset", "0"));
    let (status, page) = get(&desktop, "/fs/read-editor", &first, true).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        (page["bytesRead"].as_u64(), page["hasMore"].as_bool()),
        (Some(PAGE), Some(true))
    );
}

// Catches: untrusted page parameters force whole-file buffering, a page request
// skips the editor ceiling or path validation, or reaches files without auth.
#[tokio::test]
#[serial_test::serial]
async fn remote_large_read_cannot_allocate_unbounded_or_bypass_auth() {
    let (fx, _config_guard) = fixture();
    let app = remote_router();
    std::fs::write(
        fx.repo.path().join("pages.txt"),
        vec![b'x'; PAGE as usize + 2],
    )
    .unwrap();
    let inside = fx.repo.path().join("pages.txt");
    let inside = inside.to_str().unwrap();
    let base = repo_query(fx.repo.path(), "pages.txt");

    // No credentials: refused before any read, on both editor routes.
    let mut paged = base.clone();
    paged.push(("offset", "0"));
    assert_eq!(
        get(&app, "/fs/read-editor", &paged, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(
            &app,
            "/fs/read-editor-external",
            &[("path", inside), ("offset", "0")],
            false
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );

    // An enormous length is clamped to one bounded page.
    let mut huge = base.clone();
    huge.push(("length", "18446744073709551615"));
    let (status, page) = get(&app, "/fs/read-editor", &huge, true).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["bytesRead"], PAGE);
    assert_eq!(page["clamped"], true);
    assert_eq!(page["hasMore"], true);

    // Past EOF is an empty final page; a malformed offset is rejected, never a whole read.
    let mut eof = base.clone();
    eof.push(("offset", "18446744073709551615"));
    let (status, page) = get(&app, "/fs/read-editor", &eof, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        page,
        json!({"data":"", "offset":18446744073709551615u64, "bytesRead":0,
        "totalSize":PAGE + 2, "hasMore":false, "clamped":false})
    );
    for bad in ["-1", "bad"] {
        let mut query = base.clone();
        query.push(("offset", bad));
        assert_eq!(
            get(&app, "/fs/read-editor", &query, true).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            get(
                &app,
                "/fs/read-editor-external",
                &[("path", inside), ("offset", bad)],
                true
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }

    // Paging keeps the desktop editor ceiling: above it nothing is served.
    let huge_file = std::fs::File::create(fx.repo.path().join("huge.log")).unwrap();
    huge_file
        .set_len(crate::MAX_EDITOR_LARGE_FILE_SIZE + 1)
        .unwrap();
    let mut over = repo_query(fx.repo.path(), "huge.log");
    over.push(("offset", "0"));
    let (status, body) = get(&app, "/fs/read-editor", &over, true).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"].as_str().unwrap().contains("too large"),
        "{body}"
    );

    // Path validation: page reads apply the same path checks as whole reads.
    let secret = fx.outside.path().join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    let secret = secret.to_str().unwrap();
    // Owner symlinks are followed (2651-ec2e); above-root and absolute paths stay refused.
    let escapes = ["../outside.txt".to_string(), secret.to_string()];
    for escape in &escapes {
        let mut query = repo_query(fx.repo.path(), escape);
        query.push(("offset", "0"));
        let (status, body) = get(&app, "/fs/read-editor", &query, true).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{escape}: {body}"
        );
        assert!(body.get("data").is_none(), "{escape} leaked: {body}");
    }
    // Absolute-path reads check path syntax only (2650-bc7c): a file outside every
    // registered root is readable, like the whole read; malformed paths are refused.
    let traversal = format!("{}/../{}", fx.repo.path().display(), "x");
    for path in [traversal.as_str(), "relative.txt"] {
        assert_eq!(
            get(
                &app,
                "/fs/read-editor-external",
                &[("path", path), ("offset", "0")],
                true
            )
            .await
            .0,
            StatusCode::BAD_REQUEST,
            "{path}"
        );
    }
}
