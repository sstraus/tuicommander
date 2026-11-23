//! Header fixtures follow the provider documentation; tokens and emails are
//! never part of a rate-limit header, so nothing needed scrubbing.

use std::time::{Duration, UNIX_EPOCH};

use http::{HeaderMap, HeaderName, HeaderValue};

use crate::headers::observe;
use crate::model::{LimitReason, Provider};

type Pairs = &'static [(&'static str, &'static str)];
// (name, provider, status, headers, body, expected verdict)
type Fixture = (
    &'static str,
    Provider,
    u16,
    Pairs,
    &'static [u8],
    Option<LimitReason>,
);

fn hm(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.insert(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    h
}

const GH_CORE: Pairs = &[
    ("x-ratelimit-limit", "5000"),
    ("x-ratelimit-remaining", "4999"),
    ("x-ratelimit-used", "1"),
    ("x-ratelimit-reset", "1700000000"),
    ("x-ratelimit-resource", "core"),
];
const GH_EMPTY: Pairs = &[
    ("x-ratelimit-limit", "5000"),
    ("x-ratelimit-remaining", "0"),
    ("x-ratelimit-reset", "1700000000"),
    ("x-ratelimit-resource", "core"),
];
const GH_GQL_EMPTY: Pairs = &[
    ("x-ratelimit-limit", "5000"),
    ("x-ratelimit-remaining", "0"),
    ("x-ratelimit-reset", "1700000000"),
    ("x-ratelimit-resource", "graphql"),
];
const GL_OK: Pairs = &[
    ("ratelimit-name", "throttle_authenticated_api"),
    ("ratelimit-limit", "2000"),
    ("ratelimit-observed", "10"),
    ("ratelimit-remaining", "1990"),
    ("ratelimit-reset", "1700000000"),
];
const SECONDARY_BODY: &[u8] = br#"{"message":"You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}"#;
const PERMISSION_BODY: &[u8] = br#"{"message":"Resource not accessible by integration"}"#;

#[test]
fn headers_limited_verdict_per_fixture() {
    use LimitReason::*;
    use Provider::*;
    let cases: Vec<Fixture> = vec![
        ("gh 200 core", GitHub, 200, GH_CORE, b"{}", None),
        (
            "gh 403 primary",
            GitHub,
            403,
            GH_EMPTY,
            b"{}",
            Some(Primary),
        ),
        // Catches: a 403 permission error read as a rate limit.
        (
            "gh 403 permission",
            GitHub,
            403,
            GH_CORE,
            PERMISSION_BODY,
            None,
        ),
        (
            "gh 403 secondary",
            GitHub,
            403,
            GH_CORE,
            SECONDARY_BODY,
            Some(Secondary),
        ),
        (
            "gh 403 retry-after",
            GitHub,
            403,
            &[("retry-after", "60")],
            b"",
            Some(Secondary),
        ),
        (
            "gh 429 headroom",
            GitHub,
            429,
            GH_CORE,
            b"",
            Some(Secondary),
        ),
        ("gh 429 empty", GitHub, 429, GH_EMPTY, b"", Some(Primary)),
        // Catches: GraphQL 200 exhaustion read as success.
        (
            "gh graphql 200 remaining 0",
            GitHub,
            200,
            GH_GQL_EMPTY,
            b"{}",
            Some(Primary),
        ),
        (
            "gh graphql 200 secondary",
            GitHub,
            200,
            GH_CORE,
            SECONDARY_BODY,
            Some(Secondary),
        ),
        ("gh 500", GitHub, 500, GH_CORE, b"", None),
        ("gl 200", GitLab, 200, GL_OK, b"", None),
        (
            "gl 429 no headers",
            GitLab,
            429,
            &[],
            b"Retry later",
            Some(Unknown),
        ),
        (
            "gl 429 remaining 0",
            GitLab,
            429,
            &[("ratelimit-remaining", "0")],
            b"",
            Some(Primary),
        ),
        (
            "jira 200",
            Jira,
            200,
            &[
                ("x-ratelimit-limit", "100"),
                ("x-ratelimit-remaining", "80"),
            ],
            b"",
            None,
        ),
        (
            "jira burst",
            Jira,
            429,
            &[("ratelimit-reason", "jira-burst-based")],
            b"",
            Some(Burst),
        ),
        (
            "jira quota global",
            Jira,
            429,
            &[("ratelimit-reason", "jira-quota-global-based")],
            b"",
            Some(Quota),
        ),
        (
            "jira quota tenant",
            Jira,
            429,
            &[("ratelimit-reason", "jira-quota-tenant-based")],
            b"",
            Some(Quota),
        ),
        // Catches: Jira per-issue throttling applied as a global pause.
        (
            "jira per-issue",
            Jira,
            429,
            &[("ratelimit-reason", "jira-per-issue-on-write")],
            b"",
            Some(PerIssue),
        ),
        ("jira 429 no reason", Jira, 429, &[], b"", Some(Unknown)),
        ("jira 503", Jira, 503, &[("retry-after", "5")], b"", None),
        ("generic 429", Generic, 429, &[], b"", Some(Unknown)),
        ("generic 200", Generic, 200, &[], b"", None),
    ];
    for (name, provider, status, headers, body, want) in cases {
        let got = observe(provider, status, &hm(headers), body);
        assert_eq!(got.limited, want, "{name}");
    }
}

#[test]
fn headers_github_numbers_bucket_and_reset() {
    let o = observe(Provider::GitHub, 200, &hm(GH_CORE), b"{}");
    assert_eq!(o.bucket.as_ref().unwrap().0, "core");
    assert_eq!((o.limit, o.remaining), (Some(5000), Some(4999)));
    assert_eq!(
        o.reset,
        Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    let g = observe(Provider::GitHub, 200, &hm(GH_GQL_EMPTY), b"{}");
    assert_eq!(g.bucket.as_ref().unwrap().0, "graphql");
    assert_eq!(g.remaining, Some(0));
}

#[test]
fn headers_retry_after_is_parsed_as_seconds() {
    let o = observe(Provider::GitHub, 429, &hm(&[("retry-after", "42")]), b"");
    assert_eq!(o.retry_after, Some(Duration::from_secs(42)));
    let j = observe(Provider::Jira, 429, &hm(&[("retry-after", "7")]), b"");
    assert_eq!(j.retry_after, Some(Duration::from_secs(7)));
    let date = hm(&[("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")]);
    assert_eq!(observe(Provider::GitHub, 429, &date, b"").retry_after, None);
}

#[test]
fn headers_gitlab_fields() {
    let o = observe(Provider::GitLab, 200, &hm(GL_OK), b"");
    assert_eq!(o.bucket.unwrap().0, "throttle_authenticated_api");
    assert_eq!((o.limit, o.remaining), (Some(2000), Some(1990)));
    assert_eq!(
        o.reset,
        Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
}

#[test]
fn headers_jira_iso_reset_near_limit_and_dc_fill_rate() {
    let o = observe(
        Provider::Jira,
        429,
        &hm(&[
            ("x-ratelimit-limit", "100"),
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "2023-11-14T22:13:20Z"),
            ("x-ratelimit-nearlimit", "true"),
            ("ratelimit-reason", "jira-burst-based"),
            ("retry-after", "3"),
        ]),
        b"",
    );
    assert_eq!(
        o.reset,
        Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    assert!(o.near_limit);
    assert_eq!(o.retry_after, Some(Duration::from_secs(3)));
    let off = hm(&[("x-ratelimit-reset", "2023-11-15T00:13:20.250+02:00")]);
    assert_eq!(
        observe(Provider::Jira, 429, &off, b"").reset,
        Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    let dc = hm(&[
        ("x-ratelimit-limit", "20"),
        ("x-ratelimit-remaining", "19"),
        ("x-ratelimit-interval-seconds", "5"),
        ("x-ratelimit-fillrate", "10"),
    ]);
    let dc = observe(Provider::Jira, 200, &dc, b"");
    assert_eq!(dc.fill_rate, Some((10, Duration::from_secs(5))));
    assert!(!dc.near_limit);
}

#[test]
fn headers_garbage_values_are_ignored() {
    let bad = hm(&[
        ("x-ratelimit-remaining", "lots"),
        ("x-ratelimit-reset", "-1"),
    ]);
    let o = observe(Provider::GitHub, 200, &bad, b"");
    assert_eq!((o.remaining, o.reset, o.limited), (None, None, None));
}
