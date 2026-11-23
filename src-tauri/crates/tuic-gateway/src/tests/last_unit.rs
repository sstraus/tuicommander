//! A 2xx that spends the last unit of a bucket is a success: the caller gets
//! the body and the next call waits for the reset.

use bytes::Bytes;
use http::{HeaderValue, Method};
use url::Url;

use super::gateway::{get, request, setup, wire};
use crate::model::*;

const RATE_LIMITED_BODY: &str =
    r#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}]}"#;

// Catches: a valid body dropped as RateLimited because remaining hit 0.
#[tokio::test]
async fn last_unit_success_returns_body_and_holds_next_call() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(wire(200, Some((5000, 0, 600)), None, "the pull")));
    let r = gw.execute(get("/repos/o/r/pulls/1")).await.unwrap();
    assert_eq!(r.body, Bytes::from_static(b"the pull"));

    t.push_reply(Ok(wire(200, Some((5000, 4999, 600)), None, "never sent")));
    let e = gw.execute(get("/repos/o/r/pulls/2")).await.unwrap_err();
    assert!(matches!(e, GatewayError::RateLimited { .. }), "{e:?}");
    assert_eq!(t.requests().len(), 1, "the held call reached the provider");
}

// Catches: a duplicate write after a false error for a write that happened.
#[tokio::test]
async fn last_unit_write_success_is_not_reported_as_limited() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(wire(201, Some((5000, 0, 600)), None, "created")));
    let req = request(Method::POST, "/repos/o/r/issues/1/comments", "test");
    let r = gw.execute(req).await.unwrap();
    assert_eq!((r.status, r.body), (201, Bytes::from_static(b"created")));
}

// Catches: GraphQL exhaustion (a 200 whose body is the limit answer) read as success.
#[tokio::test]
async fn graphql_200_rate_limited_body_is_still_limited() {
    let (gw, _clock, t) = setup();
    t.push_reply(Ok(wire(200, Some((5000, 0, 600)), None, RATE_LIMITED_BODY)));
    let e = gw
        .execute(request(Method::POST, "/graphql", "test"))
        .await
        .unwrap_err();
    assert!(matches!(e, GatewayError::RateLimited { .. }), "{e:?}");
}

// Catches: the same body drop on GitLab, whose headers carry no `x-` prefix.
#[tokio::test]
async fn gitlab_last_unit_success_returns_body() {
    let (gw, _clock, t) = setup();
    let mut reply = wire(200, None, None, "the mr");
    reply
        .headers
        .insert("ratelimit-remaining", HeaderValue::from_static("0"));
    reply
        .headers
        .insert("ratelimit-limit", HeaderValue::from_static("2000"));
    t.push_reply(Ok(reply));
    let mut req = get("/api/v4/projects/1/merge_requests/2");
    req.provider = Provider::GitLab;
    req.account.api_base = Url::parse("https://gitlab.com").unwrap();
    req.url = Url::parse("https://gitlab.com/api/v4/projects/1/merge_requests/2").unwrap();
    let r = gw.execute(req).await.unwrap();
    assert_eq!(r.body, Bytes::from_static(b"the mr"));
}
