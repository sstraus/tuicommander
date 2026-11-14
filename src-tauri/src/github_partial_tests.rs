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
                    None,
                    &mut std::collections::HashMap::new(),
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
        let sequence = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let aliases = regex::Regex::new(r"(r\d+):\s*repository").unwrap();
        Self::with_response(move |request| {
            let mut response = response.clone();
            if all_aliases_accessible {
                let mut healthy = response["data"]["r0"].clone();
                healthy["issues"]["nodes"][0]["updatedAt"] = serde_json::json!(format!(
                    "2026-10-25T00:00:{:02}Z",
                    sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                if request["query"].as_str().unwrap().contains("intakeIssues:") {
                    healthy["intakeIssues"] = healthy["issues"].clone();
                    healthy["intakeIssues"]["pageInfo"] =
                        serde_json::json!({"hasNextPage": false, "endCursor": null});
                    // Full neutral intake selects labels pageInfo as well as
                    // nodes; model a complete GraphQL connection in the simulator.
                    for issue in healthy["intakeIssues"]["nodes"].as_array_mut().unwrap() {
                        issue["labels"]["pageInfo"] =
                            serde_json::json!({"hasNextPage":false,"endCursor":null});
                    }
                    if request["query"].as_str().unwrap().contains("filterBy:") {
                        healthy["issues"]["nodes"] = serde_json::json!([]);
                    }
                }
                response["data"]
                    .as_object_mut()
                    .unwrap()
                    .retain(|key, _| key == "rateLimit");
                response.as_object_mut().unwrap().remove("errors");
                for alias in aliases.captures_iter(request["query"].as_str().unwrap()) {
                    response["data"][&alias[1]] = healthy.clone();
                }
            }
            response
        })
        .await
    }

    async fn with_response(
        response: impl Fn(&serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
    ) -> Self {
        let response = std::sync::Arc::new(response);
        let (tx, requests) = tokio::sync::mpsc::unbounded_channel();
        let router = axum::Router::new().route(
            "/graphql",
            axum::routing::post(move |axum::Json(request): axum::Json<serde_json::Value>| {
                let response = response(&request);
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

// Catches a first-page-only intake that loses the 101st eligible issue, and a
// display-label cap that discards a server-filtered issue during admission.
#[tokio::test]
async fn intake_follows_pages_and_retains_labels_beyond_display_limit() {
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/github-intake-page-2415.json")).unwrap();
    let connection = &recorded["data"]["repository"]["issues"];
    let target = connection["nodes"][0].clone();
    let cursor = connection["pageInfo"]["endCursor"]
        .as_str()
        .unwrap()
        .to_string();
    let mut fixture = PollHttpFixture::with_response(move |request| {
        let query = request["query"].as_str().unwrap();
        assert!(query.contains("labels: [\"C-bug\"]"), "{query}");
        let continued = query.contains(&format!("after: {}", serde_json::to_string(&cursor).unwrap()));
        let nodes: Vec<_> = if continued {
            // Only the display labels are truncated; the server label filter matched.
            let mut node = target.clone();
            node["labels"]["nodes"] = serde_json::json!([]);
            vec![node]
        } else {
            (1..=100).map(|n| {
                let mut node = target.clone();
                node["number"] = serde_json::json!(200000 + n);
                node["url"] = serde_json::json!(format!("https://github.com/rust-lang/rust/issues/{}",200000 + n));
                node
            }).collect()
        };
        let page = serde_json::json!({"nodes":nodes,"pageInfo":{"hasNextPage":!continued,"endCursor":if continued {None} else {Some(&cursor)}}});
        if continued {
            serde_json::json!({"data":{"repository":{"intakeIssues":page}}})
        } else {
            serde_json::json!({"data":{"r0":{"intakeIssues":page}}})
        }
    }).await;
    let mut intake = std::collections::HashMap::new();
    poll_one_account(
        &fixture.state,
        &github_com_account(&fixture.state),
        None,
        &[("/repo".into(), "rust-lang".into(), "rust".into())],
        false,
        "disabled",
        false,
        &mut Default::default(),
        &mut Default::default(),
        Some("C-bug"),
        &mut intake,
    )
    .await
    .unwrap();
    assert_eq!(intake["/repo"].len(), 101);
    let last = &intake["/repo"][100];
    assert_eq!(last.number, 11361);
    assert!(last.labels.iter().any(|l| l.name == "C-bug"));
    assert!(fixture.requests.try_recv().is_ok());
    assert!(fixture.requests.try_recv().is_ok());
    assert!(
        fixture.requests.try_recv().is_err(),
        "only the incomplete intake connection needs another fetch"
    );
    fixture.close().await;
}

// Catches publishing a partial eligible set or looping forever on malformed pagination.
#[tokio::test]
async fn intake_rejects_missing_or_repeated_cursors_without_publishing_partial_pages() {
    for missing in [true, false] {
        let fixture = PollHttpFixture::with_response(move |request| {
            let page = serde_json::json!({"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":if missing {None} else {Some("repeated")}}});
            if request["query"].as_str().unwrap().contains("query IntakeIssues") {
                serde_json::json!({"data":{"repository":{"intakeIssues":page}}})
            } else {
                serde_json::json!({"data":{"r0":{"intakeIssues":page}}})
            }
        }).await;
        let mut intake = std::collections::HashMap::new();
        let result = poll_one_account(
            &fixture.state,
            &github_com_account(&fixture.state),
            None,
            &[("/repo".into(), "rust-lang".into(), "rust".into())],
            false,
            "disabled",
            false,
            &mut Default::default(),
            &mut Default::default(),
            Some("C-bug"),
            &mut intake,
        )
        .await;
        assert!(result.unwrap_err().contains(if missing {
            "missing its next cursor"
        } else {
            "did not advance"
        }));
        assert!(
            intake.is_empty(),
            "incomplete pages must not replace retry snapshots"
        );
        fixture.close().await;
    }
}

// Catches: all-open intake polling loses a label beyond ten display labels,
// including on ordinary repositories sharing the same poll batch.
#[tokio::test]
async fn all_open_poll_retains_eleventh_label_in_mixed_repository_intake() {
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/github-issue-119.json")).unwrap();
    let fixture = PollHttpFixture::with_response(move |request| {
        let query = request["query"].as_str().unwrap();
        let labels: Vec<_> = (0..11)
            .map(|n| serde_json::json!({"name": if n == 10 {"tuic-run".to_string()} else {format!("label-{n}")}, "color":"d73a49"}))
            .collect();
        if query.contains("query IntakeIssueLabels") {
            return serde_json::json!({"data":{"repository":{"issue":{"labels":{
                "nodes": [labels[10].clone()], "pageInfo":{"hasNextPage":false,"endCursor":null}
            }}}}});
        }
        let mut response = serde_json::json!({"data":{}});
        for (alias, repository) in [("r0", "triaged"), ("r1", "ordinary")] {
            let mut node = recorded["data"]["repository"]["issue"].clone();
            node["state"] = serde_json::json!("OPEN");
            node["url"] = serde_json::json!(format!("https://github.com/sstraus/{repository}/issues/119"));
            node["labels"] = serde_json::json!({"nodes": &labels[..10],
                "pageInfo":{"hasNextPage":true,"endCursor":"label-ten"}});
            response["data"][alias] = serde_json::json!({"intakeIssues":{
                "nodes":[node], "pageInfo":{"hasNextPage":false,"endCursor":null}
            }});
        }
        response
    }).await;
    let mut intake = std::collections::HashMap::new();
    poll_one_account(
        &fixture.state,
        &github_com_account(&fixture.state),
        None,
        &[
            ("/triaged".into(), "sstraus".into(), "triaged".into()),
            ("/ordinary".into(), "sstraus".into(), "ordinary".into()),
        ],
        false,
        "disabled",
        false,
        &mut Default::default(),
        &mut Default::default(),
        Some(""),
        &mut intake,
    )
    .await
    .unwrap();
    fixture.close().await;
    for path in ["/triaged", "/ordinary"] {
        assert!(
            intake[path][0]
                .labels
                .iter()
                .any(|label| label.name == "tuic-run"),
            "{path}: all-open polling discarded the eleventh intake label"
        );
    }
}
