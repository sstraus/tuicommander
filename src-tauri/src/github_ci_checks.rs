//! Commit checks use the repository's account, independent of open PR ordering.
use super::{FailingCheck, is_github_actions_link, sanitize_ci_label};
use crate::github_pr_api::{PrApi, graphql_pages};
use serde_json::{Value, json};
use tuic_git::github::{CheckCategory, classify_check_node, dedup_rollup_nodes};

const CHECKS_QUERY: &str = include_str!("../fixtures/github-ci/rollup-query.graphql");
const CONNECTION: &str = "/data/repository/object/statusCheckRollup/contexts";
const MAX_CHECK_PAGES: usize = 100;

/// A commit with no checks has a null rollup, not an empty connection. Normalize
/// only that documented absence; missing commits and malformed pages stay errors.
struct CommitChecksApi<'a, A>(&'a A);

impl<A: PrApi> PrApi for CommitChecksApi<'_, A> {
    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        let mut page = self.0.graphql(query, variables).await?;
        if page.pointer("/data/repository/object/statusCheckRollup") == Some(&Value::Null) {
            page["data"]["repository"]["object"]["statusCheckRollup"] = json!({
                "contexts": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}
            });
        }
        Ok(page)
    }

    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        self.0.rest(method, endpoint, body)
    }
}

pub(super) async fn list_failing_checks_with(
    api: &impl PrApi,
    owner: &str,
    repo: &str,
    head_sha: &str,
) -> Result<Vec<FailingCheck>, String> {
    let nodes = commit_check_nodes(api, owner, repo, head_sha).await?;
    Ok(failing_checks_from_rollup(&Value::Array(nodes)))
}

/// Every check context of one commit, all pages, deduplicated to the latest run.
pub(crate) async fn commit_check_nodes(
    api: &impl PrApi,
    owner: &str,
    repo: &str,
    head_sha: &str,
) -> Result<Vec<Value>, String> {
    let pages = graphql_pages(
        &CommitChecksApi(api),
        CHECKS_QUERY,
        json!({"owner": owner, "name": repo, "oid": head_sha}),
        CONNECTION,
        MAX_CHECK_PAGES,
    )
    .await?;
    let mut nodes = Vec::new();
    for page in pages {
        let page_nodes = page
            .pointer(CONNECTION)
            .and_then(|connection| connection["nodes"].as_array())
            .ok_or("GitHub response is missing check nodes")?;
        nodes.extend(page_nodes.iter().cloned());
    }
    // Deduplicate after combining pages: a newer rerun can be on a later page.
    Ok(dedup_rollup_nodes(&json!({"nodes": nodes})))
}

pub(super) fn failing_checks_from_rollup(nodes: &Value) -> Vec<FailingCheck> {
    dedup_rollup_nodes(&json!({"nodes": nodes}))
        .into_iter()
        .filter(|node| classify_check_node(node) == CheckCategory::Failed)
        .map(|node| {
            let name = node["name"]
                .as_str()
                .or_else(|| node["context"].as_str())
                .unwrap_or("failed check");
            let link = node["detailsUrl"]
                .as_str()
                .or_else(|| node["targetUrl"].as_str())
                .unwrap_or("");
            FailingCheck {
                name: sanitize_ci_label(name),
                link: link.to_owned(),
                is_github_actions: is_github_actions_link(link),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct Stub(Mutex<VecDeque<Result<Value, String>>>);

    impl PrApi for Stub {
        fn graphql(
            &self,
            query: &str,
            variables: Value,
        ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
            // Reject requests for any other repository/commit or a PR-list query.
            let result = if query.contains("object(oid:$oid)")
                && variables["owner"] == "curl"
                && variables["name"] == "curl"
                && variables["oid"] == "6c5ad57264cc153ae583ed774d3f36b69992812f"
            {
                let result = self
                    .0
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected extra page");
                match &result {
                    Ok(_)
                        if variables["endCursor"] == Value::Null
                            || variables["endCursor"] == "MTAw" => {}
                    Ok(_) => return std::future::ready(Err("wrong pagination cursor".into())),
                    Err(_) => {}
                }
                result
            } else {
                Err("wrong commit query".into())
            };
            std::future::ready(result)
        }

        fn rest(
            &self,
            _: &str,
            _: &str,
            _: Value,
        ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
            std::future::ready(Err("unexpected REST request".into()))
        }
    }

    fn recorded_pages() -> Vec<Value> {
        serde_json::from_str(include_str!("../fixtures/github-ci/rollup-curl-23347.json")).unwrap()
    }

    async fn list(api: &Stub) -> Result<Vec<FailingCheck>, String> {
        list_failing_checks_with(
            api,
            "curl",
            "curl",
            "6c5ad57264cc153ae583ed774d3f36b69992812f",
        )
        .await
    }

    // Catches: PR-list lookup or missing the external failure on the second page.
    #[tokio::test]
    async fn commit_checks_include_external_failure_from_second_page() {
        let api = Stub(Mutex::new(recorded_pages().into_iter().map(Ok).collect()));
        let checks = list(&api).await.unwrap();
        assert!(
            checks
                .iter()
                .any(|check| check.name == "autotools & cmake" && check.is_github_actions)
        );
        assert!(!checks.iter().any(|check| check.name == "AM awslc"));
        let external = checks
            .iter()
            .find(|check| check.name == "AppVeyor CI")
            .unwrap();
        assert_eq!(
            external.link,
            "https://ci.appveyor.com/project/curlorg/curl/builds/54874752"
        );
        assert!(!external.is_github_actions);
    }

    // Catches: a partial page read reported as the complete failing-check set.
    #[tokio::test]
    async fn commit_checks_propagate_second_page_failure() {
        let api = Stub(Mutex::new(VecDeque::from([
            Ok(recorded_pages().remove(0)),
            Err("rate limited".into()),
        ])));
        assert_eq!(list(&api).await.unwrap_err(), "rate limited");
    }

    // Catches: deduplication per page retains a stale failure when its rerun is later.
    #[tokio::test]
    async fn successful_rerun_on_later_page_removes_stale_failure() {
        let mut pages = recorded_pages();
        let reruns: Vec<_> = pages
            .iter()
            .flat_map(|page| {
                page.pointer(CONNECTION).unwrap()["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
            })
            .filter(|node| node["name"] == "AM awslc")
            .cloned()
            .collect();
        // Model a different page boundary using unmodified, recorded check nodes.
        pages[0]["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"] =
            json!([reruns[0]]);
        pages[1]["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"] =
            json!([reruns[1]]);
        let api = Stub(Mutex::new(pages.into_iter().map(Ok).collect()));
        assert!(list(&api).await.unwrap().is_empty());
    }

    // Catches: a commit without CI treated as malformed, or missing commit treated as clean.
    #[tokio::test]
    async fn commit_without_rollup_is_clean_but_missing_commit_is_an_error() {
        let no_checks: Value =
            serde_json::from_str(include_str!("../fixtures/github-ci/rollup-no-checks.json"))
                .unwrap();
        for (page, clean) in [
            (no_checks, true),
            (json!({"data":{"repository":{"object":null}}}), false),
        ] {
            let api = Stub(Mutex::new(VecDeque::from([Ok(page)])));
            let result = list(&api).await;
            if clean {
                assert!(result.unwrap().is_empty());
            } else {
                assert!(result.is_err());
            }
        }
    }
}
