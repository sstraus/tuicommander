use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, HandlerRef, ObjectSchema, WorkflowCatalog,
};
use super::neutral_control::{ControlGraph, ControlNode};
use super::neutral_publication::NeutralDefinition;
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> (NeutralDefinition, WorkflowCatalog, ControlGraph) {
    let id = DescriptorId {
        namespace: "test".into(),
        name: "transform".into(),
        version: 1,
    };
    let mut catalog = WorkflowCatalog::new();
    catalog
        .register(ActivityDescriptor {
            id: id.clone(),
            purpose: "Transform data".into(),
            handler: HandlerRef {
                name: "host.transform".into(),
                version: 1,
            },
            target: None,
            config: ObjectSchema::default(),
            inputs: ObjectSchema::default(),
            subject: None,
            outputs: ObjectSchema::default(),
            outcomes: BTreeSet::from(["accepted".into(), "rejected".into()]),
            required_capabilities: BTreeSet::new(),
            execution_policies: BTreeSet::from(["local".into()]),
        })
        .unwrap();
    let definition = NeutralDefinition {
        control: crate::workflows::neutral_control::ControlGraph {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        },
        inputs: ObjectSchema::default(),
        subject: None,
        outputs: ObjectSchema::default(),
        outcomes: BTreeSet::from(["done".into()]),
        instances: vec![ActivityInstance {
            descriptor: id,
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        }],
        input_bindings: BTreeMap::new(),
        conditions: BTreeMap::new(),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        output_bindings: BTreeMap::new(),
        subflows: vec![],
    };
    let graph = ControlGraph {
        nodes: BTreeMap::from([
            ("start".into(), ControlNode::Start),
            ("work".into(), ControlNode::Activity { activity: 0 }),
            (
                "end".into(),
                ControlNode::End {
                    outcome: "done".into(),
                },
            ),
        ]),
        edges: BTreeMap::from([
            (
                "start".into(),
                BTreeMap::from([("next".into(), "work".into())]),
            ),
            (
                "work".into(),
                BTreeMap::from([
                    ("accepted".into(), "end".into()),
                    ("rejected".into(), "end".into()),
                ]),
            ),
        ]),
    };
    (definition, catalog, graph)
}

fn publish(
    graph: &ControlGraph,
    definition: &NeutralDefinition,
    catalog: &WorkflowCatalog,
) -> Result<super::neutral_publication::NeutralPublication, String> {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = super::WorkflowStore::open_at(&dir.path().join("control.sqlite3")).unwrap();
    let mut definition = definition.clone();
    definition.control = graph.clone();
    store.save_neutral_publication(
        "/project",
        "control",
        0,
        definition,
        catalog,
        &BTreeSet::from([HandlerRef {
            name: "host.transform".into(),
            version: 1,
        }]),
    )
}

// Catches: missing/extra named outcomes or dangling targets accepted as an
// executable graph, silently stranding the corresponding activity result.
#[test]
fn neutral_control_requires_exact_descriptor_outcomes_and_real_targets() {
    let (definition, catalog, graph) = fixture();
    publish(&graph, &definition, &catalog).unwrap();
    assert_eq!(graph.entry().unwrap(), "start");
    let mut cases = Vec::new();
    let mut invalid = graph.clone();
    invalid.edges.get_mut("work").unwrap().remove("rejected");
    cases.push((invalid, "exact outcome"));
    let mut invalid = graph.clone();
    invalid
        .edges
        .get_mut("work")
        .unwrap()
        .insert("guessed".into(), "end".into());
    cases.push((invalid, "exact outcome"));
    let mut invalid = graph.clone();
    invalid
        .edges
        .get_mut("work")
        .unwrap()
        .insert("rejected".into(), "missing".into());
    cases.push((invalid, "target unavailable"));
    let mut invalid = graph.clone();
    invalid.nodes.insert(
        "end".into(),
        ControlNode::End {
            outcome: "undeclared".into(),
        },
    );
    cases.push((invalid, "undeclared"));
    let errors: Vec<_> = cases
        .into_iter()
        .filter_map(
            |(invalid, expected)| match publish(&invalid, &definition, &catalog) {
                Err(error) if error.contains(expected) => None,
                actual => Some(format!("expected {expected}: {actual:?}")),
            },
        )
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

// Catches: ordinary cycles bypassing bounded scope budgets, or unreachable nodes
// being accepted and later mistaken for runnable work.
#[test]
fn neutral_control_rejects_cycles_unreachable_nodes_and_multiple_starts() {
    let (definition, catalog, graph) = fixture();
    let mut cases = Vec::new();
    let mut invalid = graph.clone();
    invalid
        .edges
        .get_mut("work")
        .unwrap()
        .insert("rejected".into(), "work".into());
    cases.push((invalid, "cycles"));
    let mut invalid = graph.clone();
    invalid.nodes.insert(
        "hidden".into(),
        ControlNode::End {
            outcome: "done".into(),
        },
    );
    cases.push((invalid, "unreachable"));
    let mut invalid = graph.clone();
    invalid
        .nodes
        .insert("another-start".into(), ControlNode::Start);
    cases.push((invalid, "exactly one start"));
    let errors: Vec<_> = cases
        .into_iter()
        .filter_map(
            |(invalid, expected)| match publish(&invalid, &definition, &catalog) {
                Err(error) if error.contains(expected) => None,
                actual => Some(format!("expected {expected}: {actual:?}")),
            },
        )
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

// Catches: an instance executing twice under distinct graph labels, or omitted
// instances silently disappearing from executable control flow.
#[test]
fn neutral_control_has_exact_activity_ownership() {
    let (definition, catalog, graph) = fixture();
    let mut duplicated = graph.clone();
    duplicated
        .nodes
        .insert("again".into(), ControlNode::Activity { activity: 0 });
    duplicated.edges.insert(
        "again".into(),
        BTreeMap::from([
            ("accepted".into(), "end".into()),
            ("rejected".into(), "end".into()),
        ]),
    );
    assert!(
        publish(&duplicated, &definition, &catalog)
            .unwrap_err()
            .contains("duplicate")
    );
    let mut omitted = graph.clone();
    omitted
        .nodes
        .insert("work".into(), ControlNode::Wait { duration_ms: 1 });
    omitted.edges.insert(
        "work".into(),
        BTreeMap::from([("next".into(), "end".into())]),
    );
    assert!(
        publish(&omitted, &definition, &catalog)
            .unwrap_err()
            .contains("omits an activity")
    );
}

// Catches: i64::MAX being accepted as a duration and overflowing an absolute
// deadline. The literal ceiling preserves the existing seven-day run contract.
#[test]
fn neutral_wait_publication_refuses_durations_beyond_existing_workflow_ceiling() {
    let (mut definition, catalog, mut graph) = fixture();
    definition.instances.clear();
    graph.nodes.insert(
        "work".into(),
        ControlNode::Wait {
            duration_ms: 604_800_000,
        },
    );
    graph.edges.insert(
        "work".into(),
        BTreeMap::from([("next".into(), "end".into())]),
    );
    publish(&graph, &definition, &catalog).unwrap();
    let mut failures = Vec::new();
    for duration_ms in [0, 604_800_001, i64::MAX as u64, u64::MAX] {
        graph
            .nodes
            .insert("work".into(), ControlNode::Wait { duration_ms });
        match publish(&graph, &definition, &catalog) {
            Err(error) if error.contains("bounded duration") => {}
            actual => failures.push(format!("duration {duration_ms}: {actual:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Catches: scoped body nodes acquiring a second graph owner, or an existing
// declared scope being silently omitted from executable control flow.
#[test]
fn neutral_control_keeps_scoped_bodies_exclusively_owned() {
    use super::fork_scope::{BranchContract, ForkContract};
    let (mut definition, catalog, _) = fixture();
    definition.instances.push(definition.instances[0].clone());
    definition.forks.insert(
        "parallel".into(),
        ForkContract {
            branches: BTreeMap::from([
                (
                    "left".into(),
                    BranchContract {
                        activity: 0,
                        required: true,
                    },
                ),
                (
                    "right".into(),
                    BranchContract {
                        activity: 1,
                        required: false,
                    },
                ),
            ]),
            max_concurrency: 2,
        },
    );
    let graph = ControlGraph {
        nodes: BTreeMap::from([
            ("start".into(), ControlNode::Start),
            (
                "fork".into(),
                ControlNode::Fork {
                    scope: "parallel".into(),
                },
            ),
            (
                "end".into(),
                ControlNode::End {
                    outcome: "done".into(),
                },
            ),
        ]),
        edges: BTreeMap::from([
            (
                "start".into(),
                BTreeMap::from([("next".into(), "fork".into())]),
            ),
            (
                "fork".into(),
                BTreeMap::from([
                    ("complete".into(), "end".into()),
                    ("blocked".into(), "end".into()),
                ]),
            ),
        ]),
    };
    publish(&graph, &definition, &catalog).unwrap();
    let mut duplicated = graph.clone();
    duplicated
        .nodes
        .insert("body".into(), ControlNode::Activity { activity: 0 });
    duplicated.edges.insert(
        "body".into(),
        BTreeMap::from([
            ("accepted".into(), "end".into()),
            ("rejected".into(), "end".into()),
        ]),
    );
    assert!(
        publish(&duplicated, &definition, &catalog)
            .unwrap_err()
            .contains("duplicate control/scope")
    );
    let (_, _, mut omitted) = fixture();
    omitted
        .nodes
        .insert("second".into(), ControlNode::Activity { activity: 1 });
    omitted.edges.insert(
        "second".into(),
        BTreeMap::from([
            ("accepted".into(), "end".into()),
            ("rejected".into(), "end".into()),
        ]),
    );
    assert!(
        publish(&omitted, &definition, &catalog)
            .unwrap_err()
            .contains("omits or duplicates a declared scope")
    );
}

// Catches: a validly rehashed persisted publication skipping control validation
// on read, even though new-save validation protects the same dangling target.
#[test]
fn neutral_control_is_revalidated_on_verified_publication_read() {
    let (mut definition, catalog, graph) = fixture();
    definition.control = graph;
    let handlers = BTreeSet::from([HandlerRef {
        name: "host.transform".into(),
        version: 1,
    }]);
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("publications.sqlite3");
    let store = super::WorkflowStore::open_at(&path).unwrap();
    let published = store
        .save_neutral_publication("/project", "control", 0, definition, &catalog, &handlers)
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "control", 1, &catalog, &handlers)
            .unwrap(),
        published
    );
    let mut record = serde_json::to_value(&published).unwrap();
    record["definition"]["control"]["edges"]["work"]["accepted"] = serde_json::json!("missing");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE neutral_workflow_publications SET document=?1,sha256=?2",
            rusqlite::params![
                record.to_string(),
                super::catalog::content_digest(&record).unwrap()
            ],
        )
        .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "control", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("target unavailable")
    );
}
