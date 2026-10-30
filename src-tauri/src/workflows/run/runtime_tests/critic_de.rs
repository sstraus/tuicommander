use super::*;

#[tokio::test]
async fn resumed_gate_checks_the_corrected_artifact_instead_of_reusing_old_command_key() {
    // catches: a resumed Gate rejecting checks for a newly committed fix as a reused payload.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let worktree = init_worktree(project.path(), config.path());
    let published = publish(
        project_path,
        WorkflowGraph {
            nodes: vec![
                Node {
                    id: "start".into(),
                    kind: NodeKind::Start,
                },
                Node {
                    id: "implement".into(),
                    kind: NodeKind::Agent {
                        execution: None,
                        role: AgentRole::Implementer,
                        capabilities: vec!["story_read".into(), "story_report".into()],
                        prompt_template: "Implement the criteria".into(),
                    },
                },
                Node {
                    id: "gate".into(),
                    kind: NodeKind::Gate,
                },
                Node {
                    id: "pause".into(),
                    kind: NodeKind::Pause {
                        resume_to: Some("gate".into()),
                    },
                },
                Node {
                    id: "end".into(),
                    kind: NodeKind::End,
                },
            ],
            edges: vec![
                edge("start", "implement", None),
                edge("implement", "gate", None),
                edge("gate", "end", Some("pass")),
                edge("gate", "pause", Some("fail")),
            ],
        },
    );
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&request(
            project_path,
            &story,
            &published,
            "critic-gate-refresh",
        ))
        .unwrap();
    let prepared = drive_turn(&store, &run.id).unwrap();
    let implementer = prepared.attempts.last().unwrap().clone();
    store
        .command(
            &run.id,
            "critic-artifact",
            RunCommand::AssignWorktree {
                story_id: story.clone(),
                path: worktree.clone(),
            },
        )
        .unwrap();
    bind(&store, &run.id, &implementer, "critic-implementer");
    report(&store, &run.id, &implementer, "critic-implementer", None);
    drive_turn(&store, &run.id).unwrap();
    let app = state(config.path());
    let before_check = store.snapshot(&run.id).unwrap();
    assert!(drive_effect(&app, &store, &before_check).await.unwrap());
    store
        .command(&run.id, "critic-pause", RunCommand::Pause)
        .unwrap();
    std::fs::write(
        std::path::Path::new(&worktree).join("README.md"),
        "corrected artifact\n",
    )
    .unwrap();
    for args in [
        vec!["add", "README.md"],
        vec!["commit", "-qm", "Correct artifact"],
    ] {
        crate::git_cli::git_cmd(std::path::Path::new(&worktree))
            .args(args)
            .run()
            .unwrap();
    }
    let paused = store.snapshot(&run.id).unwrap();
    let gate = paused.graph_executions[0].activations.last().unwrap();
    store
        .command(
            &run.id,
            "critic-resume",
            RunCommand::ResumeGraph {
                execution_id: paused.graph_executions[0].id.clone(),
                activation_id: gate.id.clone(),
                resolution: "Operator committed corrected artifact".into(),
            },
        )
        .unwrap();
    let resumed = store.snapshot(&run.id).unwrap();
    assert!(
        drive_effect(&app, &store, &resumed)
            .await
            .expect("resumed Gate must check the corrected artifact")
    );
    let after = store.snapshot(&run.id).unwrap();
    let receipts = &after
        .stories
        .iter()
        .find(|s| s.story_id == story)
        .unwrap()
        .check_receipts;
    assert_eq!(
        receipts.len(),
        2,
        "corrected artifact needs a fresh persisted check receipt"
    );
    assert_ne!(receipts[0].commit, receipts[1].commit);
    assert!(drive_turn(&store, &run.id).unwrap().graph_executions[0].completed);
}
