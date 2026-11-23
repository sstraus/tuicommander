//! Provider gateway benchmark (R6, story 3151): every scenario sends through
//! the app's gateway to a loopback TLS receiver. No live GitHub, no credentials.
//!
//! The API replies are controlled protocol fixtures, not captured account data.
//! The wire trace is recorded from real reqwest sends received by the receiver.
//! The poller scenarios run the production poller on the wall clock; the other
//! scenarios replay recorded request sequences on a clock the benchmark moves.
//! `fixtures/github_volume/before_master.json` holds the same workloads measured
//! on the tree before the gateway (`scripts/provider-gateway-bench-before.sh`).

use super::*;
use serde_json::{Map, Value, json};
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use tuic_gateway::{ports::Clock, testing::FakeClock};

#[path = "github_volume_bench/etag_probe.rs"]
mod etag_probe;
#[path = "github_volume_bench/limits.rs"]
mod limits;
#[path = "github_volume_bench/receiver.rs"]
mod receiver;
#[path = "github_volume_bench/replay.rs"]
mod replay;

const TOKEN: &str = "fixture-not-a-real-token";
const BEFORE: &str = include_str!("fixtures/github_volume/before_master.json");

/// Reports are kept after the run, so they sit outside the per-run test root.
fn report_dir() -> std::path::PathBuf {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.tmp/provider-gateway-bench");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One app state whose `api.github.com` is the receiver.
struct Bench {
    state: Arc<AppState>,
    receiver: receiver::Receiver,
    /// `None` runs on the wall clock: the poller's timers are real.
    clock: Option<FakeClock>,
    repo_path: String,
}

impl Bench {
    fn new(server: &receiver::Server, repo_path: &str, clock: Option<FakeClock>) -> Self {
        let client = server.client();
        let mut state = crate::state::tests_support::make_test_app_state();
        state.http_client = client.clone();
        state.gateway = match &clock {
            Some(clock) => crate::provider_gateway::ProviderGateway::l1_only_with_clock(
                client,
                Arc::new(clock.clone()),
            ),
            None => crate::provider_gateway::ProviderGateway::l1_only(client),
        };
        *state.github.token.write() = Some(TOKEN.into());
        state.github.disconnected.store(false, Ordering::Relaxed);
        // Controlled workload: one active repository, visible app, one account.
        state.hot_repo_paths.write().insert(repo_path.to_string());
        Self {
            state: Arc::new(state),
            receiver: server.receiver.clone(),
            clock,
            repo_path: repo_path.to_string(),
        }
    }

    fn advance(&self, by: Duration) {
        self.clock
            .as_ref()
            .expect("a replayed scenario")
            .advance(by);
    }

    /// The receiver's `x-ratelimit-reset`, on the clock the gateway reads.
    fn reset_in(&self, seconds: u64) {
        let wall = self
            .clock
            .as_ref()
            .map_or_else(SystemTime::now, |c| c.wall());
        let epoch = wall
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.receiver.set(|mode| mode.reset_epoch = epoch + seconds);
    }

    fn open_window(&self) {
        self.reset_in(900);
        self.receiver.take();
        self.state.gateway.gateway.metrics().reset();
    }

    /// The wire trace and the gateway counters of the window. Catches: a request
    /// that reached the wire without a gateway record, or without a caller id.
    fn close_window(&self) -> Value {
        let wire = self.receiver.take();
        let stats = crate::provider_gateway::stats::report(&self.state.gateway.gateway, true);
        let counters = stats["counters"].as_array().unwrap();
        let sum = |field: &str| {
            counters
                .iter()
                .filter_map(|c| c[field].as_u64())
                .sum::<u64>()
        };
        let count = |keep: &dyn Fn(&Value) -> bool| wire.iter().filter(|row| keep(row)).count();
        let limit_answers = count(&|row| row["status"].as_u64() >= Some(400));
        for counter in counters {
            let caller = counter["caller"].as_str().unwrap_or_default();
            assert!(
                !caller.is_empty(),
                "a request without a caller id: {counter}"
            );
        }
        assert_eq!(
            sum("sent") + limit_answers as u64,
            wire.len() as u64,
            "every request on the wire is one the gateway counted for a caller"
        );
        let calls = sum("calls").max(1) as f64;
        json!({
            "wire": {
                "sends": wire.len(),
                "rest": count(&|row| row["kind"] == "REST"),
                "graphql": count(&|row| row["kind"] == "GraphQL"),
                "conditional": count(&|row| row["conditional"] == true),
                "not_modified": count(&|row| row["status"] == 304),
                "limit_answers": limit_answers,
            },
            "gateway": {
                "calls": sum("calls"), "sent": sum("sent"), "network2xx": sum("network2xx"),
                "revalidated": sum("revalidated"), "cacheHits": sum("cacheHits"),
                "staleServed": sum("staleServed"), "shed": sum("shed"),
                "rateLimited": sum("rateLimited"), "units": sum("units"),
                "cache_ratio": (sum("cacheHits") + sum("staleServed")) as f64 / calls,
                "revalidated_ratio": sum("revalidated") as f64 / calls,
            },
            "per_caller": stats["counters"],
            "buckets": stats["buckets"],
            "trace": wire,
        })
    }
}

/// The production poller on one repository: its first poll, one idle interval,
/// then a PR popover opened and reopened.
async fn poller_scenarios(b: &Bench, out: &mut Map<String, Value>) {
    let mut events = b.state.event_bus.subscribe();
    b.open_window();
    ensure_polling(&b.state, vec![b.repo_path.clone()], "all".into(), false);
    // No setup-speed assertion: the outer experiment bound catches a hang.
    loop {
        if let AppEvent::GitHubPrUpdate { statuses, .. } = events.recv().await.unwrap() {
            assert_eq!(
                statuses.len(),
                1,
                "fixture PR must reach the poller consumer"
            );
            break;
        }
    }
    out.insert("cold_start".into(), b.close_window());
    b.open_window();
    let idle = Instant::now();
    tokio::time::sleep(BASE_INTERVAL + Duration::from_millis(200)).await;
    let mut report = b.close_window();
    report["workload"] = json!({"seconds": idle.elapsed().as_secs_f64(), "repositories": 1});
    out.insert("idle".into(), report);
    b.open_window();
    // The real frontend trace has one get_ci_checks on open, none for ten
    // identical snapshots or a stable mounted panel, then one on reopen.
    for _open in 0..2 {
        let checks = crate::github::get_ci_checks_impl(&b.repo_path, 42, &b.state).await;
        assert_eq!(checks.len(), 1, "fixture check must reach the PR consumer");
    }
    let mut report = b.close_window();
    report["workload"] = json!({"popover_opens": 2});
    out.insert("pr_popover".into(), report);
    stop_poller(&b.state).await.unwrap();
}

fn fixture_repository(scratch: &std::path::Path) -> String {
    let repo_dir = scratch.join("repo");
    std::fs::create_dir(&repo_dir).unwrap();
    let remote = [
        "remote",
        "add",
        "origin",
        "https://github.com/fixture/repository.git",
    ];
    for args in [&["init", "--quiet"][..], &remote[..]] {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo_dir)
            .output()
            .expect("Git CLI is required for this explicitly selected experiment");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    repo_dir.to_string_lossy().into_owned()
}

/// Before and after for the scenarios measured on both trees, without traces.
fn comparison(scenarios: &Map<String, Value>) -> Value {
    let before: Value = serde_json::from_str(BEFORE).unwrap();
    let pairs = [
        ("idle", "idle"),
        ("pr_popover", "pr_popover"),
        ("tasks_list_160", "tasks_list_160"),
        ("trigger_binding", "trigger_binding"),
        ("trigger_binding", "trigger_binding_rest_fallback"),
    ];
    let rows: Map<String, Value> = pairs
        .into_iter()
        .map(|(measured_before, after)| {
            let row = json!({
                "before": before["scenarios"][measured_before]["wire"],
                "before_workload": before["scenarios"][measured_before]["workload"],
                "after": scenarios[after]["wire"],
                "after_workload": scenarios[after]["workload"],
            });
            (after.to_string(), row)
        })
        .collect();
    json!({"before_tree": before["tree"], "scenarios": rows})
}

// Catches: the gateway sending into an exhausted bucket, Background traffic
// sent at Critical, a Tasks list collecting more than once in a TTL window,
// and any request on the wire that no named caller accounts for.
// Explicitly ignored because one real poll interval is a measurement, not a routine gate.
#[tokio::test]
#[ignore = "3151 provider gateway benchmark; requires one real poll interval"]
async fn measure_provider_gateway() {
    let server = receiver::Server::start().await;
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
    let repo_path = fixture_repository(scratch.path());
    let live = Bench::new(&server, &repo_path, None);
    let replayed = Bench::new(&server, &repo_path, Some(FakeClock::new()));
    let experiment = async {
        let mut scenarios = Map::new();
        poller_scenarios(&live, &mut scenarios).await;
        replay::run(&replayed, &mut scenarios).await;
        limits::run(&replayed, &mut scenarios).await;
        scenarios
    };
    let result = tokio::time::timeout(Duration::from_secs(300), experiment).await;
    server.stop().await;
    let scenarios = result.expect("outer experiment hang bound exceeded");
    let comparison = comparison(&scenarios);
    let report = json!({
        "comparison": comparison,
        "scenarios": scenarios,
        "scope": "production Rust poller, GitHub client and gateway against a controlled loopback TLS API; \
                  Tasks, board and PR popover workloads replayed from recorded client request sequences; \
                  plugin admission and the plugin egress client are not exercised; \
                  replayed scenarios run on a moved clock, so their latencies are not measurements",
    });
    let path = report_dir().join("report.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("PROVIDER_GATEWAY_BENCH report {}", path.display());
    println!("PROVIDER_GATEWAY_BENCH comparison {comparison}");
}
