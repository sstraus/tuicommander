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
