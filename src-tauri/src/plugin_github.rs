//! Account-scoped plugin adapter for the existing B6/B7 services.
use crate::{
    github::Origin,
    github_pr_actions::{self as actions, PrAction},
    github_pr_api::RepoApi,
    github_review_threads as reviews,
    state::AppState,
    work_items::model::WorkItemRef,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Request {
    Threads {
        after: Option<String>,
    },
    State {},
    AutoMerge {
        enabled: bool,
        merge_method: Option<String>,
        expected_head_sha: Option<String>,
    },
    Queue {
        enqueued: bool,
        expected_head_sha: Option<String>,
    },
    Merge {
        merge_method: String,
        expected_head_sha: String,
    },
    Rerun {
        run_id: u64,
    },
    Review {
        draft: reviews::DraftReview,
    },
}

/// Validate identity before touching account credentials or the network.
fn target(account: &str, reference: &WorkItemRef) -> Result<(String, String, i64), String> {
    if account.is_empty() || reference.provider_id != "github" || reference.account_id != account {
        return Err("GitHub account mismatch".into());
    }
    let parts: Vec<_> = reference.id.split('/').collect();
    if parts.len() != 4 || parts[2] != "pull" {
        return Err("GitHub bridge requires a pull request reference".into());
    }
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    let number = parts[3].parse::<i64>().ok().filter(|n| *n > 0);
    if !valid(parts[0]) || !valid(parts[1]) || number.is_none() {
        return Err("Invalid GitHub pull request identity".into());
    }
    Ok((parts[0].into(), parts[1].into(), number.unwrap()))
}

pub(crate) async fn execute(
    state: &Arc<AppState>,
    plugin_id: String,
    account: String,
    reference: WorkItemRef,
    request: Request,
) -> Result<Value, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "net:http")?;
    let manifest = crate::plugins::read_single_manifest(&plugin_id)?;
    if !manifest.declares_provider("github") {
        return Err("Plugin does not declare account provider github".into());
    }
    let (owner, repo, number) = target(&account, &reference)?;
    let (record, token) = crate::accounts::load_credential(account, state).await?;
    let account =
        crate::accounts::github_account(&record).ok_or("Selected account is not GitHub")?;
    // Never resolve a local repository or fall back to its default account.
    let api = RepoApi {
        state,
        account,
        token,
        owner,
        repo,
        origin: Origin::visible("plugin_github.dispatch"),
    };
    dispatch(&api, api.variables(number), number, request).await
}

async fn dispatch(
    api: &impl crate::github_pr_api::PrApi,
    variables: Value,
    number: i64,
    request: Request,
) -> Result<Value, String> {
    match request {
        Request::Threads { after } => Ok(json!(
            reviews::read_threads(api, variables.clone(), after).await?
        )),
        Request::State {} => Ok(json!(actions::action_state(api, variables.clone()).await?)),
        Request::AutoMerge {
            enabled,
            merge_method,
            expected_head_sha,
        } => {
            actions::change_action(
                api,
                variables.clone(),
                if enabled {
                    PrAction::EnableAutoMerge
                } else {
                    PrAction::DisableAutoMerge
                },
                expected_head_sha.as_deref(),
                merge_method.as_deref(),
            )
            .await
        }
        Request::Queue {
            enqueued,
            expected_head_sha,
        } => {
            actions::change_action(
                api,
                variables.clone(),
                if enqueued {
                    PrAction::Enqueue
                } else {
                    PrAction::Dequeue
                },
                expected_head_sha.as_deref(),
                None,
            )
            .await
        }
        Request::Merge {
            merge_method,
            expected_head_sha,
        } => {
            actions::merge_or_enqueue(
                api,
                variables.clone(),
                number,
                &merge_method,
                &expected_head_sha,
            )
            .await
        }
        Request::Rerun { run_id } => actions::rerun_failed_jobs(api, run_id).await,
        Request::Review { draft } => Ok(json!(
            reviews::submit_review(api, variables.clone(), number, draft).await?
        )),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn plugin_github_action(
    plugin_id: String,
    account: String,
    reference: WorkItemRef,
    request: Request,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    execute(state.inner(), plugin_id, account, reference, request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a selected plugin account gets silently replaced by the item's account.
    #[test]
    fn plugin_github_rejects_cross_account_and_non_pr_targets() {
        let mut reference = WorkItemRef {
            provider_id: "github".into(),
            account_id: "selected".into(),
            id: "owner/repo/pull/12".into(),
        };
        assert_eq!(
            target("selected", &reference).unwrap(),
            ("owner".into(), "repo".into(), 12)
        );
        assert!(
            target("other", &reference)
                .unwrap_err()
                .contains("account mismatch")
        );
        for id in [
            "owner/repo/issue/12",
            "owner/repo/pull/0",
            "owner/../pull/12",
            "owner/repo/pull/9223372036854775808",
        ] {
            reference.id = id.into();
            assert!(target("selected", &reference).is_err(), "{id}");
        }
    }

    // Catches: arbitrary payload fields overriding the command's bound account/repository.
    #[test]
    fn plugin_github_request_rejects_identity_overrides() {
        assert!(
            serde_json::from_value::<Request>(json!({"action":"state","account":"other"})).is_err()
        );
        assert!(
            serde_json::from_value::<Request>(
                json!({"action":"merge","mergeMethod":"squash","expectedHeadSha":"head"})
            )
            .is_ok()
        );
    }
    // Catches: the bridge rewrites core head-pinned requests or skips merge queue policy.
    #[tokio::test]
    async fn plugin_github_dispatch_preserves_core_policy() {
        use crate::github_pr_api::PrApi;
        use std::sync::Mutex;
        struct Recorded {
            writes: Mutex<Vec<Value>>,
        }
        impl PrApi for Recorded {
            fn graphql(
                &self,
                query: &str,
                variables: Value,
            ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
                if query.contains("mutation") {
                    self.writes
                        .lock()
                        .unwrap()
                        .push(json!({"query":query,"variables":variables}));
                    return std::future::ready(Ok(
                        json!({"data":{"enqueuePullRequest":{"mergeQueueEntry":{"id":"contract-entry","position":1,"state":"QUEUED"}}}}),
                    ));
                }
                let recording: Value = serde_json::from_str(include_str!(
                    "../fixtures/github-review/actions-pr-recorded.json"
                ))
                .unwrap();
                std::future::ready(Ok(
                    json!({"data":{"repository":{"pullRequest":recording["data"]["repository"]["pullRequests"]["nodes"][0]}}}),
                ))
            }
            fn rest(
                &self,
                _method: &str,
                _endpoint: &str,
                _body: Value,
            ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
                std::future::ready(Err(
                    "queue-required PR must never use direct REST merge".into()
                ))
            }
        }
        let api = Recorded {
            writes: Mutex::new(vec![]),
        };
        let variables = json!({"owner":"recorded","repo":"repository","number":46254});
        let state = dispatch(&api, variables.clone(), 46254, Request::State {})
            .await
            .unwrap();
        let head = state["headSha"].as_str().unwrap().to_owned();
        assert!(
            dispatch(
                &api,
                variables.clone(),
                46254,
                Request::Merge {
                    merge_method: "squash".into(),
                    expected_head_sha: "stale".into()
                }
            )
            .await
            .is_err()
        );
        assert!(api.writes.lock().unwrap().is_empty());
        let result = dispatch(
            &api,
            variables,
            46254,
            Request::Merge {
                merge_method: "squash".into(),
                expected_head_sha: head.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result["action"], "enqueued");
        let writes = api.writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0]["variables"]["input"]["expectedHeadOid"], head);
    }

    // Catches: direct HTTP/IPC bridge callers bypass loaded-plugin capability and manifest consent.
    #[tokio::test]
    async fn plugin_github_requires_loaded_capability_and_provider_consent() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let reference = WorkItemRef {
            provider_id: "github".into(),
            account_id: "selected".into(),
            id: "owner/repo/pull/12".into(),
        };
        let error = execute(
            &state,
            "bridge-test".into(),
            "selected".into(),
            reference.clone(),
            Request::State {},
        )
        .await
        .unwrap_err();
        assert!(
            error.contains("capability") || error.contains("not registered"),
            "{error}"
        );
        let plugin_dir = dir.path().join("plugins/bridge-test");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("manifest.json"), json!({"id":"bridge-test","name":"Bridge","version":"1.0.0","minAppVersion":"0.0.0","main":"main.js","capabilities":["net:http"],"allowedUrls":["https://api.github.com/*"],"providers":[]}).to_string()).unwrap();
        crate::plugins::register_loaded_plugin_impl(
            &state,
            "bridge-test".into(),
            vec!["net:http".into()],
        )
        .unwrap();
        let error = execute(
            &state,
            "bridge-test".into(),
            "selected".into(),
            reference,
            Request::State {},
        )
        .await
        .unwrap_err();
        assert!(
            error.contains("does not declare account provider"),
            "{error}"
        );
    }
}
