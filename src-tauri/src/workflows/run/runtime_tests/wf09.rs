//! WF09 store/runtime boundary regressions. Wire fixtures also compile before WF09.
use super::*;
use serde_json::{Value, json};

fn publish(project: &str, graph: Value) -> crate::workflows::PublishedWorkflow {
    let store = WorkflowStore::open().unwrap();
    let reusable = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["kind"]["type"] == "return");
    let draft = store
        .create_draft(
            project,
            "Reusable decision",
            WorkflowKind::Story,
            serde_json::from_value(graph).expect("WF09 wire graph"),
        )
        .unwrap();
    let draft = if reusable {
        draft
    } else {
        store
            .update_checks(
                &draft.id,
                draft.draft_revision,
                vec![CheckDefinition {
                    id: "integrity".into(),
                    argv: vec!["git".into(), "fsck".into()],
                    timeout_secs: 30,
                }],
            )
            .unwrap()
    };
    store.publish(&draft.id, draft.draft_revision).unwrap()
}
fn returned(outcome: &str) -> Value {
    json!({"nodes":[
        {"id":"start","kind":{"type":"start"}},
        {"id":"result","kind":{"type":"return","outcome":outcome,"outputs":{"accepted":true}}}
    ],"edges":[{"from":"start","to":"result","outcome":null}]})
}
fn caller(child: &crate::workflows::PublishedWorkflow) -> Value {
    let outcomes = ["completed", "blocked", "failed", "cancelled", "uncertain"];
    let mut nodes = vec![
        json!({"id":"start","kind":{"type":"start"}}),
        json!({"id":"call","kind":{"type":"call","definition_id":child.id,"revision":child.revision,"inputs":{},"outputs":["accepted"]}}),
    ];
    let mut edges = vec![json!({"from":"start","to":"call","outcome":null})];
    for outcome in outcomes {
        nodes.push(json!({"id":outcome,"kind":{"type":"end"}}));
        edges.push(json!({"from":"call","to":outcome,"outcome":outcome}));
    }
    json!({"nodes":nodes,"edges":edges})
}

#[test]
fn new_subflow_publication_does_not_change_running_call() {
    // catches: a live call resolves the newest mutable publication instead of its start pin.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = publish(&owner, returned("completed"));
    let root = publish(&owner, caller(&child));
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let definitions = WorkflowStore::open().unwrap();
    let pinned_policy = definitions
        .import_agent_policy(0, super::routing::policy("model-a"))
        .unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "pin"))
        .unwrap();
    definitions
        .import_agent_policy(1, super::routing::policy("model-b"))
        .unwrap();
    let draft = definitions.get_draft(&child.id).unwrap();
    let draft = definitions
        .update_draft(
            &child.id,
            draft.draft_revision,
            serde_json::from_value(returned("failed")).unwrap(),
        )
        .unwrap();
    definitions
        .publish(&child.id, draft.draft_revision)
        .unwrap();
    let done = drive_turn(&store, &run.id).unwrap();
    assert_eq!(done.graph_executions.len(), 2);
    assert_eq!(done.graph_executions[1].definition, child);
    assert_eq!(
        done.graph_executions[1]
            .contract
            .as_ref()
            .unwrap()
            .agent_policy
            .as_ref(),
        Some(&pinned_policy)
    );
    assert!(
        done.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "completed")
    );
    assert_eq!(store.replay(&run.id).unwrap(), done);
}

#[test]
fn lost_call_receipt_reuses_child() {
    // catches: retry after a committed child start duplicates its nested execution.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = publish(&owner, returned("uncertain"));
    let root = publish(&owner, caller(&child));
    let path = config.path().join("runs.sqlite3");
    let store = RunStore::open_at(&path).unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "retry"))
        .unwrap();
    let activated = store
        .command(
            &run.id,
            "activate-call",
            RunCommand::Graph {
                transition: GraphTransition::Activate {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    let command: RunCommand = serde_json::from_value(json!({"action":"graph","transition":{
        "type":"call_start","execution_id":"root","activation_id":"a1"}}))
    .unwrap();
    store
        .command(&run.id, "child-start", command.clone())
        .unwrap();
    let reopened = RunStore::open_at(&path).unwrap();
    reopened.command(&run.id, "child-start", command).unwrap();
    let done = drive_turn(&reopened, &run.id).unwrap();
    assert_eq!(done.graph_executions.len(), 2);
    assert!(
        done.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "uncertain")
    );
    assert!(done.sequence > activated.sequence);
    assert_eq!(reopened.replay(&run.id).unwrap(), done);
}

#[test]
fn committed_child_return_survives_reopen_before_parent_consumption() {
    // catches: recovery respawns a completed child or loses its unconsumed typed receipt.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = publish(&owner, returned("uncertain"));
    let root = publish(&owner, caller(&child));
    let path = config.path().join("runs.sqlite3");
    let store = RunStore::open_at(&path).unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "terminal-recovery"))
        .unwrap();
    let started = store
        .command(
            &run.id,
            "activate-call",
            RunCommand::Graph {
                transition: GraphTransition::Activate {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    let started = store
        .command(
            &run.id,
            "start-child",
            RunCommand::Graph {
                transition: GraphTransition::CallStart {
                    execution_id: started.snapshot.graph_executions[0].id.clone(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    let child_id = started.snapshot.graph_executions[1].id.clone();
    for (key, transition) in [
        (
            "activate-start",
            GraphTransition::Activate {
                execution_id: child_id.clone(),
                activation_id: "a0".into(),
            },
        ),
        (
            "complete-start",
            GraphTransition::Complete {
                execution_id: child_id.clone(),
                activation_id: "a0".into(),
                outcome: None,
                evidence: None,
            },
        ),
        (
            "activate-return",
            GraphTransition::Activate {
                execution_id: child_id.clone(),
                activation_id: "a1".into(),
            },
        ),
        (
            "complete-return",
            GraphTransition::Complete {
                execution_id: child_id.clone(),
                activation_id: "a1".into(),
                outcome: None,
                evidence: None,
            },
        ),
    ] {
        store
            .command(&run.id, key, RunCommand::Graph { transition })
            .unwrap();
    }
    let committed = store.snapshot(&run.id).unwrap();
    assert_eq!(
        committed.graph_executions[0].activations[1].state,
        ActivationState::Running
    );
    assert!(committed.graph_executions[1].completed);
    let receipt = committed.graph_executions[1].call_receipt.clone().unwrap();
    drop(store);
    let reopened = RunStore::open_at(&path).unwrap();
    let done = drive_turn(&reopened, &run.id).unwrap();
    assert_eq!(done.graph_executions.len(), 2);
    assert_eq!(done.graph_executions[1].id, child_id);
    assert_eq!(
        done.graph_executions[1].call_receipt.as_ref(),
        Some(&receipt)
    );
    assert!(
        done.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "uncertain")
    );
    assert_eq!(reopened.replay(&run.id).unwrap(), done);
}

#[test]
fn spawned_child_is_not_a_completion_receipt() {
    // catches: parent completes a call before the child's typed Return has committed.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = publish(&owner, returned("blocked"));
    let root = publish(&owner, caller(&child));
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "early"))
        .unwrap();
    store
        .command(
            &run.id,
            "activate",
            RunCommand::Graph {
                transition: GraphTransition::Activate {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    assert!(
        store
            .command(
                &run.id,
                "forged",
                RunCommand::Graph {
                    transition: GraphTransition::Complete {
                        execution_id: "root".into(),
                        activation_id: "a1".into(),
                        outcome: Some(EdgeOutcome::Completed),
                        evidence: Some(DecisionEvidence {
                            actor: "daemon".into(),
                            reason: "spawned".into(),
                            references: vec!["spawn".into()]
                        })
                    }
                }
            )
            .is_err()
    );
    let done = drive_turn(&store, &run.id).unwrap();
    assert!(
        done.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "blocked")
    );
}

#[test]
fn transitive_calls_roll_up_identity_once_and_keep_unknown_usage() {
    // catches: nested pins stop at one level or inclusive child usage is counted twice.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let leaf = publish(&owner, returned("completed"));
    let mut middle_graph = caller(&leaf);
    for node in middle_graph["nodes"].as_array_mut().unwrap() {
        if node["kind"]["type"] == "end" {
            node["kind"] =
                json!({"type":"return", "outcome":node["id"], "outputs":{"accepted":true}});
        }
    }
    let middle = publish(&owner, middle_graph);
    let root = publish(&owner, caller(&middle));
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "nested"))
        .unwrap();
    let done = drive_turn(&store, &run.id).unwrap();
    assert_eq!(done.graph_executions.len(), 3);
    let wire = serde_json::to_value(&done).unwrap();
    let receipt = &wire["graphExecutions"][1]["callReceipt"];
    assert_eq!(receipt["outcome"], "completed");
    assert_eq!(receipt["outputs"]["accepted"], true);
    let expected_ids: std::collections::BTreeSet<_> = done.graph_executions[1..]
        .iter()
        .map(|execution| execution.id.clone())
        .collect();
    let actual_ids: std::collections::BTreeSet<String> =
        serde_json::from_value(receipt["accountingExecutionIds"].clone()).unwrap();
    assert_eq!(actual_ids, expected_ids);
    assert_eq!(receipt["usageCoverage"], "unknown");
    assert_eq!(store.replay(&run.id).unwrap(), done);
}

#[test]
fn cancelled_parent_requests_child_stop_without_inventing_a_return() {
    // catches: cancellation marks a live child completed or loses its stable binding.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = publish(&owner, returned("completed"));
    let root = publish(&owner, caller(&child));
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "cancel"))
        .unwrap();
    store
        .command(
            &run.id,
            "activate",
            RunCommand::Graph {
                transition: GraphTransition::Activate {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    let command: RunCommand = serde_json::from_value(json!({"action":"graph","transition":{
        "type":"call_start","execution_id":"root","activation_id":"a1"}}))
    .unwrap();
    let started = store.command(&run.id, "child", command).unwrap();
    let cancelled = store
        .command(&run.id, "cancel", RunCommand::Cancel)
        .unwrap()
        .snapshot;
    let wire = serde_json::to_value(&cancelled).unwrap();
    assert_eq!(
        wire["graphExecutions"][1]["callBinding"]["stopRequested"],
        true
    );
    assert!(wire["graphExecutions"][1].get("callReceipt").is_none());
    assert!(!cancelled.graph_executions[1].completed);
    assert_eq!(
        cancelled.graph_executions[1].id,
        started.snapshot.graph_executions[1].id
    );
    assert_eq!(drive_turn(&store, &run.id).unwrap(), cancelled);
    assert_eq!(store.replay(&run.id).unwrap(), cancelled);
}

#[test]
fn publication_rejects_delivery_nodes_in_reusable_subflows() {
    // catches: generic Call duplicates the Story Dispatch delivery lane.
    let (_config, project, _, _, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = definition(&owner, false, true);
    let store = WorkflowStore::open().unwrap();
    let draft = store
        .create_draft(
            &owner,
            "Invalid call",
            WorkflowKind::Story,
            serde_json::from_value(caller(&child)).unwrap(),
        )
        .unwrap();
    let draft = store
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "integrity".into(),
                argv: vec!["git".into(), "fsck".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    assert!(
        store
            .publish(&draft.id, draft.draft_revision)
            .unwrap_err()
            .contains("bounded reusable")
    );
}

#[test]
fn publication_rejects_deep_or_repeated_call_expansion() {
    // catches: checking only unique definition IDs admits exponential nested Calls.
    let (_config, project, _, _, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let mut child = publish(&owner, returned("completed"));
    for _ in 0..4 {
        let mut graph = caller(&child);
        for node in graph["nodes"].as_array_mut().unwrap() {
            if node["kind"]["type"] == "end" {
                node["kind"] =
                    json!({"type":"return","outcome":node["id"],"outputs":{"accepted":true}});
            }
        }
        child = publish(&owner, graph);
    }
    let store = WorkflowStore::open().unwrap();
    let draft = store
        .create_draft(
            &owner,
            "Too deep",
            WorkflowKind::Story,
            serde_json::from_value(caller(&child)).unwrap(),
        )
        .unwrap();
    let draft = store
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "integrity".into(),
                argv: vec!["git".into(), "fsck".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    assert!(
        store
            .publish(&draft.id, draft.draft_revision)
            .unwrap_err()
            .contains("depth")
    );
}

#[test]
fn declared_boolean_call_input_selects_the_actual_terminal_receipt() {
    // catches: the child ignores its pinned argument and follows an unknown/default branch.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let child = publish(
        &owner,
        json!({"nodes":[
        {"id":"start","kind":{"type":"start"}},
        {"id":"decision","kind":{"type":"if_else","predicate":{"source":"input","name":"approved","value":null}}},
        {"id":"yes","kind":{"type":"return","outcome":"completed","outputs":{"accepted":true}}},
        {"id":"no","kind":{"type":"return","outcome":"failed","outputs":{"accepted":false}}},
        {"id":"unknown","kind":{"type":"return","outcome":"uncertain","outputs":{"accepted":null}}}
    ],"edges":[{"from":"start","to":"decision","outcome":null},
        {"from":"decision","to":"yes","outcome":"yes"},
        {"from":"decision","to":"no","outcome":"no"},
        {"from":"decision","to":"unknown","outcome":"uncertain"}]}),
    );
    let mut graph = caller(&child);
    graph["nodes"][1]["kind"]["inputs"] = json!({"approved":false});
    let root = publish(&owner, graph);
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(&owner, &story, &root, "input"))
        .unwrap();
    let done = drive_turn(&store, &run.id).unwrap();
    let wire = serde_json::to_value(&done).unwrap();
    assert_eq!(
        wire["graphExecutions"][1]["callReceipt"]["outcome"],
        "failed"
    );
    assert_eq!(
        wire["graphExecutions"][1]["callReceipt"]["outputs"]["accepted"],
        false
    );
    assert!(
        done.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "failed")
    );
    assert_eq!(store.replay(&run.id).unwrap(), done);
}

#[test]
fn publication_counts_repeated_calls_not_only_unique_definitions() {
    // catches: de-duplicating definition pins also de-duplicates the expansion budget.
    let (_config, project, _, _, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let leaf = publish(&owner, returned("completed"));
    let mut nodes = vec![
        json!({"id":"start","kind":{"type":"start"}}),
        json!({"id":"end","kind":{"type":"end"}}),
    ];
    let mut edges = vec![json!({"from":"start","to":"call0","outcome":null})];
    for index in 0..33 {
        let id = format!("call{index}");
        let next = if index == 32 {
            "end".to_owned()
        } else {
            format!("call{}", index + 1)
        };
        nodes.push(json!({"id":id,"kind":{"type":"call","definition_id":leaf.id,"revision":leaf.revision,"inputs":{},"outputs":["accepted"]}}));
        for outcome in ["completed", "blocked", "failed", "cancelled", "uncertain"] {
            edges.push(json!({"from":id,"to":next,"outcome":outcome}));
        }
    }
    let store = WorkflowStore::open().unwrap();
    let draft = store
        .create_draft(
            &owner,
            "Too wide",
            WorkflowKind::Story,
            serde_json::from_value(json!({"nodes":nodes,"edges":edges})).unwrap(),
        )
        .unwrap();
    let draft = store
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "integrity".into(),
                argv: vec!["git".into(), "fsck".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    assert!(
        store
            .publish(&draft.id, draft.draft_revision)
            .unwrap_err()
            .contains("fan-out")
    );
}

#[test]
fn reusable_return_cannot_waive_checks_for_delivery_nodes() {
    // catches: adding Return lets an Agent delivery graph publish without final checks.
    let (_config, project, _, _, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let mut graph = returned("completed");
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"agent","kind":{
        "type":"agent","role":"implementer","capabilities":[],"prompt_template":"Implement"}}));
    graph["edges"][0]["to"] = json!("agent");
    graph["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":"agent","to":"result","outcome":null}));
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions
        .create_draft(
            &owner,
            "Mixed delivery",
            WorkflowKind::Story,
            serde_json::from_value(graph).unwrap(),
        )
        .unwrap();
    assert!(
        definitions
            .publish(&draft.id, draft.draft_revision)
            .unwrap_err()
            .contains("bounded reusable")
    );
}

#[test]
fn reusable_return_cannot_start_as_an_unchecked_delivery_root() {
    // catches: standalone root admission bypasses the reusable Call binding and final checks.
    let (config, project, _, story, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let reusable = publish(&owner, returned("completed"));
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    assert!(
        store
            .start_graph_run(&request(&owner, &story, &reusable, "bad-root"))
            .unwrap_err()
            .to_string()
            .contains("only through a parent Call")
    );
}

#[test]
fn reusable_call_does_not_consume_another_story_dispatch_slot() {
    // catches: nested check graphs count as extra story assignments and starve a disjoint wave.
    let (_config, project, _, _, _, _guard) = fixture();
    let owner = canonical_owner(project.path().to_str().unwrap());
    let native = StoryStore::open().unwrap();
    let plan = native
        .create_plan(crate::stories::NewPlan {
            project: owner.clone(),
            title: "Nested wave".into(),
            source: "wave.md".into(),
        })
        .unwrap();
    let mut ids = Vec::new();
    for scope in ["a.txt", "b.txt", "c.txt"] {
        ids.push(
            native
                .create_story(crate::stories::NewStory {
                    story_type: None,
                    plan_id: plan.id.clone(),
                    title: scope.into(),
                    criteria: vec!["Delivered".into()],
                    priority: 1,
                    origin: crate::stories::StoryOrigin::Native,
                    file_scope: vec![scope.into()],
                })
                .unwrap()
                .id,
        );
    }
    let leaf = publish(&owner, returned("completed"));
    let delivery = publish(&owner, caller(&leaf));
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.create_draft(&owner, "Portfolio", WorkflowKind::Plan,
        serde_json::from_value(json!({"nodes":[
            {"id":"start","kind":{"type":"start"}},
            {"id":"dispatch","kind":{"type":"story_dispatch","story_template_id":delivery.id,"story_revision":delivery.revision}},
            {"id":"done","kind":{"type":"end"}}, {"id":"blocked","kind":{"type":"end"}}
        ],"edges":[{"from":"start","to":"dispatch","outcome":null},
            {"from":"dispatch","to":"done","outcome":"completed"},
            {"from":"dispatch","to":"blocked","outcome":"blocked"}]})).unwrap()).unwrap();
    let draft = definitions
        .update_checks(&draft.id, draft.draft_revision, delivery.required_checks)
        .unwrap();
    let definition = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&GraphStartRequest {
            project: owner,
            target: RunTarget::Plan(plan.id),
            expected_revision: None,
            definition_id: definition.id,
            definition_revision: definition.revision,
            request_id: "nested-wave".into(),
            limits: RunLimits {
                max_parallel_stories: 2,
                ..RunLimits::default()
            },
        })
        .unwrap();
    store
        .command(
            &run.id,
            "activate-dispatch",
            RunCommand::Graph {
                transition: GraphTransition::Activate {
                    execution_id: "root".into(),
                    activation_id: "a1".into(),
                },
            },
        )
        .unwrap();
    store
        .command(
            &run.id,
            "dispatch-a",
            RunCommand::Graph {
                transition: GraphTransition::Start {
                    execution_id: "story-a".into(),
                    target_id: ids[0].clone(),
                },
            },
        )
        .unwrap();
    for (key, transition) in [
        (
            "start-a",
            json!({"type":"activate","execution_id":"story-a","activation_id":"a0"}),
        ),
        (
            "started-a",
            json!({"type":"complete","execution_id":"story-a","activation_id":"a0","outcome":null,"evidence":null}),
        ),
        (
            "call-a",
            json!({"type":"activate","execution_id":"story-a","activation_id":"a1"}),
        ),
        (
            "child-a",
            json!({"type":"call_start","execution_id":"story-a","activation_id":"a1"}),
        ),
    ] {
        store
            .command(
                &run.id,
                key,
                RunCommand::Graph {
                    transition: serde_json::from_value(transition).unwrap(),
                },
            )
            .unwrap();
    }
    let second = store
        .command(
            &run.id,
            "dispatch-b",
            RunCommand::Graph {
                transition: GraphTransition::Start {
                    execution_id: "story-b".into(),
                    target_id: ids[1].clone(),
                },
            },
        )
        .expect("one nested check cannot consume a second story slot")
        .snapshot;
    assert_eq!(second.graph_executions.len(), 4);
    assert!(
        store
            .command(
                &run.id,
                "dispatch-c",
                RunCommand::Graph {
                    transition: GraphTransition::Start {
                        execution_id: "story-c".into(),
                        target_id: ids[2].clone(),
                    }
                }
            )
            .unwrap_err()
            .contains("parallel story limit")
    );
    assert_eq!(store.replay(&run.id).unwrap(), second);
}

#[test]
fn call_receipt_predicates_preserve_all_five_terminal_outcomes() {
    // catches: receipt predicates collapse blocked into failed or lose known uncertain/cancelled Returns.
    let mut failures = Vec::new();
    for (actual, expected, branch) in [
        ("completed", "failed", "no"),
        ("blocked", "failed", "no"),
        ("failed", "failed", "yes"),
        ("cancelled", "failed", "no"),
        ("uncertain", "failed", "no"),
        ("uncertain", "uncertain", "yes"),
    ] {
        let (config, project, _, story, _, _guard) = fixture();
        let owner = canonical_owner(project.path().to_str().unwrap());
        let child = publish(&owner, returned(actual));
        let mut graph = caller(&child);
        graph["nodes"]
            .as_array_mut()
            .unwrap()
            .retain(|n| n["id"] == "start" || n["id"] == "call");
        graph["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"decision","kind":{"type":"if_else",
            "predicate":{"source":"receipt","node_id":"call","outcome":expected}}}));
        for edge in graph["edges"].as_array_mut().unwrap().iter_mut().skip(1) {
            edge["to"] = json!("decision");
        }
        for outcome in ["yes", "no", "uncertain"] {
            graph["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":outcome,"kind":{"type":"end"}}));
            graph["edges"]
                .as_array_mut()
                .unwrap()
                .push(json!({"from":"decision","to":outcome,"outcome":outcome}));
        }
        let root = publish(&owner, graph);
        let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
        let run = store
            .start_graph_run(&request(&owner, &story, &root, "receipt-predicate"))
            .unwrap();
        let done = drive_turn(&store, &run.id).unwrap();
        let selected = &done.graph_executions[0].activations.last().unwrap().node_id;
        if selected != branch {
            failures.push(format!(
                "Call {actual} compared to {expected}: selected {selected}, expected {branch}"
            ));
        }
        assert_eq!(store.replay(&run.id).unwrap(), done);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
