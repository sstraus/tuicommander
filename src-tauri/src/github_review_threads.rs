//! Review threads on the existing PR diff. The count-only API remains separate.
use crate::github::Origin;
use crate::{
    github_pr_api::{PrApi, RepoApi},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const THREAD_FIELDS: &str = "id path line originalLine diffSide isResolved isOutdated comments(first:100) { pageInfo { hasNextPage endCursor } nodes { id databaseId body url author { __typename login } createdAt } }";
const HEAD_QUERY: &str = "query ReviewedHead($owner:String!,$repo:String!,$number:Int!) { repository(owner:$owner,name:$repo) { pullRequest(number:$number) { headRefOid } } }";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewThreadsPage {
    pub head_sha: String,
    pub threads: Vec<Value>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DraftReview {
    pub head_sha: String,
    pub body: String,
    pub event: String,
    pub comments: Vec<crate::github::PrReviewInlineComment>,
}

/// Every comment author carries `bot`, so IPC and HTTP clients never re-derive it.
fn mark_bot(comment: &mut Value) {
    let bot = tuic_git::review_bots::is_bot(&comment["author"]);
    if let Some(author) = comment["author"].as_object_mut() {
        author.insert("bot".into(), json!(bot));
    }
}

fn mark_bot_authors(comments: &mut Value) {
    for comment in comments["nodes"].as_array_mut().into_iter().flatten() {
        mark_bot(comment);
    }
}

/// Marks the authors and attaches the finding parsed from the first comment:
/// the comment that opened the thread states the issue; replies discuss it.
fn enrich_thread(thread: &mut Value) {
    mark_bot_authors(&mut thread["comments"]);
    let finding = tuic_git::review_bots::finding_for(&thread["comments"]["nodes"][0]);
    thread["finding"] = json!(finding);
}

pub(crate) async fn read_threads(
    api: &impl PrApi,
    mut variables: Value,
    after: Option<String>,
) -> Result<ReviewThreadsPage, String> {
    variables["after"] = json!(after);
    let query = format!(
        "query ReviewThreadDetails($owner:String!,$repo:String!,$number:Int!,$after:String) {{ repository(owner:$owner,name:$repo) {{ pullRequest(number:$number) {{ headRefOid reviewThreads(first:100,after:$after) {{ pageInfo {{ hasNextPage endCursor }} nodes {{ {THREAD_FIELDS} }} }} }} }} }}"
    );
    let data = api.graphql(&query, variables).await?;
    let pr = &data["data"]["repository"]["pullRequest"];
    let head = pr["headRefOid"]
        .as_str()
        .ok_or("GitHub response missing PR head")?;
    let connection = &pr["reviewThreads"];
    let mut threads = connection["nodes"]
        .as_array()
        .ok_or("GitHub response missing review threads")?
        .clone();
    threads.iter_mut().for_each(enrich_thread);
    Ok(ReviewThreadsPage {
        head_sha: head.into(),
        threads,
        has_next_page: connection["pageInfo"]["hasNextPage"]
            .as_bool()
            .ok_or("Missing thread pagination")?,
        end_cursor: connection["pageInfo"]["endCursor"]
            .as_str()
            .map(str::to_owned),
    })
}

/// Replies beyond the first page are read explicitly, never silently discarded.
pub(crate) async fn read_comments(
    api: &impl PrApi,
    thread_id: &str,
    after: Option<String>,
) -> Result<Value, String> {
    let data = api.graphql("query ThreadComments($id:ID!,$after:String) { node(id:$id) { ... on PullRequestReviewThread { comments(first:100,after:$after) { pageInfo { hasNextPage endCursor } nodes { id databaseId body url author { __typename login } createdAt } } } } }", json!({"id":thread_id,"after":after})).await?;
    let mut comments = data["data"]["node"]["comments"].clone();
    if !comments["nodes"].is_array() {
        return Err("GitHub response missing thread comments".into());
    }
    mark_bot_authors(&mut comments);
    Ok(comments)
}

pub(crate) async fn resolve_thread(
    api: &impl PrApi,
    thread_id: &str,
    resolved: bool,
) -> Result<Value, String> {
    if thread_id.trim().is_empty() {
        return Err("Thread id is required".into());
    }
    let (operation, input) = if resolved {
        ("resolveReviewThread", "ResolveReviewThreadInput")
    } else {
        ("unresolveReviewThread", "UnresolveReviewThreadInput")
    };
    let query = format!(
        "mutation ThreadResolution($input:{input}!) {{ {operation}(input:$input) {{ thread {{ {THREAD_FIELDS} }} }} }}"
    );
    let data = api
        .graphql(&query, json!({"input":{"threadId":thread_id}}))
        .await?;
    let mut thread = data["data"][operation]["thread"].clone();
    if thread["isResolved"].as_bool() != Some(resolved) || thread["id"].as_str() != Some(thread_id)
    {
        return Err("GitHub did not confirm the requested thread resolution".into());
    }
    enrich_thread(&mut thread);
    Ok(thread)
}

pub(crate) async fn reply_thread(
    api: &impl PrApi,
    thread_id: &str,
    body: &str,
) -> Result<Value, String> {
    if thread_id.trim().is_empty() || body.trim().is_empty() {
        return Err("Thread id and reply body are required".into());
    }
    let data = api.graphql("mutation ReplyToThread($input:AddPullRequestReviewThreadReplyInput!) { addPullRequestReviewThreadReply(input:$input) { comment { id body url author { __typename login } createdAt } } }", json!({"input":{"pullRequestReviewThreadId":thread_id,"body":body}})).await?;
    let mut comment = data["data"]["addPullRequestReviewThreadReply"]["comment"].clone();
    if !comment["id"].is_string() {
        return Err("GitHub response missing posted reply".into());
    }
    mark_bot(&mut comment);
    Ok(comment)
}

pub(crate) fn validate_review(draft: &DraftReview) -> Result<(), String> {
    if draft.head_sha.trim().is_empty() {
        return Err("Reviewed head SHA is required".into());
    }
    crate::github_pr_actions::validate_review_event(&draft.body, &draft.event)?;
    if draft.comments.iter().any(|c| {
        c.path.trim().is_empty()
            || c.line == 0
            || c.body.trim().is_empty()
            || !matches!(c.side.as_deref(), None | Some("LEFT" | "RIGHT"))
    }) {
        return Err("Invalid inline review comment".into());
    }
    Ok(())
}

pub(crate) async fn submit_review(
    api: &impl PrApi,
    variables: Value,
    number: i64,
    draft: DraftReview,
) -> Result<crate::github::PostedPrReview, String> {
    validate_review(&draft)?;
    let data = api.graphql(HEAD_QUERY, variables).await?;
    if data["data"]["repository"]["pullRequest"]["headRefOid"].as_str() != Some(&draft.head_sha) {
        return Err(
            "review-head-changed: Refresh and review the new commits before submitting".into(),
        );
    }
    // Pin coordinates even if another commit arrives between the check and POST.
    let mut body =
        crate::github::build_post_pr_review_body(&draft.body, &draft.event, &draft.comments);
    body["commit_id"] = json!(draft.head_sha);
    let response = api
        .rest("POST", &format!("pulls/{number}/reviews"), body)
        .await?;
    crate::github::parse_posted_review(response)
}

pub(crate) async fn get_pr_line_threads_impl(
    path: &str,
    number: i64,
    after: Option<String>,
    state: &AppState,
) -> Result<ReviewThreadsPage, String> {
    let api = RepoApi::resolve(
        state,
        path,
        Origin::interactive("review_threads.get_pr_line_threads_impl"),
    )
    .await?;
    read_threads(&api, api.variables(number), after).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_pr_line_threads(
    repo_path: String,
    pr_number: i64,
    after: Option<String>,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<ReviewThreadsPage, String> {
    get_pr_line_threads_impl(&repo_path, pr_number, after, &state).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_pr_thread_comments(
    repo_path: String,
    thread_id: String,
    after: Option<String>,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("review_threads.get_pr_thread_comments"),
    )
    .await?;
    read_comments(&api, &thread_id, after).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn set_pr_thread_resolved(
    repo_path: String,
    thread_id: String,
    resolved: bool,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("review_threads.set_pr_thread_resolved"),
    )
    .await?;
    resolve_thread(&api, &thread_id, resolved).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn reply_pr_thread(
    repo_path: String,
    thread_id: String,
    body: String,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Value, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("review_threads.reply_pr_thread"),
    )
    .await?;
    reply_thread(&api, &thread_id, &body).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn submit_pr_draft_review(
    repo_path: String,
    pr_number: i64,
    draft: DraftReview,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<crate::github::PostedPrReview, String> {
    let api = RepoApi::resolve(
        &state,
        &repo_path,
        Origin::interactive("review_threads.submit_pr_draft_review"),
    )
    .await?;
    submit_review(&api, api.variables(pr_number), pr_number, draft).await
}

#[cfg(test)]
#[path = "github_review_threads_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "github_review_threads_live_tests.rs"]
mod live_tests;
