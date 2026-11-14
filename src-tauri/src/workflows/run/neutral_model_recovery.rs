//! Restart continuation is a new prompt, never a resumed provider RPC or write.
use super::neutral_effects::{EffectCommand, EffectKey, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralExecution, NeutralStatus};
use super::neutral_model_adapter::{model_request, run_request};
use super::neutral_model_tools::ModelSession;
use super::runtime::RuntimeOwner;
use crate::acp::{AcpClientManager, EgoAcpConfig};
use crate::workflows::catalog::content_digest;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

impl NeutralExecution {
    pub(super) fn model_recovery_context(&self, key: &EffectKey) -> Result<String, String> {
        let parent = self.effect(key)?;
        let previous = parent
            .model_session
            .as_ref()
            .ok_or("original model session missing")?;
        if !parent.dispatched
            || parent.cancel_started
            || parent.tool.is_some()
            || parent.preparation.model_tools.is_none()
            || parent.settled()
            || self.status != NeutralStatus::Running
            || !self.effect_attempt_pending(key)
        {
            return Err("model continuation requires its pending original parent".into());
        }
        let mut results = Vec::new();
        for effect in self
            .effects
            .values()
            .filter(|e| e.tool.as_ref().is_some_and(|call| call.parent == *key))
        {
            let observation = effect
                .observation
                .as_ref()
                .ok_or("original tool result missing")?;
            let EffectOutcome::Completed { result } = &observation.outcome else {
                return Err("original tool result still requires reconciliation".into());
            };
            if !observation.stopped {
                return Err("original tool has not stopped".into());
            }
            results.push(json!({"call":effect.tool,"effect":effect.key,
                "arguments":effect.invocation.inputs,"result":result}));
        }
        if results.is_empty() {
            return Err("model continuation requires a recorded original tool result".into());
        }
        serde_json::to_string(&json!({
            "type":"reconciled_tool_results", "parent":key,"previous_session":previous,
            "continuation":"The interrupted provider turn is closed. Continue from these recorded results in this new prompt. Do not repeat these operations.",
            "results":results,
        })).map_err(|e| e.to_string())
    }

    pub(super) fn validate_model_resume(
        &self,
        key: &EffectKey,
        previous: &ModelSession,
        session: &ModelSession,
        results_digest: &str,
        at_ms: i64,
    ) -> Result<(), String> {
        session.validate()?;
        if self.effect(key)?.model_session.as_ref() != Some(previous)
            || session == previous
            || at_ms >= self.deadline_ms
            || content_digest(&self.model_recovery_context(key)?)? != results_digest
        {
            return Err("model continuation session/results fence differs".into());
        }
        Ok(())
    }
}

impl RuntimeOwner {
    pub(super) async fn resume_model_activity(
        self: &Arc<Self>,
        manager: &AcpClientManager,
        effect: &NeutralEffect,
        runtime: &EgoAcpConfig,
        root: PathBuf,
    ) -> Result<BTreeMap<String, Value>, String> {
        let run = self.store.neutral_execution(&effect.key.fence.run)?;
        if run.effect(&effect.key)? != effect {
            return Err("model continuation differs from current ledger effect".into());
        }
        let context = run.model_recovery_context(&effect.key)?;
        let results_digest = content_digest(&context)?;
        let previous = effect
            .model_session
            .clone()
            .ok_or("original session missing")?;
        let connection =
            serde_json::from_value(json!(previous.connection)).map_err(|e| e.to_string())?;
        if manager
            .snapshot(connection)
            .is_ok_and(|s| s.settlement.is_none())
        {
            return Err("original model connection is still active".into());
        }
        let mut request = model_request(effect)?;
        request.context.push(context);
        let remaining = run.deadline_ms.saturating_sub(super::store::now_ms());
        if remaining <= 0 {
            return Err("model continuation deadline expired".into());
        }
        request.limits.timeout_ms = request.limits.timeout_ms.min(remaining as u64);
        request.validate(runtime)?;
        let host = super::neutral_model_mcp::ModelMcpHost::new(self, effect.key.clone())?;
        run_request(manager, &request, runtime, root, host, |session| {
            self.neutral_effect_command(
                &effect.key.fence.run,
                EffectCommand::ResumeModel {
                    key: effect.key.clone(),
                    previous,
                    session,
                    results_digest,
                },
            )
            .map(|_| ())
        })
        .await
    }
}
