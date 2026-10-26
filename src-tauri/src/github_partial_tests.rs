use super::*;

// Catches: rejecting a partial GraphQL response, or dropping healthy aliases with a denied sibling.
#[tokio::test]
async fn recorded_not_found_keeps_accessible_prs_and_issues() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/github-partial-monitoring-2336.json")).unwrap();
    let body = fixture["response"].clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = axum::Router::new().route(
        "/graphql",
        axum::routing::post(move |axum::Json(_request): axum::Json<serde_json::Value>| {
            let body = body.clone();
            async move { axum::Json(body) }
        }),
    );
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let response = graphql_request(
        &client,
        "test-only-credential",
        &format!("http://{address}/graphql"),
        fixture["request"]["query"].as_str().unwrap(),
        &serde_json::Value::Null,
    )
    .await
    .unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();

    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let repos = vec![
        ("accessible".into(), "sstraus".into(), "tuicommander".into()),
        (
            "denied".into(),
            "sstraus".into(),
            "tuic-2336-nonexistent-fixture".into(),
        ),
    ];
    let aliases = vec![
        ("r0".into(), "accessible".into()),
        ("r1".into(), "denied".into()),
    ];
    let mut prs = std::collections::HashMap::new();
    let mut issues = std::collections::HashMap::new();
    adopt_batch_response(
        &state,
        &account,
        &repos,
        &aliases,
        false,
        true,
        "",
        &response,
        &mut prs,
        &mut issues,
    )
    .await
    .unwrap();
    assert_eq!(prs["accessible"].len(), 1);
    assert_eq!(prs["accessible"][0].number, 128);
    assert_eq!(issues["accessible"].len(), 1);
    assert_eq!(issues["accessible"][0].number, 121);
    assert!(!prs.contains_key("denied"));
    assert!(!issues.contains_key("denied"));
    assert_eq!(
        state.git_cache.github_status.get("accessible").unwrap()[0].number,
        128
    );
    assert_eq!(state.git_cache.github_repo_access_errors.len(), 1);
    assert!(
        state
            .git_cache
            .github_repo_cooldown
            .contains_key(&cooldown_key(
                &account,
                "sstraus",
                "tuic-2336-nonexistent-fixture"
            ))
    );
}

// Catches: coupling routine PR/issue monitoring to optional Projects permission.
#[tokio::test]
async fn batch_poll_never_selects_projects_fields() {
    let mut fixture = PollHttpFixture::start(false).await;
    let account = github_com_account(&fixture.state);
    let repos = vec![("local".into(), "sstraus".into(), "tuicommander".into())];
    for merged in [false, true] {
        for drafts in [false, true] {
            for filter in ["disabled", "all", "assigned", "created", "mentioned"] {
                let mut prs = Default::default();
                let mut issues = Default::default();
                poll_one_account(
                    &fixture.state,
                    &account,
                    None,
                    &repos,
                    merged,
                    filter,
                    drafts,
                    &mut prs,
                    &mut issues,
                )
                .await
                .unwrap();
                let request = fixture.requests.recv().await.unwrap();
                let query = request["query"].as_str().unwrap();
                assert!(
                    query.contains("pullRequests("),
                    "missing PR selection: {query}"
                );
                assert_eq!(query.contains("issues("), filter != "disabled");
                assert!(
                    !query.to_ascii_lowercase().contains("project"),
                    "Projects selection leaked into emitted BatchPoll: {query}"
                );
                assert_eq!(prs["local"][0].number, 128);
                if filter != "disabled" {
                    assert_eq!(issues["local"][0].number, 121);
                }
            }
        }
    }
    fixture.close().await;
}

/// Real polling requests use their production URL and credentials path; only DNS
/// and trust roots point at this loopback receiver. No live credentials are read.
pub(crate) struct PollHttpFixture {
    pub(crate) state: std::sync::Arc<AppState>,
    pub(crate) requests: tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
    handle: axum_server::Handle<std::net::SocketAddr>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl PollHttpFixture {
    pub(crate) async fn start(all_aliases_accessible: bool) -> Self {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/github-partial-monitoring-2336.json"))
                .unwrap();
        let response = fixture["response"].clone();
        let (tx, requests) = tokio::sync::mpsc::unbounded_channel();
        let sequence = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let aliases = regex::Regex::new(r"(r\d+):\s*repository").unwrap();
        let router = axum::Router::new().route(
            "/graphql",
            axum::routing::post(move |axum::Json(request): axum::Json<serde_json::Value>| {
                let mut response = response.clone();
                if all_aliases_accessible {
                    // Lifecycle simulator: every requested alias succeeds with
                    // the recorded accessible payload, not a new permission fixture.
                    let mut healthy = response["data"]["r0"].clone();
                    healthy["issues"]["nodes"][0]["updatedAt"] = serde_json::json!(format!(
                        "2026-10-25T00:00:{:02}Z",
                        sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    ));
                    response["data"]
                        .as_object_mut()
                        .unwrap()
                        .retain(|key, _| key == "rateLimit");
                    response.as_object_mut().unwrap().remove("errors");
                    for alias in aliases.captures_iter(request["query"].as_str().unwrap()) {
                        response["data"][&alias[1]] = healthy.clone();
                    }
                }
                tx.send(request).unwrap();
                async move { axum::Json(response) }
            }),
        );
        let cert = include_bytes!("fixtures/github_volume/cert.pem");
        let key = include_bytes!("fixtures/github_volume/key.pem");
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert.to_vec(), key.to_vec())
            .await
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = axum_server::Handle::new();
        let server = tokio::spawn(
            axum_server::from_tcp_rustls(listener, tls)
                .unwrap()
                .handle(handle.clone())
                .serve(router.into_make_service()),
        );
        let mut state = crate::state::tests_support::make_test_app_state();
        state.http_client = reqwest::Client::builder()
            .no_proxy()
            .tls_certs_only([reqwest::Certificate::from_pem(cert).unwrap()])
            .resolve("api.github.com", address)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();
        *state.github.token.write() = Some("test-only-credential".into());
        *state.github.viewer_login.write() = Some("sstraus".into());
        Self {
            state: std::sync::Arc::new(state),
            requests,
            handle,
            server,
        }
    }

    pub(crate) async fn close(self) {
        self.handle.shutdown();
        self.server.await.unwrap().unwrap();
    }
}
