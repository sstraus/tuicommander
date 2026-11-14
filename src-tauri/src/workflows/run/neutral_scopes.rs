//! Scope decisions share the neutral owner's transaction and activation budget.
use super::neutral_execution::{
    NeutralAttempt, NeutralAttemptFence, NeutralDecision, NeutralExecution, NeutralResult,
};
use crate::workflows::collection::{
    AggregateState, CollectionIdentity, Expansion, ItemResult, ItemState,
};
use crate::workflows::data_binding::{DataOrigin, DataState, Datum, ResolvedInputs};
use crate::workflows::fork_scope::{ArrivalState, BranchArrival, BranchAttempt};
use crate::workflows::neutral_control::ControlNode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralScopeFence {
    pub identity: CollectionIdentity,
    pub item: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralScopeSlot {
    pub activity: usize,
    pub inputs: BTreeMap<String, Datum>,
    pub owner: NeutralAttemptFence,
    pub attempt: Option<NeutralAttempt>,
    pub skipped: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NeutralScopeKind {
    Collection {
        expansion: Expansion,
    },
    Fork,
    Loop {
        iteration: u32,
        carried: ResolvedInputs,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralScope {
    pub identity: CollectionIdentity,
    pub kind: NeutralScopeKind,
    pub slots: BTreeMap<String, NeutralScopeSlot>,
    pub settled: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScopeDecision {
    Skip {
        identity: CollectionIdentity,
        item: String,
        reason: String,
    },
    Open {
        scope: Box<NeutralScope>,
    },
    Settle {
        identity: CollectionIdentity,
        outcome: String,
        values: Option<BTreeMap<String, Datum>>,
    },
    Iterate {
        identity: CollectionIdentity,
        iteration: u32,
        carried: ResolvedInputs,
        values: BTreeMap<String, Datum>,
        slot: Box<NeutralScopeSlot>,
    },
}

impl NeutralExecution {
    fn scope_slot(
        &self,
        identity: &CollectionIdentity,
        item: String,
        activity: usize,
        inputs: BTreeMap<String, Datum>,
        offset: u32,
    ) -> Result<NeutralScopeSlot, String> {
        let activation = self
            .activation_count
            .checked_add(offset)
            .ok_or("neutral activation counter overflow")?;
        Ok(NeutralScopeSlot {
            activity,
            inputs,
            attempt: None,
            skipped: None,
            owner: NeutralAttemptFence {
                run: self.run.id.clone(),
                execution: self.execution.clone(),
                activation,
                attempt: 1,
                scope: Some(NeutralScopeFence {
                    identity: identity.clone(),
                    item,
                }),
            },
        })
    }

    pub(super) fn scope_decision(&self, name: &str) -> Result<Option<NeutralDecision>, String> {
        let definition = &self.publication.definition;
        let Some(scope) = &self.active_scope else {
            let identity = CollectionIdentity {
                run: self.run.id.clone(),
                scope: name.into(),
                generation: u64::from(self.activation),
            };
            let mut slots = BTreeMap::new();
            let kind = match self.current_node()? {
                ControlNode::Collection { .. } => {
                    let contract = &definition.collections[name];
                    let expansion = contract.expand(
                        identity.clone(),
                        &self.workflow_inputs(),
                        &self.activity_outputs,
                        &self.scope_values,
                    )?;
                    if !self.scope_capacity(expansion.items().len()) {
                        return Ok(Some(self.activation_limit()));
                    }
                    for item in expansion.items() {
                        let key = item.identity.index.to_string();
                        slots.insert(
                            key.clone(),
                            self.scope_slot(
                                &identity,
                                key,
                                contract.body,
                                item.inputs.clone(),
                                item.identity
                                    .index
                                    .checked_add(1)
                                    .ok_or("collection activation offset overflow")?,
                            )?,
                        );
                    }
                    NeutralScopeKind::Collection { expansion }
                }
                ControlNode::Fork { .. } => {
                    let contract = &definition.forks[name];
                    if !self.scope_capacity(contract.branches.len()) {
                        return Ok(Some(self.activation_limit()));
                    }
                    for (index, (key, branch)) in contract.branches.iter().enumerate() {
                        slots.insert(
                            key.clone(),
                            self.scope_slot(
                                &identity,
                                key.clone(),
                                branch.activity,
                                BTreeMap::new(),
                                u32::try_from(index)
                                    .ok()
                                    .and_then(|n| n.checked_add(1))
                                    .ok_or("fork activation offset overflow")?,
                            )?,
                        );
                    }
                    NeutralScopeKind::Fork
                }
                ControlNode::Loop { .. } => {
                    if !self.scope_capacity(1) {
                        return Ok(Some(self.activation_limit()));
                    }
                    let contract = &definition.loops[name];
                    let initial = contract.resolve(
                        identity.clone(),
                        0,
                        &self.workflow_inputs(),
                        &self.activity_outputs,
                        &self.scope_values,
                    )?;
                    slots.insert(
                        "1".into(),
                        self.scope_slot(
                            &identity,
                            "1".into(),
                            contract.body,
                            carried_data(&initial.fields, &contract.carried),
                            1,
                        )?,
                    );
                    NeutralScopeKind::Loop {
                        iteration: 1,
                        carried: initial.fields,
                    }
                }
                _ => return Err("neutral scope has no control owner".into()),
            };
            return Ok(Some(NeutralDecision::Scope {
                action: ScopeDecision::Open {
                    scope: Box::new(NeutralScope {
                        identity,
                        kind,
                        slots,
                        settled: None,
                    }),
                },
            }));
        };
        if scope.identity.scope != name {
            return Err("neutral scope cursor mismatch".into());
        }
        if let Some(outcome) = &scope.settled {
            return Ok(Some(self.route(
                outcome,
                BTreeSet::from([DataOrigin::ScopeResult {
                    identity: scope.identity.clone(),
                }]),
            )));
        }
        if scope.slots.values().all(|slot| {
            slot.skipped.is_some()
                || slot
                    .attempt
                    .as_ref()
                    .is_some_and(|attempt| attempt.result.is_some())
        }) {
            return self.settle_scope(scope).map(Some);
        }
        let concurrency = match &scope.kind {
            NeutralScopeKind::Collection { .. } => definition.collections[name].max_concurrency,
            NeutralScopeKind::Fork => definition.forks[name].max_concurrency,
            NeutralScopeKind::Loop { .. } => 1,
        }
        .min(self.run.request.limits.max_concurrency);
        let running = scope
            .slots
            .values()
            .filter(|slot| {
                slot.attempt
                    .as_ref()
                    .is_some_and(|attempt| self.attempt_reserves_slot(attempt))
            })
            .count();
        if running >= concurrency as usize
            || running + self.ancestor_running() >= self.run.request.limits.max_concurrency as usize
        {
            return Ok(None);
        }
        let outputs = self.scope_activity_outputs(scope)?;
        for slot in scope
            .slots
            .values()
            .filter(|slot| slot.attempt.is_none() && slot.skipped.is_none())
        {
            if let Some(decision) =
                self.prepare_activity(slot.activity, &slot.inputs, &outputs, slot.owner.clone())?
            {
                return Ok(Some(decision));
            }
        }
        if running == 0 && self.ancestor_running() == 0 {
            if scope.slots.values().any(|slot| {
                slot.attempt
                    .as_ref()
                    .is_some_and(|attempt| attempt.result.is_none())
            }) {
                return Ok(None);
            }
            let (item, _) = scope
                .slots
                .iter()
                .find(|(_, slot)| slot.attempt.is_none() && slot.skipped.is_none())
                .ok_or("neutral scope has no unresolved slot")?;
            return Ok(Some(NeutralDecision::Scope {
                action: ScopeDecision::Skip {
                    identity: scope.identity.clone(),
                    item: item.clone(),
                    reason: "required input producer supplied no value".into(),
                },
            }));
        }
        Ok(None)
    }

    fn scope_capacity(&self, additional: usize) -> bool {
        u32::try_from(additional)
            .ok()
            .and_then(|n| self.activation_count.checked_add(n))
            .is_some_and(|count| count <= self.run.request.limits.max_activations)
    }

    pub(super) fn scope_activity_outputs(
        &self,
        scope: &NeutralScope,
    ) -> Result<BTreeMap<usize, BTreeMap<String, Datum>>, String> {
        let mut outputs = self.activity_outputs.clone();
        // Distinct fork producers may feed a sibling through an ordinary binding.
        // Collection siblings share a body ordinal, so only their own slot may
        // retain those results until the ordered aggregate is settled.
        if matches!(scope.kind, NeutralScopeKind::Fork) {
            for slot in scope.slots.values() {
                if let Some(attempt) = &slot.attempt
                    && let Some(NeutralResult::Completed {
                        outputs: values, ..
                    }) = &attempt.result
                {
                    outputs.insert(
                        slot.activity,
                        self.output_data(slot.activity, values, attempt)?,
                    );
                }
            }
        }
        Ok(outputs)
    }

    fn settle_scope(&self, scope: &NeutralScope) -> Result<NeutralDecision, String> {
        let definition = &self.publication.definition;
        let name = &scope.identity.scope;
        let (outcome, values) = match &scope.kind {
            NeutralScopeKind::Collection { expansion } => {
                let contract = &definition.collections[name];
                let descriptor = self.descriptor(contract.body)?;
                let results = expansion
                    .items()
                    .iter()
                    .map(|item| {
                        let slot = &scope.slots[&item.identity.index.to_string()];
                        if let Some(reason) = &slot.skipped {
                            return Ok(ItemResult {
                                identity: item.identity.clone(),
                                state: ItemState::Skipped {
                                    reason: reason.clone(),
                                },
                            });
                        }
                        let attempt = slot
                            .attempt
                            .as_ref()
                            .ok_or("collection slot has no attempt")?;
                        if attempt.fence != slot.owner {
                            return Err("collection attempt owner changed".into());
                        }
                        let state = match attempt
                            .result
                            .as_ref()
                            .ok_or("collection slot is pending")?
                        {
                            NeutralResult::Completed { outcome, outputs } => ItemState::Completed {
                                outcome: outcome.clone(),
                                outputs: self.output_data(slot.activity, outputs, attempt)?,
                            },
                            NeutralResult::Failed { diagnostic } => ItemState::Failed {
                                diagnostic: diagnostic.clone(),
                            },
                        };
                        Ok(ItemResult {
                            identity: item.identity.clone(),
                            state,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let aggregate = contract.aggregate(
                    expansion,
                    &results,
                    &descriptor.outputs,
                    &descriptor.outcomes,
                )?;
                let outcome = match aggregate.state {
                    AggregateState::Complete => "complete",
                    AggregateState::Blocked => "blocked",
                    AggregateState::Pending => {
                        return Err("collection settled before all items".into());
                    }
                };
                (
                    outcome.to_owned(),
                    Some(crate::workflows::scope_data::collection_snapshot(
                        &aggregate,
                    )?),
                )
            }
            NeutralScopeKind::Fork => {
                let contract = &definition.forks[name];
                let expected = scope
                    .slots
                    .iter()
                    .map(|(key, slot)| {
                        (
                            key.clone(),
                            BranchAttempt {
                                activation: slot.owner.activation.to_string(),
                                attempt: slot.owner.attempt,
                            },
                        )
                    })
                    .collect();
                let arrivals = scope
                    .slots
                    .iter()
                    .map(|(key, slot)| {
                        if slot.skipped.is_some() {
                            return Ok(BranchArrival {
                                identity: scope.identity.clone(),
                                branch: key.clone(),
                                owner: BranchAttempt {
                                    activation: slot.owner.activation.to_string(),
                                    attempt: slot.owner.attempt,
                                },
                                state: ArrivalState::Skipped,
                            });
                        }
                        let attempt = slot.attempt.as_ref().ok_or("fork branch has no attempt")?;
                        if attempt.fence != slot.owner {
                            return Err("fork attempt owner changed".into());
                        }
                        let state = match attempt.result.as_ref().ok_or("fork branch is pending")? {
                            NeutralResult::Completed { .. } => ArrivalState::Completed,
                            NeutralResult::Failed { .. } => ArrivalState::Failed,
                        };
                        Ok(BranchArrival {
                            identity: scope.identity.clone(),
                            branch: key.clone(),
                            owner: BranchAttempt {
                                activation: slot.owner.activation.to_string(),
                                attempt: slot.owner.attempt,
                            },
                            state,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let joined = contract.join(&scope.identity, &expected, &arrivals)?;
                let outcome = match joined.state {
                    AggregateState::Complete => "complete",
                    AggregateState::Blocked => "blocked",
                    AggregateState::Pending => {
                        return Err("fork settled before all branches".into());
                    }
                };
                (
                    outcome.to_owned(),
                    Some(crate::workflows::scope_data::fork_snapshot(&joined)?),
                )
            }
            NeutralScopeKind::Loop { iteration, carried } => {
                let contract = &definition.loops[name];
                let slot = scope.slots.values().next().ok_or("loop body unavailable")?;
                let attempt = slot.attempt.as_ref().ok_or("loop body has no attempt")?;
                if attempt.fence != slot.owner {
                    return Err("loop attempt owner changed".into());
                }
                let result = attempt.result.as_ref().ok_or("loop body is pending")?;
                if matches!(result, NeutralResult::Failed { .. }) {
                    let result = crate::workflows::loop_data::LoopResult {
                        identity: scope.identity.clone(),
                        iteration: iteration - 1,
                        fields: carried.clone(),
                    };
                    (
                        "failed".into(),
                        Some(crate::workflows::scope_data::loop_snapshot(
                            contract, &result,
                        )),
                    )
                } else {
                    let NeutralResult::Completed {
                        outputs: values, ..
                    } = result
                    else {
                        unreachable!()
                    };
                    let mut outputs = self.activity_outputs.clone();
                    outputs.insert(
                        slot.activity,
                        self.output_data(slot.activity, values, attempt)?,
                    );
                    let result = contract.resolve(
                        scope.identity.clone(),
                        *iteration,
                        &self.workflow_inputs(),
                        &outputs,
                        &self.scope_values,
                    )?;
                    let values = crate::workflows::scope_data::loop_snapshot(contract, &result);
                    let mut scopes = self.scope_values.clone();
                    scopes.insert(name.clone(), values.clone());
                    let ControlNode::Loop { condition, .. } = self.current_node()? else {
                        return Err("loop condition owner unavailable".into());
                    };
                    let evaluation = definition.conditions[condition].evaluate(
                        &self.workflow_inputs(),
                        &outputs,
                        &scopes,
                    )?;
                    use crate::workflows::condition::Decision;
                    let outcome = match evaluation.decision {
                        Decision::False => "complete",
                        Decision::Unknown => "unknown",
                        Decision::True if *iteration >= contract.max_iterations => "exhausted",
                        Decision::True => {
                            if !self.scope_capacity(1) {
                                return Ok(self.activation_limit());
                            }
                            let next = iteration
                                .checked_add(1)
                                .ok_or("loop iteration counter overflow")?;
                            let slot = self.scope_slot(
                                &scope.identity,
                                next.to_string(),
                                contract.body,
                                carried_data(&result.fields, &contract.carried),
                                1,
                            )?;
                            return Ok(NeutralDecision::Scope {
                                action: ScopeDecision::Iterate {
                                    identity: scope.identity.clone(),
                                    iteration: next,
                                    carried: result.fields,
                                    values,
                                    slot: Box::new(slot),
                                },
                            });
                        }
                    };
                    (outcome.into(), Some(values))
                }
            }
        };
        Ok(NeutralDecision::Scope {
            action: ScopeDecision::Settle {
                identity: scope.identity.clone(),
                outcome,
                values,
            },
        })
    }

    pub(super) fn apply_scope(&mut self, action: &ScopeDecision) -> Result<(), String> {
        match action {
            ScopeDecision::Skip {
                identity,
                item,
                reason,
            } => {
                let scope = self
                    .active_scope
                    .as_mut()
                    .ok_or("neutral skipped scope unavailable")?;
                let slot = scope
                    .slots
                    .get_mut(item)
                    .ok_or("neutral skipped item unavailable")?;
                if scope.identity != *identity || slot.attempt.is_some() || slot.skipped.is_some() {
                    return Err("neutral skipped item has stale owner".into());
                }
                slot.skipped = Some(reason.clone());
            }
            ScopeDecision::Open { scope } => {
                self.activation_count = self
                    .activation_count
                    .checked_add(
                        u32::try_from(scope.slots.len())
                            .map_err(|_| "scope item count overflow")?,
                    )
                    .ok_or("neutral activation count overflow")?;
                self.active_scope = Some((**scope).clone());
            }
            ScopeDecision::Settle {
                identity,
                outcome,
                values,
            } => {
                let scope = self
                    .active_scope
                    .as_mut()
                    .ok_or("neutral scope is unavailable")?;
                if &scope.identity != identity || scope.settled.is_some() {
                    return Err("scope settlement has stale identity or is duplicated".into());
                }
                scope.settled = Some(outcome.clone());
                if let Some(values) = values {
                    self.scope_values
                        .insert(identity.scope.clone(), values.clone());
                }
                if matches!(scope.kind, NeutralScopeKind::Fork) {
                    self.activity_outputs = self.scope_activity_outputs(
                        self.active_scope.as_ref().expect("checked above"),
                    )?;
                }
            }
            ScopeDecision::Iterate {
                identity,
                iteration,
                carried,
                values,
                slot,
            } => {
                let scope = self
                    .active_scope
                    .as_mut()
                    .ok_or("neutral loop is unavailable")?;
                if &scope.identity != identity || scope.settled.is_some() {
                    return Err("loop iteration has stale scope identity".into());
                }
                scope.kind = NeutralScopeKind::Loop {
                    iteration: *iteration,
                    carried: carried.clone(),
                };
                scope.slots = BTreeMap::from([(iteration.to_string(), (**slot).clone())]);
                self.activation_count = self
                    .activation_count
                    .checked_add(1)
                    .ok_or("neutral activation count overflow")?;
                self.scope_values
                    .insert(identity.scope.clone(), values.clone());
            }
        }
        Ok(())
    }
}

fn carried_data(
    fields: &ResolvedInputs,
    schema: &crate::workflows::catalog::ObjectSchema,
) -> BTreeMap<String, Datum> {
    schema
        .fields
        .keys()
        .map(|name| {
            (
                name.clone(),
                Datum {
                    state: fields
                        .values
                        .get(name)
                        .cloned()
                        .map_or(DataState::Absent, DataState::Present),
                    provenance: fields.provenance.get(name).cloned().unwrap_or_default(),
                },
            )
        })
        .collect()
}
