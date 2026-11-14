use super::*;

// Catches: after a reducer Failure pops a child, rejecting its owned effect's
// stop evidence solely because the graph frame closed. This is latent defense:
// no valid published workflow triggering that Failure with a live sibling is proven.
#[tokio::test]
async fn stopped_orphan_observation_settles_without_graph_reopen() {
    let (dir, owner, run, at) =
        super::super::neutral_runtime_tests::subflow_fixture_with_ancestor(true, 1, true, false);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Pending, false);
    owner.neutral_host(host);
    let (root, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    assert!(
        owner
            .dispatch_neutral(&root, &Arc::new(Notify::new()))
            .unwrap()
    );
    let (child, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    assert_ne!(child.execution, root.execution);
    let live = dispatch(&owner, &run.run.id).await;
    let key = only_key(&live);
    assert_eq!(claims(&owner), 1);
    let mut orphan = live;
    // This is the actual reducer decision generated for Step errors. The
    // transition, rather than a corrupt publication, supplies the closed frame.
    orphan
        .apply(&super::super::neutral_execution::NeutralDecision::Failure {
            diagnostic: "child reducer failure".into(),
        })
        .unwrap();
    assert_eq!(orphan.status, NeutralStatus::Running);
    assert_eq!(orphan.execution, root.execution);
    let graph = orphan.clone();
    let decision = orphan
        .decide_effect(
            &EffectCommand::Observe {
                key: key.clone(),
                observation: FileHost::result(orphan.effect(&key).unwrap()),
            },
            at,
        )
        .unwrap()
        .unwrap();
    orphan.apply_effect(&decision).unwrap();
    super::super::neutral_effects::project_resources(
        &owner.store.connect().unwrap(),
        &orphan,
        &decision,
    )
    .unwrap();
    assert!(orphan.effect(&key).unwrap().settled());
    assert_eq!(claims(&owner), 0);
    assert_eq!(orphan.execution, graph.execution);
    assert_eq!(orphan.node, graph.node);
    assert_eq!(orphan.status, graph.status);
    assert_eq!(orphan.active_scope, graph.active_scope);
}
