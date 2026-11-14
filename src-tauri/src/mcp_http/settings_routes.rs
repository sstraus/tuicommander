//! HTTP retains the existing local-or-authenticated Settings access policy.
use super::guards::{Authenticated, require_local_or_auth};
use crate::{
    AppState,
    settings::{api, policy::Caller},
};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, State},
    response::{IntoResponse, Response},
};
use std::{net::SocketAddr, sync::Arc};

pub(super) async fn settings_action(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<serde_json::Value>,
) -> Response {
    if let Err(response) = require_local_or_auth(&addr, auth.is_some()) {
        return response.into_response();
    }
    // A browser may replay its proposal on another TCP connection from the same client.
    Json(api::execute(&state, request, Caller::http(format!("http:{}", addr.ip()))).await)
        .into_response()
}
