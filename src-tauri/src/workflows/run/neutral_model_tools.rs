//! Named host-tool ownership in the existing neutral effect ledger.
use super::neutral_effects::{EffectKey, NeutralEffect};
use super::neutral_execution::NeutralExecution;
use super::neutral_runtime::NeutralInvocation;
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelToolBinding {
    pub descriptor: ActivityDescriptor,
    pub instance: ActivityInstance,
}

/// Installed by the trusted host from the retained model activity contract.
/// These are exact operations, not capability labels or permission grants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelToolPolicy {
    pub tools: BTreeMap<String, ModelToolBinding>,
    pub max_calls: u32,
    pub max_concurrency: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelSession {
    pub connection: String,
    pub session: String,
    pub generation: u64,
}

impl ModelSession {
    pub fn validate(&self) -> Result<(), String> {
        if self.connection.is_empty()
            || self.connection.len() > 256
            || self.session.is_empty()
            || self.session.len() > 256
            || self.generation == 0
        {
            return Err("model session requires exact connection/session/generation".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelToolCall {
    pub parent: EffectKey,
    pub session: ModelSession,
    pub id: String,
    pub name: String,
}

impl ModelToolPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_calls == 0
            || self.max_calls > 10_000
            || self.tools.len() > 128
            || self.max_concurrency == 0
            || self.max_concurrency > self.max_calls
        {
            return Err("model tool policy requires bounded tool and call limits".into());
        }
        for (name, binding) in &self.tools {
            if name.is_empty()
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
                || binding.descriptor.target.is_some()
                || binding.instance.descriptor != binding.descriptor.id
                || !binding
                    .descriptor
                    .execution_policies
                    .contains(&binding.instance.execution_policy)
                || binding
                    .descriptor
                    .config
                    .resolve(&binding.instance.config, "model tool config")?
                    != binding.instance.config
            {
                return Err("model tool binding must retain an exact host operation/config".into());
            }
        }
        Ok(())
    }
}

impl NeutralExecution {
    pub(super) fn model_tool_capacity(&self, call: &ModelToolCall) -> Result<bool, String> {
        let parent = self.effect(&call.parent)?;
        let policy = parent
            .preparation
            .model_tools
            .as_ref()
            .ok_or("model tool policy missing")?;
        let running = self
            .effects
            .values()
            .filter(|effect| {
                effect
                    .tool
                    .as_ref()
                    .is_some_and(|old| old.parent == call.parent)
                    && effect.dispatched
                    && !effect.settled()
                    && !matches!(
                        effect.observation.as_ref().map(|o| &o.outcome),
                        Some(super::neutral_effects::EffectOutcome::Uncertain { .. })
                    )
            })
            .count();
        Ok(running < policy.max_concurrency as usize)
    }

    pub(super) fn validate_tool_intent(&self, effect: &NeutralEffect) -> Result<(), String> {
        let call = effect.tool.as_ref().ok_or("model tool identity missing")?;
        call.session.validate()?;
        let parent = self.effect(&call.parent)?;
        let policy = parent
            .preparation
            .model_tools
            .as_ref()
            .ok_or("parent has no named model tools")?;
        let binding = policy
            .tools
            .get(&call.name)
            .ok_or("model tool is not in the retained host catalogue")?;
        if call.id.is_empty()
            || call.id.len() > 1_024
            || parent.tool.is_some()
            || !parent.dispatched
            || parent.observation.as_ref().is_some_and(|o| {
                matches!(
                    o.outcome,
                    super::neutral_effects::EffectOutcome::Completed { .. }
                )
            })
            || parent.model_session.as_ref() != Some(&call.session)
            || call.parent.fence != effect.key.fence
            || effect.preparation.model_tools.is_some()
            || effect.invocation.publication != parent.invocation.publication
            || effect.invocation.subject != parent.invocation.subject
            || effect.invocation.descriptor != binding.descriptor
            || effect.invocation.instance != binding.instance
            || binding
                .descriptor
                .inputs
                .resolve(&effect.invocation.inputs, "model tool arguments")?
                != effect.invocation.inputs
        {
            return Err(
                "model tool intent differs from its original parent/session/contract".into(),
            );
        }
        if let Some(old) = self.effects.values().find(|old| {
            old.tool
                .as_ref()
                .is_some_and(|old| old.parent == call.parent && old.id == call.id)
        }) {
            if old.key != effect.key {
                return Err("model tool call identity cannot change arguments or operation".into());
            }
        } else if self
            .effects
            .values()
            .filter(|old| {
                old.tool
                    .as_ref()
                    .is_some_and(|old| old.parent == call.parent)
            })
            .count()
            >= policy.max_calls as usize
        {
            return Err("model tool call limit reached".into());
        }
        Ok(())
    }

    pub(super) fn owned_tools_settled(
        &self,
        key: &EffectKey,
        settling: Option<&EffectKey>,
    ) -> bool {
        self.effects
            .values()
            .filter(|effect| effect.tool.as_ref().is_some_and(|call| call.parent == *key))
            .all(|effect| Some(&effect.key) == settling || effect.settled())
    }

    /// Resolve the original retained call, including its immutable result, without
    /// asking a model to regenerate arguments or dispatching the service again.
    pub(super) fn model_tool_call(
        &self,
        call: &ModelToolCall,
    ) -> Result<Option<&NeutralEffect>, String> {
        let parent = self.effect(&call.parent)?;
        if parent.model_session.as_ref() != Some(&call.session) {
            return Err("model tool callback belongs to a different session generation".into());
        }
        let found = self.effects.values().find(|effect| {
            effect
                .tool
                .as_ref()
                .is_some_and(|old| old.parent == call.parent && old.id == call.id)
        });
        if found.is_some_and(|effect| effect.tool.as_ref() != Some(call)) {
            return Err("model tool call identity cannot change operation".into());
        }
        Ok(found)
    }

    pub(super) fn tool_invocation(
        &self,
        call: &ModelToolCall,
        inputs: BTreeMap<String, serde_json::Value>,
    ) -> Result<NeutralInvocation, String> {
        let parent = self.effect(&call.parent)?;
        if parent.model_session.as_ref() != Some(&call.session) {
            return Err("model tool request belongs to a different session generation".into());
        }
        let binding = parent
            .preparation
            .model_tools
            .as_ref()
            .and_then(|policy| policy.tools.get(&call.name))
            .ok_or("model tool is not in the retained host catalogue")?;
        Ok(NeutralInvocation {
            fence: parent.key.fence.clone(),
            publication: parent.invocation.publication.clone(),
            descriptor: binding.descriptor.clone(),
            instance: binding.instance.clone(),
            inputs: binding
                .descriptor
                .inputs
                .resolve(&inputs, "model tool arguments")?,
            subject: parent.invocation.subject.clone(),
        })
    }
}
