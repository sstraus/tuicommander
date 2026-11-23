//! Read-only diagnostics snapshot of the gateway (R6): per-caller counters and
//! per-bucket quota state. One function feeds the Tauri command, the HTTP route
//! and the MCP `debug action=gateway`, so the three shapes cannot drift.
//!
//! Every label comes from the `tuic_gateway::metrics` types, which hold static
//! call-site names, account ids, bucket names and route templates; no URL,
//! query value, header or token reaches this report.

use std::time::SystemTime;

use serde_json::{Value, json};
use tuic_gateway::metrics::{BucketReport, Counters, MetricKey, Outcome, RequestRecord};

use super::AppGateway;

/// Snapshot of the current window. With `reset`, the counters and the request
/// ring start a new window afterwards; bucket state belongs to the provider and
/// stays.
pub(crate) fn report(gateway: &AppGateway, reset: bool) -> Value {
    let snapshot = gateway.metrics().snapshot();
    let buckets = gateway.bucket_snapshots();
    if reset {
        gateway.metrics().reset();
    }
    json!({
        "counters": snapshot.counters.iter().map(|(k, c)| counter_json(k, c)).collect::<Vec<_>>(),
        "recent": snapshot.recent.iter().map(recent_json).collect::<Vec<_>>(),
        "buckets": buckets.iter().map(bucket_json).collect::<Vec<_>>(),
        "reset": reset,
    })
}

fn label<T: std::fmt::Debug>(value: T) -> String {
    format!("{value:?}").to_lowercase()
}

fn epoch_ms(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn counter_json(key: &MetricKey, c: &Counters) -> Value {
    json!({
        "caller": key.caller,
        "provider": key.provider.as_str(),
        "account": key.account,
        "bucket": key.bucket,
        "template": key.template,
        "calls": c.calls,
        "sent": c.sent,
        "network2xx": c.network_2xx,
        "networkOther": c.network_other,
        "revalidated": c.revalidated,
        "cacheHits": c.cache_hits,
        "staleServed": c.stale_served,
        "shed": c.shed,
        "rateLimited": c.rate_limited,
        "transportErrors": c.transport_errors,
        "cancelled": c.cancelled,
        "bytesIn": c.bytes_in,
        "units": c.units,
        "p50Ms": c.p50_ms,
        "p95Ms": c.p95_ms,
    })
}

fn recent_json(r: &RequestRecord) -> Value {
    let (outcome, source, status) = match r.outcome {
        Outcome::Served { source, status } => ("served", Some(label(source)), Some(status)),
        Outcome::Shed => ("shed", None, None),
        Outcome::RateLimited => ("rateLimited", None, None),
        Outcome::TransportError => ("transportError", None, None),
        Outcome::Cancelled => ("cancelled", None, None),
    };
    json!({
        "atMs": epoch_ms(r.at),
        "caller": r.caller,
        "template": r.template,
        "channel": label(r.channel),
        "outcome": outcome,
        "source": source,
        "status": status,
        "units": r.units,
        "latencyMs": r.latency_ms,
    })
}

fn bucket_json(b: &BucketReport) -> Value {
    json!({
        "account": b.account,
        "bucket": b.bucket.0,
        "limit": b.limit,
        "remaining": b.remaining,
        "resetMs": b.reset.map(epoch_ms),
        "inFlight": b.in_flight,
        "pressure": label(b.pressure),
        "burnPerSec": b.burn_per_sec,
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn provider_gateway_stats(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    reset: bool,
) -> Value {
    report(&state.gateway.gateway, reset)
}
