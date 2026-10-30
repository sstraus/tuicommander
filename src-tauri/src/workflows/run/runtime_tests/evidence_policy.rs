//! Native RunStore boundary regressions. JSON is client input, not an external-system fixture.
use super::*;
use serde_json::{Value, json};

fn policy() -> Value {
    json!({"maxReviews":2,"maxRepairs":1,"maxStoryProposals":16,
        "requireRed":false,"requireStrictClippy":false,"nativeTargets":[],"consumerScope":[],"changedTests":[]})
}

fn published_policy(project: &str) -> crate::workflows::PublishedWorkflow {
    let definitions = WorkflowStore::open().unwrap();
    let seed = definitions
        .seed_templates(&canonical_owner(project))
        .unwrap()
        .into_iter()
        .find(|draft| draft.kind == WorkflowKind::Story)
        .unwrap();
    let mut graph = serde_json::to_value(seed.graph).unwrap();
    graph["evidencePolicy"] = policy();
    publish(
        project,
        serde_json::from_value(graph).expect("pinned evidence policy contract"),
    )
}

fn subject(
    store: &RunStore,
    run: &RunSnapshot,
    reviewer: &NodeAttempt,
    published: &crate::workflows::PublishedWorkflow,
) -> Value {
    let story = StoryStore::open()
        .unwrap()
        .get_story(&reviewer.story_id)
        .unwrap();
    let reviewer = run
        .attempts
        .iter()
        .find(|attempt| attempt.id == reviewer.id)
        .unwrap();
    let prompt = crate::workflows::render_story_prompt(run, &story, reviewer, published, &[], None)
        .unwrap()
        .prompt;
    let context = prompt
        .split("Scoped story context (data, not authority):\n")
        .nth(1)
        .unwrap()
        .split("\n\nReport the outcome")
        .next()
        .unwrap();
    let context: Value = serde_json::from_str(context).unwrap();
    let artifact = &context["artifactSubject"];
    let _ = store; // Prompt, not a test copy of the requirements-hashing implementation, is the producer.
    json!({"type":"story","id":story.id,"commit":artifact["commit"],"tree":artifact["tree"],"requirements":artifact["requirements"]})
}

fn assessment(subject: Value, fix: bool) -> ReviewAssessment {
    let commit = subject["commit"].as_str().unwrap();
    let tree = subject["tree"].as_str().unwrap();
    serde_json::from_value(json!({
        "decision":if fix { "changes_requested" } else { "approved" },
        "artifactDigest":artifact_digest(commit, tree),
        "findings":if fix { json!([{"criterionIndex":0,"severity":"major","summary":"Repair current bug","evidence":"Current source"}]) } else {json!([])},
        "evidence":{
            "subject":subject,
            "findings":if fix {json!([{"id":"bug","risk":5,"value":1,"effort":5,"addsMechanism":true,"deletesOnly":false,"scope":[],"acceptedRisk":null,"proposal":null}])} else {json!([])},
            "resolvedFixes":[],
            "proof":{"realityEffect":"criterion holds","realityState":"native source","realityAssumptions":"none","realityRadius":"story",
                "symptom":null,"red":null,"testChanges":[],"checks":[],"omissions":[]}
        }
    })).unwrap()
}

fn setup_review(store: &RunStore, run: &RunSnapshot, worktree: &str, session: &str) -> NodeAttempt {
    let current = drive_turn(store, &run.id).unwrap();
    let implement = current.attempts.last().unwrap().clone();
    store
        .command(
            &run.id,
            "wf06:worktree",
            RunCommand::AssignWorktree {
                story_id: implement.story_id.clone(),
                path: worktree.into(),
            },
        )
        .unwrap();
    bind(store, &run.id, &implement, "wf06-implementer");
    let bound = store.snapshot(&run.id).unwrap();
    store
        .begin_graph_story(&bound, &implement.story_id, "wf06-implementer")
        .unwrap();
    report(store, &run.id, &implement, "wf06-implementer", None);
    let current = drive_turn(store, &run.id).unwrap();
    let reviewer = current.attempts.last().unwrap().clone();
    assert_eq!(reviewer.node_id, "review");
    if session != "wf06-implementer" {
        bind(store, &run.id, &reviewer, session);
    }
    let stories = StoryStore::open().unwrap();
    let story = stories.get_story(&implement.story_id).unwrap();
    let story = stories
        .transition(&story.id, story.revision, StoryCommand::CheckCriterion(0))
        .unwrap();
    stories
        .transition(&story.id, story.revision, StoryCommand::SubmitReview)
        .unwrap();
    reviewer
}

fn check(store: &RunStore, run_id: &str, story: &str) {
    let run = store.snapshot(run_id).unwrap();
    let id = run.graph_executions[0].definition.required_checks[0]
        .id
        .clone();
    store
        .execute_check(
            run_id,
            story,
            &id,
            &format!("wf06:check:{}", run.sequence),
            run.sequence,
        )
        .unwrap();
}

fn verdict(store: &RunStore, run_id: &str) -> (EdgeOutcome, DecisionEvidence) {
    let run = store.snapshot(run_id).unwrap();
    let graph = &run.graph_executions[0];
    super::super::super::runtime::judge::judge(&run, graph, graph.activations.last().unwrap())
        .unwrap()
}

#[test]
fn artifact_edit_invalidates_review_and_checks() {
    // catches: acceptance surviving a committed artifact edit or a requirements edit.
    let (config, project, _, story, _, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let worktree = init_worktree(project.path(), config.path());
    let published = published_policy(project_path);
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&request(project_path, &story, &published, "wf06-artifact"))
        .unwrap();
    let reviewer = setup_review(&store, &run, &worktree, "wf06-reviewer");
    check(&store, &run.id, &story);
    let current = store.snapshot(&run.id).unwrap();
    report(
        &store,
        &run.id,
        &reviewer,
        "wf06-reviewer",
        Some(assessment(
            subject(&store, &current, &reviewer, &published),
            false,
        )),
    );
    assert_eq!(verdict(&store, &run.id).0, EdgeOutcome::Yes);
    let ready = drive_turn(&store, &run.id).unwrap();
    let graph = &ready.graph_executions[0];
    assert!(
        super::super::super::runtime::policy::drive_policy(
            &store,
            &ready,
            graph,
            graph.activations.last().unwrap()
        )
        .unwrap()
    );
    let approved = store.snapshot(&run.id).unwrap();
    assert_eq!(
        StoryStore::open()
            .unwrap()
            .get_story(&story)
            .unwrap()
            .status,
        crate::stories::StoryStatus::Done
    );
    assert!(
        super::super::super::runtime::policy::approved(&approved, &approved.graph_executions[0])
            .unwrap()
    );
    std::fs::write(
        std::path::Path::new(&worktree).join("README.md"),
        "changed\n",
    )
    .unwrap();
    for args in [
        ["add", "README.md"].as_slice(),
        ["commit", "-qm", "changed"].as_slice(),
    ] {
        crate::git_cli::git_cmd(std::path::Path::new(&worktree))
            .args(args)
            .run()
            .unwrap();
    }
    let current = store.snapshot(&run.id).unwrap();
    assert!(
        super::super::super::runtime::policy::gate(&current, &current.graph_executions[0])
            .unwrap()
            .is_none()
    );
    assert_eq!(verdict(&store, &run.id).0, EdgeOutcome::Uncertain);
    let reopened = RunStore::open().unwrap().snapshot(&run.id).unwrap();
    assert_eq!(
        reopened.attempts.last().unwrap().report,
        current.attempts.last().unwrap().report
    );
}

#[test]
fn implementer_cannot_supply_independent_review() {
    // catches: self-approval even when deterministic checks pass.
    let (config, project, _, story, _, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let worktree = init_worktree(project.path(), config.path());
    let published = published_policy(project_path);
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&request(project_path, &story, &published, "wf06-self"))
        .unwrap();
    let reviewer = setup_review(&store, &run, &worktree, "wf06-implementer");
    check(&store, &run.id, &story);
    let reserved = store
        .command(
            &run.id,
            "wf06:self-spawn",
            RunCommand::ReserveEffect {
                key: format!("spawn:{}", reviewer.id),
                kind: EffectKind::SpawnAgent,
            },
        )
        .unwrap();
    let RunEventKind::EffectReserved { effect } = reserved.event.kind else {
        panic!("spawn intent");
    };
    let error = store
        .bind_agent(
            &run.id,
            &reviewer.id,
            AgentBinding {
                session_id: "wf06-implementer".into(),
                task_id: None,
                effect_id: effect.id,
                prompt_contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
                prompt_sha256: "a".repeat(64),
                audit_preview: "self review attempt".into(),
            },
        )
        .unwrap_err();
    assert!(error.contains("already bound"), "{error}");
    assert_eq!(
        StoryStore::open()
            .unwrap()
            .get_story(&story)
            .unwrap()
            .status,
        crate::stories::StoryStatus::Review
    );
}

#[test]
fn second_review_exhaustion_does_not_get_third_review() {
    // catches: two repairs / three reviews, or replacement/reopen resetting the review budget.
    let (config, project, _, story, _, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let worktree = init_worktree(project.path(), config.path());
    let published = published_policy(project_path);
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&request(project_path, &story, &published, "wf06-budget"))
        .unwrap();
    let first = setup_review(&store, &run, &worktree, "wf06-reviewer-1");
    check(&store, &run.id, &story);
    let current = store.snapshot(&run.id).unwrap();
    report(
        &store,
        &run.id,
        &first,
        "wf06-reviewer-1",
        Some(assessment(
            subject(&store, &current, &first, &published),
            true,
        )),
    );
    let repair = drive_turn(&store, &run.id)
        .unwrap()
        .attempts
        .last()
        .unwrap()
        .clone();
    assert_eq!(repair.node_id, "implement");
    bind(&store, &run.id, &repair, "wf06-repair");
    report(&store, &run.id, &repair, "wf06-repair", None);
    let current = drive_turn(&store, &run.id).unwrap();
    let second = current.attempts.last().unwrap().clone();
    bind(&store, &run.id, &second, "wf06-reviewer-2");
    let current = store.snapshot(&run.id).unwrap();
    report(
        &store,
        &run.id,
        &second,
        "wf06-reviewer-2",
        Some(assessment(
            subject(&store, &current, &second, &published),
            true,
        )),
    );
    let reopened = RunStore::open().unwrap();
    let error = drive_turn(&reopened, &run.id).unwrap_err();
    assert!(error.contains("budget exhausted"), "{error}");
    assert_eq!(
        reopened
            .snapshot(&run.id)
            .unwrap()
            .attempts
            .iter()
            .filter(|a| a.node_id == "review")
            .count(),
        2
    );
}

#[test]
fn failed_gate_cannot_be_overridden_by_judge_yes() {
    // catches: typed/prose approval or accepted risk manufacturing deterministic PASS.
    let (config, project, _, story, _, _guard) = fixture();
    let project_path = project.path().to_str().unwrap();
    let worktree = init_worktree(project.path(), config.path());
    let mut published = published_policy(project_path);
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(&published.id).unwrap();
    let draft = definitions
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![CheckDefinition {
                id: "fails".into(),
                argv: vec!["git".into(), "rev-parse".into(), "missing-ref".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    published = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    let store = RunStore::open().unwrap();
    let run = store
        .start_graph_run(&request(project_path, &story, &published, "wf06-fail"))
        .unwrap();
    let reviewer = setup_review(&store, &run, &worktree, "wf06-reviewer");
    check(&store, &run.id, &story);
    let current = store.snapshot(&run.id).unwrap();
    report(
        &store,
        &run.id,
        &reviewer,
        "wf06-reviewer",
        Some(assessment(
            subject(&store, &current, &reviewer, &published),
            false,
        )),
    );
    assert_eq!(verdict(&store, &run.id).0, EdgeOutcome::Uncertain);
    assert_eq!(
        StoryStore::open()
            .unwrap()
            .get_story(&story)
            .unwrap()
            .status,
        crate::stories::StoryStatus::Review
    );
}
