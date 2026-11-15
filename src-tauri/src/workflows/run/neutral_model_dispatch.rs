//! Kernel tool entry: intent precedes dispatch; replay only reads the same effect.
use super::neutral_effects::{EffectCommand, NeutralEffect};
use super::neutral_model_tools::ModelToolCall;
use super::neutral_runtime::admitted;
use super::runtime::RuntimeOwner;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Notify;

impl RuntimeOwner {
    /// A caller must retain this effect identity while waiting. An in-flight or
    /// uncertain call returns its original effect, never invokes a second write.
    /// The transport must deliver only a validated, durably committed result.
    pub(super) fn start_model_tool(
        self: &Arc<Self>,
        call: ModelToolCall,
        arguments: BTreeMap<String, Value>,
    ) -> Result<NeutralEffect, String> {
        let snapshot = self.store.neutral_execution(&call.parent.fence.run)?;
        let mut invocation = snapshot.tool_invocation(&call, arguments)?;
        if let Some(old) = snapshot.model_tool_call(&call)? {
            if old.invocation != invocation {
                return Err("model tool call identity cannot change resolved arguments".into());
            }
            if old.dispatched {
                return Ok(old.clone());
            }
        }
        let host = self
            .neutral_host
            .lock()
            .clone()
            .ok_or("model tool host unavailable")?;
        admitted(
            host.as_ref(),
            &snapshot,
            &mut invocation,
            Some(&snapshot.effect(&call.parent)?.invocation),
        )?;
        let preparation = host.prepare(&invocation)?;
        let effect = NeutralEffect::prepare_tool(invocation, preparation, call)?;
        let prepared = self.neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Prepare {
                effect: Box::new(effect.clone()),
            },
        )?;
        let effect = prepared.effect(&effect.key)?.clone();
        let tag = format!("invoke:{}", effect.key.digest);
        if !self.neutral_effect_calls.lock().insert(tag.clone()) {
            return Ok(effect);
        }
        let dispatch = (|| {
            // Reload after taking the existing in-flight reservation: another
            // caller may already have committed dispatch or its result.
            let snapshot = self.store.neutral_execution(&effect.key.fence.run)?;
            let effect = snapshot.effect(&effect.key)?.clone();
            if effect.dispatched {
                return Ok((effect, false));
            }
            let mut invocation = effect.invocation.clone();
            let parent = snapshot.effect(
                &effect
                    .tool
                    .as_ref()
                    .ok_or("model tool identity missing")?
                    .parent,
            )?;
            admitted(
                host.as_ref(),
                &snapshot,
                &mut invocation,
                Some(&parent.invocation),
            )?;
            if invocation != effect.invocation || host.prepare(&invocation)? != effect.preparation {
                return Err("model tool current admission differs from its retained intent".into());
            }
            let snapshot = self.neutral_effect_command(
                &effect.key.fence.run,
                EffectCommand::Dispatch {
                    key: effect.key.clone(),
                },
            )?;
            Ok((snapshot.effect(&effect.key)?.clone(), true))
        })();
        match dispatch {
            Ok((effect, true)) if effect.dispatched => {
                self.observe_neutral_future(
                    effect.key.fence.run.clone(),
                    effect.key.clone(),
                    tag,
                    host.invoke_effect(effect.clone()),
                    Arc::new(Notify::new()),
                );
                Ok(effect)
            }
            Ok((effect, _)) => {
                self.neutral_effect_calls.lock().remove(&tag);
                Ok(effect)
            }
            Err(error) => {
                self.neutral_effect_calls.lock().remove(&tag);
                Err(error)
            }
        }
    }
}
