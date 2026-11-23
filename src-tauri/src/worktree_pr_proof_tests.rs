use super::*;
use crate::github_pr_api::{PrApi, graphql_pages};
use serde_json::{Value, json};
use std::sync::Mutex;

// The recorded merged-PR nodes provide the schema; local stub effects substitute
// a local branch/head and pagination controls so no remote Git fetch is needed.
struct Stub {
    pages: Mutex<std::collections::VecDeque<Result<Value, String>>>,
    cursors: Mutex<Vec<Value>>,
}

impl Stub {
    fn new(pages: Vec<Result<Value, String>>) -> Self {
        Self {
            pages: Mutex::new(pages.into()),
            cursors: Mutex::new(Vec::new()),
        }
    }
}

impl PrApi for Stub {
    fn graphql(
        &self,
        _: &str,
        variables: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        self.cursors
            .lock()
            .unwrap()
            .push(variables["endCursor"].clone());
        std::future::ready(
            self.pages
                .lock()
                .unwrap()
                .pop_front()
                .expect("bounded requests"),
        )
    }

    fn rest(
        &self,
        _: &str,
        _: &str,
        _: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        std::future::ready(Err("merged PR proof must use GraphQL".to_string()))
    }
}

fn page(nodes: Value, next: bool, cursor: Value) -> Value {
    json!({"data":{"repository":{"pullRequests":{
        "nodes":nodes,"pageInfo":{"hasNextPage":next,"endCursor":cursor}
    }}}})
}

fn merged_node(tip: &str) -> Value {
    let recorded: Value = serde_json::from_str(include_str!(
        "../fixtures/github-review/threads-recorded.json"
    ))
    .unwrap();
    let mut node = recorded["data"]["repository"]["pullRequests"]["nodes"][0].clone();
    node["state"] = json!("MERGED");
    node["headRefName"] = json!("feature");
    node["headRefOid"] = json!(tip);
    node
}

// Catches: stopping after page 1, losing the cursor, or failing to restore the
// full response expected by the unchanged ancestry verifier.
#[tokio::test]
async fn merged_pr_proof_reads_second_page_and_proves_only_the_recorded_tip() {
    let repo = tuic_git::test_fixtures::setup_test_repo();
    let tip = git_cmd(repo.path())
        .args(["rev-parse", "HEAD"])
        .run()
        .unwrap()
        .stdout
        .trim()
        .to_owned();
    let api = Stub::new(vec![
        Ok(page(json!([]), true, json!("page-2"))),
        Ok(page(json!([merged_node(&tip)]), false, Value::Null)),
    ]);
    assert_eq!(
        query_merged_github_pr_with(&api, json!({}), repo.path(), "feature", &tip).await,
        Some(true)
    );
    assert_eq!(
        *api.cursors.lock().unwrap(),
        vec![Value::Null, json!("page-2")]
    );
    let api = Stub::new(vec![Ok(page(
        json!([merged_node(&tip)]),
        false,
        Value::Null,
    ))]);
    assert_eq!(
        query_merged_github_pr_with(&api, json!({}), repo.path(), "other-branch", &tip).await,
        Some(false)
    );
}

// Catches: returning/caching a partial answer after page 2 fails, including when
// page 1 already contains a matching merged PR.
#[test]
fn merged_pr_proof_page_two_error_is_unknown_and_never_cached() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let repo = tuic_git::test_fixtures::setup_test_repo();
    let tip = git_cmd(repo.path())
        .args(["rev-parse", "HEAD"])
        .run()
        .unwrap()
        .stdout
        .trim()
        .to_owned();
    let cache = moka::sync::Cache::builder().max_capacity(8).build();
    let failures = moka::sync::Cache::builder().max_capacity(8).build();
    for nodes in [json!([]), json!([merged_node(&tip)])] {
        let api = Stub::new(vec![
            Ok(page(nodes, true, json!("page-2"))),
            Err("page 2 unavailable".into()),
        ]);
        let answer = runtime.block_on(query_merged_github_pr_with(
            &api,
            json!({}),
            repo.path(),
            "feature",
            &tip,
        ));
        assert_eq!(answer, None);
        assert!(!cached_pr_proof(
            &cache,
            &failures,
            repo.path(),
            "feature",
            &tip,
            || answer
        ));
        assert_eq!(
            cache.get(&(repo.path().to_owned(), "feature".into(), tip.clone())),
            None
        );
    }
}

// Catches: accepting malformed or looping pagination as a definitive negative
// answer, or silently treating a page-cap truncation as a complete response.
#[tokio::test]
async fn graphql_pages_rejects_incomplete_and_cyclic_connections() {
    let continuing = page(json!([]), true, json!("cursor"));
    for (pages, cap) in [
        (vec![Ok(continuing.clone())], 1),
        (vec![Ok(page(json!([]), true, Value::Null))], 10),
        (vec![Ok(continuing.clone()), Ok(continuing)], 10),
        (
            vec![Ok(
                json!({"data":{"repository":{"pullRequests":{"nodes":[]}}}}),
            )],
            10,
        ),
        (
            vec![Ok(
                json!({"data":{"repository":{"pullRequests":{"nodes":[],"pageInfo":{"hasNextPage":false}}}},"errors":[{"message":"partial failure"}]}),
            )],
            10,
        ),
    ] {
        assert!(
            graphql_pages(
                &Stub::new(pages),
                "query",
                json!({}),
                "/data/repository/pullRequests",
                cap
            )
            .await
            .is_err()
        );
    }
}

// Catches: running Handle::block_on on a runtime worker rather than on the
// caller's blocking thread. An isolated repo with no remote fails closed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merged_pr_proof_closure_runs_on_spawn_blocking_without_panic() {
    let repo = tuic_git::test_fixtures::setup_test_repo();
    let _config = crate::config::set_config_dir_override(repo.path().join("config"));
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        repo.path(),
    ));
    let proof = pr_proof(state, tokio::runtime::Handle::current());
    assert!(
        !tokio::task::spawn_blocking(move || proof(repo.path(), "feature", "tip"))
            .await
            .unwrap()
    );
}

fn poller_pr(branch: &str, state: &str, tip: &str) -> crate::github::BranchPrStatus {
    crate::github::BranchPrStatus {
        branch: branch.into(),
        number: 42,
        title: "Feature".into(),
        state: state.into(),
        url: String::new(),
        additions: 0,
        deletions: 0,
        checks: crate::github::CheckSummary {
            passed: 0,
            failed: 0,
            pending: 0,
            total: 0,
        },
        author: "author".into(),
        commits: 1,
        mergeable: "UNKNOWN".into(),
        merge_state_status: String::new(),
        review_decision: String::new(),
        viewer_did_approve: false,
        labels: vec![],
        is_draft: false,
        base_ref_name: "main".into(),
        head_ref_oid: tip.into(),
        created_at: String::new(),
        updated_at: String::new(),
        merge_state_label: None,
        conflict_state: crate::github::ConflictState::Clear,
        review_state_label: None,
        merge_commit_allowed: true,
        squash_merge_allowed: true,
        rebase_merge_allowed: true,
        unresolved_threads: 0,
        unresolved_threads_truncated: false,
    }
}

// Catches: a GraphQL call when the poller already knows the PR is merged; also a
// poller hit for another tip or a still-open PR inventing merged status.
#[tokio::test]
async fn merged_proof_hits_poller_state_without_graphql() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    crate::github_poller::ensure_polling(&state, vec![], String::new(), false);
    let previous = Arc::clone(&state.github.poller.lock().as_ref().unwrap().previous_prs);
    previous.write().insert(
        "/repo".into(),
        HashMap::from([
            ("merged".into(), poller_pr("merged", "MERGED", "tip")),
            ("open".into(), poller_pr("open", "OPEN", "tip")),
        ]),
    );
    let repo = Path::new("/repo");
    let network = std::cell::Cell::new(0);
    let ask = |branch: &str, tip: &str| {
        proof_lookup(&state, repo, branch, tip, || {
            network.set(network.get() + 1);
            Some(false)
        })
    };
    assert_eq!(ask("merged", "tip"), Some(true));
    assert_eq!(network.get(), 0, "the poller answered; no network call");
    assert_eq!(ask("merged", "other-tip"), Some(false));
    assert_eq!(ask("open", "tip"), Some(false));
    assert_eq!(network.get(), 2, "only poller misses reach the network");
}

// Catches: a retry on every repo-changed after a failed lookup, and a failure
// remembered past its 60 s window or as if it were a definitive answer.
#[test]
fn failed_lookup_is_cached_briefly() {
    let proofs = moka::sync::Cache::builder().max_capacity(8).build();
    let failures = moka::sync::Cache::builder()
        .max_capacity(8)
        .time_to_live(Duration::from_millis(50))
        .build();
    let repo = Path::new("/repo");
    let calls = std::cell::Cell::new(0);
    let ask = |answer: Option<bool>| {
        cached_pr_proof(&proofs, &failures, repo, "branch", "tip", || {
            calls.set(calls.get() + 1);
            answer
        })
    };
    assert!(!ask(None));
    assert!(!ask(Some(true)), "a repo-changed storm must not retry");
    assert_eq!(calls.get(), 1);
    std::thread::sleep(Duration::from_millis(80));
    assert!(
        ask(Some(true)),
        "the failure expired and the lookup retried"
    );
    assert_eq!(calls.get(), 2);
    assert!(ask(None), "a definitive answer is still reused");
    assert_eq!(calls.get(), 2);
}
