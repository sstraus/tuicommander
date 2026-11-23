//! The GitHub client against the provider gateway: one budget per (account,
//! bucket) fed by every send, whatever the path.

use super::*;
use crate::github::Origin;
use crate::github_account::{GitHubAccount, GitHubHost};
use tuic_gateway::model::Pressure;

/// Close the GraphQL bucket of `account` with a refused call to a loopback
/// receiver. The gateway keys the bucket by account and route, not by host.
pub(super) async fn trip_graphql_limit(
    state: &AppState,
    account: &crate::github_account::GitHubAccount,
) {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/graphql")
        .with_status(429)
        .with_header("retry-after", "60")
        .create_async()
        .await;
    let refused = graphql_request(
        state,
        account,
        Origin::interactive("test"),
        "token",
        &format!("{}/graphql", server.url()),
        "query { viewer { login } }",
        &serde_json::json!({}),
    )
    .await;
    assert!(matches!(refused, Err(GqlError::RateLimit { .. })));
}

fn bucket(state: &AppState, name: &str) -> Option<tuic_gateway::metrics::BucketReport> {
    state
        .gateway
        .gateway
        .bucket_snapshots()
        .into_iter()
        .find(|b| b.bucket.0 == name)
}

/// Catches: a REST limit that the next call ignores because the gateway never
/// saw it (today's breaker guarantee, now owned by the gateway).
#[tokio::test]
async fn rest_403_rate_limit_blocks_next_core_call_without_send() {
    let mut server = mockito::Server::new_async().await;
    let limited = server
        .mock("GET", "/repos/o/r/issues/1")
        .with_status(403)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", "99999999999")
        .expect(1)
        .create_async()
        .await;
    let next = server
        .mock("GET", "/repos/o/r/issues/2")
        .expect(0)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);

    let first = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state
            .http_client
            .get(format!("{}/repos/o/r/issues/1", server.url())),
    )
    .await
    .expect_err("an exhausted primary limit is a rate limit");
    assert!(first.starts_with("rate-limit:"), "{first}");
    let second = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state
            .http_client
            .get(format!("{}/repos/o/r/issues/2", server.url())),
    )
    .await
    .expect_err("the core bucket is exhausted");
    assert!(second.starts_with("rate-limit:"), "{second}");

    limited.assert_async().await;
    next.assert_async().await;
    let core = bucket(&state, "core").expect("the gateway must have seen the core bucket");
    assert_eq!(core.pressure, Pressure::Exhausted);
}

/// Catches: a GraphQL 200 that spent the last point still lets the next query
/// out, because only an error body (not the headers) used to trip the breaker.
#[tokio::test]
async fn graphql_200_exhaustion_blocks_graphql_bucket() {
    let mut server = mockito::Server::new_async().await;
    let spent = server
        .mock("POST", "/graphql")
        .with_status(200)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", "99999999999")
        .with_body(r#"{"data":{"viewer":{"login":"o"}}}"#)
        .expect(1)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let url = format!("{}/graphql", server.url());
    let variables = serde_json::json!({});
    let ask = || {
        graphql_request(
            &state,
            &account,
            Origin::interactive("test"),
            "token",
            &url,
            "query { viewer { login } }",
            &variables,
        )
    };

    // The answer that spent the last point is data; the next call is held.
    assert!(ask().await.is_ok());
    assert!(matches!(ask().await, Err(GqlError::RateLimit { .. })));

    spent.assert_async().await;
    let graphql = bucket(&state, "graphql").expect("the gateway must have seen the graphql bucket");
    assert_eq!(graphql.pressure, Pressure::Exhausted);
    assert!(
        bucket(&state, "core").is_none_or(|b| b.pressure != Pressure::Exhausted),
        "a graphql limit must not close the core bucket"
    );
}

/// Catches: the 401 fallback (`github.rs:722-751`) sending with other
/// candidate tokens while the gateway has the bucket closed. A 401 that also
/// says the bucket is empty ends the call: the candidate is refused unsent.
#[tokio::test]
#[serial_test::serial]
async fn the_401_token_fallback_respects_the_gateway() {
    let mut server = mockito::Server::new_async().await;
    let refused = server
        .mock("POST", "/graphql")
        .with_status(401)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", "99999999999")
        .with_body(r#"{"message":"Bad credentials"}"#)
        .expect(1)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    *state.github.token.write() = Some("stale".into());
    let account = github_com_account(&state);
    // SAFETY: serialised with every other test that touches the environment.
    unsafe { std::env::set_var("GH_TOKEN", "candidate-token") };

    let result = graphql_at(
        &state,
        &account,
        Origin::interactive("test"),
        &format!("{}/graphql", server.url()),
        "query { viewer { login } }",
        serde_json::json!({}),
        None,
    )
    .await;
    unsafe { std::env::remove_var("GH_TOKEN") };
    let err = result.expect_err("an empty bucket ends the call");

    assert!(err.starts_with("rate-limit:"), "{err}");
    refused.assert_async().await;
}

/// Catches: `min_rate_budget` blind to the core bucket (`github.rs:272`): a
/// REST-heavy account drained its core quota while the poller paced for the
/// last GraphQL reading.
#[tokio::test]
async fn poller_budget_reads_gateway_buckets_including_core() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/o/r/issues/1")
        .with_status(200)
        .with_header("x-ratelimit-limit", "5000")
        .with_header("x-ratelimit-remaining", "7")
        .with_header("x-ratelimit-reset", "99999999999")
        .with_body("{}")
        .create_async()
        .await;
    server
        .mock("POST", "/graphql")
        .with_status(200)
        .with_header("x-ratelimit-limit", "5000")
        .with_header("x-ratelimit-remaining", "4000")
        .with_header("x-ratelimit-reset", "99999999999")
        .with_body(r#"{"data":{}}"#)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    assert_eq!(min_rate_budget(&state), u32::MAX, "nothing is known yet");

    graphql_request(
        &state,
        &account,
        Origin::interactive("test"),
        "token",
        &format!("{}/graphql", server.url()),
        "query { x }",
        &serde_json::json!({}),
    )
    .await
    .unwrap();
    assert_eq!(min_rate_budget(&state), 4000);
    send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state
            .http_client
            .get(format!("{}/repos/o/r/issues/1", server.url())),
    )
    .await
    .unwrap();

    assert_eq!(min_rate_budget(&state), 7);
}

// --- the breaker tests of #491-6bb2, now owned by the gateway ---

/// The defect: only graphql_with_retry and legacy gh writes consulted a breaker,
/// so a rate-limited account kept hammering GitHub through every direct-REST
/// call. A limit the gateway learned must stop the next request before it is
/// sent — asserted by the mock being hit exactly once.
#[tokio::test]
async fn a_limited_bucket_stops_a_rest_call_before_it_is_sent() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/repos/o/r/issues/1")
        .with_status(429)
        .with_header("retry-after", "42")
        .expect(1)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let url = format!("{}/repos/o/r/issues/1", server.url());

    send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(&url),
    )
    .await
    .expect_err("429 must not be handed back as an ordinary response");
    let err = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(&url),
    )
    .await
    .expect_err("a closed bucket must refuse the call");

    assert!(err.starts_with("rate-limit:"), "unexpected error: {err}");
    mock.assert_async().await;
}

#[tokio::test]
async fn a_rest_429_leaves_the_account_backing_off() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("PATCH", "/repos/o/r/issues/1")
        .with_status(429)
        .with_header("retry-after", "42")
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let url = format!("{}/repos/o/r/issues/1", server.url());

    let err = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.patch(&url),
    )
    .await
    .expect_err("429 must not be handed back as an ordinary response");
    assert!(err.contains("rate-limit"), "unexpected error: {err}");
    mock.assert_async().await;

    let status = gateway::backoff_status(&state).expect("the 429 must leave a back-off");
    assert!(status.starts_with("rate-limit:"), "{status}");
}

/// GitHub's 403 is ambiguous. An exhausted primary limit (remaining: 0) must
/// back off; a plain permission denial must not, or one protected branch would
/// silence every GitHub call for the account.
#[tokio::test]
async fn a_403_is_a_rate_limit_only_when_the_headers_say_so() {
    let mut server = mockito::Server::new_async().await;
    let exhausted = server
        .mock("GET", "/exhausted")
        .with_status(403)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", "99999999999")
        .create_async()
        .await;
    let forbidden = server
        .mock("GET", "/forbidden")
        .with_status(403)
        .with_body(r#"{"message":"Resource not accessible"}"#)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);

    let err = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(format!("{}/exhausted", server.url())),
    )
    .await
    .expect_err("an exhausted primary limit is a rate limit");
    assert!(err.contains("rate-limit"), "unexpected error: {err}");
    exhausted.assert_async().await;
    gateway::reset_account(&state, &account).await;

    // The permission 403 comes back as a Response so the caller can read the
    // body and produce its own wording (friendly_approve_error, merge errors…).
    let response = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(format!("{}/forbidden", server.url())),
    )
    .await
    .expect("a permission 403 must reach the caller with its body intact");
    assert_eq!(response.status().as_u16(), 403);
    assert!(response.text().await.unwrap().contains("not accessible"));
    forbidden.assert_async().await;
    assert!(
        gateway::backoff_status(&state).is_none(),
        "a single permission denial must not close a bucket"
    );
}

#[tokio::test]
async fn ordinary_rest_4xx_responses_do_not_poison_account_availability() {
    let mut server = mockito::Server::new_async().await;
    for (path, status) in [("/missing", 404), ("/conflict", 409), ("/invalid", 422)] {
        server
            .mock("GET", path)
            .with_status(status)
            .with_body(r#"{"message":"deterministic caller error"}"#)
            .create_async()
            .await;
    }
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);

    for path in ["missing", "conflict", "invalid"] {
        let response = send_rest_with_breaker(
            &state,
            &account,
            Origin::interactive("test"),
            state.http_client.get(format!("{}/{path}", server.url())),
        )
        .await
        .expect("ordinary 4xx must reach its caller");
        assert!(response.status().is_client_error());
    }

    assert!(gateway::backoff_status(&state).is_none());
    // A fourth call still goes out: nothing closed.
    let again = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(format!("{}/missing", server.url())),
    )
    .await;
    assert!(again.is_ok());
}

/// The gateway opens a host after three consecutive transport failures (a 5xx
/// is a caller-readable response, not one). The fourth call is refused unsent.
#[tokio::test]
async fn three_transport_failures_close_the_host_and_a_5xx_does_not() {
    let mut server = mockito::Server::new_async().await;
    let unavailable = server
        .mock("GET", "/unavailable")
        .with_status(503)
        .expect(3)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    for _ in 0..3 {
        let response = send_rest_with_breaker(
            &state,
            &account,
            Origin::interactive("test"),
            state
                .http_client
                .get(format!("{}/unavailable", server.url())),
        )
        .await
        .expect("5xx body remains caller-readable");
        assert_eq!(response.status().as_u16(), 503);
    }
    unavailable.assert_async().await;

    let closed = closed_port_url();
    for _ in 0..3 {
        let err = send_rest_with_breaker(
            &state,
            &account,
            Origin::interactive("test"),
            state.http_client.get(&closed),
        )
        .await
        .expect_err("nothing listens there");
        assert!(!err.contains("backing off"), "{err}");
    }
    let err = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(&closed),
    )
    .await
    .expect_err("the host is closed");
    assert!(err.contains("backing off"), "{err}");
}

#[tokio::test]
async fn a_successful_rest_call_clears_earlier_transport_failures() {
    let mut server = mockito::Server::new_async().await;
    let mut ok = Vec::new();
    for path in ["/ok1", "/ok2"] {
        ok.push(
            server
                .mock("GET", path)
                .with_status(200)
                .with_body("{}")
                .expect(1)
                .create_async()
                .await,
        );
    }
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let closed = closed_port_url();
    let (ok1, ok2) = (
        format!("{}/ok1", server.url()),
        format!("{}/ok2", server.url()),
    );
    let call = |url: &str| {
        send_rest_with_breaker(
            &state,
            &account,
            Origin::interactive("test"),
            state.http_client.get(url),
        )
    };

    for _ in 0..2 {
        call(&closed).await.unwrap_err();
    }
    call(&ok1).await.expect("2xx");
    for _ in 0..2 {
        let err = call(&closed).await.unwrap_err();
        assert!(!err.contains("backing off"), "{err}");
    }
    // Four failures in total, but never three in a row.
    call(&ok2).await.expect("the host is still open");
    for mock in ok {
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn a_secondary_limit_body_trips_rest_backoff_without_headers() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/secondary")
        .with_status(403)
        .with_body(r#"{"message":"You have exceeded a secondary rate limit."}"#)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);

    let error = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state.http_client.get(format!("{}/secondary", server.url())),
    )
    .await
    .expect_err("body-signalled secondary limit must back off");

    mock.assert_async().await;
    assert!(error.contains("rate-limit"), "{error}");
    assert!(gateway::backoff_status(&state).is_some());
}

// --- per-account isolation, now one budget per (account, bucket) ---

#[tokio::test]
async fn one_accounts_limit_never_closes_another_accounts_bucket() {
    let state = crate::state::tests_support::make_test_app_state();
    let (ambient, ghe, named) = (
        github_com_account(&state),
        GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None),
        GitHubAccount::github_com_named("octocat-named"),
    );
    trip_graphql_limit(&state, &ghe).await;
    trip_graphql_limit(&state, &named).await;

    assert!(gateway::graphql_closed(&state, &ghe));
    assert!(gateway::graphql_closed(&state, &named));
    assert!(
        !gateway::graphql_closed(&state, &ambient),
        "one account's limit must not throttle the others"
    );
}

#[tokio::test]
async fn min_rate_budget_paces_for_the_tightest_account() {
    let mut server = mockito::Server::new_async().await;
    for (path, remaining) in [
        ("/repos/o/r/issues/1", "5000"),
        ("/repos/o/r/issues/2", "7"),
    ] {
        server
            .mock("GET", path)
            .with_status(200)
            .with_header("x-ratelimit-limit", "5000")
            .with_header("x-ratelimit-remaining", remaining)
            .with_header("x-ratelimit-reset", "99999999999")
            .with_body("{}")
            .create_async()
            .await;
    }
    let state = crate::state::tests_support::make_test_app_state();
    let (ambient, named) = (
        github_com_account(&state),
        GitHubAccount::github_com_named("octocat-named"),
    );
    for (account, n) in [(&ambient, 1), (&named, 2)] {
        let url = format!("{}/repos/o/r/issues/{n}", server.url());
        send_rest_with_breaker(
            &state,
            account,
            Origin::interactive("test"),
            state.http_client.get(url),
        )
        .await
        .unwrap();
    }

    assert_eq!(
        min_rate_budget(&state),
        7,
        "the global poll cadence must pace for the most-constrained account"
    );
}

/// A loopback URL whose port refuses connections.
fn closed_port_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/closed")
}

async fn rest(
    state: &AppState,
    account: &GitHubAccount,
    method: reqwest::Method,
    url: String,
) -> tuic_gateway::model::GatewayResponse {
    gateway::execute(
        state,
        account,
        Origin::interactive("test"),
        method,
        url.parse().unwrap(),
        reqwest::header::HeaderMap::new(),
        None,
    )
    .await
    .expect("the mock answers")
}

fn cached_status_paths(state: &AppState, paths: &[&str]) -> Vec<bool> {
    paths
        .iter()
        .map(|p| state.git_cache.github_status.get(*p).is_some())
        .collect()
}

/// Catches: stale state after a write. `github_status` and the issue list of
/// the repository stayed on screen after the gateway closed an issue.
#[tokio::test]
async fn close_issue_invalidates_github_status_and_issue_list() {
    let mut server = mockito::Server::new_async().await;
    let list = server
        .mock("GET", "/repos/o/r/issues")
        .with_status(200)
        .with_body("[]")
        .expect(2)
        .create_async()
        .await;
    server
        .mock("PATCH", "/repos/o/r/issues/1")
        .with_status(200)
        .with_body("{}")
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    gateway::cache_status(&state, &account, "o", "r", "/repo", vec![]);
    gateway::cache_status(&state, &account, "o", "other", "/other", vec![]);
    let issues = format!("{}/repos/o/r/issues", server.url());

    rest(&state, &account, reqwest::Method::GET, issues.clone()).await;
    rest(
        &state,
        &account,
        reqwest::Method::PATCH,
        format!("{issues}/1"),
    )
    .await;
    let after = rest(&state, &account, reqwest::Method::GET, issues).await;

    assert_eq!(after.source, tuic_gateway::model::Source::Network);
    list.assert_async().await;
    assert_eq!(
        cached_status_paths(&state, &["/repo", "/other"]),
        [false, true],
        "only the written repository loses its parsed status"
    );
}

/// Catches: a refused write dropping a status that is still true.
#[tokio::test]
async fn a_failed_write_keeps_the_cached_status() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("PATCH", "/repos/o/r/issues/1")
        .with_status(422)
        .with_body("{}")
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    gateway::cache_status(&state, &account, "o", "r", "/repo", vec![]);

    rest(
        &state,
        &account,
        reqwest::Method::PATCH,
        format!("{}/repos/o/r/issues/1", server.url()),
    )
    .await;

    assert_eq!(cached_status_paths(&state, &["/repo"]), [true]);
}

/// Catches: a limit that reaches the UI as prose only, so the banner cannot
/// show the reset time and the caller cannot tell a limit from a failure.
#[tokio::test]
async fn rate_limit_error_carries_a_typed_payload_after_the_prefix() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/o/r/issues/1")
        .with_status(403)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", "99999999999")
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);

    let error = send_rest_with_breaker(
        &state,
        &account,
        Origin::interactive("test"),
        state
            .http_client
            .get(format!("{}/repos/o/r/issues/1", server.url())),
    )
    .await
    .expect_err("an exhausted primary limit is a rate limit");

    let payload: serde_json::Value =
        serde_json::from_str(error.strip_prefix("rate-limit:").expect("prefix")).expect("json");
    assert_eq!(payload["kind"], "rate_limited");
    assert_eq!(payload["bucket"], "core");
    assert!(payload["retry_at"].as_u64().is_some(), "{payload}");
    assert!(payload["message"].as_str().unwrap().contains("GitHub"));
}
