use super::{json_result, validate_repo_path};
use crate::github::Origin;
use crate::{
    github_pr_actions::{self as actions, PrAction},
    github_pr_api::RepoApi,
    state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PrQuery {
    repo_path: String,
    pr_number: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AutoMergeBody {
    repo_path: String,
    pr_number: i64,
    enabled: bool,
    merge_method: Option<String>,
    expected_head_sha: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueueBody {
    repo_path: String,
    pr_number: i64,
    enqueued: bool,
    expected_head_sha: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MergeBody {
    repo_path: String,
    pr_number: i64,
    merge_method: String,
    expected_head_sha: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RerunBody {
    repo_path: String,
    run_id: u64,
}

pub(super) async fn state(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PrQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(
            &state,
            &q.repo_path,
            Origin::interactive("mcp.pr_actions.state"),
        )
        .await?;
        actions::action_state(&api, api.variables(q.pr_number)).await
    }
    .await;
    json_result(result)
}
pub(super) async fn auto_merge(
    State(state): State<Arc<AppState>>,
    Json(q): Json<AutoMergeBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let action = if q.enabled {
        PrAction::EnableAutoMerge
    } else {
        PrAction::DisableAutoMerge
    };
    let result = async {
        let api = RepoApi::resolve(
            &state,
            &q.repo_path,
            Origin::interactive("mcp.pr_actions.auto_merge"),
        )
        .await?;
        actions::change_action(
            &api,
            api.variables(q.pr_number),
            action,
            q.expected_head_sha.as_deref(),
            q.merge_method.as_deref(),
        )
        .await
    }
    .await;
    json_result(result)
}
pub(super) async fn queue(
    State(state): State<Arc<AppState>>,
    Json(q): Json<QueueBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let action = if q.enqueued {
        PrAction::Enqueue
    } else {
        PrAction::Dequeue
    };
    let result = async {
        let api = RepoApi::resolve(
            &state,
            &q.repo_path,
            Origin::interactive("mcp.pr_actions.queue"),
        )
        .await?;
        actions::change_action(
            &api,
            api.variables(q.pr_number),
            action,
            q.expected_head_sha.as_deref(),
            None,
        )
        .await
    }
    .await;
    json_result(result)
}
pub(super) async fn merge(
    State(state): State<Arc<AppState>>,
    Json(q): Json<MergeBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(
            &state,
            &q.repo_path,
            Origin::interactive("mcp.pr_actions.merge"),
        )
        .await?;
        actions::merge_or_enqueue(
            &api,
            api.variables(q.pr_number),
            q.pr_number,
            &q.merge_method,
            &q.expected_head_sha,
        )
        .await
    }
    .await;
    json_result(result)
}
pub(super) async fn rerun(
    State(state): State<Arc<AppState>>,
    Json(q): Json<RerunBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(
            &state,
            &q.repo_path,
            Origin::interactive("mcp.pr_actions.rerun"),
        )
        .await?;
        actions::rerun_failed_jobs(&api, q.run_id).await
    }
    .await;
    json_result(result)
}
