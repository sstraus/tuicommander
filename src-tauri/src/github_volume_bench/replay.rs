//! Scenarios replayed on a clock the benchmark moves: the request sequences of
//! the Tasks list, the board and the workflow trigger source. Every request
//! goes through the app's gateway.

use super::{Bench, TOKEN, receiver};
use http::{HeaderMap, HeaderValue, Method, header::AUTHORIZATION};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, time::Duration};
use tuic_gateway::model::{
    AccountKey, CallerId, Channel, Effect, Freshness, GatewayError, GatewayRequest,
    GatewayResponse, L1_ONLY, Provider,
};

const WINDOW: Duration = Duration::from_secs(60);
const STEP: Duration = Duration::from_secs(5);
const TICKS: usize = 5;
const LIST: &str =
    "repos/fixture/repository/issues?state=all&sort=updated&direction=desc&per_page=100";
const SEARCH: &str = "search/issues?q=repo%3Afixture%2Frepository+is%3Aopen&per_page=100";

/// One request in the shape `plugin_egress::Admitted::call` gives the gateway
/// for a GitHub account: caller `plugin`, Interactive, a GET is a cacheable
/// read kept out of the persistent tier. Plugin admission and the plugin egress
/// client are not part of this measurement; the production egress client cannot
/// be pointed at the loopback receiver.
async fn plugin_request(
    b: &Bench,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<GatewayResponse, GatewayError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {TOKEN}")).unwrap(),
    );
    headers.insert(L1_ONLY, HeaderValue::from_static("1"));
    let effect = if method == Method::GET {
        Effect::Read
    } else {
        Effect::Write
    };
    let api_base = url::Url::parse("https://api.github.com/").unwrap();
    b.state
        .gateway
        .gateway
        .execute(GatewayRequest {
            provider: Provider::GitHub,
            account: AccountKey {
                account_id: "plugin-github".into(),
                api_base: api_base.clone(),
            },
            method,
            url: api_base.join(path).unwrap(),
            headers,
            body: body.map(|body| body.to_string().into()),
            effect,
            channel: Channel::Interactive,
            freshness: Freshness::Default,
            caller: CallerId("plugin"),
            graphql: None,
        })
        .await
}

/// The list asks for its whole collection every 5 s for 60 s: the request rate
/// of a list that reloads without a reason. Catches: more than one collection
/// reaching GitHub inside one TTL window.
async fn collections(b: &Bench, list: &str, ttl: Duration) -> Value {
    let pages = [format!("{list}&page=1"), format!("{list}&page=2")];
    b.open_window();
    let mut per_ttl_window = BTreeMap::<u64, usize>::new();
    let (mut elapsed, mut requests) = (Duration::ZERO, 0);
    while elapsed <= WINDOW {
        let before = b.receiver.sends();
        for page in &pages {
            let response = plugin_request(b, Method::GET, page, None).await;
            assert_eq!(response.expect("list page").status, 200);
            requests += 1;
        }
        *per_ttl_window
            .entry(elapsed.as_secs() / ttl.as_secs())
            .or_default() += b.receiver.sends() - before;
        b.advance(STEP);
        elapsed += STEP;
    }
    for sends in per_ttl_window.values() {
        assert!(
            *sends <= pages.len(),
            "more than one collection reached the wire in one TTL window: {per_ttl_window:?}"
        );
    }
    let mut report = b.close_window();
    report["workload"] = json!({
        "collections_requested": requests / pages.len(),
        "requests": requests,
        "pages_per_collection": pages.len(),
        "items": receiver::ISSUES,
        "step_seconds": STEP.as_secs(),
        "window_seconds": WINDOW.as_secs(),
        "ttl_seconds": ttl.as_secs(),
    });
    report["wire_sends_per_ttl_window"] = json!(per_ttl_window);
    report
}

/// A board open reads the project list, then 160 cards in four pages of 50;
/// the board opens twice in the window. A GraphQL POST is never cached.
async fn board(b: &Bench) -> Value {
    b.open_window();
    for _open in 0..2 {
        for page in 0..5 {
            let query = json!({"query": "query Board($page:Int) { node { id } }", "variables": {"page": page}});
            let response = plugin_request(b, Method::POST, "graphql", Some(query)).await;
            assert_eq!(response.expect("board page").status, 200);
        }
        b.advance(WINDOW / 2);
    }
    let mut report = b.close_window();
    report["workload"] = json!({"board_opens": 2, "graphql_requests_per_open": 5, "window_seconds": WINDOW.as_secs()});
    report
}

/// One trigger-source pass after each of five poll ticks, 60 s apart.
/// Catches: the source downloading open issues and pull requests again when
/// the poll tick already holds them.
async fn trigger(b: &Bench, reuse_polled: bool) -> Value {
    let (issues, pulls) = receiver::open_resources();
    b.open_window();
    for _tick in 0..TICKS {
        let snapshots = crate::workflows::github_poll_bench::source_pass(
            &b.state,
            &b.repo_path,
            &issues,
            &pulls,
            reuse_polled,
        )
        .await
        .expect("trigger source pass");
        assert_eq!(snapshots, receiver::ISSUES + receiver::PULLS);
        b.advance(WINDOW);
    }
    let mut report = b.close_window();
    if reuse_polled {
        assert_eq!(
            report["wire"]["sends"], 0,
            "a source fed by the poll tick must not read the REST pages"
        );
    }
    report["workload"] = json!({
        "poll_ticks": TICKS, "tick_seconds": WINDOW.as_secs(),
        "open_issues": receiver::ISSUES, "open_pull_requests": receiver::PULLS,
        "source": if reuse_polled { "poller list" } else { "REST pages" },
    });
    report
}

pub(super) async fn run(b: &Bench, out: &mut Map<String, Value>) {
    out.insert(
        "tasks_list_160".into(),
        collections(b, LIST, Duration::from_secs(30)).await,
    );
    out.insert(
        "tasks_list_160_filtered".into(),
        collections(b, SEARCH, WINDOW).await,
    );
    out.insert("tasks_board".into(), board(b).await);
    out.insert(
        "trigger_binding_rest_fallback".into(),
        trigger(b, false).await,
    );
    out.insert("trigger_binding".into(), trigger(b, true).await);
}
