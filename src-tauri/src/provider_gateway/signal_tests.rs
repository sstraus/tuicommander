use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method};
use tokio::sync::broadcast;
use tuic_gateway::cache::Cache;
use tuic_gateway::failure::SeededJitter;
use tuic_gateway::gateway::Gateway;
use tuic_gateway::model::*;
use tuic_gateway::testing::{FakeClock, FakeTransport};
use url::Url;

use super::signals::{RepoRef, SignalAdapter, on_account_cleared, run};
use crate::accounts::ClearedAccount;
use crate::github::{BranchPrStatus, parse_pr_node};
use crate::github_poller::PrTransition;
use crate::state::AppEvent;

const BASE: &str = "https://api.github.com";

fn account(id: &str) -> AccountKey {
    AccountKey {
        account_id: id.into(),
        api_base: Url::parse(BASE).unwrap(),
    }
}

fn adapter() -> SignalAdapter {
    SignalAdapter::new(Arc::new(|path| {
        let repo = match path {
            "/repo" => "r",
            "/other" => "r2",
            _ => return None,
        };
        Some(RepoRef {
            account: account("a1"),
            owner: "o".into(),
            repo: repo.into(),
        })
    }))
}

struct Harness {
    gateway: Arc<Gateway<FakeTransport>>,
    transport: FakeTransport,
}

impl Harness {
    fn new() -> Self {
        let clock = Arc::new(FakeClock::new());
        let transport = FakeTransport::new();
        let gateway = Gateway::new(
            clock.clone(),
            Arc::new(transport.clone()),
            Cache::new(clock, 1 << 20),
            Arc::new(SeededJitter::new(1)),
        );
        Self {
            gateway: Arc::new(gateway),
            transport,
        }
    }

    /// One GET; the fake answers 200 whenever the gateway goes to the wire.
    async fn get(&self, acct: &str, path: &str) -> Source {
        self.transport.push_reply(Ok(WireResponse {
            status: 200,
            headers: HeaderMap::new(),
            body: Bytes::from_static(b"[]"),
        }));
        self.gateway
            .execute(GatewayRequest {
                provider: Provider::GitHub,
                account: account(acct),
                method: Method::GET,
                url: Url::parse(&format!("{BASE}{path}")).unwrap(),
                headers: HeaderMap::new(),
                body: None,
                effect: Effect::Read,
                channel: Channel::Visible,
                freshness: Freshness::Default,
                caller: CallerId("signal-test"),
                graphql: None,
            })
            .await
            .unwrap()
            .source
    }

    async fn warm(&self, acct: &str, paths: &[&str]) {
        for path in paths {
            assert_eq!(self.get(acct, path).await, Source::Network, "{path}");
        }
    }

    async fn is_cached(&self, acct: &str, path: &str) -> bool {
        self.get(acct, path).await == Source::Cache
    }
}

fn pr(number: u32, branch: &str) -> BranchPrStatus {
    parse_pr_node(&serde_json::json!({
        "number": number, "headRefName": branch, "state": "OPEN",
    }))
    .unwrap()
}

fn pr_update(repo_path: &str, prs: Vec<BranchPrStatus>) -> AppEvent {
    AppEvent::GitHubPrUpdate {
        repo_path: repo_path.into(),
        statuses: prs,
    }
}

const LISTS: [&str; 2] = ["/repos/o/r/pulls", "/repos/o/r/issues"];

async fn apply(h: &Harness, a: &mut SignalAdapter, event: &AppEvent) {
    if let Some(signal) = a.on_event(event) {
        h.gateway.invalidate(&signal).await;
    }
}

#[tokio::test]
async fn pr_update_event_invalidates_that_repo_lists_only() {
    let h = Harness::new();
    let mut a = adapter();
    h.warm("a1", &LISTS).await;
    h.warm(
        "a1",
        &[
            "/repos/o/r2/pulls",
            "/repos/o/r/pulls/7",
            "/search/issues?q=x",
        ],
    )
    .await;

    apply(&h, &mut a, &pr_update("/repo", vec![pr(7, "feat/7")])).await;

    for list in LISTS {
        assert!(!h.is_cached("a1", list).await, "{list} must be dropped");
    }
    for kept in [
        "/repos/o/r2/pulls",
        "/repos/o/r/pulls/7",
        "/search/issues?q=x",
    ] {
        assert!(h.is_cached("a1", kept).await, "{kept} must stay");
    }
}

#[tokio::test]
async fn issue_intake_and_transition_events_invalidate_that_repo_lists() {
    let events = [
        AppEvent::GitHubIssuesUpdate {
            repo_path: "/repo".into(),
            issues: vec![],
        },
        AppEvent::GitHubIntakeIssuesUpdate {
            repo_path: "/repo".into(),
            issues: vec![],
        },
        AppEvent::GitHubTransition {
            transition: PrTransition::Merged {
                repo_path: "/repo".into(),
                branch: "feat/7".into(),
                pr_number: 7,
                title: "t".into(),
            },
        },
    ];
    for event in events {
        let h = Harness::new();
        h.warm("a1", &LISTS).await;
        h.warm("a1", &["/repos/o/r2/issues"]).await;

        apply(&h, &mut adapter(), &event).await;

        assert!(!h.is_cached("a1", LISTS[0]).await, "{event:?}");
        assert!(!h.is_cached("a1", LISTS[1]).await, "{event:?}");
        assert!(h.is_cached("a1", "/repos/o/r2/issues").await, "{event:?}");
    }
}

#[tokio::test]
async fn head_changed_invalidates_branch_pr() {
    let h = Harness::new();
    let mut a = adapter();
    apply(
        &h,
        &mut a,
        &pr_update("/repo", vec![pr(7, "feat/7"), pr(8, "feat/8")]),
    )
    .await;
    h.warm("a1", &["/repos/o/r/pulls/7", "/repos/o/r/pulls/8"])
        .await;
    h.warm("a1", &LISTS).await;

    let head = |branch: &str| AppEvent::HeadChanged {
        repo_path: "/repo".into(),
        branch: branch.into(),
    };
    apply(&h, &mut a, &head("feat/7")).await;

    assert!(!h.is_cached("a1", "/repos/o/r/pulls/7").await);
    assert!(h.is_cached("a1", "/repos/o/r/pulls/8").await);
    assert!(
        h.is_cached("a1", LISTS[0]).await,
        "lists are not the branch PR"
    );
    assert!(
        a.on_event(&head("no-pr-branch")).is_none(),
        "a branch without a known PR raises nothing"
    );
}

#[tokio::test]
async fn pr_that_left_the_poll_is_no_longer_a_branch_pr() {
    let mut a = adapter();
    a.on_event(&pr_update("/repo", vec![pr(7, "feat/7")]));
    a.on_event(&pr_update("/repo", vec![]));
    let head = AppEvent::HeadChanged {
        repo_path: "/repo".into(),
        branch: "feat/7".into(),
    };
    assert!(a.on_event(&head).is_none());
}

#[tokio::test]
async fn logout_clears_account() {
    let h = Harness::new();
    h.warm("a1", &["/repos/o/r/pulls", "/repos/o/r/pulls/7"])
        .await;
    h.warm("a2", &["/repos/o/r/pulls"]).await;

    let signal = on_account_cleared(&ClearedAccount {
        id: "a1".into(),
        github_api_base: Some(BASE.into()),
    })
    .expect("a GitHub account raises a signal");
    h.gateway.invalidate(&signal).await;

    assert!(!h.is_cached("a1", "/repos/o/r/pulls").await);
    assert!(!h.is_cached("a1", "/repos/o/r/pulls/7").await);
    assert!(h.is_cached("a2", "/repos/o/r/pulls").await);
}

#[test]
fn account_without_a_github_base_raises_nothing() {
    let cleared = ClearedAccount {
        id: "jira-1".into(),
        github_api_base: None,
    };
    assert!(on_account_cleared(&cleared).is_none());
}

#[test]
fn unresolved_repo_raises_nothing() {
    let event = AppEvent::GitHubIssuesUpdate {
        repo_path: "/unbound".into(),
        issues: vec![],
    };
    assert!(adapter().on_event(&event).is_none());
}

#[tokio::test]
async fn the_loop_applies_bus_events_and_account_clears() {
    let h = Harness::new();
    h.warm("a1", &LISTS).await;
    h.warm("a2", &["/repos/o/r/pulls"]).await;
    let (events_tx, events_rx) = broadcast::channel(8);
    let (cleared_tx, cleared_rx) = broadcast::channel(8);
    events_tx.send(pr_update("/repo", vec![])).unwrap();
    drop(events_tx);
    run(h.gateway.clone(), adapter(), events_rx, cleared_rx).await;
    assert!(!h.is_cached("a1", LISTS[0]).await, "bus event applied");

    let (events_tx, events_rx) = broadcast::channel(8);
    let (cleared_tx2, cleared_rx2) = broadcast::channel(8);
    cleared_tx2
        .send(ClearedAccount {
            id: "a2".into(),
            github_api_base: Some(BASE.into()),
        })
        .unwrap();
    drop(cleared_tx2);
    run(h.gateway.clone(), adapter(), events_rx, cleared_rx2).await;
    drop((events_tx, cleared_tx));
    assert!(
        !h.is_cached("a2", "/repos/o/r/pulls").await,
        "clear applied"
    );
    assert!(h.is_cached("a1", LISTS[0]).await, "other account stays");
}
