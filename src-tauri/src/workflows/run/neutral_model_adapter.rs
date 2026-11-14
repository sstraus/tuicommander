//! Exact ACP requests; interrupted turns continue only as explicit new prompts.
use super::neutral_effects::{EffectCommand, NeutralEffect};
use super::neutral_model_request::ModelRequest;
use super::neutral_model_tools::ModelSession;
use super::runtime::RuntimeOwner;
use crate::acp::oneshot::{Refusal, Step, TurnCollector};
use crate::acp::{
    AcpClientEvent, AcpClientManager, AcpConnectRequest, AcpSessionAuthority, EgoAcpConfig,
    INITIALIZE_TIMEOUT, McpOverAcpHost,
};
use agent_client_protocol::schema::v1;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

impl RuntimeOwner {
    /// The installed backend supplies the runtime, workspace and scoped host.
    /// This entry has no argv, PTY, fallback model, provider retry or broad MCP.
    pub(super) async fn run_model_activity(
        self: &Arc<Self>,
        manager: &AcpClientManager,
        effect: &NeutralEffect,
        runtime: &EgoAcpConfig,
        root: PathBuf,
    ) -> Result<BTreeMap<String, Value>, String> {
        let request = model_request(effect)?;
        request.validate(runtime)?;
        if effect.model_session.is_some()
            || effect.tool.is_some()
            || !effect.dispatched
            || effect.preparation.model_tools.is_none()
        {
            return Err(
                "model requires a fresh dispatched parent with retained tool policy".into(),
            );
        }
        let host = super::neutral_model_mcp::ModelMcpHost::new(self, effect.key.clone())?;
        run_request(manager, &request, runtime, root, host, |session| {
            self.neutral_effect_command(
                &effect.key.fence.run,
                EffectCommand::BindModel {
                    key: effect.key.clone(),
                    session,
                },
            )
            .map(|_| ())
        })
        .await
    }
}

pub(super) async fn run_request(
    manager: &AcpClientManager,
    request: &ModelRequest,
    runtime: &EgoAcpConfig,
    root: PathBuf,
    host: Arc<dyn McpOverAcpHost>,
    bind: impl FnOnce(ModelSession) -> Result<(), String>,
) -> Result<BTreeMap<String, Value>, String> {
    request.validate(runtime)?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(request.limits.timeout_ms);
    let connection = manager
        .connect_mediated(
            runtime,
            AcpConnectRequest {
                root: root.clone(),
                env: None,
            },
            Duration::from_millis(request.limits.timeout_ms).min(INITIALIZE_TIMEOUT),
            uuid::Uuid::new_v4().to_string(),
            host,
        )
        .await
        .map_err(|e| e.message)?;
    let id = connection.connection_id;
    let result = tokio::time::timeout_at(deadline, async {
        let mut events = manager
            .subscribe(id, connection.latest_sequence + 1)
            .map_err(|e| e.message)?;
        let attachment = manager
            .new_session(
                id,
                AcpSessionAuthority {
                    cwd: root,
                    additional_directories: vec![],
                    mcp_servers: vec![],
                },
            )
            .await
            .map_err(|e| e.message)?;
        let session = attachment.session_id;
        for (option, wanted) in [("model", &request.model), ("effort", &request.effort)] {
            loop {
                let snapshot = manager.snapshot(id).map_err(|e| e.message)?;
                let attached = snapshot
                    .attachments
                    .iter()
                    .find(|a| a.session_id == session)
                    .ok_or("model attachment disappeared")?;
                if let Some(error) = &attached.config_failure {
                    return Err(format!("model unavailable: {error}"));
                }
                if current(&attached.config_options, option, wanted)
                    || attached.offers_config_value(option, wanted)
                {
                    break;
                }
                // Only a wake-up: the next snapshot holds what the event reports.
                events
                    .recv()
                    .await
                    .ok_or("model connection ended while waiting for settings")?
                    .map_err(|e| e.message)?;
            }
            manager
                .set_config_option(
                    id,
                    v1::SetSessionConfigOptionRequest::new(
                        session.clone(),
                        v1::SessionConfigId::new(option),
                        v1::SessionConfigOptionValue::value_id(v1::SessionConfigValueId::new(
                            wanted.clone(),
                        )),
                    ),
                )
                .await
                .map_err(|e| e.message)?;
        }
        let snapshot = manager.snapshot(id).map_err(|e| e.message)?;
        let attached = snapshot
            .attachments
            .iter()
            .find(|a| a.session_id == session)
            .ok_or("model attachment disappeared")?;
        if !current(&attached.config_options, "model", &request.model)
            || !current(&attached.config_options, "effort", &request.effort)
        {
            return Err("model unavailable: exact settings were not acknowledged".into());
        }
        bind(ModelSession {
            connection: id.to_string(),
            session: session.to_string(),
            generation: connection.generation,
        })?;
        manager
            .prompt(
                id,
                session.clone(),
                vec![v1::ContentBlock::Text(v1::TextContent::new(
                    request.prompt()?,
                ))],
            )
            .await
            .map_err(|e| e.message)?;
        let mut collector = TurnCollector::default();
        let mut bytes = 0usize;
        while let Some(event) = events.recv().await {
            let event = event.map_err(|e| e.message)?;
            if event
                .session_id
                .as_ref()
                .is_some_and(|other| other != &session)
            {
                continue;
            }
            if let AcpClientEvent::SessionUpdate { update } = &event.event
                && let v1::SessionUpdate::AgentMessageChunk(chunk) = update.as_ref()
                && let v1::ContentBlock::Text(text) = &chunk.content
            {
                bytes = bytes
                    .checked_add(text.text.len())
                    .ok_or("model output size overflow")?;
                if bytes > request.limits.max_output_bytes {
                    return Err("model output exceeds its declared byte limit".into());
                }
            }
            match collector.observe(&event.event) {
                Step::Continue => {}
                Step::Done => break,
                Step::Refuse(Refusal::Permission(request)) => {
                    manager
                        .respond_permission(id, request, v1::RequestPermissionOutcome::Cancelled)
                        .await
                        .map_err(|e| e.message)?;
                }
                Step::Refuse(Refusal::Elicitation(request)) => {
                    manager
                        .respond_elicitation(id, request, v1::ElicitationAction::Cancel)
                        .await
                        .map_err(|e| e.message)?;
                }
            }
        }
        if let Some(error) = collector.failure() {
            return Err(error.to_owned());
        }
        let turn = collector.finish().ok_or("model turn did not settle")?;
        if turn.stop_reason != v1::StopReason::EndTurn || turn.declined != 0 {
            return Err("model turn did not complete without refused requests".into());
        }
        request.output(&turn.text)
    })
    .await
    .map_err(|_| "model unavailable or request exceeded its declared time limit".to_owned())
    .and_then(|result| result);
    // Await the manager's owned shutdown. Its transport settlement is not
    // represented here as OS process/workspace reconciliation evidence.
    let shutdown = manager.disconnect(id).await.map_err(|e| e.message);
    match (result, shutdown) {
        (Ok(value), Ok(_)) => Ok(value),
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(format!("model shutdown unconfirmed: {error}")),
    }
}

fn current(options: &[v1::SessionConfigOption], id: &str, wanted: &str) -> bool {
    options.iter().any(|option| {
        serde_json::to_value(option)
            .is_ok_and(|value| value["id"] == id && value["currentValue"] == wanted)
    })
}

pub(super) fn model_request(effect: &NeutralEffect) -> Result<ModelRequest, String> {
    super::native_agent::request(&effect.invocation)
}
