use super::RunStore;
use crate::github_pr_api::PrApi;
use crate::workflows::{
    github_event::{
        from_poller_issues, from_poller_prs, from_rest, open_snapshots, poll_snapshots,
    },
    github_trigger::GithubResource,
};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};
struct Api {
    responses: Mutex<VecDeque<Result<Value, String>>>,
    calls: Mutex<Vec<String>>,
}
impl Api {
    fn new(responses: Vec<Result<Value, String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            calls: Default::default(),
        }
    }
}
impl PrApi for Api {
    fn graphql(&self, _: &str, _: Value) -> impl Future<Output = Result<Value, String>> {
        std::future::poll_fn(|_| panic!("REST source only"))
    }
    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl Future<Output = Result<Value, String>> {
        assert_eq!(method, "GET");
        assert!(body.is_null());
        self.calls.lock().unwrap().push(endpoint.into());
        std::future::ready(
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected REST call"),
        )
    }
}
fn raw(kind: GithubResource, number: i64) -> Value {
    let mut value: Value = serde_json::from_str(match kind {
        GithubResource::Issue => include_str!("../fixtures/github-issue-rest.json"),
        GithubResource::PullRequest => include_str!("../fixtures/github-pr-rest.json"),
    })
    .unwrap();
    value["number"] = json!(number);
    value
}
// Catches F1: every open issue/PR being fetched again after listing, exhausting
// the user's core rate budget. 100 issues +30 PRs cost three list pages, not132 calls.
#[tokio::test]
async fn github_poll_cycle_uses_list_snapshots_without_per_open_resource_gets() {
    let api = Api::new(vec![
        Ok(json!(
            (1..=100)
                .map(|n| raw(GithubResource::Issue, n))
                .collect::<Vec<_>>()
        )),
        Ok(json!([])),
        Ok(json!(
            (101..=130)
                .map(|n| raw(GithubResource::PullRequest, n))
                .collect::<Vec<_>>()
        )),
    ]);
    let issues = poll_snapshots(
        &api,
        "octo-org/example",
        GithubResource::Issue,
        &(1..=100).collect::<Vec<_>>(),
        None,
    )
    .await
    .unwrap();
    let prs = poll_snapshots(
        &api,
        "octo-org/example",
        GithubResource::PullRequest,
        &(101..=130).collect::<Vec<_>>(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(issues.snapshots.len(), 100);
    assert_eq!(prs.snapshots.len(), 30);
    assert_eq!(
        *api.calls.lock().unwrap(),
        vec![
            "issues?state=open&per_page=100&page=1",
            "issues?state=open&per_page=100&page=2",
            "pulls?state=open&per_page=100&page=1"
        ]
    );
}
// Catches F2: a deleted/transferred known resource permanently preventing newer
// resources from being observed, or re-entering the failed GET on every cycle.
#[tokio::test]
async fn github_poll_cycle_retires_failed_missing_number_and_continues() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = RunStore::open_at(&dir.path().join("runs.sqlite3")).unwrap();
    for number in [1, 2, 4, 5] {
        store
            .normalize_github(
                from_rest(
                    "octo-org/example",
                    GithubResource::Issue,
                    &raw(GithubResource::Issue, number),
                )
                .unwrap(),
                None,
            )
            .unwrap();
    }
    let api = Api::new(vec![
        Ok(json!([raw(GithubResource::Issue, 3)])),
        Err(crate::github::http_error_message(
            reqwest::StatusCode::GONE,
            b"gone",
        )),
        Ok({
            let mut r = raw(GithubResource::Issue, 2);
            r["state"] = json!("closed");
            r
        }),
        Err(crate::github::http_error_message(
            reqwest::StatusCode::NOT_FOUND,
            b"not found",
        )),
        Ok(raw(GithubResource::Issue, 6)), // transferred response: wrong number
    ]);
    let polled = poll_snapshots(
        &api,
        "octo-org/example",
        GithubResource::Issue,
        &store
            .known_github_numbers("octo-org/example", GithubResource::Issue)
            .unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        polled
            .snapshots
            .iter()
            .map(|s| s.number)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(polled.unavailable, vec![1, 4, 5]);
    for number in polled.unavailable {
        store
            .retire_github_source("octo-org/example", GithubResource::Issue, number)
            .unwrap();
    }
    for snapshot in polled.snapshots {
        store.normalize_github(snapshot, None).unwrap();
    }
    let reopened = RunStore::open_at(&dir.path().join("runs.sqlite3")).unwrap();
    assert_eq!(
        reopened
            .known_github_numbers("octo-org/example", GithubResource::Issue)
            .unwrap(),
        vec![3]
    );
    assert_eq!(api.calls.lock().unwrap().len(), 5);
}
// Catches incomplete discovery, PRs entering issue admission and claiming a
// capped list is complete. Full documented payloads are used on every page.
#[tokio::test]
async fn github_source_discovery_pages_independently_and_refuses_truncation() {
    let page = json!(
        (1..=100)
            .map(|n| raw(GithubResource::Issue, n))
            .collect::<Vec<_>>()
    );
    let api = Api::new(vec![
        Ok(page.clone()),
        Ok(json!([raw(GithubResource::Issue,101),{"number":102,"pull_request":{}}])),
    ]);
    let snapshots = open_snapshots(&api, GithubResource::Issue, "octo-org/example")
        .await
        .unwrap();
    assert_eq!(snapshots.len(), 101);
    assert_eq!(snapshots.last().unwrap().number, 101);
    let api = Api::new(vec![Ok(page); 10]);
    assert!(
        open_snapshots(&api, GithubResource::Issue, "octo-org/example")
            .await
            .unwrap_err()
            .contains("complete-page bound")
    );
    assert_eq!(api.calls.lock().unwrap().len(), 10);
}

// Catches N1: retiring a known-open issue after a transient failure loses the
// subsequent closure, so its later reopen is never recognized as reopened.
#[tokio::test]
async fn transient_get_failure_keeps_known_number_for_later_closure_and_reopen() {
    for error in [
        crate::github::http_error_message(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            b"upstream mentions 404 but status is transient",
        ),
        "request timed out".into(),
        crate::github::http_error_message(reqwest::StatusCode::TOO_MANY_REQUESTS, b"rate limit"),
        crate::github::http_error_message(reqwest::StatusCode::FORBIDDEN, b"secondary rate limit"),
    ] {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let store = RunStore::open_at(&dir.path().join("runs.sqlite3")).unwrap();
        let initial = raw(GithubResource::Issue, 1);
        store
            .normalize_github(
                from_rest("octo-org/example", GithubResource::Issue, &initial).unwrap(),
                None,
            )
            .unwrap();
        let mut closed = initial.clone();
        closed["state"] = json!("closed");
        closed["updated_at"] = json!("2026-11-11T10:00:01Z");
        let mut reopened = initial;
        reopened["updated_at"] = json!("2026-11-11T10:00:02Z");
        let api = Api::new(vec![
            Ok(json!([])),
            Err(error.clone()),
            Ok(json!([])),
            Ok(closed),
            Ok(json!([reopened])),
        ]);
        let poll = poll_snapshots(
            &api,
            "octo-org/example",
            GithubResource::Issue,
            &store
                .known_github_numbers("octo-org/example", GithubResource::Issue)
                .unwrap(),
            None,
        )
        .await
        .unwrap();
        assert!(
            poll.unavailable.is_empty(),
            "transient error retired candidate: {error}"
        );
        assert_eq!(
            store
                .known_github_numbers("octo-org/example", GithubResource::Issue)
                .unwrap(),
            vec![1]
        );
        let poll = poll_snapshots(
            &api,
            "octo-org/example",
            GithubResource::Issue,
            &store
                .known_github_numbers("octo-org/example", GithubResource::Issue)
                .unwrap(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(poll.snapshots.len(), 1, "closure was missed after {error}");
        for snapshot in poll.snapshots {
            assert_eq!(
                store.normalize_github(snapshot, None).unwrap(),
                [] as [crate::workflows::trigger::TriggerEvent; 0]
            );
        }
        assert_eq!(
            store
                .known_github_numbers("octo-org/example", GithubResource::Issue)
                .unwrap(),
            [] as [i64; 0]
        );
        let poll = poll_snapshots(&api, "octo-org/example", GithubResource::Issue, &[], None)
            .await
            .unwrap();
        let events = store
            .normalize_github(poll.snapshots.into_iter().next().unwrap(), None)
            .unwrap();
        assert_eq!(events[0].event, "reopened", "reopen lost after {error}");
        assert_eq!(api.calls.lock().unwrap().len(), 5);
    }
}

fn poller_issue(number: i32) -> tuic_git::github::GitHubIssue {
    let rest = raw(GithubResource::Issue, i64::from(number));
    let title = rest["title"].as_str().unwrap().to_string();
    tuic_git::github::GitHubIssue {
        number,
        requirements_digest: tuic_git::github::requirements_digest(
            &title,
            rest["body"].as_str().unwrap(),
        ),
        title,
        state: "OPEN".into(),
        url: String::new(),
        author: String::new(),
        author_association: String::new(),
        labels: rest["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| tuic_git::github::PrLabel {
                name: l["name"].as_str().unwrap().into(),
                color: String::new(),
                text_color: String::new(),
                background_color: String::new(),
            })
            .collect(),
        assignees: vec![],
        milestone: None,
        comments_count: 0,
        created_at: String::new(),
        updated_at: rest["updated_at"].as_str().unwrap().into(),
    }
}

fn poller_pr(number: i64) -> tuic_git::github::IntakePullRequest {
    let rest = raw(GithubResource::PullRequest, number);
    let title = rest["title"].as_str().unwrap().to_string();
    tuic_git::github::IntakePullRequest {
        number,
        requirements_digest: tuic_git::github::requirements_digest(
            &title,
            rest["body"].as_str().unwrap(),
        ),
        title,
        labels: vec!["bug".into()],
        head_sha: rest["head"]["sha"].as_str().unwrap().into(),
        base_sha: rest["base"]["sha"].as_str().unwrap().into(),
        updated_at: rest["updated_at"].as_str().unwrap().into(),
    }
}

// Catches: the double download per tick. A complete poller list costs no list page;
// only a known-open number the list lacks costs one GET.
#[tokio::test]
async fn observe_uses_complete_poller_snapshot_without_rest_pages() {
    let issues = from_poller_issues("octo-org/example", &[poller_issue(1), poller_issue(2)])
        .unwrap()
        .expect("a list with digests is complete");
    let api = Api::new(vec![Ok({
        let mut r = raw(GithubResource::Issue, 3);
        r["state"] = json!("closed");
        r
    })]);
    let polled = poll_snapshots(
        &api,
        "octo-org/example",
        GithubResource::Issue,
        &[1, 2, 3],
        Some(issues),
    )
    .await
    .unwrap();
    assert_eq!(polled.snapshots.len(), 3);
    assert_eq!(*api.calls.lock().unwrap(), vec!["issues/3"]);
}

// Catches: a poller list that proves nothing (no body digest) replacing the REST
// pages, which would admit from unverified data.
#[tokio::test]
async fn observe_falls_back_to_rest_only_when_graphql_list_is_incomplete() {
    let mut bodiless = poller_issue(2);
    bodiless.requirements_digest.clear();
    assert!(
        from_poller_issues("octo-org/example", &[poller_issue(1), bodiless])
            .unwrap()
            .is_none()
    );
    let api = Api::new(vec![Ok(json!([raw(GithubResource::Issue, 1)]))]);
    let polled = poll_snapshots(&api, "octo-org/example", GithubResource::Issue, &[], None)
        .await
        .unwrap();
    assert_eq!(polled.snapshots.len(), 1);
    assert_eq!(
        *api.calls.lock().unwrap(),
        vec!["issues?state=open&per_page=100&page=1"]
    );
}

// Catches: the GraphQL-derived snapshot differing from the REST one for the same
// resource, which would re-admit every open resource as changed on the first tick.
#[test]
fn poller_snapshots_equal_the_rest_snapshots() {
    assert_eq!(
        from_poller_issues("octo-org/example", &[poller_issue(7)])
            .unwrap()
            .unwrap(),
        vec![
            from_rest(
                "octo-org/example",
                GithubResource::Issue,
                &raw(GithubResource::Issue, 7)
            )
            .unwrap()
        ]
    );
    assert_eq!(
        from_poller_prs("octo-org/example", &[poller_pr(8)]).unwrap(),
        vec![
            from_rest(
                "octo-org/example",
                GithubResource::PullRequest,
                &raw(GithubResource::PullRequest, 8)
            )
            .unwrap()
        ]
    );
}
