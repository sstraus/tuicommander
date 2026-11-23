//! Story 3151, tree before the provider gateway: the workloads of
//! `measure_provider_gateway`, sent by the production poller, GitHub client,
//! plugin HTTP path and trigger source of that tree to the same receiver.
//! Copied into the measured tree by `scripts/provider-gateway-bench-before.sh`;
//! never compiled in this tree. No live GitHub, no credentials.

use super::*;
use serde_json::{Map, Value, json};
use std::sync::atomic::Ordering;

#[allow(dead_code)] // The quota and limit switches belong to the gateway benchmark.
#[path = "github_volume_bench/receiver.rs"]
mod receiver;

const TOKEN: &str = "fixture-not-a-real-token";
const PLUGIN: &str = "bench-github-work";
const PROVIDER: &str = "bench-provider";
const COLLECTIONS: usize = 13;
const TICKS: usize = 5;

/// The wire trace since the last call, in the shape of the gateway benchmark.
fn window(receiver: &receiver::Receiver, workload: Value) -> Value {
    let wire = receiver.take();
    let count = |keep: &dyn Fn(&Value) -> bool| wire.iter().filter(|row| keep(row)).count();
    json!({
        "wire": {
            "sends": wire.len(),
            "rest": count(&|row| row["kind"] == "REST"),
            "graphql": count(&|row| row["kind"] == "GraphQL"),
            "conditional": count(&|row| row["conditional"] == true),
            "not_modified": count(&|row| row["status"] == 304),
            "limit_answers": count(&|row| row["status"].as_u64() >= Some(400)),
        },
        "workload": workload,
        "trace": wire,
    })
}

fn fixture_repository(scratch: &std::path::Path) -> String {
    let repo_dir = scratch.join("repo");
    std::fs::create_dir(&repo_dir).unwrap();
    let remote = ["remote", "add", "origin", "https://github.com/fixture/repository.git"];
    for args in [&["init", "--quiet"][..], &remote[..]] {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo_dir)
            .output()
            .expect("Git CLI is required for this explicitly selected experiment");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
    repo_dir.to_string_lossy().into_owned()
}

/// A plugin with one bearer account on a plain-HTTP receiver: this tree's
/// plugin path builds its own client, so it cannot be pointed at the TLS one.
async fn plugin_account(config: &std::path::Path, state: &AppState, base: &str) -> String {
    let plugin_dir = config.join("plugins").join(PLUGIN);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let manifest = json!({
        "id": PLUGIN, "name": PLUGIN, "version": "1.0.0", "minAppVersion": "0.0.0",
        "main": "main.js", "capabilities": ["net:http"], "allowedUrls": [format!("{base}/*")],
        "providers": [{
            "id": PROVIDER, "label": "Benchmark provider", "kinds": ["bearer"],
            "verify": {"path": "/identity", "loginPointer": "/name"}
        }],
    });
    std::fs::write(plugin_dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    crate::plugins::register_loaded_plugin_impl(state, PLUGIN.into(), vec!["net:http".into()]).unwrap();
    let input = crate::accounts::NewAccount {
        provider: PROVIDER.into(),
        host: base.into(),
        kind: crate::accounts::AuthKind::Bearer,
        label: "Benchmark account".into(),
        username: None,
    };
    crate::accounts::add_account(input, TOKEN.into()).await.unwrap().account.id
}

#[tokio::test]
#[ignore = "3151 before-gateway volume measurement; requires one real poll interval"]
async fn measure_before_gateway_3151() {
    let server = receiver::Server::start().await;
    let receiver = server.receiver.clone();
    let plain = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let plain_base = format!("http://{}", plain.local_addr().unwrap());
    let plain_router = receiver.router();
    tokio::spawn(async move { axum::serve(plain, plain_router).await.unwrap() });
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let config = scratch.path().join("config");
    let _config = crate::config::set_config_dir_override(config.clone());
    let repo_path = fixture_repository(scratch.path());
    let mut state = crate::state::tests_support::make_test_app_state();
    state.http_client = server.client();
    *state.github.token.write() = Some(TOKEN.into());
    state.github.disconnected.store(false, Ordering::Relaxed);
    state.hot_repo_paths.write().insert(repo_path.clone());
    let state = Arc::new(state);
    let mut events = state.event_bus.subscribe();
    let experiment = async {
        let mut out = Map::new();
        receiver.take();
        ensure_polling(&state, vec![repo_path.clone()], "all".into(), false);
        loop {
            if let AppEvent::GitHubPrUpdate { statuses, .. } = events.recv().await.unwrap() {
                assert_eq!(statuses.len(), 1, "fixture PR must reach the poller consumer");
                break;
            }
        }
        out.insert("cold_start".into(), window(&receiver, json!({})));
        let idle = Instant::now();
        tokio::time::sleep(BASE_INTERVAL + Duration::from_millis(200)).await;
        let workload = json!({"seconds": idle.elapsed().as_secs_f64(), "repositories": 1});
        out.insert("idle".into(), window(&receiver, workload));
        for _open in 0..2 {
            let checks = crate::github::get_ci_checks_impl(&repo_path, 42, &state).await;
            assert_eq!(checks.len(), 1, "fixture check must reach the PR consumer");
        }
        out.insert("pr_popover".into(), window(&receiver, json!({"popover_opens": 2})));
        stop_poller(&state).await.unwrap();

        // Tasks list: this tree lists every scope through search/issues and
        // holds no response cache, so the sends do not depend on the 5 s step.
        let account = plugin_account(&config, &state, &plain_base).await;
        receiver.take();
        let list = "search/issues?q=repo%3Afixture%2Frepository&per_page=100";
        for _collection in 0..COLLECTIONS {
            for page in 1..=2 {
                let response = crate::plugin_http::plugin_http_fetch_impl(
                    &state,
                    format!("{plain_base}/{list}&page={page}"),
                    None,
                    None,
                    None,
                    Some(account.clone()),
                    PLUGIN.into(),
                )
                .await
                .expect("list page");
                assert_eq!(response.status, 200);
            }
        }
        let workload = json!({
            "collections_requested": COLLECTIONS, "requests": COLLECTIONS * 2,
            "pages_per_collection": 2, "items": receiver::ISSUES,
            "step_seconds": 5, "window_seconds": 60, "ttl_seconds": null,
        });
        out.insert("tasks_list_160".into(), window(&receiver, workload));

        let (issues, pulls) = receiver::open_resources();
        for _tick in 0..TICKS {
            let snapshots = crate::workflows::github_poll_bench::source_pass(
                &state, &repo_path, &issues, &pulls,
            )
            .await
            .expect("trigger source pass");
            assert_eq!(snapshots, receiver::ISSUES + receiver::PULLS);
        }
        let workload = json!({
            "poll_ticks": TICKS, "tick_seconds": 60,
            "open_issues": receiver::ISSUES, "open_pull_requests": receiver::PULLS,
            "source": "REST pages",
        });
        out.insert("trigger_binding".into(), window(&receiver, workload));
        out
    };
    let result = tokio::time::timeout(Duration::from_secs(300), experiment).await;
    server.stop().await;
    let scenarios = result.expect("outer experiment hang bound exceeded");
    let report = json!({
        "tree": std::env::var("TUIC_BENCH_TREE").ok(),
        "scenarios": scenarios,
        "scope": "production Rust poller, GitHub client, plugin HTTP path and trigger source of the tree \
                  before the provider gateway, against the controlled loopback API of the gateway benchmark",
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    if let Ok(path) = std::env::var("TUIC_BENCH_OUT") {
        std::fs::write(path, &text).unwrap();
    }
    println!("PROVIDER_GATEWAY_BEFORE {text}");
}
