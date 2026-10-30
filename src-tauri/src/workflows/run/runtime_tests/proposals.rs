use super::*;
use crate::stories::{NewStory, StoryOrigin};

fn proposal_definition(project: &str, template: &str) -> crate::workflows::PublishedWorkflow {
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(template).unwrap();
    let mut graph = draft.graph;
    graph.nodes.retain(|n| n.id != "coordinate");
    graph.edges.retain(|e| e.from != "coordinate");
    for edge in &mut graph.edges {
        if edge.to == "coordinate" {
            edge.to = "create".into();
        }
    }
    let draft = definitions
        .update_draft(&draft.id, draft.draft_revision, graph)
        .unwrap();
    let crate::workflows::WorkflowReply::Published(definition) =
        crate::workflows::definition_action(
            project,
            crate::workflows::WorkflowAction::Publish {
                id: draft.id,
                expected_revision: draft.draft_revision,
            },
        )
        .unwrap()
    else {
        panic!("published definition")
    };
    definition
}

fn start(
    store: &RunStore,
    project: &str,
    plan: &str,
    definition: &crate::workflows::PublishedWorkflow,
) -> RunSnapshot {
    let run = store
        .start_graph_run(&GraphStartRequest {
            project: project.into(),
            target: RunTarget::Plan(plan.into()),
            expected_revision: None,
            definition_id: definition.id.clone(),
            definition_revision: definition.revision,
            request_id: "typed-proposals".into(),
            limits: RunLimits::default(),
        })
        .unwrap();
    drive_turn(store, &run.id).unwrap()
}

fn input(run: &RunSnapshot) -> StoryProposalBatch {
    let graph = &run.graph_executions[0];
    let activation = graph.activations.last().unwrap();
    assert_eq!(activation.node_id, "create");
    StoryProposalBatch {
        execution_id: graph.id.clone(),
        activation_id: activation.id.clone(),
        plan_revision: StoryStore::open()
            .unwrap()
            .get_plan(&run.plan_id)
            .unwrap()
            .revision,
        proposals: vec![NewStory {
            plan_id: run.plan_id.clone(),
            title: "Typed daemon proposal".into(),
            criteria: vec!["The daemon creates this story once".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::PlanStep {
                step: "WF04-input".into(),
            },
            file_scope: vec!["typed.txt".into()],
        }],
    }
}

// Catches: executable publication passes without Coordinator but the effect still looks up its report.
#[tokio::test]
async fn daemon_create_stories_consumes_typed_input_without_any_agent() {
    let (config, project, plan, _story, template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let published = proposal_definition(project, &template);
    let store = RunStore::open().unwrap();
    let run = start(&store, project, &plan, &published);
    assert_eq!(run.attempts, [] as [NodeAttempt; 0]);
    assert!(
        super::super::incidents::project_incidents(&run, None)
            .iter()
            .any(|i| i.source == "story_proposal_input")
    );
    let batch = input(&run);
    let request = RunCommand::SubmitStoryProposals {
        batch: batch.clone(),
    };
    let crate::workflows::RunReply::Receipt(submitted) = crate::workflows::run_action(
        project,
        RunAction::Command {
            run_id: run.id.clone(),
            command_id: "typed-input".into(),
            expected_sequence: run.sequence,
            command: Box::new(request.clone()),
        },
    )
    .unwrap() else {
        panic!("receipt")
    };
    let state = state(config.path());
    assert!(
        super::super::runtime::effects::drive_effect(&state, &store, &submitted.snapshot)
            .await
            .unwrap()
    );
    let after = store.snapshot(&run.id).unwrap();
    assert_eq!(
        StoryStore::open()
            .unwrap()
            .list_stories(&plan)
            .unwrap()
            .iter()
            .filter(|s| s.title == "Typed daemon proposal")
            .count(),
        1
    );
    assert!(super::super::proposals::complete(&after, &batch).unwrap());
    assert_eq!(
        after.attempts,
        [] as [NodeAttempt; 0],
        "no fabricated coordinator attempt"
    );
    // A lost input response and a lost effect response cannot create a second story.
    let retry = store
        .command_expected(&run.id, "typed-input", run.sequence, request)
        .unwrap();
    assert_eq!(retry.sequence, submitted.sequence);
    assert!(
        !super::super::runtime::effects::drive_effect(&state, &store, &after)
            .await
            .unwrap()
    );
    assert_eq!(store.replay(&run.id).unwrap(), after);
    let progressed = drive_turn(&store, &run.id).unwrap();
    assert!(
        progressed.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "create" && a.state == ActivationState::Completed)
    );
}

// Catches: stale input, an input for another graph visit or conflicting replacement is accepted.
#[test]
fn typed_story_proposals_reject_stale_revision_foreign_visit_and_replacement() {
    let (_config, project, plan, _story, template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let published = proposal_definition(project, &template);
    let store = RunStore::open().unwrap();
    let run = start(&store, project, &plan, &published);
    let batch = input(&run);
    let mut stale = batch.clone();
    stale.plan_revision += 1;
    assert!(
        store
            .command(
                &run.id,
                "stale-input",
                RunCommand::SubmitStoryProposals { batch: stale }
            )
            .unwrap_err()
            .contains("stale")
    );
    let mut foreign = batch.clone();
    foreign.activation_id = "foreign".into();
    assert!(
        store
            .command(
                &run.id,
                "foreign-input",
                RunCommand::SubmitStoryProposals { batch: foreign }
            )
            .is_err()
    );
    store
        .command(
            &run.id,
            "first-input",
            RunCommand::SubmitStoryProposals {
                batch: batch.clone(),
            },
        )
        .unwrap();
    assert!(
        store
            .command(
                &run.id,
                "replacement-input",
                RunCommand::SubmitStoryProposals { batch }
            )
            .unwrap_err()
            .contains("duplicate")
    );
    assert_eq!(
        store.replay(&run.id).unwrap(),
        store.snapshot(&run.id).unwrap()
    );
}

// Catches: a restart treats committed but not yet consumed input as an instruction to spawn an author.
#[tokio::test]
async fn typed_story_input_survives_restart_without_spawning_an_author() {
    let (config, project, plan, _story, template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let published = proposal_definition(project, &template);
    let store = RunStore::open().unwrap();
    let run = start(&store, project, &plan, &published);
    let batch = input(&run);
    store
        .command(
            &run.id,
            "before-restart",
            RunCommand::SubmitStoryProposals { batch },
        )
        .unwrap();
    let path = config.path().join("workflow_runs.sqlite3");
    let owner = RuntimeOwner::acquire(&path).unwrap();
    let paused = owner.store.snapshot(&run.id).unwrap();
    assert_eq!(paused.status, RunStatus::Paused);
    let graph = &paused.graph_executions[0];
    owner
        .store
        .command(
            &run.id,
            "resume-input",
            RunCommand::ResumeGraph {
                execution_id: graph.id.clone(),
                activation_id: graph.activations.last().unwrap().id.clone(),
                resolution: "Continue committed proposal input".into(),
            },
        )
        .unwrap();
    let resumed = owner.store.snapshot(&run.id).unwrap();
    assert!(
        super::super::runtime::effects::drive_effect(&state(config.path()), &owner.store, &resumed)
            .await
            .unwrap()
    );
    assert_eq!(
        owner.store.snapshot(&run.id).unwrap().attempts,
        [] as [NodeAttempt; 0]
    );
    assert_eq!(
        StoryStore::open()
            .unwrap()
            .list_stories(&plan)
            .unwrap()
            .iter()
            .filter(|s| s.title == "Typed daemon proposal")
            .count(),
        1
    );
}

// Catches: an operator-resolved effect without a native story is accepted as created work.
#[test]
fn typed_story_input_requires_the_native_creation_receipt() {
    let (_config, project, plan, _story, template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let published = proposal_definition(project, &template);
    let store = RunStore::open().unwrap();
    let run = start(&store, project, &plan, &published);
    let batch = input(&run);
    store
        .command(
            &run.id,
            "input",
            RunCommand::SubmitStoryProposals {
                batch: batch.clone(),
            },
        )
        .unwrap();
    let key = super::super::proposals::proposal_key(&batch, 0);
    let intended = store
        .command(
            &run.id,
            "unproven-intent",
            RunCommand::ReserveEffect {
                key: format!("create-story:{key}"),
                kind: EffectKind::CreateStory,
            },
        )
        .unwrap();
    let effect = intended.snapshot.effects.last().unwrap();
    store
        .command(
            &run.id,
            "unproven-success",
            RunCommand::MarkEffect {
                effect_id: effect.id.clone(),
                succeeded: true,
            },
        )
        .unwrap();
    assert!(
        drive_turn(&store, &run.id)
            .unwrap_err()
            .contains("no native story receipt")
    );
}

// Catches: input accepted for one plan revision is consumed after the native source changes.
#[tokio::test]
async fn typed_story_input_does_not_create_after_plan_revision_changes() {
    let (config, project, plan, _story, template, _guard) = fixture();
    let project = project.path().to_str().unwrap();
    let published = proposal_definition(project, &template);
    let store = RunStore::open().unwrap();
    let run = start(&store, project, &plan, &published);
    let batch = input(&run);
    store
        .command(
            &run.id,
            "input",
            RunCommand::SubmitStoryProposals {
                batch: batch.clone(),
            },
        )
        .unwrap();
    StoryStore::open()
        .unwrap()
        .update_plan(
            &plan,
            batch.plan_revision,
            crate::stories::PlanPatch {
                title: Some("Plan changed after input".into()),
                source: None,
            },
        )
        .unwrap();
    let current = store.snapshot(&run.id).unwrap();
    assert!(
        super::super::runtime::effects::drive_effect(&state(config.path()), &store, &current)
            .await
            .unwrap_err()
            .contains("stale plan revision")
    );
    assert_eq!(
        StoryStore::open()
            .unwrap()
            .list_stories(&plan)
            .unwrap()
            .iter()
            .filter(|s| s.title == "Typed daemon proposal")
            .count(),
        0
    );
}
