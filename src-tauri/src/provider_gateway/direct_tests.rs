use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{Router, routing::get};
use reqwest::Method;
use tuic_gateway::model::Provider;

use super::ProviderGateway;
use super::direct::{self, Direct};
use crate::github::gateway::Origin;

const ASSET: &[u8] = b"0123456789abcdef";

async fn serve_asset(hits: Arc<AtomicUsize>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/asset",
        get(move || {
            hits.fetch_add(1, Ordering::SeqCst);
            async { ASSET.to_vec() }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/asset")
}

// Catches: a release download that is cached (the second call never reaches the
// server) or that the benchmark never sees (no call, no bytes, no unit).
#[tokio::test]
async fn release_download_is_uncached_and_counted() {
    let hits = Arc::new(AtomicUsize::new(0));
    let url = serve_asset(hits.clone()).await;
    let gateway = ProviderGateway::l1_only(reqwest::Client::new()).gateway;
    let call = Direct::new(
        Origin::interactive("test.release"),
        Provider::Generic,
        "release",
    );

    for _ in 0..2 {
        let response = call
            .stream_via(&gateway, direct::builder(Method::GET, &url))
            .await
            .unwrap();
        assert_eq!(direct::declared_length(&response), Some(ASSET.len() as u64));
        assert_eq!(response.bytes().await.unwrap().as_ref(), ASSET);
    }

    assert_eq!(hits.load(Ordering::SeqCst), 2, "a download is never cached");
    let snapshot = gateway.metrics().snapshot();
    let (key, counters) = &snapshot.counters[0];
    assert_eq!(key.caller, "test.release");
    assert_eq!(key.provider.as_str(), "generic");
    assert_eq!(
        (
            counters.calls,
            counters.network_2xx,
            counters.bytes_in,
            counters.units
        ),
        (2, 2, 2 * ASSET.len() as u64, 2)
    );
}

// Catches: a no-redirect request that follows a redirect anyway, so a credential
// header reaches the host the provider redirected to.
#[tokio::test]
async fn a_no_redirect_send_returns_the_redirect_to_the_caller() {
    let hits = Arc::new(AtomicUsize::new(0));
    let target = serve_asset(hits.clone()).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let location = target.clone();
    let app = Router::new().route(
        "/go",
        get(move || {
            let location = location.clone();
            async move {
                (
                    axum::http::StatusCode::FOUND,
                    [(axum::http::header::LOCATION, location)],
                )
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let gateway = ProviderGateway::l1_only(reqwest::Client::new()).gateway;
    let call = Direct::new(Origin::interactive("test.redirect"), Provider::Generic, "x");
    let url = format!("http://{addr}/go");

    let followed = call
        .send_via(&gateway, direct::builder(Method::GET, &url))
        .await
        .unwrap();
    assert_eq!(followed.status().as_u16(), 200, "default policy follows");

    let held = call
        .no_redirect()
        .send_via(&gateway, direct::builder(Method::GET, &url))
        .await
        .unwrap();
    assert_eq!(held.status().as_u16(), 302);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "only the followed send hit");
}

// Catches: a transport error quoting a pre-signed URL into a message the UI and
// the logs show.
#[tokio::test]
async fn a_failure_message_never_quotes_the_request_url() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let gateway = ProviderGateway::l1_only(reqwest::Client::new()).gateway;
    let call = Direct::new(Origin::interactive("test.leak"), Provider::Generic, "x").redact_url();
    let url = format!("http://127.0.0.1:{port}/log?X-Amz-Signature=secret");

    let error = call
        .send_via(&gateway, direct::builder(Method::GET, &url))
        .await
        .unwrap_err();

    assert!(!error.contains("secret"), "{error}");
}

// Catches: the updater and installer redirect limit drifting from five hops
// (the default client follows ten).
#[tokio::test]
async fn limited_redirects_follow_five_hops_and_no_more() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/hop/{n}",
        get(
            |axum::extract::Path(n): axum::extract::Path<u32>| async move {
                if n == 0 {
                    return (
                        axum::http::StatusCode::OK,
                        [(axum::http::header::LOCATION, String::new())],
                    );
                }
                (
                    axum::http::StatusCode::FOUND,
                    [(axum::http::header::LOCATION, format!("/hop/{}", n - 1))],
                )
            },
        ),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let gateway = ProviderGateway::l1_only(reqwest::Client::new()).gateway;
    let call =
        Direct::new(Origin::interactive("test.hops"), Provider::Generic, "x").limited_redirects();

    let five = call
        .send_via(
            &gateway,
            direct::builder(Method::GET, format!("http://{addr}/hop/5")),
        )
        .await
        .unwrap();
    assert_eq!(five.status().as_u16(), 200);

    let six = call
        .send_via(
            &gateway,
            direct::builder(Method::GET, format!("http://{addr}/hop/6")),
        )
        .await;
    assert!(six.is_err(), "a sixth hop must be refused");
}
