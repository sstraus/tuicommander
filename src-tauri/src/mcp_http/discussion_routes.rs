//! HTTP face of Discussion; same operations and shapes as the Tauri commands.
//! Questions and answers are journal entries: listing, reading and deleting
//! them go through the Progress routes.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;

use crate::AppState;
use crate::discussion::service;

#[derive(Deserialize)]
pub(super) struct QuestionBody {
    question: String,
}

/// The journal's own rule (`progress_auth`): loopback or an authenticated caller.
/// A side question starts a process, so it never runs for an anonymous remote.
fn guard(
    addr: &SocketAddr,
    auth: &Option<Extension<super::guards::Authenticated>>,
) -> Option<Response> {
    super::guards::require_local_or_auth(addr, auth.is_some())
        .err()
        .map(IntoResponse::into_response)
}

/// Refusals (no binding, empty question, limit reached, nothing to cancel) are
/// shown to the user as plain messages.
fn shown_to_user(_: &str) -> bool {
    true
}

pub(super) async fn ask(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<QuestionBody>,
) -> Response {
    if let Some(denied) = guard(&addr, &auth) {
        return denied;
    }
    super::user_error_result(
        service::ask(state, session_id, body.question).await,
        shown_to_user,
    )
}

pub(super) async fn get(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    Path(entry_id): Path<i64>,
) -> Response {
    if let Some(denied) = guard(&addr, &auth) {
        return denied;
    }
    super::user_error_result(service::get(entry_id).await, shown_to_user)
}

pub(super) async fn follow_up(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    State(state): State<Arc<AppState>>,
    Path(entry_id): Path<i64>,
    Json(body): Json<QuestionBody>,
) -> Response {
    if let Some(denied) = guard(&addr, &auth) {
        return denied;
    }
    super::user_error_result(
        service::follow_up(state, entry_id, body.question).await,
        shown_to_user,
    )
}

pub(super) async fn cancel(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    Path(entry_id): Path<i64>,
) -> Response {
    if let Some(denied) = guard(&addr, &auth) {
        return denied;
    }
    super::user_error_result(service::cancel(entry_id).await, shown_to_user)
}
