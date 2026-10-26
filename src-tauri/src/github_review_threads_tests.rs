use super::*;
use std::sync::Mutex;

struct Stub {
    pr: Value,
    writes: Mutex<Vec<Value>>,
    resolved: Mutex<bool>,
}
impl Stub {
    fn recorded() -> Self {
        let data: Value = serde_json::from_str(include_str!(
            "../fixtures/github-review/threads-recorded.json"
        ))
        .unwrap();
        Self {
            pr: data["data"]["repository"]["pullRequests"]["nodes"][3].clone(),
            writes: Mutex::new(vec![]),
            resolved: Mutex::new(false),
        }
    }
}
impl PrApi for Stub {
    fn graphql(
        &self,
        query: &str,
        vars: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        if query.contains("ThreadResolution") {
            self.writes.lock().unwrap().push(vars.clone());
            let resolved = !query.contains("unresolveReviewThread(");
            *self.resolved.lock().unwrap() = resolved;
            let mut thread = self.pr["reviewThreads"]["nodes"][0].clone();
            thread["isResolved"] = json!(resolved);
            let operation = if resolved {
                "resolveReviewThread"
            } else {
                "unresolveReviewThread"
            };
            return std::future::ready(Ok(json!({"data":{operation:{"thread":thread}}})));
        }
        if query.contains("ReplyToThread") {
            self.writes.lock().unwrap().push(vars.clone());
            let mut comment = self.pr["reviewThreads"]["nodes"][0]["comments"]["nodes"][0].clone();
            comment["body"] = vars["input"]["body"].clone();
            return std::future::ready(Ok(
                json!({"data":{"addPullRequestReviewThreadReply":{"comment":comment}}}),
            ));
        }
        if query.contains("ThreadComments") {
            return std::future::ready(Ok(
                json!({"data":{"node":{"comments":self.pr["reviewThreads"]["nodes"][0]["comments"]}}}),
            ));
        }
        if query.contains("ReviewThreadDetails") {
            // The recorded thread is outdated: only a query that selects these
            // fields can render it as such.
            for field in ["isOutdated", "line", "originalLine"] {
                let selected = query.split_whitespace().any(|t| t == field);
                if !selected {
                    return std::future::ready(Err(format!("query does not select {field}")));
                }
            }
        }
        std::future::ready(Ok(json!({"data":{"repository":{"pullRequest":self.pr}}})))
    }
    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        self.writes
            .lock()
            .unwrap()
            .push(json!({"method":method,"endpoint":endpoint,"body":body}));
        let reviews: Value = serde_json::from_str(include_str!(
            "../fixtures/github-review/reviews-recorded.json"
        ))
        .unwrap();
        std::future::ready(Ok(reviews[0].clone()))
    }
}
fn draft(head: &str) -> DraftReview {
    DraftReview {
        head_sha: head.into(),
        body: "Review".into(),
        event: "COMMENT".into(),
        comments: vec![crate::github::PrReviewInlineComment {
            path: "src/main.rs".into(),
            line: 10,
            body: "Check this".into(),
            side: Some("RIGHT".into()),
        }],
    }
}

// Catches: posting line coordinates after the reviewed head has moved.
#[tokio::test]
async fn draft_review_rejected_when_head_moved() {
    let api = Stub::recorded();
    let error = submit_review(&api, json!({}), 340803, draft("old-sha"))
        .await
        .unwrap_err();
    assert!(error.contains("review-head-changed"));
    assert!(api.writes.lock().unwrap().is_empty());
}

// Catches: reading outside the GraphQL data envelope or attaching an outdated
// thread to its original line in the new diff.
#[tokio::test]
async fn outdated_thread_shown_as_outdated() {
    let api = Stub::recorded();
    let page = read_threads(&api, json!({}), None).await.unwrap();
    assert_eq!(page.head_sha, "7926288fd77e8fe9f86fb54228583c4454c66801");
    assert_eq!(page.threads[0]["isOutdated"], true);
    assert_eq!(page.threads[0]["line"], Value::Null);
    assert!(page.threads[0]["originalLine"].as_u64().is_some());
    assert!(!page.has_next_page);
    assert_ne!(
        page.threads[0]["comments"]["nodes"]
            .as_array()
            .unwrap()
            .as_slice(),
        [] as [serde_json::Value; 0]
    );
}

// Catches: optimistic-only resolution or an unresolve command that resolves again.
#[tokio::test]
async fn resolve_updates_thread_state() {
    let api = Stub::recorded();
    let id = api.pr["reviewThreads"]["nodes"][0]["id"].as_str().unwrap();
    let response = resolve_thread(&api, id, true).await.unwrap();
    assert_eq!(response["isResolved"], true);
    assert!(*api.resolved.lock().unwrap());
    let response = resolve_thread(&api, id, false).await.unwrap();
    assert_eq!(response["isResolved"], false);
    assert!(!*api.resolved.lock().unwrap());
    assert_eq!(api.writes.lock().unwrap()[0]["input"]["threadId"], id);
}

// Catches: omitting commit_id allows GitHub to rebind review coordinates to a new head.
#[tokio::test]
async fn current_head_review_posts_pinned_coordinates() {
    let api = Stub::recorded();
    let head = api.pr["headRefOid"].as_str().unwrap();
    let response = submit_review(&api, json!({}), 340803, draft(head))
        .await
        .unwrap();
    assert!(response.id > 0);
    let writes = api.writes.lock().unwrap();
    assert_eq!(writes[0]["endpoint"], "pulls/340803/reviews");
    assert_eq!(writes[0]["body"]["commit_id"], head);
    assert_eq!(writes[0]["body"]["comments"][0]["line"], 10);
}

// Catches: dropping a posted reply or accepting whitespace as a valid reply.
#[tokio::test]
async fn reply_returns_persisted_comment_and_rejects_empty_body() {
    let api = Stub::recorded();
    let reply = reply_thread(&api, "thread-id", "Please check")
        .await
        .unwrap();
    assert_eq!(reply["body"], "Please check");
    assert!(reply["id"].is_string());
    assert!(reply_thread(&api, "thread-id", "  ").await.is_err());
    assert_eq!(api.writes.lock().unwrap().len(), 1);
}

// Catches: null/deleted PRs being reported as an empty successful thread list.
#[tokio::test]
async fn absent_pr_returns_error() {
    let mut api = Stub::recorded();
    api.pr = Value::Null;
    assert!(read_threads(&api, json!({}), None).await.is_err());
}

// Catches: reading thread comments outside the GraphQL data envelope.
#[tokio::test]
async fn thread_comments_read_from_full_graphql_envelope() {
    let api = Stub::recorded();
    let comments = read_comments(&api, "thread-id", None).await.unwrap();
    assert_eq!(comments["nodes"][0]["databaseId"], 4234708030_u64);
    assert_eq!(comments["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(comments["pageInfo"]["hasNextPage"], false);
    assert_eq!(
        comments["pageInfo"]["endCursor"],
        "Y3Vyc29yOnYyOpK0MjAyNi0xMC0wOVQyMTo0Mjo1MVrO_GmtNg=="
    );
}

/// Serves the recorded cursor/verifiers#2833 thread page (fixtures README). Like
/// GitHub, it returns an author's `__typename` only when the query selects it.
struct BotPage;
impl PrApi for BotPage {
    fn graphql(
        &self,
        query: &str,
        _vars: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        let recorded: Value = serde_json::from_str(include_str!(
            "../fixtures/github-review/bot-threads-cur-2833.json"
        ))
        .unwrap();
        let mut nodes =
            recorded["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"].clone();
        if !query.contains("author { __typename login }") {
            for thread in nodes.as_array_mut().unwrap() {
                for comment in thread["comments"]["nodes"].as_array_mut().unwrap() {
                    comment["author"]
                        .as_object_mut()
                        .unwrap()
                        .remove("__typename");
                }
            }
        }
        std::future::ready(Ok(json!({"data":{"repository":{"pullRequest":{
            "headRefOid": "recorded-head",
            "reviewThreads": {"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes}
        }}}})))
    }
    fn rest(
        &self,
        _method: &str,
        _endpoint: &str,
        _body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        std::future::ready(Err("no REST in this stub".into()))
    }
}

// Catches: a thread query without `author.__typename`, so a GraphQL bot login
// (no `[bot]` suffix there, e.g. `cursor`) is shown as a human.
#[tokio::test]
async fn graphql_bot_login_without_suffix_is_marked_bot() {
    let page = read_threads(&BotPage, json!({}), None).await.unwrap();
    let author = |i: usize| &page.threads[i]["comments"]["nodes"][0]["author"];
    assert_eq!(author(0)["login"], "cursor");
    assert_eq!(author(0)["bot"], true);
    assert_eq!(author(5)["login"], "mikasenghaas");
    assert_eq!(author(5)["bot"], false);
}

/// One recorded Bugbot comment (cur-2833, comment 4251485369) and one human
/// reply-less comment, as the GraphQL thread query returns them.
struct FindingPage;

impl PrApi for FindingPage {
    fn graphql(
        &self,
        _query: &str,
        _vars: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        let recorded: Value = serde_json::from_str(include_str!(
            "../crates/tuic-git/src/review_bots/fixtures/comments.json"
        ))
        .unwrap();
        let body = recorded["sources"]["cur-2833"]["comments"][0]["body"].clone();
        let thread = |login: &str, kind: &str, body: Value| {
            json!({"id": login, "path": "verifiers/v1/rollout.py", "line": 1, "isResolved": false, "isOutdated": false,
                "comments": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                    "nodes": [{"id": "c", "body": body, "author": {"__typename": kind, "login": login}}]}})
        };
        std::future::ready(Ok(json!({"data":{"repository":{"pullRequest":{
            "headRefOid": "recorded-head",
            "reviewThreads": {"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": [
                thread("cursor", "Bot", body),
                thread("mikasenghaas", "User", json!("Should this stay sync?")),
            ]}
        }}}})))
    }
    fn rest(
        &self,
        _method: &str,
        _endpoint: &str,
        _body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        std::future::ready(Err("no REST in this stub".into()))
    }
}

// Catches: the parser exists but read_threads never attaches its finding, so
// the PR Files tab and the plugin port see only raw Markdown.
#[tokio::test]
async fn read_threads_attaches_the_parsed_finding() {
    let page = read_threads(&FindingPage, json!({}), None).await.unwrap();
    let finding = &page.threads[0]["finding"];
    assert_eq!(finding["tool"], "cursor");
    assert_eq!(finding["severity"], "high");
    assert_eq!(finding["title"], "Sidecar timeout still scores isolated");
    assert_eq!(
        finding["externalId"],
        "415bcdbf-9336-4062-a60b-ba008f1e2c7e"
    );
    assert_eq!(page.threads[1]["finding"], Value::Null);
}
