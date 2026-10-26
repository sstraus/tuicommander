//! Opt-in joined browser/backend observation. Never runs in routine suites.
//! The receiver forwards only read-only GitHub requests and never retains auth.
use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::StatusCode,
    response::Response,
    routing::{any, get, post},
};
use std::sync::atomic::Ordering;

#[derive(Clone)]
struct LiveReceiver {
    upstream: reqwest::Client,
    token: Arc<String>,
    evidence: std::path::PathBuf,
    started: Instant,
    trace: Arc<parking_lot::Mutex<Vec<serde_json::Value>>>,
    phase: Arc<parking_lot::Mutex<String>>,
}

impl LiveReceiver {
    async fn receive(State(receiver): State<Self>, request: Request) -> Response {
        let method = request.method().clone();
        let path = request.uri().path_and_query().unwrap().as_str().to_string();
        let bytes = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        let query = body["query"].as_str().unwrap_or_default();
        let read_only = method == reqwest::Method::GET
            || (method == reqwest::Method::POST
                && path == "/graphql"
                && query.trim_start().starts_with("query")
                && !query.contains("mutation"));
        if !read_only {
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from(
                    "Live observation permits read-only queries only",
                ))
                .unwrap();
        }
        let elapsed = receiver.started.elapsed().as_secs_f64();
        let phase = receiver.phase.lock().clone();
        // Count arrival before awaiting upstream: a slow response can cross a
        // measurement-window boundary without changing when the request was sent.
        let index = {
            let mut trace = receiver.trace.lock();
            trace.push(serde_json::json!({
                "elapsed_seconds":elapsed, "phase":phase, "method":method.as_str(), "path":path,
                "kind":if path == "/graphql" {"GraphQL"} else {"REST"}, "query":query,
                "status":null,
            }));
            std::fs::write(
                receiver.evidence.join("receiver-trace.json"),
                serde_json::to_vec_pretty(&*trace).unwrap(),
            )
            .unwrap();
            trace.len()
        };
        let result = receiver
            .upstream
            .request(method.clone(), format!("https://api.github.com{path}"))
            .bearer_auth(receiver.token.as_str())
            .header("user-agent", "TUICommander-1995-read-only-observation")
            .header("accept", "application/vnd.github+json")
            .header("content-type", "application/json")
            .body(bytes)
            .send()
            .await;
        let response = match result {
            Ok(response) => response,
            Err(_) => {
                let mut trace = receiver.trace.lock();
                trace[index - 1]["status"] = serde_json::json!(502);
                std::fs::write(
                    receiver.evidence.join("receiver-trace.json"),
                    serde_json::to_vec_pretty(&*trace).unwrap(),
                )
                .unwrap();
                return Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Body::from("Live upstream request failed"))
                    .unwrap();
            }
        };
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.unwrap();
        {
            let mut trace = receiver.trace.lock();
            let row = &mut trace[index - 1];
            row["status"] = serde_json::json!(status.as_u16());
            row["remaining"] = serde_json::json!(
                headers
                    .get("x-ratelimit-remaining")
                    .and_then(|h| h.to_str().ok())
            );
            row["response_seconds"] =
                serde_json::json!(receiver.started.elapsed().as_secs_f64() - elapsed);
            std::fs::write(
                receiver.evidence.join("receiver-trace.json"),
                serde_json::to_vec_pretty(&*trace).unwrap(),
            )
            .unwrap();
        }
        // External-system fixture: captured from live GitHub, never hand-written.
        std::fs::write(
            receiver.evidence.join(format!("response-{index}.json")),
            &body,
        )
        .unwrap();
        let mut reply = Response::builder().status(status);
        for name in [
            "content-type",
            "x-ratelimit-remaining",
            "x-ratelimit-reset",
            "retry-after",
        ] {
            if let Some(value) = headers.get(name) {
                reply = reply.header(name, value);
            }
        }
        reply.body(Body::from(body)).unwrap()
    }
}

// Catches: actual App/HTTP/poller composition sending more requests than its isolated pieces.
// This is a manual opt-in measurement, not an assertion about live GitHub availability.
#[tokio::test]
#[ignore = "1995 joined live-account browser observation; requires process-local GH_TOKEN and browser driver"]
async fn observe_joined_live_github_1995() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap_or_else(|_| assert!(rustls::crypto::CryptoProvider::get_default().is_some()));
    let evidence = std::path::PathBuf::from(
        std::env::var("TUIC_1995_EVIDENCE").expect("explicit Gits evidence directory"),
    );
    assert!(
        evidence.starts_with(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join(".tmp")
        )
    );
    std::fs::create_dir_all(&evidence).unwrap();
    let token = std::env::var("GH_TOKEN").expect("process-local token required");
    assert_ne!(token, "");
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let config = scratch.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let _config = crate::config::set_config_dir_override(config.clone());
    let repo = scratch.path().join("live-repository");
    std::fs::create_dir_all(&repo).unwrap();
    for args in [
        vec!["init", "--quiet", "--initial-branch=main"],
        vec![
            "remote",
            "add",
            "origin",
            "https://github.com/sstraus/tuicommander.git",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let repo_path = repo.to_string_lossy().to_string();
    let repos = serde_json::json!({"repos": {(repo_path.clone()): {
        "path":repo_path, "displayName":"1995 isolated live GitHub", "initials":"GH", "isGitRepo":true,
        "expanded":true, "collapsed":false, "parked":false, "workspaces":{}, "activeWorkspaceId":null
    }}, "repoOrder":[repo_path], "activeRepoPath":repo_path, "groups":{}, "groupOrder":[]});
    std::fs::write(
        config.join("repositories.json"),
        serde_json::to_vec(&repos).unwrap(),
    )
    .unwrap();
    let receiver = LiveReceiver {
        upstream: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap(),
        token: Arc::new(token.clone()),
        evidence: evidence.clone(),
        started: Instant::now(),
        trace: Arc::default(),
        phase: Arc::new(parking_lot::Mutex::new("cold".into())),
    };
    let cert = include_bytes!("fixtures/github_volume/cert.pem");
    let key = include_bytes!("fixtures/github_volume/key.pem");
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert.to_vec(), key.to_vec())
        .await
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let tls_handle = axum_server::Handle::new();
    let receiver_task = tokio::spawn(
        axum_server::from_tcp_rustls(listener, tls)
            .unwrap()
            .handle(tls_handle.clone())
            .serve(
                Router::new()
                    .fallback(any(LiveReceiver::receive))
                    .with_state(receiver.clone())
                    .into_make_service(),
            ),
    );
    let mut state = crate::state::tests_support::make_test_app_state_in(scratch.path());
    state.http_client = reqwest::Client::builder()
        .no_proxy()
        .tls_certs_only([reqwest::Certificate::from_pem(cert).unwrap()])
        .resolve("api.github.com", address)
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    *state.github.token.write() = Some(token);
    *state.github.token_source.write() = crate::github_auth::TokenSource::Env;
    state.github.disconnected.store(false, Ordering::Relaxed);
    let state = Arc::new(state);
    state.wire_event_bus();
    let stopped = Arc::new(Notify::new());
    let stop = stopped.clone();
    let status_receiver = receiver.clone();
    let phase_receiver = receiver.clone();
    let router = crate::mcp_http::build_router(state.clone(), false, false)
        .route("/__1995/status", get(move || {let r=status_receiver.clone(); async move {axum::Json(serde_json::json!({"trace":r.trace.lock().clone(),"elapsed_seconds":r.started.elapsed().as_secs_f64()}))}}))
        .route("/__1995/phase/{phase}", post(move |axum::extract::Path(phase):axum::extract::Path<String>| {let r=phase_receiver.clone(); async move {*r.phase.lock()=phase; StatusCode::OK}}))
        .route("/__1995/stop", post(move || {let stop=stop.clone(); async move {stop.notify_one(); StatusCode::OK}}));
    let api = tokio::net::TcpListener::bind("127.0.0.1:14360")
        .await
        .unwrap();
    let shutdown = Arc::new(Notify::new());
    let server_shutdown = shutdown.clone();
    let api_task = tokio::spawn(async move {
        axum::serve(
            api,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move { server_shutdown.notified().await })
        .await
        .unwrap();
    });
    std::fs::write(evidence.join("ready.json"), serde_json::to_vec_pretty(&serde_json::json!({"backend":"http://127.0.0.1:14360","repo_path":repo_path,"config_dir":config,"source_ref":std::env::var("TUIC_1995_SOURCE_REF").unwrap_or_default()})).unwrap()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(600), stopped.notified()).await;
    if state.github.poller.lock().is_some() {
        stop_poller(&state).await.unwrap();
    }
    shutdown.notify_one();
    // SSE connections belong to the browser driver, which closes before stop.
    api_task.await.unwrap();
    tls_handle.shutdown();
    receiver_task.await.unwrap().unwrap();
    result.expect("joined browser driver did not stop within the observation bound");
}
