//! Owner-configured credentialless plugin HTTP destinations (#2659-b083).
//! Local servers stand in for a LAN development service: loopback is in the
//! same private-host class the manifest rule gates.
use super::account_tests::{isolated_config, load, server, write_manifest};
use super::plugin_http_fetch_impl;
use std::sync::Arc;

/// A manifest `net:http` pattern no test server answers.
pub(super) const UNRELATED: &str = "https://unrelated.example.test/*";

/// Write `plugin_http_destinations` the way an owner config save lands in
/// memory: through the serialized config, so this helper names no Rust field.
pub(super) fn set_owner_destinations(state: &crate::AppState, patterns: &[String]) {
    let mut value = serde_json::to_value(&*state.config.read()).unwrap();
    value["plugin_http_destinations"] = serde_json::json!(patterns);
    *state.config.write() = serde_json::from_value(value).unwrap();
}

async fn fetch(
    state: &Arc<crate::AppState>,
    plugin: &str,
    url: String,
) -> Result<super::HttpResponse, String> {
    plugin_http_fetch_impl(state, url, None, None, None, None, plugin.into()).await
}

// Catches: a trusted plugin cannot call an owner-configured development
// endpoint, initially or after a redirect, without amending its manifest.
#[tokio::test]
async fn owner_credentialless_plugin_destination_reaches_lan() {
    let (dir, _guard) = isolated_config();
    let (target, target_seen) = server("", None).await;
    let target = target.replace("127.0.0.1", "localhost");
    let (service, seen) = server("", Some(format!("{target}/landed"))).await;
    let (other, other_seen) = server("", None).await;
    // The manifest declares an unrelated service: only the owner configuration
    // can admit the development service.
    write_manifest(
        dir.path(),
        "owner-dest",
        serde_json::json!([]),
        &[UNRELATED.into()],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "owner-dest");
    set_owner_destinations(&state, &[format!("{service}/*"), format!("{target}/*")]);

    let response = fetch(&state, "owner-dest", format!("{service}/redirect"))
        .await
        .unwrap();
    assert_eq!((response.status, response.body.as_str()), (200, "ok"));
    assert_eq!(*seen.lock().unwrap(), vec![("/redirect".to_string(), None)]);
    assert_eq!(
        *target_seen.lock().unwrap(),
        vec![("/landed".to_string(), None)]
    );

    let refused = fetch(&state, "owner-dest", format!("{other}/x"))
        .await
        .unwrap_err();
    assert!(refused.contains("Localhost"), "unexpected error: {refused}");
    assert!(
        other_seen.lock().unwrap().is_empty(),
        "an unconfigured host was contacted"
    );
}

// Catches: plugin requests leave without a User-Agent, which GitHub REST answers
// with 403 "Request forbidden by administrative rules" (live run 2026-11-11).
#[tokio::test]
async fn plugin_request_carries_a_user_agent() {
    let (dir, _guard) = isolated_config();
    let seen: Arc<std::sync::Mutex<Vec<Option<String>>>> = Arc::default();
    let record = seen.clone();
    let app = axum::Router::new().fallback(move |headers: axum::http::HeaderMap| {
        let record = record.clone();
        async move {
            let agent = headers
                .get("user-agent")
                .map(|v| v.to_str().unwrap().to_string());
            record.lock().unwrap().push(agent);
            "ok"
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    write_manifest(
        dir.path(),
        "ua-test",
        serde_json::json!([]),
        &[UNRELATED.into()],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "ua-test");
    set_owner_destinations(&state, &[format!("{service}/*")]);

    let response = fetch(&state, "ua-test", format!("{service}/x"))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0]
            .as_deref()
            .is_some_and(|agent| agent.starts_with("tuicommander-plugin/")),
        "request User-Agent: {:?}",
        seen[0]
    );
}

// Catches: an owner destination admitting a non-HTTP scheme, or a pattern that
// matches only the raw text of a URL the parser normalises.
#[test]
fn owner_destination_keeps_http_schemes_only() {
    let owner = [
        "http://192.168.1.20:3000/*".to_string(),
        "file://*".into(),
        "ftp://*".into(),
    ];
    for url in ["file:///etc/passwd", "ftp://192.168.1.20/x"] {
        let err = super::validate_destination(url, &[], &owner).unwrap_err();
        assert!(err.contains("not allowed"), "{url}: {err}");
    }
    assert!(super::validate_destination("http://192.168.1.20:3000", &[], &owner).is_ok());
    assert!(super::validate_destination("http://192.168.1.21:3000/x", &[], &owner).is_err());
}
