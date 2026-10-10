//! The HTTP half of the ACP client surface, one route per `acp_*` command.
//!
//! Every handler reads its arguments and calls the same `acp_commands` core the
//! Tauri command calls, so the two transports cannot answer differently. No
//! decision lives here: what is refused, what is unavailable, and what a
//! settlement means are the client's judgements, made once in `crate::acp`.
//!
//! The error body is the serialized [`AcpClientError`] itself rather than this
//! module's own shape, for the same reason: a browser reading `code` and
//! `retryable` reads exactly what the desktop reads. The HTTP status is a
//! translation of that code, not a second opinion about it.
//!
//! Like the AI token bridges next door, the per-connection event stream is a
//! dedicated WebSocket rather than an `AppEvent`: one turn emits chunks far
//! faster than the 256-entry global bus can carry without lagging unrelated
//! subscribers, and those chunks belong to exactly one connection.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::v1;
use axum::Json;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedReceiver;

use super::guards::{Authenticated, require_local_or_auth};
use crate::AppState;
use crate::acp::{
    AcpAttachKind, AcpClientError, AcpClientErrorCode, AcpConnectionId, AcpDetachKind,
    AcpHostRequestId, AcpSessionAuthority, AcpStreamFrame, AcpTurnId, EgoHoldRequest,
};
use crate::acp_commands;

/// Sub-router mounted at `/acp`, merged by both the desktop and the remote
/// router so a phone drives ego exactly as the desktop app does.
pub(super) fn acp_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/workspace", get(workspace_root))
        .route("/chat/open", post(chat_open))
        .route("/connections", post(connect))
        .route(
            "/connections/{connection_id}",
            get(connection_snapshot).delete(disconnect),
        )
        .route("/connections/{connection_id}/kill", post(kill))
        .route("/connections/{connection_id}/reconnect", post(reconnect))
        .route("/connections/{connection_id}/stream", get(stream_ws))
        .route(
            "/connections/{connection_id}/sessions",
            get(session_list).post(session_new),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}",
            delete(session_delete),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/load",
            post(session_load),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/resume",
            post(session_resume),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/fork",
            post(session_fork),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/close",
            post(session_close),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/prompt",
            post(session_prompt).layer(axum::extract::DefaultBodyLimit::max(
                super::acp_prompt_body_limit(),
            )),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/cancel",
            post(session_cancel),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/queue/{turn_id}",
            delete(queued_prompt_cancel),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/config",
            post(session_set_config_option),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/pause",
            post(turn_pause),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/resume-turn",
            post(turn_resume),
        )
        .route(
            "/connections/{connection_id}/sessions/{session_id}/compact",
            post(session_compact),
        )
        .route(
            "/connections/{connection_id}/interactions",
            get(pending_interactions),
        )
        .route(
            "/connections/{connection_id}/permissions/{request_id}/response",
            post(respond_permission),
        )
        .route(
            "/connections/{connection_id}/elicitations/{request_id}/response",
            post(respond_elicitation),
        )
        .route("/one-shot", post(one_shot_prompt))
}

/// The HTTP status that carries an ACP client error code.
///
/// A translation, never a re-judgement: the code in the body stays the truth,
/// and the status only tells a plain HTTP client which half of the exchange is
/// at fault. `CapabilityUnavailable` is 501 because the agent, not the caller,
/// is the one that cannot do it; a settled connection and a lost stream are
/// both 410 because the resource named in the URL is genuinely gone.
///
/// Exhaustive rather than defaulted: 502 is the right answer for every code
/// that names the agent as the failing party, but it is the wrong answer to
/// reach by accident, and a catch-all would hand it to the next code somebody
/// adds without anyone deciding that is what a browser should see.
fn status_for(code: AcpClientErrorCode) -> StatusCode {
    match code {
        AcpClientErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
        AcpClientErrorCode::NotFound => StatusCode::NOT_FOUND,
        AcpClientErrorCode::CapabilityUnavailable => StatusCode::NOT_IMPLEMENTED,
        AcpClientErrorCode::TransportClosed | AcpClientErrorCode::StreamGap => StatusCode::GONE,
        AcpClientErrorCode::AgentError
        | AcpClientErrorCode::ProtocolViolation
        | AcpClientErrorCode::InitializationFailed
        | AcpClientErrorCode::UnsupportedProtocol => StatusCode::BAD_GATEWAY,
    }
}

/// Render an error with the status its code maps to and the error itself as the
/// body, so the desktop and the browser read the same object.
fn failed(error: AcpClientError) -> Response {
    (status_for(error.code), Json(error)).into_response()
}

/// Render whatever a manager method returned. `Ok(())` becomes `null`, which is
/// what the Tauri command returns for the same call.
fn answer<T: Serialize>(result: Result<T, AcpClientError>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => failed(error),
    }
}

// ---------------------------------------------------------------------------
// Request bodies. Camel-cased to match the Tauri argument names one-for-one.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootBody {
    root: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OneShotBody {
    root: PathBuf,
    prompt: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorityBody {
    authority: AcpSessionAuthority,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptBody {
    prompt: Vec<v1::ContentBlock>,
    /// The repository on screen, a hint for this turn only.
    #[serde(default)]
    viewed_repo: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBody {
    config_id: v1::SessionConfigId,
    value: v1::SessionConfigOptionValue,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestIdBody {
    request_id: uuid::Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutcomeBody {
    outcome: v1::RequestPermissionOutcome,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActionBody {
    action: v1::ElicitationAction,
}

#[derive(Deserialize)]
struct ListQuery {
    cwd: Option<PathBuf>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct StreamQuery {
    after: Option<u64>,
}

// ---------------------------------------------------------------------------
// Connection lifecycle.
// ---------------------------------------------------------------------------

/// Launching the configured ego binary is the one authority-bearing act on this
/// surface, so `connect` and `reconnect` — and only they — take the spawn guard.
/// Every other route needs a connection id that one of these two handed out.
async fn connect(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<axum::Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RootBody>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    answer(acp_commands::connect(&state, body.root).await)
}

async fn reconnect(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<axum::Extension<Authenticated>>,
    Path(connection_id): Path<AcpConnectionId>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RootBody>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    answer(acp_commands::reconnect(&state, connection_id, body.root).await)
}

/// The third route that launches ego, so it takes the same spawn guard.
///
/// It needs no connection id because it makes its own and shuts it down again,
/// which is exactly why the guard belongs here: without it, anything that can
/// reach this port could start an ego process per request.
async fn one_shot_prompt(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<axum::Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<OneShotBody>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    answer(crate::acp::oneshot::run_prompt(&state, body.root, body.prompt).await)
}

async fn workspace_root(State(state): State<Arc<AppState>>) -> Response {
    answer(acp_commands::workspace_root(&state).await)
}

async fn connection_snapshot(
    Path(connection_id): Path<AcpConnectionId>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(state.acp.snapshot(connection_id))
}

async fn disconnect(
    Path(connection_id): Path<AcpConnectionId>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(state.acp.disconnect(connection_id).await)
}

async fn kill(
    Path(connection_id): Path<AcpConnectionId>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(state.acp.kill(connection_id).await)
}

// ---------------------------------------------------------------------------
// Sessions.
// ---------------------------------------------------------------------------

async fn session_new(
    Path(connection_id): Path<AcpConnectionId>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthorityBody>,
) -> Response {
    answer(crate::acp_commands::session_new(&state, connection_id, body.authority).await)
}

async fn session_list(
    Path(connection_id): Path<AcpConnectionId>,
    Query(query): Query<ListQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let mut request = v1::ListSessionsRequest::new();
    request.cwd = query.cwd;
    request.cursor = query.cursor;
    answer(
        state
            .acp
            .list_sessions_for_display(connection_id, request)
            .await,
    )
}

/// The three ways to attach differ only in the kind, so they share one body.
async fn attach(
    state: &AppState,
    kind: AcpAttachKind,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
) -> Response {
    answer(
        state
            .acp
            .attach(connection_id, kind, session_id, authority)
            .await,
    )
}

async fn session_load(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthorityBody>,
) -> Response {
    attach(
        &state,
        AcpAttachKind::Load,
        connection_id,
        session_id,
        body.authority,
    )
    .await
}

async fn session_resume(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthorityBody>,
) -> Response {
    attach(
        &state,
        AcpAttachKind::Resume,
        connection_id,
        session_id,
        body.authority,
    )
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ForkBody {
    authority: AcpSessionAuthority,
    at_message_id: Option<String>,
}

async fn session_fork(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ForkBody>,
) -> Response {
    answer(
        crate::acp_chat::fork(
            &state,
            connection_id,
            session_id,
            body.authority,
            body.at_message_id,
        )
        .await,
    )
}

async fn session_delete(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(
        state
            .acp
            .detach(connection_id, AcpDetachKind::Delete, session_id)
            .await,
    )
}

async fn session_close(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(
        state
            .acp
            .detach(connection_id, AcpDetachKind::Close, session_id)
            .await,
    )
}

// ---------------------------------------------------------------------------
// Turns.
// ---------------------------------------------------------------------------

async fn session_prompt(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PromptBody>,
) -> Response {
    answer(
        state
            .acp
            .prompt_with_context(connection_id, session_id, body.prompt, body.viewed_repo)
            .await,
    )
}

async fn session_cancel(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(state.acp.cancel(connection_id, session_id).await)
}

async fn queued_prompt_cancel(
    Path((connection_id, session_id, turn_id)): Path<(AcpConnectionId, v1::SessionId, AcpTurnId)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(
        state
            .acp
            .cancel_queued(connection_id, session_id, turn_id)
            .await,
    )
}

async fn session_set_config_option(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConfigBody>,
) -> Response {
    answer(
        state
            .acp
            .set_config_option(
                connection_id,
                v1::SetSessionConfigOptionRequest::new(session_id, body.config_id, body.value),
            )
            .await,
    )
}

async fn turn_pause(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RequestIdBody>,
) -> Response {
    answer(
        state
            .acp
            .pause_turn(
                connection_id,
                EgoHoldRequest {
                    session_id,
                    request_id: body.request_id,
                },
            )
            .await,
    )
}

async fn turn_resume(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RequestIdBody>,
) -> Response {
    answer(
        state
            .acp
            .resume_turn(
                connection_id,
                EgoHoldRequest {
                    session_id,
                    request_id: body.request_id,
                },
            )
            .await,
    )
}

async fn session_compact(
    Path((connection_id, session_id)): Path<(AcpConnectionId, v1::SessionId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RequestIdBody>,
) -> Response {
    answer(crate::acp_chat::compact(&state, connection_id, session_id, body.request_id).await)
}

// ---------------------------------------------------------------------------
// Interactions the agent is waiting on.
// ---------------------------------------------------------------------------

async fn pending_interactions(
    Path(connection_id): Path<AcpConnectionId>,
    State(state): State<Arc<AppState>>,
) -> Response {
    answer(state.acp.pending_interactions(connection_id).await)
}

async fn respond_permission(
    Path((connection_id, request_id)): Path<(AcpConnectionId, AcpHostRequestId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<OutcomeBody>,
) -> Response {
    answer(
        state
            .acp
            .respond_permission(connection_id, request_id, body.outcome)
            .await,
    )
}

async fn respond_elicitation(
    Path((connection_id, request_id)): Path<(AcpConnectionId, AcpHostRequestId)>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActionBody>,
) -> Response {
    answer(
        state
            .acp
            .respond_elicitation(connection_id, request_id, body.action)
            .await,
    )
}

// ---------------------------------------------------------------------------
// The event stream.
// ---------------------------------------------------------------------------

/// Subscribe to one connection's frames, resuming after `?after=<sequence>`.
///
/// The subscription is taken *before* the upgrade so an unknown connection or a
/// sequence that has already fallen out of the buffer is an honest HTTP error
/// rather than a socket that opens and immediately shuts.
async fn stream_ws(
    ws: WebSocketUpgrade,
    Path(connection_id): Path<AcpConnectionId>,
    Query(query): Query<StreamQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let (frames_tx, frames_rx) = tokio::sync::mpsc::unbounded_channel();
    let events = match state.acp.subscribe(connection_id, query.after.unwrap_or(0)) {
        Ok(events) => events,
        Err(error) => return failed(error),
    };
    tokio::spawn(acp_commands::forward(events, move |frame| {
        frames_tx.send(frame).is_ok()
    }));
    ws.on_upgrade(move |socket| pump(socket, frames_rx))
}

/// Forward frames to the socket until the stream ends or the client leaves.
///
/// Nothing the client sends means anything here — the whole conversation with
/// the agent goes through the request/response routes — so an inbound message
/// is read only to notice a close.
async fn pump(socket: WebSocket, mut frames: UnboundedReceiver<AcpStreamFrame>) {
    let (mut sender, mut receiver) = socket.split();
    loop {
        tokio::select! {
            frame = frames.recv() => {
                let Some(frame) = frame else { break };
                // A gap and an end are both terminal: the producer stops after
                // either, so holding the socket open would only look alive.
                let last = !matches!(frame, AcpStreamFrame::Event(_));
                let Ok(json) = serde_json::to_string(&frame) else {
                    tracing::warn!("acp_routes: failed to serialize a stream frame");
                    break;
                };
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
                if last {
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(_)) => continue,
                    _ => break,
                }
            }
        }
    }
    let _ = sender.close().await;
}

/// Conversation-specific executable authority uses the same spawn guard as connect.
async fn chat_open(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<axum::Extension<Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::acp_chat::ChatOpenRequest>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    answer(crate::acp_chat::open(&state, request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::Request;
    use tower::ServiceExt;

    const CID: &str = "01932d5e-0000-7000-8000-0000000000aa";
    const SID: &str = "01932d5e-0000-7000-8000-0000000000bb";
    const RID: &str = "01932d5e-0000-7000-8000-0000000000cc";

    /// What a plain HTTP client is told, for every code there is.
    ///
    /// Written out rather than read back from `status_for`, which would pass
    /// for whatever that function happened to answer. The point of pinning all
    /// nine is the four that share 502: they are the codes that name the agent
    /// as the failing party, and a caller reading only the status must not be
    /// able to mistake one of them for something it did wrong.
    #[test]
    fn every_error_code_carries_the_status_its_half_of_the_exchange_deserves() {
        for (code, status) in [
            (AcpClientErrorCode::InvalidInput, 400),
            (AcpClientErrorCode::NotFound, 404),
            (AcpClientErrorCode::TransportClosed, 410),
            (AcpClientErrorCode::StreamGap, 410),
            (AcpClientErrorCode::CapabilityUnavailable, 501),
            (AcpClientErrorCode::AgentError, 502),
            (AcpClientErrorCode::ProtocolViolation, 502),
            (AcpClientErrorCode::InitializationFailed, 502),
            (AcpClientErrorCode::UnsupportedProtocol, 502),
        ] {
            assert_eq!(status_for(code).as_u16(), status, "{code:?}");
        }
    }

    /// No `mcpServers`: the field is synthesised server-side and a body that
    /// names one is refused, so sending it here would test the refusal on every
    /// row instead of the route.
    fn authority(cwd: &str) -> serde_json::Value {
        serde_json::json!({
            "authority": {"cwd": cwd, "additionalDirectories": []}
        })
    }

    /// Every route, with the method and body `transport.ts` actually sends.
    ///
    /// The expected code is the point of the table, not decoration: nothing here
    /// is connected, so a route that answers `notFound` proves the request was
    /// matched, its path parameters parsed, and its body deserialized into the
    /// call the Tauri command makes — the parts a browser could silently get
    /// wrong. `connect` and `reconnect` stop earlier, on the unset executable,
    /// because process authority is checked before anything is looked up.
    fn table(
        cwd: &str,
    ) -> Vec<(
        &'static str,
        String,
        Option<serde_json::Value>,
        AcpClientErrorCode,
    )> {
        let session = format!("/acp/connections/{CID}/sessions/{SID}");
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("cwd", cwd)
            .append_pair("cursor", "next")
            .finish();
        vec![
            (
                "POST",
                "/acp/connections".to_string(),
                Some(serde_json::json!({"root": cwd})),
                AcpClientErrorCode::InvalidInput,
            ),
            (
                "POST",
                format!("/acp/connections/{CID}/reconnect"),
                Some(serde_json::json!({"root": cwd})),
                AcpClientErrorCode::InvalidInput,
            ),
            (
                "GET",
                format!("/acp/connections/{CID}"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "DELETE",
                format!("/acp/connections/{CID}"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("/acp/connections/{CID}/kill"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("/acp/connections/{CID}/sessions"),
                Some(authority(cwd)),
                AcpClientErrorCode::NotFound,
            ),
            (
                "GET",
                format!("/acp/connections/{CID}/sessions?{query}"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/load"),
                Some(authority(cwd)),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/resume"),
                Some(authority(cwd)),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/fork"),
                Some(authority(cwd)),
                AcpClientErrorCode::NotFound,
            ),
            (
                "DELETE",
                session.clone(),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/close"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/prompt"),
                Some(serde_json::json!({"prompt": [{"type": "text", "text": "hi"}]})),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/cancel"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/config"),
                Some(serde_json::json!({"configId": "model", "value": {"value": "opus"}})),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/pause"),
                Some(serde_json::json!({"requestId": RID})),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/resume-turn"),
                Some(serde_json::json!({"requestId": RID})),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("{session}/compact"),
                Some(serde_json::json!({"requestId": RID})),
                AcpClientErrorCode::NotFound,
            ),
            (
                "GET",
                format!("/acp/connections/{CID}/interactions"),
                None,
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("/acp/connections/{CID}/permissions/{RID}/response"),
                Some(serde_json::json!({"outcome": {"outcome": "cancelled"}})),
                AcpClientErrorCode::NotFound,
            ),
            (
                "POST",
                format!("/acp/connections/{CID}/elicitations/{RID}/response"),
                Some(serde_json::json!({"action": {"action": "decline"}})),
                AcpClientErrorCode::NotFound,
            ),
            // Launches its own ego, so with none configured it fails the same
            // way `connect` does rather than for want of a connection.
            (
                "POST",
                "/acp/one-shot".to_string(),
                Some(serde_json::json!({"root": cwd, "prompt": "hi"})),
                AcpClientErrorCode::InvalidInput,
            ),
        ]
    }

    fn request(method: &str, path: &str, body: Option<serde_json::Value>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        let body = match body {
            Some(json) => {
                builder = builder.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&json).unwrap())
            }
            None => Body::empty(),
        };
        let mut req = builder.body(body).unwrap();
        // Loopback, so the spawn guard on connect/reconnect passes exactly as it
        // does for the desktop app talking to its own server.
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        req
    }

    #[tokio::test]
    async fn every_acp_route_answers_its_own_command_with_an_acp_error_body() {
        let state = super::super::tests::test_state();
        let app = super::super::shared_routes().with_state(state);
        // A rooted Unix literal is not an absolute Windows session authority.
        let cwd = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        for (method, path, body, expected) in table(cwd.path().to_str().unwrap()) {
            let resp = app
                .clone()
                .oneshot(request(method, &path, body))
                .await
                .unwrap();
            let status = resp.status();
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let error: AcpClientError = serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                panic!(
                    "{method} {path} did not answer with an AcpClientError ({status}): {err}: {}",
                    String::from_utf8_lossy(&bytes)
                )
            });
            assert_eq!(error.code, expected, "{method} {path}");
            // Pinned literally rather than through `status_for`: a status
            // derived from the function under test would agree with any
            // mapping it grew.
            let wanted = match expected {
                AcpClientErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                AcpClientErrorCode::NotFound => StatusCode::NOT_FOUND,
                other => panic!("this table has no status pinned for {other:?}"),
            };
            assert_eq!(status, wanted, "{method} {path}");
        }
    }

    /// Catches: the browser transport answering with a client-composed root, or
    /// hiding the refusal that names the setting.
    #[tokio::test]
    async fn the_workspace_route_answers_from_the_hosts_setting() {
        let dir = tempfile::tempdir().unwrap();
        let state = super::super::tests::test_state();
        let app = super::super::shared_routes().with_state(state.clone());

        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        let resp = app
            .clone()
            .oneshot(request("GET", "/acp/workspace", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let root: String = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            std::path::PathBuf::from(root),
            std::fs::canonicalize(dir.path()).unwrap()
        );

        state.config.write().ai_chat_workspace = "relative".to_string();
        let resp = app
            .oneshot(request("GET", "/acp/workspace", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: AcpClientError = serde_json::from_slice(&bytes).unwrap();
        assert!(error.message.contains("ai_chat_workspace"));
    }

    /// A body that names an MCP server is refused, not quietly stripped.
    ///
    /// The session routes carry no spawn guard and a browser can reach them, so
    /// a caller able to add a server could point ego at any HTTP endpoint it
    /// liked. Refusing is the whole reason `AcpSessionAuthority` denies unknown
    /// fields; stripping would let a caller believe it had widened the authority.
    #[tokio::test]
    async fn a_session_body_cannot_name_an_mcp_server() {
        let state = super::super::tests::test_state();
        let app = super::super::shared_routes().with_state(state);
        let body = serde_json::json!({
            "authority": {
                "cwd": "/tmp",
                "additionalDirectories": [],
                "mcpServers": [{"name": "elsewhere", "url": "http://evil.invalid/mcp"}],
            }
        });

        let resp = app
            .oneshot(request(
                "POST",
                &format!("/acp/connections/{CID}/sessions"),
                Some(body),
            ))
            .await
            .unwrap();

        // Not `notFound`: the body is rejected before the unknown connection is
        // ever looked up, which is what proves the field never reaches the call.
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// The stream is a WebSocket and only a WebSocket, and a subscription it
    /// cannot honour is refused before the socket opens.
    ///
    /// The second half is the one that matters: a client that got `101` and
    /// then an immediate close would have to guess why, so an unknown
    /// connection has to come back as the same `notFound` every other route
    /// gives, with the same body.
    #[tokio::test]
    async fn the_stream_route_is_a_websocket_that_refuses_before_upgrading() {
        let state = super::super::tests::test_state();
        let app = super::super::shared_routes().with_state(state);
        let path = format!("/acp/connections/{CID}/stream?after=7");

        let plain = app
            .clone()
            .oneshot(request("GET", &path, None))
            .await
            .unwrap();
        assert_eq!(plain.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(plain.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(
            serde_json::from_slice::<AcpClientError>(&bytes).is_err(),
            "a request that never asked to upgrade must be refused by the upgrade \
             extractor, not answered as if it were an ACP call: {}",
            String::from_utf8_lossy(&bytes)
        );

        // A real handshake needs a real connection — the upgrade extractor wants
        // the hyper machinery a one-shot call does not have — so this half runs
        // against a server on a port, the way a browser would reach it.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let refusal = tokio_tungstenite::connect_async(format!("ws://{addr}{path}"))
            .await
            .expect_err("an unknown connection has no stream to open");
        let tokio_tungstenite::tungstenite::Error::Http(resp) = refusal else {
            panic!("the handshake failed for the wrong reason: {refusal}");
        };
        assert_eq!(resp.status().as_u16(), 404);
        let body = resp
            .body()
            .as_ref()
            .expect("a refused handshake still says why");
        let error: AcpClientError = serde_json::from_slice(body).unwrap();
        assert_eq!(error.code, AcpClientErrorCode::NotFound);
    }

    #[test]
    fn a_lost_stream_and_a_settled_connection_are_both_gone_not_a_server_fault() {
        assert_eq!(
            status_for(AcpClientErrorCode::StreamGap),
            StatusCode::GONE,
            "a subscriber that missed frames must not be told to retry the same URL"
        );
        assert_eq!(
            status_for(AcpClientErrorCode::TransportClosed),
            StatusCode::GONE
        );
        assert_eq!(
            status_for(AcpClientErrorCode::CapabilityUnavailable),
            StatusCode::NOT_IMPLEMENTED,
            "an agent that never advertised an operation is not the caller's mistake"
        );
        assert_eq!(
            status_for(AcpClientErrorCode::AgentError),
            StatusCode::BAD_GATEWAY
        );
    }
}
