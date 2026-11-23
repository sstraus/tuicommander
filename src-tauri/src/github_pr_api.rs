//! Account-aware API boundary shared by review threads and PR actions.
use crate::{github::Origin, github_account::GitHubAccount, state::AppState};
use serde_json::Value;

pub(crate) trait PrApi: Sync {
    fn graphql(
        &self,
        query: &str,
        variables: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send;
    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send;
    fn rest_text(
        &self,
        _endpoint: &str,
        _max_bytes: usize,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send {
        std::future::ready(Err("Text downloads are unsupported".into()))
    }
}

/// Read a complete connection; incomplete pagination is an error, never evidence.
/// `connection_ptr` addresses the connection in the full GraphQL response.
pub(crate) async fn graphql_pages(
    api: &impl PrApi,
    query: &str,
    mut variables: Value,
    connection_ptr: &str,
    max_pages: usize,
) -> Result<Vec<Value>, String> {
    let mut pages = Vec::new();
    let mut cursors = std::collections::HashSet::new();
    variables["endCursor"] = Value::Null;
    for _ in 0..max_pages {
        let page = api.graphql(query, variables.clone()).await?;
        if page.get("errors").is_some_and(|errors| !errors.is_null()) {
            return Err("GitHub returned incomplete GraphQL data".into());
        }
        let info = page
            .pointer(connection_ptr)
            .and_then(|connection| connection.get("pageInfo"))
            .ok_or("GitHub response is missing connection pageInfo")?;
        let has_next = info["hasNextPage"]
            .as_bool()
            .ok_or("GitHub response is missing hasNextPage")?;
        let cursor = if has_next {
            let cursor = info["endCursor"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or("GitHub response is missing the next cursor")?;
            if !cursors.insert(cursor.to_owned()) {
                return Err("GitHub pagination repeated a cursor".into());
            }
            Some(cursor.to_owned())
        } else {
            None
        };
        pages.push(page);
        match cursor {
            Some(cursor) => variables["endCursor"] = Value::String(cursor),
            None => return Ok(pages),
        }
    }
    Err("GitHub pagination exceeded the page limit".into())
}

pub(crate) struct RepoApi<'a> {
    pub state: &'a AppState,
    pub account: GitHubAccount,
    pub token: String,
    pub owner: String,
    pub repo: String,
    pub origin: Origin,
}

impl<'a> RepoApi<'a> {
    pub async fn resolve(state: &'a AppState, path: &str, origin: Origin) -> Result<Self, String> {
        let (account, token, owner, repo) =
            crate::github::resolve_repo_for_rest(state, path).await?;
        Ok(Self {
            state,
            account,
            token,
            owner,
            repo,
            origin,
        })
    }
    fn text_request(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let path = format!("/repos/{}/{}/{}", self.owner, self.repo, endpoint);
        crate::github_debug::log_api("GET", &path, "CI job log download");
        crate::github::github_rest_request(
            &self.state.http_client,
            reqwest::Method::GET,
            &self.account.host,
            &path,
            &self.token,
            crate::github::GITHUB_JSON,
        )
    }

    pub fn variables(&self, number: i64) -> Value {
        serde_json::json!({"owner": self.owner, "repo": self.repo, "number": number})
    }
}

impl PrApi for RepoApi<'_> {
    async fn rest_text(&self, endpoint: &str, max_bytes: usize) -> Result<String, String> {
        download_text_tail(
            self.state,
            &self.account,
            self.origin,
            self.text_request(endpoint),
            max_bytes,
        )
        .await
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        crate::github::graphql_with_retry(
            self.state,
            &self.account,
            self.origin,
            query,
            variables,
            Some(&self.token),
        )
        .await
    }
    async fn rest(&self, method: &str, endpoint: &str, body: Value) -> Result<Value, String> {
        let path = format!("/repos/{}/{}/{}", self.owner, self.repo, endpoint);
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let request = crate::github::github_rest_request(
            &self.state.http_client,
            method.clone(),
            &self.account.host,
            &path,
            &self.token,
            crate::github::GITHUB_JSON,
        );
        let request = if method == reqwest::Method::GET {
            request
        } else {
            request.json(&body)
        };
        rest_json(self.state, &self.account, self.origin, request).await
    }
}

/// Send a REST request and parse its JSON body. Non-success statuses map to the
/// same user-facing text as the GraphQL layer; rate limits are classified by
/// `send_rest_with_breaker`.
async fn rest_json(
    state: &AppState,
    account: &GitHubAccount,
    origin: Origin,
    request: reqwest::RequestBuilder,
) -> Result<Value, String> {
    let response = crate::github::send_rest_with_breaker(state, account, origin, request).await?;
    let status = response.status();
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(crate::github::http_error_message(status, &bytes));
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid GitHub response: {e}"))
}

/// Stream an Actions log without retaining its unbounded prefix. The shared
/// client's redirect policy removes bearer auth when the download host changes.
async fn download_text_tail(
    state: &AppState,
    account: &GitHubAccount,
    origin: Origin,
    request: reqwest::RequestBuilder,
    max_bytes: usize,
) -> Result<String, String> {
    let mut response =
        crate::github::send_rest_with_breaker(state, account, origin, request).await?;
    let status = response.status();
    let mut tail = std::collections::VecDeque::with_capacity(max_bytes);
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if chunk.len() >= max_bytes {
            tail.clear();
            tail.extend(&chunk[chunk.len() - max_bytes..]);
        } else {
            let discard = tail
                .len()
                .saturating_add(chunk.len())
                .saturating_sub(max_bytes);
            tail.drain(..discard);
            tail.extend(&chunk);
        }
    }
    let bytes = tail.make_contiguous();
    // A byte cap can cut through a UTF-8 codepoint; omit its remaining bytes.
    let start = bytes
        .iter()
        .position(|byte| byte & 0xc0 != 0x80)
        .unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[start..]).into_owned();
    if !status.is_success() {
        return Err(crate::github::http_error_message(
            status,
            crate::github::sanitize_ci_logs(&text).as_bytes(),
        ));
    }
    Ok(text)
}

#[cfg(test)]
#[path = "github_ci_download_tests.rs"]
mod download_tests;
