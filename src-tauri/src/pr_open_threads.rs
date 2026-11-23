//! The open review threads of a PR as untrusted prompt data, the value of the
//! `{pr_open_threads}` smart-prompt variable ("Address Review Comments").
use crate::github::Origin;
use crate::{
    github_pr_api::{PrApi, RepoApi},
    github_review_threads::read_threads,
    state::AppState,
};
use serde_json::Value;

/// Thread pages read per prompt, 100 threads each: the bound the thread split uses.
const MAX_PAGES: usize = 10;

/// Read every thread page (up to [`MAX_PAGES`]) and render the open ones. Past
/// the bound the remaining threads are neither listed nor counted.
pub(crate) async fn open_threads_prompt(
    api: &impl PrApi,
    variables: Value,
) -> Result<String, String> {
    let mut threads = Vec::new();
    let mut after = None;
    for _ in 0..MAX_PAGES {
        let page = read_threads(api, variables.clone(), after).await?;
        threads.extend(page.threads);
        if !page.has_next_page {
            break;
        }
        after = page.end_cursor;
    }
    Ok(tuic_git::review_bots::render_open_threads(&threads))
}

pub(crate) async fn get_pr_open_threads_impl(
    path: &str,
    number: i64,
    state: &AppState,
) -> Result<String, String> {
    let api = RepoApi::resolve(
        state,
        path,
        Origin::visible("pr_open_threads.get_pr_open_threads_impl"),
    )
    .await?;
    open_threads_prompt(&api, api.variables(number)).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_pr_open_threads(
    repo_path: String,
    pr_number: i64,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<String, String> {
    get_pr_open_threads_impl(&repo_path, pr_number, &state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    /// Two thread pages; records the `after` cursor of every call.
    struct TwoPages(Mutex<Vec<Value>>);

    impl PrApi for TwoPages {
        fn graphql(
            &self,
            _query: &str,
            vars: Value,
        ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
            let mut sent = self.0.lock().unwrap();
            sent.push(vars["after"].clone());
            let (path, next) = if sent.len() == 1 {
                ("first.rs", json!({"hasNextPage": true, "endCursor": "p2"}))
            } else {
                (
                    "second.rs",
                    json!({"hasNextPage": false, "endCursor": null}),
                )
            };
            let thread = json!({"path": path, "line": 1, "isResolved": false, "isOutdated": false,
                "comments": {"nodes": [{"body": "Fix it", "url": "u", "author": {"__typename": "User", "login": "a"}}]}});
            std::future::ready(Ok(json!({"data": {"repository": {"pullRequest": {
                "headRefOid": "h", "reviewThreads": {"pageInfo": next, "nodes": [thread]}
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

    // Catches: only the first 100 threads reaching the agent, so open findings on
    // a busy PR silently drop out of "Address Review Comments".
    #[tokio::test]
    async fn the_prompt_reads_every_thread_page() {
        let api = TwoPages(Mutex::new(Vec::new()));
        let prompt = open_threads_prompt(&api, json!({})).await.unwrap();
        assert!(
            prompt.contains("\n- first.rs:1 · human · unrated · Fix it · u\n"),
            "{prompt}"
        );
        assert!(
            prompt.contains("\n- second.rs:1 · human · unrated · Fix it · u\n"),
            "{prompt}"
        );
        assert_eq!(*api.0.lock().unwrap(), vec![Value::Null, json!("p2")]);
    }
}
