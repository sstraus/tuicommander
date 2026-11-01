//! Same-daemon Calls share their owning run's scheduler and effect ledger.
use super::graph::{ActivationState, DecisionEvidence, EdgeOutcome, GraphExecution};
use super::{RunSnapshot, contracts::GraphContract};
use crate::workflows::{NodeKind, PublishedWorkflow, WorkflowKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const MAX_CALL_DEPTH: usize = 4;
pub(super) const MAX_CALLS: usize = 32;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallOutcome {
    Completed,
    Blocked,
    Failed,
    Cancelled,
    Uncertain,
}
impl CallOutcome {
    pub(super) fn edge(self) -> EdgeOutcome {
        match self {
            Self::Completed => EdgeOutcome::Completed,
            Self::Blocked => EdgeOutcome::Blocked,
            Self::Failed => EdgeOutcome::Failed,
            Self::Cancelled => EdgeOutcome::Cancelled,
            Self::Uncertain => EdgeOutcome::Uncertain,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallBinding {
    pub execution_id: String,
    pub activation_id: String,
    pub inputs: BTreeMap<String, Option<bool>>,
    pub outputs: Vec<String>,
    /// A request is not proof that an external command process has stopped.
    pub stop_requested: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageCoverage {
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallReceipt {
    pub outcome: CallOutcome,
    pub outputs: BTreeMap<String, Option<bool>>,
    /// Set union, never sum an inclusive parent and its nested child again.
    pub accounting_execution_ids: BTreeSet<String>,
    /// DEFERRED (2026-10-31) — WF12 owns provider usage ingestion. This run contract
    /// has no money/token ledger; retain deduplicated identities and unknown, never zero.
    pub usage_coverage: UsageCoverage,
}

pub(crate) fn validate_fields(
    definition_id: &str,
    revision: i64,
    inputs: &BTreeMap<String, Option<bool>>,
    outputs: &[String],
) -> Result<(), String> {
    let name = |s: &str| {
        !s.is_empty()
            && s.len() <= 80
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    if definition_id.is_empty()
        || definition_id.len() > 80
        || revision < 1
        || inputs.len() > 16
        || outputs.len() > 16
        || inputs.keys().any(|s| !name(s))
        || outputs.iter().any(|s| !name(s))
        || outputs.iter().collect::<BTreeSet<_>>().len() != outputs.len()
    {
        return Err(
            "Call needs an exact definition revision and bounded unique boolean input/output names"
                .into(),
        );
    }
    Ok(())
}

/// Resolve at publication/start only. Replay supplies a reader over persisted pins.
pub(crate) fn definition_closure(
    root: &PublishedWorkflow,
    read: &impl Fn(&str, i64) -> Result<PublishedWorkflow, String>,
) -> Result<Vec<PublishedWorkflow>, String> {
    fn walk(
        definition: &PublishedWorkflow,
        read: &impl Fn(&str, i64) -> Result<PublishedWorkflow, String>,
        path: &mut Vec<String>,
        depth: usize,
        count: &mut usize,
        pins: &mut BTreeMap<(String, i64), PublishedWorkflow>,
    ) -> Result<(), String> {
        if depth > MAX_CALL_DEPTH || path.contains(&definition.id) {
            return Err("Call recursion or depth exceeds the publication bound".into());
        }
        path.push(definition.id.clone());
        for node in &definition.graph.nodes {
            let (id, revision, call) = match &node.kind {
                NodeKind::Call {
                    definition_id,
                    revision,
                    ..
                } => (definition_id, *revision, true),
                NodeKind::StoryDispatch {
                    story_template_id,
                    story_revision,
                } => (story_template_id, *story_revision, false),
                _ => continue,
            };
            let child = read(id, revision)?;
            if child.id != *id
                || child.revision != revision
                || child.project != definition.project
                || child.kind != WorkflowKind::Story
            {
                return Err(
                    "Call/dispatch must pin a same-project published story definition".into(),
                );
            }
            if !call
                && child
                    .graph
                    .nodes
                    .iter()
                    .any(|n| matches!(n.kind, NodeKind::Return { .. }))
            {
                return Err("Story Dispatch cannot deliver a reusable Return subflow".into());
            }
            if call {
                *count += 1;
                if *count > MAX_CALLS {
                    return Err("expanded Call fan-out exceeds 32".into());
                }
                validate_subflow(&child, &node.kind)?;
            }
            walk(&child, read, path, depth + usize::from(call), count, pins)?;
            pins.insert((id.clone(), revision), child);
        }
        path.pop();
        Ok(())
    }
    let mut pins = BTreeMap::new();
    let mut count = 0;
    walk(root, read, &mut vec![], 0, &mut count, &mut pins)?;
    // Conservative bound includes repeated root invocations, not just unique definitions.
    let repeats = root
        .graph
        .nodes
        .iter()
        .filter_map(|n| match n.kind {
            NodeKind::Loop { max_iterations } => Some(usize::from(max_iterations) + 1),
            _ => None,
        })
        .fold(1usize, |total, n| total.saturating_mul(n));
    if count.saturating_mul(repeats) > MAX_CALLS {
        return Err("Call expansion including bounded root loops exceeds 32".into());
    }
    Ok(pins.into_values().collect())
}

/// Publication and Call binding share one reusable-shape validator. Return cannot
/// waive delivery checks on a graph that still contains delivery/Agent nodes.
pub(crate) fn reusable_signature(
    child: &PublishedWorkflow,
) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    let mut declared = BTreeSet::new();
    let mut outputs: Option<BTreeSet<String>> = None;
    for node in &child.graph.nodes {
        match &node.kind {
            NodeKind::Start | NodeKind::Notify | NodeKind::Call { .. }
                | NodeKind::RunCommand { .. } | NodeKind::Tool { .. } => {},
            NodeKind::IfElse { predicate } => {
                if let crate::workflows::definition::Predicate::Input { name, value: None } = predicate {
                    declared.insert(name.clone());
                }
            },
            NodeKind::Return { outputs: returned, .. } => {
                let keys = returned.keys().cloned().collect();
                if outputs.as_ref().is_some_and(|previous| previous != &keys) {
                    return Err("all typed Returns must declare the same output names".into());
                }
                outputs = Some(keys);
            },
            _ => return Err("Call accepts only bounded reusable decision/check subflows, not story delivery, loops or fan-out".into()),
        }
    }
    let outputs = outputs.ok_or("reusable subflow needs a typed Return")?;
    if !child.required_checks.is_empty() || child.graph.evidence_policy.is_some() {
        return Err(
            "reusable Return definitions use explicit check nodes, not implicit delivery checks"
                .into(),
        );
    }
    Ok((declared, outputs))
}

fn validate_subflow(child: &PublishedWorkflow, call: &NodeKind) -> Result<(), String> {
    let NodeKind::Call {
        inputs, outputs, ..
    } = call
    else {
        unreachable!()
    };
    let (declared, returned) = reusable_signature(child)?;
    if returned != outputs.iter().cloned().collect() {
        return Err("Call output declaration differs from a typed Return".into());
    }
    if declared != inputs.keys().cloned().collect() {
        return Err("Call inputs must exactly match the subflow's declared boolean inputs".into());
    }
    Ok(())
}

pub(super) fn pin_contract(
    root: &PublishedWorkflow,
    policy: Option<crate::workflows::AgentPolicy>,
    pins: Vec<PublishedWorkflow>,
) -> Result<GraphContract, String> {
    let canonical = definition_closure(root, &|id, revision| {
        pins.iter()
            .find(|p| p.id == id && p.revision == revision)
            .cloned()
            .ok_or_else(|| "transitive definition pin missing".into())
    })?;
    if canonical != pins {
        return Err("definition pins are not the complete canonical closure".into());
    }
    let mut contract = GraphContract::with_agent_policy(root, policy.clone())?;
    for pin in &pins {
        if let Some(policy) = &policy {
            policy.validate_definition(pin)?;
        }
    }
    contract.definition_pins = pins;
    Ok(contract)
}

pub(super) fn child_id(execution_id: &str, activation_id: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "call:{}",
        hex::encode(Sha256::digest(format!("{execution_id}\0{activation_id}")))
    )
}

pub(super) fn start(
    run: &RunSnapshot,
    parent_id: &str,
    activation_id: &str,
) -> Result<GraphExecution, String> {
    if run.event_contract_version < 6 {
        return Err("Call requires event contract 6".into());
    }
    if run
        .graph_executions
        .iter()
        .filter(|g| g.call_binding.is_some())
        .count()
        >= MAX_CALLS
    {
        return Err("run Call budget exhausted".into());
    }
    let parent = run
        .graph_executions
        .iter()
        .find(|g| g.id == parent_id)
        .ok_or("Call parent missing")?;
    let activation = parent
        .activations
        .iter()
        .find(|a| a.id == activation_id && a.state == ActivationState::Running)
        .ok_or("Call activation is not running")?;
    let NodeKind::Call {
        definition_id,
        revision,
        inputs,
        outputs,
    } = &parent
        .definition
        .graph
        .nodes
        .iter()
        .find(|n| n.id == activation.node_id)
        .ok_or("Call node missing")?
        .kind
    else {
        return Err("activation is not a Call".into());
    };
    let contract = parent.contract.as_ref().ok_or("Call parent has no pins")?;
    let definition = contract
        .definition_pins
        .iter()
        .find(|p| p.id == *definition_id && p.revision == *revision)
        .ok_or("Call definition pin missing")?;
    let mut execution = GraphExecution::start(
        child_id(parent_id, activation_id),
        parent.target_id.clone(),
        definition.clone(),
    )?;
    execution.contract = Some(pin_contract(
        definition,
        contract.agent_policy.clone(),
        definition_closure(definition, &|id, revision| {
            contract
                .definition_pins
                .iter()
                .find(|p| p.id == id && p.revision == revision)
                .cloned()
                .ok_or_else(|| "nested definition pin missing".into())
        })?,
    )?);
    execution.call_binding = Some(CallBinding {
        execution_id: parent_id.into(),
        activation_id: activation_id.into(),
        inputs: inputs.clone(),
        outputs: outputs.clone(),
        stop_requested: false,
    });
    Ok(execution)
}

pub(super) fn decision(
    run: &RunSnapshot,
    parent: &GraphExecution,
    activation_id: &str,
) -> Option<(EdgeOutcome, DecisionEvidence)> {
    let child = run
        .graph_executions
        .iter()
        .find(|g| g.id == child_id(&parent.id, activation_id))?;
    let receipt = child.call_receipt.as_ref().filter(|_| child.completed)?;
    Some((
        receipt.outcome.edge(),
        DecisionEvidence {
            actor: "daemon".into(),
            reason: "Pinned child reached a typed terminal Return".into(),
            references: vec![format!("call-receipt:{}", child.id)],
        },
    ))
}

pub(super) fn validate_completion(
    run: &RunSnapshot,
    transition: &super::graph::GraphTransition,
) -> Result<(), String> {
    let super::graph::GraphTransition::Complete {
        execution_id,
        activation_id,
        outcome,
        evidence,
    } = transition
    else {
        return Ok(());
    };
    let Some(parent) = run.graph_executions.iter().find(|g| g.id == *execution_id) else {
        return Ok(());
    };
    let is_call = parent
        .activations
        .iter()
        .find(|a| a.id == *activation_id)
        .is_some_and(|a| {
            parent
                .definition
                .graph
                .nodes
                .iter()
                .any(|n| n.id == a.node_id && matches!(n.kind, NodeKind::Call { .. }))
        });
    if is_call {
        let (expected, proof) =
            decision(run, parent, activation_id).ok_or("Call has no terminal child receipt")?;
        if *outcome != Some(expected) || evidence.as_ref() != Some(&proof) {
            return Err("Call completion differs from its typed child receipt".into());
        }
    }
    Ok(())
}
