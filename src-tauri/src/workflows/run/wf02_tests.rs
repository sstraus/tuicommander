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

#[test]
fn wf02_sqlite_reopen_replays_waits_cancellation_and_single_repair() {
    // catches: projection-only waits or cancellation vanish on reopen and release duplicate work.
    use super::{RunCommand, RunLimits, RunStore};
    use crate::workflows::WorkflowStore;
    let (config, project, plan, story, plan_definition, _guard) = super::tests::fixture();
    assert!(
        config
            .path()
            .starts_with(crate::test_support::test_temp_root())
    );
    let definitions = WorkflowStore::open().unwrap();
    let owner = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut wire = graph_json();
    wire["nodes"][4]["kind"] = json!({"type":"agent","role":"validator","capabilities":[],"prompt_template":"Run the declared checks"});
    wire["nodes"][2]["kind"]["branches"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"advisor","target":"advisor","required":false}));
    wire["nodes"].as_array_mut().unwrap().push(json!({"id":"advisor","kind":{"type":"agent","role":"reviewer","capabilities":[],"prompt_template":"Advise independently"}}));
    wire["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":"advisor","to":"join","outcome":null}));
    let draft = definitions
        .create_draft(
            &owner,
            "WF02 replay",
            WorkflowKind::Story,
            serde_json::from_value(wire).unwrap(),
        )
        .unwrap();
    let draft = definitions
        .update_checks(
            &draft.id,
            draft.draft_revision,
            execution(graph_json()).definition.required_checks,
        )
        .unwrap();
    let publication = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    let plan_draft = definitions.get_draft(&plan_definition).unwrap();
    let mut graph = serde_json::to_value(&plan_draft.graph).unwrap();
    for node in graph["nodes"].as_array_mut().unwrap() {
        if node["kind"]["type"] == "story_dispatch" {
            node["kind"]["story_template_id"] = json!(publication.id);
            node["kind"]["story_revision"] = json!(publication.revision);
        }
    }
    let plan_draft = definitions
        .update_draft(
            &plan_definition,
            plan_draft.draft_revision,
            serde_json::from_value(graph).unwrap(),
        )
        .unwrap();
    let publication = definitions
        .publish(&plan_draft.id, plan_draft.draft_revision)
        .unwrap();
    let db = config.path().join("wf02.sqlite3");
    let store = RunStore::open_at(&db).unwrap();
    let run = store
        .start_plan(
            &owner,
            &plan,
            &plan_definition,
            publication.revision,
            RunLimits::default(),
        )
        .unwrap();
    let commit = |wire: Value| {
        let store = RunStore::open_at(&db).unwrap();
        let snapshot = store.snapshot(&run.id).unwrap();
        let transition = serde_json::from_value(wire).unwrap();
        let result = store
            .command_expected(
                &run.id,
                &format!("wf02:{}", snapshot.sequence),
                snapshot.sequence,
                RunCommand::Graph { transition },
            )
            .unwrap()
            .snapshot;
        let reopened = RunStore::open_at(&db).unwrap();
        assert_eq!(reopened.replay(&run.id).unwrap(), result);
        assert_eq!(reopened.snapshot(&run.id).unwrap(), result);
        result
    };
    commit(json!({"type":"start","execution_id":"execution","target_id":story}));
    for id in ["a0", "a1", "a2"] {
        commit(json!({"type":"activate","execution_id":"execution","activation_id":id}));
        commit(
            json!({"type":"complete","execution_id":"execution","activation_id":id,"outcome":null,"evidence":null}),
        );
    }
    let admitted = super::runtime::drive_turn(&store, &run.id).unwrap();
    assert_eq!(
        admitted
            .attempts
            .iter()
            .map(|a| a.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["critic", "checks"]
    );
    assert!(
        admitted
            .attempts
            .iter()
            .all(|a| a.state == super::AttemptState::Running)
    );
    let waiting = RunStore::open_at(&db).unwrap().snapshot(&run.id).unwrap();
    assert_eq!(
        serde_json::to_value(&waiting.graph_executions[0]).unwrap()["activations"][5]["resourceWait"],
        "Parallel activity capacity is occupied"
    );
    assert_eq!(
        RunStore::open_at(&db).unwrap().replay(&run.id).unwrap(),
        waiting
    );
    // catches: superseded activation timers remaining pending forever and blocking completion,
    // or retiring an independent service timer together with the old branch generation.
    let branch = waiting.graph_executions[0]
        .activations
        .iter()
        .find(|a| a.node_id == "critic")
        .unwrap();
    for (key, scope) in [
        (
            "old-branch",
            json!({"type":"activation","execution_id":"execution","activation_id":branch.id}),
        ),
        (
            "independent",
            json!({"type":"service","owner":"proof-cohort"}),
        ),
    ] {
        let command = serde_json::from_value(
            json!({"action":"timer","command":{"action":"schedule","timer":{
                "key":key,"scope":scope,"purpose":{"type":"provider_retry","provider":"github"},
                "dueMs":waiting.started_ms + 100000,"expiresMs":waiting.started_ms + 200000,
                "maxAttempts":3,"baseBackoffMs":1000,"maxBackoffMs":5000,
                "expiryOutcome":"incomplete","attempt":1,"state":"pending","outcome":null
            }}}),
        )
        .unwrap();
        store
            .command(&run.id, &format!("schedule:{key}"), command)
            .unwrap();
    }
    let superseded = commit(
        json!({"type":"supersede_fork","execution_id":"execution","fork_id":"a2","generation":1}),
    );
    let timers = serde_json::to_value(&superseded).unwrap()["timers"].clone();
    assert_eq!(timers[0]["state"], "cancelled");
    assert_eq!(timers[1]["state"], "pending");
    let late_timer = serde_json::from_value(
        json!({"action":"timer","command":{"action":"fire","key":"old-branch","attempt":1}}),
    )
    .unwrap();
    assert!(
        store
            .command_at(
                &run.id,
                "late-old-timer",
                late_timer,
                waiting.started_ms + 100000
            )
            .is_err()
    );
    assert!(
        superseded
            .attempts
            .iter()
            .all(|a| a.state == super::AttemptState::Interrupted)
    );
    let old = &superseded.attempts[1];
    let late = store
        .command(
            &run.id,
            "late-old-check",
            RunCommand::ReportAttempt {
                attempt_id: old.id.clone(),
                generation: old.generation,
                outcome: super::AttemptOutcome::Completed,
            },
        )
        .unwrap();
    assert!(matches!(
        late.event.kind,
        super::RunEventKind::LateReportIgnored { .. }
    ));
    let stopped =
        json!({"type":"fork_stopped","execution_id":"execution","fork_id":"a2","generation":1});
    commit(stopped.clone());
    let replayed = commit(stopped);
    assert_eq!(
        replayed.graph_executions[0]
            .activations
            .iter()
            .filter(|a| a.node_id == "implement" && a.state == ActivationState::Ready)
            .count(),
        1
    );
    // A capacity-waiting advisor had no attempt in generation one. It must not
    // shift or steal the binding of any generation-two activity.
    for id in ["a6", "a7"] {
        commit(json!({"type":"activate","execution_id":"execution","activation_id":id}));
        commit(
            json!({"type":"complete","execution_id":"execution","activation_id":id,"outcome":null,"evidence":null}),
        );
    }
    let replacement = super::runtime::drive_turn(&store, &run.id).unwrap();
    let checks = replacement
        .attempts
        .iter()
        .rev()
        .find(|a| a.node_id == "checks")
        .unwrap();
    assert_eq!(
        replacement
            .attempts
            .iter()
            .filter(|a| a.state == super::AttemptState::Running)
            .count(),
        2
    );
    store
        .command(
            &run.id,
            "replacement-check-failed",
            RunCommand::ReportAttempt {
                attempt_id: checks.id.clone(),
                generation: checks.generation,
                outcome: super::AttemptOutcome::Failed,
            },
        )
        .unwrap();
    let admitted = super::runtime::drive_turn(&store, &run.id).unwrap();
    assert!(
        admitted
            .attempts
            .iter()
            .any(|a| a.node_id == "advisor" && a.state == super::AttemptState::Running)
    );
    assert_eq!(
        RunStore::open_at(&db).unwrap().replay(&run.id).unwrap(),
        admitted
    );
}

#[test]
fn wf02_cancelling_one_check_generation_fences_late_starts_but_not_repair() {
    // catches: cancellation either misses late old starts or permanently disables replacement checks.
    use super::check::{cancel_checks, execute_run_check};
    use crate::workflows::CheckDefinition;
    let repo = tempfile::tempdir().unwrap();
    assert!(
        repo.path()
            .starts_with(crate::test_support::test_temp_root())
    );
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Workflow"],
        vec!["config", "user.email", "workflow@example.test"],
        vec!["commit", "--allow-empty", "-qm", "initial"],
    ] {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let db = repo.path().join("owner.sqlite3");
    let check = CheckDefinition {
        id: "status".into(),
        argv: vec!["git".into(), "status".into(), "--porcelain".into()],
        timeout_secs: 30,
    };
    cancel_checks(&db, "run:execution:fork:1");
    assert!(
        execute_run_check(&check, repo.path(), &db, "run:execution:fork:1", false)
            .unwrap_err()
            .contains("cancelled")
    );
    assert_eq!(
        execute_run_check(&check, repo.path(), &db, "run:execution:fork:2", false)
            .unwrap()
            .exit_code,
        0
    );
    cancel_checks(&db, "run");
    assert!(
        execute_run_check(&check, repo.path(), &db, "run:execution:fork:2", false)
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(
        execute_run_check(&check, repo.path(), &db, "run:execution:fork:3", false)
            .unwrap_err()
            .contains("cancelled")
    );
}

#[test]
fn wf02_dynamic_foreach_expands_exact_receipt_items_with_distinct_tokens() {
    // catches: duplicate collection values share one obligation or overflow ignores the pinned bound.
    let mut wire = graph_json();
    wire["nodes"][2]["kind"] = json!({"type":"for_each","join":"join","body":"checks","items":[],"receipt_evidence":"implement","max_items":2,"empty":"fail"});
    wire["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["id"] != "critic");
    wire["edges"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["from"] != "critic");
    let mut g = execution(wire);
    visit(&mut g, "start");
    let source = visit(&mut g, "implement");
    let id = g.activations.last().unwrap().id.clone();
    activate(&mut g, &id);
    let overflow = json!({"type":"expand","execution_id":"execution","activation_id":id,"source_activation_id":source,"items":["a","a","b"]});
    assert!(
        g.clone()
            .apply_for_contract(
                &serde_json::from_value(overflow).unwrap(),
                20,
                RUN_EVENT_CONTRACT_VERSION
            )
            .is_err()
    );
    apply(
        &mut g,
        json!({"type":"expand","execution_id":"execution","activation_id":id,"source_activation_id":source,"items":["a","a"]}),
    );
    let state = serde_json::to_value(&g).unwrap();
    assert_eq!(state["forks"][0]["branches"][0]["item"], "a");
    assert_eq!(state["forks"][0]["branches"][1]["item"], "a");
    assert_ne!(
        state["forks"][0]["branches"][0]["id"],
        state["forks"][0]["branches"][1]["id"]
    );
    let ids: Vec<_> = g
        .activations
        .iter()
        .filter(|a| a.node_id == "checks")
        .map(|a| a.id.clone())
        .collect();
    for id in ids {
        activate(&mut g, &id);
        complete(&mut g, &id);
    }
    visit(&mut g, "join");
    assert_eq!(g.activations.last().unwrap().node_id, "done");
}

#[test]
fn wf02_receipt_predicate_reads_all_join_terminal_outcome() {
    // catches F3: a tokenless failed/cancelled/skipped all-Join is fabricated Completed.
    for (outcome, empty) in [
        ("completed", false),
        ("failed", false),
        ("cancelled", false),
        ("skipped", false),
        ("completed", true),
        ("failed", true),
        ("skipped", true),
    ] {
        let mut wire = graph_json();
        if empty {
            let rule = match outcome {
                "completed" => "complete",
                "failed" => "fail",
                _ => "skip",
            };
            wire["nodes"][2]["kind"] = json!({"type":"for_each","join":"join","body":"checks","items":[],"max_items":2,"empty":rule});
            wire["nodes"]
                .as_array_mut()
                .unwrap()
                .retain(|n| n["id"] != "critic");
            wire["edges"]
                .as_array_mut()
                .unwrap()
                .retain(|e| e["from"] != "critic");
        }
        wire["nodes"].as_array_mut().unwrap().retain(|node| {
            !["done", "failed", "cancelled", "skipped"]
                .iter()
                .any(|id| node["id"] == *id)
        });
        wire["nodes"].as_array_mut().unwrap().extend([
            json!({"id":"predicate","kind":{"type":"if_else","predicate":{"source":"receipt","node_id":"join","outcome":outcome}}}),
            json!({"id":"yes","kind":{"type":"end"}}),
            json!({"id":"no","kind":{"type":"end"}}),
            json!({"id":"unknown","kind":{"type":"end"}}),
        ]);
        for edge in wire["edges"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .filter(|e| e["from"] == "join")
        {
            edge["to"] = json!("predicate");
        }
        wire["edges"].as_array_mut().unwrap().extend([
            json!({"from":"predicate","to":"yes","outcome":"yes"}),
            json!({"from":"predicate","to":"no","outcome":"no"}),
            json!({"from":"predicate","to":"unknown","outcome":"uncertain"}),
        ]);
        let mut g = execution(wire);
        for node in ["start", "implement", "fork"] {
            visit(&mut g, node);
        }
        if !empty {
            visit(&mut g, "critic");
            let (id, t) = token(&g, "checks");
            activate(&mut g, &id);
            arrive(&mut g, &id, t, outcome);
        }
        visit(&mut g, "join");
        visit(&mut g, "predicate");
        assert_eq!(g.activations.last().unwrap().node_id, "yes", "{outcome}");
    }
}

#[test]
fn wf02_receipt_predicate_requires_typed_decision_evidence() {
    // catches: a failed Gate or an untyped completed node is fabricated successful evidence.
    for (gate, source, wanted, expected) in [
        ("pass", "gate", "completed", "yes"),
        ("fail", "gate", "completed", "no"),
        ("fail", "gate", "failed", "yes"),
        ("pass", "start", "completed", "unknown"),
    ] {
        let mut g = execution(json!({"nodes":[
            {"id":"start","kind":{"type":"start"}},
            {"id":"gate","kind":{"type":"gate"}},
            {"id":"predicate","kind":{"type":"if_else","predicate":{"source":"receipt","node_id":source,"outcome":wanted}}},
            {"id":"yes","kind":{"type":"end"}},
            {"id":"no","kind":{"type":"end"}},
            {"id":"unknown","kind":{"type":"end"}}
        ],"edges":[
            {"from":"start","to":"gate","outcome":null},
            {"from":"gate","to":"predicate","outcome":"pass"},
            {"from":"gate","to":"predicate","outcome":"fail"},
            {"from":"predicate","to":"yes","outcome":"yes"},
            {"from":"predicate","to":"no","outcome":"no"},
            {"from":"predicate","to":"unknown","outcome":"uncertain"}
        ]}));
        visit(&mut g, "start");
        let id = g.activations.last().unwrap().id.clone();
        activate(&mut g, &id);
        apply(
            &mut g,
            json!({"type":"complete","execution_id":"execution","activation_id":id,"outcome":gate,"evidence":{
                "actor":"daemon","reason":"Current deterministic check result","references":["check:integrity"]
            }}),
        );
        visit(&mut g, "predicate");
        assert_eq!(
            g.activations.last().unwrap().node_id,
            expected,
            "{gate}/{source}/{wanted}"
        );
    }
}
