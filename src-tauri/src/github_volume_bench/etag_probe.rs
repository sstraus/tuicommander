//! Opt-in live read-only probe: does GitHub send an ETag for `search/issues`
//! and `POST /graphql`, and does it answer `If-None-Match` with a 304? Never
//! runs in routine suites. At most six requests; the token is process-local and
//! only headers on the allow-list below are recorded.

use serde_json::{Value, json};

const KEPT: [&str; 9] = [
    "etag",
    "last-modified",
    "cache-control",
    "vary",
    "x-ratelimit-limit",
    "x-ratelimit-remaining",
    "x-ratelimit-used",
    "x-ratelimit-resource",
    "x-github-media-type",
];

async fn exchange(
    client: &reqwest::Client,
    token: &str,
    method: &reqwest::Method,
    url: &str,
    body: Option<&Value>,
    validator: Option<&str>,
) -> Value {
    let mut request = client
        .request(method.clone(), url)
        .bearer_auth(token)
        .header("user-agent", "TUICommander-3151-read-only-probe")
        .header("accept", "application/vnd.github+json");
    if let Some(body) = body {
        request = request.json(body);
    }
    if let Some(validator) = validator {
        request = request.header("if-none-match", validator);
    }
    let response = request.send().await.expect("live GitHub request");
    let headers: serde_json::Map<String, Value> = KEPT
        .iter()
        .filter_map(|name| {
            let value = response.headers().get(*name)?.to_str().ok()?;
            Some(((*name).to_string(), json!(value)))
        })
        .collect();
    json!({
        "status": response.status().as_u16(),
        "if_none_match_sent": validator.is_some(),
        "headers": headers,
    })
}

// Catches: a cache design that relies on free 304 answers from an endpoint
// that sends no validator or ignores it.
#[tokio::test]
#[ignore = "3151 live read-only GitHub probe; needs a process-local GH_TOKEN"]
async fn probe_github_etag_live_3151() {
    let token = std::env::var("GH_TOKEN").expect("process-local GH_TOKEN from `gh auth token`");
    let repository =
        std::env::var("TUIC_PROBE_REPO").unwrap_or_else(|_| "sstraus/tuicommander".into());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let api = "https://api.github.com";
    let search = format!(
        "{api}/search/issues?q=repo%3A{}+is%3Aissue&per_page=1",
        repository.replace('/', "%2F")
    );
    let targets = [
        ("search_issues", reqwest::Method::GET, search, None),
        (
            "graphql",
            reqwest::Method::POST,
            format!("{api}/graphql"),
            Some(json!({"query": "query { viewer { login } }"})),
        ),
        // Control: the core list is documented to support conditional requests.
        (
            "core_issues_control",
            reqwest::Method::GET,
            format!("{api}/repos/{repository}/issues?state=all&per_page=1"),
            None,
        ),
    ];
    let mut results = serde_json::Map::new();
    for (name, method, url, body) in targets {
        let first = exchange(&client, &token, &method, &url, body.as_ref(), None).await;
        let etag = first["headers"]["etag"].as_str().map(str::to_owned);
        let second = match &etag {
            Some(etag) => exchange(&client, &token, &method, &url, body.as_ref(), Some(etag)).await,
            None => Value::Null,
        };
        let verdict = json!({
            "sends_etag": etag.is_some(),
            "honours_if_none_match": second["status"] == 304,
        });
        results.insert(
            name.into(),
            json!({"method": method.as_str(), "url": url, "first": first, "second": second, "verdict": verdict}),
        );
    }
    let report = json!({"repository": repository, "results": results});
    let text = serde_json::to_string_pretty(&report).unwrap();
    assert!(
        !text.contains(&token),
        "the token must not reach the report"
    );
    let path = super::report_dir().join("etag-probe.json");
    std::fs::write(&path, &text).unwrap();
    println!("PROVIDER_GATEWAY_ETAG_PROBE {} {text}", path.display());
}
