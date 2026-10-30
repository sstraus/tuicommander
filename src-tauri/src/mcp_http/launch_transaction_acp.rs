//! Genuine ACP side of the managed launch journal. This module never allocates
//! a terminal, and never retries an uncertain process/session/prompt effect.
use super::*;
use crate::acp::{AcpConnectRequest, AcpSessionAuthority, EgoAcpConfig};
use agent_client_protocol::schema::v1;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AcpLaunch {
    pub executable: PathBuf,
    pub profile: String,
    pub machine_profile: String,
    pub model: String,
    pub effort: String,
}

fn current(options: &[v1::SessionConfigOption], id: &str, wanted: &str) -> bool {
    options.iter().any(|option| {
        serde_json::to_value(option)
            .is_ok_and(|value| value["id"] == id && value["currentValue"] == wanted)
    })
}

pub(super) async fn create(
    state: &Arc<AppState>,
    journal: &Journal,
    key: &str,
    record: &mut Record,
    launch: &AcpLaunch,
) -> Result<(), String> {
    let root = PathBuf::from(
        record
            .workspace_path
            .as_deref()
            .ok_or("missing ACP workspace")?,
    );
    if record.stage == "selected" {
        record.stage = "acp_connect_started".into();
        journal.save(key, record)?;
        let snapshot = state
            .acp
            .connect_with_peer(
                &EgoAcpConfig {
                    executable: launch.executable.clone(),
                    profile: launch.profile.clone(),
                },
                AcpConnectRequest {
                    root: root.clone(),
                    env: None,
                },
                record.session_id.clone(),
            )
            .await
            .map_err(|e| e.message)?;
        record.acp_connection = Some(snapshot.connection_id);
        record.acp_generation = Some(snapshot.generation);
        record.stage = "acp_connected".into();
        journal.save(key, record)?;
    }
    if record.stage == "acp_connect_started" {
        // A successful connection is observable under the pre-issued peer. A
        // missing inventory cannot establish that a process was never started.
        let snapshot = state
            .acp
            .peer_snapshot(&record.session_id)
            .filter(|snapshot| snapshot.settlement.is_none())
            .ok_or("ACP connection creation uncertain; retained peer is not in live inventory")?;
        record.acp_connection = Some(snapshot.connection_id);
        record.acp_generation = Some(snapshot.generation);
        record.stage = "acp_connected".into();
        journal.save(key, record)?;
    }
    if record.acp_process_id.is_none() {
        let peer = record.session_id.clone();
        record.acp_process_id = tokio::task::spawn_blocking(move || {
            crate::pty::process_tree_snapshot()
                .and_then(|processes| crate::pty::workflow_acp_process(&peer, &processes).ok())
        })
        .await
        .map_err(|e| format!("ACP process observation: {e}"))?;
        journal.save(key, record)?;
    }
    let connection = record
        .acp_connection
        .ok_or("missing ACP connection reference")?;
    if record.stage == "acp_connected" {
        record.stage = "acp_session_started".into();
        journal.save(key, record)?;
        let attachment = state
            .acp
            .new_session_for_repo(
                connection,
                AcpSessionAuthority {
                    cwd: root,
                    additional_directories: vec![],
                    mcp_servers: vec![],
                },
                &launch.machine_profile,
            )
            .await
            .map_err(|e| e.message)?;
        record.acp_conversation = Some(attachment.session_id.to_string());
        record.stage = "acp_configuring".into();
        journal.save(key, record)?;
    }
    if record.stage == "acp_session_started" {
        let snapshot = state
            .acp
            .peer_snapshot(&record.session_id)
            .filter(|s| {
                Some(s.connection_id) == record.acp_connection
                    && Some(s.generation) == record.acp_generation
                    && s.settlement.is_none()
            })
            .ok_or("ACP session creation uncertain; connection inventory is unavailable")?;
        let [attachment] = snapshot.attachments.as_slice() else {
            return Err("ACP session creation uncertain; no unique attachment proof".into());
        };
        record.acp_conversation = Some(attachment.session_id.to_string());
        record.stage = "acp_configuring".into();
        journal.save(key, record)?;
    }
    if record.stage == "acp_configuring" {
        let session: v1::SessionId = record
            .acp_conversation
            .clone()
            .ok_or("missing ACP conversation")?
            .into();
        let snapshot = state.acp.snapshot(connection).map_err(|e| e.message)?;
        let mut options = snapshot
            .attachments
            .iter()
            .find(|a| a.session_id == session)
            .ok_or("ACP conversation is not attached")?
            .config_options
            .clone();
        // Config writes are absolute idempotent settings, not prompt delivery.
        // Effort can alter the offered model set; verify both on the final set.
        for (id, wanted) in [("effort", &launch.effort), ("model", &launch.model)] {
            if !current(&options, id, wanted) {
                options = state
                    .acp
                    .set_config_option(
                        connection,
                        v1::SetSessionConfigOptionRequest::new(
                            session.clone(),
                            v1::SessionConfigId::new(id),
                            v1::SessionConfigOptionValue::value_id(v1::SessionConfigValueId::new(
                                wanted.clone(),
                            )),
                        ),
                    )
                    .await
                    .map_err(|e| e.message)?;
            }
        }
        if !current(&options, "model", &launch.model)
            || !current(&options, "effort", &launch.effort)
        {
            return Err(
                "ACP did not retain the exact requested model and effort; prompt not delivered"
                    .into(),
            );
        }
        record.child = Some(json!({
            "peer_id":record.session_id,
            "acp":crate::workflows::AcpAgentBinding {
                connection_id: connection,
                generation: record.acp_generation.ok_or("missing ACP generation")?,
                conversation_id: session.to_string(),
            },
            "effective_config":options,
        }));
        // Workflow binding must commit before the prompt can invoke tools.
        record.stage = "acp_not_delivered".into();
        journal.save(key, record)?;
    }
    Ok(())
}

pub(super) async fn deliver(
    state: &Arc<AppState>,
    journal: &Journal,
    key: &str,
    record: &mut Record,
    prompt: &str,
) -> Result<(), String> {
    if record.stage == "acp_delivered" {
        return Ok(());
    }
    if record.stage == "acp_prompt_started" {
        if let (Some(connection), Some(turn)) = (record.acp_connection, record.acp_turn)
            && state.acp.has_sent_prompt(connection, turn, prompt)
        {
            record.prompt_handoff = true;
            record.stage = "acp_delivered".into();
            return journal.save(key, record);
        }
        return Err(
            "ACP prompt handoff is unobserved or queued; the payload was not resent".into(),
        );
    }
    if record.stage != "acp_not_delivered" {
        return Err("ACP prompt outcome is uncertain; delivery is not replayed".into());
    }
    let connection = record.acp_connection.ok_or("missing ACP connection")?;
    let session: v1::SessionId = record
        .acp_conversation
        .clone()
        .ok_or("missing ACP conversation")?
        .into();
    let snapshot = state
        .acp
        .peer_snapshot(&record.session_id)
        .filter(|s| {
            s.connection_id == connection
                && Some(s.generation) == record.acp_generation
                && s.settlement.is_none()
        })
        .ok_or("ACP delivery not attempted: matching live peer is unavailable")?;
    if !snapshot.attachments.iter().any(|a| {
        a.session_id == session
            && a.active_turn.is_none()
            && a.state == crate::acp::AcpAttachmentState::Idle
    }) {
        return Err("ACP delivery not attempted: conversation is not idle".into());
    }
    record.stage = "acp_prompt_started".into();
    journal.save(key, record)?;
    let turn = state
        .acp
        .prompt(
            connection,
            session,
            vec![v1::ContentBlock::Text(v1::TextContent::new(prompt))],
        )
        .await
        .map_err(|e| e.message)?;
    record.acp_turn = Some(turn);
    journal.save(key, record)?;
    if !state.acp.has_sent_prompt(connection, turn, prompt) {
        return Err(
            "ACP prompt accepted but handoff is unobserved or queued; the payload was not resent"
                .into(),
        );
    }
    record.prompt_handoff = true;
    record.stage = "acp_delivered".into();
    journal.save(key, record)
}
