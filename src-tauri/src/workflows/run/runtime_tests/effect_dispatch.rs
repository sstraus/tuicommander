use super::*;

#[tokio::test]
async fn retained_notify_intents_never_replay_delivery() {
    // Catches: extraction treating a retained failed/intended notification as fresh work,
    // or reporting durable progress again for an already successful delivery.
    for succeeded in [None, Some(false), Some(true)] {
        let (config, project, _plan, story, _template, _guard) = fixture();
        let published = semantics::publish(
            project.path().to_str().unwrap(),
            WorkflowGraph {
                evidence_policy: None,
                nodes: vec![
                    Node {
                        id: "start".into(),
                        kind: NodeKind::Start,
                    },
                    Node {
                        id: "notice".into(),
                        kind: NodeKind::Notify,
                    },
                    Node {
                        id: "end".into(),
                        kind: NodeKind::End,
                    },
                ],
                edges: vec![
                    Edge {
                        from: "start".into(),
                        to: "notice".into(),
                        outcome: None,
                    },
                    Edge {
                        from: "notice".into(),
                        to: "end".into(),
                        outcome: None,
                    },
                ],
            },
        );
        let store = RunStore::open_at(&config.path().join("effects.sqlite3")).unwrap();
        let run = store
            .start_graph_run(&request(
                project.path().to_str().unwrap(),
                &story,
                &published,
                "retained-notify",
            ))
            .unwrap();
        let run = drive_turn(&store, &run.id).unwrap();
        let graph = &run.graph_executions[0];
        let activation = graph
            .activations
            .iter()
            .find(|a| a.node_id == "notice")
            .unwrap();
        let reserved = store
            .command(
                &run.id,
                "retained-intent",
                RunCommand::ReserveEffect {
                    key: format!("notify:{}:{}", graph.id, activation.id),
                    kind: EffectKind::Notify,
                },
            )
            .unwrap();
        if let Some(succeeded) = succeeded {
            store
                .command(
                    &run.id,
                    "retained-receipt",
                    RunCommand::MarkEffect {
                        effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                        succeeded,
                    },
                )
                .unwrap();
        }
        let before = store.snapshot(&run.id).unwrap();
        let result = runtime::effects::drive_effect(&state(config.path()), &store, &before).await;
        if succeeded == Some(true) {
            assert!(!result.unwrap());
        } else {
            assert_eq!(
                result.unwrap_err(),
                "Notify intent needs explicit reconciliation; delivery is not replayed"
            );
        }
        assert_eq!(store.snapshot(&run.id).unwrap(), before);
        assert_eq!(store.replay(&run.id).unwrap(), before);
    }
}

#[tokio::test]
async fn retained_workspace_intent_fences_command_preparation() {
    // Catches: workspace extraction reserving or creating a second worktree after a crash.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let graph: WorkflowGraph = serde_json::from_value(serde_json::json!({
        "nodes": [
            {"id":"start", "kind":{"type":"start"}},
            {"id":"command", "kind":{"type":"run_command", "command": {
                "version":1, "invocation":{"type":"argv", "argv":["git", "status"]},
                "workspace":"assigned", "target":{"type":"local", "os":std::env::consts::OS, "arch":std::env::consts::ARCH},
                "resources":[], "secretRefs":{}, "stdinArtifact":null, "timeoutSecs":15, "output":{"type":"exit_status"}
            }}},
            {"id":"end", "kind":{"type":"end"}}
        ],
        "edges": [
            {"from":"start", "to":"command", "outcome":null},
            {"from":"command", "to":"end", "outcome":"pass"},
            {"from":"command", "to":"end", "outcome":"fail"}
        ]
    })).unwrap();
    let published = semantics::publish(project.path().to_str().unwrap(), graph);
    let store = RunStore::open_at(&config.path().join("workspace-effects.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story,
            &published,
            "retained-workspace",
        ))
        .unwrap();
    let run = drive_turn(&store, &run.id).unwrap();
    let key = format!(
        "command-workspace:{}:{}",
        run.id, run.graph_executions[0].id
    );
    let reserved = store
        .command(
            &run.id,
            "prior-workspace-intent",
            RunCommand::ReserveEffect {
                key: key.clone(),
                kind: EffectKind::PrepareWorkspace,
            },
        )
        .unwrap();
    let error = runtime::effects::drive_effect(&state(config.path()), &store, &reserved.snapshot)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        "workspace preparation already intended; reconcile before retry"
    );
    let after = store.snapshot(&run.id).unwrap();
    assert_eq!(
        after
            .effects
            .iter()
            .filter(|effect| effect.kind == EffectKind::PrepareWorkspace)
            .count(),
        1
    );
    assert_eq!(
        after.effects.iter().find(|effect| effect.key == key),
        reserved
            .snapshot
            .effects
            .iter()
            .find(|effect| effect.key == key)
    );
    assert!(
        after
            .stories
            .iter()
            .all(|story| story.worktree_path.is_none())
    );
    assert_eq!(store.replay(&run.id).unwrap(), after);
}
