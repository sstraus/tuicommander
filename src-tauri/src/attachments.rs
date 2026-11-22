//! Session-owned binary attachment uploads.

use std::path::PathBuf;

mod storage;
use std::sync::Arc;
pub(crate) use storage::{
    cleanup_orphans, prepare as prepare_session_directory, remove as cleanup_session,
};

use axum::Json;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::AppState;

type UploadError = (StatusCode, Json<serde_json::Value>);

#[derive(Deserialize)]
pub(crate) struct UploadQuery {
    kind: String,
    id: String,
    session: Option<String>,
    name: String,
}

#[derive(Serialize)]
pub(crate) struct AttachmentReceipt {
    pub(crate) path: String,
    pub(crate) size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notice: Option<String>,
}

fn error(status: StatusCode, message: &str) -> UploadError {
    (status, Json(serde_json::json!({ "error": message })))
}

fn session_directory(state: &AppState, query: &UploadQuery) -> Result<PathBuf, UploadError> {
    if query.kind == "acp" {
        let id = serde_json::from_value(serde_json::json!(query.id))
            .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid ACP connection id"))?;
        let session = query.session.as_deref().ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "ACP uploads require the conversation session id",
            )
        })?;
        return state
            .acp
            .attachment_directory(id, session, &state.data_dir)
            .map_err(|message| {
                error(
                    if message == "ACP connection not found" {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::CONFLICT
                    },
                    &message,
                )
            });
    }
    if query.kind != "pty" {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "unsupported attachment target",
        ));
    }
    // The map guard prevents explicit teardown from removing the session until
    // its directory has been opened. Teardown removes the map before the files.
    let session = state
        .session_maps
        .sessions
        .get(&query.id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "session not found"))?;
    if state.session_maps.exit_codes.contains_key(&query.id) {
        return Err(error(StatusCode::CONFLICT, "session has ended"));
    }
    let needs_grant = state
        .session_maps
        .session_states
        .get(&query.id)
        .and_then(|row| row.agent_type.clone())
        .is_some_and(|agent| ["claude", "agy"].contains(&agent.as_str()));
    if needs_grant
        && !state
            .session_maps
            .attachment_launch_dirs
            .contains_key(&query.id)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "This agent started before attachment access was configured. Start a new session to attach files.",
        ));
    }
    let directory = storage::prepare(&state.data_dir, "pty", &query.id)
        .map_err(|message| error(StatusCode::BAD_REQUEST, &message));
    drop(session);
    directory
}

fn safe_name(name: &str) -> Result<String, UploadError> {
    let name: String = name
        .chars()
        .take(120)
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        return Err(error(StatusCode::BAD_REQUEST, "invalid attachment name"));
    }
    Ok(name)
}

pub(crate) async fn upload_http(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<AttachmentReceipt>, UploadError> {
    upload(state, query, headers, body).await.map(Json)
}

async fn upload(
    state: Arc<AppState>,
    query: UploadQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<AttachmentReceipt, UploadError> {
    let name = safe_name(&query.name)?;
    let max_bytes = state.config.read().attachment_max_bytes;
    if max_bytes == 0 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "attachment limit must be positive",
        ));
    }
    if headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > max_bytes)
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "attachment exceeds configured limit",
        ));
    }
    let ego = query.kind == "acp"
        || (state
            .session_maps
            .session_states
            .get(&query.id)
            .is_some_and(|session| session.agent_type.as_deref() == Some("ego"))
            && !state
                .session_maps
                .attachment_launch_dirs
                .contains_key(&query.id));
    let directory_state = Arc::clone(&state);
    let directory =
        tokio::task::spawn_blocking(move || session_directory(&directory_state, &query))
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not prepare attachment directory",
                )
            })??;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = directory.join(format!("{timestamp}-{name}"));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not create attachment",
                )
            })?;
        let mut stream = body.into_data_stream();
        let mut size = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "attachment upload failed"))?;
            size = size.checked_add(chunk.len() as u64).ok_or_else(|| {
                error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "attachment exceeds configured limit",
                )
            })?;
            if size > max_bytes {
                return Err(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "attachment exceeds configured limit",
                ));
            }
            file.write_all(&chunk).await.map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not write attachment",
                )
            })?;
        }
        if size == 0 {
            return Err(error(StatusCode::BAD_REQUEST, "empty attachment"));
        }
        file.flush().await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not flush attachment",
            )
        })?;
        Ok(size)
    }
    .await;
    match result {
        Ok(_) if !path.exists() => Err(error(StatusCode::CONFLICT, "session ended during attachment upload")),
        Ok(size) => Ok(AttachmentReceipt {
            path: path.to_string_lossy().into_owned(),
            size,
            notice: ego.then(|| "This connection has no launch-time attachment read grant. Confined ACP runs require an existing user/profile read root covering this directory.".into()),
        }),
        Err(problem) => {
            let _ = tokio::fs::remove_file(&path).await;
            Err(problem)
        }
    }
}

#[cfg(test)]
mod tests;
