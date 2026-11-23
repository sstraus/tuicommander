use futures::StreamExt;

use super::gateway::{get, setup, wire};
use crate::metrics::{Counters, MetricKey};
use crate::model::GatewayError;

fn counters(gw: &crate::gateway::Gateway<crate::testing::FakeTransport>) -> Counters {
    let snap = gw.metrics().snapshot();
    let key: &MetricKey = &snap.counters[0].0;
    assert_eq!(key.caller, "test");
    snap.counters[0].1.clone()
}

async fn drain(
    body: futures::stream::BoxStream<'static, Result<bytes::Bytes, crate::model::TransportError>>,
) -> usize {
    body.map(|c| c.unwrap().len())
        .fold(0, |a, n| async move { a + n })
        .await
}

// Catches: a streamed download that is cached (the second call never reaches
// the wire) or never counted (the benchmark misses the heaviest traffic).
#[tokio::test]
async fn a_stream_is_never_cached_and_counts_its_bytes() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(wire(
        200,
        Some((5000, 4000, 7200)),
        Some("\"v1\""),
        "binary",
    )));
    t.push_reply(Ok(wire(
        200,
        Some((5000, 3999, 7200)),
        Some("\"v1\""),
        "binary",
    )));

    for _ in 0..2 {
        let resp = gw
            .execute_stream(get("/repos/o/r/releases/assets/1"))
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(drain(resp.body).await, 6);
    }

    assert_eq!(t.requests().len(), 2, "no cache for a stream");
    let c = counters(&gw);
    assert_eq!(
        (c.calls, c.sent, c.network_2xx, c.bytes_in, c.units),
        (2, 2, 2, 12, 2)
    );
}

// Catches: a stream that bypasses the budget, so a limit learned from its
// headers never blocks the next call.
#[tokio::test]
async fn a_stream_feeds_the_budget_and_honours_a_closed_bucket() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(wire(429, Some((5000, 0, 7200)), None, "")));

    let first = gw.execute_stream(get("/repos/o/r/releases/assets/1")).await;
    assert!(matches!(first, Err(GatewayError::RateLimited { .. })));
    let second = gw.execute_stream(get("/repos/o/r/releases/assets/1")).await;
    assert!(second.is_err());
    assert_eq!(
        t.requests().len(),
        1,
        "the closed bucket stops the second send"
    );
    let c = counters(&gw);
    assert_eq!((c.calls, c.rate_limited), (2, 1 + 1));
}
