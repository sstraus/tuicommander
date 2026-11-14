//! Neutral execution decisions in the existing owner's event ledger.
// Internal runtime surface; public neutral transport and native adapters follow in later stories.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "004 execution before actor integration")
)]

use super::neutral::{CONTRACT_VERSION, NeutralEvent, NeutralEventKind, NeutralRun};
use super::neutral_scopes::{NeutralScope, NeutralScopeFence, ScopeDecision};
use super::runtime::RuntimeOwner;
use super::store::{RunStore, now_ms};
use crate::workflows::catalog::{ActivityDescriptor, content_digest};
use crate::workflows::data_binding::{
    DataOrigin, DataState, Datum, ResolvedInputs, resolve_bindings,
};
use crate::workflows::neutral_control::ControlNode;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralAttemptFence {
    pub run: String,
    pub execution: String,
    pub activation: u32,
    pub attempt: u32,
    pub scope: Option<NeutralScopeFence>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NeutralExecutionCommand {
    Effect {
        command: super::neutral_effects::EffectCommand,
    },
    Step,
    EnterSubflow {
        fence: NeutralAttemptFence,
    },
    Dispatch {
        fence: NeutralAttemptFence,
    },
    Complete {
        fence: NeutralAttemptFence,
        outcome: String,
        outputs: BTreeMap<String, Value>,
    },
    Fail {
        fence: NeutralAttemptFence,
        diagnostic: String,
    },
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NeutralDecision {
    Effect {
        decision: Box<super::neutral_effects::EffectDecision>,
    },
    EnterSubflow {
        fence: NeutralAttemptFence,
    },
    Failure {
        diagnostic: String,
    },
    Dispatch {
        fence: NeutralAttemptFence,
    },
    Route {
        outcome: String,
        provenance: BTreeSet<DataOrigin>,
    },
    Prepare {
        fence: NeutralAttemptFence,
        inputs: BTreeMap<String, Value>,
        provenance: BTreeMap<String, BTreeSet<DataOrigin>>,
        subject: Option<BTreeMap<String, Value>>,
    },
    Complete {
        fence: NeutralAttemptFence,
        result: NeutralResult,
    },
    Scope {
        action: ScopeDecision,
    },
    ArmWait {
        due_ms: i64,
    },
    Finish {
        outcome: String,
        outputs: BTreeMap<String, Value>,
        provenance: BTreeMap<String, BTreeSet<DataOrigin>>,
    },
    Cancel,
    Expire,
    Limit {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NeutralStatus {
    Running,
    Completed { outcome: String },
    Cancelled,
    Expired,
    Limited { reason: String },
    Failed { diagnostic: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NeutralResult {
    Completed {
        outcome: String,
        outputs: BTreeMap<String, Value>,
    },
    Failed {
        diagnostic: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralAttempt {
    pub fence: NeutralAttemptFence,
    pub inputs: BTreeMap<String, Value>,
    pub provenance: BTreeMap<String, BTreeSet<DataOrigin>>,
    pub subject: Option<BTreeMap<String, Value>>,
    pub result: Option<NeutralResult>,
    pub dispatched: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct NeutralExecution {
    pub run: NeutralRun,
    pub effects: super::neutral_effects::Effects,
    pub publication: crate::workflows::neutral_publication::NeutralPublication,
    pub inputs: BTreeMap<String, Value>,
    pub subject: Option<BTreeMap<String, Value>>,
    pub execution: String,
    pub parent: Option<Box<NeutralExecution>>,
    pub parent_fence: Option<NeutralAttemptFence>,
    pub sequence: i64,
    pub deadline_ms: i64,
    pub status: NeutralStatus,
    pub node: String,
    pub activation: u32,
    pub activation_count: u32,
    pub active_scope: Option<NeutralScope>,
    pub scope_values: crate::workflows::scope_data::ScopeValues,
    pub attempt: Option<NeutralAttempt>,
    pub wait_due_ms: Option<i64>,
    pub activity_outputs: BTreeMap<usize, BTreeMap<String, Datum>>,
    pub outputs: Option<ResolvedInputs>,
}

impl NeutralExecution {
    fn started(run: NeutralRun, at_ms: i64) -> Result<Self, String> {
        let deadline_ms = checked_deadline(at_ms, run.request.limits.max_duration_ms)?;
        let node = run.closure.root.definition.control.entry()?.to_owned();
        Ok(Self {
            effects: BTreeMap::new(),
            publication: run.closure.root.clone(),
            inputs: run.inputs.clone(),
            subject: run.subject.clone(),
            execution: "root".into(),
            parent: None,
            parent_fence: None,
            run,
            sequence: 1,
            deadline_ms,
            status: NeutralStatus::Running,
            node,
            activation: 1,
            activation_count: 1,
            active_scope: None,
            scope_values: BTreeMap::new(),
            attempt: None,
            wait_due_ms: None,
            activity_outputs: BTreeMap::new(),
            outputs: None,
        })
    }

    pub(super) fn workflow_inputs(&self) -> BTreeMap<String, Datum> {
        self.inputs
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    Datum {
                        state: DataState::Present(value.clone()),
                        provenance: BTreeSet::from([DataOrigin::WorkflowInput {
                            field: name.clone(),
                        }]),
                    },
                )
            })
            .collect()
    }

    pub(super) fn descriptor(&self, activity: usize) -> Result<&ActivityDescriptor, String> {
        let instance = self
            .publication
            .definition
            .instances
            .get(activity)
            .ok_or("neutral activity unavailable")?;
        self.run
            .closure
            .descriptors
            .iter()
            .find(|d| d.id == instance.descriptor)
            .ok_or_else(|| "retained neutral descriptor unavailable".into())
    }

    pub(super) fn current_node(&self) -> Result<&ControlNode, String> {
        self.publication
            .definition
            .control
            .nodes
            .get(&self.node)
            .ok_or_else(|| "neutral control cursor unavailable".into())
    }

    pub(super) fn decide(
        &self,
        command: &NeutralExecutionCommand,
        at_ms: i64,
    ) -> Result<Option<NeutralDecision>, String> {
        match self.decide_inner(command, at_ms) {
            Err(diagnostic)
                if matches!(command, NeutralExecutionCommand::Step)
                    && self.status == NeutralStatus::Running =>
            {
                Ok(Some(NeutralDecision::Failure { diagnostic }))
            }
            result => result,
        }
    }

    fn decide_inner(
        &self,
        command: &NeutralExecutionCommand,
        at_ms: i64,
    ) -> Result<Option<NeutralDecision>, String> {
        self.decide_inner_mode(command, at_ms, false)
    }

    pub(super) fn decide_inner_mode(
        &self,
        command: &NeutralExecutionCommand,
        at_ms: i64,
        correlated: bool,
    ) -> Result<Option<NeutralDecision>, String> {
        if let NeutralExecutionCommand::Complete { fence, .. }
        | NeutralExecutionCommand::Fail { fence, .. }
        | NeutralExecutionCommand::Dispatch { fence }
        | NeutralExecutionCommand::EnterSubflow { fence } = command
            && !correlated
            && self.effect_for(fence).is_some()
        {
            return Err("neutral effect result requires exact callback correlation".into());
        }
        if let NeutralExecutionCommand::Effect { command } = command {
            return self.decide_effect(command, at_ms).map(|decision| {
                decision.map(|decision| NeutralDecision::Effect {
                    decision: Box::new(decision),
                })
            });
        }
        if self.status != NeutralStatus::Running {
            return Err("neutral run is terminal".into());
        }
        if matches!(command, NeutralExecutionCommand::Cancel) {
            return Ok(Some(NeutralDecision::Cancel));
        }
        if at_ms >= self.deadline_ms {
            return match command {
                NeutralExecutionCommand::Step => Ok(Some(NeutralDecision::Expire)),
                _ => Err("neutral run deadline elapsed".into()),
            };
        }
        if let NeutralExecutionCommand::Complete { fence, .. }
        | NeutralExecutionCommand::Fail { fence, .. } = command
            && fence.execution != self.execution
        {
            return self
                .parent
                .as_ref()
                .ok_or("neutral callback execution unavailable")?
                .decide_inner_mode(command, at_ms, correlated);
        }
        let node = self.current_node()?;
        match command {
            NeutralExecutionCommand::EnterSubflow { fence } => {
                let (activity, attempt) = self.pending_attempt(fence)?;
                if attempt.dispatched {
                    return Err("neutral attempt was already dispatched".into());
                }
                self.subflow_publication(activity)?;
                if self.activation_count >= self.run.request.limits.max_activations {
                    return Ok(Some(self.activation_limit()));
                }
                return Ok(Some(NeutralDecision::EnterSubflow {
                    fence: fence.clone(),
                }));
            }
            NeutralExecutionCommand::Dispatch { fence } => {
                let (_, attempt) = self.pending_attempt(fence)?;
                if attempt.dispatched {
                    return Err("neutral attempt was already dispatched".into());
                }
                return Ok(Some(NeutralDecision::Dispatch {
                    fence: fence.clone(),
                }));
            }
            NeutralExecutionCommand::Complete {
                fence,
                outcome,
                outputs,
            } => {
                let (activity, _) = self.pending_attempt(fence)?;
                let descriptor = self.descriptor(activity)?;
                if !descriptor.outcomes.contains(outcome) {
                    return Err("neutral activity returned an undeclared outcome".into());
                }
                let outputs = descriptor
                    .outputs
                    .resolve(outputs, "neutral activity result")?;
                return Ok(Some(NeutralDecision::Complete {
                    fence: fence.clone(),
                    result: NeutralResult::Completed {
                        outcome: outcome.clone(),
                        outputs,
                    },
                }));
            }
            NeutralExecutionCommand::Fail { fence, diagnostic } => {
                self.pending_attempt(fence)?;
                if diagnostic.trim().is_empty() {
                    return Err("neutral failure needs a diagnostic".into());
                }
                return Ok(Some(NeutralDecision::Complete {
                    fence: fence.clone(),
                    result: NeutralResult::Failed {
                        diagnostic: diagnostic.clone(),
                    },
                }));
            }
            _ => {}
        }
        let route = |outcome: &str| Some(self.route(outcome, BTreeSet::new()));
        match node {
            ControlNode::Start => Ok(route("next")),
            ControlNode::End { outcome } => {
                let resolved = self.publication.definition.resolve_outputs(
                    &self.workflow_inputs(),
                    &self.activity_outputs,
                    &self.scope_values,
                )?;
                Ok(Some(NeutralDecision::Finish {
                    outcome: outcome.clone(),
                    outputs: resolved.values,
                    provenance: resolved.provenance,
                }))
            }
            ControlNode::Condition { condition } => {
                let condition = self
                    .publication
                    .definition
                    .conditions
                    .get(condition)
                    .ok_or("neutral condition unavailable")?;
                let evaluated = condition.evaluate(
                    &self.workflow_inputs(),
                    &self.activity_outputs,
                    &self.scope_values,
                )?;
                let outcome = match evaluated.decision {
                    crate::workflows::condition::Decision::True => "true",
                    crate::workflows::condition::Decision::False => "false",
                    crate::workflows::condition::Decision::Unknown => "unknown",
                };
                Ok(Some(self.route(outcome, evaluated.provenance)))
            }
            ControlNode::Wait { duration_ms } => match self.wait_due_ms {
                None => Ok(Some(NeutralDecision::ArmWait {
                    due_ms: checked_deadline(at_ms, *duration_ms)?,
                })),
                Some(due_ms) if at_ms >= due_ms => Ok(route("next")),
                Some(_) => Ok(None),
            },
            ControlNode::Activity { activity } => {
                if let Some(attempt) = &self.attempt {
                    return match &attempt.result {
                        Some(NeutralResult::Completed { outcome, .. }) => Ok(route(outcome)),
                        // Failures are applied to the terminal state with their immutable result.
                        _ => Ok(None),
                    };
                }
                if self.ancestor_running() >= self.run.request.limits.max_concurrency as usize {
                    return Ok(None);
                }
                self.prepare_activity(
                    *activity,
                    &BTreeMap::new(),
                    &self.activity_outputs,
                    NeutralAttemptFence {
                        run: self.run.id.clone(),
                        execution: self.execution.clone(),
                        activation: self.activation,
                        attempt: 1,
                        scope: None,
                    },
                )
            }
            ControlNode::Collection { scope }
            | ControlNode::Fork { scope }
            | ControlNode::Loop { scope, .. } => self.scope_decision(scope),
        }
    }

    fn fail_current(&mut self, diagnostic: &str) -> Result<(), String> {
        if let Some(parent) = self.parent.take() {
            let fence = self
                .parent_fence
                .take()
                .ok_or("subflow failure fence unavailable")?;
            let sequence = self.sequence;
            let count = self.activation_count;
            let effects = std::mem::take(&mut self.effects);
            *self = *parent;
            self.effects = effects;
            self.sequence = sequence;
            self.activation_count = count;
            self.apply(&NeutralDecision::Complete {
                fence,
                result: NeutralResult::Failed {
                    diagnostic: diagnostic.into(),
                },
            })
        } else {
            self.status = NeutralStatus::Failed {
                diagnostic: diagnostic.into(),
            };
            Ok(())
        }
    }

    fn subflow_publication(
        &self,
        activity: usize,
    ) -> Result<&crate::workflows::neutral_publication::NeutralPublication, String> {
        let descriptor = self.descriptor(activity)?;
        let Some(crate::workflows::catalog::ReusableTarget::Subflow { publication }) =
            &descriptor.target
        else {
            return Err("neutral activity is not a subflow".into());
        };
        self.run
            .closure
            .children
            .iter()
            .find(|child| {
                child.project == publication.reference.project
                    && child.id == publication.reference.id
                    && child.revision == publication.reference.revision
            })
            .ok_or_else(|| "retained subflow unavailable".into())
    }

    fn enter_subflow(&mut self, fence: &NeutralAttemptFence) -> Result<(), String> {
        let (activity, attempt) = self.pending_attempt(fence)?;
        let publication = self.subflow_publication(activity)?.clone();
        let (inputs, subject) = publication
            .definition
            .validate_values(&attempt.inputs, attempt.subject.as_ref())?;
        let entry = publication.definition.control.entry()?.to_owned();
        self.attempt_mut(fence)?.dispatched = true;
        let parent = Box::new(self.clone());
        self.execution = format!("{}/{}:{}", self.execution, fence.activation, fence.attempt);
        self.publication = publication;
        self.inputs = inputs;
        self.subject = subject;
        self.parent = Some(parent);
        self.parent_fence = Some(fence.clone());
        self.node = entry;
        self.activation_count = self
            .activation_count
            .checked_add(1)
            .ok_or("neutral subflow activation overflow")?;
        self.activation = self.activation_count;
        self.attempt = None;
        self.active_scope = None;
        self.wait_due_ms = None;
        self.activity_outputs.clear();
        self.scope_values.clear();
        Ok(())
    }

    pub(super) fn ancestor_invocations(
        &self,
    ) -> Result<Vec<super::neutral_runtime::NeutralInvocation>, String> {
        let Some(parent) = &self.parent else {
            return Ok(Vec::new());
        };
        let mut chain = parent.ancestor_invocations()?;
        let fence = self
            .parent_fence
            .as_ref()
            .ok_or("subflow parent identity unavailable")?;
        let (activity, attempt) = parent.pending_attempt(fence)?;
        chain.push(super::neutral_runtime::NeutralInvocation {
            fence: fence.clone(),
            publication: parent.publication.clone(),
            descriptor: parent.descriptor(activity)?.clone(),
            instance: parent.publication.definition.instances[activity].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        });
        Ok(chain)
    }

    pub(super) fn ancestor_running(&self) -> usize {
        self.ancestor_running_with(&self.effects)
    }

    fn ancestor_running_with(&self, effects: &super::neutral_effects::Effects) -> usize {
        let Some(parent) = &self.parent else {
            return 0;
        };
        let excluded = self.parent_fence.as_ref();
        let count = if let Some(scope) = &parent.active_scope {
            scope
                .slots
                .values()
                .filter_map(|slot| slot.attempt.as_ref())
                .filter(|attempt| {
                    attempt.dispatched
                        && Self::attempt_reserves_slot_with(attempt, effects)
                        && Some(&attempt.fence) != excluded
                })
                .count()
        } else {
            0
        };
        count + parent.ancestor_running_with(effects)
    }

    pub(super) fn attempt_reserves_slot(&self, attempt: &NeutralAttempt) -> bool {
        Self::attempt_reserves_slot_with(attempt, &self.effects)
    }

    fn attempt_reserves_slot_with(
        attempt: &NeutralAttempt,
        effects: &super::neutral_effects::Effects,
    ) -> bool {
        attempt.result.is_none()
            && !effects
                .values()
                .find(|effect| effect.tool.is_none() && effect.key.fence == attempt.fence)
                .is_some_and(|effect| {
                    !effect.dispatched
                        || !effect.approved()
                        || matches!(
                            effect.observation.as_ref().map(|o| &o.outcome),
                            Some(super::neutral_effects::EffectOutcome::Uncertain { .. })
                        )
                })
    }

    pub(super) fn dispatch_capacity(&self) -> bool {
        let (running, limit) = if let Some(scope) = &self.active_scope {
            let limit = match &scope.kind {
                super::neutral_scopes::NeutralScopeKind::Collection { .. } => {
                    self.publication.definition.collections[&scope.identity.scope].max_concurrency
                }
                super::neutral_scopes::NeutralScopeKind::Fork => {
                    self.publication.definition.forks[&scope.identity.scope].max_concurrency
                }
                super::neutral_scopes::NeutralScopeKind::Loop { .. } => 1,
            };
            (
                scope
                    .slots
                    .values()
                    .filter_map(|slot| slot.attempt.as_ref())
                    .filter(|a| a.dispatched && self.attempt_reserves_slot(a))
                    .count(),
                limit,
            )
        } else {
            (
                usize::from(
                    self.attempt
                        .as_ref()
                        .is_some_and(|a| a.dispatched && self.attempt_reserves_slot(a)),
                ),
                1,
            )
        };
        running < limit as usize
            && running + self.ancestor_running() < self.run.request.limits.max_concurrency as usize
    }

    pub(super) fn activation_limit(&self) -> NeutralDecision {
        NeutralDecision::Limit {
            reason: "neutral activation limit exhausted".into(),
        }
    }

    pub(super) fn pending_attempt(
        &self,
        fence: &NeutralAttemptFence,
    ) -> Result<(usize, &NeutralAttempt), String> {
        let (activity, attempt) = if let Some(owner) = &fence.scope {
            let scope = self
                .active_scope
                .as_ref()
                .ok_or("neutral completion has no current scope")?;
            if scope.identity != owner.identity || scope.settled.is_some() {
                return Err("neutral completion has stale scope generation".into());
            }
            let slot = scope
                .slots
                .get(&owner.item)
                .ok_or("neutral completion has no scope item")?;
            if slot.owner != *fence {
                return Err("neutral completion has stale or foreign attempt identity".into());
            }
            (slot.activity, slot.attempt.as_ref())
        } else {
            let ControlNode::Activity { activity } = self.current_node()? else {
                return Err("neutral completion has no current activity".into());
            };
            (*activity, self.attempt.as_ref())
        };
        let attempt = attempt.ok_or("neutral activity has no prepared attempt")?;
        if attempt.fence != *fence {
            return Err("neutral completion has stale or foreign attempt identity".into());
        }
        if attempt.result.is_some() {
            return Err("neutral activity result is immutable".into());
        }
        Ok((activity, attempt))
    }

    pub(super) fn attempt_mut(
        &mut self,
        fence: &NeutralAttemptFence,
    ) -> Result<&mut NeutralAttempt, String> {
        if let Some(scope) = &fence.scope {
            self.active_scope
                .as_mut()
                .and_then(|active| active.slots.get_mut(&scope.item))
                .and_then(|slot| slot.attempt.as_mut())
                .ok_or_else(|| "neutral scope attempt unavailable".into())
        } else {
            self.attempt
                .as_mut()
                .ok_or_else(|| "neutral attempt unavailable".into())
        }
    }

    pub(super) fn pending_invocations(&self) -> Vec<(usize, &NeutralAttempt)> {
        if self.status != NeutralStatus::Running {
            return Vec::new();
        }
        if let Some(scope) = &self.active_scope {
            scope
                .slots
                .values()
                .filter_map(|slot| {
                    slot.attempt
                        .as_ref()
                        .filter(|a| a.result.is_none() && !a.dispatched)
                        .map(|a| (slot.activity, a))
                })
                .collect()
        } else {
            match (self.current_node(), &self.attempt) {
                (Ok(ControlNode::Activity { activity }), Some(attempt))
                    if attempt.result.is_none() && !attempt.dispatched =>
                {
                    vec![(*activity, attempt)]
                }
                _ => Vec::new(),
            }
        }
    }

    pub(super) fn prepare_activity(
        &self,
        activity: usize,
        scoped: &BTreeMap<String, Datum>,
        outputs: &BTreeMap<usize, BTreeMap<String, Datum>>,
        fence: NeutralAttemptFence,
    ) -> Result<Option<NeutralDecision>, String> {
        let descriptor = self.descriptor(activity)?;
        let definition = &self.publication.definition;
        let bindings = definition.input_bindings.get(&activity);
        let resolved = if fence.scope.is_none() && bindings.is_none() {
            ResolvedInputs {
                values: descriptor.inputs.resolve(&self.inputs, "neutral inputs")?,
                provenance: BTreeMap::new(),
            }
        } else {
            let mut schema = descriptor.inputs.clone();
            for name in scoped.keys() {
                schema.fields.remove(name);
            }
            let empty = BTreeMap::new();
            let bindings = bindings.unwrap_or(&empty);
            // A fork may wait for another current branch. Unknown is never a default.
            if bindings.values().any(|source| {
                matches!(
                    source
                        .resolve(&self.workflow_inputs(), outputs, &self.scope_values)
                        .state,
                    DataState::Unknown
                )
            }) {
                if fence.scope.is_some() {
                    return Ok(None);
                }
                return Err("neutral activity input source is unknown".into());
            }
            let mut resolved = resolve_bindings(
                bindings,
                &self.workflow_inputs(),
                outputs,
                &schema,
                &self.scope_values,
            )?;
            for (name, datum) in scoped {
                match &datum.state {
                    DataState::Present(value) => {
                        resolved.values.insert(name.clone(), value.clone());
                        resolved
                            .provenance
                            .insert(name.clone(), datum.provenance.clone());
                    }
                    DataState::Absent => {
                        resolved.values.remove(name);
                    }
                    DataState::Unknown => return Err("neutral scoped input is unknown".into()),
                }
            }
            resolved.values = descriptor
                .inputs
                .resolve(&resolved.values, "neutral scoped inputs")?;
            resolved
        };
        let subject = match &descriptor.subject {
            Some(schema) => schema.resolve(self.subject.as_ref())?,
            None => self.subject.clone(),
        };
        Ok(Some(NeutralDecision::Prepare {
            fence,
            inputs: resolved.values,
            provenance: resolved.provenance,
            subject,
        }))
    }

    pub(super) fn output_data(
        &self,
        activity: usize,
        outputs: &BTreeMap<String, Value>,
        attempt: &NeutralAttempt,
    ) -> Result<BTreeMap<String, Datum>, String> {
        Ok(self
            .descriptor(activity)?
            .outputs
            .fields
            .keys()
            .map(|name| {
                let mut provenance = BTreeSet::from([DataOrigin::ActivityOutput {
                    activity,
                    field: name.clone(),
                }]);
                if let Some(scope) = &attempt.fence.scope {
                    provenance.insert(DataOrigin::ScopeResult {
                        identity: scope.identity.clone(),
                    });
                }
                (
                    name.clone(),
                    Datum {
                        state: outputs
                            .get(name)
                            .cloned()
                            .map_or(DataState::Absent, DataState::Present),
                        provenance,
                    },
                )
            })
            .collect())
    }

    pub(super) fn route(&self, outcome: &str, provenance: BTreeSet<DataOrigin>) -> NeutralDecision {
        if self.activation_count >= self.run.request.limits.max_activations {
            NeutralDecision::Limit {
                reason: "neutral activation limit exhausted".into(),
            }
        } else {
            NeutralDecision::Route {
                outcome: outcome.into(),
                provenance,
            }
        }
    }

    pub(super) fn apply(&mut self, decision: &NeutralDecision) -> Result<(), String> {
        match decision {
            NeutralDecision::Effect { decision } => self.apply_effect(decision)?,
            NeutralDecision::EnterSubflow { fence } => self.enter_subflow(fence)?,
            NeutralDecision::Failure { diagnostic } => self.fail_current(diagnostic)?,
            NeutralDecision::Dispatch { fence } => {
                self.pending_attempt(fence)?;
                self.attempt_mut(fence)?.dispatched = true;
            }
            NeutralDecision::Route { outcome, .. } => {
                let target = self
                    .publication
                    .definition
                    .control
                    .edges
                    .get(&self.node)
                    .and_then(|ports| ports.get(outcome))
                    .ok_or("neutral outcome edge unavailable")?;
                if self.activation_count >= self.run.request.limits.max_activations {
                    return Err("neutral activation limit exhausted".into());
                }
                self.node = target.clone();
                self.activation_count = self
                    .activation_count
                    .checked_add(1)
                    .ok_or("neutral activation count overflow")?;
                self.activation = self.activation_count;
                self.active_scope = None;
                self.attempt = None;
                self.wait_due_ms = None;
            }
            NeutralDecision::Prepare {
                fence,
                inputs,
                provenance,
                subject,
            } => {
                let attempt = NeutralAttempt {
                    fence: fence.clone(),
                    inputs: inputs.clone(),
                    provenance: provenance.clone(),
                    subject: subject.clone(),
                    result: None,
                    dispatched: false,
                };
                if let Some(scope_fence) = &fence.scope {
                    let scope = self
                        .active_scope
                        .as_mut()
                        .ok_or("neutral scope unavailable")?;
                    let slot = scope
                        .slots
                        .get_mut(&scope_fence.item)
                        .ok_or("neutral scope slot unavailable")?;
                    if scope.identity != scope_fence.identity
                        || slot.owner != *fence
                        || slot.attempt.is_some()
                    {
                        return Err("neutral scope preparation owner mismatch".into());
                    }
                    slot.attempt = Some(attempt);
                } else {
                    self.attempt = Some(attempt);
                }
            }
            NeutralDecision::Complete { fence, result } => {
                if fence.execution != self.execution {
                    let parent = self
                        .parent
                        .as_mut()
                        .ok_or("neutral callback execution unavailable")?;
                    parent.apply(decision)?;
                    if let NeutralStatus::Failed { diagnostic } = &parent.status {
                        self.status = NeutralStatus::Failed {
                            diagnostic: diagnostic.clone(),
                        };
                    }
                    return Ok(());
                }
                let (activity, attempt) = self.pending_attempt(fence)?;
                if let Some(scope) = &fence.scope {
                    self.active_scope
                        .as_mut()
                        .ok_or("neutral scope unavailable")?
                        .slots
                        .get_mut(&scope.item)
                        .ok_or("neutral scope slot unavailable")?
                        .attempt
                        .as_mut()
                        .ok_or("neutral scope attempt unavailable")?
                        .result = Some(result.clone());
                } else {
                    match result {
                        NeutralResult::Completed { outputs, .. } => {
                            let fields = self.output_data(activity, outputs, attempt)?;
                            self.activity_outputs.insert(activity, fields);
                        }
                        NeutralResult::Failed { diagnostic } => {
                            if self.parent.is_some() {
                                return self.fail_current(diagnostic);
                            }
                            self.status = NeutralStatus::Failed {
                                diagnostic: diagnostic.clone(),
                            };
                        }
                    }
                    self.attempt
                        .as_mut()
                        .ok_or("neutral result has no attempt")?
                        .result = Some(result.clone());
                }
            }
            NeutralDecision::Scope { action } => self.apply_scope(action)?,
            NeutralDecision::ArmWait { due_ms } => self.wait_due_ms = Some(*due_ms),
            NeutralDecision::Finish {
                outcome,
                outputs,
                provenance,
            } => {
                if let Some(parent) = self.parent.take() {
                    let fence = self
                        .parent_fence
                        .take()
                        .ok_or("subflow return fence unavailable")?;
                    let sequence = self.sequence;
                    let activation_count = self.activation_count;
                    let effects = std::mem::take(&mut self.effects);
                    *self = *parent;
                    self.effects = effects;
                    self.sequence = sequence;
                    self.activation_count = activation_count;
                    return self.apply(&NeutralDecision::Complete {
                        fence,
                        result: NeutralResult::Completed {
                            outcome: outcome.clone(),
                            outputs: outputs.clone(),
                        },
                    });
                }
                self.status = NeutralStatus::Completed {
                    outcome: outcome.clone(),
                };
                self.outputs = Some(ResolvedInputs {
                    values: outputs.clone(),
                    provenance: provenance.clone(),
                });
            }
            NeutralDecision::Cancel => self.status = NeutralStatus::Cancelled,
            NeutralDecision::Expire => self.status = NeutralStatus::Expired,
            NeutralDecision::Limit { reason } => {
                self.status = NeutralStatus::Limited {
                    reason: reason.clone(),
                }
            }
        }
        Ok(())
    }
}

pub(super) fn checked_deadline(at_ms: i64, duration_ms: u64) -> Result<i64, String> {
    let duration =
        i64::try_from(duration_ms).map_err(|_| "neutral duration exceeds timestamp range")?;
    at_ms
        .checked_add(duration)
        .ok_or_else(|| "neutral deadline timestamp overflow".into())
}

impl RuntimeOwner {
    pub(super) fn neutral_command(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: i64,
        command: NeutralExecutionCommand,
    ) -> Result<NeutralExecution, String> {
        self.neutral_command_at(run_id, command_id, expected_sequence, command, now_ms())
    }

    pub(super) fn neutral_command_at(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: i64,
        command: NeutralExecutionCommand,
        at_ms: i64,
    ) -> Result<NeutralExecution, String> {
        if command_id.trim().is_empty() {
            return Err("neutral command requires an identity".into());
        }
        let hash = content_digest(&command)?;
        let mut conn = self.store.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin neutral execution decision: {e}"))?;
        let (mut snapshot, events) = replay(&tx, run_id)?;
        if let Some(event) = events.iter().find(|event| event.command_id == command_id) {
            if event.command_hash != hash {
                return Err("neutral command identity reused with different arguments".into());
            }
            // Return the original receipt projection, not today's cursor.
            return replay_until(&tx, run_id, Some(event.sequence)).map(|(snapshot, _)| snapshot);
        }
        if snapshot.sequence != expected_sequence {
            return Err("neutral execution sequence changed; retry from current snapshot".into());
        }
        let Some(decision) = snapshot.decide(&command, at_ms)? else {
            return Ok(snapshot);
        };
        if super::correction_generation::fence(&tx, run_id, &decision)? {
            drop(tx);
            drop(conn);
            let cancelled = self.neutral_command_at(
                run_id,
                &format!("superseded:{command_id}"),
                expected_sequence,
                NeutralExecutionCommand::Cancel,
                at_ms,
            )?;
            return self.neutral_command_at(run_id, command_id, cancelled.sequence, command, at_ms);
        }
        snapshot.apply(&decision)?;
        if let NeutralDecision::Effect { decision } = &decision {
            super::neutral_effects::project_resources(&tx, &snapshot, decision)?;
        }
        let sequence = snapshot
            .sequence
            .checked_add(1)
            .ok_or("neutral event sequence overflow")?;
        let event = NeutralEvent {
            contract_version: CONTRACT_VERSION,
            sequence,
            command_id: command_id.into(),
            command_hash: hash,
            at_ms,
            kind: NeutralEventKind::Execution {
                command: Box::new(command),
                decision: Box::new(decision),
            },
        };
        let document =
            serde_json::to_string(&event).map_err(|e| format!("encode neutral decision: {e}"))?;
        tx.execute("INSERT INTO neutral_workflow_events(run_id,sequence,command_id,event_json,sha256) VALUES (?1,?2,?3,?4,?5)",
            params![run_id, sequence, command_id, document, content_digest(&event)?])
            .map_err(|e| format!("save neutral decision: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit neutral decision: {e}"))?;
        snapshot.sequence = sequence;
        self.publish_neutral_cursor(&snapshot.run.request.publication.project, run_id, sequence);
        Ok(snapshot)
    }
}

impl RunStore {
    /// Startup enumerates identities only. Each actor verifies its own history,
    /// so one corrupt run cannot prevent the shared owner from becoming ready.
    pub(super) fn neutral_recovery_run_ids(&self) -> Result<Vec<String>, String> {
        let conn = self.connect()?;
        let mut statement = conn
            .prepare("SELECT id FROM neutral_workflow_runs ORDER BY id")
            .map_err(|e| format!("prepare neutral restart scan: {e}"))?;
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("read neutral restart scan: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read neutral restart id: {e}"))
    }

    pub(crate) fn neutral_inspection(
        &self,
        run_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<(NeutralExecution, Vec<NeutralEvent>), String> {
        if after_sequence < 0 || limit == 0 || limit > 500 {
            return Err("invalid neutral event cursor or limit".into());
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let (snapshot, events) = replay(&tx, run_id)?;
        let events = events
            .into_iter()
            .filter(|e| e.sequence > after_sequence)
            .take(limit)
            .collect();
        tx.commit().map_err(|e| e.to_string())?;
        Ok((snapshot, events))
    }

    pub(crate) fn neutral_events_after(
        &self,
        run_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<NeutralEvent>, String> {
        if after_sequence < 0 || limit == 0 || limit > 500 {
            return Err("invalid neutral event cursor or limit".into());
        }
        let (_, events) = replay(&self.connect()?, run_id)?;
        Ok(events
            .into_iter()
            .filter(|event| event.sequence > after_sequence)
            .take(limit)
            .collect())
    }

    pub(crate) fn neutral_execution(&self, run_id: &str) -> Result<NeutralExecution, String> {
        replay(&self.connect()?, run_id).map(|(snapshot, _)| snapshot)
    }
}

fn replay(
    conn: &Connection,
    run_id: &str,
) -> Result<(NeutralExecution, Vec<NeutralEvent>), String> {
    replay_until(conn, run_id, None)
}

fn replay_until(
    conn: &Connection,
    run_id: &str,
    through: Option<i64>,
) -> Result<(NeutralExecution, Vec<NeutralEvent>), String> {
    replay_observed(conn, run_id, through, |_, _| Ok(()))
}

fn replay_observed(
    conn: &Connection,
    run_id: &str,
    through: Option<i64>,
    mut observe: impl FnMut(&NeutralExecution, &NeutralDecision) -> Result<(), String>,
) -> Result<(NeutralExecution, Vec<NeutralEvent>), String> {
    let run = super::neutral::read_run(conn, run_id)?;
    let mut statement = conn.prepare("SELECT sequence,command_id,event_json,sha256 FROM neutral_workflow_events WHERE run_id=?1 ORDER BY sequence")
        .map_err(|e| format!("prepare neutral execution replay: {e}"))?;
    let rows = statement
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("read neutral execution replay: {e}"))?;
    let mut snapshot: Option<NeutralExecution> = None;
    let mut events = Vec::new();
    for row in rows {
        let (sequence, command_id, document, digest) =
            row.map_err(|e| format!("read neutral execution event: {e}"))?;
        if through.is_some_and(|last| sequence > last) {
            break;
        }
        let event: NeutralEvent = serde_json::from_str(&document)
            .map_err(|e| format!("decode neutral execution event: {e}"))?;
        if event.contract_version != CONTRACT_VERSION
            || event.sequence != sequence
            || event.command_id != command_id
            || content_digest(&event)? != digest
        {
            return Err("neutral execution event identity or content mismatch".into());
        }
        match &event.kind {
            NeutralEventKind::Started { .. } if sequence == 1 && snapshot.is_none() => {
                snapshot = Some(NeutralExecution::started(run.clone(), event.at_ms)?);
            }
            NeutralEventKind::Execution { command, decision } => {
                let current = snapshot.as_mut().ok_or("neutral execution has no start")?;
                if sequence
                    != current
                        .sequence
                        .checked_add(1)
                        .ok_or("neutral sequence overflow")?
                    || content_digest(command)? != event.command_hash
                    || current.decide(command, event.at_ms)?.as_ref() != Some(decision.as_ref())
                {
                    return Err("neutral execution decision or sequence mismatch".into());
                }
                observe(current, decision)?;
                current.apply(decision)?;
                current.sequence = sequence;
            }
            _ => return Err("neutral execution has an invalid start event".into()),
        }
        events.push(event);
    }
    snapshot
        .map(|snapshot| (snapshot, events))
        .ok_or_else(|| "neutral execution history is empty".into())
}

/// The exact parent invocation of a normally returned child, proved by replay.
/// No current-catalog lookup, new dispatch, or durable projection is involved.
impl RunStore {
    pub(super) fn neutral_returned_subflow(
        &self,
        run: &str,
        execution: &str,
    ) -> Result<
        (
            super::neutral_runtime::NeutralInvocation,
            String,
            BTreeMap<String, Value>,
        ),
        String,
    > {
        let mut returned = None;
        replay_observed(&self.connect()?, run, None, |current, decision| {
            if current.execution != execution {
                return Ok(());
            }
            let NeutralDecision::Finish {
                outcome, outputs, ..
            } = decision
            else {
                return Ok(());
            };
            let parent = current
                .parent
                .as_ref()
                .ok_or("returned frame is not a subflow")?;
            let fence = current
                .parent_fence
                .as_ref()
                .ok_or("returned child lacks parent fence")?;
            let (activity, attempt) = parent.pending_attempt(fence)?;
            let invocation = super::neutral_runtime::NeutralInvocation {
                fence: fence.clone(),
                publication: parent.publication.clone(),
                descriptor: parent.descriptor(activity)?.clone(),
                instance: parent.publication.definition.instances[activity].clone(),
                inputs: attempt.inputs.clone(),
                subject: attempt.subject.clone(),
            };
            if returned
                .replace((invocation, outcome.clone(), outputs.clone()))
                .is_some()
            {
                return Err("child execution returned more than once".into());
            }
            Ok(())
        })?;
        returned.ok_or_else(|| "exact child has no retained normal return".into())
    }
}
