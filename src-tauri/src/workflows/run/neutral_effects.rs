//! Durable adapter effects share the neutral command transaction. This is not
//! an executor or an admission policy: the installed host owns those boundaries.
use super::neutral_execution::{
    NeutralAttemptFence, NeutralDecision, NeutralExecution, NeutralResult, NeutralStatus,
};
use super::neutral_model_tools::{ModelSession, ModelToolCall, ModelToolPolicy};
use super::neutral_runtime::NeutralInvocation;
use crate::workflows::catalog::content_digest;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectPreparation {
    pub mutation: bool,
    pub resources: BTreeSet<String>,
    pub approval: Option<EffectQuestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_tools: Option<ModelToolPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectQuestion {
    pub revision: u64,
    pub title: String,
    pub deadline_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectKey {
    pub fence: NeutralAttemptFence,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum EffectOutcome {
    Pending { handle: String },
    Completed { result: NeutralResult },
    Uncertain { diagnostic: String },
}

/// A completed result does not prove that an owned process stopped. Resources
/// remain held until both certainty and stop evidence have been observed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectObservation {
    pub outcome: EffectOutcome,
    pub stopped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceWait {
    pub resource: String,
    pub owner_run: String,
    pub owner_effect: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MutationStopProof {
    RecordedObservation,
    OriginalProcessGone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MutationResolution {
    pub evidence: String,
    pub stop_proof: MutationStopProof,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralEffect {
    pub key: EffectKey,
    #[serde(with = "invocation_json")]
    pub invocation: NeutralInvocation,
    pub preparation: EffectPreparation,
    pub question: Option<EffectQuestion>,
    pub answer: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_receipt: Option<serde_json::Value>,
    pub dispatched: bool,
    pub cancel_started: bool,
    pub handle: Option<String>,
    pub observation: Option<EffectObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ModelToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_session: Option<ModelSession>,
    /// Operator-approved dispatches after the first, under the same effect key.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reruns: u32,
    // DEFERRED (2026-11-20) — ended runs retain their last resource blocker;
    // consumers must check run lifecycle before presenting it as an active wait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_wait: Option<ResourceWait>,
    /// Explicit operator evidence, separate from the original uncertain result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_resolution: Option<MutationResolution>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl NeutralEffect {
    pub fn prepare(
        invocation: NeutralInvocation,
        preparation: EffectPreparation,
    ) -> Result<Self, String> {
        if let Some(policy) = &preparation.model_tools {
            policy.validate()?;
            if preparation.mutation || !preparation.resources.is_empty() {
                return Err("mediated parent delegates mutations to owned tool effects".into());
            }
        }
        if preparation.mutation && preparation.resources.is_empty() {
            return Err("neutral mutation requires resolved resource ownership".into());
        }
        if preparation
            .resources
            .iter()
            .any(|key| key.trim().is_empty())
        {
            return Err("neutral effect resource is empty".into());
        }
        let digest = content_digest(&(&invocation, &preparation))?;
        Ok(Self {
            key: EffectKey {
                fence: invocation.fence.clone(),
                digest,
            },
            question: preparation.approval.clone(),
            invocation,
            preparation,
            answer: None,
            answer_receipt: None,
            dispatched: false,
            cancel_started: false,
            handle: None,
            observation: None,
            tool: None,
            model_session: None,
            reruns: 0,
            resource_wait: None,
            mutation_resolution: None,
        })
    }

    pub fn prepare_tool(
        invocation: NeutralInvocation,
        preparation: EffectPreparation,
        call: ModelToolCall,
    ) -> Result<Self, String> {
        let mut effect = Self::prepare(invocation, preparation)?;
        effect.key.digest = content_digest(&(&effect.invocation, &effect.preparation, &call))?;
        effect.tool = Some(call);
        Ok(effect)
    }

    pub fn settled(&self) -> bool {
        matches!(
            &self.observation,
            Some(EffectObservation {
                outcome: EffectOutcome::Completed { .. },
                stopped: true
            })
        )
    }

    pub fn resources_released(&self) -> bool {
        self.settled() || self.mutation_resolution.is_some()
    }

    /// A read-only script whose output is lost has nothing to roll back, so the
    /// operator may run the original again. Any mutating effect keeps its
    /// uncertainty and its reconciliation procedure.
    pub fn rerun_candidate(&self) -> bool {
        self.tool.is_none()
            && self.dispatched
            && !self.preparation.mutation
            && !self.cancel_started
            && matches!(
                self.invocation.descriptor.target,
                Some(crate::workflows::catalog::ReusableTarget::Script { .. })
            )
            && matches!(
                &self.observation,
                Some(EffectObservation {
                    outcome: EffectOutcome::Uncertain { .. },
                    ..
                })
            )
    }

    pub fn approved(&self) -> bool {
        self.question.is_none() || self.answer == Some(true)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum EffectCommand {
    ResourceWait {
        key: EffectKey,
        wait: ResourceWait,
    },
    ResolveMutation {
        key: EffectKey,
        evidence: String,
        stop_proof: MutationStopProof,
    },
    BindModel {
        key: EffectKey,
        session: ModelSession,
    },
    ResumeModel {
        key: EffectKey,
        previous: ModelSession,
        session: ModelSession,
        results_digest: String,
    },
    Prepare {
        effect: Box<NeutralEffect>,
    },
    Dispatch {
        key: EffectKey,
    },
    Observe {
        key: EffectKey,
        observation: EffectObservation,
    },
    Question {
        key: EffectKey,
        question: EffectQuestion,
    },
    Answer {
        key: EffectKey,
        revision: u64,
        approved: bool,
    },
    /// Backend-authenticated metadata, committed atomically with the answer.
    AnswerRecorded {
        key: EffectKey,
        revision: u64,
        approved: bool,
        receipt: serde_json::Value,
    },
    Cancel {
        key: EffectKey,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum EffectDecision {
    ResourceWait {
        key: EffectKey,
        wait: ResourceWait,
    },
    ResolveMutation {
        key: EffectKey,
        resolution: MutationResolution,
    },
    BindModel {
        key: EffectKey,
        session: ModelSession,
    },
    ResumeModel {
        key: EffectKey,
        previous: ModelSession,
        session: ModelSession,
        results_digest: String,
    },
    /// A prepared effect whose authority expired before adapter entry.
    Refuse {
        key: EffectKey,
        diagnostic: String,
        result: Option<NeutralDecision>,
    },
    Prepare {
        effect: Box<NeutralEffect>,
    },
    Dispatch {
        key: EffectKey,
    },
    Observe {
        key: EffectKey,
        observation: EffectObservation,
        result: Option<NeutralDecision>,
    },
    Question {
        key: EffectKey,
        question: EffectQuestion,
    },
    Answer {
        key: EffectKey,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        receipt: Option<serde_json::Value>,
        result: Option<NeutralDecision>,
    },
    Cancel {
        key: EffectKey,
    },
}

impl NeutralExecution {
    pub(super) fn mutation_resolution_candidate(
        &self,
        key: &EffectKey,
    ) -> Result<&NeutralEffect, String> {
        let effect = self.effect(key)?;
        if self.status == NeutralStatus::Running
            || !effect.preparation.mutation
            || !effect.dispatched
            || effect.settled()
        {
            return Err(crate::workflows::neutral_api::refusal::MUTATION_RESOLUTION_STATE.into());
        }
        Ok(effect)
    }
    // DEFERRED (2026-11-15) — an Answer naming an unknown effect key still answers
    // HTTP 500 (3076-306f review follow-up). This text is shared with an internal
    // callback identity mismatch, which is a fault, so it is not classified as a
    // 404 by text alone; telling the two apart needs a typed error here.
    pub(super) fn effect(&self, key: &EffectKey) -> Result<&NeutralEffect, String> {
        self.effects
            .get(&key.digest)
            .filter(|effect| effect.key == *key)
            .ok_or_else(|| "neutral effect callback identity or invocation digest mismatch".into())
    }

    pub(super) fn effect_for(&self, fence: &NeutralAttemptFence) -> Option<&NeutralEffect> {
        self.effects
            .values()
            .find(|effect| effect.tool.is_none() && effect.key.fence == *fence)
    }

    pub(super) fn effect_attempt_pending(&self, key: &EffectKey) -> bool {
        if self.status != NeutralStatus::Running {
            return false;
        }
        if key.fence.execution != self.execution {
            return self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.effect_attempt_pending(key));
        }
        self.pending_attempt(&key.fence).is_ok()
    }

    fn callback_decision(
        &self,
        key: &EffectKey,
        result: &NeutralResult,
        at_ms: i64,
    ) -> Result<Option<NeutralDecision>, String> {
        let disposition = super::native_issue_publication::refusal(self.effect(key)?, result)?;
        let result = disposition.as_ref().unwrap_or(result);
        let (key, result) = if let Some(call) = &self.effect(key)?.tool {
            if !self.owned_tools_settled(&call.parent, Some(key)) {
                return Ok(None);
            }
            match &self.effect(&call.parent)?.observation {
                Some(EffectObservation {
                    outcome: EffectOutcome::Completed { result },
                    stopped: true,
                }) => (&call.parent, result),
                _ => return Ok(None),
            }
        } else {
            if !self.owned_tools_settled(key, None) {
                return Ok(None);
            }
            (key, result)
        };
        if !self.effect_attempt_pending(key) || at_ms >= self.deadline_ms {
            // The effect ledger outlives a graph frame. Valid result/stop
            // evidence still settles ownership without reopening that frame.
            return Ok(None);
        }
        let command = match result {
            NeutralResult::Completed { outcome, outputs } => {
                super::neutral_execution::NeutralExecutionCommand::Complete {
                    fence: key.fence.clone(),
                    outcome: outcome.clone(),
                    outputs: outputs.clone(),
                }
            }
            NeutralResult::Failed { diagnostic } => {
                super::neutral_execution::NeutralExecutionCommand::Fail {
                    fence: key.fence.clone(),
                    diagnostic: diagnostic.clone(),
                }
            }
        };
        self.decide_inner_mode(&command, at_ms, true)
    }

    pub(super) fn decide_effect(
        &self,
        command: &EffectCommand,
        at_ms: i64,
    ) -> Result<Option<EffectDecision>, String> {
        match command {
            EffectCommand::ResourceWait { key, wait } => {
                let effect = self.effect(key)?;
                if !effect.preparation.mutation
                    || !effect.preparation.resources.contains(&wait.resource)
                    || wait.owner_run.trim().is_empty()
                    || wait.owner_effect.trim().is_empty()
                    || (wait.owner_run == key.fence.run && wait.owner_effect == key.digest)
                    || !matches!(
                        self.decide_effect(&EffectCommand::Dispatch { key: key.clone() }, at_ms)?,
                        Some(EffectDecision::Dispatch { .. })
                    )
                {
                    return Err(
                        "resource wait requires a dispatchable mutation and a conflicting owner"
                            .into(),
                    );
                }
                if effect.resource_wait.as_ref() == Some(wait) {
                    return Ok(None);
                }
                Ok(Some(EffectDecision::ResourceWait {
                    key: key.clone(),
                    wait: wait.clone(),
                }))
            }
            EffectCommand::ResolveMutation {
                key,
                evidence,
                stop_proof,
            } => {
                let effect = self.mutation_resolution_candidate(key)?;
                validate_mutation_resolution(evidence)?;
                let proven = match stop_proof {
                    MutationStopProof::RecordedObservation => {
                        effect.observation.as_ref().is_some_and(|o| o.stopped)
                    }
                    MutationStopProof::OriginalProcessGone => matches!(
                        effect.invocation.descriptor.target,
                        Some(crate::workflows::catalog::ReusableTarget::Script { .. })
                    ),
                };
                if !proven {
                    return Err(format!(
                        "{}: effect {} has no backend stop proof; stop the original operation and reconcile using the retained procedure",
                        crate::workflows::neutral_api::refusal::MUTATION_RESOLUTION_STATE,
                        key.digest
                    ));
                }
                let resolution = MutationResolution {
                    evidence: evidence.clone(),
                    stop_proof: stop_proof.clone(),
                };
                if let Some(original) = &effect.mutation_resolution {
                    return if original == &resolution {
                        Ok(None)
                    } else {
                        Err(
                            crate::workflows::neutral_api::refusal::MUTATION_RESOLUTION_IMMUTABLE
                                .into(),
                        )
                    };
                }
                Ok(Some(EffectDecision::ResolveMutation {
                    key: key.clone(),
                    resolution,
                }))
            }
            EffectCommand::ResumeModel {
                key,
                previous,
                session,
                results_digest,
            } => {
                self.validate_model_resume(key, previous, session, results_digest, at_ms)?;
                Ok(Some(EffectDecision::ResumeModel {
                    key: key.clone(),
                    previous: previous.clone(),
                    session: session.clone(),
                    results_digest: results_digest.clone(),
                }))
            }
            EffectCommand::BindModel { key, session } => {
                session.validate()?;
                let effect = self.effect(key)?;
                if effect.model_session.as_ref() == Some(session) {
                    return Ok(None);
                }
                if effect.model_session.is_some()
                    || effect.tool.is_some()
                    || !effect.dispatched
                    || effect.preparation.model_tools.is_none()
                    || !self.effect_attempt_pending(key)
                    || at_ms >= self.deadline_ms
                    || effect
                        .observation
                        .as_ref()
                        .is_some_and(|o| matches!(o.outcome, EffectOutcome::Completed { .. }))
                {
                    return Err("model session binding is unavailable or immutable".into());
                }
                Ok(Some(EffectDecision::BindModel {
                    key: key.clone(),
                    session: session.clone(),
                }))
            }
            EffectCommand::Prepare { effect } => {
                if effect.tool.is_some() {
                    let expected = NeutralEffect::prepare_tool(
                        effect.invocation.clone(),
                        effect.preparation.clone(),
                        effect.tool.clone().ok_or("model tool identity missing")?,
                    )?;
                    if expected != **effect {
                        return Err("model tool intent must precede dispatch".into());
                    }
                    if self
                        .effects
                        .get(&effect.key.digest)
                        .is_some_and(|old| old.key == effect.key)
                    {
                        return Ok(None);
                    }
                    if !self.effect_attempt_pending(&effect.key) || at_ms >= self.deadline_ms {
                        return Err("model tool cannot prepare on an ended parent".into());
                    }
                    self.validate_tool_intent(effect)?;
                    return Ok(Some(EffectDecision::Prepare {
                        effect: effect.clone(),
                    }));
                }
                if self.status != NeutralStatus::Running || at_ms >= self.deadline_ms {
                    return Err("neutral effect cannot prepare on an ended run".into());
                }
                let (_, attempt) = self.pending_attempt(&effect.key.fence)?;
                if attempt.dispatched || self.effect_for(&effect.key.fence).is_some() {
                    return Err("neutral effect intent is immutable".into());
                }
                let expected =
                    NeutralEffect::prepare(effect.invocation.clone(), effect.preparation.clone())?;
                if expected != **effect
                    || effect.invocation.inputs != attempt.inputs
                    || effect.invocation.subject != attempt.subject
                {
                    return Err(
                        "neutral effect preparation does not match resolved invocation".into(),
                    );
                }
                // Descriptor, configuration and publication are retained, not
                // whatever the current catalog happens to contain.
                let (activity, _) = self.pending_attempt(&effect.key.fence)?;
                let mut instance = self.publication.definition.instances[activity].clone();
                instance.config = self
                    .descriptor(activity)?
                    .config
                    .resolve(&instance.config, "config")?;
                if effect.invocation.descriptor != *self.descriptor(activity)?
                    || effect.invocation.instance != instance
                    || effect.invocation.publication != self.publication
                {
                    return Err(
                        "neutral effect preparation does not match retained contract".into(),
                    );
                }
                if let Some(question) = &effect.question {
                    validate_question(question, at_ms, self.deadline_ms)?;
                }
                Ok(Some(EffectDecision::Prepare {
                    effect: effect.clone(),
                }))
            }
            EffectCommand::Dispatch { key } => {
                let effect = self.effect(key)?;
                super::native_issue_publication::admit_mutation(self, effect)?;
                self.pending_attempt(&key.fence)?;
                if self.status != NeutralStatus::Running
                    || at_ms >= self.deadline_ms
                    || effect.dispatched
                    || !effect.approved()
                {
                    return Err(
                        "neutral effect dispatch is not admitted by current lifecycle".into(),
                    );
                }
                if effect
                    .question
                    .as_ref()
                    .is_some_and(|q| at_ms >= q.deadline_ms)
                {
                    let diagnostic = "neutral effect approval expired before dispatch".to_owned();
                    let result = self.callback_decision(
                        key,
                        &NeutralResult::Failed {
                            diagnostic: diagnostic.clone(),
                        },
                        at_ms,
                    )?;
                    return Ok(Some(EffectDecision::Refuse {
                        key: key.clone(),
                        diagnostic,
                        result,
                    }));
                }
                if effect.tool.is_none() && !self.dispatch_capacity() {
                    return Err("neutral effect dispatch has no current capacity".into());
                }
                if let Some(call) = &effect.tool
                    && !self.model_tool_capacity(call)?
                {
                    return Err("model tool dispatch has no current capacity".into());
                }
                Ok(Some(EffectDecision::Dispatch { key: key.clone() }))
            }
            EffectCommand::Observe { key, observation } => {
                let effect = self.effect(key)?;
                // A late callback cannot rewrite an explicit terminal disposition.
                if effect.mutation_resolution.is_some() {
                    return Ok(None);
                }
                if !effect.dispatched {
                    return Err("neutral effect was not dispatched".into());
                }
                let mut observation = observation.clone();
                if effect.invocation.descriptor.handler.name
                    == super::native_issue_publication::HANDLER
                    && let Some(EffectObservation {
                        outcome:
                            EffectOutcome::Uncertain {
                                diagnostic: original,
                            },
                        stopped,
                    }) = &effect.observation
                {
                    match &observation.outcome {
                        EffectOutcome::Pending { .. } => return Ok(None),
                        EffectOutcome::Uncertain { diagnostic } if diagnostic == original && (!*stopped || observation.stopped) => {},
                        _ => return Err("original issue publication uncertainty is immutable; no result relabelled".into()),
                    }
                }
                match &mut observation.outcome {
                    EffectOutcome::Pending { handle } => {
                        if handle.trim().is_empty() || observation.stopped {
                            return Err("neutral pending observation needs a live handle".into());
                        }
                        if effect.handle.as_ref().is_some_and(|old| old != handle) {
                            return Err("neutral effect handle is immutable".into());
                        }
                        // A callback may beat the pending acknowledgment. Never
                        // replace a result or uncertainty with that older ack.
                        if effect.observation.as_ref().is_some_and(|old| {
                            !matches!(old.outcome, EffectOutcome::Pending { .. })
                        }) {
                            return Ok(None);
                        }
                    }
                    EffectOutcome::Completed { result } => {
                        match result {
                            NeutralResult::Completed { outcome, outputs } => {
                                let descriptor = &effect.invocation.descriptor;
                                if !descriptor.outcomes.contains(outcome) {
                                    return Err(
                                        "neutral effect returned an undeclared outcome".into()
                                    );
                                }
                                *outputs = descriptor
                                    .outputs
                                    .resolve(outputs, "neutral effect result")?;
                            }
                            NeutralResult::Failed { diagnostic }
                                if diagnostic.trim().is_empty() =>
                            {
                                return Err("neutral effect failure needs a diagnostic".into());
                            }
                            _ => {}
                        }
                        if let Some(EffectObservation {
                            outcome: EffectOutcome::Completed { result: old },
                            stopped,
                        }) = &effect.observation
                            && (old != result || (*stopped && !observation.stopped))
                        {
                            return Err("neutral effect result is immutable".into());
                        }
                    }
                    EffectOutcome::Uncertain { diagnostic } => {
                        if diagnostic.trim().is_empty() {
                            return Err("neutral effect uncertainty needs a diagnostic".into());
                        }
                        if effect.observation.as_ref().is_some_and(|old| {
                            matches!(old.outcome, EffectOutcome::Completed { .. })
                        }) {
                            return Err("neutral effect result is immutable".into());
                        }
                    }
                }
                if effect.observation.as_ref() == Some(&observation) {
                    return Ok(None);
                }
                let result = match &observation {
                    EffectObservation {
                        outcome: EffectOutcome::Completed { result },
                        stopped: true,
                    } if !effect.settled() => self.callback_decision(key, result, at_ms)?,
                    _ => {
                        match super::native_issue_publication::uncertainty(effect, &observation)? {
                            Some(disposition) => {
                                self.callback_decision(key, &disposition, at_ms)?
                            }
                            None => None,
                        }
                    }
                };
                Ok(Some(EffectDecision::Observe {
                    key: key.clone(),
                    observation,
                    result,
                }))
            }
            EffectCommand::Question { key, question } => {
                let effect = self.effect(key)?;
                if (effect.dispatched && !effect.rerun_candidate())
                    || effect.answer == Some(false)
                    || !self.effect_attempt_pending(key)
                {
                    return Err(
                        "neutral question cannot revise a dispatched or resolved effect".into(),
                    );
                }
                validate_question(question, at_ms, self.deadline_ms)?;
                if effect
                    .question
                    .as_ref()
                    .is_some_and(|old| question.revision <= old.revision)
                {
                    return Err("neutral question revision must increase".into());
                }
                Ok(Some(EffectDecision::Question {
                    key: key.clone(),
                    question: question.clone(),
                }))
            }
            EffectCommand::AnswerRecorded {
                key,
                revision,
                approved,
                receipt,
            } => {
                if !receipt.is_object()
                    || serde_json::to_vec(receipt)
                        .map_err(|e| e.to_string())?
                        .len()
                        > 32768
                {
                    return Err("neutral answer receipt must be a bounded object".into());
                }
                let decision = self.decide_effect(
                    &EffectCommand::Answer {
                        key: key.clone(),
                        revision: *revision,
                        approved: *approved,
                    },
                    at_ms,
                )?;
                match decision {
                    Some(EffectDecision::Answer {
                        key,
                        approved,
                        result,
                        ..
                    }) => Ok(Some(EffectDecision::Answer {
                        key,
                        approved,
                        result,
                        receipt: Some(receipt.clone()),
                    })),
                    _ => Err("neutral answer decision unavailable".into()),
                }
            }
            EffectCommand::Answer {
                key,
                revision,
                approved,
            } => {
                let effect = self.effect(key)?;
                let question = effect
                    .question
                    .as_ref()
                    .ok_or("neutral effect has no question")?;
                if !self.effect_attempt_pending(key)
                    || at_ms >= self.deadline_ms
                    || at_ms >= question.deadline_ms
                    || question.revision != *revision
                    || effect.answer.is_some()
                    || (effect.dispatched && !effect.rerun_candidate())
                {
                    return Err(crate::workflows::neutral_api::refusal::APPROVAL_STALE.into());
                }
                let result = if *approved {
                    None
                } else {
                    let diagnostic = if effect.dispatched || effect.reruns > 0 {
                        "script output was lost and the operator abandoned the activity"
                    } else {
                        "neutral effect approval declined"
                    };
                    self.callback_decision(
                        key,
                        &NeutralResult::Failed {
                            diagnostic: diagnostic.into(),
                        },
                        at_ms,
                    )?
                };
                Ok(Some(EffectDecision::Answer {
                    key: key.clone(),
                    approved: *approved,
                    receipt: None,
                    result,
                }))
            }
            EffectCommand::Cancel { key } => {
                let effect = self.effect(key)?;
                if self.status == NeutralStatus::Running && at_ms < self.deadline_ms {
                    return Err("neutral effect stop requires an ended run".into());
                }
                if !effect.dispatched || effect.settled() || effect.cancel_started {
                    return Ok(None);
                }
                Ok(Some(EffectDecision::Cancel { key: key.clone() }))
            }
        }
    }

    pub(super) fn apply_effect(&mut self, decision: &EffectDecision) -> Result<(), String> {
        match decision {
            EffectDecision::ResourceWait { key, wait } => {
                self.effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?
                    .resource_wait = Some(wait.clone());
            }
            EffectDecision::ResolveMutation { key, resolution } => {
                self.effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?
                    .mutation_resolution = Some(resolution.clone());
            }
            EffectDecision::BindModel { key, session }
            | EffectDecision::ResumeModel { key, session, .. } => {
                self.effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?
                    .model_session = Some(session.clone());
            }
            EffectDecision::Refuse {
                key,
                diagnostic,
                result,
            } => {
                // No adapter entry occurred, so failure and stop are known.
                let effect = self
                    .effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?;
                effect.resource_wait = None;
                effect.observation = Some(EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Failed {
                            diagnostic: diagnostic.clone(),
                        },
                    },
                });
                if let Some(result) = result {
                    self.apply(result)?;
                }
            }
            EffectDecision::Prepare { effect } => {
                self.effects
                    .insert(effect.key.digest.clone(), *effect.clone());
            }
            EffectDecision::Dispatch { key } => {
                self.attempt_mut(&key.fence)?.dispatched = true;
                let effect = self
                    .effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?;
                effect.dispatched = true;
                effect.resource_wait = None;
            }
            EffectDecision::Observe {
                key,
                observation,
                result,
            } => {
                let effect = self
                    .effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?;
                if let EffectOutcome::Pending { handle } = &observation.outcome {
                    effect.handle = Some(handle.clone());
                }
                effect.observation = Some(observation.clone());
                if let Some(result) = result {
                    self.apply(result)?;
                }
            }
            EffectDecision::Question { key, question } => {
                let effect = self
                    .effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?;
                effect.question = Some(question.clone());
                effect.answer = None;
                effect.answer_receipt = None;
            }
            EffectDecision::Answer {
                key,
                approved,
                receipt,
                result,
            } => {
                let effect = self
                    .effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?;
                effect.answer = Some(*approved);
                effect.answer_receipt = receipt.clone();
                // Approval of a lost read-only script re-opens the original
                // dispatch; the same key and the same retained invocation run again.
                let rerun = *approved && effect.dispatched;
                if rerun {
                    effect.dispatched = false;
                    effect.observation = None;
                    effect.handle = None;
                    effect.reruns += 1;
                    self.attempt_mut(&key.fence)?.dispatched = false;
                }
                if let Some(result) = result {
                    self.apply(result)?;
                }
            }
            EffectDecision::Cancel { key } => {
                self.effects
                    .get_mut(&key.digest)
                    .ok_or("neutral effect unavailable")?
                    .cancel_started = true;
            }
        }
        Ok(())
    }
}

fn validate_question(
    question: &EffectQuestion,
    at_ms: i64,
    deadline_ms: i64,
) -> Result<(), String> {
    if question.revision == 0
        || question.title.trim().is_empty()
        || question.deadline_ms <= at_ms
        || question.deadline_ms > deadline_ms
    {
        return Err("neutral approval requires a bounded current question revision".into());
    }
    Ok(())
}

pub(super) fn validate_mutation_resolution(evidence: &str) -> Result<(), String> {
    if evidence.trim().is_empty() || evidence.len() > 4096 {
        return Err(crate::workflows::neutral_api::refusal::MUTATION_RESOLUTION_EVIDENCE.into());
    }
    Ok(())
}

/// This is only the transactionally maintained conflict projection of the
/// existing effect ledger, not a work queue. Adapter-owned claims still apply.
pub(super) fn project_resources(
    conn: &Connection,
    execution: &NeutralExecution,
    decision: &EffectDecision,
) -> Result<(), String> {
    match decision {
        EffectDecision::Dispatch { key } => {
            let effect = execution.effect(key)?;
            if effect.preparation.mutation {
                for resource in &effect.preparation.resources {
                    conn.execute("INSERT OR IGNORE INTO neutral_effect_resources(resource,run_id,effect_id) VALUES (?1,?2,?3)", params![resource, key.fence.run, key.digest])
                        .map_err(|e| format!("claim neutral resource: {e}"))?;
                }
            }
        }
        EffectDecision::Observe { key, .. } | EffectDecision::ResolveMutation { key, .. }
            if execution.effect(key)?.resources_released() =>
        {
            conn.execute(
                "DELETE FROM neutral_effect_resources WHERE run_id=?1 AND effect_id=?2",
                params![key.fence.run, key.digest],
            )
            .map_err(|e| format!("release neutral resource: {e}"))?;
        }
        _ => {}
    }
    Ok(())
}

/// Read under the command's IMMEDIATE transaction, before any claim or event.
/// A wait is journalled atomically with the exact owner that prevented dispatch.
pub(super) fn resource_conflict(
    conn: &Connection,
    execution: &NeutralExecution,
    key: &EffectKey,
) -> Result<Option<ResourceWait>, String> {
    let effect = execution.effect(key)?;
    if !effect.preparation.mutation {
        return Ok(None);
    }
    for resource in &effect.preparation.resources {
        let owner: Option<(String, String)> = conn
            .query_row(
                "SELECT run_id,effect_id FROM neutral_effect_resources WHERE resource=?1",
                [resource],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("read neutral resource: {e}"))?;
        if let Some((owner_run, owner_effect)) = owner
            && (owner_run != key.fence.run || owner_effect != key.digest)
            && !super::native_issue_publication::shares_unknown(
                execution,
                effect,
                &owner_run,
                &owner_effect,
            )
        {
            return Ok(Some(ResourceWait {
                resource: resource.clone(),
                owner_run,
                owner_effect,
            }));
        }
    }
    Ok(None)
}

pub(super) type Effects = BTreeMap<String, NeutralEffect>;

// Preserve JSON map-key decoding inside internally tagged event envelopes.
// Serde's buffered Content cannot deserialize JSON string keys into usize.
mod invocation_json {
    use super::NeutralInvocation;
    use serde::{Deserialize, Serialize};
    pub fn serialize<S: serde::Serializer>(
        value: &NeutralInvocation,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<NeutralInvocation, D::Error> {
        serde_json::from_value(serde_json::Value::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
