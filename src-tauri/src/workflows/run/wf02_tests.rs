//! Boundary tests use wire graphs/transitions, so they also run on the pre-WF02 revision.
use super::graph::{ActivationState, GraphExecution, GraphTransition, RUN_EVENT_CONTRACT_VERSION};
use crate::workflows::{PublishedWorkflow, WorkflowGraph, WorkflowKind, validate_executable_graph};
use serde_json::{Value, json};

fn graph_json() -> Value {
    json!({"nodes": [
        {"id":"start","kind":{"type":"start"}},
        {"id":"implement","kind":{"type":"agent","role":"implementer","capabilities":[],"prompt_template":"Implement the current artifact"}},
        {"id":"fork","kind":{"type":"fork","join":"join","repair_to":"implement","branches":[
            {"id":"critic","target":"critic","required":true},
            {"id":"checks","target":"checks","required":true}]}},
        {"id":"critic","kind":{"type":"agent","role":"reviewer","capabilities":[],"prompt_template":"Review independently"}},
        {"id":"checks","kind":{"type":"notify"}},
        {"id":"join","kind":{"type":"all_join","fork":"fork"}},
        {"id":"done","kind":{"type":"end"}},
        {"id":"failed","kind":{"type":"end"}},
        {"id":"cancelled","kind":{"type":"end"}},
        {"id":"skipped","kind":{"type":"end"}}
    ], "edges": [
        {"from":"start","to":"implement","outcome":null},
        {"from":"implement","to":"fork","outcome":null},
        {"from":"critic","to":"join","outcome":null},
        {"from":"checks","to":"join","outcome":null},
        {"from":"join","to":"done","outcome":"completed"},
        {"from":"join","to":"failed","outcome":"failed"},
        {"from":"join","to":"cancelled","outcome":"cancelled"},
        {"from":"join","to":"skipped","outcome":"skipped"}
    ]})
}

fn execution(value: Value) -> GraphExecution {
    let graph: WorkflowGraph = serde_json::from_value(value).expect("published WF02 wire graph");
    let definition: PublishedWorkflow = serde_json::from_value(json!({
        "id":"delivery", "project":"project", "name":"Delivery", "kind":"story", "closure":"human",
        "requiredChecks":[{"id":"integrity","argv":["git","fsck"],"timeoutSecs":300}], "graph": graph, "revision":1
    })).unwrap();
    GraphExecution::start("execution".into(), "story".into(), definition).unwrap()
}

fn apply(g: &mut GraphExecution, transition: Value) {
    let transition: GraphTransition = serde_json::from_value(transition).expect("WF02 transition");
    g.apply_for_contract(&transition, 20, RUN_EVENT_CONTRACT_VERSION)
        .unwrap();
}
fn activate(g: &mut GraphExecution, id: &str) {
    apply(
        g,
        json!({"type":"activate","execution_id":"execution","activation_id":id}),
    );
}
fn complete(g: &mut GraphExecution, id: &str) {
    apply(
        g,
        json!({"type":"complete","execution_id":"execution","activation_id":id,"outcome":null,"evidence":null}),
    );
}
fn visit(g: &mut GraphExecution, node: &str) -> String {
    let id = g
        .activations
        .iter()
        .rev()
        .find(|a| a.node_id == node && a.state == ActivationState::Ready)
        .expect("ready node")
        .id
        .clone();
    activate(g, &id);
    complete(g, &id);
    id
}
fn opened() -> GraphExecution {
    let mut g = execution(graph_json());
    visit(&mut g, "start");
    visit(&mut g, "implement");
    visit(&mut g, "fork");
    g
}
fn token(g: &GraphExecution, node: &str) -> (String, Value) {
    let a = g
        .activations
        .iter()
        .rev()
        .find(|a| a.node_id == node)
        .unwrap();
    let value = serde_json::to_value(a).unwrap();
    (
        a.id.clone(),
        value["tokens"].as_array().unwrap().last().unwrap().clone(),
    )
}
fn arrive(g: &mut GraphExecution, id: &str, token: Value, outcome: &str) {
    apply(
        g,
        json!({"type":"arrive","execution_id":"execution","activation_id":id,"token":token,"outcome":outcome}),
    );
}
fn supersede(g: &mut GraphExecution) -> (String, Value) {
    let (_, t) = token(g, "critic");
    let id = t["forkId"].as_str().unwrap().to_string();
    apply(
        g,
        json!({"type":"supersede_fork","execution_id":"execution","fork_id":id,"generation":t["generation"]}),
    );
    (id, t)
}

#[test]
fn critic_fix_supersedes_fork_and_restarts_both_branches() {
    // catches: repair waits on old arrivals, reuses checks, or duplicates Implement after replay.
    let mut g = opened();
    let old = token(&g, "checks");
    activate(&mut g, &old.0);
    let (fork, t) = supersede(&mut g);
    assert!(
        !g.activations
            .iter()
            .any(|a| a.node_id == "implement" && a.state == ActivationState::Ready)
    );
    // A crash/reopen at the cancellation boundary must retain the same generation.
    g = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
    arrive(&mut g, &old.0, old.1, "completed");
    let stopped = json!({"type":"fork_stopped","execution_id":"execution","fork_id":fork,"generation":t["generation"]});
    apply(&mut g, stopped.clone());
    apply(&mut g, stopped);
    assert_eq!(
        g.activations
            .iter()
            .filter(|a| a.node_id == "implement" && a.state == ActivationState::Ready)
            .count(),
        1
    );
    visit(&mut g, "implement");
    visit(&mut g, "fork");
    for node in ["critic", "checks"] {
        let (id, token) = token(&g, node);
        assert_eq!(token["generation"], 2);
        assert!(
            g.activations
                .iter()
                .any(|a| a.id == id && a.state == ActivationState::Ready)
        );
    }
    let state = serde_json::to_value(&g).unwrap();
    assert_eq!(state["forks"][0]["state"], "superseded");
    assert_eq!(state["forks"][1]["branches"][0]["arrival"], Value::Null);
    assert_eq!(state["forks"][1]["branches"][1]["arrival"], Value::Null);
}

#[test]
fn late_old_generation_cannot_complete_all_join() {
    // catches: a stale successful check discharges the replacement generation.
    let mut g = opened();
    let (old, old_token) = token(&g, "checks");
    activate(&mut g, &old);
    let (fork, t) = supersede(&mut g);
    apply(
        &mut g,
        json!({"type":"fork_stopped","execution_id":"execution","fork_id":fork,"generation":t["generation"]}),
    );
    visit(&mut g, "implement");
    visit(&mut g, "fork");
    visit(&mut g, "critic");
    arrive(&mut g, &old, old_token, "completed");
    assert!(!g.activations.iter().any(|a| a.node_id == "join"));
    visit(&mut g, "checks");
    visit(&mut g, "join");
    assert_eq!(
        g.activations.iter().filter(|a| a.node_id == "done").count(),
        1
    );
}

#[test]
fn cancelled_required_branch_blocks_join() {
    // catches: missing/cancelled required work becomes green.
    for outcome in ["cancelled", "failed", "skipped"] {
        let mut g = opened();
        let (id, token) = token(&g, "checks");
        activate(&mut g, &id);
        arrive(&mut g, &id, token, outcome);
        assert!(!g.activations.iter().any(|a| a.node_id == "join"));
        visit(&mut g, "critic");
        visit(&mut g, "join");
        assert!(
            !g.activations.iter().any(|a| a.node_id == "done"),
            "{outcome}"
        );
        assert_eq!(g.activations.last().unwrap().node_id, outcome);
    }
}

#[test]
fn duplicate_arrival_does_not_release_twice() {
    // catches: out-of-order/duplicate deliveries enqueue multiple join successors.
    let mut g = opened();
    let (checks, check_token) = token(&g, "checks");
    activate(&mut g, &checks);
    arrive(&mut g, &checks, check_token.clone(), "completed");
    arrive(&mut g, &checks, check_token.clone(), "completed");
    visit(&mut g, "critic");
    arrive(&mut g, &checks, check_token, "completed");
    assert_eq!(
        g.activations.iter().filter(|a| a.node_id == "join").count(),
        1
    );
    visit(&mut g, "join");
    assert_eq!(
        g.activations.iter().filter(|a| a.node_id == "done").count(),
        1
    );
}

#[test]
fn wf02_nested_forks_keep_exact_parent_arrivals() {
    // catches: an inner fork's first receipt is mistaken for its whole parent's receipt.
    let mut value = graph_json();
    value["nodes"][2]["kind"]["branches"][1]["target"] = json!("nested");
    value["nodes"].as_array_mut().unwrap().extend([
        json!({"id":"nested","kind":{"type":"fork","join":"nested_join","repair_to":null,"branches":[
            {"id":"a","target":"checks","required":true},{"id":"b","target":"extra","required":true}]}}),
        json!({"id":"extra","kind":{"type":"notify"}}),
        json!({"id":"nested_join","kind":{"type":"all_join","fork":"nested"}}),
    ]);
    let edges = value["edges"].as_array_mut().unwrap();
    edges.iter_mut().find(|e| e["from"] == "checks").unwrap()["to"] = json!("nested_join");
    edges.push(json!({"from":"extra","to":"nested_join","outcome":null}));
    for outcome in ["completed", "failed", "cancelled", "skipped"] {
        edges.push(json!({"from":"nested_join","to":"join","outcome":outcome}));
    }
    let mut g = execution(value);
    for node in ["start", "implement", "fork", "nested", "critic", "extra"] {
        visit(&mut g, node);
    }
    assert!(!g.activations.iter().any(|a| a.node_id == "join"));
    visit(&mut g, "checks");
    visit(&mut g, "nested_join");
    visit(&mut g, "join");
    assert_eq!(g.activations.last().unwrap().node_id, "done");
}

#[test]
fn wf02_empty_foreach_obeys_published_outcome_and_bound() {
    // catches: an empty or oversized collection silently passes.
    for (rule, expected) in [
        ("complete", "done"),
        ("skip", "skipped"),
        ("fail", "failed"),
    ] {
        let mut value = graph_json();
        value["nodes"][2]["kind"] = json!({"type":"for_each","join":"join","body":"checks","items":[],"max_items":2,"empty":rule});
        value["nodes"]
            .as_array_mut()
            .unwrap()
            .retain(|n| n["id"] != "critic");
        value["edges"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["from"] != "critic");
        let mut g = execution(value.clone());
        for node in ["start", "implement", "fork", "join"] {
            visit(&mut g, node);
        }
        assert_eq!(g.activations.last().unwrap().node_id, expected);
        value["nodes"][2]["kind"]["items"] = json!(["a", "b", "c"]);
        let graph: WorkflowGraph = serde_json::from_value(value).unwrap();
        assert!(validate_executable_graph(&graph, WorkflowKind::Story, true).is_err());
    }
}

#[test]
fn wf02_publication_refuses_crossed_missing_and_unbounded_regions() {
    // catches: publication allows stray arrivals, mismatched joins or ambiguous branches.
    let mut cases = vec![];
    let mut g = graph_json();
    g["nodes"][5]["kind"]["fork"] = json!("missing");
    cases.push(g);
    let mut g = graph_json();
    g["nodes"][2]["kind"]["branches"][1]["id"] = json!("critic");
    cases.push(g);
    let mut g = graph_json();
    g["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":"start","to":"join","outcome":null}));
    cases.push(g);
    let mut g = graph_json();
    g["edges"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["outcome"] != "cancelled");
    cases.push(g);
    let mut g = graph_json();
    g["edges"][2]["to"] = json!("done");
    cases.push(g);
    let mut g = graph_json();
    g["nodes"][2]["kind"]["repair_to"] = json!("checks");
    cases.push(g);
    for (index, value) in cases.into_iter().enumerate() {
        let graph: WorkflowGraph = serde_json::from_value(value).unwrap();
        assert!(
            validate_executable_graph(&graph, WorkflowKind::Story, true).is_err(),
            "negative case {index}"
        );
    }
}

#[test]
fn wf02_if_else_routes_unknown_without_evaluating_expressions() {
    // catches: a missing input implies yes or bypasses an explicit unknown edge.
    for (value, expected) in [
        (json!(true), "yes"),
        (json!(false), "no"),
        (Value::Null, "uncertain"),
    ] {
        let wire = json!({"nodes":[
            {"id":"start","kind":{"type":"start"}},
            {"id":"if","kind":{"type":"if_else","predicate":{"source":"input","name":"approved","value":value}}},
            {"id":"yes","kind":{"type":"end"}},{"id":"no","kind":{"type":"end"}},{"id":"uncertain","kind":{"type":"end"}}
        ],"edges":[{"from":"start","to":"if","outcome":null},{"from":"if","to":"yes","outcome":"yes"},
            {"from":"if","to":"no","outcome":"no"},{"from":"if","to":"uncertain","outcome":"uncertain"}]});
        let mut g = execution(wire);
        visit(&mut g, "start");
        visit(&mut g, "if");
        assert_eq!(g.activations.last().unwrap().node_id, expected);
    }
}
