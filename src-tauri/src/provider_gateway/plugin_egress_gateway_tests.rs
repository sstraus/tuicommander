//! Plugin requests through the gateway: budgets, cache and the plugin's limits.
use super::plugin_egress::{FetchOptions, MAX_RESPONSE_BYTES, MAX_TIMEOUT};
use crate::accounts::AuthKind;
use crate::plugin_http::account_tests::{
    account, descriptor, isolated_config, load, write_manifest,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const PLUGIN: &str = "gateway-plugin";
const PROVIDER: &str = "gateway-provider";

/// Answers `/identity` for account verification; `/slow` answers after 3 s and
/// `/big` with 100 bytes, `/limited` with a 429 and `/loop` redirects to itself. Every other path answers 8 bytes at once. All but
/// `/identity` are counted.
async fn counting_server() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = axum::Router::new().fallback(
        move |headers: axum::http::HeaderMap, uri: axum::http::Uri| {
            let counter = counter.clone();
            async move {
                use axum::response::IntoResponse;
                if uri.path().ends_with("/identity") {
                    return axum::Json(serde_json::json!({"name":"gw-user"})).into_response();
                }
                counter.fetch_add(1, Ordering::SeqCst);
                match uri.path() {
                    "/limited" => {
                        return (
                            axum::http::StatusCode::TOO_MANY_REQUESTS,
                            [("retry-after", "120")],
                            "slow down",
                        )
                            .into_response();
                    }
                    "/loop" => {
                        return (axum::http::StatusCode::FOUND, [("location", "/loop")], "")
                            .into_response();
                    }
                    "/slow" => {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        "x".repeat(8)
                    }
                    "/big" => "x".repeat(100),
                    "/hdr" => format!(
                        "freshness:{}",
                        headers
                            .get("x-tuic-freshness")
                            .map_or("", |v| v.to_str().unwrap_or(""))
                    ),
                    "/huge" => "x".repeat(MAX_RESPONSE_BYTES + 1024),
                    "/echo" => format!(
                        "seen: {}",
                        headers
                            .get("authorization")
                            .map_or("", |v| v.to_str().unwrap_or(""))
                    ),
                    _ => "x".repeat(8),
                }
                .into_response()
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, hits)
}

struct Setup {
    state: Arc<crate::AppState>,
    base: String,
    hits: Arc<AtomicUsize>,
    account: String,
    dir: tempfile::TempDir,
    _guard: Box<dyn std::any::Any>,
}

async fn setup() -> Setup {
    let (dir, guard) = isolated_config();
    let (base, hits) = counting_server().await;
    write_manifest(
        dir.path(),
        PLUGIN,
        descriptor(PROVIDER),
        &[format!("{base}/*")],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, PLUGIN);
    let account = account(PROVIDER, &base, AuthKind::Bearer, "gateway-token").await;
    Setup {
        state,
        base,
        hits,
        account,
        dir,
        _guard: Box::new(guard),
    }
}

async fn get(
    setup: &Setup,
    path: &str,
    options: FetchOptions,
) -> Result<crate::plugin_http::HttpResponse, String> {
    crate::plugin_http::plugin_http_fetch_with_options_impl(
        &setup.state,
        format!("{}{path}", setup.base),
        None,
        None::<HashMap<String, String>>,
        None,
        Some(setup.account.clone()),
        PLUGIN.into(),
        options,
    )
    .await
}

// Catches: a cache lookup that runs before admission, serving a cached body to a
// plugin whose consent was withdrawn; and a plugin request that skips the budget.
#[tokio::test]
async fn plugin_request_is_budgeted_and_cached_after_admission() {
    let s = setup().await;

    let first = get(&s, "/data", FetchOptions::default()).await.unwrap();
    assert_eq!(first.status, 200);
    assert!(!first.headers.contains_key("x-tuic-cache"), "{first:?}");
    let second = get(&s, "/data", FetchOptions::default()).await.unwrap();
    assert_eq!(
        second.headers.get("x-tuic-cache").map(String::as_str),
        Some("hit")
    );
    assert_eq!(
        s.hits.load(Ordering::SeqCst),
        1,
        "the second read must come from the cache"
    );
    let buckets = s.state.gateway.gateway.bucket_snapshots();
    assert!(
        buckets.iter().any(|b| b.account == s.account),
        "the request never reached the budget book: {buckets:?}"
    );

    // Consent withdrawn: the manifest no longer declares the account's provider.
    write_manifest(
        s.dir.path(),
        PLUGIN,
        descriptor("another-provider"),
        &[format!("{}/*", s.base)],
    );
    let denied = get(&s, "/data", FetchOptions::default()).await.unwrap_err();
    assert!(
        denied.contains("does not declare account provider"),
        "{denied}"
    );
    assert_eq!(s.hits.load(Ordering::SeqCst), 1);
}

// Catches: a plugin timeout or size option that is dropped, or one that raises a
// host maximum.
#[tokio::test]
async fn plugin_timeout_and_size_options_are_honoured_but_capped() {
    let s = setup().await;
    let started = std::time::Instant::now();
    let err = get(
        &s,
        "/slow",
        FetchOptions {
            timeout_ms: Some(150),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(err.contains("timed out"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );

    let err = get(
        &s,
        "/big",
        FetchOptions {
            max_response_bytes: Some(10),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(err.contains("exceeds maximum size"), "{err}");

    let raised = FetchOptions {
        timeout_ms: Some(10 * 60 * 1000),
        max_response_bytes: Some(1 << 40),
    };
    assert_eq!(raised.timeout(), MAX_TIMEOUT);
    assert_eq!(raised.max_bytes(), MAX_RESPONSE_BYTES);
    let lowered = FetchOptions {
        timeout_ms: Some(250),
        max_response_bytes: Some(512),
    };
    assert_eq!(lowered.timeout(), Duration::from_millis(250));
    assert_eq!(lowered.max_bytes(), 512);
}

// Catches: a provider limit that reaches the plugin as the provider's own answer
// (no typed marker, no wait), and a redirect loop that is followed without end.
#[tokio::test]
async fn plugin_sees_a_typed_429_and_a_bounded_redirect_chain() {
    let s = setup().await;

    let before = s.hits.load(Ordering::SeqCst);
    let err = get(&s, "/loop", FetchOptions::default()).await.unwrap_err();
    assert!(err.contains("too many redirects"), "{err}");
    assert_eq!(
        s.hits.load(Ordering::SeqCst) - before,
        6,
        "initial request plus five hops"
    );

    // The limit holds the account's bucket, so it comes last.
    let limited = get(&s, "/limited", FetchOptions::default()).await.unwrap();
    assert_eq!(limited.status, 429);
    assert_eq!(
        limited.headers.get("x-tuic-gateway").map(String::as_str),
        Some("rate_limited")
    );
    let wait: u64 = limited.headers["retry-after"].parse().unwrap();
    assert!((1..=120).contains(&wait), "retry-after {wait}");
    assert!(limited.body.is_empty(), "{limited:?}");
}

// Catches: an echoed token persisted on disk in the provider cache file.
#[tokio::test]
async fn plugin_account_body_never_reaches_l2() {
    let s = setup().await;
    let reply = get(&s, "/echo", FetchOptions::default()).await.unwrap();
    assert!(reply.body.contains("[redacted]"), "{reply:?}");
    let again = get(&s, "/echo", FetchOptions::default()).await.unwrap();
    assert_eq!(
        again.headers.get("x-tuic-cache").map(String::as_str),
        Some("hit")
    );

    s.state
        .gateway
        .l2
        .as_ref()
        .expect("test state has an L2")
        .flush()
        .await;
    let db =
        rusqlite::Connection::open(s.state.data_dir.join(crate::db::PROVIDER_CACHE_DB)).unwrap();
    let rows: i64 = db
        .query_row("SELECT COUNT(*) FROM l2_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0, "a plugin account body reached the persistent tier");
}

// Catches: a body over the plugin's own limit counted as a host failure, so
// three of them close the host to the plugin's next request.
#[tokio::test]
async fn oversized_plugin_body_does_not_back_off_the_host() {
    let s = setup().await;
    for _ in 0..4 {
        let err = get(&s, "/huge", FetchOptions::default()).await.unwrap_err();
        assert!(err.contains("exceeds maximum size"), "{err}");
    }
    let ok = get(&s, "/data", FetchOptions::default()).await.unwrap();
    assert_eq!(ok.status, 200);
}

// Catches: a refresh that is answered from the cache, a freshness header that
// reaches the provider, or a plain read that bypasses the cache.
#[tokio::test]
async fn freshness_force_header_bypasses_the_cache_and_never_leaves() {
    let setup = setup().await;
    let force = |path: &str| {
        let headers = HashMap::from([("x-tuic-freshness".to_string(), "force".to_string())]);
        crate::plugin_http::plugin_http_fetch_with_options_impl(
            &setup.state,
            format!("{}{path}", setup.base),
            None,
            Some(headers),
            None,
            Some(setup.account.clone()),
            PLUGIN.into(),
            FetchOptions::default(),
        )
    };
    get(&setup, "/page", FetchOptions::default()).await.unwrap();
    get(&setup, "/page", FetchOptions::default()).await.unwrap();
    assert_eq!(
        setup.hits.load(Ordering::SeqCst),
        1,
        "plain reads share the cache"
    );
    force("/page").await.unwrap();
    assert_eq!(
        setup.hits.load(Ordering::SeqCst),
        2,
        "force goes to the network once"
    );
    let seen = force("/hdr").await.unwrap();
    assert_eq!(
        seen.body, "freshness:",
        "the header is stripped before egress"
    );
}
