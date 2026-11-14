//! The IPC surface of the ACP client: one command per manager method.
//!
//! Thin on purpose. Every command reads its arguments, calls exactly one
//! manager method, and returns what that method returned. Nothing here decides
//! whether an operation is available, validates a permission option, or invents
//! a fallback — those are the client's judgements and they must be identical
//! whether a person is on the desktop app or on a phone over HTTP.
//!
//! The one thing this layer does own is process authority. `connect` and
//! `reconnect` read the ego executable from configuration and pass it down; no
//! argument on these shared connections names a binary. Conversation-specific
//! launch authority lives in `acp_chat` and uses a dedicated peer.

use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::v1;
#[cfg(feature = "desktop")]
use tauri::State;

use crate::AppState;
use crate::acp::{
    AcpAttachKind, AcpAttachmentSnapshot, AcpClientError, AcpConnectRequest, AcpConnectionId,
    AcpConnectionSettlement, AcpConnectionSnapshot, AcpDetachKind, AcpHostRequestId,
    AcpInteractionSettlement, AcpPendingInteraction, AcpReconnectRequest, AcpSessionAuthority,
    AcpTurnId, EgoAcpConfig, EgoCompactResponse, EgoHoldRequest, EgoHoldResponse,
};
#[cfg(feature = "desktop")]
use crate::acp::{AcpEventStream, AcpStreamFrame};

/// The one executable this host may launch for ACP, as configured right now.
///
/// Read per call rather than remembered, so a person who corrects the setting
/// does not have to restart the app to have the correction take effect. An
/// empty setting is refused here rather than deep in a launch failure, because
/// "not configured" and "configured wrongly" are different things to be told.
pub(crate) fn ego_config(state: &AppState) -> Result<EgoAcpConfig, AcpClientError> {
    let settings = state.config.read();
    let executable = settings.ego_executable.clone();
    if executable.trim().is_empty() {
        return Err(AcpClientError::invalid_input(
            "no ego executable is configured; set it before connecting over ACP",
        ));
    }
    Ok(EgoAcpConfig {
        executable: PathBuf::from(executable),
        profile: settings.ego_profile.clone(),
        // AI Chat instructions belong to its conversation-specific ChatLaunch snapshot.
        master_prompt: String::new(),
        ai_chat: false,
    })
}

/// The directory every AI Chat conversation runs in, from the
/// `ai_chat_workspace` setting; empty means the home directory of this host.
///
/// Resolved here so the desktop and a browser get the same answer for the host
/// that runs ego. A configured folder that does not exist is created; every
/// refusal names the setting, because the person has to edit it to recover.
pub(crate) async fn workspace_root(state: &AppState) -> Result<PathBuf, AcpClientError> {
    let configured = state.config.read().ai_chat_workspace.trim().to_string();
    let unusable = |reason: String| {
        AcpClientError::invalid_input(format!(
            "AI Chat workspace (setting ai_chat_workspace) is unusable: {reason}"
        ))
    };
    let path = if configured.is_empty() {
        dirs::home_dir().ok_or_else(|| unusable("the home directory is unavailable".into()))?
    } else {
        PathBuf::from(&configured)
    };
    if !path.is_absolute() {
        return Err(unusable(format!(
            "{} is not an absolute path",
            path.display()
        )));
    }
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|error| unusable(format!("cannot create {}: {error}", path.display())))?;
    tokio::fs::canonicalize(&path)
        .await
        .map_err(|error| unusable(format!("cannot open {}: {error}", path.display())))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_workspace_root(
    state: State<'_, Arc<AppState>>,
) -> Result<PathBuf, AcpClientError> {
    workspace_root(&state).await
}

pub(crate) async fn connect(
    state: &Arc<AppState>,
    request: AcpConnectRequest,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    let config = ego_config(state)?;
    let peer_id = peer_id_for_root(state, &request.root).await?;
    let snapshot = state
        .acp
        .connect_with_peer(&config, request, peer_id.clone())
        .await?;
    label_peer(state, &peer_id);
    Ok(snapshot)
}

pub(crate) async fn reconnect(
    state: &Arc<AppState>,
    connection_id: AcpConnectionId,
    root: PathBuf,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    let config = ego_config(state)?;
    let peer_id = peer_id_for_root(state, &root).await?;
    let snapshot = state
        .acp
        .reconnect_with_peer(
            &config,
            AcpReconnectRequest {
                connection_id,
                root,
            },
            Some(peer_id.clone()),
        )
        .await?;
    label_peer(state, &peer_id);
    Ok(snapshot)
}

fn label_peer(state: &AppState, peer_id: &str) {
    if let Some(mut peer) = state.peer_agents.get_mut(peer_id) {
        peer.name = "ego".to_string();
        peer.project = state
            .acp
            .peer_root(peer_id)
            .map(|root| root.to_string_lossy().into_owned());
    }
}

/// Persist the host-issued peer address before ego starts, so reconnect and an
/// app restart hand the same address to both ego and its MCP bridge.
async fn peer_id_for_root(
    state: &Arc<AppState>,
    root: &std::path::Path,
) -> Result<String, AcpClientError> {
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(|error| AcpClientError::invalid_input(format!("invalid ACP root: {error}")))?;
    if !tokio::fs::metadata(&root)
        .await
        .map_err(|error| AcpClientError::invalid_input(format!("invalid ACP root: {error}")))?
        .is_dir()
    {
        return Err(AcpClientError::invalid_input(
            "ACP root must be a directory",
        ));
    }
    let root = root
        .to_str()
        .ok_or_else(|| AcpClientError::invalid_input("ACP root must be valid UTF-8"))?
        .to_string();
    if let Some(id) = state
        .config
        .read()
        .ai_chat_peer_ids
        .get(&root)
        .filter(|id| crate::acp::valid_peer_id(id))
        .cloned()
    {
        return Ok(id);
    }
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        let mut selected = String::new();
        crate::config::commit_config_change(&state, |current| {
            let mut next = current.clone();
            selected = next
                .ai_chat_peer_ids
                .get(&root)
                .filter(|id| crate::acp::valid_peer_id(id))
                .cloned()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            next.ai_chat_peer_ids.insert(root, selected.clone());
            Ok(next)
        })
        .map_err(|error| {
            AcpClientError::invalid_input(format!("cannot persist ACP peer identity: {error}"))
        })?;
        Ok(selected)
    })
    .await
    .map_err(|error| {
        AcpClientError::invalid_input(format!("cannot prepare ACP peer identity: {error}"))
    })?
}

/// Forward one connection's frames to a desktop Channel until the stream ends.
///
/// The frames are what both transports carry: the desktop Channel and the
/// browser WebSocket send the same JSON, so a host written against one is
/// written against the other. A gap is a frame rather than a dropped
/// connection, because the subscriber has to learn that it missed something.
///
/// Spawned on the Tauri runtime, which any thread can reach: a sync Tauri
/// command runs on the main thread, where `tokio::spawn` panics and takes every
/// PTY session down with the app.
#[cfg(feature = "desktop")]
fn forward_detached(
    mut events: AcpEventStream,
    mut send: impl FnMut(AcpStreamFrame) -> bool + Send + 'static,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let frame = events.next_frame().await;
            let last = !matches!(frame, AcpStreamFrame::Event(_));
            if !send(frame) || last {
                return;
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tauri commands. One per manager method, in the order the plan lists them.
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_connect(
    state: State<'_, Arc<AppState>>,
    root: PathBuf,
    env: Option<std::collections::BTreeMap<String, String>>,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    connect(&state, AcpConnectRequest { root, env }).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_reconnect(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    root: PathBuf,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    reconnect(&state, connection_id, root).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_disconnect(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<AcpConnectionSettlement, AcpClientError> {
    state.acp.disconnect(connection_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_kill(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<AcpConnectionSettlement, AcpClientError> {
    state.acp.kill(connection_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_connection_snapshot(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<AcpConnectionSnapshot, AcpClientError> {
    state.acp.snapshot(connection_id)
}

/// The dedicated-Channel exception, matched by a dedicated WebSocket.
///
/// Not an `AppEvent`: a turn's chunks are high-frequency and belong to one
/// connection, and putting them on the global broadcast would either drown the
/// 256-entry bus or make every other subscriber pay to filter them out.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_subscribe(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    after_sequence: u64,
    channel: tauri::ipc::Channel<AcpStreamFrame>,
) -> Result<(), AcpClientError> {
    let events = state.acp.subscribe(connection_id, after_sequence)?;
    forward_detached(events, move |frame| channel.send(frame).is_ok());
    Ok(())
}

pub(crate) async fn session_new(
    state: &AppState,
    connection_id: AcpConnectionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    let machine_profile = state.config.read().ego_profile.clone();
    state
        .acp
        .new_session_for_repo(connection_id, authority, &machine_profile)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_new(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    session_new(&state, connection_id, authority).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_list(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    cwd: Option<PathBuf>,
    cursor: Option<String>,
) -> Result<v1::ListSessionsResponse, AcpClientError> {
    let mut request = v1::ListSessionsRequest::new();
    request.cwd = cwd;
    request.cursor = cursor;
    state
        .acp
        .list_sessions_for_display(connection_id, request)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_load(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    state
        .acp
        .attach(connection_id, AcpAttachKind::Load, session_id, authority)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_resume(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    state
        .acp
        .attach(connection_id, AcpAttachKind::Resume, session_id, authority)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_fork(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    authority: AcpSessionAuthority,
    at_message_id: Option<String>,
) -> Result<AcpAttachmentSnapshot, AcpClientError> {
    crate::acp_chat::fork(&state, connection_id, session_id, authority, at_message_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_delete(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
) -> Result<(), AcpClientError> {
    state
        .acp
        .detach(connection_id, AcpDetachKind::Delete, session_id)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_close(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
) -> Result<(), AcpClientError> {
    state
        .acp
        .detach(connection_id, AcpDetachKind::Close, session_id)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_session_set_viewed_repo(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    viewed_repo: Option<String>,
) -> Result<(), AcpClientError> {
    state
        .acp
        .set_viewed_repo(connection_id, session_id, viewed_repo)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_prompt(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    prompt: Vec<v1::ContentBlock>,
    viewed_repo: Option<String>,
) -> Result<AcpTurnId, AcpClientError> {
    state
        .acp
        .prompt_with_context(connection_id, session_id, prompt, viewed_repo)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_cancel(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
) -> Result<(), AcpClientError> {
    state.acp.cancel(connection_id, session_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_queued_prompt_cancel(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    turn_id: AcpTurnId,
) -> Result<(), AcpClientError> {
    state
        .acp
        .cancel_queued(connection_id, session_id, turn_id)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_queued_prompt_recall(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    turn_id: AcpTurnId,
) -> Result<String, AcpClientError> {
    state
        .acp
        .recall_queued(connection_id, session_id, turn_id)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_set_config_option(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    config_id: v1::SessionConfigId,
    value: v1::SessionConfigOptionValue,
) -> Result<Vec<v1::SessionConfigOption>, AcpClientError> {
    state
        .acp
        .set_config_option(
            connection_id,
            v1::SetSessionConfigOptionRequest::new(session_id, config_id, value),
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_turn_pause(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<EgoHoldResponse, AcpClientError> {
    state
        .acp
        .pause_turn(
            connection_id,
            EgoHoldRequest {
                session_id,
                request_id,
            },
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_turn_resume(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<EgoHoldResponse, AcpClientError> {
    state
        .acp
        .resume_turn(
            connection_id,
            EgoHoldRequest {
                session_id,
                request_id,
            },
        )
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_session_compact(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<EgoCompactResponse, AcpClientError> {
    crate::acp_chat::compact(&state, connection_id, session_id, request_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_pending_interactions(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
) -> Result<Vec<AcpPendingInteraction>, AcpClientError> {
    state.acp.pending_interactions(connection_id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_respond_permission(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    request_id: AcpHostRequestId,
    outcome: v1::RequestPermissionOutcome,
) -> Result<AcpInteractionSettlement, AcpClientError> {
    state
        .acp
        .respond_permission(connection_id, request_id, outcome)
        .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_respond_elicitation(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    request_id: AcpHostRequestId,
    action: v1::ElicitationAction,
) -> Result<AcpInteractionSettlement, AcpClientError> {
    state
        .acp
        .respond_elicitation(connection_id, request_id, action)
        .await
}

/// One ego turn with nobody watching, for a Smart Prompt in `api` mode.
///
/// The odd one out on this surface: it takes no connection id because it owns
/// the whole lifetime — launch, one turn, shutdown. A caller cannot hand it a
/// connection the AI Chat panel is using, and it cannot leave one behind.
/// Everything it does is in [`crate::acp::oneshot`]; this is only the door.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_one_shot_prompt(
    state: State<'_, Arc<AppState>>,
    root: PathBuf,
    prompt: String,
) -> Result<crate::acp::oneshot::EgoTurn, AcpClientError> {
    crate::acp::oneshot::run_prompt(&state, root, prompt).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_one_shot_prepare(
    state: State<'_, Arc<AppState>>,
) -> Result<String, AcpClientError> {
    Ok(crate::acp::oneshot::prepare(state.inner()))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_one_shot_cancel(state: State<'_, Arc<AppState>>, request_id: String) -> bool {
    crate::acp::oneshot::cancel(&state, &request_id)
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use tokio::sync::broadcast;

    use super::*;
    use crate::acp::{AcpClientEvent, AcpEventJournal};

    fn state_with_workspace(value: &str) -> AppState {
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().ai_chat_workspace = value.to_string();
        state
    }

    /// Catches: connect refused on a machine without ~/Gits because the root is
    /// composed from a hardcoded folder name instead of configuration.
    #[tokio::test]
    async fn an_unset_workspace_is_the_home_directory_never_a_fixed_folder() {
        let state = state_with_workspace("");
        let home = std::fs::canonicalize(dirs::home_dir().unwrap()).unwrap();
        assert_eq!(workspace_root(&state).await.unwrap(), home);
    }

    /// Catches: the setting is read but ignored, so a user's chosen folder never applies.
    #[tokio::test]
    async fn a_configured_workspace_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_workspace(dir.path().to_str().unwrap());
        assert_eq!(
            workspace_root(&state).await.unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    /// Catches: a missing configured folder falls through to the generic
    /// "invalid ACP root" refusal at canonicalize.
    #[tokio::test]
    async fn a_missing_configured_workspace_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = dir.path().join("chat").join("root");
        let state = state_with_workspace(wanted.to_str().unwrap());
        let root = workspace_root(&state).await.unwrap();
        assert!(wanted.is_dir());
        assert_eq!(root, std::fs::canonicalize(&wanted).unwrap());
    }

    /// Catches: a bad setting is reported without naming it, leaving the user
    /// with no idea what to edit.
    #[tokio::test]
    async fn an_unusable_workspace_names_the_setting() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        let under_file = file.join("sub");
        for bad in [
            "relative/path".to_string(),
            file.to_string_lossy().into_owned(),
            under_file.to_string_lossy().into_owned(),
        ] {
            let state = state_with_workspace(&bad);
            let error = workspace_root(&state).await.unwrap_err();
            assert!(
                error.message.contains("ai_chat_workspace"),
                "{bad}: {}",
                error.message
            );
        }
    }

    #[tokio::test]
    async fn a_conversation_peer_survives_reconnect_and_app_restart() {
        let config_dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config_dir.path().to_path_buf());
        let root = tempfile::tempdir().unwrap();
        let other_root = tempfile::tempdir().unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let first = peer_id_for_root(&state, root.path()).await.unwrap();
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        assert_eq!(peer_id_for_root(&state, root.path()).await.unwrap(), first);
        assert_ne!(
            peer_id_for_root(&state, other_root.path()).await.unwrap(),
            first
        );

        let persisted = crate::config::load_app_config();
        let restarted = Arc::new(crate::state::tests_support::make_test_app_state());
        *restarted.config.write() = persisted;
        assert_eq!(
            peer_id_for_root(&restarted, root.path()).await.unwrap(),
            first
        );
        restarted.config.write().ai_chat_peer_ids.insert(
            root.path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            uuid::Uuid::parse_str(&first).unwrap().simple().to_string(),
        );
        let repaired = peer_id_for_root(&restarted, root.path()).await.unwrap();
        assert_eq!(repaired.len(), 36, "bridge headers require canonical UUIDs");
        assert_ne!(repaired, first);
    }

    /// Catches: native Fill panics when reservation expiry spawns on the IPC
    /// thread without an entered Tokio runtime. Exercise generated dispatch,
    /// not the helper inside a Tokio test or an HTTP handler.
    #[cfg(feature = "settings-ipc-test")]
    #[test]
    fn one_shot_prepare_dispatches_without_a_caller_tokio_runtime() {
        assert!(tokio::runtime::Handle::try_current().is_err());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .invoke_handler(tauri::generate_handler![super::acp_one_shot_prepare])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "prepare-test", Default::default())
            .build()
            .unwrap();
        let response = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "acp_one_shot_prepare".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({})),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_owned(),
            },
        )
        .expect("native preparation returns a handle rather than panicking");
        let id: String = response.deserialize().unwrap();
        assert!(crate::acp::oneshot::cancel(&state, &id));
        assert!(!crate::acp::oneshot::cancel(&state, &id));
    }

    /// `acp_subscribe` is a sync command, so Tauri runs it on the main thread,
    /// which has no Tokio runtime. A plain `#[test]` is that thread: spawning
    /// there with `tokio::spawn` panicked with `TryCurrentError` and aborted
    /// the whole app the moment the AI Chat panel opened.
    #[test]
    fn a_subscription_forwards_from_a_thread_without_a_tokio_runtime() {
        assert!(
            tokio::runtime::Handle::try_current().is_err(),
            "the test must run where the main thread runs: outside any runtime"
        );
        let (notices, _) = broadcast::channel(4);
        let journal = AcpEventJournal::new(AcpConnectionId::new(), 1, notices);
        journal.append(None, None, AcpClientEvent::TurnStarted);
        let events = journal.subscribe(0).expect("subscribe");
        drop(journal);

        let (frames_tx, frames_rx) = mpsc::channel();
        forward_detached(events, move |frame| frames_tx.send(frame).is_ok());

        let bound = Duration::from_secs(10);
        let first = frames_rx.recv_timeout(bound).expect("the backlog event");
        assert!(
            matches!(&first, AcpStreamFrame::Event(envelope)
                if matches!(envelope.event, AcpClientEvent::TurnStarted)),
            "expected the retained event first, got {first:?}"
        );
        let last = frames_rx.recv_timeout(bound).expect("the end frame");
        assert!(
            matches!(last, AcpStreamFrame::End),
            "a closed journal ends the stream, got {last:?}"
        );
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_message_action(
    state: State<'_, Arc<AppState>>,
    connection_id: AcpConnectionId,
    session_id: v1::SessionId,
    request: crate::acp_message_actions::MessageActionRequest,
) -> Result<crate::acp_message_actions::MessageActionReceipt, AcpClientError> {
    crate::acp_message_actions::send(&state, connection_id, session_id, request).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn acp_message_reactions(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> crate::acp_message_actions::MessageReactions {
    crate::acp_message_actions::reactions(&state, &session_id)
}
