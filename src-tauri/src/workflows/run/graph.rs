//! Versioned graph projections. No scheduler or external effects run here.
use crate::workflows::{NodeKind, PublishedWorkflow, validate_executable_graph};
use serde::{Deserialize, Serialize};

/// Run event contract that owns serial graph transition and replay semantics.
pub const RUN_EVENT_CONTRACT_VERSION: u16 = super::contracts::CURRENT_EVENT_CONTRACT_VERSION;

/// Every event snapshots all activations, so the bound caps per-run storage.
pub(super) const MAX_ACTIVATIONS: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationState {
    Ready,
    Running,
    Completed,
    Paused,
    Superseded,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Activation {
    pub id: String,
    pub node_id: String,
    pub state: ActivationState,
    pub edge_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<super::parallel::BranchToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_wait: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_claim: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeOutcome {
    Yes,
    No,
    Uncertain,
    Pass,
    Fail,
    Completed,
    Blocked,
}

impl EdgeOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Uncertain => "uncertain",
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionEvidence {
    pub actor: String,
    pub reason: String,
    pub references: Vec<String>,
}

impl DecisionEvidence {
    fn validate(&self) -> Result<(), String> {
        let actor = &self.actor;
        if actor.trim().is_empty()
            || actor.len() > 256
            || self.reason.trim().is_empty()
            || self.reason.len() > 4096
            || self.references.is_empty()
            || self.references.len() > 32
            || self
                .references
                .iter()
                .any(|reference| reference.trim().is_empty() || reference.len() > 4096)
        {
            return Err(
                "graph decision needs bounded actor, reason and evidence references".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphDecision {
    pub activation_id: String,
    pub edge_index: usize,
    pub evidence: DecisionEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoopCounter {
    pub node_id: String,
    pub repeats: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphPause {
    pub activation_id: String,
    pub resume_to: String,
    pub evidence: DecisionEvidence,
    pub resolution: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphExecution {
    pub id: String,
    pub target_id: String,
    pub definition: PublishedWorkflow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<super::contracts::GraphContract>,
    pub activations: Vec<Activation>,
    pub decisions: Vec<GraphDecision>,
    pub loops: Vec<LoopCounter>,
    pub pauses: Vec<GraphPause>,
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forks: Vec<super::parallel::ForkInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<super::parallel::ScopedCheck>,
}

impl GraphExecution {
    /// Command-time start: the definition must pass executable validation.
    pub(super) fn start(
        id: String,
        target_id: String,
        definition: PublishedWorkflow,
    ) -> Result<Self, String> {
        validate_executable_graph(
            &definition.graph,
            definition.kind,
            !definition.required_checks.is_empty(),
        )?;
        let contract = super::contracts::GraphContract::serial(&definition)?;
        let mut execution = Self::pristine(id, target_id, definition)?;
        execution.contract = Some(contract);
        Ok(execution)
    }

    /// Initial state of a pinned definition. Replay uses this alone, so a later
    /// validator change cannot make a persisted run unreplayable.
    pub(super) fn pristine(
        id: String,
        target_id: String,
        definition: PublishedWorkflow,
    ) -> Result<Self, String> {
        if id.trim().is_empty()
            || id.len() > 80
            || target_id.trim().is_empty()
            || definition.revision < 1
        {
            return Err("invalid graph execution identity or revision".into());
        }
        let start = definition
            .graph
            .nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Start))
            .ok_or("graph Start is missing")?;
        let root = Activation {
            id: "a0".into(),
            node_id: start.id.clone(),
            state: ActivationState::Ready,
            edge_index: None,
            predecessor: None,
            tokens: vec![],
            resource_wait: None,
            resource_claim: None,
        };
        Ok(Self {
            id,
            target_id,
            definition,
            contract: None,
            activations: vec![root],
            decisions: vec![],
            loops: vec![],
            pauses: vec![],
            completed: false,
            forks: vec![],
            checks: vec![],
        })
    }

    /// Keep the run's contract when a version-2 run dispatches a later child.
    pub(super) fn start_for_contract(
        id: String,
        target_id: String,
        definition: PublishedWorkflow,
        version: u16,
    ) -> Result<Self, String> {
        let mut execution = Self::start(id, target_id, definition)?;
        match version {
            super::contracts::SERIAL_EVENT_CONTRACT_VERSION => execution.contract = None,
            super::contracts::PINNED_SERIAL_EVENT_CONTRACT_VERSION => {
                execution
                    .contract
                    .as_mut()
                    .expect("pinned contract")
                    .version = version;
            }
            super::contracts::CURRENT_EVENT_CONTRACT_VERSION => {}
            _ => return Err("unsupported workflow graph event contract".into()),
        }
        Ok(execution)
    }

    /// Versions 2 and 3 share the frozen exclusive/serial implementation.
    /// Version 4 uses explicit branch tokens; historical Join stays exclusive.
    pub(super) fn apply_for_contract(
        &mut self,
        transition: &GraphTransition,
        remaining_loops: u16,
        version: u16,
    ) -> Result<u16, String> {
        match version {
            super::contracts::CURRENT_EVENT_CONTRACT_VERSION => {
                self.apply_parallel(transition, remaining_loops)
            }
            super::contracts::SERIAL_EVENT_CONTRACT_VERSION
            | super::contracts::PINNED_SERIAL_EVENT_CONTRACT_VERSION => {
                self.apply(transition, remaining_loops)
            }
            _ => Err("unsupported workflow graph event contract".into()),
        }
    }

    pub(super) fn enqueue(
        &mut self,
        edge_index: Option<usize>,
        target: String,
    ) -> Result<(), String> {
        if self.activations.len() >= MAX_ACTIVATIONS {
            return Err("graph activation budget exhausted".into());
        }
        let id = format!("a{}", self.activations.len());
        self.activations.push(Activation {
            id,
            node_id: target,
            state: ActivationState::Ready,
            edge_index,
            predecessor: None,
            tokens: vec![],
            resource_wait: None,
            resource_claim: None,
        });
        Ok(())
    }

    /// Validate and project a committed transition without performing effects.
    pub(super) fn apply(
        &mut self,
        transition: &GraphTransition,
        remaining_loops: u16,
    ) -> Result<u16, String> {
        if self.completed {
            return Err("completed graph cannot advance".into());
        }
        let activation_id = match transition {
            GraphTransition::Activate { activation_id, .. }
            | GraphTransition::Complete { activation_id, .. }
            | GraphTransition::ResolvePause { activation_id, .. } => activation_id,
            _ => return Err("transition is not part of the serial contract".into()),
        };
        let index = self
            .activations
            .iter()
            .position(|a| a.id == *activation_id)
            .ok_or("graph activation not reached")?;
        let activation = self.activations[index].clone();
        let kind = self
            .definition
            .graph
            .nodes
            .iter()
            .find(|node| node.id == activation.node_id)
            .ok_or("activation node is missing")?
            .kind
            .clone();
        let paused = self.pauses.iter().any(|pause| pause.resolution.is_none());
        match transition {
            GraphTransition::Activate { .. } => {
                if paused || activation.state != ActivationState::Ready {
                    return Err("graph activation is not ready".into());
                }
                if let Some(previous) = index.checked_sub(1) {
                    let predecessor = self
                        .activations
                        .get(previous)
                        .filter(|previous| previous.state == ActivationState::Completed)
                        .ok_or("activation predecessor is not completed")?;
                    if let Some(edge_index) = activation.edge_index {
                        let edge = self
                            .definition
                            .graph
                            .edges
                            .get(edge_index)
                            .ok_or("activation predecessor edge is missing")?;
                        if edge.from != predecessor.node_id || edge.to != activation.node_id {
                            return Err("activation predecessor edge does not match".into());
                        }
                    } else if !self.pauses.iter().any(|pause| {
                        pause.activation_id == predecessor.id
                            && pause.resume_to == activation.node_id
                            && pause.resolution.is_some()
                    }) {
                        return Err("activation predecessor pause is unresolved".into());
                    }
                } else if !matches!(kind, NodeKind::Start) || activation.edge_index.is_some() {
                    return Err("root activation must be an unlinked Start".into());
                }
                self.activations[index].state = ActivationState::Running;
            }
            GraphTransition::Complete {
                outcome, evidence, ..
            } => {
                if paused || activation.state != ActivationState::Running {
                    return Err("graph activation is not running".into());
                }
                if let NodeKind::Pause { resume_to } = kind {
                    if outcome.is_some() {
                        return Err("Pause has no outcome edge".into());
                    }
                    let evidence = evidence.as_ref().ok_or("Pause needs a recorded reason")?;
                    evidence.validate()?;
                    self.pauses.push(GraphPause {
                        activation_id: activation.id.clone(),
                        resume_to: resume_to.ok_or("Pause has no resume_to")?,
                        evidence: evidence.clone(),
                        resolution: None,
                    });
                    self.activations[index].state = ActivationState::Paused;
                    return Ok(0);
                }
                if matches!(kind, NodeKind::End) {
                    if outcome.is_some() || evidence.is_some() {
                        return Err("End has no outcome".into());
                    }
                    self.activations[index].state = ActivationState::Completed;
                    self.completed = true;
                    return Ok(0);
                }
                let mut repeats = 0;
                let label = match kind {
                    NodeKind::Judge | NodeKind::Gate | NodeKind::StoryDispatch { .. } => {
                        let evidence = evidence.as_ref().ok_or("decision needs evidence")?;
                        evidence.validate()?;
                        Some(outcome.ok_or("decision needs one outcome")?.label())
                    }
                    NodeKind::Loop { max_iterations } => {
                        if outcome.is_some() || evidence.is_some() {
                            return Err("Loop outcome is computed from its pinned cap".into());
                        }
                        let used = self
                            .loops
                            .iter()
                            .find(|counter| counter.node_id == activation.node_id)
                            .map_or(0, |counter| counter.repeats);
                        if used < max_iterations && remaining_loops > 0 {
                            repeats = 1;
                            Some("repeat")
                        } else {
                            Some("exhausted")
                        }
                    }
                    _ => {
                        if outcome.is_some() || evidence.is_some() {
                            return Err("node needs an unlabeled completion".into());
                        }
                        None
                    }
                };
                let (edge_index, edge) = self
                    .definition
                    .graph
                    .edges
                    .iter()
                    .enumerate()
                    .find(|(_, edge)| {
                        edge.from == activation.node_id && edge.outcome.as_deref() == label
                    })
                    .ok_or("outcome has no published edge")?;
                let target = edge.to.clone();
                if let Some(evidence) = evidence {
                    self.decisions.push(GraphDecision {
                        activation_id: activation.id.clone(),
                        edge_index,
                        evidence: evidence.clone(),
                    });
                }
                if repeats > 0 {
                    if let Some(counter) = self
                        .loops
                        .iter_mut()
                        .find(|counter| counter.node_id == activation.node_id)
                    {
                        counter.repeats = counter
                            .repeats
                            .checked_add(1)
                            .ok_or("loop counter overflow")?;
                    } else {
                        self.loops.push(LoopCounter {
                            node_id: activation.node_id.clone(),
                            repeats: 1,
                        });
                    }
                }
                self.enqueue(Some(edge_index), target)?;
                self.activations[index].state = ActivationState::Completed;
                return Ok(repeats);
            }
            GraphTransition::ResolvePause { resolution, .. } => {
                if resolution.trim().is_empty() || resolution.len() > 4096 {
                    return Err("pause resolution is empty or too large".into());
                }
                if activation.state != ActivationState::Paused {
                    return Err("activation has no pending pause".into());
                }
                let pause_index = self
                    .pauses
                    .iter()
                    .position(|pause| {
                        pause.activation_id == activation.id && pause.resolution.is_none()
                    })
                    .ok_or("activation has no pending pause")?;
                let target = self.pauses[pause_index].resume_to.clone();
                self.enqueue(None, target)?;
                self.pauses[pause_index].resolution = Some(resolution.clone());
                self.activations[index].state = ActivationState::Completed;
            }
            _ => return Err("transition is not part of the serial contract".into()),
        }
        Ok(0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphTransition {
    Start {
        execution_id: String,
        target_id: String,
    },
    Activate {
        execution_id: String,
        activation_id: String,
    },
    Complete {
        execution_id: String,
        activation_id: String,
        outcome: Option<EdgeOutcome>,
        evidence: Option<DecisionEvidence>,
    },
    Expand {
        execution_id: String,
        activation_id: String,
        source_activation_id: String,
        items: Vec<String>,
    },
    ResourceClaim {
        execution_id: String,
        activation_id: String,
        key: Option<String>,
    },
    CheckBound {
        execution_id: String,
        activation_id: String,
        receipt: super::check::CheckReceipt,
    },
    ResourceWait {
        execution_id: String,
        activation_id: String,
        reason: Option<String>,
    },
    Arrive {
        execution_id: String,
        activation_id: String,
        token: super::parallel::BranchToken,
        outcome: super::parallel::ArrivalOutcome,
    },
    SupersedeFork {
        execution_id: String,
        fork_id: String,
        generation: u32,
    },
    ForkStopped {
        execution_id: String,
        fork_id: String,
        generation: u32,
    },
    ResolvePause {
        execution_id: String,
        activation_id: String,
        resolution: String,
    },
}

impl GraphTransition {
    pub(super) fn execution_id(&self) -> &str {
        match self {
            Self::Start { execution_id, .. }
            | Self::Activate { execution_id, .. }
            | Self::Complete { execution_id, .. }
            | Self::ResolvePause { execution_id, .. }
            | Self::Expand { execution_id, .. }
            | Self::ResourceClaim { execution_id, .. }
            | Self::CheckBound { execution_id, .. }
            | Self::ResourceWait { execution_id, .. }
            | Self::Arrive { execution_id, .. }
            | Self::SupersedeFork { execution_id, .. }
            | Self::ForkStopped { execution_id, .. } => execution_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphEvent {
    Started { execution: Box<GraphExecution> },
    Transition { transition: GraphTransition },
}
