use std::time::{Duration, SystemTime};

use tuic_gateway::budget::BucketConfig;
use tuic_gateway::metrics::{MetricKey, Outcome, Record};
use tuic_gateway::model::{AccountKey, BucketId, Channel, Provider, Source};

use super::ProviderGateway;
use super::stats::report;

fn gateway() -> (tempfile::TempDir, ProviderGateway) {
    let dir = tempfile::tempdir().unwrap();
    let gw = ProviderGateway::new(reqwest::Client::new(), &dir.path().join("cache.sqlite3"));
    (dir, gw)
}

fn seed(gw: &ProviderGateway) {
    let account = AccountKey {
        account_id: "acct-1".into(),
        api_base: "https://api.github.com/".parse().unwrap(),
    };
    gw.gateway
        .budgets()
        .configure(&account, &BucketId("core".into()), BucketConfig::default());
    gw.gateway.metrics().record(
        MetricKey {
            caller: "tasks.list",
            provider: Provider::GitHub.into(),
            account: "acct-1".into(),
            bucket: "core".into(),
            template: "/repos/{o}/{r}/issues",
        },
        Record {
            at: SystemTime::now(),
            channel: Channel::Visible,
            outcome: Outcome::Served {
                source: Source::Cache,
                status: 200,
            },
            bytes_in: 10,
            units: 0,
            latency: Duration::from_millis(3),
        },
    );
}

#[test]
fn stats_report_names_caller_and_bucket() {
    let (_dir, gw) = gateway();
    seed(&gw);
    let v = report(&gw.gateway, false);
    assert_eq!(v["counters"][0]["caller"], "tasks.list");
    assert_eq!(v["counters"][0]["provider"], "github");
    assert_eq!(v["counters"][0]["cacheHits"], 1);
    assert_eq!(v["recent"][0]["source"], "cache");
    assert_eq!(v["buckets"][0]["bucket"], "core");
    assert_eq!(v["buckets"][0]["pressure"], "free");
}

#[test]
fn stats_reset_starts_a_new_window() {
    let (_dir, gw) = gateway();
    seed(&gw);
    let closed = report(&gw.gateway, true);
    assert_eq!(closed["counters"].as_array().unwrap().len(), 1);
    let next = report(&gw.gateway, false);
    assert!(next["counters"].as_array().unwrap().is_empty());
    assert!(next["recent"].as_array().unwrap().is_empty());
    assert_eq!(next["buckets"].as_array().unwrap().len(), 1);
}
