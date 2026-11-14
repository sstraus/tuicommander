//! Durable launch authority for a conversation with its own ego process.
use crate::{
    AppState,
    acp::{
        AcpAttachKind, AcpClientError, AcpConnectRequest, AcpConnectionSnapshot,
        AcpSessionAuthority, EgoAcpConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatLaunch {
    #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"next_session","effect":"Used when the next session or process starts; existing sessions retain their launch configuration.","operations":["get","set"],"confirmationClass":"execution_authority"}))]
    pub executable: String,
    #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"next_session","effect":"Used when the next session or process starts; existing sessions retain their launch configuration.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub profile: String,
    #[serde(
        default,
        deserialize_with = "crate::config::deserialize_ai_chat_master_prompt"
    )]
    #[schemars(extend("x-not-setting" = "Saved conversation instruction snapshot; edited through ai_chat_master_prompt for new chats."))]
    pub master_prompt: String,
    #[schemars(extend("x-setting" = {"sensitivity":"private","apply":"next_session","effect":"Used when the next session or process starts; existing sessions retain their launch configuration.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub workspace: PathBuf,
    #[schemars(extend("x-not-setting" = "Host-issued conversation identity; not a configurable launch option."))]
    pub peer_id: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChatOpenRequest {
    pub session_id: Option<String>,
    pub executable: Option<String>,
    pub profile: Option<String>,
    pub workspace: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatOpened {
    pub connection: AcpConnectionSnapshot,
    pub session_id: String,
    pub launch: ChatLaunch,
    pub replayed: bool,
}

impl ChatLaunch {
    fn ego_config(&self) -> EgoAcpConfig {
        EgoAcpConfig {
            executable: PathBuf::from(&self.executable),
            profile: self.profile.clone(),
            master_prompt: self.master_prompt.clone(),
            ai_chat: true,
        }
    }
}

async fn resolve(
    state: &AppState,
    request: &ChatOpenRequest,
) -> Result<ChatLaunch, AcpClientError> {
    if let Some(session) = &request.session_id {
        if request.executable.is_some() || request.profile.is_some() || request.workspace.is_some()
        {
            return Err(AcpClientError::invalid_input(
                "saved conversation launch options cannot be replaced",
            ));
        }
        return state
            .config
            .read()
            .ai_chat_launches
            .get(session)
            .cloned()
            .ok_or_else(|| {
                AcpClientError::invalid_input("no saved launch configuration for this conversation")
            });
    }
    let (executable, profile) = {
        let settings = state.config.read();
        (
            request
                .executable
                .clone()
                .unwrap_or_else(|| settings.ego_executable.clone()),
            request
                .profile
                .clone()
                .unwrap_or_else(|| settings.ego_profile.clone()),
        )
    };
    let workspace = match &request.workspace {
        Some(path) => {
            if !path.is_absolute() {
                return Err(AcpClientError::invalid_input(
                    "conversation workspace must be absolute",
                ));
            }
            tokio::fs::create_dir_all(path).await.map_err(|error| {
                AcpClientError::invalid_input(format!(
                    "cannot create conversation workspace: {error}"
                ))
            })?;
            tokio::fs::canonicalize(path).await.map_err(|error| {
                AcpClientError::invalid_input(format!(
                    "cannot open conversation workspace: {error}"
                ))
            })?
        }
        None => crate::acp_commands::workspace_root(state).await?,
    };
    let launch = ChatLaunch {
        executable,
        profile,
        master_prompt: state.config.read().ai_chat_master_prompt.clone(),
        workspace,
        peer_id: uuid::Uuid::new_v4().to_string(),
    };
    crate::acp::launch_spec(&launch.ego_config(), &launch.workspace)?;
    Ok(launch)
}

pub(crate) async fn open(
    state: &Arc<AppState>,
    request: ChatOpenRequest,
) -> Result<ChatOpened, AcpClientError> {
    let launch = resolve(state, &request).await?;
    let connection = state
        .acp
        .connect_with_peer(
            &launch.ego_config(),
            AcpConnectRequest {
                root: launch.workspace.clone(),
                env: None,
            },
            launch.peer_id.clone(),
        )
        .await?;
    let authority = AcpSessionAuthority {
        cwd: launch.workspace.clone(),
        additional_directories: vec![],
        mcp_servers: vec![],
    };
    let mut replayed = false;
    let session_id =
        if let Some(session) = request.session_id {
            if !connection
                .attachments
                .iter()
                .any(|attachment| attachment.session_id.to_string() == session)
            {
                replayed = true;
                if let Err(error) = state
                    .acp
                    .attach(
                        connection.connection_id,
                        AcpAttachKind::Load,
                        session.clone().into(),
                        authority,
                    )
                    .await
                {
                    let _ = state.acp.disconnect(connection.connection_id).await;
                    return Err(error);
                }
            }
            session
        } else {
            let attachment =
                match crate::acp_commands::session_new(state, connection.connection_id, authority)
                    .await
                {
                    Ok(attachment) => attachment,
                    Err(error) => {
                        let _ = state.acp.disconnect(connection.connection_id).await;
                        return Err(error);
                    }
                };
            let session = attachment.session_id.to_string();
            if let Err(error) = persist(state, &session, &launch).await {
                let _ = state.acp.disconnect(connection.connection_id).await;
                return Err(error);
            }
            session
        };
    Ok(ChatOpened {
        connection: state.acp.snapshot(connection.connection_id)?,
        session_id,
        launch,
        replayed,
    })
}

async fn persist(
    state: &Arc<AppState>,
    session: &str,
    launch: &ChatLaunch,
) -> Result<(), AcpClientError> {
    let saved = session.to_owned();
    let saved_launch = launch.clone();
    let saved_state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        crate::config::commit_config_change(&saved_state, |current| {
            let mut next = current.clone();
            next.ai_chat_launches
                .insert(saved.clone(), saved_launch.clone());
            Ok(next)
        })
    })
    .await
    .map_err(|error| {
        AcpClientError::invalid_input(format!("cannot save conversation launch: {error}"))
    })?
    .map(|_| ())
    .map_err(|error| {
        AcpClientError::invalid_input(format!("cannot save conversation launch: {error}"))
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn acp_chat_open(
    state: tauri::State<'_, Arc<AppState>>,
    request: ChatOpenRequest,
) -> Result<ChatOpened, AcpClientError> {
    open(&state, request).await
}

/// Successor conversations must retain launch authority rather than fall into defaults.
async fn inherit(state: &Arc<AppState>, source: &str, target: &str) -> Result<(), AcpClientError> {
    let saved = state.config.read().ai_chat_launches.get(source).cloned();
    if let Some(mut launch) = saved {
        launch.peer_id = uuid::Uuid::new_v4().to_string();
        persist(state, target, &launch).await?;
    }
    Ok(())
}

pub(crate) async fn fork(
    state: &Arc<AppState>,
    connection_id: crate::acp::AcpConnectionId,
    session_id: agent_client_protocol::schema::v1::SessionId,
    authority: AcpSessionAuthority,
    at_message_id: Option<String>,
) -> Result<crate::acp::AcpAttachmentSnapshot, AcpClientError> {
    let source = session_id.to_string();
    let attachment = state
        .acp
        .attach_at_message(
            connection_id,
            AcpAttachKind::Fork,
            session_id,
            authority,
            at_message_id,
        )
        .await?;
    inherit(state, &source, &attachment.session_id.to_string()).await?;
    Ok(attachment)
}

pub(crate) async fn compact(
    state: &Arc<AppState>,
    connection_id: crate::acp::AcpConnectionId,
    session_id: agent_client_protocol::schema::v1::SessionId,
    request_id: uuid::Uuid,
) -> Result<crate::acp::EgoCompactResponse, AcpClientError> {
    let source = session_id.to_string();
    let result = state
        .acp
        .compact(
            connection_id,
            crate::acp::EgoCompactRequest {
                session_id,
                request_id,
            },
        )
        .await?;
    // A failed publication produces no successor to persist.
    if !result.publication.retry_safe() {
        inherit(state, &source, &result.target_session_id.to_string()).await?;
    }
    Ok(result)
}

/// Registered peers of every AI Chat: daily chats per workspace and custom conversations.
pub(crate) fn registered_ai_chat_peers(state: &AppState) -> Vec<String> {
    let config = state.config.read();
    config
        .ai_chat_peer_ids
        .values()
        .chain(config.ai_chat_launches.values().map(|l| &l.peer_id))
        .filter(|peer| state.peer_agents.contains_key(*peer))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_fixture(scenario: &str) -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        for (source, target) in [
            (format!("{scenario}.jsonl"), "scenario.jsonl"),
            ("ego-initialize.json".into(), "ego-initialize.json"),
            (
                "ego-profile-ceiling.json".into(),
                "ego-profile-ceiling.json",
            ),
        ] {
            std::fs::copy(
                format!("tests/fixtures/acp/{source}"),
                dir.path().join(target),
            )
            .unwrap();
        }
        std::fs::write(dir.path().join(".tuic.json"), r#"{"ego_profile":"wide"}"#).unwrap();
        std::fs::write(
            dir.path().join("expected-profile.txt"),
            "conversation-profile",
        )
        .unwrap();
        // Nextest's fixture-bins setup builds the same recorded-response agent
        // used by the ACP integration tests before starting library tests.
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!(
                "tuic-acp-fixture-agent{}",
                std::env::consts::EXE_SUFFIX
            ));
        assert!(
            executable.is_file(),
            "fixture agent missing: {}",
            executable.display()
        );
        std::fs::write(
            dir.path().join("expected-master-prompt.txt"),
            crate::acp::AI_CHAT_PRESENTATION_INSTRUCTIONS,
        )
        .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().ego_executable = executable.to_string_lossy().into_owned();
        state.config.write().ego_profile = "machine".into();
        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        (dir, state)
    }

    /// Catches: custom or default chat creation bypasses the repo ceiling core,
    /// or drops ego's recorded clamp warnings from the returned connection.
    #[tokio::test]
    async fn custom_chat_repo_profile_sends_machine_ceiling_and_keeps_warnings() {
        for profile in [Some("conversation-profile".to_string()), None] {
            let (dir, state) = profile_fixture("repo-profile-ceiling");
            let _config = crate::config::set_config_dir_override(dir.path().join("config"));
            std::fs::write(
                dir.path().join("expected-profile.txt"),
                profile.as_deref().unwrap_or("machine"),
            )
            .unwrap();
            let opened = open(
                &state,
                ChatOpenRequest {
                    profile: profile.clone(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(
                opened.launch.profile,
                profile.as_deref().unwrap_or("machine")
            );
            assert_eq!(state.config.read().ego_profile, "machine");
            assert_eq!(opened.session_id, "2293c32a-83b6-564b-967f-3c4534548885");
            assert_eq!(
                opened.connection.attachments[0].profile_warnings,
                [
                    "mode yolo exceeds profile ceiling default; using default",
                    "sandbox off exceeds profile ceiling workspace; using workspace",
                ]
            );
            assert_eq!(
                crate::config::load_app_config().ai_chat_launches[&opened.session_id],
                opened.launch
            );
            state
                .acp
                .disconnect(opened.connection.connection_id)
                .await
                .unwrap();
        }
    }

    /// Catches: the persisted setting never reaches ego, or editing defaults restarts a live chat.
    #[tokio::test]
    async fn custom_chat_master_prompt_reaches_new_process_only() {
        let (dir, state) = profile_fixture("repo-profile-ceiling");
        let _config = crate::config::set_config_dir_override(dir.path().join("config"));
        state.config.write().ai_chat_master_prompt =
            "Delegate investigations.\nReturn to chat.".into();
        std::fs::write(dir.path().join("expected-profile.txt"), "machine").unwrap();
        std::fs::write(
            dir.path().join("expected-master-prompt.txt"),
            format!(
                "{}\n\nAI Chat master instructions:\nDelegate investigations.\nReturn to chat.",
                crate::acp::AI_CHAT_PRESENTATION_INSTRUCTIONS
            ),
        )
        .unwrap();
        let opened = open(&state, ChatOpenRequest::default()).await.unwrap();
        let saved = crate::config::load_app_config();
        assert_eq!(
            saved.ai_chat_master_prompt,
            "Delegate investigations.\nReturn to chat."
        );
        assert_eq!(
            saved.ai_chat_launches[&opened.session_id].master_prompt,
            "Delegate investigations.\nReturn to chat."
        );
        state.config.write().ai_chat_master_prompt = "New instructions".into();
        let reopened = open(
            &state,
            ChatOpenRequest {
                session_id: Some(opened.session_id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            reopened.connection.connection_id,
            opened.connection.connection_id
        );
        assert_eq!(
            reopened.launch.master_prompt,
            "Delegate investigations.\nReturn to chat."
        );
        let next = resolve(&state, &ChatOpenRequest::default()).await.unwrap();
        let spec = crate::acp::launch_spec(&next.ego_config(), &next.workspace).unwrap();
        assert!(
            spec.args
                .last()
                .unwrap()
                .ends_with("AI Chat master instructions:\nNew instructions")
        );
        state
            .acp
            .disconnect(opened.connection.connection_id)
            .await
            .unwrap();
    }

    /// Catches: blank prompts lose host guidance, @ text reads a file, or host text consumes the user byte limit.
    #[tokio::test]
    async fn master_prompt_launch_preserves_literals_and_enforces_byte_limit() {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().ego_executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        for text in ["", "@private.txt\n--system"] {
            state.config.write().ai_chat_master_prompt = text.into();
            let launch = resolve(&state, &ChatOpenRequest::default()).await.unwrap();
            let spec = crate::acp::launch_spec(&launch.ego_config(), &launch.workspace).unwrap();
            assert!(!spec.args.iter().any(|arg| arg == "--system"));
            let index = spec
                .args
                .iter()
                .position(|arg| arg == "--append-system")
                .unwrap();
            let instructions = &spec.args[index + 1];
            assert!(instructions.contains("fenced code blocks with the `mermaid` language tag"));
            assert!(instructions.contains("prefer Mermaid over ASCII diagrams"));
            assert!(instructions.contains("Use diagrams only when useful"));
            if text.is_empty() {
                assert!(!instructions.contains("AI Chat master instructions:"));
            } else {
                assert!(
                    instructions.ends_with("AI Chat master instructions:\n@private.txt\n--system")
                );
            }
        }
        for text in ["a".repeat(8192), "é".repeat(4096)] {
            let mut document = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
            document["ai_chat_master_prompt"] = serde_json::json!(text);
            let config: crate::config::AppConfig = serde_json::from_value(document).unwrap();
            state.config.write().ai_chat_master_prompt = config.ai_chat_master_prompt;
            let launch = resolve(&state, &ChatOpenRequest::default()).await.unwrap();
            let spec = crate::acp::launch_spec(&launch.ego_config(), &launch.workspace).unwrap();
            assert!(spec.args.last().unwrap().ends_with(&text));
            assert!(spec.args.last().unwrap().len() > 8192);
        }
        for text in ["a".repeat(8193), "é".repeat(4097), "secret\0text".into()] {
            let mut document = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
            document["ai_chat_master_prompt"] = serde_json::json!(text);
            let error = serde_json::from_value::<crate::config::AppConfig>(document)
                .err()
                .expect("invalid prompt must be refused");
            assert!(error.to_string().contains("AI Chat master prompt"));
            assert!(!error.to_string().contains(&text));
        }
    }

    /// Catches: a conversation override is mistaken for an explicit machine ceiling.
    #[tokio::test]
    async fn custom_chat_repo_profile_without_machine_ceiling_is_refused() {
        let (_dir, state) = profile_fixture("ready");
        state.config.write().ego_profile.clear();
        let result = open(
            &state,
            ChatOpenRequest {
                profile: Some("conversation-profile".into()),
                ..Default::default()
            },
        )
        .await;
        let error = result
            .err()
            .expect("missing machine ceiling must refuse creation");
        assert!(error.message.contains("explicit machine ego profile"));
        assert!(state.config.read().ai_chat_launches.is_empty());
    }

    /// Catches: custom chat publishes a session from ego that ignores the ceiling extension.
    #[tokio::test]
    async fn custom_chat_ignored_repo_ceiling_is_not_persisted() {
        let (_dir, state) = profile_fixture("repo-profile-ignored");
        let result = open(
            &state,
            ChatOpenRequest {
                profile: Some("conversation-profile".into()),
                ..Default::default()
            },
        )
        .await;
        let error = result
            .err()
            .expect("unacknowledged ceiling must refuse creation");
        assert!(error.message.contains("did not acknowledge"));
        assert!(state.config.read().ai_chat_launches.is_empty());
    }

    /// Catches: a conversation profile changes global defaults or another chat's ACP launch args.
    #[tokio::test]
    async fn conversation_override_reaches_launch_args_without_changing_defaults() {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().ego_executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        state.config.write().ego_profile = "daily".into();
        state.config.write().ai_chat_workspace = dir.path().to_string_lossy().into_owned();
        let defaults = state.config.read().clone();
        let custom_executable = dir.path().join("observer-ego");
        let custom_workspace = dir.path().join("observer-workspace");
        let custom = resolve(
            &state,
            &ChatOpenRequest {
                profile: Some("coordinator".into()),
                executable: Some(custom_executable.to_string_lossy().into_owned()),
                workspace: Some(custom_workspace),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let spec = crate::acp::launch_spec(&custom.ego_config(), &custom.workspace).unwrap();
        assert_eq!(spec.program, custom_executable);
        assert_ne!(custom.workspace, dir.path());
        assert_eq!(
            spec.args,
            vec![
                "acp",
                "-C",
                custom.workspace.to_str().unwrap(),
                "--profile",
                "coordinator",
                "--append-system",
                crate::acp::AI_CHAT_PRESENTATION_INSTRUCTIONS,
            ]
        );
        let other = resolve(&state, &ChatOpenRequest::default()).await.unwrap();
        let other_spec = crate::acp::launch_spec(&other.ego_config(), &other.workspace).unwrap();
        assert_eq!(&other_spec.args[3..5], &["--profile", "daily"]);
        assert_ne!(custom.peer_id, other.peer_id);
        assert_eq!(state.config.read().ego_profile, defaults.ego_profile);
        assert_eq!(state.config.read().ego_executable, defaults.ego_executable);
        assert_eq!(
            state.config.read().ai_chat_workspace,
            defaults.ai_chat_workspace
        );
    }

    /// Catches: reopening silently uses changed global defaults or loses the durable peer identity.
    #[tokio::test]
    async fn conversation_reopen_keeps_persisted_launch_options() {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(dir.path().join("config"));
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let launch = ChatLaunch {
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            profile: "coordinator".into(),
            master_prompt: "Saved instructions".into(),
            workspace: dir.path().to_owned(),
            peer_id: uuid::Uuid::new_v4().to_string(),
        };
        persist(&state, "conversation", &launch).await.unwrap();
        inherit(&state, "conversation", "forked").await.unwrap();
        let child = state
            .config
            .read()
            .ai_chat_launches
            .get("forked")
            .unwrap()
            .clone();
        assert_eq!(child.profile, launch.profile);
        assert_eq!(child.executable, launch.executable);
        assert_eq!(child.workspace, launch.workspace);
        assert_ne!(child.peer_id, launch.peer_id);
        let saved = crate::config::load_app_config();
        let restarted = crate::state::tests_support::make_test_app_state();
        *restarted.config.write() = saved;
        restarted.config.write().ego_profile = "other-default".into();
        restarted.config.write().ai_chat_master_prompt = "Changed default".into();
        let restored = resolve(
            &restarted,
            &ChatOpenRequest {
                session_id: Some("conversation".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(restored, launch);
        let spec = crate::acp::launch_spec(&restored.ego_config(), &restored.workspace).unwrap();
        assert!(
            spec.args
                .last()
                .unwrap()
                .contains("prefer Mermaid over ASCII diagrams")
        );
        assert!(
            spec.args
                .last()
                .unwrap()
                .ends_with("AI Chat master instructions:\nSaved instructions")
        );
        let error = resolve(
            &restarted,
            &ChatOpenRequest {
                session_id: Some("conversation".into()),
                profile: Some("replace".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("cannot be replaced"));
    }
}
