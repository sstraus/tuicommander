//! Desktop wrappers for the PR action backend; HTTP calls the same core functions.
use crate::github::Origin;
use crate::{
    github_pr_actions::{self as actions, PrAction, PrActionState},
    github_pr_api::RepoApi,
    state::AppState,
};
use serde_json::Value;
use std::sync::Arc;

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_pr_action_state(
    repo_path: String,
    pr_number: i64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<PrActionState, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("pr_actions.get_pr_action_state"),
    )
    .await?;
    actions::action_state(&api, api.variables(pr_number)).await
}
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn set_pr_auto_merge(
    repo_path: String,
    pr_number: i64,
    enabled: bool,
    merge_method: Option<String>,
    expected_head_sha: Option<String>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("pr_actions.set_pr_auto_merge"),
    )
    .await?;
    let action = if enabled {
        PrAction::EnableAutoMerge
    } else {
        PrAction::DisableAutoMerge
    };
    actions::change_action(
        &api,
        api.variables(pr_number),
        action,
        expected_head_sha.as_deref(),
        merge_method.as_deref(),
    )
    .await
}
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn set_pr_merge_queue(
    repo_path: String,
    pr_number: i64,
    enqueued: bool,
    expected_head_sha: Option<String>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("pr_actions.set_pr_merge_queue"),
    )
    .await?;
    let action = if enqueued {
        PrAction::Enqueue
    } else {
        PrAction::Dequeue
    };
    actions::change_action(
        &api,
        api.variables(pr_number),
        action,
        expected_head_sha.as_deref(),
        None,
    )
    .await
}
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn merge_or_enqueue_pr(
    repo_path: String,
    pr_number: i64,
    merge_method: String,
    expected_head_sha: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("pr_actions.merge_or_enqueue_pr"),
    )
    .await?;
    actions::merge_or_enqueue(
        &api,
        api.variables(pr_number),
        pr_number,
        &merge_method,
        &expected_head_sha,
    )
    .await
}
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn rerun_failed_pr_jobs(
    repo_path: String,
    run_id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("pr_actions.rerun_failed_pr_jobs"),
    )
    .await?;
    actions::rerun_failed_jobs(&api, run_id).await
}
