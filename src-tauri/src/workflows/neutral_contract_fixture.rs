//! Explicit test-only control authoring for pre-runtime data-contract fixtures.
//! This is not a production fallback, a runtime oracle, or a graph repair path.

use super::WorkflowStore;
use super::catalog::{HandlerRef, WorkflowCatalog};
use super::condition::{Comparison, Condition};
use super::data_binding::DataSource;
use super::neutral_control::{ControlGraph, ControlNode};
use super::neutral_publication::{NeutralDefinition, NeutralPublication};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn authored_control(
    definition: &mut NeutralDefinition,
    catalog: &WorkflowCatalog,
) -> ControlGraph {
    let mut units = Vec::new();
    let mut bodies = BTreeSet::new();
    for (scope, contract) in &definition.collections {
        bodies.insert(contract.body);
        units.push((
            contract.body,
            format!("collection:{scope}"),
            ControlNode::Collection {
                scope: scope.clone(),
            },
            BTreeSet::from(["complete".into(), "blocked".into()]),
        ));
    }
    for (scope, contract) in &definition.forks {
        let first = contract
            .branches
            .values()
            .map(|branch| branch.activity)
            .min()
            .unwrap_or(0);
        bodies.extend(contract.branches.values().map(|branch| branch.activity));
        units.push((
            first,
            format!("fork:{scope}"),
            ControlNode::Fork {
                scope: scope.clone(),
            },
            BTreeSet::from(["complete".into(), "blocked".into()]),
        ));
    }
    for (scope, contract) in &definition.loops {
        bodies.insert(contract.body);
        let condition = format!("fixture-stop:{scope}");
        definition.conditions.insert(
            condition.clone(),
            Condition::Compare {
                comparison: Comparison::Equal,
                left: DataSource::Constant {
                    value: json!(false),
                },
                right: DataSource::Constant { value: json!(true) },
            },
        );
        units.push((
            contract.body,
            format!("loop:{scope}"),
            ControlNode::Loop {
                scope: scope.clone(),
                condition,
            },
            BTreeSet::from([
                "complete".into(),
                "exhausted".into(),
                "unknown".into(),
                "failed".into(),
            ]),
        ));
    }
    for (activity, instance) in definition
        .instances
        .iter()
        .enumerate()
        .filter(|(activity, _)| !bodies.contains(activity))
    {
        // Invalid descriptor fixtures still reach the real publication validator;
        // no missing descriptor is registered or repaired by this test authoring.
        let ports = catalog
            .get(&instance.descriptor)
            .map(|descriptor| descriptor.outcomes.clone())
            .unwrap_or_default();
        units.push((
            activity,
            format!("activity:{activity}"),
            ControlNode::Activity { activity },
            ports,
        ));
    }
    units.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut nodes = BTreeMap::from([("start".into(), ControlNode::Start)]);
    let mut edges = BTreeMap::new();
    let mut previous = "start".to_string();
    let mut ports = BTreeSet::from(["next".to_string()]);
    for (_, id, node, next_ports) in units {
        edges.insert(
            previous,
            ports.into_iter().map(|port| (port, id.clone())).collect(),
        );
        nodes.insert(id.clone(), node);
        previous = id;
        ports = next_ports;
    }
    // Contract fixtures declare a single successful return. New control tests
    // author all routing literally and never use this helper as their oracle.
    let outcome = definition
        .outcomes
        .iter()
        .next()
        .cloned()
        .unwrap_or_default();
    edges.insert(
        previous,
        ports.into_iter().map(|port| (port, "end".into())).collect(),
    );
    nodes.insert("end".into(), ControlNode::End { outcome });
    ControlGraph { nodes, edges }
}

impl WorkflowStore {
    pub(crate) fn save_neutral_contract_fixture(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        mut definition: NeutralDefinition,
        catalog: &WorkflowCatalog,
        handlers: &BTreeSet<HandlerRef>,
    ) -> Result<NeutralPublication, String> {
        definition.control = authored_control(&mut definition, catalog);
        self.save_neutral_publication(project, id, revision, definition, catalog, handlers)
    }
}
