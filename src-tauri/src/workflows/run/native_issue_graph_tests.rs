//! Real graph frames, native preparation/creation/cohort services and pinned return proof.
use super::native_issue_prepare_tests::{dispatch, next};
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralExecution, NeutralResult};
use super::neutral_runtime::{NeutralHost, NeutralInvocation};
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::{
    WorkflowStore,
    catalog::*,
    data_binding::DataSource,
    neutral_admission::NeutralAdmissionService,
    neutral_control::{ControlGraph, ControlNode},
    neutral_publication::{NeutralDefinition, PublicationPin, PublicationRef},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn schema(name: &str, value_type: ValueType) -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            name.into(),
            FieldSchema {
                value_type,
                required: true,
                default: None,
            },
        )]),
    }
}
fn native(name: &str) -> ActivityInstance {
    ActivityInstance {
        descriptor: DescriptorId {
            namespace: "native".into(),
            name: name.into(),
            version: 1,
        },
        config: BTreeMap::new(),
        execution_policy: "native".into(),
    }
}
fn constant(value: Value) -> DataSource {
    DataSource::Constant { value }
}
fn object(fields: Vec<(&str, DataSource)>) -> DataSource {
    DataSource::Object {
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}
fn output(activity: usize) -> DataSource {
    DataSource::ActivityOutput {
        activity,
        field: "receipt".into(),
    }
}
fn at(activity: usize, pointer: &str, value_type: ValueType) -> DataSource {
    DataSource::Projection {
        from: Box::new(output(activity)),
        pointer: pointer.into(),
        value_type,
    }
}
fn bindings(value: DataSource) -> BTreeMap<String, DataSource> {
    BTreeMap::from([("request".into(), value)])
}
fn observed(
    f: &Fixture,
    effect: &NeutralEffect,
    outcome: &str,
    receipt: Value,
) -> NeutralExecution {
    f.owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Observe {
                key: effect.key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Completed {
                            outcome: outcome.into(),
                            outputs: BTreeMap::from([("receipt".into(), receipt)]),
                        },
                    },
                },
            },
        )
        .unwrap()
}

// Catches: a graph receipt borrowing caller JSON, another child/cohort, or an
// observation that exists before its pinned child has actually returned.
// Read/model replies are controlled; all mutation and frame owners are real.
#[tokio::test]
async fn native_issue_graph_joins_exact_preparation_created_story_and_returned_pinned_child() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let backend = crate::stories::StoryStore::open().unwrap();
    let (source, intent, bound) = super::native_issue_tests::admission(&f);
    let project = f.context.workspace_path().to_string_lossy().into_owned();
    let workflows =
        WorkflowStore::open_at(&f.dir.path().join("graph-publications.sqlite3")).unwrap();
    let installed = f.host.catalog(&project).unwrap();
    let registry = f.host.registry();
    let mut catalog = workflows
        .build_project_catalog(&project, &installed, &registry)
        .unwrap();
    let context = f.host.publication_context(&project).unwrap();
    let mut child: NeutralDefinition = serde_json::from_value(json!({
        "inputs":schema("cohort",ValueType::Object),"outputs":schema("receipt",ValueType::Object),
        "subject":{"required":true,"fields":schema("workspace",ValueType::WorkspaceReference)},
        "outcomes":["completed","blocked"],"instances":[native("story.cohort.observe")],
        "input_bindings":{"0":bindings(DataSource::WorkflowInput{field:"cohort".into()})},
        "output_bindings":{"receipt":output(0)},"conditions":{},"collections":{},"loops":{},"forks":{},"subflows":[],
        "control":{"nodes":{"start":{"type":"start"},"observe":{"type":"activity","activity":0},"completed":{"type":"end","outcome":"completed"},"blocked":{"type":"end","outcome":"blocked"}},"edges":{"start":{"next":"observe"},"observe":{"completed":"completed","blocked":"blocked"}}}
    })).unwrap();
    let child_publication = NeutralAdmissionService {
        store: &workflows,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        &project,
        "issue-proof-child",
        0,
        child.clone(),
    )
    .unwrap();
    let pin = PublicationPin {
        reference: PublicationRef {
            project: project.clone(),
            id: child_publication.id.clone(),
            revision: child_publication.revision,
        },
        sha256: content_digest(&child_publication).unwrap(),
    };
    let descriptor = crate::workflows::native_child::descriptor(
        DescriptorId {
            namespace: "user".into(),
            name: "issue-proof-child".into(),
            version: 1,
        },
        pin.clone(),
        &child_publication,
        &catalog,
        None,
    )
    .unwrap();
    workflows
        .save_reusable_operation(&project, 0, descriptor.clone(), &catalog, &registry)
        .unwrap();
    catalog.register(descriptor.clone()).unwrap();
    let profile = super::native_issue_service::IssueProfile {
        profile: "controlled".into(),
        model: "controlled".into(),
        effort: "high".into(),
        instructions: "Evaluate requirements".into(),
    };
    let policy = super::native_issue_service::IssuePolicy {
        accepted_label: "accepted".into(),
        rejection_label: "rejected".into(),
        needs_input_comment: true,
    };
    let triage_request = super::native_issue_triage::IssueTriageRequest {
        source: source.clone(),
        profile: profile.clone(),
        policy: policy.clone(),
    };
    let instances = vec![
        native("issue.read"),
        native("issue.triage"),
        native("issue.prepare"),
        native("story.create"),
        native("story.cohort.created"),
        ActivityInstance {
            descriptor: descriptor.id,
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        },
        native("issue.graph.receipt"),
        native("issue.effect"),
        native("issue.effect"),
    ];
    child.inputs = ObjectSchema::default();
    child.outputs = ObjectSchema::default();
    child.output_bindings.clear();
    child.outcomes = BTreeSet::from(["done".into()]);
    child.instances = instances;
    child.input_bindings = BTreeMap::from([
        (0, bindings(constant(json!({})))),
        (1, bindings(constant(json!(triage_request)))),
        (
            2,
            bindings(object(vec![
                ("source", constant(json!(source))),
                ("read", output(0)),
                ("triage", at(1, "/effect", ValueType::Object)),
            ])),
        ),
        (3, bindings(at(2, "/create", ValueType::Object))),
        (
            4,
            bindings(object(vec![
                ("plan", at(2, "/plan/id", ValueType::String)),
                ("review", at(2, "/create/review", ValueType::String)),
            ])),
        ),
        (
            5,
            BTreeMap::from([("cohort".into(), at(4, "/cohort", ValueType::Object))]),
        ),
        (
            6,
            bindings(object(vec![
                ("source", constant(json!(source))),
                ("preparation", at(2, "/effect", ValueType::Object)),
                ("create", at(3, "/effect", ValueType::Object)),
                ("publication", constant(json!(pin.reference))),
                ("cohort", at(4, "/cohort", ValueType::Object)),
                ("observation", at(5, "/owner", ValueType::Object)),
            ])),
        ),
    ]);
    for activity in [7, 8] {
        child.input_bindings.insert(
            activity,
            bindings(object(vec![
                ("source", constant(json!(source))),
                (
                    "action",
                    object(vec![
                        ("type", constant(json!("status_comment"))),
                        ("child", at(6, "/effect", ValueType::Object)),
                    ]),
                ),
            ])),
        );
    }
    let names = [
        "read",
        "triage",
        "prepare",
        "create",
        "cohort",
        "child",
        "receipt",
        "status",
        "second_status",
    ];
    let mut nodes = BTreeMap::from([
        ("start".into(), ControlNode::Start),
        (
            "done".into(),
            ControlNode::End {
                outcome: "done".into(),
            },
        ),
    ]);
    let mut edges = BTreeMap::from([(
        "start".into(),
        BTreeMap::from([("next".into(), "read".into())]),
    )]);
    for (activity, name) in names.iter().enumerate() {
        nodes.insert((*name).into(), ControlNode::Activity { activity });
        let target = names.get(activity + 1).copied().unwrap_or("done");
        edges.insert(
            (*name).into(),
            if activity == 5 {
                BTreeMap::from([
                    ("completed".into(), target.into()),
                    ("blocked".into(), target.into()),
                ])
            } else {
                BTreeMap::from([("ok".into(), target.into())])
            },
        );
    }
    child.control = ControlGraph { nodes, edges };
    let publication = NeutralAdmissionService {
        store: &workflows,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        &project,
        "issue-proof-parent",
        0,
        child,
    )
    .unwrap();
    let request = super::neutral::NeutralStartRequest {
        publication: PublicationRef {
            project: project.clone(),
            id: publication.id,
            revision: publication.revision,
        },
        request_id: format!("trigger:{}", source.receipt),
        inputs: BTreeMap::new(),
        subject: Some(BTreeMap::from([(
            "workspace".into(),
            json!({"reference":"workspace","id":project}),
        )])),
        limits: super::neutral::NeutralLimits {
            max_activations: 40,
            max_concurrency: 1,
            max_duration_ms: 600000,
        },
    };
    let run = f
        .owner
        .start_neutral(
            &workflows,
            &request,
            &catalog,
            &registry.available_handlers(),
        )
        .unwrap();
    f.host
        .bind_run(&f.owner.store, &run.id, f.context.clone())
        .unwrap();
    f.owner.store.trigger_started(&intent, &run.id).unwrap();
    let run = next(&f, f.owner.store.neutral_execution(&run.id).unwrap());
    let read = dispatch(&f, &run);
    let run = next(
        &f,
        observed(
            &f,
            &read,
            "ok",
            json!(super::native_issue_read::IssueReadReceipt::new(
                source.clone(),
                super::native_issue_tests::api().issue
            )),
        ),
    );
    let triage = dispatch(&f, &run);
    let triage_receipt = super::native_issue_triage::IssueTriageReceipt {
        effect: triage.key.clone(),
        policy,
        assessment: super::native_issue_service::ScopedTriage {
            source: source.clone(),
            profile,
            decision: crate::workflows::review_verdict::TriageDecision {
                outcome: crate::workflows::review_verdict::TriageOutcome::Accepted,
                reason: "Controlled acceptance".into(),
            },
            report: "controlled-report".into(),
        },
    };
    let run = next(&f, observed(&f, &triage, "ok", json!(triage_receipt)));
    let preparation = dispatch(&f, &run);
    let prepare_request =
        serde_json::from_value(preparation.invocation.inputs["request"].clone()).unwrap();
    let prepared =
        super::native_issue_prepare::prepare(&bound, &preparation.key, &prepare_request, &backend)
            .unwrap();
    let run = next(&f, observed(&f, &preparation, "ok", json!(prepared)));
    let create = dispatch(&f, &run);
    let create_request =
        serde_json::from_value(create.invocation.inputs["request"].clone()).unwrap();
    let created = super::native_story_create::create(
        &f.context,
        &f.owner.store,
        &create.key,
        &backend,
        &create_request,
    )
    .unwrap();
    let run = next(&f, observed(&f, &create, "ok", json!(created)));
    let cohort_effect = dispatch(&f, &run);
    let cohort_request =
        serde_json::from_value(cohort_effect.invocation.inputs["request"].clone()).unwrap();
    let cohort = super::native_cohort::created_cohort(
        &f.context,
        &f.owner.store,
        &cohort_effect.key,
        &backend,
        &cohort_request,
    )
    .unwrap();
    let run = next(
        &f,
        observed(
            &f,
            &cohort_effect,
            "ok",
            super::native_cohort::created_receipt(&cohort),
        ),
    );
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let mut invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    super::neutral_runtime::admitted(f.host.as_ref(), &run, &mut invocation, None).unwrap();
    let run = f
        .owner
        .neutral_command_at(
            &run.run.id,
            "enter-owned-child",
            run.sequence,
            super::neutral_execution::NeutralExecutionCommand::EnterSubflow {
                fence: attempt.fence.clone(),
            },
            super::store::now_ms(),
        )
        .unwrap();
    let run = next(&f, run);
    let observe = dispatch(&f, &run);
    let receipt =
        super::native_cohort::observe(&f.context, &observe.key, &backend, &cohort).unwrap();
    let run = observed(&f, &observe, "blocked", receipt);
    assert!(
        f.owner
            .store
            .neutral_returned_subflow(&run.run.id, &observe.key.fence.execution)
            .unwrap_err()
            .contains("no retained normal return")
    );
    let run = next(&f, run);
    let graph = dispatch(&f, &run);
    let request: super::native_issue_graph::Request =
        serde_json::from_value(graph.invocation.inputs["request"].clone()).unwrap();
    let observation = f.host.invoke_effect(graph.clone()).await.unwrap();
    let EffectOutcome::Completed {
        result: NeutralResult::Completed { outputs, .. },
    } = &observation.outcome
    else {
        panic!("{observation:?}");
    };
    let receipt: super::native_issue_graph::Receipt =
        serde_json::from_value(outputs["receipt"].clone()).unwrap();
    assert_eq!(
        receipt.child.status,
        super::native_issue_service::IssueChildStatus::Failed
    );
    assert_eq!(receipt.child.child, observe.key.fence.execution);
    assert!(receipt.child.evidence.is_none());
    let mut foreign = request.clone();
    foreign.observation.fence.run = "foreign".into();
    assert!(super::native_issue_graph::compose(&bound, &graph.key, &foreign, &backend).is_err());
    let mut wrong = request.clone();
    wrong.publication.revision += 1;
    assert!(super::native_issue_graph::compose(&bound, &graph.key, &wrong, &backend).is_err());
    let mut wrong = request.clone();
    wrong.cohort.stories[0].revision += 1;
    assert!(super::native_issue_graph::compose(&bound, &graph.key, &wrong, &backend).is_err());
    let run = next(&f, observed(&f, &graph, "ok", json!(receipt)));
    let status = super::native_issue_effect::IssueEffectRequest {
        source,
        action: super::native_issue_effect::IssueAction::StatusComment {
            child: graph.key.clone(),
        },
    };
    assert!(
        matches!(super::native_issue_policy::resolve(&bound,&graph.key,&status).unwrap(),super::native_issue_policy::IssueWrite::Comment(body) if body.contains(&format!("child {} is Failed", observe.key.fence.execution)))
    );
    // Catches: a blocked child admitting an arbitrary write or a second status.
    let status_effect = dispatch(&f, &run);
    let snapshot = f.owner.store.neutral_execution(&run.run.id).unwrap();
    let mut arbitrary = status_effect.clone();
    arbitrary.invocation.inputs.insert(
        "request".into(),
        json!({"source":status.source,"action":{"type":"comment","body":"not permitted"}}),
    );
    assert!(
        super::native_issue_publication::admit_mutation(&snapshot, &arbitrary)
            .unwrap_err()
            .contains("only its exact")
    );
    let run = next(&f, observed(&f, &status_effect, "ok", json!({})));
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    let run = f
        .owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(effect.clone()),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Answer {
                key: effect.key.clone(),
                revision: effect.question.as_ref().unwrap().revision,
                approved: true,
            },
        )
        .unwrap();
    assert!(
        f.owner
            .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: effect.key })
            .unwrap_err()
            .contains("already attempted")
    );
}
