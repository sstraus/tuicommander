//! Correlated token projection. The event ledger, not completion order, owns joins.
use super::graph::{Activation, ActivationState, GraphExecution, GraphTransition};
use crate::workflows::NodeKind;
use crate::workflows::definition::{EmptyFanOut, Predicate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchToken {
    pub fork_id: String,
    pub generation: u32,
    pub branch_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalOutcome {
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

impl ArrivalOutcome {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchObligation {
    pub id: String,
    pub required: bool,
    pub item: Option<String>,
    pub arrival: Option<ArrivalOutcome>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForkState {
    Open,
    Joined,
    Cancelling,
    Superseded,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForkInstance {
    pub id: String,
    pub node_id: String,
    pub join: String,
    pub generation: u32,
    pub parent: Vec<BranchToken>,
    pub branches: Vec<BranchObligation>,
    pub state: ForkState,
    pub repair_to: Option<String>,
    pub empty: Option<ArrivalOutcome>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopedCheck {
    pub activation_id: String,
    pub token: BranchToken,
    pub receipt: super::check::CheckReceipt,
}

pub(super) fn check_owner(run_id: &str, graph_id: &str, token: &BranchToken) -> String {
    format!("{run_id}:{graph_id}:{}:{}", token.fork_id, token.generation)
}

impl GraphExecution {
    fn push_token(
        &mut self,
        node_id: String,
        edge: Option<usize>,
        predecessor: &Activation,
        tokens: Vec<BranchToken>,
    ) -> Result<(), String> {
        self.enqueue(edge, node_id)?;
        let next = self.activations.last_mut().expect("enqueued activation");
        next.predecessor = Some(predecessor.id.clone());
        next.tokens = tokens;
        Ok(())
    }

    pub(super) fn apply_parallel(
        &mut self,
        transition: &GraphTransition,
        remaining_loops: u16,
    ) -> Result<u16, String> {
        match transition {
            GraphTransition::Expand {
                activation_id,
                items,
                ..
            } => {
                let index = self
                    .activations
                    .iter()
                    .position(|a| a.id == *activation_id && a.state == ActivationState::Running)
                    .ok_or("For each activation is not running")?;
                let activation = self.activations[index].clone();
                let NodeKind::ForEach {
                    join,
                    body,
                    max_items,
                    empty,
                    receipt_evidence: Some(_),
                    ..
                } = self
                    .definition
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.id == activation.node_id)
                    .ok_or("For each node missing")?
                    .kind
                    .clone()
                else {
                    return Err("activation has no declared receipt collection".into());
                };
                if items.len() > usize::from(max_items)
                    || items.iter().any(|item| item.len() > 4096)
                {
                    return Err("dynamic For each exceeds its published bound".into());
                }
                let branches = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (format!("item-{i}"), body.clone(), true, Some(item.clone())))
                    .collect();
                let empty = match empty {
                    EmptyFanOut::Complete => ArrivalOutcome::Completed,
                    EmptyFanOut::Skip => ArrivalOutcome::Skipped,
                    EmptyFanOut::Fail => ArrivalOutcome::Failed,
                };
                self.open_fork(&activation, &join, branches, None, Some(empty))?;
                self.activations[index].state = ActivationState::Completed;
                self.release_join(&activation.id)?;
                return Ok(0);
            }
            GraphTransition::ResourceClaim {
                activation_id, key, ..
            } => {
                if key
                    .as_ref()
                    .is_some_and(|key| !key.starts_with("check:") || key.len() > 80)
                {
                    return Err("unknown activity resource".into());
                }
                let activation = self
                    .activations
                    .iter_mut()
                    .find(|a| {
                        a.id == *activation_id
                            && (key.is_none() || a.state == ActivationState::Running)
                    })
                    .ok_or("resource claim activation is not running")?;
                activation.resource_claim = key.clone();
                activation.resource_wait = None;
                return Ok(0);
            }
            GraphTransition::CheckBound {
                activation_id,
                receipt,
                ..
            } => {
                let activation = self
                    .activations
                    .iter()
                    .find(|a| a.id == *activation_id && a.state == ActivationState::Running)
                    .ok_or("check activation is not running")?;
                let token = activation
                    .tokens
                    .last()
                    .ok_or("check is not branch-scoped")?
                    .clone();
                if !self.forks.iter().any(|f| {
                    f.id == token.fork_id
                        && f.generation == token.generation
                        && f.state == ForkState::Open
                }) {
                    return Err("check belongs to an obsolete fork generation".into());
                }
                if self
                    .checks
                    .iter()
                    .any(|c| c.activation_id == *activation_id && c.receipt == *receipt)
                {
                    return Ok(0);
                }
                self.activations
                    .iter_mut()
                    .find(|a| a.id == *activation_id)
                    .expect("check activation")
                    .resource_claim = None;
                self.checks.push(ScopedCheck {
                    activation_id: activation_id.clone(),
                    token,
                    receipt: receipt.clone(),
                });
                return Ok(0);
            }
            GraphTransition::ResourceWait {
                activation_id,
                reason,
                ..
            } => {
                if reason
                    .as_ref()
                    .is_some_and(|r| r.trim().is_empty() || r.len() > 4096)
                {
                    return Err("resource wait needs a bounded reason".into());
                }
                let activation = self
                    .activations
                    .iter_mut()
                    .find(|a| a.id == *activation_id && a.state == ActivationState::Running)
                    .ok_or("resource wait activation is not running")?;
                activation.resource_wait = reason.clone();
                return Ok(0);
            }
            GraphTransition::Arrive {
                activation_id,
                token,
                outcome,
                ..
            } => {
                return self.arrive(activation_id, token, *outcome).map(|_| 0);
            }
            GraphTransition::SupersedeFork {
                fork_id,
                generation,
                ..
            } => {
                let fork = self
                    .forks
                    .iter_mut()
                    .find(|f| f.id == *fork_id && f.generation == *generation)
                    .ok_or("fork generation not found")?;
                if matches!(fork.state, ForkState::Cancelling | ForkState::Superseded) {
                    return Ok(0);
                }
                if fork.state != ForkState::Open || fork.repair_to.is_none() {
                    return Err("fork has no published repair transition".into());
                }
                if remaining_loops == 0 {
                    return Err("repair traversal budget exhausted".into());
                }
                fork.state = ForkState::Cancelling;
                for activation in &mut self.activations {
                    if activation
                        .tokens
                        .iter()
                        .any(|t| t.fork_id == *fork_id && t.generation == *generation)
                        && activation.state != ActivationState::Completed
                    {
                        activation.state = ActivationState::Superseded;
                        // Keep owned resource claims until the stop observation commits.
                        activation.resource_wait = None;
                    }
                }
                // Nested obligations are terminated together with their owning generation.
                for nested in &mut self.forks {
                    if nested
                        .parent
                        .iter()
                        .any(|t| t.fork_id == *fork_id && t.generation == *generation)
                    {
                        nested.state = ForkState::Superseded;
                    }
                }
                return Ok(1);
            }
            GraphTransition::ForkStopped {
                fork_id,
                generation,
                ..
            } => {
                let index = self
                    .forks
                    .iter()
                    .position(|f| f.id == *fork_id && f.generation == *generation)
                    .ok_or("fork generation not found")?;
                let fork = self.forks[index].clone();
                if fork.state == ForkState::Superseded {
                    return Ok(0);
                }
                if fork.state != ForkState::Cancelling {
                    return Err("fork cancellation was not requested".into());
                }
                let predecessor = self
                    .activations
                    .iter()
                    .find(|a| a.id == fork.id)
                    .ok_or("fork activation missing")?
                    .clone();
                self.push_token(
                    fork.repair_to.ok_or("fork has no repair target")?,
                    None,
                    &predecessor,
                    fork.parent,
                )?;
                for activation in &mut self.activations {
                    if activation
                        .tokens
                        .iter()
                        .any(|t| t.fork_id == *fork_id && t.generation == *generation)
                    {
                        activation.resource_claim = None;
                    }
                }
                self.forks[index].state = ForkState::Superseded;
                return Ok(0);
            }
            _ => {}
        }
        let activation_id = match transition {
            GraphTransition::Activate { activation_id, .. }
            | GraphTransition::Complete { activation_id, .. }
            | GraphTransition::ResolvePause { activation_id, .. } => activation_id,
            _ => return Err("unsupported token transition".into()),
        };
        let index = self
            .activations
            .iter()
            .position(|a| a.id == *activation_id)
            .ok_or("activation not reached")?;
        let activation = self.activations[index].clone();
        if activation.tokens.iter().any(|token| {
            self.forks.iter().any(|f| {
                f.id == token.fork_id
                    && (f.generation != token.generation
                        || matches!(f.state, ForkState::Cancelling | ForkState::Superseded))
            })
        }) {
            return Err("activation belongs to a superseded fork generation".into());
        }
        let kind = self
            .definition
            .graph
            .nodes
            .iter()
            .find(|n| n.id == activation.node_id)
            .ok_or("activation node missing")?
            .kind
            .clone();
        if matches!(transition, GraphTransition::Activate { .. }) {
            if activation.state != ActivationState::Ready {
                return Err("graph activation is not ready".into());
            }
            if let Some(predecessor) = &activation.predecessor {
                if !self
                    .activations
                    .iter()
                    .any(|a| a.id == *predecessor && a.state == ActivationState::Completed)
                {
                    return Err("token predecessor is not completed".into());
                }
            } else if index != 0 || !matches!(kind, NodeKind::Start) {
                return Err("token predecessor missing".into());
            }
            self.activations[index].state = ActivationState::Running;
            return Ok(0);
        }
        if let GraphTransition::Complete {
            outcome, evidence, ..
        } = transition
        {
            if activation.state != ActivationState::Running {
                return Err("activation is not running".into());
            }
            match &kind {
                NodeKind::Fork {
                    join,
                    branches,
                    repair_to,
                } => {
                    if outcome.is_some() || evidence.is_some() {
                        return Err("Fork computes its branch set".into());
                    }
                    let branches = branches
                        .iter()
                        .map(|b| (b.id.clone(), b.target.clone(), b.required, None))
                        .collect();
                    self.open_fork(&activation, join, branches, repair_to.clone(), None)?;
                    self.activations[index].state = ActivationState::Completed;
                    return Ok(0);
                }
                NodeKind::ForEach {
                    join,
                    body,
                    items,
                    max_items,
                    empty,
                    receipt_evidence,
                } => {
                    if receipt_evidence.is_some() {
                        return Err("dynamic For each requires its committed source receipt".into());
                    }
                    if outcome.is_some()
                        || evidence.is_some()
                        || items.len() > usize::from(*max_items)
                    {
                        return Err("invalid bounded fan-out".into());
                    }
                    let branches = items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            (format!("item-{i}"), body.clone(), true, Some(item.clone()))
                        })
                        .collect();
                    let empty = match empty {
                        EmptyFanOut::Complete => ArrivalOutcome::Completed,
                        EmptyFanOut::Skip => ArrivalOutcome::Skipped,
                        EmptyFanOut::Fail => ArrivalOutcome::Failed,
                    };
                    self.open_fork(&activation, join, branches, None, Some(empty))?;
                    self.activations[index].state = ActivationState::Completed;
                    self.release_join(&activation.id)?;
                    return Ok(0);
                }
                NodeKind::IfElse { predicate } => {
                    if outcome.is_some() || evidence.is_some() {
                        return Err("If/else computes its typed predicate".into());
                    }
                    let value = match predicate {
                        Predicate::Input { value, .. } => *value,
                        Predicate::Receipt { node_id, outcome } => self
                            .activations
                            .iter()
                            .take(index)
                            .rev()
                            .find(|a| {
                                a.node_id == *node_id
                                    && a.state == ActivationState::Completed
                                    && a.tokens.iter().all(|t| {
                                        self.forks.iter().any(|f| {
                                            f.id == t.fork_id
                                                && f.generation == t.generation
                                                && !matches!(
                                                    f.state,
                                                    ForkState::Cancelling | ForkState::Superseded
                                                )
                                        })
                                    })
                            })
                            .map(|a| {
                                let arrival = a.tokens.last().and_then(|t| {
                                    self.forks
                                        .iter()
                                        .find(|f| f.id == t.fork_id)
                                        .and_then(|f| {
                                            f.branches.iter().find(|b| b.id == t.branch_id)
                                        })
                                        .and_then(|b| b.arrival)
                                });
                                arrival.unwrap_or(ArrivalOutcome::Completed).label() == outcome
                            }),
                    };
                    let label = match value {
                        Some(true) => "yes",
                        Some(false) => "no",
                        None => "uncertain",
                    };
                    return self
                        .follow_parallel_edge(index, &activation, label, ArrivalOutcome::Completed)
                        .map(|_| 0);
                }
                NodeKind::AllJoin { .. } => {
                    if outcome.is_some() || evidence.is_some() {
                        return Err("all-Join outcome is computed from exact arrivals".into());
                    }
                    let fork = self
                        .forks
                        .iter()
                        .find(|f| {
                            f.join == activation.node_id
                                && f.state == ForkState::Joined
                                && format!("join:{}", f.id) == activation.id
                        })
                        .ok_or("all-Join obligations are unresolved")?;
                    let outcome = fork.empty.unwrap_or_else(|| {
                        fork.branches
                            .iter()
                            .filter(|b| b.required)
                            .filter_map(|b| b.arrival)
                            .find(|a| *a != ArrivalOutcome::Completed)
                            .unwrap_or(ArrivalOutcome::Completed)
                    });
                    return self
                        .follow_parallel_edge(index, &activation, outcome.label(), outcome)
                        .map(|_| 0);
                }
                _ => {}
            }
            // Crossing the matching region boundary records an arrival, not a second join visit.
            if outcome.is_none()
                && evidence.is_none()
                && let Some(token) = activation.tokens.last()
            {
                let join = self
                    .forks
                    .iter()
                    .find(|f| f.id == token.fork_id)
                    .ok_or("fork missing")?
                    .join
                    .clone();
                if self
                    .definition
                    .graph
                    .edges
                    .iter()
                    .any(|e| e.from == activation.node_id && e.to == join && e.outcome.is_none())
                {
                    return self
                        .arrive(activation_id, token, ArrivalOutcome::Completed)
                        .map(|_| 0);
                }
            }
        }
        let before = self.activations.len();
        let result = self.apply(transition, remaining_loops)?;
        if let Some(token) = activation.tokens.last()
            && self.activations.get(before).is_some_and(|next| {
                self.forks
                    .iter()
                    .any(|f| f.id == token.fork_id && f.join == next.node_id)
            })
        {
            self.activations.pop();
            self.activations[index].state = ActivationState::Running;
            let failed = matches!(
                transition,
                GraphTransition::Complete {
                    outcome: Some(
                        super::graph::EdgeOutcome::Fail
                            | super::graph::EdgeOutcome::Uncertain
                            | super::graph::EdgeOutcome::Blocked
                    ),
                    ..
                }
            );
            self.arrive(
                &activation.id,
                token,
                if failed {
                    ArrivalOutcome::Failed
                } else {
                    ArrivalOutcome::Completed
                },
            )?;
            return Ok(result);
        }
        for successor in &mut self.activations[before..] {
            successor.predecessor = Some(activation.id.clone());
            successor.tokens = activation.tokens.clone();
        }
        Ok(result)
    }

    fn follow_parallel_edge(
        &mut self,
        index: usize,
        activation: &Activation,
        label: &str,
        arrival: ArrivalOutcome,
    ) -> Result<(), String> {
        let (edge_index, edge) = self
            .definition
            .graph
            .edges
            .iter()
            .enumerate()
            .find(|(_, e)| e.from == activation.node_id && e.outcome.as_deref() == Some(label))
            .ok_or("typed outcome has no published edge")?;
        let target = edge.to.clone();
        if let Some(token) = activation.tokens.last()
            && self
                .forks
                .iter()
                .any(|f| f.id == token.fork_id && f.join == target)
        {
            return self.arrive(&activation.id, token, arrival);
        }
        self.push_token(
            target,
            Some(edge_index),
            activation,
            activation.tokens.clone(),
        )?;
        self.activations[index].state = ActivationState::Completed;
        Ok(())
    }

    fn open_fork(
        &mut self,
        activation: &Activation,
        join: &str,
        branches: Vec<(String, String, bool, Option<String>)>,
        repair_to: Option<String>,
        empty: Option<ArrivalOutcome>,
    ) -> Result<(), String> {
        let generation = self
            .forks
            .iter()
            .filter(|f| f.node_id == activation.node_id)
            .map(|f| f.generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("fork generation overflow")?;
        if self.activations.len() + branches.len() + 1 > super::graph::MAX_ACTIVATIONS {
            return Err("parallel activation budget exhausted".into());
        }
        let instance = ForkInstance {
            id: activation.id.clone(),
            node_id: activation.node_id.clone(),
            join: join.into(),
            generation,
            parent: activation.tokens.clone(),
            branches: branches
                .iter()
                .map(|(id, _, required, item)| BranchObligation {
                    id: id.clone(),
                    required: *required,
                    item: item.clone(),
                    arrival: None,
                })
                .collect(),
            state: ForkState::Open,
            repair_to,
            empty: branches.is_empty().then_some(empty).flatten(),
        };
        for (id, target, _, _) in branches {
            let mut tokens = activation.tokens.clone();
            tokens.push(BranchToken {
                fork_id: instance.id.clone(),
                generation,
                branch_id: id,
            });
            self.push_token(target, None, activation, tokens)?;
        }
        self.forks.push(instance);
        Ok(())
    }

    fn arrive(
        &mut self,
        activation_id: &str,
        token: &BranchToken,
        outcome: ArrivalOutcome,
    ) -> Result<(), String> {
        let index = self
            .activations
            .iter()
            .position(|a| a.id == activation_id)
            .ok_or("arrival activation missing")?;
        if self.activations[index].tokens.last() != Some(token) {
            return Err("arrival token does not own activation".into());
        }
        let fork = self
            .forks
            .iter_mut()
            .find(|f| f.id == token.fork_id && f.generation == token.generation)
            .ok_or("arrival fork generation missing")?;
        if matches!(fork.state, ForkState::Cancelling | ForkState::Superseded) {
            return Ok(());
        }
        let branch = fork
            .branches
            .iter_mut()
            .find(|b| b.id == token.branch_id)
            .ok_or("arrival branch is not declared")?;
        if let Some(previous) = branch.arrival {
            return if previous == outcome {
                Ok(())
            } else {
                Err("conflicting branch arrival".into())
            };
        }
        if self.activations[index].state != ActivationState::Running {
            return Err("arrival activation is not running".into());
        }
        branch.arrival = Some(outcome);
        self.activations[index].state = ActivationState::Completed;
        self.release_join(&token.fork_id)
    }

    fn release_join(&mut self, fork_id: &str) -> Result<(), String> {
        let index = self
            .forks
            .iter()
            .position(|f| f.id == fork_id)
            .ok_or("fork missing")?;
        let fork = self.forks[index].clone();
        if fork.state != ForkState::Open || fork.branches.iter().any(|b| b.arrival.is_none()) {
            return Ok(());
        }
        self.enqueue(None, fork.join)?;
        let activation = self.activations.last_mut().expect("join enqueued");
        activation.id = format!("join:{}", fork.id);
        activation.predecessor = Some(fork.id);
        activation.tokens = fork.parent;
        self.forks[index].state = ForkState::Joined;
        Ok(())
    }
}
