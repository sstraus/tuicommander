//! Quota levels and limit answers, replayed on a clock the benchmark moves:
//! what each channel may still send as the core bucket empties, and that
//! nothing is sent into a bucket GitHub closed.

use super::{Bench, TOKEN};
use crate::github::Origin;
use http::Method;
use serde_json::{Map, Value, json};
use std::time::Duration;

/// Longer than any hold the failure memory keeps (15 min cap, jitter 1.3).
const PAST_ANY_HOLD: Duration = Duration::from_secs(30 * 60);

/// One core REST read of an issue through the production GitHub client, with
/// what the gateway did with it.
async fn probe(b: &Bench, origin: Origin, number: usize) -> Value {
    let before = b.receiver.sends();
    let account = crate::github::github_com_account(&b.state);
    let request = crate::github::github_rest_request(
        &b.state.http_client,
        Method::GET,
        &account.host,
        &format!("/repos/fixture/repository/issues/{number}"),
        TOKEN,
        crate::github::GITHUB_JSON,
    );
    let result = crate::github::send_rest_with_breaker(&b.state, &account, origin, request).await;
    let stats = crate::provider_gateway::stats::report(&b.state.gateway.gateway, false);
    let last = stats["recent"].as_array().unwrap().last().unwrap().clone();
    json!({
        "caller": origin.caller.0,
        "wire_sends": b.receiver.sends() - before,
        "outcome": last["outcome"],
        "source": last["source"],
        "refused": result.is_err(),
    })
}

fn channels(scenario: &'static str) -> [Origin; 3] {
    match scenario {
        "storm" => [
            Origin::background("bench.storm.background"),
            Origin::visible("bench.storm.visible"),
            Origin::interactive("bench.storm.interactive"),
        ],
        _ => [
            Origin::background("bench.exhausted.background"),
            Origin::visible("bench.exhausted.visible"),
            Origin::interactive("bench.exhausted.interactive"),
        ],
    }
}

/// Quota at 30 %, 10 % and 3 % of the core bucket, from the receiver headers.
/// Catches: Background traffic still sent at Critical, and a free or watched
/// bucket that refuses traffic.
async fn quota(b: &Bench, out: &mut Map<String, Value>) {
    let levels = [
        ("quota_30", 1500, 1000),
        ("quota_10", 500, 2000),
        ("quota_3", 150, 3000),
    ];
    // Entries that are stale, with a validator, when the levels run.
    b.open_window();
    for (_, _, base) in levels {
        probe(b, Origin::visible("bench.quota.warm"), base + 4).await;
        probe(b, Origin::background("bench.quota.warm"), base + 5).await;
    }
    b.close_window();
    b.advance(Duration::from_secs(61));
    for (name, remaining, base) in levels {
        b.open_window();
        b.receiver.set(|mode| mode.quota = Some((5000, remaining)));
        // The quota is header truth: one answer moves the bucket to the level.
        probe(b, Origin::interactive("bench.quota.prime"), base).await;
        let interactive_miss =
            probe(b, Origin::interactive("bench.quota.interactive"), base + 1).await;
        let visible_miss = probe(b, Origin::visible("bench.quota.visible"), base + 2).await;
        let background_miss =
            probe(b, Origin::background("bench.quota.background"), base + 3).await;
        let visible_stale = probe(b, Origin::visible("bench.quota.visible"), base + 4).await;
        let background_stale =
            probe(b, Origin::background("bench.quota.background"), base + 5).await;
        let mut report = b.close_window();
        let pressure = report["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|bucket| bucket["bucket"] == "core" && bucket["remaining"] == remaining)
            .map(|bucket| bucket["pressure"].clone())
            .expect("core bucket at the level");
        assert_eq!(
            interactive_miss["wire_sends"], 1,
            "{name}: Interactive is never shed"
        );
        match name {
            "quota_30" => {
                assert_eq!(pressure, "watch");
                assert_eq!(background_miss["wire_sends"], 1, "Watch must not shed");
            }
            "quota_10" => assert_eq!(pressure, "tight"),
            _ => {
                assert_eq!(pressure, "critical");
                assert_eq!(
                    background_miss["outcome"], "shed",
                    "Background is shed at L3"
                );
                assert_eq!(background_miss["wire_sends"], 0, "Background is shed at L3");
                assert_eq!(
                    background_stale["wire_sends"], 0,
                    "Background is shed at L3"
                );
            }
        }
        report["pressure"] = pressure;
        report["remaining_of_5000"] = json!(remaining);
        report["probes"] = json!({
            "interactive_miss": interactive_miss, "visible_miss": visible_miss,
            "background_miss": background_miss, "visible_stale": visible_stale,
            "background_stale": background_stale,
        });
        out.insert(name.into(), report);
    }
    // Back to a free bucket for the scenarios that follow.
    b.open_window();
    b.receiver.set(|mode| mode.quota = None);
    probe(b, Origin::interactive("bench.quota.prime"), 4000).await;
    b.close_window();
}

/// Every request is answered 429 with `retry-after: 60`. Catches: a send after
/// the first refusal of a bucket, and a bucket that never opens again.
async fn storm(b: &Bench) -> Value {
    b.open_window();
    b.receiver.set(|mode| mode.storm = Some((429, 60)));
    let account = crate::github::github_com_account(&b.state);
    let mut refused = 0;
    for n in 0..50 {
        let result = probe(b, channels("storm")[n % 3], 5000 + n).await;
        refused += usize::from(result["refused"] == true);
    }
    for _ in 0..10 {
        // GraphQL is its own bucket: its first refusal closes it too.
        let result = crate::github::graphql_with_retry(
            &b.state,
            &account,
            Origin::background("bench.storm.graphql"),
            "query { viewer { login } }",
            Value::Null,
            None,
        )
        .await;
        refused += usize::from(result.unwrap_err().starts_with("rate-limit:"));
    }
    let during = b.receiver.sends();
    assert_eq!(refused, 60, "every request of the storm is refused");
    assert_eq!(during, 2, "one refused send for each bucket, then none");
    b.receiver.set(|mode| mode.storm = None);
    let held = probe(b, Origin::interactive("bench.storm.interactive"), 5998).await;
    assert_eq!(held["wire_sends"], 0, "the hold lasts until retry-after");
    b.advance(PAST_ANY_HOLD);
    b.reset_in(900);
    let resumed = probe(b, Origin::interactive("bench.storm.interactive"), 5999).await;
    assert_eq!(
        (&resumed["wire_sends"], &resumed["refused"]),
        (&json!(1), &json!(false))
    );
    let mut report = b.close_window();
    report["workload"] = json!({"requests_during_storm": 60, "answer": "429, retry-after 60"});
    report["sends_during_storm"] = json!(during);
    report["sends_while_held_after_storm"] = held["wire_sends"].clone();
    report
}

/// The answer that spends the last unit (`x-ratelimit-remaining: 0`).
/// Catches: any send into an exhausted bucket before its reset.
async fn exhaustion(b: &Bench) -> Value {
    b.open_window();
    b.receiver.set(|mode| mode.quota = Some((5000, 0)));
    let last_unit = probe(b, Origin::interactive("bench.exhausted.interactive"), 6000).await;
    assert_eq!(
        (&last_unit["wire_sends"], &last_unit["refused"]),
        (&json!(1), &json!(false))
    );
    let before = b.receiver.sends();
    for n in 0..50 {
        probe(b, channels("exhausted")[n % 3], 6001 + n).await;
    }
    let during = b.receiver.sends() - before;
    assert_eq!(during, 0, "zero sends during exhaustion");
    b.receiver.set(|mode| mode.quota = None);
    b.advance(PAST_ANY_HOLD);
    b.reset_in(900);
    let resumed = probe(b, Origin::interactive("bench.exhausted.interactive"), 6999).await;
    assert_eq!(
        (&resumed["wire_sends"], &resumed["refused"]),
        (&json!(1), &json!(false))
    );
    let mut report = b.close_window();
    report["workload"] =
        json!({"requests_during_exhaustion": 50, "answer": "200 with x-ratelimit-remaining 0"});
    report["sends_during_exhaustion"] = json!(during);
    report
}

pub(super) async fn run(b: &Bench, out: &mut Map<String, Value>) {
    quota(b, out).await;
    out.insert("storm_429".into(), storm(b).await);
    out.insert("exhaustion".into(), exhaustion(b).await);
}
