//! Neutral outcome routing. Bounded scopes, not ordinary edges, own repetition.
use super::catalog::WorkflowCatalog;
use super::neutral_publication::NeutralDefinition;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ControlNode {
    Start,
    End {
        outcome: String,
    },
    Activity {
        activity: usize,
    },
    Condition {
        condition: String,
    },
    Collection {
        scope: String,
    },
    Fork {
        scope: String,
    },
    /// Evaluate after a completed body traversal. True repeats until the bound;
    /// false completes, and unknown is never interpreted as false.
    Loop {
        scope: String,
        condition: String,
    },
    Wait {
        duration_ms: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlGraph {
    pub nodes: BTreeMap<String, ControlNode>,
    pub edges: BTreeMap<String, BTreeMap<String, String>>,
}

impl ControlGraph {
    pub(crate) fn entry(&self) -> Result<&str, String> {
        let entries: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| matches!(node, ControlNode::Start).then_some(id.as_str()))
            .collect();
        match entries.as_slice() {
            [entry] => Ok(entry),
            _ => Err("neutral control graph requires exactly one start".into()),
        }
    }

    pub(crate) fn validate(
        &self,
        definition: &NeutralDefinition,
        catalog: &WorkflowCatalog,
    ) -> Result<(), String> {
        let entry = self.entry()?;
        if self.nodes.keys().any(|id| id.trim().is_empty()) {
            return Err("neutral control node identity must be nonempty".into());
        }
        if self.edges.keys().any(|id| !self.nodes.contains_key(id)) {
            return Err("neutral control edges name an unknown source".into());
        }
        let mut activities = BTreeSet::new();
        let mut scopes = BTreeSet::new();
        let mut end_outcomes = BTreeSet::new();
        let mut incoming: BTreeMap<_, usize> =
            self.nodes.keys().map(|id| (id.as_str(), 0)).collect();
        for (id, node) in &self.nodes {
            let ports: BTreeSet<String> = match node {
                ControlNode::Start | ControlNode::Wait { .. } => {
                    if let ControlNode::Wait { duration_ms } = node
                        && (*duration_ms == 0 || *duration_ms > super::duration::MAX_DURATION_MS)
                    {
                        return Err("neutral wait requires a positive bounded duration".into());
                    }
                    BTreeSet::from(["next".into()])
                }
                ControlNode::End { outcome } => {
                    if !definition.outcomes.contains(outcome) {
                        return Err("neutral end uses an undeclared workflow outcome".into());
                    }
                    end_outcomes.insert(outcome.clone());
                    BTreeSet::new()
                }
                ControlNode::Activity { activity } => {
                    claim_activity(&mut activities, *activity, definition.instances.len())?;
                    let instance = &definition.instances[*activity];
                    catalog
                        .get(&instance.descriptor)
                        .ok_or("neutral activity descriptor unavailable")?
                        .outcomes
                        .clone()
                }
                ControlNode::Condition { condition } => {
                    require_condition(definition, condition)?;
                    BTreeSet::from(["true".into(), "false".into(), "unknown".into()])
                }
                ControlNode::Collection { scope } => {
                    claim_scope(&mut scopes, scope)?;
                    let contract = definition
                        .collections
                        .get(scope)
                        .ok_or("neutral collection scope unavailable")?;
                    claim_activity(&mut activities, contract.body, definition.instances.len())?;
                    BTreeSet::from(["complete".into(), "blocked".into()])
                }
                ControlNode::Fork { scope } => {
                    claim_scope(&mut scopes, scope)?;
                    let contract = definition
                        .forks
                        .get(scope)
                        .ok_or("neutral fork scope unavailable")?;
                    for branch in contract.branches.values() {
                        claim_activity(
                            &mut activities,
                            branch.activity,
                            definition.instances.len(),
                        )?;
                    }
                    BTreeSet::from(["complete".into(), "blocked".into()])
                }
                ControlNode::Loop { scope, condition } => {
                    claim_scope(&mut scopes, scope)?;
                    require_condition(definition, condition)?;
                    let contract = definition
                        .loops
                        .get(scope)
                        .ok_or("neutral loop scope unavailable")?;
                    claim_activity(&mut activities, contract.body, definition.instances.len())?;
                    BTreeSet::from([
                        "complete".into(),
                        "exhausted".into(),
                        "unknown".into(),
                        "failed".into(),
                    ])
                }
            };
            let actual: BTreeSet<_> = self
                .edges
                .get(id)
                .into_iter()
                .flat_map(|edges| edges.keys().cloned())
                .collect();
            if actual != ports {
                return Err(format!(
                    "neutral control node {id} requires exact outcome ports {ports:?}"
                ));
            }
            for target in self
                .edges
                .get(id)
                .into_iter()
                .flat_map(|edges| edges.values())
            {
                let count = incoming
                    .get_mut(target.as_str())
                    .ok_or("neutral control edge target unavailable")?;
                *count += 1;
            }
        }
        if end_outcomes != definition.outcomes {
            return Err("neutral control ends do not cover workflow outcomes".into());
        }
        if activities.len() != definition.instances.len() {
            return Err("neutral control graph omits an activity instance".into());
        }
        let declared_scopes: BTreeSet<_> = definition
            .collections
            .keys()
            .chain(definition.loops.keys())
            .chain(definition.forks.keys())
            .cloned()
            .collect();
        if scopes != declared_scopes {
            return Err("neutral control graph omits or duplicates a declared scope".into());
        }
        // Iterative traversal avoids using the host stack for a user-defined graph.
        let mut seen = BTreeSet::new();
        let mut ready = VecDeque::from([entry]);
        while let Some(id) = ready.pop_front() {
            if seen.insert(id) {
                ready.extend(
                    self.edges
                        .get(id)
                        .into_iter()
                        .flat_map(|edges| edges.values().map(String::as_str)),
                );
            }
        }
        if seen.len() != self.nodes.len() {
            return Err("neutral control graph contains unreachable nodes".into());
        }
        let mut ready: VecDeque<_> = incoming
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        let mut ordered = 0;
        while let Some(id) = ready.pop_front() {
            ordered += 1;
            for target in self
                .edges
                .get(id)
                .into_iter()
                .flat_map(|edges| edges.values())
            {
                let count = incoming
                    .get_mut(target.as_str())
                    .expect("targets verified above");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(target.as_str());
                }
            }
        }
        if ordered != self.nodes.len() {
            return Err(
                "ordinary neutral control cycles are forbidden; use a bounded scope".into(),
            );
        }
        Ok(())
    }
}

fn claim_activity(
    activities: &mut BTreeSet<usize>,
    activity: usize,
    count: usize,
) -> Result<(), String> {
    if activity >= count {
        return Err("neutral control activity index unavailable".into());
    }
    if !activities.insert(activity) {
        return Err("neutral activity has duplicate control/scope ownership".into());
    }
    Ok(())
}

fn claim_scope(scopes: &mut BTreeSet<String>, scope: &str) -> Result<(), String> {
    if !scopes.insert(scope.into()) {
        return Err("neutral scope has duplicate control ownership".into());
    }
    Ok(())
}

fn require_condition(definition: &NeutralDefinition, condition: &str) -> Result<(), String> {
    if !definition.conditions.contains_key(condition) {
        return Err("neutral control condition unavailable".into());
    }
    Ok(())
}
