//! The selected AI Chat document belongs to the host workspace, not a device.
use crate::{AppState, acp::AcpClientError};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSelection {
    pub workspace: String,
    pub session_id: Option<String>,
    /// The tab `session_id` continues, while the selection is a compaction successor.
    /// Followers replace that tab in place instead of appending the successor.
    pub replacing: Option<String>,
}

/// `(workspace, successor, replaced)` of the last compaction selection. In memory only:
/// after a restart every client rereads its own tabs, so a lost link appends at worst.
pub(crate) type SuccessorLink = parking_lot::Mutex<Option<(String, String, String)>>;

pub(crate) async fn read(state: &AppState) -> Result<ChatSelection, AcpClientError> {
    let workspace = crate::acp_commands::workspace_root(state)
        .await?
        .to_string_lossy()
        .into_owned();
    let session_id = state
        .config
        .read()
        .ai_chat_sessions
        .get(&workspace)
        .cloned();
    let replacing = state
        .ai_chat_successor
        .lock()
        .as_ref()
        .filter(|(linked, successor, _)| {
            *linked == workspace && session_id.as_deref() == Some(successor.as_str())
        })
        .map(|(_, _, replaced)| replaced.clone());
    Ok(ChatSelection {
        workspace,
        session_id,
        replacing,
    })
}

pub(crate) async fn select(
    state: Arc<AppState>,
    session_id: Option<String>,
    replacing: Option<String>,
) -> Result<(), String> {
    if session_id
        .iter()
        .chain(replacing.iter())
        .any(|id| id.trim().is_empty() || id.contains('\0'))
    {
        return Err("Conversation id must not be empty or contain NUL".into());
    }
    let workspace = crate::acp_commands::workspace_root(&state)
        .await
        .map_err(|error| error.message)?
        .to_string_lossy()
        .into_owned();
    // Recorded before the commit publishes, so the reread that follows sees it.
    if let Some(session) = &session_id {
        let mut link = state.ai_chat_successor.lock();
        match replacing.filter(|replaced| replaced != session) {
            Some(replaced) => *link = Some((workspace.clone(), session.clone(), replaced)),
            None if link.as_ref().is_some_and(|(linked, successor, _)| {
                *linked == workspace && successor != session
            }) =>
            {
                *link = None
            }
            None => {}
        }
    }
    tokio::task::spawn_blocking(move || {
        crate::config::commit_config_change(&state, |current| {
            let mut next = current.clone();
            if let Some(session) = session_id {
                next.ai_chat_sessions.insert(workspace, session);
            } else {
                next.ai_chat_sessions.remove(&workspace);
            }
            Ok(next)
        })
        .map(|_| ())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Invalidation only: clients reread the latest selection rather than replaying old focus.
pub(crate) fn publish(state: &AppState) {
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::AiChatSelectionChanged);
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit("ai-chat-selection-changed", serde_json::json!({}));
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn ai_chat_selection(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ChatSelection, AcpClientError> {
    read(&state).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn select_ai_chat_conversation(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: Option<String>,
    replacing: Option<String>,
) -> Result<(), String> {
    select(state.inner().clone(), session_id, replacing).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppEvent;

    // Catches: a desktop selection never reaching another device, or a restart restoring stale focus.
    #[tokio::test]
    #[serial_test::serial]
    async fn ai_chat_selection_persists_and_invalidates_only_real_changes() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        let mut events = state.event_bus.subscribe();
        select(state.clone(), Some("desktop".into()), None)
            .await
            .unwrap();
        assert_eq!(
            read(&state).await.unwrap().session_id.as_deref(),
            Some("desktop")
        );
        assert!(
            std::iter::from_fn(|| events.try_recv().ok())
                .any(|event| matches!(event, AppEvent::AiChatSelectionChanged))
        );
        select(state.clone(), Some("desktop".into()), None)
            .await
            .unwrap();
        assert!(
            !std::iter::from_fn(|| events.try_recv().ok())
                .any(|event| matches!(event, AppEvent::AiChatSelectionChanged))
        );
        select(state.clone(), Some("phone".into()), None)
            .await
            .unwrap();
        let restored = Arc::new(crate::state::tests_support::make_test_app_state());
        *restored.config.write() = crate::config::load_app_config();
        assert_eq!(
            read(&restored).await.unwrap().session_id.as_deref(),
            Some("phone")
        );
    }

    // Catches: the selection payload dropping the compaction source (followers append the successor
    // beside it, 3039-fea4), or a later ordinary selection still telling followers to replace a tab.
    #[tokio::test]
    #[serial_test::serial]
    async fn ai_chat_selection_names_the_tab_a_compaction_successor_replaces() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        select(state.clone(), Some("source".into()), None)
            .await
            .unwrap();
        assert_eq!(read(&state).await.unwrap().replacing, None);
        select(
            state.clone(),
            Some("successor".into()),
            Some("source".into()),
        )
        .await
        .unwrap();
        let selected = read(&state).await.unwrap();
        assert_eq!(selected.session_id.as_deref(), Some("successor"));
        assert_eq!(selected.replacing.as_deref(), Some("source"));
        assert_eq!(
            serde_json::to_value(&selected).unwrap()["replacing"],
            serde_json::json!("source")
        );
        // Reselecting the successor (a reload, a resync) keeps the link for late followers.
        select(state.clone(), Some("successor".into()), None)
            .await
            .unwrap();
        assert_eq!(
            read(&state).await.unwrap().replacing.as_deref(),
            Some("source")
        );
        select(state.clone(), Some("other".into()), None)
            .await
            .unwrap();
        assert_eq!(read(&state).await.unwrap().replacing, None);
        select(state.clone(), Some("successor".into()), None)
            .await
            .unwrap();
        assert_eq!(read(&state).await.unwrap().replacing, None);
    }

    // Catches: selection of a different workspace clobbering the first, or clear leaving a stale id.
    #[tokio::test]
    #[serial_test::serial]
    async fn ai_chat_selection_isolates_workspaces_and_clears() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let first = dir.path().join("first").to_string_lossy().into_owned();
        state.config.write().ai_chat_workspace = first.clone();
        select(state.clone(), Some("first-chat".into()), None)
            .await
            .unwrap();
        crate::config::commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.ai_chat_workspace = dir.path().join("second").to_string_lossy().into_owned();
            Ok(next)
        })
        .unwrap();
        assert_eq!(read(&state).await.unwrap().session_id, None);
        select(state.clone(), Some("second-chat".into()), None)
            .await
            .unwrap();
        select(state.clone(), None, None).await.unwrap();
        assert_eq!(read(&state).await.unwrap().session_id, None);
        state.config.write().ai_chat_workspace = first;
        assert_eq!(
            read(&state).await.unwrap().session_id.as_deref(),
            Some("first-chat")
        );
    }
}
